use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec};

/// Defines the scope of a pause operation.
/// Supports granular, per-function pauses or a global halt.
#[derive(Clone, PartialEq, Debug)]
#[contracttype]
pub enum PauseScope {
    /// Pause all operations (transfers, trading, minting)
    All,
    /// Pause only transfer operations
    Transfers,
    /// Pause only trading/marketplace operations
    Trading,
    /// Pause only minting operations
    Minting,
}

/// A record of a pause or unpause event for audit trail purposes.
#[derive(Clone)]
#[contracttype]
pub struct PauseRecord {
    pub asset_id: u64,
    pub admin: Address,
    pub scope: PauseScope,
    pub reason: String,
    pub ledger_timestamp: u32,
    pub is_pause: bool,
}

/// Custom error codes for emergency control operations.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum EmergencyControlError {
    NotAdmin,
    AlreadyPaused,
    NotPaused,
    InvalidAsset,
}

// --- Storage Key Helpers ---

/// Storage key for the admin address.
fn admin_key(env: &Env) -> Symbol {
    Symbol::new(env, "ec_admin")
}

/// Storage key for a pause flag: "pause_{asset_id}_{scope_index}"
fn pause_flag_key(env: &Env, asset_id: u64, scope: &PauseScope) -> Symbol {
    let scope_idx: u32 = match scope {
        PauseScope::All => 0,
        PauseScope::Transfers => 1,
        PauseScope::Trading => 2,
        PauseScope::Minting => 3,
    };
    let mut key_str = [0u8; 32];
    let prefix = b"p_";
    let mut pos = 0;
    for &b in prefix {
        if pos < 32 {
            key_str[pos] = b;
            pos += 1;
        }
    }
    let id_str = encode_u64(asset_id);
    for &b in id_str.iter() {
        if b == 0 { break; }
        if pos < 32 {
            key_str[pos] = b;
            pos += 1;
        }
    }
    if pos < 32 {
        key_str[pos] = b'_';
        pos += 1;
    }
    if pos < 32 {
        key_str[pos] = b'0' + scope_idx as u8;
        pos += 1;
    }
    let s = core::str::from_utf8(&key_str[..pos]).unwrap_or("p_0_0");
    Symbol::new(env, s)
}

/// Storage key for auto-unpause ledger sequence.
fn auto_unpause_key(env: &Env, asset_id: u64, scope: &PauseScope) -> Symbol {
    let scope_idx: u32 = match scope {
        PauseScope::All => 0,
        PauseScope::Transfers => 1,
        PauseScope::Trading => 2,
        PauseScope::Minting => 3,
    };
    let mut key_str = [0u8; 32];
    let prefix = b"au_";
    let mut pos = 0;
    for &b in prefix {
        if pos < 32 {
            key_str[pos] = b;
            pos += 1;
        }
    }
    let id_str = encode_u64(asset_id);
    for &b in id_str.iter() {
        if b == 0 { break; }
        if pos < 32 {
            key_str[pos] = b;
            pos += 1;
        }
    }
    if pos < 32 {
        key_str[pos] = b'_';
        pos += 1;
    }
    if pos < 32 {
        key_str[pos] = b'0' + scope_idx as u8;
        pos += 1;
    }
    let s = core::str::from_utf8(&key_str[..pos]).unwrap_or("au_0_0");
    Symbol::new(env, s)
}

/// Storage key for pause history of an asset.
fn history_key(env: &Env, asset_id: u64) -> Symbol {
    let mut key_str = [0u8; 32];
    let prefix = b"ph_";
    let mut pos = 0;
    for &b in prefix {
        if pos < 32 {
            key_str[pos] = b;
            pos += 1;
        }
    }
    let id_str = encode_u64(asset_id);
    for &b in id_str.iter() {
        if b == 0 { break; }
        if pos < 32 {
            key_str[pos] = b;
            pos += 1;
        }
    }
    let s = core::str::from_utf8(&key_str[..pos]).unwrap_or("ph_0");
    Symbol::new(env, s)
}

/// Encode a u64 into a fixed-size byte array of ASCII digits.
fn encode_u64(mut val: u64) -> [u8; 20] {
    let mut buf = [0u8; 20];
    if val == 0 {
        buf[0] = b'0';
        return buf;
    }
    let mut pos = 0;
    let mut tmp = [0u8; 20];
    while val > 0 {
        tmp[pos] = b'0' + (val % 10) as u8;
        val /= 10;
        pos += 1;
    }
    for i in 0..pos {
        buf[i] = tmp[pos - 1 - i];
    }
    buf
}

#[contract]
pub struct EmergencyControl;

#[contractimpl]
impl EmergencyControl {
    // ---------------------------------------------------------------
    // Admin Management
    // ---------------------------------------------------------------

    /// Initialize the emergency control contract with an admin address.
    /// Must be called once before any other function.
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        if env.storage().instance().has(&admin_key(&env)) {
            panic!("already initialized");
        }
        env.storage().instance().set(&admin_key(&env), &admin);
    }

    /// Returns the current admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&admin_key(&env))
            .expect("not initialized")
    }

    // ---------------------------------------------------------------
    // Pause / Unpause
    // ---------------------------------------------------------------

    /// Pause an asset for a given scope.
    ///
    /// # Arguments
    /// * `admin` - Must be the contract admin
    /// * `asset_id` - The asset to pause
    /// * `scope` - Which operations to pause (All, Transfers, Trading, Minting)
    /// * `reason` - Human-readable reason for the pause
    /// * `auto_unpause_ledger` - Optional ledger sequence number at which to auto-unpause (0 = no auto-unpause)
    pub fn pause_asset(
        env: Env,
        admin: Address,
        asset_id: u64,
        scope: PauseScope,
        reason: String,
        auto_unpause_ledger: u32,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let flag_key = pause_flag_key(&env, asset_id, &scope);

        // Check not already paused for this scope
        let already_paused: bool = env.storage().instance().get(&flag_key).unwrap_or(false);
        if already_paused {
            panic!("asset is already paused for this scope");
        }

        // Set the pause flag
        env.storage().instance().set(&flag_key, &true);

        // Set auto-unpause if requested
        if auto_unpause_ledger > 0 {
            let au_key = auto_unpause_key(&env, asset_id, &scope);
            env.storage().instance().set(&au_key, &auto_unpause_ledger);
        }

        // Record in audit trail
        let record = PauseRecord {
            asset_id,
            admin: admin.clone(),
            scope: scope.clone(),
            reason: reason.clone(),
            ledger_timestamp: env.ledger().sequence(),
            is_pause: true,
        };
        Self::append_history(&env, asset_id, record);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "asset_paused"), asset_id),
            (admin, scope, reason, env.ledger().sequence()),
        );
    }

    /// Unpause an asset for a given scope.
    ///
    /// # Arguments
    /// * `admin` - Must be the contract admin
    /// * `asset_id` - The asset to unpause
    /// * `scope` - Which operations to unpause
    pub fn unpause_asset(env: Env, admin: Address, asset_id: u64, scope: PauseScope) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let flag_key = pause_flag_key(&env, asset_id, &scope);

        // Check that it is currently paused
        let is_paused: bool = env.storage().instance().get(&flag_key).unwrap_or(false);
        if !is_paused {
            panic!("asset is not paused for this scope");
        }

        // Clear the pause flag
        env.storage().instance().set(&flag_key, &false);

        // Clear any auto-unpause timer
        let au_key = auto_unpause_key(&env, asset_id, &scope);
        env.storage().instance().remove(&au_key);

        // Record in audit trail
        let reason = String::from_str(&env, "manual_unpause");
        let record = PauseRecord {
            asset_id,
            admin: admin.clone(),
            scope: scope.clone(),
            reason,
            ledger_timestamp: env.ledger().sequence(),
            is_pause: false,
        };
        Self::append_history(&env, asset_id, record);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "asset_unpaused"), asset_id),
            (admin, scope, env.ledger().sequence()),
        );
    }

    // ---------------------------------------------------------------
    // Query Functions
    // ---------------------------------------------------------------

    /// Check if an asset is paused for a given scope.
    /// Also checks the `All` scope (if `All` is paused, everything is paused).
    /// Handles auto-unpause expiry transparently.
    pub fn is_paused(env: Env, asset_id: u64, scope: PauseScope) -> bool {
        // Check auto-unpause for the specific scope
        Self::check_auto_unpause_internal(&env, asset_id, &scope);
        // Check auto-unpause for the All scope
        Self::check_auto_unpause_internal(&env, asset_id, &PauseScope::All);

        // Check if paused for the specific scope
        let flag_key = pause_flag_key(&env, asset_id, &scope);
        let specific_paused: bool = env.storage().instance().get(&flag_key).unwrap_or(false);
        if specific_paused {
            return true;
        }

        // Check if globally paused (scope = All)
        if scope != PauseScope::All {
            let all_key = pause_flag_key(&env, asset_id, &PauseScope::All);
            let all_paused: bool = env.storage().instance().get(&all_key).unwrap_or(false);
            if all_paused {
                return true;
            }
        }

        false
    }

    /// Enforcement modifier: panics if the asset is paused for the given scope.
    /// Call this at the start of any function that should be blocked when paused.
    pub fn require_not_paused(env: Env, asset_id: u64, scope: PauseScope) {
        if Self::is_paused(env, asset_id, scope) {
            panic!("operation blocked: asset is paused");
        }
    }

    /// Get the full pause history for an asset.
    pub fn get_pause_history(env: Env, asset_id: u64) -> Vec<PauseRecord> {
        let h_key = history_key(&env, asset_id);
        env.storage()
            .instance()
            .get(&h_key)
            .unwrap_or(Vec::new(&env))
    }

    // ---------------------------------------------------------------
    // Internal Helpers
    // ---------------------------------------------------------------

    /// Verify that the caller is the admin.
    fn require_admin(env: &Env, caller: &Address) {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&admin_key(env))
            .expect("not initialized");
        if *caller != stored_admin {
            panic!("caller is not admin");
        }
    }

    /// Append a PauseRecord to the history for a given asset.
    fn append_history(env: &Env, asset_id: u64, record: PauseRecord) {
        let h_key = history_key(env, asset_id);
        let mut history: Vec<PauseRecord> = env
            .storage()
            .instance()
            .get(&h_key)
            .unwrap_or(Vec::new(env));
        history.push_back(record);
        env.storage().instance().set(&h_key, &history);
    }

    /// Check if auto-unpause should trigger for a given asset+scope.
    /// If the current ledger sequence >= the auto-unpause ledger, clear the pause.
    fn check_auto_unpause_internal(env: &Env, asset_id: u64, scope: &PauseScope) {
        let au_key = auto_unpause_key(env, asset_id, scope);
        let auto_ledger: Option<u32> = env.storage().instance().get(&au_key);
        if let Some(target_ledger) = auto_ledger {
            if env.ledger().sequence() >= target_ledger {
                // Auto-unpause: clear the flag and timer
                let flag_key = pause_flag_key(env, asset_id, scope);
                env.storage().instance().set(&flag_key, &false);
                env.storage().instance().remove(&au_key);

                // Record in audit trail
                let reason = String::from_str(env, "auto_unpause");
                let record = PauseRecord {
                    asset_id,
                    admin: env
                        .storage()
                        .instance()
                        .get(&admin_key(env))
                        .unwrap_or_else(|| panic!("not initialized")),
                    scope: scope.clone(),
                    reason,
                    ledger_timestamp: env.ledger().sequence(),
                    is_pause: false,
                };
                Self::append_history(env, asset_id, record);

                // Emit event
                let admin: Address = env
                    .storage()
                    .instance()
                    .get(&admin_key(env))
                    .unwrap_or_else(|| panic!("not initialized"));
                env.events().publish(
                    (Symbol::new(env, "asset_unpaused"), asset_id),
                    (admin, scope.clone(), env.ledger().sequence()),
                );
            }
        }
    }
}

// =================================================================
// Unit Tests
// =================================================================

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};

    #[test]
    fn test_initialize_and_get_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);
        let stored_admin = client.get_admin();
        assert_eq!(stored_admin, admin);
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_double_initialize_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);
        client.initialize(&admin);
    }

    #[test]
    fn test_pause_and_unpause_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset_id: u64 = 1;
        let reason = String::from_str(&env, "security incident");

        // Pause trading
        client.pause_asset(&admin, &asset_id, &PauseScope::Trading, &reason, &0);
        assert!(client.is_paused(&asset_id, &PauseScope::Trading));

        // Transfers should NOT be paused
        assert!(!client.is_paused(&asset_id, &PauseScope::Transfers));

        // Unpause trading
        client.unpause_asset(&admin, &asset_id, &PauseScope::Trading);
        assert!(!client.is_paused(&asset_id, &PauseScope::Trading));
    }

    #[test]
    fn test_pause_all_blocks_everything() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset_id: u64 = 1;
        let reason = String::from_str(&env, "global halt");

        client.pause_asset(&admin, &asset_id, &PauseScope::All, &reason, &0);

        // All scopes should report as paused
        assert!(client.is_paused(&asset_id, &PauseScope::All));
        assert!(client.is_paused(&asset_id, &PauseScope::Transfers));
        assert!(client.is_paused(&asset_id, &PauseScope::Trading));
        assert!(client.is_paused(&asset_id, &PauseScope::Minting));
    }

    #[test]
    fn test_pause_history_recorded() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset_id: u64 = 1;
        let reason = String::from_str(&env, "audit");

        client.pause_asset(&admin, &asset_id, &PauseScope::Minting, &reason, &0);
        client.unpause_asset(&admin, &asset_id, &PauseScope::Minting);

        let history = client.get_pause_history(&asset_id);
        assert_eq!(history.len(), 2);

        // First record is the pause
        let first = history.get(0).unwrap();
        assert!(first.is_pause);
        assert_eq!(first.asset_id, asset_id);

        // Second record is the unpause
        let second = history.get(1).unwrap();
        assert!(!second.is_pause);
    }

    #[test]
    #[should_panic(expected = "asset is already paused for this scope")]
    fn test_double_pause_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset_id: u64 = 1;
        let reason = String::from_str(&env, "test");

        client.pause_asset(&admin, &asset_id, &PauseScope::Trading, &reason, &0);
        client.pause_asset(&admin, &asset_id, &PauseScope::Trading, &reason, &0);
    }

    #[test]
    #[should_panic(expected = "asset is not paused for this scope")]
    fn test_unpause_not_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset_id: u64 = 1;
        client.unpause_asset(&admin, &asset_id, &PauseScope::Trading);
    }

    #[test]
    #[should_panic(expected = "caller is not admin")]
    fn test_non_admin_pause_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let non_admin = Address::generate(&env);
        let asset_id: u64 = 1;
        let reason = String::from_str(&env, "test");

        client.pause_asset(&non_admin, &asset_id, &PauseScope::Trading, &reason, &0);
    }

    #[test]
    #[should_panic(expected = "operation blocked: asset is paused")]
    fn test_require_not_paused_blocks_when_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset_id: u64 = 1;
        let reason = String::from_str(&env, "blocked");

        client.pause_asset(&admin, &asset_id, &PauseScope::Trading, &reason, &0);
        client.require_not_paused(&asset_id, &PauseScope::Trading);
    }

    #[test]
    fn test_require_not_paused_allows_when_not_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset_id: u64 = 1;
        // Should not panic
        client.require_not_paused(&asset_id, &PauseScope::Trading);
    }

    #[test]
    fn test_auto_unpause() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset_id: u64 = 1;
        let reason = String::from_str(&env, "temporary halt");

        // Set auto-unpause at ledger sequence 100
        client.pause_asset(&admin, &asset_id, &PauseScope::Trading, &reason, &100);
        assert!(client.is_paused(&asset_id, &PauseScope::Trading));

        // Advance ledger sequence to 100
        env.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 20,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        // Should auto-unpause
        assert!(!client.is_paused(&asset_id, &PauseScope::Trading));
    }

    #[test]
    fn test_selective_scope_independence() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset_id: u64 = 1;
        let reason = String::from_str(&env, "scope test");

        // Pause only transfers
        client.pause_asset(&admin, &asset_id, &PauseScope::Transfers, &reason, &0);

        // Transfers paused
        assert!(client.is_paused(&asset_id, &PauseScope::Transfers));

        // Trading and Minting should NOT be paused
        assert!(!client.is_paused(&asset_id, &PauseScope::Trading));
        assert!(!client.is_paused(&asset_id, &PauseScope::Minting));
    }

    #[test]
    fn test_multiple_assets_independent() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let reason = String::from_str(&env, "asset isolation test");

        // Pause asset 1 for trading
        client.pause_asset(&admin, &1, &PauseScope::Trading, &reason, &0);

        // Asset 1 trading paused
        assert!(client.is_paused(&1, &PauseScope::Trading));

        // Asset 2 trading should NOT be paused
        assert!(!client.is_paused(&2, &PauseScope::Trading));
    }

    #[test]
    fn test_empty_pause_history() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyControl);
        let client = EmergencyControlClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let history = client.get_pause_history(&1);
        assert_eq!(history.len(), 0);
    }
}
