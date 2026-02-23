use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol};

use crate::emergency_control::{EmergencyControl, EmergencyControlClient, PauseScope};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    AssetInfo,
    Balance(Address),
    TotalSupply,
}

#[derive(Clone)]
#[contracttype]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
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
        decimals: u32,
    ) {
        if env.storage().instance().has(&DataKey::AssetInfo) {
            panic!("already initialized");
        }

        admin.require_auth();

        // Store asset metadata
        let asset = Asset {
            id: 1, // Simplified for this implementation
            name,
            symbol,
            decimals,
            owner: admin.clone(),
        };

        env.storage().instance().set(&DataKey::AssetInfo, &asset);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);
    }

    /// Mint new tokens for an asset.
    /// Blocked if the asset is paused for Minting scope.
    pub fn mint(
        env: Env,
        to: Address,
        amount: i128,
        asset_id: u64,
        emergency_control_id: Address,
    ) {
        if amount <= 0 {
            panic!("amount must be positive");
        }

        // Only admin can mint
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();

        // Enforce pause check for minting operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Minting);

        // Update balance
        let balance = Self::balance(env.clone(), to.clone());
        let new_balance = balance.checked_add(amount).expect("balance overflow");
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &new_balance);

        // Update total supply
        let total_supply = Self::total_supply(env.clone());
        let new_total_supply = total_supply.checked_add(amount).expect("supply overflow");
        env.storage().instance().set(&DataKey::TotalSupply, &new_total_supply);

        // Emit Mint event
        env.events().publish(
            (Symbol::new(&env, "mint"), to),
            amount,
        );
    }

    /// Get balance of an address
    pub fn balance(env: Env, address: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(address))
            .unwrap_or(0)
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
    ) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        
        from.require_auth();

        // Enforce pause check for transfer operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Transfers);

        // Check and update sender balance
        let from_balance = Self::balance(env.clone(), from.clone());
        if from_balance < amount {
            panic!("insufficient balance");
        }
        let new_from_balance = from_balance - amount;
        env.storage().persistent().set(&DataKey::Balance(from.clone()), &new_from_balance);

        // Update recipient balance
        let to_balance = Self::balance(env.clone(), to.clone());
        let new_to_balance = to_balance.checked_add(amount).expect("balance overflow");
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &new_to_balance);

        // Emit Transfer event
        env.events().publish(
            (Symbol::new(&env, "transfer"), from, to),
            amount,
        );
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    pub fn name(env: Env) -> String {
        let asset: Asset = env.storage().instance().get(&DataKey::AssetInfo).expect("asset not initialized");
        asset.name
    }

    pub fn symbol(env: Env) -> String {
        let asset: Asset = env.storage().instance().get(&DataKey::AssetInfo).expect("asset not initialized");
        asset.symbol
    }

    pub fn decimals(env: Env) -> u32 {
        let asset: Asset = env.storage().instance().get(&DataKey::AssetInfo).expect("asset not initialized");
        asset.decimals
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
        let decimals = 7;

        client.initialize(&admin, &name, &symbol, &decimals);
        
        assert_eq!(client.name(), name);
        assert_eq!(client.symbol(), symbol);
        assert_eq!(client.decimals(), decimals);
        assert_eq!(client.total_supply(), 0);
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_initialize_twice_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let name = String::from_str(&env, "Token");
        client.initialize(&admin, &name, &name, &7);
        client.initialize(&admin, &name, &name, &7);
    }

    #[test]
    fn test_mint_success() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);

        let to = Address::generate(&env);
        at_client.mint(&to, &1000, &1, &ec_id);

        assert_eq!(at_client.balance(&to), 1000);
        assert_eq!(at_client.total_supply(), 1000);
    }

    #[test]
    fn test_transfer_success() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);

        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        
        at_client.mint(&user1, &1000, &1, &ec_id);
        at_client.transfer(&user1, &user2, &400, &1, &ec_id);

        assert_eq!(at_client.balance(&user1), 600);
        assert_eq!(at_client.balance(&user2), 400);
        assert_eq!(at_client.total_supply(), 1000);
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_transfer_insufficient_balance() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);

        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        
        at_client.mint(&user1, &100, &1, &ec_id);
        at_client.transfer(&user1, &user2, &101, &1, &ec_id);
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

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);

        let reason = String::from_str(&env, "minting halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::Minting, &reason, &0);

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

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);

        let reason = String::from_str(&env, "transfer freeze");
        ec_client.pause_asset(&admin, &1, &PauseScope::Transfers, &reason, &0);

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

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);

        let from = Address::generate(&env);
        let to = Address::generate(&env);
        
        // Mint tokens BEFORE pausing
        at_client.mint(&from, &1000, &1, &ec_id);

        // Pause only minting - transfers should still work
        let reason = String::from_str(&env, "minting halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::Minting, &reason, &0);

        at_client.transfer(&from, &to, &500, &1, &ec_id);
        assert_eq!(at_client.balance(&to), 500);
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

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);

        let reason = String::from_str(&env, "global halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::All, &reason, &0);

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

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);

        let reason = String::from_str(&env, "global halt");
        ec_client.pause_asset(&admin, &1, &PauseScope::All, &reason, &0);

        let from = Address::generate(&env);
        let to = Address::generate(&env);
        at_client.transfer(&from, &to, &500, &1, &ec_id);
    }
}
