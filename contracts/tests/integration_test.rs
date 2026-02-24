// Integration tests for kor-AssetForge smart contracts
// Tests cross-contract interactions between EmergencyControl, Marketplace, AssetToken, and Governance

#[cfg(test)]
mod tests {
    extern crate kor_assetforge_contracts;

    use kor_assetforge_contracts::emergency_control::{
        EmergencyControl, EmergencyControlClient, PauseScope,
    };
    use kor_assetforge_contracts::governance::{Governance, GovernanceClient, ProposalStatus};
    use kor_assetforge_contracts::marketplace::{Marketplace, MarketplaceClient};
    use kor_assetforge_contracts::asset_token::{AssetToken, AssetTokenClient};
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{Address, Env, String, IntoVal};

    /// Helper: set up the environment with all contracts deployed.
    fn setup() -> (
        Env,
        Address, // ec_id
        Address, // mp_id
        Address, // at_id
        Address, // gov_id
        Address, // admin
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let mp_id = env.register_contract(None, Marketplace);
        let at_id = env.register_contract(None, AssetToken);
        let gov_id = env.register_contract(None, Governance);

        let admin = Address::generate(&env);

        // Initialize emergency control
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        ec_client.initialize(&admin);

        // Initialize asset token
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(&admin, &String::from_str(&env, "Token"), &String::from_str(&env, "TKN"), &7);

        // Initialize governance: quorum=100, deposit=50
        let gov_client = GovernanceClient::new(&env, &gov_id);
        gov_client.initialize(&admin, &at_id, &100, &50);

        (env, ec_id, mp_id, at_id, gov_id, admin)
    }

    #[test]
    fn test_full_lifecycle_pause_unpause_resume() {
        let (env, ec_id, mp_id, _at_id, _gov_id, admin) = setup();

        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let seller = Address::generate(&env);
        let asset_id: u64 = 1;

        // Step 1: Create listing should succeed (no pause, no governance gate)
        let listing_id = mp_client.create_listing(&seller, &asset_id, &100, &1000, &ec_id, &None);
        assert_eq!(listing_id, 1);

        // Step 2: Pause trading
        let reason = String::from_str(&env, "emergency maintenance");
        ec_client.pause_asset(&admin, &asset_id, &PauseScope::Trading, &reason, &0);
        assert!(ec_client.is_paused(&asset_id, &PauseScope::Trading));

        // Step 3: Verify listing creation is now blocked
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.create_listing(&seller, &asset_id, &50, &500, &ec_id, &None);
        }));
        assert!(result.is_err());

        // Step 4: Unpause trading
        ec_client.unpause_asset(&admin, &asset_id, &PauseScope::Trading);
        assert!(!ec_client.is_paused(&asset_id, &PauseScope::Trading));

        // Step 5: Listing creation should work again
        let listing_id2 = mp_client.create_listing(&seller, &asset_id, &50, &500, &ec_id, &None);
        assert_eq!(listing_id2, 1);
    }

    #[test]
    fn test_cross_contract_scope_isolation() {
        let (env, ec_id, mp_id, at_id, _gov_id, admin) = setup();

        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let user = Address::generate(&env);
        let asset_id: u64 = 1;

        // Mint some tokens for transfer test BEFORE pausing
        at_client.mint(&user, &100, &asset_id, &ec_id);

        // Pause only minting
        let reason = String::from_str(&env, "minting freeze");
        ec_client.pause_asset(&admin, &asset_id, &PauseScope::Minting, &reason, &0);

        // Trading should still work
        let listing_id = mp_client.create_listing(&user, &asset_id, &100, &1000, &ec_id, &None);
        assert_eq!(listing_id, 1);

        // Transfers should still work
        let to = Address::generate(&env);
        at_client.transfer(&user, &to, &50, &asset_id, &ec_id);

        // Minting should be blocked
        let mint_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            at_client.mint(&user, &1000, &asset_id, &ec_id);
        }));
        assert!(mint_result.is_err());
    }

    #[test]
    fn test_global_pause_blocks_all_operations() {
        let (env, ec_id, mp_id, at_id, _gov_id, admin) = setup();

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
            mp_client.create_listing(&user, &asset_id, &100, &1000, &ec_id, &None);
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
        let (env, ec_id, _mp_id, _at_id, _gov_id, admin) = setup();

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
        let (env, ec_id, mp_id, _at_id, _gov_id, admin) = setup();

        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let seller = Address::generate(&env);

        // Pause asset 1 trading
        let reason = String::from_str(&env, "asset 1 issue");
        ec_client.pause_asset(&admin, &1, &PauseScope::Trading, &reason, &0);

        // Asset 2 trading should work fine
        let listing = mp_client.create_listing(&seller, &2, &100, &500, &ec_id, &None);
        assert_eq!(listing, 1);

        // Asset 1 trading should be blocked
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.create_listing(&seller, &1, &100, &500, &ec_id, &None);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_valuation_management() {
        let (env, _ec_id, _mp_id, at_id, _gov_id, admin) = setup();
        let at_client = AssetTokenClient::new(&env, &at_id);
        let oracle = Address::generate(&env);

        // Step 1: Set oracle
        at_client.set_oracle(&oracle);

        // Step 2: Update valuation via oracle
        at_client.update_valuation(&oracle, &1500);
        let val = at_client.get_valuation().unwrap();
        assert_eq!(val.value, 1500);

        // Step 3: Verify history
        let history = at_client.get_valuation_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().value, 1500);

        // Step 4: Admin update should also work
        at_client.update_valuation(&admin, &1600);
        assert_eq!(at_client.get_valuation().unwrap().value, 1600);
    }

    #[test]
    fn test_asset_dividend_lifecycle() {
        let (env, ec_id, _mp_id, at_id, _gov_id, _admin) = setup();
        let at_client = AssetTokenClient::new(&env, &at_id);
        let payout_asset = Address::generate(&env);

        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        // 1. Mint tokens: user1 (600), user2 (400)
        at_client.mint(&user1, &600, &1, &ec_id);
        at_client.mint(&user2, &400, &1, &ec_id);

        // 2. Schedule dividend: 100M units total
        at_client.schedule_dividend(&1, &100_000_000, &payout_asset, &3600);

        // 3. Verify schedule info
        let info = at_client.get_dividend_info(&1).expect("dividend not scheduled");
        assert_eq!(info.total_dividend, 100_000_000);

        // 4. Advance time
        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        // 5. Claim dividends
        at_client.claim_dividend(&1, &user1);
        at_client.claim_dividend(&1, &user2);

        // 6. Verify double claim fails
        let res = env.try_invoke_contract::<soroban_sdk::Val, soroban_sdk::Error>(
            &at_id,
            &soroban_sdk::Symbol::new(&env, "claim_dividend"),
            (1u64, user1).into_val(&env),
        );
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // Governance integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_governance_full_proposal_lifecycle() {
        let (env, ec_id, mp_id, at_id, gov_id, _admin) = setup();
        let at_client = AssetTokenClient::new(&env, &at_id);
        let gov_client = GovernanceClient::new(&env, &gov_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let proposer = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        let seller = Address::generate(&env);
        let asset_id: u64 = 10;

        // Mint governance tokens
        at_client.mint(&proposer, &200, &1, &ec_id);
        at_client.mint(&voter1, &80, &1, &ec_id);
        at_client.mint(&voter2, &60, &1, &ec_id);

        // 1. Create proposal
        let pid = gov_client.create_proposal(
            &proposer,
            &asset_id,
            &String::from_str(&env, "List premium real estate asset"),
            &7200,
        );
        assert_eq!(pid, 1);

        // 2. Voters cast votes
        gov_client.vote(&voter1, &pid, &true);
        gov_client.vote(&voter2, &pid, &true);

        // 3. Advance time past voting period
        env.ledger().with_mut(|li| {
            li.timestamp += 7201;
        });

        // 4. Tally — should pass (140 votes >= 100 quorum, all for)
        gov_client.tally_execute(&pid);

        let p = gov_client.get_proposal(&pid).unwrap();
        assert_eq!(p.status, ProposalStatus::Passed);
        assert!(gov_client.is_approved(&asset_id));

        // 5. Marketplace listing now works with governance gate
        let lid = mp_client.create_listing(
            &seller,
            &asset_id,
            &50,
            &5000,
            &ec_id,
            &Some(gov_id.clone()),
        );
        assert_eq!(lid, 1);
    }

    #[test]
    fn test_governance_rejected_proposal_blocks_listing() {
        let (env, ec_id, mp_id, at_id, gov_id, _admin) = setup();
        let at_client = AssetTokenClient::new(&env, &at_id);
        let gov_client = GovernanceClient::new(&env, &gov_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);
        let asset_id: u64 = 20;

        at_client.mint(&proposer, &200, &1, &ec_id);
        at_client.mint(&voter, &120, &1, &ec_id);

        // Create proposal and vote against
        let pid = gov_client.create_proposal(
            &proposer,
            &asset_id,
            &String::from_str(&env, "Bad asset"),
            &3600,
        );
        gov_client.vote(&voter, &pid, &false); // 120 against

        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        gov_client.tally_execute(&pid);

        let p = gov_client.get_proposal(&pid).unwrap();
        assert_eq!(p.status, ProposalStatus::Rejected);
        assert!(!gov_client.is_approved(&asset_id));

        // Listing with governance gate should fail
        let seller = Address::generate(&env);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.create_listing(
                &seller,
                &asset_id,
                &50,
                &1000,
                &ec_id,
                &Some(gov_id.clone()),
            );
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_governance_and_pause_combined() {
        let (env, ec_id, mp_id, at_id, gov_id, admin) = setup();
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let at_client = AssetTokenClient::new(&env, &at_id);
        let gov_client = GovernanceClient::new(&env, &gov_id);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);
        let asset_id: u64 = 30;

        at_client.mint(&proposer, &200, &1, &ec_id);
        at_client.mint(&voter, &150, &1, &ec_id);

        // Pass proposal
        let pid = gov_client.create_proposal(
            &proposer,
            &asset_id,
            &String::from_str(&env, "Approved asset"),
            &3600,
        );
        gov_client.vote(&voter, &pid, &true);

        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });
        gov_client.tally_execute(&pid);
        assert!(gov_client.is_approved(&asset_id));

        // Pause trading on the asset
        let reason = String::from_str(&env, "maintenance");
        ec_client.pause_asset(&admin, &asset_id, &PauseScope::Trading, &reason, &0);

        // Even though governance approved, pause should block listing
        let seller = Address::generate(&env);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.create_listing(
                &seller,
                &asset_id,
                &50,
                &1000,
                &ec_id,
                &Some(gov_id.clone()),
            );
        }));
        assert!(result.is_err());

        // Unpause — listing should work
        ec_client.unpause_asset(&admin, &asset_id, &PauseScope::Trading);
        let lid = mp_client.create_listing(
            &seller,
            &asset_id,
            &50,
            &1000,
            &ec_id,
            &Some(gov_id),
        );
        assert_eq!(lid, 1);
    }
}
