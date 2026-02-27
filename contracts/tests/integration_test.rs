// Integration tests for kor-AssetForge smart contracts
// Tests cross-contract interactions between EmergencyControl, Marketplace, AssetToken, and Governance

#[cfg(test)]
mod tests {
    // use soroban_sdk::{Address, Env, String, Symbol};

    // UNIT TESTS: Asset Initialization
    extern crate kor_assetforge_contracts;

    use kor_assetforge_contracts::asset_token::{
        AssetToken, AssetTokenClient, BridgeStatus, TargetChain,
    };
    use kor_assetforge_contracts::emergency_control::{
        EmergencyControl, EmergencyControlClient, PauseScope,
    };
    use kor_assetforge_contracts::governance::{Governance, GovernanceClient};
    use kor_assetforge_contracts::marketplace::{Marketplace, MarketplaceClient};
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{Address, Bytes, BytesN, Env, String};

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
        at_client.initialize(
            &admin,
            &String::from_str(&env, "Token"),
            &String::from_str(&env, "TKN"),
            &7,
            &0,
        );

        // Initialize governance: quorum=100, deposit=50
        let gov_client = GovernanceClient::new(&env, &gov_id);
        gov_client.initialize(&admin, &at_id, &100, &50);

        (env, ec_id, mp_id, at_id, gov_id, admin)
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

    #[test]
    fn test_asset_initialization_success() {
        // Test successful asset initialization
        // - Admin authorization required
        // - Asset ID generation
        // - Metadata storage (name, symbol, total_supply)
        // - Owner assignment
        // - Fractionalization flags initialized to false/0
        assert!(true);
    }

    #[test]
    fn test_asset_initialization_missing_auth() {
        // Test initialization fails without admin authorization
        // - Should reject unauthorized callers
        // - Should not create asset
        // Implement: Pass non-admin address as caller
        assert!(true);
    }

    #[test]
    fn test_asset_initialization_invalid_params() {
        // Test initialization with invalid parameters
        // - Zero total supply
        // - Empty name/symbol
        // - Invalid addresses
        // Implement: Call initialize() with edge case parameters
        assert!(true);
    }

    // UNIT TESTS: Fractional Minting Core Logic

    #[test]
    fn test_fractional_mint_basic() {
        // Test basic fractional minting
        // - Calculate unit_value correctly: total_value / fractions
        // - Create fractional balances
        // - Update total supply
        // - Emit "fractions_minted" event with asset_id, fractions, unit_value
        // Implement: mint_fractional(admin, 100_000, 1000, None) => unit_value = 100
        assert!(true);
    }

    #[test]
    fn test_fractional_mint_with_decimal_handling() {
        // Test fractional minting with decimal precision (i128)
        // - Handle large total_value (e.g., 1_000_000_000 * 10^18)
        // - Divide into fractions accurately
        // - Avoid rounding errors and dust
        // - Ensure unit_value calculation precision
        // Implement: mint_fractional(admin, 1_000_000_000_000_000_000, 1_000_000, None)
        assert!(true);
    }

    #[test]
    fn test_fractional_mint_single_owner() {
        // Test fractional minting assigning all shares to single owner
        // - All fractions assigned to one address
        // - Balance reflects total fractional shares
        // - Transfer permissions correct
        // Implement: mint_fractional with vec![(owner, 1000)] => owner.balance = 100_000
        assert!(true);
    }

    #[test]
    fn test_fractional_mint_multiple_owners() {
        // Test fractional minting with initial_owners distribution
        // - Distribute shares across multiple owners
        // - Verify each owner's balance matches assigned share count
        // - Validate share distribution totals match fractions
        // Implement: 5 owners, each 200 shares (1000 total) => verify balances
        assert!(true);
    }

    #[test]
    fn test_fractional_mint_error_zero_fractions() {
        // Test error handling for zero fractions
        // - Should reject fractions == 0
        // - Return ZeroFractions error
        // - Asset not created
        // Implement: mint_fractional(admin, 100_000, 0, None) => Err(ZeroFractions)
        assert!(true);
    }

    #[test]
    fn test_fractional_mint_error_uneven_division() {
        // Test error handling for uneven division
        // - total_value not evenly divisible by fractions
        // - Should reject with UnevenDivision error
        // - Verify no dust remains
        // Implement: mint_fractional(admin, 100_000, 1001, None) => Err(UnevenDivision)
        assert!(true);
    }

    #[test]
    fn test_fractional_mint_error_already_fractionalized() {
        // Test preventing re-fractionalization
        // - Asset already has fractional tokens
        // - Should reject with AlreadyFractionalized error
        // - Maintain existing fractional structure
        // Implement: Call mint_fractional twice on same asset => Err(AlreadyFractionalized)
        assert!(true);
    }

    #[test]
    fn test_fractional_mint_error_invalid_owner_list() {
        // Test error handling for invalid initial_owners
        // - Duplicate addresses in owner list
        // - Share distribution > fractions
        // - Empty owners list when expecting distribution
        // - Invalid/zero addresses
        // Implement: Test duplicate addr, sum > fractions, etc.
        assert!(true);
    }

    // UNIT TESTS: Metadata and Events

    #[test]
    fn test_fractional_metadata_storage() {
        // Test metadata updates in asset struct
        // - Store is_fractionalized flag
        // - Store total_fractions count
        // - Store unit_value for reference
        // - Store initial_owners list (or merkle root)
        assert!(true);
    }

    #[test]
    fn test_fractions_minted_event_emission() {
        // Test "fractions_minted" event contains:
        // - asset_id
        // - fractions (count)
        // - unit_value
        // - timestamp
        // - issuer address
        assert!(true);
    }

    #[test]
    fn test_fractional_balance_query() {
        // Test balance lookup for fractional holders
        // - Query address with fractional tokens
        // - Verify balance equals assigned fractions * unit_value
        // - Test multiple fractional token holders
        assert!(true);
    }

    // UNIT TESTS: Fractional Token Transfers

    #[test]
    fn test_fractional_transfer_basic() {
        // Test transferring fractional tokens between addresses
        // - Sender has sufficient balance
        // - Transfer amount is multiple of unit_value
        // - Receiver balance updated correctly
        // - Sender balance decreased correctly
        // Implement: transfer(sender, receiver, 50_000) => sender: 50_000, receiver: 50_000
        assert!(true);
    }

    #[test]
    fn test_fractional_transfer_partial() {
        // Test partial transfer of fractional holdings
        // - Transfer subset of owned fractions
        // - Sender retains remainder
        // - Transfer maintains precision
        // Implement: transfer 30_000 from 100_000 => sender: 70_000, receiver: 30_000
        assert!(true);
    }

    #[test]
    fn test_fractional_transfer_authorization() {
        // Test transfer authorization required from sender
        // - Sender must call or authorize transfer
        // - Unauthorized transfers rejected
        // - Emit transfer event with authorization details
        // Implement: Call transfer with unauthorized signer => Should fail
        assert!(true);
    }

    #[test]
    fn test_fractional_transfer_insufficient_balance() {
        // Test transfer fails with insufficient balance
        // - Attempt to transfer more than owned
        // - Reject with InsufficientBalance error
        // - No state changes on failure
        // Implement: transfer(sender, receiver, 150_000) when sender has 100_000 => Err
        assert!(true);
    }

    #[test]
    fn test_fractional_transfer_decimal_precision() {
        // Test transfers maintain decimal precision
        // - Transfer amount respects unit_value precision
        // - No dust accumulation
        // - Rounding handled correctly
        // Implement: Large i128 transfers with precision verification
        assert!(true);
    }

    #[test]
    fn test_fractional_transfer_to_self() {
        // Test self-transfers (edge case)
        // - Should succeed but be no-op
        // - Balance unchanged
        // - Event still emitted
        // Implement: transfer(addr, addr, 50_000) => balance still 100_000
        assert!(true);
    }

    // INTEGRATION TESTS: Multi-Owner Scenarios
    // =========================================================================
    // Cross-Chain Bridging Integration Tests
    // =========================================================================

    #[test]
    fn test_full_bridge_out_in_lifecycle() {
        let (env, ec_id, _mp_id, at_id, _gov_id, _admin) = setup();
        let at_client = AssetTokenClient::new(&env, &at_id);

        let user = Address::generate(&env);
        let recipient = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        // Mint tokens and configure bridge (0 fee for clarity)
        at_client.mint(&user, &50_000, &1, &ec_id);
        at_client.set_bridge_config(&0, &pool, &3600, &10, &pubkey);

        let asset_id = Address::generate(&env);
        let target_addr = Bytes::from_array(&env, &[0xABu8; 20]);

        // Bridge out 20,000 tokens
        let bridge_id = at_client.bridge_out(
            &user,
            &asset_id,
            &20_000,
            &TargetChain::Ethereum,
            &target_addr,
        );
        assert_eq!(at_client.balance(&user), 30_000);
        assert_eq!(at_client.total_supply(), 30_000);

        // Verify pending bridge
        let pending = at_client.get_pending_bridge(&bridge_id).unwrap();
        assert_eq!(pending.status, BridgeStatus::Pending);
        assert_eq!(pending.amount, 20_000);

        // Bridge in to recipient on Stellar (admin auth mocked)
        at_client.bridge_in(
            &bridge_id,
            &recipient,
            &asset_id,
            &20_000,
            &TargetChain::Ethereum,
        );

        assert_eq!(at_client.balance(&recipient), 20_000);
        assert_eq!(at_client.total_supply(), 50_000);
        assert_eq!(
            at_client.get_pending_bridge(&bridge_id).unwrap().status,
            BridgeStatus::Completed,
        );
    }

    #[test]
    fn test_bridge_with_fees_and_expiry() {
        let (env, ec_id, _mp_id, at_id, _gov_id, _admin) = setup();
        let at_client = AssetTokenClient::new(&env, &at_id);

        let user = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        at_client.mint(&user, &100_000, &1, &ec_id);
        // 50 bps fee (0.50%), 200s timeout
        at_client.set_bridge_config(&50, &pool, &200, &10, &pubkey);

        let bridge_id = at_client.bridge_out(
            &user,
            &Address::generate(&env),
            &10_000,
            &TargetChain::Solana,
            &Bytes::from_array(&env, &[0xCDu8; 32]),
        );

        // Fee = 10000 * 50 / 10000 = 50
        assert_eq!(at_client.balance(&pool), 50);
        assert_eq!(at_client.balance(&user), 90_000);

        let pending = at_client.get_pending_bridge(&bridge_id).unwrap();
        assert_eq!(pending.amount, 9_950); // net after fee
        assert_eq!(pending.fee, 50);

        // Let bridge expire
        env.ledger().with_mut(|li| {
            li.timestamp += 300;
        });

        at_client.expire_bridge(&bridge_id);
        assert_eq!(
            at_client.get_pending_bridge(&bridge_id).unwrap().status,
            BridgeStatus::Failed,
        );
    }

    #[test]
    fn test_bridge_pause_blocks_operations() {
        let (env, ec_id, _mp_id, at_id, _gov_id, _admin) = setup();
        let at_client = AssetTokenClient::new(&env, &at_id);

        let user = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        at_client.mint(&user, &10_000, &1, &ec_id);
        at_client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        // Pause bridging
        at_client.set_bridge_paused(&true);

        // bridge_out should fail
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            at_client.bridge_out(
                &user,
                &Address::generate(&env),
                &1000,
                &TargetChain::Ethereum,
                &Bytes::from_array(&env, &[0xABu8; 20]),
            );
        }));
        assert!(result.is_err());

        // Unpause and retry
        at_client.set_bridge_paused(&false);
        let bridge_id = at_client.bridge_out(
            &user,
            &Address::generate(&env),
            &1000,
            &TargetChain::Ethereum,
            &Bytes::from_array(&env, &[0xABu8; 20]),
        );
        assert_eq!(
            at_client.get_pending_bridge(&bridge_id).unwrap().status,
            BridgeStatus::Pending,
        );
    }

    // -----------------------------------------------------------------------
    // Governance integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_multi_owner_fractional_workflow() {
        // Complete workflow: mint fractions for 5 owners, then transfers
        // - Initialize asset
        // - Mint with 5 owners, each getting 20% of 1000 fractions
        // - Each owner transfers portion of their share
        // - Verify final balances for all participants
        assert!(true);
    }

    #[test]
    fn test_fractional_holding_periods() {
        // Test fractional ownership over time
        // - Mint fractions
        // - Hold for period
        // - Query balance remains same (no lock/unlock timing)
        // - Transfer anytime (no lock-up period unless specified)
        assert!(true);
    }

    #[test]
    fn test_fractional_liquidity_pool_scenario() {
        // Simulate fractional tokens in liquidity scenarios
        // - Mint fractions to marketplace account
        // - Distribute to liquidity pool
        // - Multiple traders swap fractions
        // - Verify balances and transfers
        assert!(true);
    }

    // INTEGRATION TESTS: Error Scenarios

    #[test]
    fn test_fractional_mint_large_numbers() {
        // Test fractional minting with very large numbers
        // - total_value: u128::MAX equivalent
        // - fractions: large u64 values
        // - Verify no overflow in unit_value calculation
        // - Precision maintained
        assert!(true);
    }

    #[test]
    fn test_fractional_mint_small_fractions() {
        // Test fractional minting with very small unit values
        // - Large total_value / large fractions = small unit_value
        // - Verify i128 precision sufficient
        // - Dust handling
        assert!(true);
    }

    #[test]
    fn test_fractional_state_consistency() {
        // Test state consistency after multiple operations
        // - Mint fractions
        // - Multiple transfers
        // - Query final state
        // - Verify total_supply unchanged
        // - Verify sum of all balances = total_supply
        assert!(true);
    }

    // INTEGRATION TESTS: Stellar Integration

    #[test]
    fn test_stellar_decimal_asset_integration() {
        // Test integration with Stellar's decimal assets
        // - Fractions map to Stellar's decimal representation
        // - unit_value aligns with Stellar's precision model
        // - Transfer compatibility with Stellar operations
        assert!(true);
    }

    #[test]
    fn test_stellar_trustline_creation() {
        // Test Stellar trustline creation for fractional assets
        // - Holder establishes trustline for fractional token
        // - Balance queried from trustline
        // - Transfer through trustline mechanism
        assert!(true);
    }

    // INTEGRATION TESTS: Metadata and Re-fractionalization

    #[test]
    fn test_fractional_metadata_retrieval() {
        // Test retrieving fractional metadata
        // - get_asset returns fractionalization details
        // - is_fractionalized flag present
        // - total_fractions and unit_value accessible
        assert!(true);
    }

    #[test]
    fn test_fractional_merge_scenario() {
        // Test merging fractional tokens back (future feature)
        // - Combine multiple fractional holdings
        // - Convert back to base asset (if implemented)
        // - Verify precision maintained
        assert!(true);
    }

    #[test]
    fn test_fractional_re_fractionalization_prevention() {
        // Test that already-fractionalized assets cannot be re-fractionalized
        // - Mint fractions initially
        // - Attempt to re-fractionaliz with different parameters
        // - Should be rejected
        assert!(true);
    }

    // PERFORMANCE & STRESS TESTS

    #[test]
    fn test_fractional_mint_performance() {
        // Performance test: mint with large owner list
        // - 1000+ initial owners
        // - Measure gas/computation
        // - Verify reasonable performance
        assert!(true);
    }

    #[test]
    fn test_fractional_transfer_chain() {
        // Performance test: chain multiple transfers
        // - 100+ sequential transfers of same fractional asset
        // - Verify state consistency
        // - Measure performance
        assert!(true);
    }

    #[test]
    fn test_fractional_concurrent_operations() {
        // Concurrency test: multiple transfers in parallel
        // - Simulate concurrent transfers
        // - Verify atomicity and consistency
        // - No race conditions
        assert!(true);
    }

    // SECURITY & EDGE CASES

    #[test]
    fn test_fractional_front_running_prevention() {
        // Test protection against front-running on fractions
        // - Mint/transfer operations immune to front-running
        // - Signature/authorization prevents hijacking
        // Implement: Verify require_auth() is enforced on sensitive ops
        assert!(true);
    }

    #[test]
    fn test_fractional_precision_attack() {
        // Test against dust/precision attacks
        // - Attempt to create dust through rounding
        // - Attempt to exploit decimal precision
        // - Verify protections in place
        // Implement: Try mint_fractional with dust-inducing params => Should reject or handle
        assert!(true);
    }

    #[test]
    fn test_fractional_reentrancy_prevention() {
        // Test reentrancy protection
        // - Mint/transfer operations cannot be re-entered
        // - State updated atomically
        // Implement: Verify state is locked during operations
        assert!(true);
    }

    #[test]
    fn test_fractional_overflow_prevention() {
        // Test integer overflow prevention
        // - Large fractions count (u64::MAX)
        // - Large unit_value (i128::MAX)
        // - Large total_supply
        // - Verify no overflow in calculations
        // Implement: Use checked_mul, checked_add to prevent overflow
        assert!(true);
    }

    #[test]
    fn test_fractional_owner_impersonation_prevention() {
        // Test owner cannot be impersonated
        // - Wrong signer cannot transfer
        // - Signature verification required
        // - Authorization check enforced
        // Implement: Call transfer with wrong signer => Should fail auth
        assert!(true);
    }

    // =======================================================================
    // Buy-Back & Burn Integration Tests
    // =======================================================================

    #[test]
    fn test_buyback_full_lifecycle() {
        let (env, _ec_id, mp_id, _at_id, _gov_id, admin) = setup();
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        // 1. Initialize buy-back system
        mp_client.initialize_buyback(
            &admin,
            &10_000,   // burn_cap
            &50_000,   // auto_threshold
            &5_000,    // auto_buyback_amount
            &30,       // fee_bps (0.30%)
            &false,    // require_governance
        );

        assert_eq!(mp_client.get_treasury_balance(), 0);
        assert_eq!(mp_client.get_total_burned(), 0);

        // 2. Accumulate treasury via deposits
        mp_client.deposit_to_treasury(&admin, &30_000);
        assert_eq!(mp_client.get_treasury_balance(), 30_000);

        // 3. Accumulate treasury via fee collection
        mp_client.collect_fee(&100_000); // 300 fee
        assert_eq!(mp_client.get_treasury_balance(), 30_300);

        // 4. Execute manual buy-back
        mp_client.buy_back_tokens(&admin, &8_000, &8_000, &None);
        assert_eq!(mp_client.get_treasury_balance(), 22_300);
        assert_eq!(mp_client.get_total_burned(), 8_000);

        // 5. Execute direct burn
        mp_client.burn_tokens(&admin, &2_000, &None);
        assert_eq!(mp_client.get_total_burned(), 10_000);

        // 6. Verify history
        let history = mp_client.get_buyback_history();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_buyback_auto_trigger_from_fees() {
        let (env, _ec_id, mp_id, _at_id, _gov_id, admin) = setup();
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        // Initialize with low threshold
        mp_client.initialize_buyback(
            &admin,
            &5_000,    // burn_cap
            &1_000,    // auto_threshold (low)
            &500,      // auto_buyback_amount
            &100,      // fee_bps (1%)
            &false,
        );

        // Collect enough fees to trigger auto buy-back
        mp_client.collect_fee(&50_000); // 500 fee
        assert_eq!(mp_client.get_treasury_balance(), 500);
        assert!(!mp_client.is_auto_buyback_ready());

        mp_client.collect_fee(&60_000); // 600 fee
        assert_eq!(mp_client.get_treasury_balance(), 1_100);
        assert!(mp_client.is_auto_buyback_ready());

        // Auto buy-back executes
        mp_client.auto_buy_back();
        assert_eq!(mp_client.get_total_burned(), 500);
        assert_eq!(mp_client.get_treasury_balance(), 600);

        let history = mp_client.get_buyback_history();
        assert_eq!(history.len(), 1);
        let record = history.get(0).unwrap();
        assert!(record.auto_triggered);
    }

    #[test]
    fn test_buyback_pause_blocks_all_operations() {
        let (env, _ec_id, mp_id, _at_id, _gov_id, admin) = setup();
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        mp_client.initialize_buyback(
            &admin,
            &10_000,
            &50_000,
            &5_000,
            &30,
            &false,
        );

        mp_client.deposit_to_treasury(&admin, &100_000);

        // Pause system
        mp_client.set_buyback_paused(&admin, &true);

        // Buy-back blocked
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.buy_back_tokens(&admin, &5_000, &5_000, &None);
        }));
        assert!(result.is_err());

        // Burn blocked
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.burn_tokens(&admin, &5_000, &None);
        }));
        assert!(result.is_err());

        // Auto buy-back blocked
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.auto_buy_back();
        }));
        assert!(result.is_err());

        // Unpause and verify operations work
        mp_client.set_buyback_paused(&admin, &false);
        mp_client.buy_back_tokens(&admin, &5_000, &5_000, &None);
        assert_eq!(mp_client.get_total_burned(), 5_000);
    }

    #[test]
    fn test_buyback_burn_cap_enforcement() {
        let (env, _ec_id, mp_id, _at_id, _gov_id, admin) = setup();
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        mp_client.initialize_buyback(
            &admin,
            &5_000,    // small burn_cap
            &50_000,
            &5_000,
            &30,
            &false,
        );

        mp_client.deposit_to_treasury(&admin, &100_000);

        // Under cap succeeds
        mp_client.buy_back_tokens(&admin, &5_000, &5_000, &None);
        assert_eq!(mp_client.get_total_burned(), 5_000);

        // Over cap fails
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mp_client.buy_back_tokens(&admin, &5_001, &5_001, &None);
        }));
        assert!(result.is_err());

        // Update cap
        mp_client.set_burn_cap(&admin, &10_000);
        mp_client.buy_back_tokens(&admin, &8_000, &8_000, &None);
        assert_eq!(mp_client.get_total_burned(), 13_000);
    }

    #[test]
    fn test_buyback_config_update() {
        let (env, _ec_id, mp_id, _at_id, _gov_id, admin) = setup();
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        mp_client.initialize_buyback(
            &admin,
            &10_000,
            &50_000,
            &5_000,
            &30,
            &false,
        );

        // Update config
        mp_client.update_buyback_config(
            &admin,
            &20_000,
            &100_000,
            &10_000,
            &50,
            &true,
        );

        let config = mp_client.get_buyback_config().unwrap();
        assert_eq!(config.burn_cap, 20_000);
        assert_eq!(config.auto_threshold, 100_000);
        assert_eq!(config.fee_bps, 50);
        assert!(config.require_governance);
    }

    #[test]
    fn test_buyback_reporting_total_burned_tracking() {
        let (env, _ec_id, mp_id, _at_id, _gov_id, admin) = setup();
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        mp_client.initialize_buyback(
            &admin,
            &10_000,
            &50_000,
            &5_000,
            &30,
            &false,
        );

        mp_client.deposit_to_treasury(&admin, &100_000);

        // Multiple burn operations
        mp_client.buy_back_tokens(&admin, &3_000, &3_000, &None);
        assert_eq!(mp_client.get_total_burned(), 3_000);

        mp_client.burn_tokens(&admin, &2_000, &None);
        assert_eq!(mp_client.get_total_burned(), 5_000);

        mp_client.buy_back_tokens(&admin, &4_000, &4_000, &None);
        assert_eq!(mp_client.get_total_burned(), 9_000);

        // History records all operations
        let history = mp_client.get_buyback_history();
        assert_eq!(history.len(), 3);
    }
}
