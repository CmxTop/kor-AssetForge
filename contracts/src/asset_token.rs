use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol};

use crate::emergency_control::{EmergencyControl, EmergencyControlClient, PauseScope};

#[derive(Clone)]
#[contracttype]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub symbol: String,
    pub total_supply: i128,
    pub owner: Address,
}

#[contract]
pub struct AssetToken;

#[contractimpl]
impl AssetToken {
    /// Initialize a new asset token
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
        total_supply: i128,
    ) -> u64 {
        admin.require_auth();

        // Generate asset ID (simplified - use counter in production)
        let asset_id: u64 = 1;

        // Store asset metadata
        let asset = Asset {
            id: asset_id,
            name,
            symbol,
            total_supply,
            owner: admin.clone(),
        };

        env.storage()
            .instance()
            .set(&Symbol::new(&env, "asset"), &asset);

        asset_id
    }

    /// Mint new tokens for an asset.
    /// Blocked if the asset is paused for Minting scope.
    pub fn mint(
        env: Env,
        to: Address,
        amount: i128,
        asset_id: u64,
        emergency_control_id: Address,
    ) -> bool {
        to.require_auth();

        // Enforce pause check for minting operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Minting);

        // TODO: Implement minting logic
        // - Check authorization
        // - Update balances
        // - Emit events

        true
    }

    /// Get asset details
    pub fn get_asset(env: Env) -> Option<Asset> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "asset"))
    }

    /// Get balance of an address
    pub fn balance(env: Env, address: Address) -> i128 {
        // TODO: Implement balance lookup
        0
    }

    /// Transfer tokens between addresses.
    /// Blocked if the asset is paused for Transfers scope.
    pub fn transfer(
        env: Env,
        from: Address,
        to: Address,
        amount: i128,
        asset_id: u64,
        emergency_control_id: Address,
    ) -> bool {
        from.require_auth();

        // Enforce pause check for transfer operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Transfers);

        // TODO: Implement transfer logic
        // - Check balance
        // - Update balances
        // - Emit events

        true
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::emergency_control::EmergencyControl;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let asset_id = client.initialize(&admin, &name, &symbol, &supply);
        assert_eq!(asset_id, 1);
    }

    #[test]
    fn test_mint_when_not_paused() {
        let env = Env::default();
        env.mock_all_auths();

        // Deploy emergency control contract
        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        // Deploy asset token contract
        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let to = Address::generate(&env);
        let result = at_client.mint(&to, &1000, &1, &ec_id);
        assert!(result);
    }

    #[test]
    #[should_panic(expected = "operation blocked: asset is paused")]
    fn test_mint_blocked_when_minting_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let reason = String::from_str(&env, "minting halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::Minting, &reason, &0);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let to = Address::generate(&env);
        at_client.mint(&to, &1000, &1, &ec_id);
    }

    #[test]
    #[should_panic(expected = "operation blocked: asset is paused")]
    fn test_transfer_blocked_when_transfers_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let reason = String::from_str(&env, "transfer freeze");
        ec_client.pause_asset(&admin, &1, &PauseScope::Transfers, &reason, &0);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let from = Address::generate(&env);
        let to = Address::generate(&env);
        at_client.transfer(&from, &to, &500, &1, &ec_id);
    }

    #[test]
    fn test_transfer_allowed_when_minting_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        // Pause only minting - transfers should still work
        let reason = String::from_str(&env, "minting halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::Minting, &reason, &0);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let from = Address::generate(&env);
        let to = Address::generate(&env);
        let result = at_client.transfer(&from, &to, &500, &1, &ec_id);
        assert!(result);
    }

    #[test]
    #[should_panic(expected = "operation blocked: asset is paused")]
    fn test_all_pause_blocks_mint() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let reason = String::from_str(&env, "global halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::All, &reason, &0);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let to = Address::generate(&env);
        at_client.mint(&to, &1000, &1, &ec_id);
    }

    #[test]
    #[should_panic(expected = "operation blocked: asset is paused")]
    fn test_all_pause_blocks_transfer() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let reason = String::from_str(&env, "global halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::All, &reason, &0);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let from = Address::generate(&env);
        let to = Address::generate(&env);
        at_client.transfer(&from, &to, &500, &1, &ec_id);
    }
}
