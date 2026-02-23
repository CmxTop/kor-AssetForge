// Integration tests for kor-AssetForge smart contracts
// Tests cross-contract interactions between EmergencyControl, Marketplace, and AssetToken

#[cfg(test)]
mod tests {
    use kor_assetforge_contracts::emergency_control::{
        EmergencyControl, EmergencyControlClient, PauseScope,
    };
    use kor_assetforge_contracts::marketplace::{Marketplace, MarketplaceClient};
    use kor_assetforge_contracts::asset_token::{AssetToken, AssetTokenClient};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, String};

    /// Helper: set up the environment with all three contracts deployed.
    fn setup() -> (
        Env,
        Address, // ec_id
        Address, // mp_id
        Address, // at_id
        Address, // admin
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let mp_id = env.register_contract(None, Marketplace);
        let at_id = env.register_contract(None, AssetToken);

        let admin = Address::generate(&env);

        // Initialize emergency control
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        ec_client.initialize(&admin);

        (env, ec_id, mp_id, at_id, admin)
    }

    #[test]
    fn test_full_lifecycle_pause_unpause_resume() {
        let (env, ec_id, mp_id, _at_id, admin) = setup();

        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let seller = Address::generate(&env);
        let asset_id: u64 = 1;

        // Step 1: Create listing should succeed (no pause)
        let listing_id = mp_client.create_listing(&seller, &asset_id, &100, &1000, &ec_id);
        assert_eq!(listing_id, 1);

        // Step 2: Pause trading
        let reason = String::from_str(&env, "emergency maintenance");
        ec_client.pause_asset(&admin, &asset_id, &PauseScope::Trading, &reason, &0);
        assert!(ec_client.is_paused(&asset_id, &PauseScope::Trading));

        // Step 3: Verify listing creation is now blocked
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.create_listing(&seller, &asset_id, &50, &500, &ec_id);
        }));
        assert!(result.is_err());

        // Step 4: Unpause trading
        ec_client.unpause_asset(&admin, &asset_id, &PauseScope::Trading);
        assert!(!ec_client.is_paused(&asset_id, &PauseScope::Trading));

        // Step 5: Listing creation should work again
        let listing_id2 = mp_client.create_listing(&seller, &asset_id, &50, &500, &ec_id);
        assert_eq!(listing_id2, 1);
    }

    #[test]
    fn test_cross_contract_scope_isolation() {
        let (env, ec_id, mp_id, at_id, admin) = setup();

        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let user = Address::generate(&env);
        let asset_id: u64 = 1;

        // Pause only minting
        let reason = String::from_str(&env, "minting freeze");
        ec_client.pause_asset(&admin, &asset_id, &PauseScope::Minting, &reason, &0);

        // Trading should still work
        let listing_id = mp_client.create_listing(&user, &asset_id, &100, &1000, &ec_id);
        assert_eq!(listing_id, 1);

        // Transfers should still work
        let to = Address::generate(&env);
        let transfer_result = at_client.transfer(&user, &to, &50, &asset_id, &ec_id);
        assert!(transfer_result);

        // Minting should be blocked
        let mint_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            at_client.mint(&user, &1000, &asset_id, &ec_id);
        }));
        assert!(mint_result.is_err());
    }

    #[test]
    fn test_global_pause_blocks_all_operations() {
        let (env, ec_id, mp_id, at_id, admin) = setup();

        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let user = Address::generate(&env);
        let to = Address::generate(&env);
        let asset_id: u64 = 1;

        // Pause ALL
        let reason = String::from_str(&env, "full system halt");
        ec_client.pause_asset(&admin, &asset_id, &PauseScope::All, &reason, &0);

        // Trading blocked
        let trading_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.create_listing(&user, &asset_id, &100, &1000, &ec_id);
        }));
        assert!(trading_result.is_err());

        // Transfer blocked
        let transfer_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            at_client.transfer(&user, &to, &50, &asset_id, &ec_id);
        }));
        assert!(transfer_result.is_err());

        // Minting blocked
        let mint_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            at_client.mint(&user, &1000, &asset_id, &ec_id);
        }));
        assert!(mint_result.is_err());
    }

    #[test]
    fn test_audit_trail_across_operations() {
        let (env, ec_id, _mp_id, _at_id, admin) = setup();

        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let asset_id: u64 = 42;

        // Perform several pause/unpause operations
        let reason1 = String::from_str(&env, "security vulnerability");
        ec_client.pause_asset(&admin, &asset_id, &PauseScope::Trading, &reason1, &0);

        let reason2 = String::from_str(&env, "legal requirement");
        ec_client.pause_asset(&admin, &asset_id, &PauseScope::Transfers, &reason2, &0);

        ec_client.unpause_asset(&admin, &asset_id, &PauseScope::Trading);

        // Check history
        let history = ec_client.get_pause_history(&asset_id);
        assert_eq!(history.len(), 3);

        // First: pause trading
        let h0 = history.get(0).unwrap();
        assert!(h0.is_pause);
        assert_eq!(h0.scope, PauseScope::Trading);

        // Second: pause transfers
        let h1 = history.get(1).unwrap();
        assert!(h1.is_pause);
        assert_eq!(h1.scope, PauseScope::Transfers);

        // Third: unpause trading
        let h2 = history.get(2).unwrap();
        assert!(!h2.is_pause);
        assert_eq!(h2.scope, PauseScope::Trading);
    }

    #[test]
    fn test_multiple_assets_paused_independently() {
        let (env, ec_id, mp_id, _at_id, admin) = setup();

        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let seller = Address::generate(&env);

        // Pause asset 1 trading
        let reason = String::from_str(&env, "asset 1 issue");
        ec_client.pause_asset(&admin, &1, &PauseScope::Trading, &reason, &0);

        // Asset 2 trading should work fine
        let listing = mp_client.create_listing(&seller, &2, &100, &500, &ec_id);
        assert_eq!(listing, 1);

        // Asset 1 trading should be blocked
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.create_listing(&seller, &1, &100, &500, &ec_id);
        }));
        assert!(result.is_err());
    }
}
