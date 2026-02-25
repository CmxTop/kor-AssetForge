use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec, Symbol, vec};

use crate::emergency_control::{EmergencyControlClient, PauseScope};
use crate::governance::GovernanceClient;

#[derive(Clone)]
#[contracttype]
pub struct Listing {
    pub asset_id: u64,
    pub seller: Address,
    pub price: i128,
    pub amount: i128,
    pub active: bool,
}

#[derive(Clone)]
#[contracttype]
pub enum MarketplaceDataKey {
    Admin,
    AssetPrivate(u64),
    Whitelisted(u64, Address),
}

#[contract]
pub struct Marketplace;

#[contractimpl]
impl Marketplace {
    /// List an asset for sale.
    /// Blocked if the asset is paused for Trading scope.
    /// Requires the asset to have been approved via governance.
    pub fn create_listing(
        env: Env,
        seller: Address,
        asset_id: u64,
        amount: i128,
        price: i128,
        emergency_control_id: Address,
        governance_id: Option<Address>,
    ) -> u64 {
        seller.require_auth();

        // Enforce pause check for trading operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Trading);

        // Enforce governance approval if governance contract is provided
        if let Some(gov_addr) = governance_id {
            let gov_client = GovernanceClient::new(&env, &gov_addr);
            gov_client.require_approved(&asset_id);
        }

        // Enforce whitelisting if asset is private
        Self::require_whitelisted_if_private(&env, asset_id, &seller);

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

        // Enforce whitelisting if asset is private
        Self::require_whitelisted_if_private(&env, asset_id, &buyer);

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

    /// Initialize the marketplace with an admin
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&MarketplaceDataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&MarketplaceDataKey::Admin, &admin);
    }

    /// Set an asset as private or public (Admin only)
    pub fn set_asset_privacy(env: Env, admin: Address, asset_id: u64, private: bool) {
        Self::require_admin(&env, &admin);
        env.storage().persistent().set(&MarketplaceDataKey::AssetPrivate(asset_id), &private);
        env.events().publish((Symbol::new(&env, "asset_privacy_updated"), asset_id), private);
    }

    /// Add a user to an asset's whitelist (Admin only)
    pub fn add_to_whitelist(env: Env, admin: Address, asset_id: u64, user: Address) {
        Self::require_admin(&env, &admin);
        env.storage().persistent().set(&MarketplaceDataKey::Whitelisted(asset_id, user.clone()), &true);
        env.events().publish((Symbol::new(&env, "user_whitelisted"), asset_id), user);
    }

    /// Remove a user from an asset's whitelist (Admin only)
    pub fn remove_from_whitelist(env: Env, admin: Address, asset_id: u64, user: Address) {
        Self::require_admin(&env, &admin);
        env.storage().persistent().set(&MarketplaceDataKey::Whitelisted(asset_id, user.clone()), &false);
        env.events().publish((Symbol::new(&env, "user_removed"), asset_id), user);
    }

    /// Bulk add users to an asset's whitelist (Admin only)
    pub fn bulk_add_to_whitelist(env: Env, admin: Address, asset_id: u64, users: Vec<Address>) {
        Self::require_admin(&env, &admin);
        for user in users.iter() {
            env.storage().persistent().set(&MarketplaceDataKey::Whitelisted(asset_id, user.clone()), &true);
            env.events().publish((Symbol::new(&env, "user_whitelisted"), asset_id), user);
        }
    }

    /// Check if a user is whitelisted for an asset
    pub fn is_whitelisted(env: Env, asset_id: u64, user: Address) -> bool {
        env.storage().persistent().get(&MarketplaceDataKey::Whitelisted(asset_id, user)).unwrap_or(false)
    }

    /// Check if an asset is private
    pub fn is_private(env: Env, asset_id: u64) -> bool {
        env.storage().persistent().get(&MarketplaceDataKey::AssetPrivate(asset_id)).unwrap_or(false)
    }

    // Helper: Require admin authorization
    fn require_admin(env: &Env, admin: &Address) {
        admin.require_auth();
        let stored_admin: Address = env.storage().instance().get(&MarketplaceDataKey::Admin).expect("admin not set");
        if admin != &stored_admin {
            panic!("not authorized: admin only");
        }
    }

    // Helper: Require user whitelisted if asset is private
    fn require_whitelisted_if_private(env: &Env, asset_id: u64, user: &Address) {
        if Self::is_private(env.clone(), asset_id) {
            if !Self::is_whitelisted(env.clone(), asset_id, user.clone()) {
                panic!("user not whitelisted for private asset");
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::asset_token::{AssetToken, AssetTokenClient};
    use crate::emergency_control::EmergencyControl;
    use crate::governance::{Governance, GovernanceClient};
    use soroban_sdk::testutils::{Address as _, Ledger};
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

        // No governance gate (None)
        let listing_id =
            mp_client.create_listing(&seller, &asset_id, &amount, &price, &ec_id, &None);
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
        mp_client.create_listing(&seller, &1, &100, &1000, &ec_id, &None);
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

    // -----------------------------------------------------------------------
    // Governance-gated listing tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_listing_with_governance_approved() {
        let env = Env::default();
        env.mock_all_auths();

        // Emergency control
        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        // Asset / governance token
        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "GOV"),
            &String::from_str(&env, "GOV"),
            &7,
        );

        // Governance
        let gov_id = env.register_contract(None, Governance);
        let gov_client = GovernanceClient::new(&env, &gov_id);
        gov_client.initialize(&admin, &at_id, &100, &50);

        // Mint tokens and run governance flow
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);
        at_client.mint(&proposer, &200, &1, &ec_id);
        at_client.mint(&voter, &150, &1, &ec_id);

        let asset_id: u64 = 1;
        let pid =
            gov_client.create_proposal(&proposer, &asset_id, &String::from_str(&env, "List"), &3600);
        gov_client.vote(&voter, &pid, &true);

        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });
        gov_client.tally_execute(&pid);

        // Now listing should succeed with governance gate
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let seller = Address::generate(&env);

        let lid = mp_client.create_listing(
            &seller,
            &asset_id,
            &100,
            &1000,
            &ec_id,
            &Some(gov_id),
        );
        assert_eq!(lid, 1);
    }

    #[test]
    #[should_panic(expected = "asset not approved by governance")]
    fn test_create_listing_blocked_without_governance_approval() {
        let env = Env::default();
        env.mock_all_auths();

        // Emergency control
        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        // Asset / governance token
        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "GOV"),
            &String::from_str(&env, "GOV"),
            &7,
        );

        // Governance (no proposals passed)
        let gov_id = env.register_contract(None, Governance);
        let gov_client = GovernanceClient::new(&env, &gov_id);
        gov_client.initialize(&admin, &at_id, &100, &50);

        // Try listing with governance gate — should fail
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let seller = Address::generate(&env);

        mp_client.create_listing(&seller, &1, &100, &1000, &ec_id, &Some(gov_id));
    }

    // -----------------------------------------------------------------------
    // Whitelisting tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize_and_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        mp_client.initialize(&admin);
        
        let not_admin = Address::generate(&env);
        env.mock_all_auths();
        
        // This should pass with admin auth
        mp_client.set_asset_privacy(&admin, &1, &true);
        assert!(mp_client.is_private(&1));

        // This should panic because not_admin is not the stored admin
        // Note: mock_all_auths handles require_auth, but our logic checks the stored address
    }

    #[test]
    #[should_panic(expected = "not authorized: admin only")]
    fn test_set_privacy_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);

        mp_client.initialize(&admin);
        
        let not_admin = Address::generate(&env);
        mp_client.set_asset_privacy(&not_admin, &1, &true);
    }

    #[test]
    fn test_whitelisting_flow() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        mp_client.initialize(&admin);

        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let asset_id = 1;

        assert!(!mp_client.is_whitelisted(&asset_id, &user1));

        // Add to whitelist
        mp_client.add_to_whitelist(&admin, &asset_id, &user1);
        assert!(mp_client.is_whitelisted(&asset_id, &user1));

        // Bulk add
        let users = vec![&env, user2.clone()];
        mp_client.bulk_add_to_whitelist(&admin, &asset_id, &users);
        assert!(mp_client.is_whitelisted(&asset_id, &user2));

        // Remove from whitelist
        mp_client.remove_from_whitelist(&admin, &asset_id, &user1);
        assert!(!mp_client.is_whitelisted(&asset_id, &user1));
    }

    #[test]
    #[should_panic(expected = "user not whitelisted for private asset")]
    fn test_create_listing_private_asset_not_whitelisted() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let ec_id = env.register_contract(None, EmergencyControl);
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        mp_client.initialize(&admin);

        let asset_id = 42;
        mp_client.set_asset_privacy(&admin, &asset_id, &true);

        let seller = Address::generate(&env);
        // Not whitelisted, should panic
        mp_client.create_listing(&seller, &asset_id, &100, &1000, &ec_id, &None);
    }

    #[test]
    fn test_create_listing_private_asset_whitelisted() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let ec_id = env.register_contract(None, EmergencyControl);
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        mp_client.initialize(&admin);

        let asset_id = 42;
        mp_client.set_asset_privacy(&admin, &asset_id, &true);

        let seller = Address::generate(&env);
        mp_client.add_to_whitelist(&admin, &asset_id, &seller);

        // Whitelisted, should succeed
        let lid = mp_client.create_listing(&seller, &asset_id, &100, &1000, &ec_id, &None);
        assert_eq!(lid, 1);
    }

    #[test]
    #[should_panic(expected = "user not whitelisted for private asset")]
    fn test_purchase_private_asset_not_whitelisted() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let ec_id = env.register_contract(None, EmergencyControl);
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        mp_client.initialize(&admin);

        let asset_id = 42;
        mp_client.set_asset_privacy(&admin, &asset_id, &true);

        let buyer = Address::generate(&env);
        // Not whitelisted, should panic
        mp_client.purchase(&buyer, &1, &50, &asset_id, &ec_id);
    }
}
