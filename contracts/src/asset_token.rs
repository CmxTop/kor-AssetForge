use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec, Val, IntoVal,
};

use crate::emergency_control::{EmergencyControlClient, PauseScope};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    AssetInfo,
    Balance(Address),
    TotalSupply,
    Oracle,
    Valuation,
    ValuationHistory,
    ValuationConfig,
    ValuationTimestamps,
    DividendSchedule(u64), // asset_id -> schedule
    LastClaim(u64, Address),
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

    /// Set the trusted oracle address. (Admin only)
    pub fn set_oracle(env: Env, oracle: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        env.storage().instance().set(&DataKey::Oracle, &oracle);
    }

    /// Set the minimum time interval between updates. (Admin only)
    pub fn set_valuation_config(env: Env, min_interval: u64) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        let config = ValuationConfig { min_interval };
        env.storage().instance().set(&DataKey::ValuationConfig, &config);
    }

    /// Get current valuation and timestamp.
    pub fn get_valuation(env: Env) -> Option<ValuationRecord> {
        env.storage().instance().get(&DataKey::Valuation)
    }

    /// Update the asset valuation. (Admin or Oracle only)
    pub fn update_valuation(env: Env, updater: Address, new_value: i128) {
        updater.require_auth();

        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        let oracle: Option<Address> = env.storage().instance().get(&DataKey::Oracle);
        
        let is_admin = updater == admin;
        let is_oracle = if let Some(o) = oracle { updater == o } else { false };
        
        if !is_admin && !is_oracle {
            panic!("not authorized");
        }

        // Enforce interval
        let now = env.ledger().timestamp();
        let config: ValuationConfig = env.storage().instance()
            .get(&DataKey::ValuationConfig)
            .unwrap_or(ValuationConfig { min_interval: 0 });
            
        if let Some(last) = env.storage().instance().get::<_, ValuationRecord>(&DataKey::Valuation) {
            if now < last.timestamp + config.min_interval {
                panic!("too frequent update");
            }
        }

        let record = ValuationRecord { value: new_value, timestamp: now };
        env.storage().instance().set(&DataKey::Valuation, &record);

        // Store in history
        let mut history: soroban_sdk::Vec<ValuationRecord> = env.storage().persistent()
            .get(&DataKey::ValuationHistory)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        history.push_back(record.clone());
        env.storage().persistent().set(&DataKey::ValuationHistory, &history);

        // Emit valuation event
        env.events().publish(
            (Symbol::new(&env, "valuation_updated"),),
            new_value,
        );
    }

    pub fn schedule_dividend(
        env: Env,
        asset_id: u64,
        total_dividend: i128,
        payout_asset: Address,
        interval: u64,
    ) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();

        if total_dividend <= 0 {
            panic!("dividend amount must be positive");
        }

        let total_supply = Self::total_supply(env.clone());
        if total_supply == 0 {
            panic!("cannot distribute to zero supply");
        }

        let now = env.ledger().timestamp();
        
        // Calculate amount per token scaled for precision (e.g., 7 decimals)
        // amount_per_token = (total_dividend * multiplier) / total_supply
        let amount_per_token = (total_dividend.checked_mul(10_000_000).expect("overflow")) / total_supply;

        let schedule = DividendSchedule {
            total_dividend,
            payout_asset,
            next_payout_time: now + interval,
            interval,
            amount_per_token,
        };

        env.storage().persistent().set(&DataKey::DividendSchedule(asset_id), &schedule);

        env.events().publish(
            (Symbol::new(&env, "dividend_scheduled"), asset_id),
            total_dividend,
        );
    }

    pub fn claim_dividend(env: Env, asset_id: u64, claimant: Address) {
        claimant.require_auth();

        let schedule: DividendSchedule = env.storage().persistent()
            .get(&DataKey::DividendSchedule(asset_id))
            .expect("no dividend schedule found");

        let now = env.ledger().timestamp();
        if now < schedule.next_payout_time {
            panic!("payout not yet due");
        }

        // Double-claim protection
        let last_claim_key = DataKey::LastClaim(asset_id, claimant.clone());
        if let Some(last_claim_time) = env.storage().persistent().get::<_, u64>(&last_claim_key) {
            if last_claim_time >= schedule.next_payout_time {
                panic!("already claimed");
            }
        }

        let balance = Self::balance(env.clone(), claimant.clone());
        if balance == 0 {
            panic!("no tokens held");
        }

        // Calculate pro-rata amount: (balance * amount_per_token) / multiplier
        let gross_amount = (balance.checked_mul(schedule.amount_per_token).expect("overflow")) / 10_000_000;
        
        // 2% platform fee (stubbed)
        let fee = gross_amount * 2 / 100;
        let final_amount = gross_amount - fee;

        // Perform payout (Stubbed for this phase as it requires payout asset contract interaction)
        // In reality, this would be: token_client.transfer(&env.current_contract_address(), &claimant, &final_amount);

        // Record claim
        env.storage().persistent().set(&last_claim_key, &now);

        env.events().publish(
            (Symbol::new(&env, "dividend_claimed"), asset_id, claimant),
            final_amount,
        );
    }

    pub fn get_dividend_info(env: Env, asset_id: u64) -> Option<DividendSchedule> {
        env.storage().persistent().get(&DataKey::DividendSchedule(asset_id))
    }

    /// Get valuation history.
    pub fn get_valuation_history(env: Env) -> soroban_sdk::Vec<ValuationRecord> {
        env.storage().persistent().get(&DataKey::ValuationHistory).unwrap_or(soroban_sdk::Vec::new(&env))
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

    #[test]
    fn test_valuation_flow() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);

        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);
        
        // Set oracle
        at_client.set_oracle(&oracle);
        
        // Admin update
        at_client.update_valuation(&admin, &1000);
        let val = at_client.get_valuation().unwrap();
        assert_eq!(val.value, 1000);
        
        // Oracle update
        at_client.update_valuation(&oracle, &1100);
        let val = at_client.get_valuation().unwrap();
        assert_eq!(val.value, 1100);
        
        // Check history
        let history = at_client.get_valuation_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().value, 1000);
        assert_eq!(history.get(1).unwrap().value, 1100);
    }

    #[test]
    #[should_panic(expected = "not authorized")]
    fn test_valuation_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);
        let intruder = Address::generate(&env);

        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);
        at_client.update_valuation(&intruder, &1000);
    }

    #[test]
    #[should_panic(expected = "too frequent update")]
    fn test_valuation_interval_enforcement() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);

        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);
        at_client.set_valuation_config(&3600); // 1 hour

        at_client.update_valuation(&admin, &1000);
        
        // Immediate update should fail
        at_client.update_valuation(&admin, &1100);
    }

    #[test]
    fn test_valuation_interval_success_after_wait() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);

        at_client.initialize(&admin, &String::from_str(&env, "T"), &String::from_str(&env, "T"), &7);
        at_client.set_valuation_config(&3600); // 1 hour

        at_client.update_valuation(&admin, &1000);
        
        at_client.update_valuation(&admin, &1600);
        assert_eq!(at_client.get_valuation().unwrap().value, 1600);
    }

    #[test]
    fn test_dividend_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let admin = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let payout_asset = Address::generate(&env);

        at_client.initialize(&admin, &String::from_str(&env, "Token"), &String::from_str(&env, "TKN"), &7);

        // Mint: user1 (600), user2 (400)
        at_client.mint(&user1, &600, &1, &Address::generate(&env));
        at_client.mint(&user2, &400, &1, &Address::generate(&env));

        // Schedule: 100M units total. interval 3600
        at_client.schedule_dividend(&1, &100_000_000, &payout_asset, &3600);

        // Advance time 3601s
        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        // user1 claims: (600 * 100k) - 2% fee = 60M - 1.2M = 58.8M
        at_client.claim_dividend(&1, &user1);

        // user2 claims: (400 * 100k) - 2% fee = 40M - 0.8M = 39.2M
        at_client.claim_dividend(&1, &user2);

        // Attempt to claim again - should fail
        let res = env.try_invoke_contract::<soroban_sdk::Val, soroban_sdk::Error>(
            &at_id,
            &Symbol::new(&env, "claim_dividend"),
            (1u64, user1.clone()).into_val(&env),
        );
        assert!(res.is_err());
    }

    #[test]
    #[should_panic(expected = "payout not yet due")]
    fn test_claim_too_early() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let payout_asset = Address::generate(&env);

        at_client.initialize(&admin, &String::from_str(&env, "Token"), &String::from_str(&env, "TKN"), &7);
        at_client.mint(&user, &100, &1, &Address::generate(&env));

        at_client.schedule_dividend(&1, &1_000_000, &payout_asset, &3600);
        
        // Claim immediately (0s passed)
        at_client.claim_dividend(&1, &user);
    }
}
