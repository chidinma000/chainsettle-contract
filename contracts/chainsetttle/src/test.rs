#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, vec, Address, Env, String,
};

// ============================================================
// TEST HELPERS
// ============================================================

struct TestSetup {
    env: Env,
    contract_id: Address,
    token_id: Address,
    buyer: Address,
    buyer2: Address,
    supplier: Address,
    logistics: Address,
    arbiter: Address,
    treasury: Address,
}

fn setup() -> TestSetup {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ChainSettleContract, ());

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::StellarAssetClient::new(&env, &token_id);

    let buyer = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let supplier = Address::generate(&env);
    let logistics = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let treasury = Address::generate(&env);

    token_client.mint(&buyer, &10_000_000_000);
    token_client.mint(&buyer2, &10_000_000_000);

    let client = ChainSettleContractClient::new(&env, &contract_id);
    client.init(&buyer);

    TestSetup {
        env,
        contract_id,
        token_id,
        buyer,
        buyer2,
        supplier,
        logistics,
        arbiter,
        treasury,
    }
}

fn build_milestones(env: &Env) -> Vec<Milestone> {
    vec![
        env,
        Milestone {
            name: String::from_str(env, "Goods Dispatched"),
            payment_percent: 25,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            deadline_ledger: None,
            approvals: Vec::new(env),
        },
        Milestone {
            name: String::from_str(env, "In Transit"),
            payment_percent: 50,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            deadline_ledger: None,
            approvals: Vec::new(env),
        },
        Milestone {
            name: String::from_str(env, "Delivered"),
            payment_percent: 25,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            deadline_ledger: None,
            approvals: Vec::new(env),
        },
    ]
}

fn single_buyer_vec(env: &Env, buyer: &Address) -> Vec<Address> {
    vec![env, buyer.clone()]
}

// ============================================================
// ORIGINAL TESTS (updated for new API)
// ============================================================

#[test]
fn test_create_shipment_success() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = String::from_str(&t.env, "SHIP-001");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    let buyer_balance = token_client.balance(&t.buyer);
    assert_eq!(buyer_balance, 10_000_000_000 - total_amount);

    let escrow_balance = token_client.balance(&t.contract_id);
    assert_eq!(escrow_balance, total_amount);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Active);
    assert_eq!(shipment.total_amount, total_amount);
    assert_eq!(shipment.released_amount, 0);
    assert_eq!(shipment.milestones.len(), 3);
}

#[test]
#[should_panic(expected = "milestone percentages must sum to 100")]
fn test_create_shipment_invalid_percentages() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let bad_milestones = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "Step 1"),
            payment_percent: 30,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            deadline_ledger: None,
            approvals: Vec::new(&t.env),
        },
        Milestone {
            name: String::from_str(&t.env, "Step 2"),
            payment_percent: 30,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            deadline_ledger: None,
            approvals: Vec::new(&t.env),
        },
        Milestone {
            name: String::from_str(&t.env, "Step 3"),
            payment_percent: 30,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            deadline_ledger: None,
            approvals: Vec::new(&t.env),
        },
    ];

    client.create_shipment(
        &String::from_str(&t.env, "SHIP-BAD"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &bad_milestones,
        &MilestoneMode::Parallel,
    );
}

#[test]
fn test_submit_proof_and_confirm_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = String::from_str(&t.env, "SHIP-001");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "ipfs://QmXxx...dispatch"),
    );

    let m0 = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0.status, MilestoneStatus::ProofSubmitted);

    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    let expected_payment = total_amount * 25 / 100;
    let supplier_balance = token_client.balance(&t.supplier);
    assert_eq!(supplier_balance, expected_payment);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.released_amount, expected_payment);
    assert_eq!(shipment.status, ShipmentStatus::Active);
}

#[test]
fn test_full_shipment_lifecycle() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = String::from_str(&t.env, "SHIP-FULL");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://dispatch"));
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    client.submit_proof(&t.logistics, &shipment_id, &1, &String::from_str(&t.env, "ipfs://transit"));
    client.confirm_milestone(&t.buyer, &shipment_id, &1);

    client.submit_proof(&t.supplier, &shipment_id, &2, &String::from_str(&t.env, "ipfs://delivered"));
    client.confirm_milestone(&t.buyer, &shipment_id, &2);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Completed);
    assert_eq!(shipment.released_amount, total_amount);

    let supplier_balance = token_client.balance(&t.supplier);
    assert_eq!(supplier_balance, total_amount);

    let escrow = client.get_escrow_balance(&shipment_id);
    assert_eq!(escrow, 0);
}

#[test]
fn test_raise_and_resolve_dispute_approve() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = String::from_str(&t.env, "SHIP-DISPUTE");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://proof"));
    client.raise_dispute(&t.buyer, &shipment_id, &0);

    let m0 = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0.status, MilestoneStatus::Disputed);

    client.resolve_dispute(&t.arbiter, &shipment_id, &0, &true);

    let expected = total_amount * 25 / 100;
    let supplier_balance = token_client.balance(&t.supplier);
    assert_eq!(supplier_balance, expected);
}

#[test]
fn test_raise_and_resolve_dispute_reject() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = String::from_str(&t.env, "SHIP-REJECT");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://bad-proof"));
    client.raise_dispute(&t.buyer, &shipment_id, &0);
    client.resolve_dispute(&t.arbiter, &shipment_id, &0, &false);

    let m0 = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0.status, MilestoneStatus::Pending);

    let supplier_balance = token_client.balance(&t.supplier);
    assert_eq!(supplier_balance, 0);
}

#[test]
fn test_cancel_shipment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = String::from_str(&t.env, "SHIP-CANCEL");
    let total_amount: i128 = 1_000_000_000;
    let buyer_balance_before = token_client.balance(&t.buyer);

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.cancel_shipment(&t.buyer, &shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);

    let buyer_balance_after = token_client.balance(&t.buyer);
    assert_eq!(buyer_balance_after, buyer_balance_before);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_unauthorized_confirm_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-AUTH");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://proof"));

    // Supplier tries to confirm — should panic
    client.confirm_milestone(&t.supplier, &shipment_id, &0);
}

// ============================================================
// FEATURE: MILESTONE DEADLINE ENFORCEMENT
// ============================================================

fn build_milestones_with_deadline(env: &Env, deadline: u32) -> Vec<Milestone> {
    vec![
        env,
        Milestone {
            name: String::from_str(env, "Dispatched"),
            payment_percent: 50,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            deadline_ledger: Some(deadline),
            approvals: Vec::new(env),
        },
        Milestone {
            name: String::from_str(env, "Delivered"),
            payment_percent: 50,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            deadline_ledger: None,
            approvals: Vec::new(env),
        },
    ]
}

#[test]
fn test_deadline_cancellation_success() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = String::from_str(&t.env, "SHIP-DEADLINE");
    let total_amount: i128 = 1_000_000_000;

    // Current ledger sequence is 0 in tests; deadline at ledger 100
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones_with_deadline(&t.env, 100),
        &MilestoneMode::Parallel,
    );

    // Advance ledger past the deadline
    t.env.ledger().set_sequence_number(101);

    let buyer_balance_before = token_client.balance(&t.buyer);

    // Anyone can trigger — use supplier here
    client.trigger_deadline_cancellation(&shipment_id, &0);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);

    // Full refund to primary buyer
    let buyer_balance_after = token_client.balance(&t.buyer);
    assert_eq!(buyer_balance_after - buyer_balance_before, total_amount);
}

#[test]
#[should_panic(expected = "deadline has not been breached")]
fn test_deadline_cancellation_too_early() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-EARLY");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &build_milestones_with_deadline(&t.env, 100),
        &MilestoneMode::Parallel,
    );

    // Ledger is still at 0 — deadline not breached
    client.trigger_deadline_cancellation(&shipment_id, &0);
}

#[test]
#[should_panic(expected = "milestone deadline has passed")]
fn test_submit_proof_after_deadline_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-LATE");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &build_milestones_with_deadline(&t.env, 100),
        &MilestoneMode::Parallel,
    );

    t.env.ledger().set_sequence_number(101);

    // Supplier tries to submit proof after deadline — should panic
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "ipfs://late"),
    );
}

#[test]
fn test_submit_proof_on_deadline_ledger_allowed() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-ONTIME");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &build_milestones_with_deadline(&t.env, 100),
        &MilestoneMode::Parallel,
    );

    // Exactly at the deadline — still allowed
    t.env.ledger().set_sequence_number(100);

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "ipfs://ontime"),
    );

    let m0 = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0.status, MilestoneStatus::ProofSubmitted);
}

// ============================================================
// FEATURE: MULTI-SIGNATURE BUYER APPROVAL
// ============================================================

#[test]
fn test_multisig_both_buyers_must_confirm() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = String::from_str(&t.env, "SHIP-MULTI");
    let total_amount: i128 = 1_000_000_000;

    let buyers = vec![&t.env, t.buyer.clone(), t.buyer2.clone()];

    client.create_shipment(
        &shipment_id,
        &buyers,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://proof"));

    // First buyer approves — payment should NOT release yet
    client.confirm_milestone(&t.buyer, &shipment_id, &0);
    let supplier_balance_after_first = token_client.balance(&t.supplier);
    assert_eq!(supplier_balance_after_first, 0, "payment must not release after only one approval");

    let m0 = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0.status, MilestoneStatus::ProofSubmitted, "status stays ProofSubmitted until all approve");
    assert_eq!(m0.approvals.len(), 1);

    // Second buyer approves — payment releases now
    client.confirm_milestone(&t.buyer2, &shipment_id, &0);
    let expected_payment = total_amount * 25 / 100;
    let supplier_balance_final = token_client.balance(&t.supplier);
    assert_eq!(supplier_balance_final, expected_payment);

    let m0_final = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0_final.status, MilestoneStatus::Confirmed);
}

#[test]
#[should_panic(expected = "buyer already approved this milestone")]
fn test_multisig_duplicate_approval_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-DUP");
    let buyers = vec![&t.env, t.buyer.clone(), t.buyer2.clone()];

    client.create_shipment(
        &shipment_id,
        &buyers,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://proof"));

    client.confirm_milestone(&t.buyer, &shipment_id, &0);
    // Same buyer tries to approve again
    client.confirm_milestone(&t.buyer, &shipment_id, &0);
}

#[test]
fn test_multisig_minority_veto_dispute() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-VETO");
    let buyers = vec![&t.env, t.buyer.clone(), t.buyer2.clone()];

    client.create_shipment(
        &shipment_id,
        &buyers,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://proof"));

    // Only buyer2 raises dispute — minority veto is sufficient
    client.raise_dispute(&t.buyer2, &shipment_id, &0);

    let m0 = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0.status, MilestoneStatus::Disputed);
    // Approvals cleared on dispute
    assert_eq!(m0.approvals.len(), 0);
}

// ============================================================
// FEATURE: SEQUENTIAL vs PARALLEL MILESTONE MODE
// ============================================================

#[test]
#[should_panic(expected = "previous milestone not yet complete")]
fn test_sequential_mode_blocks_out_of_order_proof() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-SEQ");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &build_milestones(&t.env),
        &MilestoneMode::Sequential,
    );

    // Try to submit proof for milestone 1 before milestone 0 is confirmed
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &1,
        &String::from_str(&t.env, "ipfs://skip"),
    );
}

#[test]
fn test_sequential_mode_allows_in_order_proof() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-SEQ-OK");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &build_milestones(&t.env),
        &MilestoneMode::Sequential,
    );

    // Complete milestone 0 first
    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://m0"));
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    // Now milestone 1 is unlocked
    client.submit_proof(&t.supplier, &shipment_id, &1, &String::from_str(&t.env, "ipfs://m1"));
    let m1 = client.get_milestone(&shipment_id, &1);
    assert_eq!(m1.status, MilestoneStatus::ProofSubmitted);
}

#[test]
fn test_parallel_mode_allows_any_order() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-PAR");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    // Submit milestone 2 before 0 or 1 — allowed in Parallel mode
    client.submit_proof(&t.supplier, &shipment_id, &2, &String::from_str(&t.env, "ipfs://m2-first"));
    let m2 = client.get_milestone(&shipment_id, &2);
    assert_eq!(m2.status, MilestoneStatus::ProofSubmitted);
}

// ============================================================
// FEATURE: PROTOCOL FEE COLLECTION
// ============================================================

#[test]
fn test_fee_deducted_on_confirm_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    // Set 1% fee (100 bps)
    client.set_fee_config(&100_u32, &t.treasury);

    let shipment_id = String::from_str(&t.env, "SHIP-FEE");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://proof"));
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    // Milestone 0 = 25% of 1_000_000_000 = 250_000_000
    // Fee = 1% of 250_000_000 = 2_500_000
    // Supplier receives 247_500_000
    let gross_payment: i128 = total_amount * 25 / 100;
    let expected_fee: i128 = gross_payment * 100 / 10_000;
    let expected_net: i128 = gross_payment - expected_fee;

    assert_eq!(token_client.balance(&t.supplier), expected_net);
    assert_eq!(token_client.balance(&t.treasury), expected_fee);
}

#[test]
fn test_zero_fee_bps_no_deduction() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    // fee_bps = 0 — fees disabled
    client.set_fee_config(&0_u32, &t.treasury);

    let shipment_id = String::from_str(&t.env, "SHIP-NOFEE");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://proof"));
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    let expected_payment = total_amount * 25 / 100;
    assert_eq!(token_client.balance(&t.supplier), expected_payment);
    assert_eq!(token_client.balance(&t.treasury), 0);
}

#[test]
#[should_panic(expected = "fee_bps exceeds maximum of 1000 (10%)")]
fn test_fee_bps_exceeds_max_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // 1001 bps > 1000 cap
    client.set_fee_config(&1001_u32, &t.treasury);
}

#[test]
fn test_fee_deducted_on_dispute_resolve_approve() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    // 0.5% fee (50 bps)
    client.set_fee_config(&50_u32, &t.treasury);

    let shipment_id = String::from_str(&t.env, "SHIP-DISPUTE-FEE");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://proof"));
    client.raise_dispute(&t.buyer, &shipment_id, &0);
    client.resolve_dispute(&t.arbiter, &shipment_id, &0, &true);

    let gross: i128 = total_amount * 25 / 100;
    let fee: i128 = gross * 50 / 10_000;
    let net: i128 = gross - fee;

    assert_eq!(token_client.balance(&t.supplier), net);
    assert_eq!(token_client.balance(&t.treasury), fee);
}

#[test]
fn test_no_fee_config_backward_compatible() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    // No set_fee_config call at all
    let shipment_id = String::from_str(&t.env, "SHIP-COMPAT");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &MilestoneMode::Parallel,
    );

    client.submit_proof(&t.supplier, &shipment_id, &0, &String::from_str(&t.env, "ipfs://proof"));
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    // Full payment, no fee
    let expected = total_amount * 25 / 100;
    assert_eq!(token_client.balance(&t.supplier), expected);
    assert_eq!(token_client.balance(&t.treasury), 0);
}
