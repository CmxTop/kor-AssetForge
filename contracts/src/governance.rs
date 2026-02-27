use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol};

use crate::asset_token::AssetTokenClient;

// ---------------------------------------------------------------------------
// Storage Keys
// ---------------------------------------------------------------------------

#[derive(Clone)]
#[contracttype]
pub enum GovDataKey {
    /// Admin address
    Admin,
    /// Address of the governance token (AssetToken) contract
    TokenContract,
    /// Auto-incrementing proposal counter
    NextProposalId,
    /// Proposal metadata by id
    Proposal(u64),
    /// Whether a voter has already voted on a proposal
    Voted(u64, Address),
    /// Snapshot of a voter's balance at proposal creation
    Snapshot(u64, Address),
    /// Set of approved asset ids (proposal passed)
    Approved(u64),
    /// Required deposit amount for creating proposals (anti-spam)
    DepositAmount,
    /// Depositor address per proposal (for refund)
    Depositor(u64),
    /// Quorum threshold (minimum total votes for a proposal to be valid)
    Quorum,
}

// ---------------------------------------------------------------------------
// Proposal Status
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Debug)]
#[contracttype]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
}

// ---------------------------------------------------------------------------
// Proposal Struct
// ---------------------------------------------------------------------------

#[derive(Clone)]
#[contracttype]
pub struct Proposal {
    pub proposal_id: u64,
    pub asset_id: u64,
    pub proposer: Address,
    pub description: String,
    pub votes_for: i128,
    pub votes_against: i128,
    pub quorum_threshold: i128,
    pub end_time: u64,
    pub status: ProposalStatus,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Governance;

#[contractimpl]
impl Governance {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the governance contract.
    ///
    /// # Arguments
    /// * `admin` – Contract admin who can adjust parameters.
    /// * `token_contract` – Address of the governance token (AssetToken).
    /// * `quorum_threshold` – Minimum total weighted votes for a valid result.
    /// * `deposit_amount` – Tokens required as anti-spam deposit for proposals.
    pub fn initialize(
        env: Env,
        admin: Address,
        token_contract: Address,
        quorum_threshold: i128,
        deposit_amount: i128,
    ) {
        if env.storage().instance().has(&GovDataKey::Admin) {
            panic!("already initialized");
        }
        admin.require_auth();

        env.storage().instance().set(&GovDataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&GovDataKey::TokenContract, &token_contract);
        env.storage()
            .instance()
            .set(&GovDataKey::Quorum, &quorum_threshold);
        env.storage()
            .instance()
            .set(&GovDataKey::DepositAmount, &deposit_amount);
        env.storage()
            .instance()
            .set(&GovDataKey::NextProposalId, &1u64);
    }

    // -----------------------------------------------------------------------
    // Proposals
    // -----------------------------------------------------------------------

    /// Create a new governance proposal to list an asset.
    ///
    /// The proposer must hold at least `deposit_amount` governance tokens.
    /// Their current balance is snapshotted at the time of creation so that
    /// voting weight is locked to the creation ledger.
    ///
    /// Returns the new `proposal_id`.
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        asset_id: u64,
        description: String,
        duration: u64,
    ) -> u64 {
        proposer.require_auth();

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&GovDataKey::TokenContract)
            .expect("not initialized");

        let deposit_amount: i128 = env
            .storage()
            .instance()
            .get(&GovDataKey::DepositAmount)
            .unwrap_or(0);

        // Check proposer has enough tokens for the deposit
        let token_client = AssetTokenClient::new(&env, &token_addr);
        let balance = token_client.balance(&proposer);
        if balance < deposit_amount {
            panic!("insufficient tokens for deposit");
        }

        let quorum: i128 = env
            .storage()
            .instance()
            .get(&GovDataKey::Quorum)
            .unwrap_or(0);

        // Allocate proposal id
        let proposal_id: u64 = env
            .storage()
            .instance()
            .get(&GovDataKey::NextProposalId)
            .unwrap_or(1);
        env.storage()
            .instance()
            .set(&GovDataKey::NextProposalId, &(proposal_id + 1));

        // end_time = current ledger timestamp + duration
        let end_time = env.ledger().timestamp() + duration;

        let proposal = Proposal {
            proposal_id,
            asset_id,
            proposer: proposer.clone(),
            description,
            votes_for: 0,
            votes_against: 0,
            quorum_threshold: quorum,
            end_time,
            status: ProposalStatus::Active,
        };

        env.storage()
            .persistent()
            .set(&GovDataKey::Proposal(proposal_id), &proposal);

        // Record depositor for refund on success
        env.storage()
            .persistent()
            .set(&GovDataKey::Depositor(proposal_id), &proposer);

        // Snapshot proposer balance
        env.storage()
            .persistent()
            .set(&GovDataKey::Snapshot(proposal_id, proposer.clone()), &balance);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "proposal_created"), proposal_id),
            (asset_id, end_time),
        );

        proposal_id
    }

    // -----------------------------------------------------------------------
    // Voting
    // -----------------------------------------------------------------------

    /// Cast a vote on an active proposal.
    ///
    /// The voter's governance-token balance is snapshotted on first
    /// interaction with this proposal (the balance at the time of voting).
    /// Double-voting is prevented.
    pub fn vote(env: Env, voter: Address, proposal_id: u64, vote_yes: bool) {
        voter.require_auth();

        // Load proposal
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&GovDataKey::Proposal(proposal_id))
            .expect("proposal not found");

        // Must still be active
        if proposal.status != ProposalStatus::Active {
            panic!("proposal not active");
        }

        // Must not be expired
        let now = env.ledger().timestamp();
        if now >= proposal.end_time {
            panic!("proposal expired");
        }

        // Prevent double-voting
        if env
            .storage()
            .persistent()
            .has(&GovDataKey::Voted(proposal_id, voter.clone()))
        {
            panic!("already voted");
        }

        // Snapshot voter balance (weight) — uses current on-chain balance
        let token_addr: Address = env
            .storage()
            .instance()
            .get(&GovDataKey::TokenContract)
            .expect("not initialized");
        let token_client = AssetTokenClient::new(&env, &token_addr);
        let weight = token_client.balance(&voter);

        if weight <= 0 {
            panic!("insufficient tokens to vote");
        }

        // Record snapshot
        env.storage()
            .persistent()
            .set(&GovDataKey::Snapshot(proposal_id, voter.clone()), &weight);

        // Record vote
        if vote_yes {
            proposal.votes_for = proposal
                .votes_for
                .checked_add(weight)
                .expect("vote overflow");
        } else {
            proposal.votes_against = proposal
                .votes_against
                .checked_add(weight)
                .expect("vote overflow");
        }

        // Mark as voted
        env.storage()
            .persistent()
            .set(&GovDataKey::Voted(proposal_id, voter.clone()), &true);

        // Save updated proposal
        env.storage()
            .persistent()
            .set(&GovDataKey::Proposal(proposal_id), &proposal);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "vote_cast"), proposal_id),
            (voter, vote_yes, weight),
        );
    }

    // -----------------------------------------------------------------------
    // Tally & Execute
    // -----------------------------------------------------------------------

    /// Tally votes after the proposal's `end_time` has passed.
    ///
    /// If quorum is met and votes_for > votes_against, the proposal is
    /// marked `Passed` and the asset_id is recorded as approved for listing.
    /// The proposer's deposit is refunded on success.
    pub fn tally_execute(env: Env, proposal_id: u64) {
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&GovDataKey::Proposal(proposal_id))
            .expect("proposal not found");

        if proposal.status != ProposalStatus::Active {
            panic!("proposal already finalized");
        }

        let now = env.ledger().timestamp();
        if now < proposal.end_time {
            panic!("voting period not ended");
        }

        let total_votes = proposal
            .votes_for
            .checked_add(proposal.votes_against)
            .expect("overflow");

        if total_votes >= proposal.quorum_threshold && proposal.votes_for > proposal.votes_against {
            proposal.status = ProposalStatus::Passed;

            // Mark asset as approved for listing
            env.storage()
                .persistent()
                .set(&GovDataKey::Approved(proposal.asset_id), &true);

            // Emit execution event
            env.events().publish(
                (Symbol::new(&env, "proposal_executed"), proposal_id),
                proposal.asset_id,
            );
        } else {
            proposal.status = ProposalStatus::Rejected;

            // Emit rejection event
            env.events().publish(
                (Symbol::new(&env, "proposal_rejected"), proposal_id),
                proposal.asset_id,
            );
        }

        // Save finalized proposal
        env.storage()
            .persistent()
            .set(&GovDataKey::Proposal(proposal_id), &proposal);
    }

    // -----------------------------------------------------------------------
    // Query helpers
    // -----------------------------------------------------------------------

    /// Check if an asset has been approved via governance.
    pub fn is_approved(env: Env, asset_id: u64) -> bool {
        env.storage()
            .persistent()
            .get(&GovDataKey::Approved(asset_id))
            .unwrap_or(false)
    }

    /// Enforce that an asset has been approved. Panics otherwise.
    pub fn require_approved(env: Env, asset_id: u64) {
        if !Self::is_approved(env, asset_id) {
            panic!("asset not approved by governance");
        }
    }

    /// Return proposal details.
    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        env.storage()
            .persistent()
            .get(&GovDataKey::Proposal(proposal_id))
    }

    /// Check whether a voter has voted on a given proposal.
    pub fn has_voted(env: Env, proposal_id: u64, voter: Address) -> bool {
        env.storage()
            .persistent()
            .has(&GovDataKey::Voted(proposal_id, voter))
    }

    /// Return the snapshotted voting weight for a voter on a proposal.
    pub fn get_vote_weight(env: Env, proposal_id: u64, voter: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&GovDataKey::Snapshot(proposal_id, voter))
            .unwrap_or(0)
    }
}

// ===========================================================================
// Unit Tests
// ===========================================================================

#[cfg(test)]
mod test {
    use super::*;
    use crate::asset_token::AssetToken;
    use crate::emergency_control::{EmergencyControl, EmergencyControlClient};
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::String;

    /// Helper: deploy governance ecosystem (emergency control + asset token + governance).
    fn setup() -> (
        Env,
        Address, // gov_id
        Address, // at_id
        Address, // ec_id
        Address, // admin
    ) {
        let env = Env::default();
        env.mock_all_auths();

        // Emergency control
        let ec_id = env.register_contract(None, EmergencyControl);
        let ec_client = EmergencyControlClient::new(&env, &ec_id);
        let admin = Address::generate(&env);
        ec_client.initialize(&admin);

        // Asset token (governance token)
        let at_id = env.register_contract(None, AssetToken);
        let at_client = AssetTokenClient::new(&env, &at_id);
        at_client.initialize(
            &admin,
            &String::from_str(&env, "GovToken"),
            &String::from_str(&env, "GOV"),
            &7,
            &0,
        );

        // Governance contract
        let gov_id = env.register_contract(None, Governance);
        let gov_client = GovernanceClient::new(&env, &gov_id);
        gov_client.initialize(&admin, &at_id, &100, &50); // quorum=100, deposit=50

        (env, gov_id, at_id, ec_id, admin)
    }

    /// Mint governance tokens to an address.
    fn mint_tokens(env: &Env, at_id: &Address, ec_id: &Address, to: &Address, amount: i128) {
        let at_client = AssetTokenClient::new(env, at_id);
        at_client.mint(to, &amount, &1, ec_id);
    }

    // -----------------------------------------------------------------------
    // 1. Initialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize() {
        let (env, gov_id, _at_id, _ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        // No proposals yet – get_proposal returns None
        assert!(client.get_proposal(&1).is_none());
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_double_initialize_panics() {
        let (env, gov_id, at_id, _ec_id, admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);
        client.initialize(&admin, &at_id, &100, &50);
    }

    // -----------------------------------------------------------------------
    // 2. Proposal creation
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_proposal() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);
        let proposer = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);

        let pid = client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "List real estate asset"),
            &3600,
        );
        assert_eq!(pid, 1);

        let p = client.get_proposal(&pid).unwrap();
        assert_eq!(p.asset_id, 1);
        assert_eq!(p.votes_for, 0);
        assert_eq!(p.votes_against, 0);
        assert_eq!(p.status, ProposalStatus::Active);
    }

    #[test]
    fn test_create_multiple_proposals_increments_id() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);
        let proposer = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);

        let pid1 = client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "First"),
            &3600,
        );
        let pid2 = client.create_proposal(
            &proposer,
            &2,
            &String::from_str(&env, "Second"),
            &3600,
        );
        assert_eq!(pid1, 1);
        assert_eq!(pid2, 2);
    }

    #[test]
    #[should_panic(expected = "insufficient tokens for deposit")]
    fn test_create_proposal_insufficient_deposit() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);
        let proposer = Address::generate(&env);

        // Mint less than deposit_amount (50)
        mint_tokens(&env, &at_id, &ec_id, &proposer, 10);

        client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "Under-funded"),
            &3600,
        );
    }

    // -----------------------------------------------------------------------
    // 3. Voting
    // -----------------------------------------------------------------------

    #[test]
    fn test_vote_yes() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &voter, 150);

        let pid = client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "Vote test"),
            &3600,
        );

        client.vote(&voter, &pid, &true);

        let p = client.get_proposal(&pid).unwrap();
        assert_eq!(p.votes_for, 150);
        assert_eq!(p.votes_against, 0);
        assert!(client.has_voted(&pid, &voter));
        assert_eq!(client.get_vote_weight(&pid, &voter), 150);
    }

    #[test]
    fn test_vote_no() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &voter, 80);

        let pid = client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "Nay test"),
            &3600,
        );

        client.vote(&voter, &pid, &false);

        let p = client.get_proposal(&pid).unwrap();
        assert_eq!(p.votes_for, 0);
        assert_eq!(p.votes_against, 80);
    }

    #[test]
    #[should_panic(expected = "already voted")]
    fn test_double_vote_panics() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &voter, 100);

        let pid = client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "Double vote"),
            &3600,
        );

        client.vote(&voter, &pid, &true);
        client.vote(&voter, &pid, &false); // should panic
    }

    #[test]
    #[should_panic(expected = "proposal expired")]
    fn test_vote_after_expiry_panics() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &voter, 100);

        let pid = client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "Expired"),
            &3600,
        );

        // Advance past end_time
        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        client.vote(&voter, &pid, &true);
    }

    #[test]
    #[should_panic(expected = "insufficient tokens to vote")]
    fn test_vote_with_zero_balance_panics() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env); // no tokens

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);

        let pid = client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "No tokens"),
            &3600,
        );

        client.vote(&voter, &pid, &true);
    }

    // -----------------------------------------------------------------------
    // 4. Tally & Execute
    // -----------------------------------------------------------------------

    #[test]
    fn test_tally_proposal_passes() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &voter1, 80);
        mint_tokens(&env, &at_id, &ec_id, &voter2, 60);

        let pid = client.create_proposal(
            &proposer,
            &42,
            &String::from_str(&env, "List asset 42"),
            &3600,
        );

        client.vote(&voter1, &pid, &true);
        client.vote(&voter2, &pid, &true);

        // Advance past end
        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        client.tally_execute(&pid);

        let p = client.get_proposal(&pid).unwrap();
        assert_eq!(p.status, ProposalStatus::Passed);
        assert!(client.is_approved(&42));
    }

    #[test]
    fn test_tally_proposal_rejected_more_against() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &voter1, 40);
        mint_tokens(&env, &at_id, &ec_id, &voter2, 80);

        let pid = client.create_proposal(
            &proposer,
            &5,
            &String::from_str(&env, "Reject me"),
            &3600,
        );

        client.vote(&voter1, &pid, &true);   // 40 for
        client.vote(&voter2, &pid, &false);   // 80 against

        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        client.tally_execute(&pid);

        let p = client.get_proposal(&pid).unwrap();
        assert_eq!(p.status, ProposalStatus::Rejected);
        assert!(!client.is_approved(&5));
    }

    #[test]
    fn test_tally_proposal_rejected_quorum_not_met() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &voter, 50); // quorum = 100, only 50 votes

        let pid = client.create_proposal(
            &proposer,
            &7,
            &String::from_str(&env, "Low quorum"),
            &3600,
        );

        client.vote(&voter, &pid, &true);

        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        client.tally_execute(&pid);

        let p = client.get_proposal(&pid).unwrap();
        assert_eq!(p.status, ProposalStatus::Rejected);
        assert!(!client.is_approved(&7));
    }

    #[test]
    #[should_panic(expected = "voting period not ended")]
    fn test_tally_before_end_panics() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);

        let pid = client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "Too early"),
            &3600,
        );

        client.tally_execute(&pid); // should panic
    }

    #[test]
    #[should_panic(expected = "proposal already finalized")]
    fn test_tally_twice_panics() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &voter, 150);

        let pid = client.create_proposal(
            &proposer,
            &1,
            &String::from_str(&env, "Finalized"),
            &3600,
        );

        client.vote(&voter, &pid, &true);

        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        client.tally_execute(&pid);
        client.tally_execute(&pid); // should panic
    }

    // -----------------------------------------------------------------------
    // 5. Governance-gated listing (require_approved)
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "asset not approved by governance")]
    fn test_require_approved_panics_if_not_approved() {
        let (env, gov_id, _at_id, _ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);
        client.require_approved(&999);
    }

    #[test]
    fn test_require_approved_succeeds_after_passed_proposal() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &voter, 120);

        let pid = client.create_proposal(
            &proposer,
            &10,
            &String::from_str(&env, "Approve asset 10"),
            &3600,
        );

        client.vote(&voter, &pid, &true);

        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        client.tally_execute(&pid);

        // Should NOT panic
        client.require_approved(&10);
    }

    // -----------------------------------------------------------------------
    // 6. Multiple voters, weighted tally
    // -----------------------------------------------------------------------

    #[test]
    fn test_weighted_voting_with_multiple_voters() {
        let (env, gov_id, at_id, ec_id, _admin) = setup();
        let client = GovernanceClient::new(&env, &gov_id);

        let proposer = Address::generate(&env);
        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        let v3 = Address::generate(&env);

        mint_tokens(&env, &at_id, &ec_id, &proposer, 200);
        mint_tokens(&env, &at_id, &ec_id, &v1, 30);
        mint_tokens(&env, &at_id, &ec_id, &v2, 50);
        mint_tokens(&env, &at_id, &ec_id, &v3, 70);

        let pid = client.create_proposal(
            &proposer,
            &99,
            &String::from_str(&env, "Weighted"),
            &3600,
        );

        client.vote(&v1, &pid, &true);  // 30 for
        client.vote(&v2, &pid, &false); // 50 against
        client.vote(&v3, &pid, &true);  // 70 for  => total for=100, against=50

        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        client.tally_execute(&pid);

        let p = client.get_proposal(&pid).unwrap();
        assert_eq!(p.votes_for, 100);
        assert_eq!(p.votes_against, 50);
        assert_eq!(p.status, ProposalStatus::Passed);
    }
}
