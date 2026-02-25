use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

use crate::emergency_control::{EmergencyControlClient, PauseScope};

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
    // Cross-chain bridging keys
    BridgeConfig,
    PendingBridge(BytesN<32>), // bridge_id -> PendingBridge
    UserPendingCount(Address), // rate limit tracker per user
    BridgeNonce,               // monotonic nonce for unique bridge IDs
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

#[derive(Clone)]
#[contracttype]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
    pub owner: Address,
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

#[derive(Clone, PartialEq, Debug)]
#[contracttype]
pub enum TargetChain {
    Ethereum,
    Solana,
}

#[derive(Clone, PartialEq, Debug)]
#[contracttype]
pub enum BridgeStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Clone)]
#[contracttype]
pub struct PendingBridge {
    pub caller: Address,
    pub asset_id: Address,
    pub amount: i128,
    pub fee: i128,
    pub target_chain: TargetChain,
    pub target_address: Bytes,
    pub timeout: u64,
    pub status: BridgeStatus,
}

#[derive(Clone)]
#[contracttype]
pub struct BridgeConfig {
    pub fee_bps: u32,
    pub relayer_pool: Address,
    pub bridge_timeout: u64,
    pub max_pending_per_user: u32,
    pub paused: bool,
    pub relayer_pubkey: BytesN<32>,
}

#[contract]
pub struct AssetToken;

#[contractimpl]
impl AssetToken {
    pub fn initialize(env: Env, admin: Address, name: String, symbol: String, decimals: u32) {
        if env.storage().instance().has(&DataKey::AssetInfo) {
            panic!("already initialized");
        }
        admin.require_auth();

        let asset = Asset {
            id: 1,
            name,
            symbol,
            decimals,
            owner: admin.clone(),
            is_fractionalized: false,
            total_fractions: 0,
            unit_value: 0,
        };
        env.storage().instance().set(&DataKey::AssetInfo, &asset);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);
    }

    pub fn mint_fractional(
        env: Env,
        admin: Address,
        total_value: i128,
        fractions: u64,
        initial_owners: Option<Vec<(Address, u64)>>,
    ) -> u64 {
        admin.require_auth();
        let mut asset: Asset = env.storage().instance().get(&DataKey::AssetInfo).expect("Asset not initialized");
        assert_eq!(asset.owner, admin, "not owner");
        assert!(!asset.is_fractionalized, "already fractionalized");
        assert!(fractions > 0, "fractions must be > 0");
        assert_eq!(total_value % (fractions as i128), 0, "uneven division");

        let unit_value = total_value / (fractions as i128);
        let mut total_distributed: u64 = 0;

        if let Some(owners) = initial_owners {
            for (owner_addr, share_count) in owners.iter() {
                total_distributed += share_count;
                assert!(total_distributed <= fractions, "exceeds fractions");
                let balance = (share_count as i128) * unit_value;
                let current: i128 = env.storage().persistent().get(&DataKey::Balance(owner_addr.clone())).unwrap_or(0);
                env.storage().persistent().set(&DataKey::Balance(owner_addr), &(current + balance));
            }
        }

        asset.is_fractionalized = true;
        asset.total_fractions = fractions;
        asset.unit_value = unit_value;

        env.storage().instance().set(&DataKey::AssetInfo, &asset);
        env.storage().instance().set(&DataKey::TotalSupply, &total_value);

        env.events().publish((Symbol::new(&env, "fractions_minted"),), (asset.id, fractions, unit_value));
        asset.id
    }

    pub fn mint(env: Env, to: Address, amount: i128, asset_id: u64, emergency_control_id: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();

        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Minting);

        let balance = Self::balance(env.clone(), to.clone());
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &(balance + amount));

        let supply = Self::total_supply(env.clone());
        env.storage().instance().set(&DataKey::TotalSupply, &(supply + amount));

        env.events().publish((Symbol::new(&env, "mint"), to), amount);
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128, asset_id: u64, emergency_control_id: Address) {
        from.require_auth();
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Transfers);

        let from_balance = Self::balance(env.clone(), from.clone());
        assert!(from_balance >= amount, "insufficient balance");
        
        env.storage().persistent().set(&DataKey::Balance(from.clone()), &(from_balance - amount));
        let to_balance = Self::balance(env.clone(), to.clone());
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &(to_balance + amount));

        env.events().publish((Symbol::new(&env, "transfer"), from, to), amount);
    }

    pub fn balance(env: Env, address: Address) -> i128 {
        env.storage().persistent().get(&DataKey::Balance(address)).unwrap_or(0)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::TotalSupply).unwrap_or(0)
    }

    pub fn name(env: Env) -> String {
        env.storage().instance().get::<_, Asset>(&DataKey::AssetInfo).expect("not initialized").name
    }

    pub fn symbol(env: Env) -> String {
        env.storage().instance().get::<_, Asset>(&DataKey::AssetInfo).expect("not initialized").symbol
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage().instance().get::<_, Asset>(&DataKey::AssetInfo).expect("not initialized").decimals
    }

    pub fn get_asset(env: Env) -> Option<Asset> {
        env.storage().instance().get(&DataKey::AssetInfo)
    }

    pub fn set_oracle(env: Env, oracle: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        env.storage().instance().set(&DataKey::Oracle, &oracle);
    }

    pub fn set_valuation_config(env: Env, min_interval: u64) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        env.storage().instance().set(&DataKey::ValuationConfig, &ValuationConfig { min_interval });
    }

    pub fn update_valuation(env: Env, updater: Address, new_value: i128) {
        updater.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        let oracle: Option<Address> = env.storage().instance().get(&DataKey::Oracle);
        
        if updater != admin && (oracle.is_none() || updater != oracle.unwrap()) {
            panic!("not authorized");
        }

        let now = env.ledger().timestamp();
        let config: ValuationConfig = env.storage().instance().get(&DataKey::ValuationConfig).unwrap_or(ValuationConfig { min_interval: 0 });

        if let Some(last) = env.storage().instance().get::<_, ValuationRecord>(&DataKey::Valuation) {
            if now < last.timestamp + config.min_interval {
                panic!("too frequent update");
            }
        }

        let record = ValuationRecord { value: new_value, timestamp: now };
        env.storage().instance().set(&DataKey::Valuation, &record);

        let mut history: Vec<ValuationRecord> = env.storage().persistent().get(&DataKey::ValuationHistory).unwrap_or(Vec::new(&env));
        history.push_back(record);
        env.storage().persistent().set(&DataKey::ValuationHistory, &history);

        env.events().publish((Symbol::new(&env, "valuation_updated"),), new_value);
    }

    pub fn schedule_dividend(env: Env, asset_id: u64, total_dividend: i128, payout_asset: Address, interval: u64) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        assert!(total_dividend > 0, "dividend must be positive");

        let supply = Self::total_supply(env.clone());
        assert!(supply > 0, "no supply");

        let amount_per_token = (total_dividend * 10_000_000) / supply;
        let schedule = DividendSchedule {
            total_dividend,
            payout_asset,
            next_payout_time: env.ledger().timestamp() + interval,
            interval,
            amount_per_token,
        };

        env.storage().persistent().set(&DataKey::DividendSchedule(asset_id), &schedule);
        env.events().publish((Symbol::new(&env, "dividend_scheduled"), asset_id), total_dividend);
    }

    pub fn claim_dividend(env: Env, asset_id: u64, claimant: Address) {
        claimant.require_auth();
        let schedule: DividendSchedule = env.storage().persistent().get(&DataKey::DividendSchedule(asset_id)).expect("no schedule");
        let now = env.ledger().timestamp();
        assert!(now >= schedule.next_payout_time, "not due");

        let last_claim_key = DataKey::LastClaim(asset_id, claimant.clone());
        if let Some(last) = env.storage().persistent().get::<_, u64>(&last_claim_key) {
            assert!(last < schedule.next_payout_time, "already claimed");
        }

        let balance = Self::balance(env.clone(), claimant.clone());
        assert!(balance > 0, "no tokens");

        let amount = (balance * schedule.amount_per_token) / 10_000_000;
        env.storage().persistent().set(&last_claim_key, &now);
        env.events().publish((Symbol::new(&env, "dividend_claimed"), asset_id, claimant), amount);
    }

    pub fn get_dividend_info(env: Env, asset_id: u64) -> Option<DividendSchedule> {
        env.storage().persistent().get(&DataKey::DividendSchedule(asset_id))
    }

    pub fn get_valuation_history(env: Env) -> Vec<ValuationRecord> {
        env.storage().persistent().get(&DataKey::ValuationHistory).unwrap_or(Vec::new(&env))
    }

    pub fn set_bridge_config(env: Env, fee_bps: u32, relayer_pool: Address, bridge_timeout: u64, max_pending_per_user: u32, relayer_pubkey: BytesN<32>) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        let config = BridgeConfig { fee_bps, relayer_pool, bridge_timeout, max_pending_per_user, paused: false, relayer_pubkey };
        env.storage().instance().set(&DataKey::BridgeConfig, &config);
    }

    pub fn set_bridge_paused(env: Env, paused: bool) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        let mut config: BridgeConfig = env.storage().instance().get(&DataKey::BridgeConfig).expect("not configured");
        config.paused = paused;
        env.storage().instance().set(&DataKey::BridgeConfig, &config);
    }

    pub fn bridge_out(env: Env, caller: Address, asset_id: Address, amount: i128, target_chain: TargetChain, target_address: Bytes) -> BytesN<32> {
        caller.require_auth();
        let config: BridgeConfig = env.storage().instance().get(&DataKey::BridgeConfig).expect("not configured");
        assert!(!config.paused, "paused");

        let pending_count: u32 = env.storage().persistent().get(&DataKey::UserPendingCount(caller.clone())).unwrap_or(0);
        assert!(pending_count < config.max_pending_per_user, "rate limit");

        let balance = Self::balance(env.clone(), caller.clone());
        assert!(balance >= amount, "insufficient balance");

        let fee = (amount * (config.fee_bps as i128)) / 10_000;
        let net = amount - fee;

        env.storage().persistent().set(&DataKey::Balance(caller.clone()), &(balance - amount));
        let pool_bal = Self::balance(env.clone(), config.relayer_pool.clone());
        env.storage().persistent().set(&DataKey::Balance(config.relayer_pool.clone()), &(pool_bal + fee));

        let supply = Self::total_supply(env.clone());
        env.storage().instance().set(&DataKey::TotalSupply, &(supply - net));

        let nonce: u64 = env.storage().instance().get(&DataKey::BridgeNonce).unwrap_or(0);
        env.storage().instance().set(&DataKey::BridgeNonce, &(nonce + 1));

        let mut id_bytes = [0u8; 32];
        id_bytes[..8].copy_from_slice(&nonce.to_le_bytes());
        let bridge_id = BytesN::from_array(&env, &id_bytes);

        let pending = PendingBridge { caller: caller.clone(), asset_id, amount: net, fee, target_chain: target_chain.clone(), target_address, timeout: env.ledger().timestamp() + config.bridge_timeout, status: BridgeStatus::Pending };
        env.storage().persistent().set(&DataKey::PendingBridge(bridge_id.clone()), &pending);
        env.storage().persistent().set(&DataKey::UserPendingCount(caller.clone()), &(pending_count + 1));

        env.events().publish((Symbol::new(&env, "bridge_initiated"), caller, target_chain), (net, bridge_id.clone()));
        bridge_id
    }

    pub fn bridge_in(env: Env, bridge_id: BytesN<32>, recipient: Address, asset_id: Address, amount: i128, source_chain: TargetChain) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        
        let balance = Self::balance(env.clone(), recipient.clone());
        env.storage().persistent().set(&DataKey::Balance(recipient.clone()), &(balance + amount));

        let supply = Self::total_supply(env.clone());
        env.storage().instance().set(&DataKey::TotalSupply, &(supply + amount));

        if let Some(mut pending) = env.storage().persistent().get::<_, PendingBridge>(&DataKey::PendingBridge(bridge_id.clone())) {
            pending.status = BridgeStatus::Completed;
            env.storage().persistent().set(&DataKey::PendingBridge(bridge_id), &pending);
            let count: u32 = env.storage().persistent().get(&DataKey::UserPendingCount(pending.caller.clone())).unwrap_or(1);
            env.storage().persistent().set(&DataKey::UserPendingCount(pending.caller), &(if count > 0 { count - 1 } else { 0 }));
        }

        env.events().publish((Symbol::new(&env, "bridge_completed"), recipient, source_chain), (asset_id, amount));
    }

    pub fn get_pending_bridge(env: Env, bridge_id: BytesN<32>) -> Option<PendingBridge> {
        env.storage().persistent().get(&DataKey::PendingBridge(bridge_id))
    }

    pub fn expire_bridge(env: Env, bridge_id: BytesN<32>) {
        let mut pending: PendingBridge = env.storage().persistent().get(&DataKey::PendingBridge(bridge_id.clone())).expect("bridge not found");
        assert!(pending.status == BridgeStatus::Pending, "not pending");
        assert!(env.ledger().timestamp() >= pending.timeout, "not expired");

        pending.status = BridgeStatus::Failed;
        env.storage().persistent().set(&DataKey::PendingBridge(bridge_id), &pending);
        
        let count: u32 = env.storage().persistent().get(&DataKey::UserPendingCount(pending.caller.clone())).unwrap_or(1);
        env.storage().persistent().set(&DataKey::UserPendingCount(pending.caller), &(if count > 0 { count - 1 } else { 0 }));

        env.events().publish((Symbol::new(&env, "bridge_expired"),), pending.amount);
    }

    pub fn get_bridge_config(env: Env) -> Option<BridgeConfig> {
        env.storage().instance().get(&DataKey::BridgeConfig)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();
        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);


        let admin = Address::generate(&env);
        let name = String::from_str(&env, "Test Asset");
        let symbol = String::from_str(&env, "TSTA");
        client.initialize(&admin, &name, &symbol, &7);

        assert_eq!(client.name(), name);
        assert_eq!(client.symbol(), symbol);

        // Fractional mint
        let user1 = Address::generate(&env);
        let mut owners = Vec::new(&env);
        owners.push_back((user1.clone(), 100u64));
        client.mint_fractional(&admin, &100_000, &1000, &Some(owners));

        assert_eq!(client.balance(&user1), 10_000);
        assert_eq!(client.total_supply(), 100_000);

        // Valuation
        client.set_oracle(&admin);
        client.update_valuation(&admin, &110_000);
        let history = client.get_valuation_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().value, 110_000);

        // Bridge config
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);
        
        // Bridge out
        let target_addr = Bytes::from_array(&env, &[0xABu8; 20]);
        let _bridge_id = client.bridge_out(&user1, &at_id, &1000, &TargetChain::Ethereum, &target_addr);
        
        assert_eq!(client.balance(&user1), 9_000);
        assert_eq!(client.balance(&pool), 3); // 1000 * 30 / 10000 = 3
    }
}
