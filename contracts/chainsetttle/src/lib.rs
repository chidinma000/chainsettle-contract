#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, String, Vec, Symbol,
};

// ============================================================
// DATA TYPES
// ============================================================

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum MilestoneStatus {
    Pending,
    ProofSubmitted,
    Confirmed,
    Disputed,
    Resolved,
    // Confirmed but payment held until release_after_ledger
    ConfirmedHeld,
}

/// Controls whether milestones must be completed in order (Sequential)
/// or can be submitted and confirmed independently (Parallel).
/// Immutable after shipment creation.
#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum MilestoneMode {
    /// Proof for milestone N requires milestone N-1 to be Confirmed or Resolved first.
    Sequential,
    /// All milestones are independently submittable at any time.
    Parallel,
}

#[contracttype]
#[derive(Clone)]
pub struct Milestone {
    pub name: String,
    pub payment_percent: u32,
    pub proof_hash: String,
    pub status: MilestoneStatus,
    // Set when holdback_ledgers > 0 and milestone is confirmed
    pub release_after_ledger: u32,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum ShipmentStatus {
    Active,
    Completed,
    Cancelled,
}

#[contracttype]
#[derive(Clone)]
pub struct Shipment {
    pub id: String,
    /// All co-buyers. All must call confirm_milestone for payment to release.
    /// raise_dispute requires only one co-buyer's signature.
    pub buyers: Vec<Address>,
    pub supplier: Address,
    pub logistics: Address,
    pub arbiter: Address,
    pub token: Address,
    pub total_amount: i128,
    pub released_amount: i128,
    pub milestones: Vec<Milestone>,
    pub status: ShipmentStatus,
    pub milestone_mode: MilestoneMode,
    pub created_at: u32,
    // Issue #1: enforce sequential milestone ordering
    pub sequential: bool,
    // Issue #4: ledgers to hold payment after confirmation (0 = immediate)
    pub holdback_ledgers: u32,
}

/// Protocol fee configuration set by admin.
#[contracttype]
#[derive(Clone)]
pub struct FeeConfig {
    /// Fee in basis points (e.g. 30 = 0.30%). Max 1000 (10%).
    pub fee_bps: u32,
    /// Address that receives collected fees.
    pub treasury: Address,
}

// ============================================================
// STORAGE KEYS
// ============================================================

#[contracttype]
pub enum DataKey {
    Shipment(String),
    AllShipments,
    Admin,
    // Issue #2: allowed token whitelist
    AllowedTokens,
}

// ============================================================
// ERRORS
// ============================================================

#[contracttype]
#[derive(Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum ChainSettleError {
    ShipmentAlreadyExists = 1,
    ShipmentNotFound = 2,
    Unauthorized = 3,
    InvalidMilestoneIndex = 4,
    InvalidMilestoneStatus = 5,
    ShipmentNotActive = 6,
    InvalidPercentages = 7,
    InvalidAmount = 8,
    DisputeAlreadyOpen = 9,
    DeadlineNotBreached = 10,
    FeeTooHigh = 11,
    PreviousMilestoneNotComplete = 12,
}

// ============================================================
// CONTRACT
// ============================================================

#[contract]
pub struct ChainSettleContract;

#[contractimpl]
impl ChainSettleContract {

    // ----------------------------------------------------------
    // INIT
    // ----------------------------------------------------------

    pub fn init(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    // ----------------------------------------------------------
    // ISSUE #2: TOKEN WHITELIST MANAGEMENT
    // ----------------------------------------------------------

    /// Admin adds a token to the allowed list.
    /// When the list is non-empty, only listed tokens are accepted.
    pub fn add_allowed_token(env: Env, token: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialised"));
        admin.require_auth();

        let mut list: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AllowedTokens)
            .unwrap_or_else(|| Vec::new(&env));

        for i in 0..list.len() {
            if list.get(i).unwrap() == token {
                return; // already present
            }
        }
        list.push_back(token);
        env.storage().instance().set(&DataKey::AllowedTokens, &list);
    }

    /// Admin removes a token from the allowed list.
    pub fn remove_allowed_token(env: Env, token: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialised"));
        admin.require_auth();

        let list: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AllowedTokens)
            .unwrap_or_else(|| Vec::new(&env));

        let mut new_list: Vec<Address> = Vec::new(&env);
        for i in 0..list.len() {
            let t = list.get(i).unwrap();
            if t != token {
                new_list.push_back(t);
            }
        }
        env.storage().instance().set(&DataKey::AllowedTokens, &new_list);
    }

    // ----------------------------------------------------------
    // CREATE SHIPMENT
    // ----------------------------------------------------------

    /// Create a new shipment and lock funds in escrow.
    /// sequential=true enforces in-order milestone proof submission.
    /// holdback_ledgers>0 delays payment transfer after confirmation.
    pub fn create_shipment(
        env: Env,
        shipment_id: String,
        buyers: Vec<Address>,
        supplier: Address,
        logistics: Address,
        arbiter: Address,
        token: Address,
        total_amount: i128,
        milestones: Vec<Milestone>,
        sequential: bool,
        holdback_ledgers: u32,
    ) -> String {
        if buyers.is_empty() {
            panic!("at least one buyer is required");
        }

        // All co-buyers must authorise the creation
        for i in 0..buyers.len() {
            buyers.get(i).unwrap().require_auth();
        }

        if total_amount <= 0 {
            panic!("amount must be greater than zero");
        }

        // Issue #2: enforce token whitelist when non-empty
        let allowed: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AllowedTokens)
            .unwrap_or_else(|| Vec::new(&env));
        if allowed.len() > 0 {
            let mut found = false;
            for i in 0..allowed.len() {
                if allowed.get(i).unwrap() == token {
                    found = true;
                    break;
                }
            }
            if !found {
                panic!("unauthorized");
            }
        }

        let mut total_percent: u32 = 0;
        for i in 0..milestones.len() {
            let m = milestones.get(i).unwrap();
            total_percent += m.payment_percent;
        }
        if total_percent != 100 {
            panic!("milestone percentages must sum to 100");
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::Shipment(shipment_id.clone()))
        {
            panic!("shipment already exists");
        }

        // Transfer total_amount from the first buyer (primary payer).
        // In multi-buyer setups the callers are expected to have pre-funded
        // the primary buyer address, or the primary buyer holds the full escrow.
        let primary_buyer = buyers.get(0).unwrap();
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&primary_buyer, &env.current_contract_address(), &total_amount);

        // Normalise milestones: clear any approvals passed in by the caller
        let mut clean_milestones: Vec<Milestone> = Vec::new(&env);
        for i in 0..milestones.len() {
            let mut m = milestones.get(i).unwrap();
            m.approvals = Vec::new(&env);
            m.status = MilestoneStatus::Pending;
            m.proof_hash = String::from_str(&env, "");
            clean_milestones.push_back(m);
        }

        let shipment = Shipment {
            id: shipment_id.clone(),
            buyers,
            supplier,
            logistics,
            arbiter,
            token,
            total_amount,
            released_amount: 0,
            milestones: clean_milestones,
            status: ShipmentStatus::Active,
            milestone_mode,
            created_at: env.ledger().sequence(),
            sequential,
            holdback_ledgers,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Shipment(shipment_id.clone()), &shipment);

        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Shipment(shipment_id.clone()), 100_000, 6_300_000);

        env.events().publish(
            (Symbol::new(&env, "shipment_created"), shipment_id.clone()),
            shipment_id.clone(),
        );

        shipment_id
    }

    // ----------------------------------------------------------
    // SUBMIT PROOF
    // ----------------------------------------------------------

    /// Supplier or logistics party submits proof for a milestone.
    /// Issue #1: when sequential=true, all prior milestones must be Confirmed or Resolved.
    pub fn submit_proof(
        env: Env,
        caller: Address,
        shipment_id: String,
        milestone_index: u32,
        proof_hash: String,
    ) {
        caller.require_auth();

        let mut shipment = Self::get_shipment_internal(&env, &shipment_id);

        if shipment.status != ShipmentStatus::Active {
            panic!("shipment is not active");
        }

        let idx = milestone_index as usize;
        if idx >= shipment.milestones.len() as usize {
            panic!("invalid milestone index");
        }

        // Issue #1: sequential enforcement
        if shipment.sequential && milestone_index > 0 {
            for i in 0..milestone_index {
                let prev = shipment.milestones.get(i).unwrap();
                if prev.status != MilestoneStatus::Confirmed
                    && prev.status != MilestoneStatus::Resolved
                {
                    panic!("milestone is not in pending status");
                }
            }
        }

        let mut milestone = shipment.milestones.get(milestone_index).unwrap();

        if milestone.status != MilestoneStatus::Pending {
            panic!("milestone is not in pending status");
        }

        if caller != shipment.supplier && caller != shipment.logistics {
            panic!("unauthorized");
        }

        // Sequential mode: previous milestone must be complete
        if shipment.milestone_mode == MilestoneMode::Sequential && milestone_index > 0 {
            let prev = shipment.milestones.get(milestone_index - 1).unwrap();
            if prev.status != MilestoneStatus::Confirmed && prev.status != MilestoneStatus::Resolved {
                panic!("previous milestone not yet complete");
            }
        }

        // Deadline check: proof must arrive on or before the deadline ledger
        if let Some(deadline) = milestone.deadline_ledger {
            if env.ledger().sequence() > deadline {
                panic!("milestone deadline has passed");
            }
        }

        milestone.proof_hash = proof_hash;
        milestone.status = MilestoneStatus::ProofSubmitted;
        shipment.milestones.set(milestone_index, milestone);

        env.storage()
            .persistent()
            .set(&DataKey::Shipment(shipment_id.clone()), &shipment);

        env.events().publish(
            (Symbol::new(&env, "proof_submitted"), shipment_id.clone()),
            milestone_index,
        );
    }

    // ----------------------------------------------------------
    // CONFIRM MILESTONE (multi-sig)
    // ----------------------------------------------------------

    /// Buyer confirms a milestone.
    /// Issue #4: when holdback_ledgers > 0, records release_after_ledger instead of
    /// transferring immediately; status becomes ConfirmedHeld.
    pub fn confirm_milestone(
        env: Env,
        buyer: Address,
        shipment_id: String,
        milestone_index: u32,
    ) {
        buyer.require_auth();

        let mut shipment = Self::get_shipment_internal(&env, &shipment_id);

        if shipment.status != ShipmentStatus::Active {
            panic!("shipment is not active");
        }

        // Verify caller is one of the co-buyers
        if !Self::is_buyer(&shipment, &buyer) {
            panic!("unauthorized");
        }

        let idx = milestone_index as usize;
        if idx >= shipment.milestones.len() as usize {
            panic!("invalid milestone index");
        }

        let mut milestone = shipment.milestones.get(milestone_index).unwrap();

        if milestone.status != MilestoneStatus::ProofSubmitted {
            panic!("milestone proof not yet submitted");
        }

        let payment = (shipment.total_amount * milestone.payment_percent as i128) / 100;

        if shipment.holdback_ledgers > 0 {
            // Issue #4: hold payment
            milestone.release_after_ledger =
                env.ledger().sequence() + shipment.holdback_ledgers;
            milestone.status = MilestoneStatus::ConfirmedHeld;
            shipment.milestones.set(milestone_index, milestone.clone());

            env.storage()
                .persistent()
                .set(&DataKey::Shipment(shipment_id.clone()), &shipment);

            env.events().publish(
                (Symbol::new(&env, "payment_held"), shipment_id.clone()),
                (milestone_index, milestone.release_after_ledger),
            );
        } else {
            milestone.status = MilestoneStatus::Confirmed;
            shipment.milestones.set(milestone_index, milestone.clone());
            shipment.released_amount += payment;

            let token_client = token::Client::new(&env, &shipment.token);
            token_client.transfer(
                &env.current_contract_address(),
                &shipment.supplier,
                &payment,
            );

            let all_done = Self::all_milestones_done(&shipment);
            if all_done {
                shipment.status = ShipmentStatus::Completed;
            }

            env.storage()
                .persistent()
                .set(&DataKey::Shipment(shipment_id.clone()), &shipment);

            env.events().publish(
                (Symbol::new(&env, "milestone_confirmed"), shipment_id.clone()),
                (milestone_index, payment),
            );
        }
    }

    // ----------------------------------------------------------
    // ISSUE #4: RELEASE HELD PAYMENT
    // ----------------------------------------------------------

    /// Anyone can call this once the holdback window has passed.
    /// Transfers the held payment to the supplier.
    pub fn release_held_payment(env: Env, shipment_id: String, milestone_index: u32) {
        let mut shipment = Self::get_shipment_internal(&env, &shipment_id);

        if shipment.status != ShipmentStatus::Active {
            panic!("shipment is not active");
        }

        let mut milestone = shipment.milestones.get(milestone_index).unwrap();

        if milestone.status != MilestoneStatus::ConfirmedHeld {
            panic!("milestone is not in pending status");
        }

        if env.ledger().sequence() < milestone.release_after_ledger {
            panic!("holdback period not yet expired");
        }

        let payment = (shipment.total_amount * milestone.payment_percent as i128) / 100;
        milestone.status = MilestoneStatus::Confirmed;
        milestone.release_after_ledger = 0;
        shipment.milestones.set(milestone_index, milestone);
        shipment.released_amount += payment;

        if all_approved {
            let payment = (shipment.total_amount * milestone.payment_percent as i128) / 100;
            let net_payment = Self::deduct_fee(&env, payment, &shipment.token, &mut fee_amount);

        let all_done = Self::all_milestones_done(&shipment);
        if all_done {
            shipment.status = ShipmentStatus::Completed;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Shipment(shipment_id.clone()), &shipment);

        env.events().publish(
            (Symbol::new(&env, "milestone_confirmed"), shipment_id.clone()),
            (milestone_index, all_approved, fee_amount),
        );
    }

    // ----------------------------------------------------------
    // RAISE DISPUTE
    // ----------------------------------------------------------

    /// Buyer raises a dispute on a ProofSubmitted or ConfirmedHeld milestone.
    /// Issue #4: disputing a ConfirmedHeld milestone cancels the holdback.
    pub fn raise_dispute(
        env: Env,
        buyer: Address,
        shipment_id: String,
        milestone_index: u32,
    ) {
        buyer.require_auth();

        let mut shipment = Self::get_shipment_internal(&env, &shipment_id);

        if shipment.status != ShipmentStatus::Active {
            panic!("shipment is not active");
        }

        if !Self::is_buyer(&shipment, &buyer) {
            panic!("unauthorized");
        }

        let mut milestone = shipment.milestones.get(milestone_index).unwrap();

        if milestone.status != MilestoneStatus::ProofSubmitted
            && milestone.status != MilestoneStatus::ConfirmedHeld
        {
            panic!("can only dispute a submitted proof");
        }

        // Issue #4: cancel holdback if within window
        milestone.release_after_ledger = 0;
        milestone.status = MilestoneStatus::Disputed;
        // Clear partial approvals so the slate is clean if proof is resubmitted
        milestone.approvals = Vec::new(&env);
        shipment.milestones.set(milestone_index, milestone);

        env.storage()
            .persistent()
            .set(&DataKey::Shipment(shipment_id.clone()), &shipment);

        env.events().publish(
            (Symbol::new(&env, "dispute_raised"), shipment_id.clone()),
            milestone_index,
        );
    }

    // ----------------------------------------------------------
    // RESOLVE DISPUTE
    // ----------------------------------------------------------

    pub fn resolve_dispute(
        env: Env,
        arbiter: Address,
        shipment_id: String,
        milestone_index: u32,
        approve: bool,
    ) {
        arbiter.require_auth();

        let mut shipment = Self::get_shipment_internal(&env, &shipment_id);

        if shipment.status != ShipmentStatus::Active {
            panic!("shipment is not active");
        }

        if arbiter != shipment.arbiter {
            panic!("unauthorized");
        }

        let mut milestone = shipment.milestones.get(milestone_index).unwrap();

        if milestone.status != MilestoneStatus::Disputed {
            panic!("milestone is not in disputed status");
        }

        if approve {
            let payment = (shipment.total_amount * milestone.payment_percent as i128) / 100;
            let mut fee_amount: i128 = 0;
            let net_payment = Self::deduct_fee(&env, payment, &shipment.token, &mut fee_amount);

            shipment.released_amount += payment;

            let token_client = token::Client::new(&env, &shipment.token);
            token_client.transfer(
                &env.current_contract_address(),
                &shipment.supplier,
                &net_payment,
            );

            milestone.status = MilestoneStatus::Resolved;
        } else {
            milestone.status = MilestoneStatus::Pending;
            milestone.proof_hash = String::from_str(&env, "");
            milestone.approvals = Vec::new(&env);
        }

        shipment.milestones.set(milestone_index, milestone);

        let all_done = Self::all_milestones_done(&shipment);
        if all_done {
            shipment.status = ShipmentStatus::Completed;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Shipment(shipment_id.clone()), &shipment);

        env.events().publish(
            (Symbol::new(&env, "dispute_resolved"), shipment_id.clone()),
            (milestone_index, approve),
        );
    }

    // ----------------------------------------------------------
    // CANCEL SHIPMENT
    // ----------------------------------------------------------

    /// Cancel the shipment and refund unreleased escrow to the buyer.
    /// Issue #3: allowed even after some milestones are Confirmed; already-released
    /// funds stay with the supplier. Blocked if any milestone is Disputed.
    pub fn cancel_shipment(env: Env, buyer: Address, shipment_id: String) {
        buyer.require_auth();

        let mut shipment = Self::get_shipment_internal(&env, &shipment_id);

        if shipment.status != ShipmentStatus::Active {
            panic!("shipment is not active");
        }

        if !Self::is_buyer(&shipment, &buyer) {
            panic!("unauthorized");
        }

        // Issue #3: block cancellation if any milestone is Disputed
        for i in 0..shipment.milestones.len() {
            let m = shipment.milestones.get(i).unwrap();
            if m.status == MilestoneStatus::Disputed {
                panic!("cannot cancel: dispute must be resolved first");
            }
        }

        let refund = shipment.total_amount - shipment.released_amount;
        if refund > 0 {
            let token_client = token::Client::new(&env, &shipment.token);
            token_client.transfer(&env.current_contract_address(), &shipment.buyer, &refund);
        }

        shipment.status = ShipmentStatus::Cancelled;

        env.storage()
            .persistent()
            .set(&DataKey::Shipment(shipment_id.clone()), &shipment);

        // Issue #3: event now carries refunded_amount
        env.events().publish(
            (Symbol::new(&env, "shipment_cancelled"), shipment_id.clone()),
            refund,
        );
    }

    // ----------------------------------------------------------
    // TRIGGER DEADLINE CANCELLATION
    // ----------------------------------------------------------

    /// Anyone can call this to cancel a shipment when a milestone's deadline has passed
    /// and proof has not yet been submitted. Remaining funds are returned to the primary buyer.
    ///
    /// Conditions:
    ///   - Shipment must be Active.
    ///   - The milestone must be Pending (proof not submitted).
    ///   - env.ledger().sequence() > deadline_ledger.
    pub fn trigger_deadline_cancellation(
        env: Env,
        shipment_id: String,
        milestone_index: u32,
    ) {
        let mut shipment = Self::get_shipment_internal(&env, &shipment_id);

        if shipment.status != ShipmentStatus::Active {
            panic!("shipment is not active");
        }

        let idx = milestone_index as usize;
        if idx >= shipment.milestones.len() as usize {
            panic!("invalid milestone index");
        }

        let milestone = shipment.milestones.get(milestone_index).unwrap();

        if milestone.status != MilestoneStatus::Pending {
            panic!("milestone is not pending");
        }

        let deadline = milestone
            .deadline_ledger
            .unwrap_or_else(|| panic!("milestone has no deadline"));

        if env.ledger().sequence() <= deadline {
            panic!("deadline has not been breached");
        }

        // Refund remaining escrow to primary buyer
        let refund = shipment.total_amount - shipment.released_amount;
        let primary_buyer = shipment.buyers.get(0).unwrap();
        let token_client = token::Client::new(&env, &shipment.token);
        token_client.transfer(&env.current_contract_address(), &primary_buyer, &refund);

        shipment.status = ShipmentStatus::Cancelled;

        env.storage()
            .persistent()
            .set(&DataKey::Shipment(shipment_id.clone()), &shipment);

        env.events().publish(
            (Symbol::new(&env, "deadline_breached"), shipment_id.clone()),
            (milestone_index, deadline),
        );
    }

    // ----------------------------------------------------------
    // READ-ONLY QUERIES
    // ----------------------------------------------------------

    pub fn get_shipment(env: Env, shipment_id: String) -> Shipment {
        Self::get_shipment_internal(&env, &shipment_id)
    }

    pub fn get_milestone(env: Env, shipment_id: String, milestone_index: u32) -> Milestone {
        let shipment = Self::get_shipment_internal(&env, &shipment_id);
        shipment
            .milestones
            .get(milestone_index)
            .unwrap_or_else(|| panic!("invalid milestone index"))
    }

    pub fn get_escrow_balance(env: Env, shipment_id: String) -> i128 {
        let shipment = Self::get_shipment_internal(&env, &shipment_id);
        shipment.total_amount - shipment.released_amount
    }

    pub fn get_fee_config(env: Env) -> Option<FeeConfig> {
        env.storage().instance().get(&DataKey::FeeConfig)
    }

    // ----------------------------------------------------------
    // INTERNAL HELPERS
    // ----------------------------------------------------------

    fn get_shipment_internal(env: &Env, shipment_id: &String) -> Shipment {
        env.storage()
            .persistent()
            .get(&DataKey::Shipment(shipment_id.clone()))
            .unwrap_or_else(|| panic!("shipment not found"))
    }

    fn all_milestones_done(shipment: &Shipment) -> bool {
        (0..shipment.milestones.len()).all(|i| {
            let s = shipment.milestones.get(i).unwrap().status;
            s == MilestoneStatus::Confirmed || s == MilestoneStatus::Resolved
        })
    }
}

mod test;
