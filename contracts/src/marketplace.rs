use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

use crate::emergency_control::{EmergencyControlClient, PauseScope};
use crate::governance::GovernanceClient;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ComplianceError {
    InvalidTimeRange = 1,
    Unauthorized = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeRange {
    pub start_timestamp: u64,
    pub end_timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComplianceMetrics {
    pub total_value: i128,
    pub volume: i128,
    pub holders: u32,
}


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
    MarketplaceAdmin,
    AssetPrivate(u64),
    Whitelisted(u64, Address),
}

/// Storage keys for buy-back and burn system.
#[derive(Clone)]
#[contracttype]
pub enum BuyBackDataKey {
    BuyBackAdminKey,
    BuyBackConfigKey,
    TreasuryBalanceKey,
    TotalBurnedKey,
    HistoryKey,
    GovernanceContractKey,
}

/// Storage keys for the referral system.
#[derive(Clone)]
#[contracttype]
pub enum ReferralDataKey {
    ReferralConfigKey,
    ReferrerKey(Address),
    RewardBalanceKey(Address),
    ReferralCountKey(Address),
}

/// Configuration for the referral system.
#[derive(Clone)]
#[contracttype]
pub struct ReferralConfig {
    /// Reward percentage in basis points (e.g., 500 = 5% of trade fee)
    pub reward_bps: u32,
    /// Address of the treasury from which rewards are paid
    pub treasury: Address,
    /// Minimum trade activity (in native asset) requested for referral validity
    pub min_activity: i128,
}

/// Configuration for the buy-back and burn system.
#[derive(Clone)]
#[contracttype]
pub struct BuyBackConfig {
    /// Maximum tokens that can be burned in a single operation
    pub burn_cap: i128,
    /// Treasury balance threshold that triggers auto buy-back
    pub auto_threshold: i128,
    /// Amount to buy back when auto-triggered
    pub auto_buyback_amount: i128,
    /// Whether governance approval is required for burns
    pub require_governance: bool,
    /// Fee percentage (basis points) collected from marketplace trades
    pub fee_bps: u32,
    /// Whether the buy-back system is paused
    pub paused: bool,
}

/// Record of a buy-back or burn operation for audit trail.
#[derive(Clone)]
#[contracttype]
pub struct BuyBackRecord {
    /// Amount of tokens bought back
    pub amount: i128,
    /// Amount of tokens burned
    pub burned: i128,
    /// Source of funds used (treasury)
    pub source_funds: i128,
    /// Ledger timestamp when the operation occurred
    pub timestamp: u64,
    /// Address that triggered the operation
    pub executor: Address,
    /// Whether this was an auto-triggered buy-back
    pub auto_triggered: bool,
}

// ============================================================================
// CONTRACT
// ============================================================================

#[contract]
pub struct Marketplace;

#[contractimpl]
impl Marketplace {
    // -----------------------------------------------------------------------
    // Marketplace Operations (existing)
    // -----------------------------------------------------------------------

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
        amount: i128,
        asset_id: u64,
        emergency_control_id: Address,
    ) -> bool {
        buyer.require_auth();

        // Enforce pause check for trading operations
        let ec_client = EmergencyControlClient::new(&env, &emergency_control_id);
        ec_client.require_not_paused(&asset_id, &PauseScope::Trading);

        // Enforce whitelisting if asset is private
        Self::require_whitelisted_if_private(&env, asset_id, &buyer);

        // Collect fee and credit referral reward
        if env.storage().instance().has(&BuyBackDataKey::BuyBackConfigKey) {
            let fee = Self::collect_fee(env.clone(), amount);
            Self::credit_referral_reward(&env, &buyer, fee);
        }

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

    // -----------------------------------------------------------------------
    // Whitelisting – Admin & Privacy Management
    // -----------------------------------------------------------------------

    /// Initialize the marketplace with an admin
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&MarketplaceDataKey::MarketplaceAdmin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&MarketplaceDataKey::MarketplaceAdmin, &admin);
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
        let stored_admin: Address = env.storage().instance().get(&MarketplaceDataKey::MarketplaceAdmin).expect("admin not set");
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

    // -----------------------------------------------------------------------
    // Buy-Back & Burn System – Initialization
    // -----------------------------------------------------------------------

    /// Initialize the buy-back and burn system.
    ///
    /// # Arguments
    /// * `admin` – Admin address authorized for buy-back operations.
    /// * `burn_cap` – Maximum tokens that can be burned per operation.
    /// * `auto_threshold` – Treasury balance threshold to trigger auto buy-back.
    /// * `auto_buyback_amount` – Amount to buy-back when auto-triggered.
    /// * `fee_bps` – Marketplace fee in basis points (e.g., 30 = 0.30%).
    /// * `require_governance` – Whether governance approval is required for burns.
    pub fn initialize_buyback(
        env: Env,
        admin: Address,
        burn_cap: i128,
        auto_threshold: i128,
        auto_buyback_amount: i128,
        fee_bps: u32,
        require_governance: bool,
    ) {
        admin.require_auth();

        if env.storage().instance().has(&BuyBackDataKey::BuyBackAdminKey) {
            panic!("buy-back system already initialized");
        }

        if burn_cap <= 0 {
            panic!("burn cap must be positive");
        }
        if auto_threshold < 0 {
            panic!("auto threshold must be non-negative");
        }
        if auto_buyback_amount < 0 {
            panic!("auto buyback amount must be non-negative");
        }
        if fee_bps > 10_000 {
            panic!("fee basis points must not exceed 10000");
        }

        env.storage().instance().set(&BuyBackDataKey::BuyBackAdminKey, &admin);

        let config = BuyBackConfig {
            burn_cap,
            auto_threshold,
            auto_buyback_amount,
            require_governance,
            fee_bps,
            paused: false,
        };

        env.storage()
            .instance()
            .set(&BuyBackDataKey::BuyBackConfigKey, &config);
        env.storage()
            .instance()
            .set(&BuyBackDataKey::TreasuryBalanceKey, &0i128);
        env.storage()
            .instance()
            .set(&BuyBackDataKey::TotalBurnedKey, &0i128);

        let history: Vec<BuyBackRecord> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&BuyBackDataKey::HistoryKey, &history);

        env.events().publish(
            (Symbol::new(&env, "buyback_initialized"),),
            (admin, burn_cap, fee_bps),
        );
    }

    // -----------------------------------------------------------------------
    // Buy-Back & Burn System – Treasury Management
    // -----------------------------------------------------------------------

    /// Deposit funds into the buy-back treasury.
    /// Typically called when marketplace fees are collected.
    ///
    /// # Arguments
    /// * `depositor` – The address depositing funds.
    /// * `amount` – Amount to deposit into the treasury.
    pub fn deposit_to_treasury(env: Env, depositor: Address, amount: i128) {
        depositor.require_auth();

        if amount <= 0 {
            panic!("deposit amount must be positive");
        }

        Self::require_buyback_initialized(&env);

        let current: i128 = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::TreasuryBalanceKey)
            .unwrap_or(0);

        let new_balance = current
            .checked_add(amount)
            .expect("treasury balance overflow");

        env.storage()
            .instance()
            .set(&BuyBackDataKey::TreasuryBalanceKey, &new_balance);

        env.events()
            .publish((Symbol::new(&env, "treasury_deposit"), depositor), amount);
    }

    /// Collect a fee from a trade amount and deposit it into the treasury.
    /// Called internally during marketplace `purchase` operations.
    ///
    /// # Arguments
    /// * `trade_amount` – The total trade amount to calculate fee from.
    ///
    /// # Returns
    /// The fee amount collected.
    pub fn collect_fee(env: Env, trade_amount: i128) -> i128 {
        Self::require_buyback_initialized(&env);

        let config: BuyBackConfig = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::BuyBackConfigKey)
            .expect("buy-back not configured");

        let fee = trade_amount
            .checked_mul(config.fee_bps as i128)
            .expect("fee calculation overflow")
            / 10_000;

        if fee > 0 {
            let current: i128 = env
                .storage()
                .instance()
                .get(&BuyBackDataKey::TreasuryBalanceKey)
                .unwrap_or(0);

            let new_balance = current.checked_add(fee).expect("treasury balance overflow");

            env.storage()
                .instance()
                .set(&BuyBackDataKey::TreasuryBalanceKey, &new_balance);

            env.events()
                .publish((Symbol::new(&env, "fee_collected"),), (trade_amount, fee));
        }

        fee
    }

    // -----------------------------------------------------------------------
    // Buy-Back & Burn System – Core Operations
    // -----------------------------------------------------------------------

    /// Buy back tokens using treasury funds.
    ///
    /// Uses accumulated treasury funds to buy back tokens from the market.
    /// In production, this would integrate with Stellar's path payments / DEX.
    /// Currently simulates the purchase and removes tokens from circulation.
    ///
    /// # Arguments
    /// * `admin` – Must be the buy-back admin.
    /// * `amount` – Amount of tokens to buy back.
    /// * `source_funds` – Treasury funds to use for the buy-back.
    /// * `governance_id` – Optional governance contract for approval check.
    pub fn buy_back_tokens(
        env: Env,
        admin: Address,
        amount: i128,
        source_funds: i128,
        governance_id: Option<Address>,
    ) {
        admin.require_auth();
        Self::require_buyback_admin(&env, &admin);
        Self::require_buyback_not_paused(&env);

        if amount <= 0 {
            panic!("buy-back amount must be positive");
        }
        if source_funds <= 0 {
            panic!("source funds must be positive");
        }

        let config: BuyBackConfig = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::BuyBackConfigKey)
            .expect("buy-back not configured");

        // Enforce burn cap
        if amount > config.burn_cap {
            panic!("amount exceeds burn cap");
        }

        // Check treasury has sufficient funds
        let treasury: i128 = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::TreasuryBalanceKey)
            .unwrap_or(0);

        if treasury < source_funds {
            panic!("insufficient treasury funds");
        }

        // Enforce governance approval if required
        if config.require_governance {
            if let Some(gov_addr) = governance_id {
                let gov_client = GovernanceClient::new(&env, &gov_addr);
                // Use asset_id 0 as a sentinel for buy-back governance proposals
                gov_client.require_approved(&0);
            } else {
                panic!("governance approval required but no governance contract provided");
            }
        }

        // Deduct from treasury
        let new_treasury = treasury
            .checked_sub(source_funds)
            .expect("treasury underflow");
        env.storage()
            .instance()
            .set(&BuyBackDataKey::TreasuryBalanceKey, &new_treasury);

        // Update total burned
        let total_burned: i128 = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::TotalBurnedKey)
            .unwrap_or(0);
        let new_total_burned = total_burned
            .checked_add(amount)
            .expect("total burned overflow");
        env.storage()
            .instance()
            .set(&BuyBackDataKey::TotalBurnedKey, &new_total_burned);

        // Record in history
        let record = BuyBackRecord {
            amount,
            burned: amount,
            source_funds,
            timestamp: env.ledger().timestamp(),
            executor: admin.clone(),
            auto_triggered: false,
        };
        Self::append_buyback_history(&env, record);

        // Emit buy-back event
        env.events().publish(
            (Symbol::new(&env, "buy_back_executed"), admin),
            (amount, source_funds),
        );

        // Emit burn event
        env.events().publish(
            (Symbol::new(&env, "tokens_burned"),),
            (amount, new_total_burned),
        );
    }

    /// Burn tokens directly from the treasury allocation.
    ///
    /// Reduces total supply by removing tokens permanently.
    /// Subject to burn cap and optional governance approval.
    ///
    /// # Arguments
    /// * `admin` – Must be the buy-back admin.
    /// * `amount` – Amount of tokens to burn.
    /// * `governance_id` – Optional governance contract for approval check.
    pub fn burn_tokens(env: Env, admin: Address, amount: i128, governance_id: Option<Address>) {
        admin.require_auth();
        Self::require_buyback_admin(&env, &admin);
        Self::require_buyback_not_paused(&env);

        if amount <= 0 {
            panic!("burn amount must be positive");
        }

        let config: BuyBackConfig = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::BuyBackConfigKey)
            .expect("buy-back not configured");

        // Enforce burn cap
        if amount > config.burn_cap {
            panic!("burn amount exceeds burn cap");
        }

        // Enforce governance approval if required
        if config.require_governance {
            if let Some(gov_addr) = governance_id {
                let gov_client = GovernanceClient::new(&env, &gov_addr);
                gov_client.require_approved(&0);
            } else {
                panic!("governance approval required but no governance contract provided");
            }
        }

        // Update total burned
        let total_burned: i128 = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::TotalBurnedKey)
            .unwrap_or(0);
        let new_total_burned = total_burned
            .checked_add(amount)
            .expect("total burned overflow");
        env.storage()
            .instance()
            .set(&BuyBackDataKey::TotalBurnedKey, &new_total_burned);

        // Record in history
        let record = BuyBackRecord {
            amount: 0,
            burned: amount,
            source_funds: 0,
            timestamp: env.ledger().timestamp(),
            executor: admin.clone(),
            auto_triggered: false,
        };
        Self::append_buyback_history(&env, record);

        // Emit burn event
        env.events().publish(
            (Symbol::new(&env, "tokens_burned"),),
            (amount, new_total_burned),
        );
    }

    /// Auto buy-back triggered when treasury balance exceeds the threshold.
    ///
    /// Anyone can call this function, but it only executes if:
    /// - The treasury balance exceeds `auto_threshold`
    /// - The buy-back system is not paused
    /// - The auto-buyback amount does not exceed the burn cap
    pub fn auto_buy_back(env: Env) {
        Self::require_buyback_initialized(&env);
        Self::require_buyback_not_paused(&env);

        let config: BuyBackConfig = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::BuyBackConfigKey)
            .expect("buy-back not configured");

        if config.auto_threshold <= 0 || config.auto_buyback_amount <= 0 {
            panic!("auto buy-back not configured");
        }

        let treasury: i128 = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::TreasuryBalanceKey)
            .unwrap_or(0);

        if treasury < config.auto_threshold {
            panic!("treasury below auto threshold");
        }

        // Enforce burn cap on auto amount
        let buyback_amount = if config.auto_buyback_amount > config.burn_cap {
            config.burn_cap
        } else {
            config.auto_buyback_amount
        };

        // Deduct from treasury (use buyback_amount as proxy for cost)
        let new_treasury = treasury
            .checked_sub(buyback_amount)
            .expect("treasury underflow");
        env.storage()
            .instance()
            .set(&BuyBackDataKey::TreasuryBalanceKey, &new_treasury);

        // Update total burned
        let total_burned: i128 = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::TotalBurnedKey)
            .unwrap_or(0);
        let new_total_burned = total_burned
            .checked_add(buyback_amount)
            .expect("total burned overflow");
        env.storage()
            .instance()
            .set(&BuyBackDataKey::TotalBurnedKey, &new_total_burned);

        // Record in history
        let admin: Address = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::BuyBackAdminKey)
            .expect("admin not set");

        let record = BuyBackRecord {
            amount: buyback_amount,
            burned: buyback_amount,
            source_funds: buyback_amount,
            timestamp: env.ledger().timestamp(),
            executor: admin,
            auto_triggered: true,
        };
        Self::append_buyback_history(&env, record);

        // Emit events
        env.events().publish(
            (Symbol::new(&env, "buy_back_executed"),),
            (buyback_amount, true), // (amount, auto_triggered)
        );

        env.events().publish(
            (Symbol::new(&env, "tokens_burned"),),
            (buyback_amount, new_total_burned),
        );
    }

    // -----------------------------------------------------------------------
    // Buy-Back & Burn System – BBConfiguration Management
    // -----------------------------------------------------------------------

    /// Update the buy-back configuration. BBAdmin only.
    ///
    /// # Arguments
    /// * `admin` – Must be the buy-back admin.
    /// * `burn_cap` – New maximum burn per operation.
    /// * `auto_threshold` – New auto-trigger threshold.
    /// * `auto_buyback_amount` – New auto buy-back amount.
    /// * `fee_bps` – New fee in basis points.
    /// * `require_governance` – Whether governance is required.
    pub fn update_buyback_config(
        env: Env,
        admin: Address,
        burn_cap: i128,
        auto_threshold: i128,
        auto_buyback_amount: i128,
        fee_bps: u32,
        require_governance: bool,
    ) {
        admin.require_auth();
        Self::require_buyback_admin(&env, &admin);

        if burn_cap <= 0 {
            panic!("burn cap must be positive");
        }
        if fee_bps > 10_000 {
            panic!("fee basis points must not exceed 10000");
        }

        let config = BuyBackConfig {
            burn_cap,
            auto_threshold,
            auto_buyback_amount,
            require_governance,
            fee_bps,
            paused: false,
        };

        env.storage()
            .instance()
            .set(&BuyBackDataKey::BuyBackConfigKey, &config);

        env.events().publish(
            (Symbol::new(&env, "buyback_config_updated"), admin),
            (burn_cap, auto_threshold, fee_bps),
        );
    }

    /// Set the burn cap. BBAdmin only.
    /// Provides a dedicated function for governance to adjust the burn cap.
    pub fn set_burn_cap(env: Env, admin: Address, new_cap: i128) {
        admin.require_auth();
        Self::require_buyback_admin(&env, &admin);

        if new_cap <= 0 {
            panic!("burn cap must be positive");
        }

        let mut config: BuyBackConfig = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::BuyBackConfigKey)
            .expect("buy-back not configured");

        config.burn_cap = new_cap;

        env.storage()
            .instance()
            .set(&BuyBackDataKey::BuyBackConfigKey, &config);

        env.events()
            .publish((Symbol::new(&env, "burn_cap_updated"),), new_cap);
    }

    /// Pause or unpause the buy-back system. BBAdmin only.
    pub fn set_buyback_paused(env: Env, admin: Address, paused: bool) {
        admin.require_auth();
        Self::require_buyback_admin(&env, &admin);

        let mut config: BuyBackConfig = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::BuyBackConfigKey)
            .expect("buy-back not configured");

        config.paused = paused;

        env.storage()
            .instance()
            .set(&BuyBackDataKey::BuyBackConfigKey, &config);

        env.events()
            .publish((Symbol::new(&env, "buyback_paused"),), paused);
    }

    // -----------------------------------------------------------------------
    // Buy-Back & Burn System – Query Functions
    // -----------------------------------------------------------------------

    /// Get the current treasury balance.
    pub fn get_treasury_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&BuyBackDataKey::TreasuryBalanceKey)
            .unwrap_or(0)
    }

    /// Get the total tokens burned across all operations.
    pub fn get_total_burned(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&BuyBackDataKey::TotalBurnedKey)
            .unwrap_or(0)
    }

    /// Get the current buy-back configuration.
    pub fn get_buyback_config(env: Env) -> Option<BuyBackConfig> {
        env.storage().instance().get(&BuyBackDataKey::BuyBackConfigKey)
    }

    /// Get the buy-back operation history.
    pub fn get_buyback_history(env: Env) -> Vec<BuyBackRecord> {
        env.storage()
            .persistent()
            .get(&BuyBackDataKey::HistoryKey)
            .unwrap_or(Vec::new(&env))
    }

    /// Check whether the auto buy-back threshold has been reached.
    pub fn is_auto_buyback_ready(env: Env) -> bool {
        let config: Option<BuyBackConfig> = env.storage().instance().get(&BuyBackDataKey::BuyBackConfigKey);

        if let Some(c) = config {
            if c.paused || c.auto_threshold <= 0 {
                return false;
            }
            let treasury: i128 = env
                .storage()
                .instance()
                .get(&BuyBackDataKey::TreasuryBalanceKey)
                .unwrap_or(0);
            treasury >= c.auto_threshold
        } else {
            false
        }
    }

    // -----------------------------------------------------------------------
    // Buy-Back & Burn System – Internal Helpers
    // -----------------------------------------------------------------------

    /// Verify that the buy-back system has been initialized.
    fn require_buyback_initialized(env: &Env) {
        if !env.storage().instance().has(&BuyBackDataKey::BuyBackAdminKey) {
            panic!("buy-back system not initialized");
        }
    }

    /// Verify the caller is the buy-back admin.
    fn require_buyback_admin(env: &Env, caller: &Address) {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::BuyBackAdminKey)
            .expect("buy-back system not initialized");
        if *caller != stored_admin {
            panic!("caller is not buy-back admin");
        }
    }

    /// Verify the buy-back system is not paused.
    fn require_buyback_not_paused(env: &Env) {
        let config: BuyBackConfig = env
            .storage()
            .instance()
            .get(&BuyBackDataKey::BuyBackConfigKey)
            .expect("buy-back not configured");
        if config.paused {
            panic!("buy-back system is paused");
        }
    }

    /// Append a BuyBackRecord to the history.
    fn append_buyback_history(env: &Env, record: BuyBackRecord) {
        let mut history: Vec<BuyBackRecord> = env
            .storage()
            .persistent()
            .get(&BuyBackDataKey::HistoryKey)
            .unwrap_or(Vec::new(env));
        history.push_back(record);
        env.storage()
            .persistent()
            .set(&BuyBackDataKey::HistoryKey, &history);
    }

    // -----------------------------------------------------------------------
    // Referral System Operations
    // -----------------------------------------------------------------------

    /// Initialize the referral reward system.
    pub fn initialize_referral(
        env: Env,
        admin: Address,
        treasury: Address,
        reward_bps: u32,
        min_activity: i128,
    ) {
        Self::require_admin(&env, &admin);

        if env.storage().instance().has(&ReferralDataKey::ReferralConfigKey) {
            panic!("referral system already initialized");
        }

        if reward_bps > 10_000 {
            panic!("reward basis points must not exceed 10000");
        }

        let config = ReferralConfig {
            reward_bps,
            treasury,
            min_activity,
        };

        env.storage().instance().set(&ReferralDataKey::ReferralConfigKey, &config);
        env.events().publish((Symbol::new(&env, "referral_initialized"),), (reward_bps, min_activity));
    }

    /// Link a new user to a referrer.
    pub fn refer_user(env: Env, referred: Address, referrer: Address) {
        referred.require_auth();

        if referred == referrer {
            panic!("self-referral not allowed");
        }

        if env.storage().persistent().has(&ReferralDataKey::ReferrerKey(referred.clone())) {
            panic!("user already referred");
        }

        env.storage().persistent().set(&ReferralDataKey::ReferrerKey(referred.clone()), &referrer);
        
        let count: u32 = env.storage().persistent().get(&ReferralDataKey::ReferralCountKey(referrer.clone())).unwrap_or(0);
        env.storage().persistent().set(&ReferralDataKey::ReferralCountKey(referrer.clone()), &(count + 1));

        env.events().publish((Symbol::new(&env, "referral_set"), referred), referrer);
    }

    /// Claim accumulated referral rewards.
    pub fn claim_referral_reward(env: Env, referrer: Address) {
        referrer.require_auth();

        let reward: i128 = env.storage().persistent().get(&ReferralDataKey::RewardBalanceKey(referrer.clone())).unwrap_or(0);
        if reward <= 0 {
            panic!("no reward to claim");
        }

        let _config: ReferralConfig = env.storage().instance().get(&ReferralDataKey::ReferralConfigKey).expect("referral system not initialized");

        // reset balance before transfer for security
        env.storage().persistent().set(&ReferralDataKey::RewardBalanceKey(referrer.clone()), &0i128);

        // In a real implementation, we would use a token client to transfer reward from config.treasury to referrer.
        // For this phase, we emit the event and simulate the state change.
        env.events().publish((Symbol::new(&env, "reward_claimed"), referrer), reward);
    }

    /// Query referral status for a user.
    pub fn get_referral_info(env: Env, user: Address) -> (Option<Address>, i128, u32) {
        let referrer: Option<Address> = env.storage().persistent().get(&ReferralDataKey::ReferrerKey(user.clone()));
        let reward: i128 = env.storage().persistent().get(&ReferralDataKey::RewardBalanceKey(user.clone())).unwrap_or(0);
        let count: u32 = env.storage().persistent().get(&ReferralDataKey::ReferralCountKey(user.clone())).unwrap_or(0);
        (referrer, reward, count)
    }

    // Helper to credit referral rewards
    fn credit_referral_reward(env: &Env, referred: &Address, fee_amount: i128) {
        if let Some(config) = env.storage().instance().get::<_, ReferralConfig>(&ReferralDataKey::ReferralConfigKey) {
            if let Some(referrer) = env.storage().persistent().get::<_, Address>(&ReferralDataKey::ReferrerKey(referred.clone())) {
                let reward = fee_amount
                    .checked_mul(config.reward_bps as i128)
                    .expect("reward calculation overflow")
                    / 10_000;

                if reward > 0 {
                    let current: i128 = env.storage().persistent().get(&ReferralDataKey::RewardBalanceKey(referrer.clone())).unwrap_or(0);
                    env.storage().persistent().set(&ReferralDataKey::RewardBalanceKey(referrer.clone()), &(current + reward));
                    env.events().publish((Symbol::new(env, "referral_reward_credited"), referrer), reward);
                }
            }
        }
    }

    /// Get total tokenized value
    pub fn get_total_tokenized_value(_env: Env, _asset_id: Option<u64>, time_range: Option<TimeRange>) -> Result<i128, ComplianceError> {
        if let Some(range) = time_range {
            if range.start_timestamp > range.end_timestamp {
                return Err(ComplianceError::InvalidTimeRange);
            }
        }
        // Stub: sum supplies across assets from storage
        Ok(0)
    }

    /// Get transaction volume
    pub fn get_transaction_volume(_env: Env, _asset_id: Option<u64>, time_range: Option<TimeRange>) -> Result<i128, ComplianceError> {
        if let Some(range) = time_range {
            if range.start_timestamp > range.end_timestamp {
                return Err(ComplianceError::InvalidTimeRange);
            }
        }
        // Stub: aggregate transaction volumes
        Ok(0)
    }

    /// Get unique holder count
    pub fn get_holder_count(_env: Env, _asset_id: Option<u64>, time_range: Option<TimeRange>) -> Result<u32, ComplianceError> {
        if let Some(range) = time_range {
            if range.start_timestamp > range.end_timestamp {
                return Err(ComplianceError::InvalidTimeRange);
            }
        }
        // Stub: count unique holders
        Ok(0)
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::vec;
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
        
        let _not_admin = Address::generate(&env);
        env.mock_all_auths();
        
        // This should pass with admin auth
        mp_client.set_asset_privacy(&admin, &1, &true);
        assert!(mp_client.is_private(&1));
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

    // =======================================================================
    // Buy-Back & Burn System Tests
    // =======================================================================

    /// Helper: deploy marketplace and initialize buy-back system.
    fn setup_buyback() -> (Env, Address, MarketplaceClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let admin = Address::generate(&env);

        mp_client.initialize_buyback(
            &admin, &10_000, // burn_cap
            &50_000, // auto_threshold
            &5_000,  // auto_buyback_amount
            &30,     // fee_bps (0.30%)
            &false,  // require_governance
        );

        (env, mp_id, mp_client, admin)
    }

    // -----------------------------------------------------------------------
    // Initialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize_buyback_success() {
        let (_env, _mp_id, mp_client, _admin) = setup_buyback();

        assert_eq!(mp_client.get_treasury_balance(), 0);
        assert_eq!(mp_client.get_total_burned(), 0);

        let config = mp_client.get_buyback_config().unwrap();
        assert_eq!(config.burn_cap, 10_000);
        assert_eq!(config.auto_threshold, 50_000);
        assert_eq!(config.auto_buyback_amount, 5_000);
        assert_eq!(config.fee_bps, 30);
        assert!(!config.require_governance);
        assert!(!config.paused);
    }

    #[test]
    #[should_panic(expected = "buy-back system already initialized")]
    fn test_initialize_buyback_twice_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.initialize_buyback(&admin, &10_000, &50_000, &5_000, &30, &false);
    }

    #[test]
    #[should_panic(expected = "burn cap must be positive")]
    fn test_initialize_buyback_zero_burn_cap_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let admin = Address::generate(&env);
        mp_client.initialize_buyback(&admin, &0, &50_000, &5_000, &30, &false);
    }

    #[test]
    #[should_panic(expected = "fee basis points must not exceed 10000")]
    fn test_initialize_buyback_invalid_fee_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let admin = Address::generate(&env);
        mp_client.initialize_buyback(&admin, &10_000, &50_000, &5_000, &10_001, &false);
    }

    // -----------------------------------------------------------------------
    // Treasury deposit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_deposit_to_treasury() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();

        mp_client.deposit_to_treasury(&admin, &25_000);
        assert_eq!(mp_client.get_treasury_balance(), 25_000);

        mp_client.deposit_to_treasury(&admin, &10_000);
        assert_eq!(mp_client.get_treasury_balance(), 35_000);
    }

    #[test]
    #[should_panic(expected = "deposit amount must be positive")]
    fn test_deposit_to_treasury_zero_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &0);
    }

    #[test]
    #[should_panic(expected = "deposit amount must be positive")]
    fn test_deposit_to_treasury_negative_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &-100);
    }

    // -----------------------------------------------------------------------
    // Fee collection tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_collect_fee() {
        let (_env, _mp_id, mp_client, _admin) = setup_buyback();

        // 30 bps on 100_000 = 300
        let fee = mp_client.collect_fee(&100_000);
        assert_eq!(fee, 300);
        assert_eq!(mp_client.get_treasury_balance(), 300);
    }

    #[test]
    fn test_collect_fee_accumulates() {
        let (_env, _mp_id, mp_client, _admin) = setup_buyback();

        mp_client.collect_fee(&100_000); // 300
        mp_client.collect_fee(&200_000); // 600
        assert_eq!(mp_client.get_treasury_balance(), 900);
    }

    // -----------------------------------------------------------------------
    // Buy-back tokens tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_buy_back_tokens_success() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();

        // Fund treasury
        mp_client.deposit_to_treasury(&admin, &50_000);

        // Buy back 5000 tokens using 5000 from treasury
        mp_client.buy_back_tokens(&admin, &5_000, &5_000, &None);

        assert_eq!(mp_client.get_treasury_balance(), 45_000);
        assert_eq!(mp_client.get_total_burned(), 5_000);

        let history = mp_client.get_buyback_history();
        assert_eq!(history.len(), 1);
        let record = history.get(0).unwrap();
        assert_eq!(record.amount, 5_000);
        assert_eq!(record.burned, 5_000);
        assert_eq!(record.source_funds, 5_000);
        assert!(!record.auto_triggered);
    }

    #[test]
    fn test_buy_back_tokens_multiple_operations() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();

        mp_client.deposit_to_treasury(&admin, &50_000);

        mp_client.buy_back_tokens(&admin, &3_000, &3_000, &None);
        mp_client.buy_back_tokens(&admin, &2_000, &2_000, &None);

        assert_eq!(mp_client.get_treasury_balance(), 45_000);
        assert_eq!(mp_client.get_total_burned(), 5_000);

        let history = mp_client.get_buyback_history();
        assert_eq!(history.len(), 2);
    }

    #[test]
    #[should_panic(expected = "buy-back amount must be positive")]
    fn test_buy_back_tokens_zero_amount_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &50_000);
        mp_client.buy_back_tokens(&admin, &0, &5_000, &None);
    }

    #[test]
    #[should_panic(expected = "amount exceeds burn cap")]
    fn test_buy_back_tokens_exceeds_burn_cap_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &50_000);
        // burn_cap is 10_000, trying 15_000
        mp_client.buy_back_tokens(&admin, &15_000, &15_000, &None);
    }

    #[test]
    #[should_panic(expected = "insufficient treasury funds")]
    fn test_buy_back_tokens_insufficient_treasury_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &1_000);
        mp_client.buy_back_tokens(&admin, &5_000, &5_000, &None);
    }

    #[test]
    #[should_panic(expected = "caller is not buy-back admin")]
    fn test_buy_back_tokens_non_admin_panics() {
        let (env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &50_000);
        let non_admin = Address::generate(&env);
        mp_client.buy_back_tokens(&non_admin, &5_000, &5_000, &None);
    }

    // -----------------------------------------------------------------------
    // Burn tokens tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_burn_tokens_success() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();

        mp_client.burn_tokens(&admin, &5_000, &None);

        assert_eq!(mp_client.get_total_burned(), 5_000);

        let history = mp_client.get_buyback_history();
        assert_eq!(history.len(), 1);
        let record = history.get(0).unwrap();
        assert_eq!(record.burned, 5_000);
        assert_eq!(record.amount, 0); // direct burn, no buy-back
    }

    #[test]
    #[should_panic(expected = "burn amount must be positive")]
    fn test_burn_tokens_zero_amount_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.burn_tokens(&admin, &0, &None);
    }

    #[test]
    #[should_panic(expected = "burn amount exceeds burn cap")]
    fn test_burn_tokens_exceeds_cap_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.burn_tokens(&admin, &15_000, &None); // cap is 10_000
    }

    #[test]
    #[should_panic(expected = "caller is not buy-back admin")]
    fn test_burn_tokens_non_admin_panics() {
        let (env, _mp_id, mp_client, _admin) = setup_buyback();
        let non_admin = Address::generate(&env);
        mp_client.burn_tokens(&non_admin, &5_000, &None);
    }

    // -----------------------------------------------------------------------
    // Auto buy-back tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_auto_buy_back_triggers_above_threshold() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();

        // Fund treasury above auto_threshold (50_000)
        mp_client.deposit_to_treasury(&admin, &60_000);

        assert!(mp_client.is_auto_buyback_ready());

        mp_client.auto_buy_back();

        // auto_buyback_amount = 5_000 deducted
        assert_eq!(mp_client.get_treasury_balance(), 55_000);
        assert_eq!(mp_client.get_total_burned(), 5_000);

        let history = mp_client.get_buyback_history();
        assert_eq!(history.len(), 1);
        let record = history.get(0).unwrap();
        assert!(record.auto_triggered);
        assert_eq!(record.amount, 5_000);
    }

    #[test]
    #[should_panic(expected = "treasury below auto threshold")]
    fn test_auto_buy_back_below_threshold_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &10_000);
        assert!(!mp_client.is_auto_buyback_ready());
        mp_client.auto_buy_back();
    }

    #[test]
    fn test_auto_buy_back_respects_burn_cap() {
        let env = Env::default();
        env.mock_all_auths();

        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let admin = Address::generate(&env);

        // Set auto_buyback_amount (20_000) > burn_cap (5_000)
        mp_client.initialize_buyback(
            &admin, &5_000,  // burn_cap
            &10_000, // auto_threshold
            &20_000, // auto_buyback_amount (exceeds cap)
            &30, &false,
        );

        mp_client.deposit_to_treasury(&admin, &50_000);
        mp_client.auto_buy_back();

        // Should burn only up to burn_cap (5_000)
        assert_eq!(mp_client.get_total_burned(), 5_000);
        assert_eq!(mp_client.get_treasury_balance(), 45_000);
    }

    // -----------------------------------------------------------------------
    // Pause tests
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "buy-back system is paused")]
    fn test_buy_back_tokens_paused_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &50_000);
        mp_client.set_buyback_paused(&admin, &true);
        mp_client.buy_back_tokens(&admin, &5_000, &5_000, &None);
    }

    #[test]
    #[should_panic(expected = "buy-back system is paused")]
    fn test_burn_tokens_paused_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.set_buyback_paused(&admin, &true);
        mp_client.burn_tokens(&admin, &5_000, &None);
    }

    #[test]
    #[should_panic(expected = "buy-back system is paused")]
    fn test_auto_buy_back_paused_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &60_000);
        mp_client.set_buyback_paused(&admin, &true);
        mp_client.auto_buy_back();
    }

    #[test]
    fn test_pause_unpause_cycle() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &50_000);

        // Pause
        mp_client.set_buyback_paused(&admin, &true);
        let config = mp_client.get_buyback_config().unwrap();
        assert!(config.paused);

        // Unpause
        mp_client.set_buyback_paused(&admin, &false);
        let config = mp_client.get_buyback_config().unwrap();
        assert!(!config.paused);

        // Operations work after unpause
        mp_client.buy_back_tokens(&admin, &5_000, &5_000, &None);
        assert_eq!(mp_client.get_total_burned(), 5_000);
    }

    // -----------------------------------------------------------------------
    // Configuration update tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_buyback_config() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();

        mp_client.update_buyback_config(
            &admin, &20_000,  // new burn_cap
            &100_000, // new auto_threshold
            &10_000,  // new auto_buyback_amount
            &50,      // new fee_bps
            &false,
        );

        let config = mp_client.get_buyback_config().unwrap();
        assert_eq!(config.burn_cap, 20_000);
        assert_eq!(config.auto_threshold, 100_000);
        assert_eq!(config.auto_buyback_amount, 10_000);
        assert_eq!(config.fee_bps, 50);
    }

    #[test]
    fn test_set_burn_cap() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();

        mp_client.set_burn_cap(&admin, &25_000);
        let config = mp_client.get_buyback_config().unwrap();
        assert_eq!(config.burn_cap, 25_000);
    }

    #[test]
    #[should_panic(expected = "burn cap must be positive")]
    fn test_set_burn_cap_zero_panics() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.set_burn_cap(&admin, &0);
    }

    // -----------------------------------------------------------------------
    // Reporting and history tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_buyback_history_tracks_operations() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();
        mp_client.deposit_to_treasury(&admin, &50_000);

        // Multiple operations
        mp_client.buy_back_tokens(&admin, &3_000, &3_000, &None);
        mp_client.burn_tokens(&admin, &2_000, &None);
        mp_client.buy_back_tokens(&admin, &1_000, &1_000, &None);

        let history = mp_client.get_buyback_history();
        assert_eq!(history.len(), 3);
        assert_eq!(mp_client.get_total_burned(), 6_000);
    }

    #[test]
    fn test_is_auto_buyback_ready_reflects_state() {
        let (_env, _mp_id, mp_client, admin) = setup_buyback();

        // Below threshold
        assert!(!mp_client.is_auto_buyback_ready());

        mp_client.deposit_to_treasury(&admin, &30_000);
        assert!(!mp_client.is_auto_buyback_ready());

        mp_client.deposit_to_treasury(&admin, &25_000);
        assert!(mp_client.is_auto_buyback_ready());

        // After auto buy-back drains below threshold
        mp_client.auto_buy_back();
        assert!(mp_client.is_auto_buyback_ready()); // 50_000 still >= 50_000

        // Pause makes it not ready
        mp_client.set_buyback_paused(&admin, &true);
        assert!(!mp_client.is_auto_buyback_ready());
    }

    // -----------------------------------------------------------------------
    // Governance-gated burn tests
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "governance approval required but no governance contract provided")]
    fn test_burn_tokens_governance_required_no_contract_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let admin = Address::generate(&env);

        // Initialize with require_governance = true
        mp_client.initialize_buyback(&admin, &10_000, &50_000, &5_000, &30, &true);

        mp_client.burn_tokens(&admin, &5_000, &None);
    }

    #[test]
    #[should_panic(expected = "governance approval required but no governance contract provided")]
    fn test_buy_back_governance_required_no_contract_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let admin = Address::generate(&env);

        mp_client.initialize_buyback(&admin, &10_000, &50_000, &5_000, &30, &true);
        mp_client.deposit_to_treasury(&admin, &50_000);

        mp_client.buy_back_tokens(&admin, &5_000, &5_000, &None);
    }

    // -----------------------------------------------------------------------
    // Referral System Tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_referral_initialization() {
        let env = Env::default();
        env.mock_all_auths();
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        mp_client.initialize(&admin);
        mp_client.initialize_referral(&admin, &treasury, &500, &1000); // 5% reward on fees

        let (referrer, reward, count) = mp_client.get_referral_info(&Address::generate(&env));
        assert!(referrer.is_none());
        assert_eq!(reward, 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_refer_user_flow() {
        let env = Env::default();
        env.mock_all_auths();
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let admin = Address::generate(&env);
        let referrer = Address::generate(&env);
        let referred = Address::generate(&env);

        mp_client.initialize(&admin);
        mp_client.refer_user(&referred, &referrer);

        let (stored_referrer, _reward, _count) = mp_client.get_referral_info(&referred);
        assert_eq!(stored_referrer.unwrap(), referrer);

        let (_, _, count) = mp_client.get_referral_info(&referrer);
        assert_eq!(count, 1);
    }

    #[test]
    #[should_panic(expected = "self-referral not allowed")]
    fn test_refer_self_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let user = Address::generate(&env);

        mp_client.refer_user(&user, &user);
    }

    #[test]
    fn test_referral_reward_crediting() {
        let env = Env::default();
        env.mock_all_auths();
        
        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let treasury = Address::generate(&env);
        let referrer = Address::generate(&env);
        let referred = Address::generate(&env);

        mp_client.initialize(&admin);
        // 5% reward on fees (500 bps), 30 bps marketplace fee
        mp_client.initialize_buyback(&admin, &10000, &50000, &5000, &30, &false);
        mp_client.initialize_referral(&admin, &treasury, &500, &100);

        mp_client.refer_user(&referred, &referrer);

        // Trade of 100,000 -> 300 fee -> 5% of 300 = 15 reward
        mp_client.purchase(&referred, &1, &100_000, &1, &ec_id);

        let (_, reward, _) = mp_client.get_referral_info(&referrer);
        assert_eq!(reward, 15);
    }

    #[test]
    fn test_claim_referral_reward_flow() {
        let env = Env::default();
        env.mock_all_auths();
        
        // Setup same as above and then claim
        let ec_id = env.register_contract(None, EmergencyControl);
        let mp_id = env.register_contract(None, Marketplace);
        let mp_client = MarketplaceClient::new(&env, &mp_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let referrer = Address::generate(&env);
        let referred = Address::generate(&env);

        mp_client.initialize(&admin);
        mp_client.initialize_buyback(&admin, &10000, &50000, &5000, &30, &false);
        mp_client.initialize_referral(&admin, &treasury, &500, &100);

        mp_client.refer_user(&referred, &referrer);
        mp_client.purchase(&referred, &1, &100_000, &1, &ec_id);

        // Claim
        mp_client.claim_referral_reward(&referrer);
        
        let (_, reward, _) = mp_client.get_referral_info(&referrer);
        assert_eq!(reward, 0);
    }

    #[test]
    fn test_compliance_queries_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, Marketplace);
        let client = MarketplaceClient::new(&env, &contract_id);

        // Test total tokenized value
        let total_val = client.get_total_tokenized_value(&None, &None);
        assert_eq!(total_val, 0);

        // Test transaction volume
        let volume = client.get_transaction_volume(&None, &None);
        assert_eq!(volume, 0);

        // Test holder count
        let holders = client.get_holder_count(&None, &None);
        assert_eq!(holders, 0);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #1)")]
    fn test_compliance_queries_invalid_time_range() {
        let env = Env::default();
        let contract_id = env.register_contract(None, Marketplace);
        let client = MarketplaceClient::new(&env, &contract_id);

        let invalid_range = TimeRange {
            start_timestamp: 100,
            end_timestamp: 50,
        };

        client.get_total_tokenized_value(&None, &Some(invalid_range));
    }
}


