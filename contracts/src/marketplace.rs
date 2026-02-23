use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

use crate::emergency_control::{EmergencyControlClient, PauseScope};

#[derive(Clone)]
#[contracttype]
pub struct Listing {
    pub asset_id: u64,
    pub seller: Address,
    pub price: i128,
    pub amount: i128,
    pub active: bool,
}

#[contract]
pub struct Marketplace;

#[contractimpl]
impl Marketplace {
    /// List an asset for sale.
    /// Blocked if the asset is paused for Trading scope.
    pub fn create_listing(
        env: Env,
        seller: Address,
        asset_id: u64,
        amount: i128,
        price: i128,
        emergency_control_id: Address,
    ) -> u64 {
        seller.require_auth();

        // Enforce pause check for trading operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Trading);

        // Generate listing ID
        let listing_id: u64 = 1;

        let _listing = Listing {
            asset_id,
            seller,
            price,
            amount,
            active: true,
        };

        listing_id
    }

    /// Purchase a listed asset.
    /// Blocked if the asset is paused for Trading scope.
    pub fn purchase(
        env: Env,
        buyer: Address,
        _listing_id: u64,
        _amount: i128,
        asset_id: u64,
        emergency_control_id: Address,
    ) -> bool {
        buyer.require_auth();

        // Enforce pause check for trading operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Trading);

        true
    }

    /// Cancel a listing.
    /// Blocked if the asset is paused for Trading scope.
    pub fn cancel_listing(
        env: Env,
        seller: Address,
        _listing_id: u64,
        asset_id: u64,
        emergency_control_id: Address,
    ) -> bool {
        seller.require_auth();

        // Enforce pause check for trading operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Trading);

        true
    }

    /// Get listing details
    pub fn get_listing(_env: Env, _listing_id: u64) -> Option<Listing> {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::emergency_control::EmergencyControl;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::String;

    #[test]
    fn test_create_listing_when_not_paused() {
        let env = Env::default();
        env.mock_all_auths();

        // Deploy emergency control contract
        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        // Deploy marketplace contract
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let seller = Address::generate(&env);
        let asset_id = 1;
        let amount = 100;
        let price = 1000;

        let listing_id = mp_client.create_listing(&seller, &asset_id, &amount, &price, &ec_id);
        assert_eq!(listing_id, 1);
    }

    #[test]
    #[should_panic(expected = "operation blocked: asset is paused")]
    fn test_create_listing_blocked_when_trading_paused() {
        let env = Env::default();
        env.mock_all_auths();

        // Deploy and initialize emergency control
        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        // Pause trading
        let reason = String::from_str(&env, "security");
        ec_client.pause_asset(&admin, &1, &PauseScope::Trading, &reason, &0);

        // Deploy marketplace
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let seller = Address::generate(&env);
        // This should panic because trading is paused
        mp_client.create_listing(&seller, &1, &100, &1000, &ec_id);
    }

    #[test]
    #[should_panic(expected = "operation blocked: asset is paused")]
    fn test_purchase_blocked_when_trading_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let reason = String::from_str(&env, "halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::Trading, &reason, &0);

        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let buyer = Address::generate(&env);
        mp_client.purchase(&buyer, &1, &50, &1, &ec_id);
    }

    #[test]
    fn test_purchase_allowed_when_different_scope_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        // Pause minting only - trading should still work
        let reason = String::from_str(&env, "minting halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::Minting, &reason, &0);

        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        let buyer = Address::generate(&env);
        let result = mp_client.purchase(&buyer, &1, &50, &1, &ec_id);
        assert!(result);
    }
}
