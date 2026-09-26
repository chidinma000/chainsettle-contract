// #521 — Warranty holdback period after shipment completion.
//
// `warranty_bps` of every net milestone payment stays in escrow after completion
// until `completed_ledger + warranty_ledgers`. The buyer may file a warranty claim
// in that window (resolved by the arbiter); otherwise anyone may call
// `release_warranty` to pay the supplier once the period ends.

#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec, TestSetup};
use soroban_sdk::{
    testutils::{Events as _, Ledger as _},
    token, BytesN, String, Symbol, TryFromVal,
};

const TOTAL: i128 = 1_000_000;
const WARRANTY_LEDGERS: u32 = 100;
// 5% of every net payment → 50_000 on a fee-free 1_000_000 shipment.
const WARRANTY_AMOUNT: i128 = 50_000;

fn warranty_options(t: &TestSetup) -> ShipmentOptions {
    let mut opts = default_options(&t.env);
    opts.warranty_bps = 500;
    opts.warranty_ledgers = WARRANTY_LEDGERS;
    opts
}

fn create(t: &TestSetup, id: &str, opts: &ShipmentOptions) -> String {
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = String::from_str(&t.env, id);
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &TOTAL,
        &build_milestones(&t.env),
        opts,
    );
    shipment_id
}

fn complete(t: &TestSetup, shipment_id: &String) {
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    for i in 0..3u32 {
        client.submit_proof(
            &t.supplier,
            shipment_id,
            &i,
            &String::from_str(&t.env, "proof"),
            &Symbol::new(&t.env, "ipfs"),
        );
        client.confirm_milestone(&t.buyer, shipment_id, &i);
    }
}

fn completed_shipment(t: &TestSetup, id: &str) -> String {
    let shipment_id = create(t, id, &warranty_options(t));
    complete(t, &shipment_id);
    shipment_id
}

fn balance(t: &TestSetup, addr: &Address) -> i128 {
    token::Client::new(&t.env, &t.token_id).balance(addr)
}

fn evidence(t: &TestSetup) -> BytesN<32> {
    BytesN::from_array(&t.env, &[7u8; 32])
}

fn advance_to_warranty_end(t: &TestSetup, shipment_id: &String) {
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let end = client.get_warranty_end_ledger(shipment_id);
    t.env.ledger().with_mut(|l| l.sequence_number = end);
}

fn has_event(t: &TestSetup, topic: &str) -> bool {
    let wanted = Symbol::new(&t.env, topic);
    t.env.events().all().iter().any(|e| {
        e.1.get(0)
            .and_then(|v| Symbol::try_from_val(&t.env, &v).ok())
            .map(|s| s == wanted)
            .unwrap_or(false)
    })
}

#[test]
fn test_warranty_amount_stays_in_escrow_until_period_ends() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let before = balance(&t, &t.supplier);
    let id = completed_shipment(&t, "WAR-HOLD");

    assert_eq!(client.get_shipment(&id).status, ShipmentStatus::Completed);
    assert_eq!(balance(&t, &t.supplier) - before, TOTAL - WARRANTY_AMOUNT);
    assert_eq!(client.get_warranty_balance(&id), WARRANTY_AMOUNT);
    assert_eq!(
        client.get_warranty_end_ledger(&id),
        t.env.ledger().sequence() + WARRANTY_LEDGERS
    );
    assert_eq!(client.get_total_escrowed_value(&t.token_id), WARRANTY_AMOUNT);
}

#[test]
fn test_release_after_period_pays_exact_warranty_amount() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-RELEASE");

    advance_to_warranty_end(&t, &id);
    let before = balance(&t, &t.supplier);
    client.release_warranty(&id);
    assert!(has_event(&t, "warranty_released"));

    assert_eq!(balance(&t, &t.supplier) - before, WARRANTY_AMOUNT);
    assert_eq!(client.get_warranty_balance(&id), 0);
    assert_eq!(client.get_total_escrowed_value(&t.token_id), 0);
}

#[test]
#[should_panic(expected = "warranty period not yet ended")]
fn test_release_before_period_end_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-EARLY");
    client.release_warranty(&id);
}

#[test]
fn test_claim_in_time_blocks_release_until_resolved() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-BLOCK");

    client.file_warranty_claim(&t.buyer, &id, &evidence(&t));
    assert!(has_event(&t, "warranty_claim_filed"));
    let claim = client.get_warranty_claim(&id).unwrap();
    assert_eq!(claim.buyer, t.buyer);

    advance_to_warranty_end(&t, &id);
    assert!(client.try_release_warranty(&id).is_err());

    // Arbiter dismisses the claim; release is then possible.
    client.resolve_warranty_claim(&t.arbiter, &id, &false);
    assert!(client.get_warranty_claim(&id).is_none());
    let before = balance(&t, &t.supplier);
    client.release_warranty(&id);
    assert_eq!(balance(&t, &t.supplier) - before, WARRANTY_AMOUNT);
}

#[test]
fn test_upheld_claim_refunds_buyer() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-UPHELD");

    client.file_warranty_claim(&t.buyer, &id, &evidence(&t));
    let before = balance(&t, &t.buyer);
    client.resolve_warranty_claim(&t.arbiter, &id, &true);
    assert!(has_event(&t, "warranty_claim_resolved"));

    assert_eq!(balance(&t, &t.buyer) - before, WARRANTY_AMOUNT);
    assert_eq!(client.get_warranty_balance(&id), 0);

    let log = client.get_shipment(&id).audit_log;
    let last = log.get(log.len() - 1).unwrap();
    assert_eq!(last.action, Symbol::new(&t.env, "warranty_claim_resolved"));
}

#[test]
#[should_panic(expected = "warranty period has ended")]
fn test_claim_after_period_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-LATE");
    advance_to_warranty_end(&t, &id);
    client.file_warranty_claim(&t.buyer, &id, &evidence(&t));
}

#[test]
#[should_panic(expected = "warranty claim already open")]
fn test_duplicate_claim_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-DUP");
    client.file_warranty_claim(&t.buyer, &id, &evidence(&t));
    client.file_warranty_claim(&t.buyer, &id, &evidence(&t));
}

#[test]
#[should_panic(expected = "invalid evidence hash")]
fn test_zero_evidence_hash_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-EVID");
    client.file_warranty_claim(&t.buyer, &id, &BytesN::from_array(&t.env, &[0u8; 32]));
}

#[test]
#[should_panic(expected = "warranty claims require a completed shipment")]
fn test_claim_requires_completed_shipment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "WAR-ACTIVE", &warranty_options(&t));
    client.file_warranty_claim(&t.buyer, &id, &evidence(&t));
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_non_buyer_cannot_file_claim() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-AUTH1");
    client.file_warranty_claim(&t.supplier, &id, &evidence(&t));
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_non_arbiter_cannot_resolve_claim() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-AUTH2");
    client.file_warranty_claim(&t.buyer, &id, &evidence(&t));
    client.resolve_warranty_claim(&t.buyer, &id, &true);
}

#[test]
fn test_claim_and_resolution_require_signatures() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = completed_shipment(&t, "WAR-SIG");

    t.env.set_auths(&[]);
    assert!(client
        .try_file_warranty_claim(&t.buyer, &id, &evidence(&t))
        .is_err());

    t.env.mock_all_auths();
    client.file_warranty_claim(&t.buyer, &id, &evidence(&t));

    t.env.set_auths(&[]);
    assert!(client
        .try_resolve_warranty_claim(&t.arbiter, &id, &true)
        .is_err());
}

#[test]
fn test_cancellation_refunds_withheld_warranty_to_buyer() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "WAR-CANCEL", &warranty_options(&t));
    client.submit_proof(
        &t.supplier,
        &id,
        &0u32,
        &String::from_str(&t.env, "proof"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &id, &0u32);
    assert_eq!(client.get_warranty_balance(&id), 12_500);

    let before = balance(&t, &t.buyer);
    client.cancel_shipment(&t.buyer, &id);
    assert_eq!(balance(&t, &t.buyer) - before, 750_000 + 12_500);
    assert_eq!(client.get_warranty_balance(&id), 0);
}

#[test]
#[should_panic(expected = "warranty_bps and warranty_ledgers must both be set")]
fn test_creation_rejects_warranty_without_period() {
    let t = setup();
    let mut opts = warranty_options(&t);
    opts.warranty_ledgers = 0;
    create(&t, "WAR-CFG", &opts);
}

#[test]
#[should_panic(expected = "warranty_bps exceeds maximum")]
fn test_creation_rejects_warranty_above_max() {
    let t = setup();
    let mut opts = warranty_options(&t);
    opts.warranty_bps = 2_001;
    create(&t, "WAR-MAX", &opts);
}
