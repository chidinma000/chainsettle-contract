#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    token, vec, Address, Env, IntoVal, String,
};

// ============================================================
// TEST HELPERS
// ============================================================

fn setup() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ChainSettleContract, ());

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    let token_client = token::StellarAssetClient::new(&env, &token_id);

    let buyer = Address::generate(&env);
    let supplier = Address::generate(&env);
    let logistics = Address::generate(&env);
    let arbiter = Address::generate(&env);

    token_client.mint(&buyer, &10_000_000_000);

    let client = ChainSettleContractClient::new(&env, &contract_id);
    client.init(&buyer);

    (env, contract_id, token_id, buyer, supplier, logistics, arbiter)
}

fn build_milestones(env: &Env) -> Vec<Milestone> {
    vec![
        env,
        Milestone {
            name: String::from_str(env, "Goods Dispatched"),
            payment_percent: 25,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
        },
        Milestone {
            name: String::from_str(env, "In Transit"),
            payment_percent: 50,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
        },
        Milestone {
            name: String::from_str(env, "Delivered"),
            payment_percent: 25,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
        },
    ]
}

// ============================================================
// TESTS
// ============================================================

#[test]
fn test_create_shipment_success() {
    let (env, contract_id, token_id, buyer, supplier, logistics, arbiter) = setup();
    let client = ChainSettleContractClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_id);

    let shipment_id = String::from_str(&env, "SHIP-001");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &buyer,
        &supplier,
        &logistics,
        &arbiter,
        &token_id,
        &total_amount,
        &build_milestones(&env),
    );

    let buyer_balance = token_client.balance(&buyer);
    assert_eq!(buyer_balance, 10_000_000_000 - total_amount);

    let escrow_balance = token_client.balance(&contract_id);
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
    let (env, contract_id, token_id, buyer, supplier, logistics, arbiter) = setup();
    let client = ChainSettleContractClient::new(&env, &contract_id);

    let bad_milestones = vec![
        &env,
        Milestone {
            name: String::from_str(&env, "Step 1"),
            payment_percent: 30,
            proof_hash: String::from_str(&env, ""),
            status: MilestoneStatus::Pending,
        },
        Milestone {
            name: String::from_str(&env, "Step 2"),
            payment_percent: 30,
            proof_hash: String::from_str(&env, ""),
            status: MilestoneStatus::Pending,
        },
        Milestone {
            name: String::from_str(&env, "Step 3"),
            payment_percent: 30,
            proof_hash: String::from_str(&env, ""),
            status: MilestoneStatus::Pending,
        },
    ];

    client.create_shipment(
        &String::from_str(&env, "SHIP-BAD"),
        &buyer,
        &supplier,
        &logistics,
        &arbiter,
        &token_id,
        &1_000_000_000,
        &bad_milestones,
    );
}

#[test]
fn test_submit_proof_and_confirm_milestone() {
    let (env, contract_id, token_id, buyer, supplier, logistics, arbiter) = setup();
    let client = ChainSettleContractClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_id);

    let shipment_id = String::from_str(&env, "SHIP-001");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &buyer,
        &supplier,
        &logistics,
        &arbiter,
        &token_id,
        &total_amount,
        &build_milestones(&env),
    );

    client.submit_proof(
        &supplier,
        &shipment_id,
        &0,
        &String::from_str(&env, "ipfs://QmXxx...dispatch"),
    );

    let m0 = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0.status, MilestoneStatus::ProofSubmitted);

    client.confirm_milestone(&buyer, &shipment_id, &0);

    let expected_payment = total_amount * 25 / 100;
    let supplier_balance = token_client.balance(&supplier);
    assert_eq!(supplier_balance, expected_payment);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.released_amount, expected_payment);
    assert_eq!(shipment.status, ShipmentStatus::Active); // not complete yet
}

#[test]
fn test_full_shipment_lifecycle() {
    let (env, contract_id, token_id, buyer, supplier, logistics, arbiter) = setup();
    let client = ChainSettleContractClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_id);

    let shipment_id = String::from_str(&env, "SHIP-FULL");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &buyer,
        &supplier,
        &logistics,
        &arbiter,
        &token_id,
        &total_amount,
        &build_milestones(&env),
    );

    client.submit_proof(&supplier, &shipment_id, &0, &String::from_str(&env, "ipfs://dispatch"));
    client.confirm_milestone(&buyer, &shipment_id, &0);

    client.submit_proof(&logistics, &shipment_id, &1, &String::from_str(&env, "ipfs://transit"));
    client.confirm_milestone(&buyer, &shipment_id, &1);

    client.submit_proof(&supplier, &shipment_id, &2, &String::from_str(&env, "ipfs://delivered"));
    client.confirm_milestone(&buyer, &shipment_id, &2);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Completed);
    assert_eq!(shipment.released_amount, total_amount);

    let supplier_balance = token_client.balance(&supplier);
    assert_eq!(supplier_balance, total_amount);

    let escrow = client.get_escrow_balance(&shipment_id);
    assert_eq!(escrow, 0);
}

#[test]
fn test_raise_and_resolve_dispute_approve() {
    let (env, contract_id, token_id, buyer, supplier, logistics, arbiter) = setup();
    let client = ChainSettleContractClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_id);

    let shipment_id = String::from_str(&env, "SHIP-DISPUTE");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &buyer,
        &supplier,
        &logistics,
        &arbiter,
        &token_id,
        &total_amount,
        &build_milestones(&env),
    );

    client.submit_proof(&supplier, &shipment_id, &0, &String::from_str(&env, "ipfs://proof"));
    client.raise_dispute(&buyer, &shipment_id, &0);

    let m0 = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0.status, MilestoneStatus::Disputed);

    client.resolve_dispute(&arbiter, &shipment_id, &0, &true);

    let expected = total_amount * 25 / 100;
    let supplier_balance = token_client.balance(&supplier);
    assert_eq!(supplier_balance, expected);
}

#[test]
fn test_raise_and_resolve_dispute_reject() {
    let (env, contract_id, token_id, buyer, supplier, logistics, arbiter) = setup();
    let client = ChainSettleContractClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_id);

    let shipment_id = String::from_str(&env, "SHIP-REJECT");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &buyer,
        &supplier,
        &logistics,
        &arbiter,
        &token_id,
        &total_amount,
        &build_milestones(&env),
    );

    client.submit_proof(&supplier, &shipment_id, &0, &String::from_str(&env, "ipfs://bad-proof"));
    client.raise_dispute(&buyer, &shipment_id, &0);
    client.resolve_dispute(&arbiter, &shipment_id, &0, &false);

    let m0 = client.get_milestone(&shipment_id, &0);
    assert_eq!(m0.status, MilestoneStatus::Pending);

    let supplier_balance = token_client.balance(&supplier);
    assert_eq!(supplier_balance, 0);
}

#[test]
fn test_cancel_shipment() {
    let (env, contract_id, token_id, buyer, supplier, logistics, arbiter) = setup();
    let client = ChainSettleContractClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_id);

    let shipment_id = String::from_str(&env, "SHIP-CANCEL");
    let total_amount: i128 = 1_000_000_000;
    let buyer_balance_before = token_client.balance(&buyer);

    client.create_shipment(
        &shipment_id,
        &buyer,
        &supplier,
        &logistics,
        &arbiter,
        &token_id,
        &total_amount,
        &build_milestones(&env),
    );

    client.cancel_shipment(&buyer, &shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);

    let buyer_balance_after = token_client.balance(&buyer);
    assert_eq!(buyer_balance_after, buyer_balance_before);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_unauthorized_confirm_milestone() {
    let (env, contract_id, token_id, buyer, supplier, logistics, arbiter) = setup();
    let client = ChainSettleContractClient::new(&env, &contract_id);

    let shipment_id = String::from_str(&env, "SHIP-AUTH");

    client.create_shipment(
        &shipment_id,
        &buyer,
        &supplier,
        &logistics,
        &arbiter,
        &token_id,
        &1_000_000_000,
        &build_milestones(&env),
    );

    client.submit_proof(&supplier, &shipment_id, &0, &String::from_str(&env, "ipfs://proof"));

    // Supplier tries to confirm — should panic
    client.confirm_milestone(&supplier, &shipment_id, &0);
}
