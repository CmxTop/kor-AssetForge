use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec, Map};
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, String, Symbol,
};


#[derive(Clone)]
#[contracttype]
pub struct FractionalMintedEvent {
    pub asset_id: u64,
    pub total_fractions: u64,
    pub unit_value: i128,
    pub issuer: Address,
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

// --- Cross-Chain Bridging Types ---

/// Supported target chains for bridging. Extend as new chains are integrated.
#[derive(Clone, PartialEq, Debug)]
#[contracttype]
pub enum TargetChain {
    Ethereum,
    Solana,
}

/// Status of a pending bridge operation.
#[derive(Clone, PartialEq, Debug)]
#[contracttype]
pub enum BridgeStatus {
    Pending,
    Completed,
    Failed,
}

/// Tracks a single bridge operation (outbound or inbound).
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

/// Admin-configurable bridging parameters.
#[derive(Clone)]
#[contracttype]
pub struct BridgeConfig {
    pub fee_bps: u32,               // Fee in basis points (e.g., 30 = 0.30%)
    pub relayer_pool: Address,      // Address to receive bridging fees
    pub bridge_timeout: u64,        // Seconds before a pending bridge expires
    pub max_pending_per_user: u32,  // Rate limit: max concurrent pending bridges per user
    pub paused: bool,               // Emergency pause for bridging
    pub relayer_pubkey: BytesN<32>, // Ed25519 public key for proof verification (stub: mock relayer)
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
        
    pub fn initialize(env: Env, admin: Address, name: String, symbol: String, decimals: u32) {
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
    pub fn mint(env: Env, to: Address, amount: i128, asset_id: u64, emergency_control_id: Address) {
        if amount <= 0 {
            panic!("amount must be positive");
        }

        // Only admin can mint
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();

        // Enforce pause check for minting operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Minting);

        // Update balance
        let balance = Self::balance(env.clone(), to.clone());
        let new_balance = balance.checked_add(amount).expect("balance overflow");
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &new_balance);

        // Update total supply
        let total_supply = Self::total_supply(env.clone());
        let new_total_supply = total_supply.checked_add(amount).expect("supply overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total_supply);

        // Emit Mint event
        env.events()
            .publish((Symbol::new(&env, "mint"), to), amount);
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
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &new_from_balance);

        // Update recipient balance
        let to_balance = Self::balance(env.clone(), to.clone());
        let new_to_balance = to_balance.checked_add(amount).expect("balance overflow");
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &new_to_balance);

        // Emit Transfer event
        env.events()
            .publish((Symbol::new(&env, "transfer"), from, to), amount);
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    pub fn name(env: Env) -> String {
        let asset: Asset = env
            .storage()
            .instance()
            .get(&DataKey::AssetInfo)
            .expect("asset not initialized");
        asset.name
    }

    pub fn symbol(env: Env) -> String {
        let asset: Asset = env
            .storage()
            .instance()
            .get(&DataKey::AssetInfo)
            .expect("asset not initialized");
        asset.symbol
    }

    pub fn decimals(env: Env) -> u32 {
        let asset: Asset = env
            .storage()
            .instance()
            .get(&DataKey::AssetInfo)
            .expect("asset not initialized");
        asset.decimals
    }

    /// Set the trusted oracle address. (Admin only)
    pub fn set_oracle(env: Env, oracle: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        env.storage().instance().set(&DataKey::Oracle, &oracle);
    }

    /// Set the minimum time interval between updates. (Admin only)
    pub fn set_valuation_config(env: Env, min_interval: u64) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        let config = ValuationConfig { min_interval };
        env.storage()
            .instance()
            .set(&DataKey::ValuationConfig, &config);
    }

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
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        let oracle: Option<Address> = env.storage().instance().get(&DataKey::Oracle);

        let is_admin = updater == admin;
        let is_oracle = if let Some(o) = oracle {
            updater == o
        } else {
            false
        };

        if !is_admin && !is_oracle {
            panic!("not authorized");
        }

        // Enforce interval
        let now = env.ledger().timestamp();
        let config: ValuationConfig = env
            .storage()
            .instance()
            .get(&DataKey::ValuationConfig)
            .unwrap_or(ValuationConfig { min_interval: 0 });

        if let Some(last) = env
            .storage()
            .instance()
            .get::<_, ValuationRecord>(&DataKey::Valuation)
        {
            if now < last.timestamp + config.min_interval {
                panic!("too frequent update");
            }
        }

        let record = ValuationRecord {
            value: new_value,
            timestamp: now,
        };
        env.storage().instance().set(&DataKey::Valuation, &record);

        // Store in history
        let mut history: soroban_sdk::Vec<ValuationRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::ValuationHistory)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        history.push_back(record.clone());
        env.storage()
            .persistent()
            .set(&DataKey::ValuationHistory, &history);

        // Emit valuation event
        env.events()
            .publish((Symbol::new(&env, "valuation_updated"),), new_value);
    }

    pub fn schedule_dividend(
        env: Env,
        asset_id: u64,
        total_dividend: i128,
        payout_asset: Address,
        interval: u64,
    ) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();

        if total_dividend <= 0 {
            panic!("dividend amount must be positive");
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
        let now = env.ledger().timestamp();

        // Calculate amount per token scaled for precision (e.g., 7 decimals)
        // amount_per_token = (total_dividend * multiplier) / total_supply
        let amount_per_token =
            (total_dividend.checked_mul(10_000_000).expect("overflow")) / total_supply;

        let schedule = DividendSchedule {
            total_dividend,
            payout_asset,
            next_payout_time: now + interval,
            interval,
            amount_per_token,
        };

        env.storage()
            .persistent()
            .set(&DataKey::DividendSchedule(asset_id), &schedule);

        env.events().publish(
            (Symbol::new(&env, "dividend_scheduled"), asset_id),
            total_dividend,
        );
    }

    pub fn claim_dividend(env: Env, asset_id: u64, claimant: Address) {
        claimant.require_auth();

        let schedule: DividendSchedule = env
            .storage()
            .persistent()
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
        let gross_amount = (balance
            .checked_mul(schedule.amount_per_token)
            .expect("overflow"))
            / 10_000_000;

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
        env.storage()
            .persistent()
            .get(&DataKey::DividendSchedule(asset_id))
    }

    /// Get valuation history.
    pub fn get_valuation_history(env: Env) -> soroban_sdk::Vec<ValuationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::ValuationHistory)
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    // =========================================================================
    // Cross-Chain Bridging Stub
    // Prepares for real bridge integration (e.g., Wormhole). Replace mock proof
    // validation with actual relayer verification in production.
    // =========================================================================

    /// Configure bridging parameters. Admin only.
    pub fn set_bridge_config(
        env: Env,
        fee_bps: u32,
        relayer_pool: Address,
        bridge_timeout: u64,
        max_pending_per_user: u32,
        relayer_pubkey: BytesN<32>,
    ) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();

        let config = BridgeConfig {
            fee_bps,
            relayer_pool,
            bridge_timeout,
            max_pending_per_user,
            paused: false,
            relayer_pubkey,
        };
        env.storage()
            .instance()
            .set(&DataKey::BridgeConfig, &config);

        env.events()
            .publish((Symbol::new(&env, "bridge_configured"),), fee_bps);
    }

    /// Pause or unpause bridging. Admin only.
    pub fn set_bridge_paused(env: Env, paused: bool) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();

        let mut config: BridgeConfig = env
            .storage()
            .instance()
            .get(&DataKey::BridgeConfig)
            .expect("bridge not configured");
        config.paused = paused;
        env.storage()
            .instance()
            .set(&DataKey::BridgeConfig, &config);

        env.events()
            .publish((Symbol::new(&env, "bridge_paused"),), paused);
    }

    /// Initiate an outbound bridge: burn tokens on Stellar, emit event for relayers.
    /// Deducts a fee (in basis points) and tracks the pending bridge with a timeout.
    pub fn bridge_out(
        env: Env,
        caller: Address,
        asset_id: Address,
        amount: i128,
        target_chain: TargetChain,
        target_address: Bytes,
    ) -> BytesN<32> {
        caller.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }

        let config: BridgeConfig = env
            .storage()
            .instance()
            .get(&DataKey::BridgeConfig)
            .expect("bridge not configured");

        if config.paused {
            panic!("bridge is paused");
        }

        // Rate limit check
        let pending_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::UserPendingCount(caller.clone()))
            .unwrap_or(0);
        if pending_count >= config.max_pending_per_user {
            panic!("too many pending bridges");
        }

        // Validate caller has sufficient balance
        let balance = Self::balance(env.clone(), caller.clone());
        if balance < amount {
            panic!("insufficient balance");
        }

        // Calculate fee
        let fee = amount
            .checked_mul(config.fee_bps as i128)
            .expect("fee overflow")
            / 10_000;
        let net_amount = amount.checked_sub(fee).expect("fee underflow");

        // Burn the full amount from caller's balance
        let new_balance = balance.checked_sub(amount).expect("balance underflow");
        env.storage()
            .persistent()
            .set(&DataKey::Balance(caller.clone()), &new_balance);

        // Reduce total supply (burn)
        let total_supply = Self::total_supply(env.clone());
        let new_total_supply = total_supply.checked_sub(amount).expect("supply underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total_supply);

        // Credit fee to relayer pool
        let pool_balance = Self::balance(env.clone(), config.relayer_pool.clone());
        let new_pool_balance = pool_balance
            .checked_add(fee)
            .expect("pool balance overflow");
        env.storage().persistent().set(
            &DataKey::Balance(config.relayer_pool.clone()),
            &new_pool_balance,
        );

        // Adjust total supply: fee tokens are re-minted to relayer pool
        let adjusted_supply = new_total_supply.checked_add(fee).expect("supply overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &adjusted_supply);

        // Generate unique bridge ID from nonce
        let nonce: u64 = env
            .storage()
            .instance()
            .get(&DataKey::BridgeNonce)
            .unwrap_or(0);
        let next_nonce = nonce.checked_add(1).expect("nonce overflow");
        env.storage()
            .instance()
            .set(&DataKey::BridgeNonce, &next_nonce);

        // Create bridge ID by hashing nonce bytes
        let mut nonce_bytes = [0u8; 32];
        let nonce_le = nonce.to_le_bytes();
        nonce_bytes[..8].copy_from_slice(&nonce_le);
        let bridge_id = BytesN::from_array(&env, &nonce_bytes);

        let now = env.ledger().timestamp();
        let pending = PendingBridge {
            caller: caller.clone(),
            asset_id: asset_id.clone(),
            amount: net_amount,
            fee,
            target_chain: target_chain.clone(),
            target_address: target_address.clone(),
            timeout: now + config.bridge_timeout,
            status: BridgeStatus::Pending,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PendingBridge(bridge_id.clone()), &pending);

        // Increment user pending count
        env.storage().persistent().set(
            &DataKey::UserPendingCount(caller.clone()),
            &(pending_count + 1),
        );

        env.events().publish(
            (Symbol::new(&env, "bridge_initiated"), caller, target_chain),
            (asset_id, net_amount, target_address),
        );

        bridge_id
    }

    /// Complete an inbound bridge: verify authorization, mint tokens on Stellar.
    ///
    /// Stub: Uses admin-only authorization as a mock for relayer proof verification.
    /// Production: Replace with Ed25519 multi-sig verification from bridge relayers, e.g.:
    ///   `env.crypto().ed25519_verify(&config.relayer_pubkey, &msg, &proof);`
    pub fn bridge_in(
        env: Env,
        bridge_id: BytesN<32>,
        recipient: Address,
        asset_id: Address,
        amount: i128,
        source_chain: TargetChain,
    ) {
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let config: BridgeConfig = env
            .storage()
            .instance()
            .get(&DataKey::BridgeConfig)
            .expect("bridge not configured");

        if config.paused {
            panic!("bridge is paused");
        }

        // Stub: admin authorization acts as mock relayer proof.
        // In production, replace with real Ed25519 multi-sig verification:
        //   let msg = Bytes::from_array(&env, &bridge_id.to_array());
        //   env.crypto().ed25519_verify(&config.relayer_pubkey, &msg, &proof);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();

        // Mint tokens to recipient
        let balance = Self::balance(env.clone(), recipient.clone());
        let new_balance = balance.checked_add(amount).expect("balance overflow");
        env.storage()
            .persistent()
            .set(&DataKey::Balance(recipient.clone()), &new_balance);

        let total_supply = Self::total_supply(env.clone());
        let new_total_supply = total_supply.checked_add(amount).expect("supply overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total_supply);

        // Update bridge record if it exists (for outbound-return flows)
        if let Some(mut pending) = env
            .storage()
            .persistent()
            .get::<_, PendingBridge>(&DataKey::PendingBridge(bridge_id.clone()))
        {
            pending.status = BridgeStatus::Completed;
            env.storage()
                .persistent()
                .set(&DataKey::PendingBridge(bridge_id.clone()), &pending);

            // Decrement user pending count
            let count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::UserPendingCount(pending.caller.clone()))
                .unwrap_or(1);
            let new_count = if count > 0 { count - 1 } else { 0 };
            env.storage()
                .persistent()
                .set(&DataKey::UserPendingCount(pending.caller), &new_count);
        }

        env.events().publish(
            (
                Symbol::new(&env, "bridge_completed"),
                recipient,
                source_chain,
            ),
            (asset_id, amount),
        );
    }

    /// Query a pending bridge by its ID.
    pub fn get_pending_bridge(env: Env, bridge_id: BytesN<32>) -> Option<PendingBridge> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingBridge(bridge_id))
    }

    /// Expire a timed-out bridge, marking it as Failed. Anyone can call this
    /// after the timeout to clean up stuck bridges.
    pub fn expire_bridge(env: Env, bridge_id: BytesN<32>) {
        let mut pending: PendingBridge = env
            .storage()
            .persistent()
            .get(&DataKey::PendingBridge(bridge_id.clone()))
            .expect("bridge not found");

        if pending.status != BridgeStatus::Pending {
            panic!("bridge not pending");
        }

        let now = env.ledger().timestamp();
        if now < pending.timeout {
            panic!("bridge not expired");
        }

        pending.status = BridgeStatus::Failed;
        env.storage()
            .persistent()
            .set(&DataKey::PendingBridge(bridge_id), &pending);

        // Decrement user pending count
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::UserPendingCount(pending.caller.clone()))
            .unwrap_or(1);
        let new_count = if count > 0 { count - 1 } else { 0 };
        env.storage()
            .persistent()
            .set(&DataKey::UserPendingCount(pending.caller), &new_count);

        env.events()
            .publish((Symbol::new(&env, "bridge_expired"),), pending.amount);
    }

    /// Get the current bridge configuration.
    pub fn get_bridge_config(env: Env) -> Option<BridgeConfig> {
        env.storage().instance().get(&DataKey::BridgeConfig)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::emergency_control::EmergencyControl;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::IntoVal;

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
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

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
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

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
        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        at_client.mint(&user1, &100, &1, &ec_id);
        at_client.transfer(&user1, &user2, &101, &1, &ec_id);
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
        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

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
        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

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
        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let from = Address::generate(&env);
        let to = Address::generate(&env);

        // Mint tokens BEFORE pausing
        at_client.mint(&from, &1000, &1, &ec_id);

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
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

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
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

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
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

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

        let _ = client.mint_fractional(&admin, &100_000i128, &1000u64, &None);

        // Try to fractionaliz again - should panic
        let _ = client.mint_fractional(&admin, &200_000i128, &2000u64, &None);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );
        at_client.update_valuation(&intruder, &1000);
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
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );
        at_client.set_valuation_config(&3600); // 1 hour

        at_client.update_valuation(&admin, &1000);

        // Immediate update should fail
        at_client.update_valuation(&admin, &1100);
    }

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
        at_client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );
        at_client.set_valuation_config(&3600); // 1 hour

        at_client.update_valuation(&admin, &1000);

        at_client.update_valuation(&admin, &1600);
        assert_eq!(at_client.get_valuation().unwrap().value, 1600);
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
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let payout_asset = Address::generate(&env);

        at_client.initialize(
            &admin,
            &String::from_str(&env, "Token"),
            &String::from_str(&env, "TKN"),
            &7,
        );

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
        at_client.initialize(
            &admin,
            &String::from_str(&env, "Token"),
            &String::from_str(&env, "TKN"),
            &7,
        );
        at_client.mint(&user, &100, &1, &Address::generate(&env));

        at_client.schedule_dividend(&1, &1_000_000, &payout_asset, &3600);

        // Claim immediately (0s passed)
        at_client.claim_dividend(&1, &user);
    }

    // =========================================================================
    // Cross-Chain Bridging Tests
    // =========================================================================

    #[test]
    fn test_bridge_config_setup() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        let config = client.get_bridge_config().unwrap();
        assert_eq!(config.fee_bps, 30);
        assert_eq!(config.bridge_timeout, 3600);
        assert_eq!(config.max_pending_per_user, 5);
        assert!(!config.paused);
    }

    #[test]
    fn test_bridge_out_burns_tokens_and_deducts_fee() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let user = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        // Mint 10,000 tokens to user
        client.mint(&user, &10_000, &1, &ec_id);
        assert_eq!(client.total_supply(), 10_000);

        // Configure bridge: 30 bps (0.30%)
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        let asset_id = Address::generate(&env);
        let target_addr = Bytes::from_array(&env, &[0xABu8; 20]);

        // Bridge out 10,000 tokens
        let bridge_id = client.bridge_out(
            &user,
            &asset_id,
            &10_000,
            &TargetChain::Ethereum,
            &target_addr,
        );

        // Fee = 10000 * 30 / 10000 = 30
        // Net burn = 10000 - 30 = 9970 (net amount bridged)
        // User balance: 0 (10000 burned)
        assert_eq!(client.balance(&user), 0);
        // Relayer pool gets fee: 30
        assert_eq!(client.balance(&pool), 30);
        // Total supply = 10000 - 10000 + 30 = 30 (burned net, fee re-minted to pool)
        assert_eq!(client.total_supply(), 30);

        // Verify pending bridge record
        let pending = client.get_pending_bridge(&bridge_id).unwrap();
        assert_eq!(pending.amount, 9970); // net amount
        assert_eq!(pending.fee, 30);
        assert_eq!(pending.status, BridgeStatus::Pending);
        assert_eq!(pending.target_chain, TargetChain::Ethereum);
    }

    #[test]
    fn test_bridge_in_mints_tokens_with_admin_auth() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        let recipient = Address::generate(&env);
        let asset_id = Address::generate(&env);
        let bridge_id = BytesN::from_array(&env, &[0u8; 32]);

        // Bridge in 5000 tokens (admin auth mocked)
        client.bridge_in(
            &bridge_id,
            &recipient,
            &asset_id,
            &5000,
            &TargetChain::Solana,
        );

        assert_eq!(client.balance(&recipient), 5000);
        assert_eq!(client.total_supply(), 5000);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_bridge_in_rejects_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let pool = Address::generate(&env);
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        client.bridge_in(
            &BytesN::from_array(&env, &[0u8; 32]),
            &Address::generate(&env),
            &Address::generate(&env),
            &0,
            &TargetChain::Ethereum,
        );
    }

    #[test]
    #[should_panic(expected = "bridge is paused")]
    fn test_bridge_out_blocked_when_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let user = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        client.mint(&user, &10_000, &1, &ec_id);
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        // Pause bridging
        client.set_bridge_paused(&true);

        client.bridge_out(
            &user,
            &Address::generate(&env),
            &1000,
            &TargetChain::Ethereum,
            &Bytes::from_array(&env, &[0xABu8; 20]),
        );
    }

    #[test]
    #[should_panic(expected = "bridge is paused")]
    fn test_bridge_in_blocked_when_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let pool = Address::generate(&env);
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);
        client.set_bridge_paused(&true);

        client.bridge_in(
            &BytesN::from_array(&env, &[0u8; 32]),
            &Address::generate(&env),
            &Address::generate(&env),
            &1000,
            &TargetChain::Ethereum,
        );
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_bridge_out_insufficient_balance() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let user = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        // User has 0 balance, trying to bridge out 1000
        client.bridge_out(
            &user,
            &Address::generate(&env),
            &1000,
            &TargetChain::Ethereum,
            &Bytes::from_array(&env, &[0xABu8; 20]),
        );
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_bridge_out_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        client.bridge_out(
            &Address::generate(&env),
            &Address::generate(&env),
            &0,
            &TargetChain::Solana,
            &Bytes::from_array(&env, &[0u8; 20]),
        );
    }

    #[test]
    #[should_panic(expected = "too many pending bridges")]
    fn test_bridge_out_rate_limit() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let user = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        // Mint enough tokens
        client.mint(&user, &1_000_000, &1, &ec_id);

        // Set max_pending_per_user = 2
        client.set_bridge_config(&30, &pool, &3600, &2, &pubkey);

        let asset_id = Address::generate(&env);
        let target = Bytes::from_array(&env, &[0xABu8; 20]);

        // First two should succeed
        client.bridge_out(&user, &asset_id, &100, &TargetChain::Ethereum, &target);
        client.bridge_out(&user, &asset_id, &100, &TargetChain::Solana, &target);

        // Third should fail: rate limit exceeded
        client.bridge_out(&user, &asset_id, &100, &TargetChain::Ethereum, &target);
    }

    #[test]
    fn test_bridge_expire_after_timeout() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let user = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        client.mint(&user, &10_000, &1, &ec_id);
        // Timeout = 100 seconds
        client.set_bridge_config(&30, &pool, &100, &5, &pubkey);

        let bridge_id = client.bridge_out(
            &user,
            &Address::generate(&env),
            &1000,
            &TargetChain::Ethereum,
            &Bytes::from_array(&env, &[0xABu8; 20]),
        );

        // Advance time past timeout
        env.ledger().with_mut(|li| {
            li.timestamp += 200;
        });

        client.expire_bridge(&bridge_id);

        let bridge = client.get_pending_bridge(&bridge_id).unwrap();
        assert_eq!(bridge.status, BridgeStatus::Failed);
    }

    #[test]
    #[should_panic(expected = "bridge not expired")]
    fn test_bridge_expire_before_timeout_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let user = Address::generate(&env);
        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        client.mint(&user, &10_000, &1, &ec_id);
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        let bridge_id = client.bridge_out(
            &user,
            &Address::generate(&env),
            &1000,
            &TargetChain::Ethereum,
            &Bytes::from_array(&env, &[0xABu8; 20]),
        );

        // Try to expire immediately (not timed out yet)
        client.expire_bridge(&bridge_id);
    }

    #[test]
    fn test_bridge_pause_and_unpause() {
        let env = Env::default();
        env.mock_all_auths();

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        let admin = Address::generate(&env);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);
        client.set_bridge_config(&30, &pool, &3600, &5, &pubkey);

        // Initially not paused
        assert!(!client.get_bridge_config().unwrap().paused);

        // Pause
        client.set_bridge_paused(&true);
        assert!(client.get_bridge_config().unwrap().paused);

        // Unpause
        client.set_bridge_paused(&false);
        assert!(!client.get_bridge_config().unwrap().paused);
    }

    #[test]
    fn test_bridge_in_completes_pending_bridge() {
        let env = Env::default();
        env.mock_all_auths();

        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let at_id = env.register_contract(None, AssetToken);
        let client = AssetTokenClient::new(&env, &at_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
        );

        let pool = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &[1u8; 32]);

        let user = Address::generate(&env);
        client.mint(&user, &10_000, &1, &ec_id);
        client.set_bridge_config(&0, &pool, &3600, &5, &pubkey); // 0 fee for simplicity

        // Bridge out
        let asset_id = Address::generate(&env);
        let bridge_id = client.bridge_out(
            &user,
            &asset_id,
            &5000,
            &TargetChain::Ethereum,
            &Bytes::from_array(&env, &[0xABu8; 20]),
        );

        assert_eq!(
            client.get_pending_bridge(&bridge_id).unwrap().status,
            BridgeStatus::Pending
        );

        // Bridge in (completing the round-trip, admin auth mocked)
        let recipient = Address::generate(&env);
        client.bridge_in(
            &bridge_id,
            &recipient,
            &asset_id,
            &5000,
            &TargetChain::Ethereum,
        );

        // Bridge marked completed
        assert_eq!(
            client.get_pending_bridge(&bridge_id).unwrap().status,
            BridgeStatus::Completed
        );
        assert_eq!(client.balance(&recipient), 5000);
    }
}
