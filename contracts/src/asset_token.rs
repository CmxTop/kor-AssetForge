use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec, Map};


#[derive(Clone)]
#[contracttype]
pub struct FractionalMintedEvent {
    pub asset_id: u64,
    pub total_fractions: u64,
    pub unit_value: i128,
    pub issuer: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct FractionalTransferEvent {
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub asset_id: u64,
}


#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
#[allow(dead_code)]
pub enum FractionalError {
    UnauthorizedAdmin = 1,
    AlreadyFractionalized = 2,
    ZeroFractions = 3,
    UnevenDivision = 4,
    InvalidOwnerList = 5,
    InsufficientBalance = 6,
    ArithmeticOverflow = 7,
    InvalidAsset = 8,
}



// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
    pub owner: Address,
    // Fractional minting fields
    pub is_fractionalized: bool,
    pub total_fractions: u64,
    pub unit_value: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct ValuationConfig {
    pub min_interval: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct ValuationRecord {
    pub value: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct DividendSchedule {
    pub total_dividend: i128,
    pub payout_asset: Address,
    pub next_payout_time: u64,
    pub interval: u64,
    pub amount_per_token: i128,
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
        // Note: In production, you'd verify admin authorization here
        // For now, we store the admin and verify on fractionalization
        
        // Generate asset ID (simplified - use counter in production)
        let asset_id: u64 = 1;
        
        // Store asset metadata
        let asset = Asset {
            id: 1, // Simplified for this implementation
            name,
            symbol,
            decimals,
            owner: admin.clone(),
            is_fractionalized: false,
            total_fractions: 0,
            unit_value: 0,
        };

    /// Mint fractional tokens for an asset
    /// 
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `admin` - Admin address with authorization
    /// * `total_value` - Total value of the asset being fractionalized
    /// * `fractions` - Number of fractional shares to create
    /// * `initial_owners` - Optional vector of (address, share_count) tuples for initial distribution
    ///
    /// # Returns
    /// * `Result<u64, FractionalError>` - Asset ID on success or error code
    ///
    /// # Errors
    /// * `UnauthorizedAdmin` - Caller is not the asset admin
    /// * `AlreadyFractionalized` - Asset is already fractionalized
    /// * `ZeroFractions` - Fractions parameter is 0
    /// * `UnevenDivision` - total_value not evenly divisible by fractions
    /// * `InvalidOwnerList` - Owner list is invalid (duplicates, exceeds fractions, etc.)
    /// * `ArithmeticOverflow` - Calculation would overflow i128
    pub fn mint_fractional(
        env: Env,
        admin: Address,
        total_value: i128,
        fractions: u64,
        initial_owners: Option<Vec<(Address, u64)>>,
    ) -> u64 {
        // Retrieve current asset
        let mut asset: Asset = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "asset"))
            .expect("Asset not found");

        // Verify admin authorization - ensure caller is the asset owner
        assert_eq!(asset.owner, admin, "Unauthorized: not asset admin");

        // Prevent re-fractionalization
        assert!(!asset.is_fractionalized, "Asset already fractionalized");

        // Validate fractions > 0
        assert!(fractions > 0, "Fractions must be > 0");

        // Calculate unit_value: total_value / fractions (must divide evenly)
        // Ensure total_value is evenly divisible by fractions
        assert_eq!(total_value % (fractions as i128), 0, "Uneven division: total_value not divisible by fractions");

        let unit_value = total_value.checked_div(fractions as i128)
            .expect("Arithmetic overflow in division");

        // Initialize fractional balances storage
        let mut balances: Map<Address, i128> = env.storage().instance()
            .get(&Symbol::new(&env, "balances"))
            .unwrap_or_else(|| Map::new(&env));

        // Validate and distribute initial owners
        let mut total_distributed: u64 = 0;

        if let Some(owners) = initial_owners {
            let mut seen_addresses: Vec<Address> = Vec::new(&env);

            for (owner_addr, share_count) in owners.iter() {
                // Check for duplicates
                for seen in seen_addresses.iter() {
                    assert_ne!(seen, owner_addr, "Duplicate address in owner list");
                }
                seen_addresses.push_back(owner_addr.clone());

                // Check distribution doesn't exceed fractions
                total_distributed = total_distributed.checked_add(share_count)
                    .expect("Arithmetic overflow in share distribution");

                assert!(total_distributed <= fractions, "Share distribution exceeds total fractions");

                // Calculate and store balance
                let balance = (share_count as i128).checked_mul(unit_value)
                    .expect("Arithmetic overflow in balance calculation");

                balances.set(owner_addr.clone(), balance);
            }
        }

        // Update asset to mark as fractionalized
        asset.is_fractionalized = true;
        asset.total_fractions = fractions;
        asset.unit_value = unit_value;

        // Store updated asset
        env.storage().instance().set(&Symbol::new(&env, "asset"), &asset);

        // Store balances
        env.storage().instance().set(&Symbol::new(&env, "balances"), &balances);

        // Emit fractional minted event
        let event = FractionalMintedEvent {
            asset_id: asset.id,
            total_fractions: fractions,
            unit_value,
            issuer: admin,
        };

        env.events()
            .publish((Symbol::new(&env, "fractions_minted"),), event);

        asset.id
    }

    /// Mint new tokens for an asset (non-fractional minting)
    pub fn mint(_env: Env, _to: Address, _amount: i128) -> bool {
        
        // TODO: Implement minting logic for non-fractionalized tokens
        // - Check authorization
        // - Update balances
        // - Emit events
        
        true
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
        let balances: Map<Address, i128> = env.storage().instance()
            .get(&Symbol::new(&env, "balances"))
            .unwrap_or_else(|| Map::new(&env));

        balances.get(address).unwrap_or(0)
    }

    /// Transfer fractional tokens between addresses
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> bool {
        // Note: In production, you'd verify transfer authorization here
        // For now, we verify through address comparison in business logic

        // Get asset to verify it's fractionalized
        let asset: Asset = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "asset"))
            .expect("Asset not found");

        assert!(asset.is_fractionalized, "Asset is not fractionalized");

        // Get balances map
        let mut balances: Map<Address, i128> = env.storage().instance()
            .get(&Symbol::new(&env, "balances"))
            .unwrap_or_else(|| Map::new(&env));

        // Get from balance
        let from_balance = balances.get(from.clone()).unwrap_or(0);

        // Check sufficient balance
        assert!(from_balance >= amount, "Insufficient balance");

        if from != to {
            // Update balances for different addresses
            let new_from_balance = from_balance.checked_sub(amount)
                .expect("Arithmetic overflow in subtraction");

            let to_balance = balances.get(to.clone()).unwrap_or(0);
            let new_to_balance = to_balance.checked_add(amount)
                .expect("Arithmetic overflow in addition");

            balances.set(from.clone(), new_from_balance);
            balances.set(to.clone(), new_to_balance);
        }

        // Store updated balances
        env.storage().instance().set(&Symbol::new(&env, "balances"), &balances);

        // Emit transfer event
        let event = FractionalTransferEvent {
            from: from.clone(),
            to,
            amount,
            asset_id: asset.id,
        };

        env.events()
            .publish((Symbol::new(&env, "fractional_transfer"),), event);

        true
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::emergency_control::EmergencyControl;
    use soroban_sdk::testutils::{Address as _, Ledger};

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

        let asset_id = client.initialize(&admin, &name, &symbol, &supply);
        assert_eq!(asset_id, 1);

        // Verify asset is initialized but not fractionalized
        let asset = client.get_asset();
        assert!(asset.is_some());
        let asset = asset.unwrap();
        assert_eq!(asset.is_fractionalized, false);
        assert_eq!(asset.total_fractions, 0);
        assert_eq!(asset.unit_value, 0);
    }

    // UNIT TESTS: Fractional Minting Core Logic

    #[test]
    fn test_fractional_mint_basic() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        // Mint 1000 fractions from 100,000 total value (unit_value = 100)
        let asset_id = client.mint_fractional(&admin, &100_000i128, &1000u64, &None);
        assert_eq!(asset_id, 1u64);

        let asset = client.get_asset().unwrap();
        assert_eq!(asset.is_fractionalized, true);
        assert_eq!(asset.total_fractions, 1000);
        assert_eq!(asset.unit_value, 100);
    }

    #[test]
    fn test_fractional_mint_with_decimal_handling() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        // Large total_value with high precision
        let total_value: i128 = 1_000_000_000_000_000_000i128; // 10^18
        let fractions = 1_000_000u64;
        let unit_value = total_value / (fractions as i128);

        let asset_id = client.mint_fractional(&admin, &total_value, &fractions, &None);
        assert_eq!(asset_id, 1u64);

        let asset = client.get_asset().unwrap();
        assert_eq!(asset.unit_value, unit_value);
    }

    #[test]
    fn test_fractional_mint_single_owner() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        let mut owners = Vec::new(&env);
        owners.push_back((owner.clone(), 1000u64));

        let result = client.try_mint_fractional(&admin, &100_000i128, &1000u64, &Some(owners));
        assert!(result.is_ok());

        let balance = client.balance(&owner);
        assert_eq!(balance, 100_000i128); // 1000 fractions * 100 unit_value
    }

    #[test]
    fn test_fractional_mint_multiple_owners() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let owner1 = Address::generate(&env);
        let owner2 = Address::generate(&env);
        let owner3 = Address::generate(&env);

        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        let mut owners = Vec::new(&env);
        owners.push_back((owner1.clone(), 200u64)); // 20%
        owners.push_back((owner2.clone(), 300u64)); // 30%
        owners.push_back((owner3.clone(), 500u64)); // 50%

        let result = client.try_mint_fractional(&admin, &100_000i128, &1000u64, &Some(owners));
        assert!(result.is_ok());

        let balance1 = client.balance(&owner1);
        let balance2 = client.balance(&owner2);
        let balance3 = client.balance(&owner3);

        assert_eq!(balance1, 20_000i128);
        assert_eq!(balance2, 30_000i128);
        assert_eq!(balance3, 50_000i128);
    }

    #[test]
    #[should_panic]
    fn test_fractional_mint_error_zero_fractions() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        let _ = client.mint_fractional(&admin, &100_000i128, &0u64, &None);
    }

    #[test]
    #[should_panic]
    fn test_fractional_mint_error_uneven_division() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        // 100_000 / 1001 = 99.90... (not evenly divisible)
        let _ = client.mint_fractional(&admin, &100_000i128, &1001u64, &None);
    }

    #[test]
    #[should_panic]
    fn test_fractional_mint_error_already_fractionalized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        let _ = client.mint_fractional(&admin, &100_000i128, &1000u64, &None);

        // Try to fractionaliz again - should panic
        let _ = client.mint_fractional(&admin, &200_000i128, &2000u64, &None);
    }

    // UNIT TESTS: Fractional Token Transfers

    #[test]
    fn test_fractional_transfer_basic() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let recipient = Address::generate(&env);

        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        let mut owners = Vec::new(&env);
        owners.push_back((owner.clone(), 1000u64));

        let _ = client.mint_fractional(&admin, &100_000i128, &1000u64, &Some(owners));

        // Transfer 50_000 from owner to recipient (use try_ variant to bypass auth in tests)
        let result = client.try_transfer(&owner, &recipient, &50_000i128);
        assert!(result.is_ok());

        let owner_balance = client.balance(&owner);
        let recipient_balance = client.balance(&recipient);

        assert_eq!(owner_balance, 50_000i128);
        assert_eq!(recipient_balance, 50_000i128);
    }

    #[test]
    #[should_panic]
    fn test_fractional_transfer_insufficient_balance() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let recipient = Address::generate(&env);

        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        let mut owners = Vec::new(&env);
        owners.push_back((owner.clone(), 500u64));

        let _ = client.mint_fractional(&admin, &100_000i128, &1000u64, &Some(owners));

        // Try to transfer more than available (60_000 > 50_000) - should panic
        let _ = client.try_transfer(&owner, &recipient, &60_000i128).unwrap();
    }

    #[test]
    fn test_fractional_transfer_to_self() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let owner = Address::generate(&env);

        let name = String::from_str(&env, "Real Estate Token");
        let symbol = String::from_str(&env, "RET");
        let supply = 1_000_000;

        let _ = client.initialize(&admin, &name, &symbol, &supply);

        let mut owners = Vec::new(&env);
        owners.push_back((owner.clone(), 1000u64));

        let _ = client.mint_fractional(&admin, &100_000i128, &1000u64, &Some(owners));

        // Transfer to self (use try_ variant to bypass auth in tests)
        let result = client.try_transfer(&owner, &owner, &50_000i128);
        assert!(result.is_ok());

        let balance = client.balance(&owner);
        assert_eq!(balance, 100_000i128); // Balance unchanged
    }
}
