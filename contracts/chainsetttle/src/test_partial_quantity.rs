// #518 — Quantity-based pro-rata milestone payment for partial deliveries.
//
// A milestone with an `expected_quantity` releases `gross * delivered / expected`
// per `confirm_partial_quantity` call. The call that delivers the final unit
// releases the remainder (including rounding dust) and confirms the milestone.

#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec, TestSetup};
use soroban_sdk::{testutils::Events as _, token, vec, String, Symbol, TryFromVal};

const TOTAL: i128 = 1_000_000;
// Milestone 0 is 25% of TOTAL.
const M0_GROSS: i128 = 250_000;

fn quantity_options(t: &TestSetup, expected: u32) -> ShipmentOptions {
    let mut opts = default_options(&t.env);
    opts.milestone_quantities = vec![&t.env, expected, 0u32, 0u32];
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
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0u32,
        &String::from_str(&t.env, "proof"),
        &Symbol::new(&t.env, "ipfs"),
    );
    shipment_id
}

fn balance(t: &TestSetup, addr: &Address) -> i128 {
    token::Client::new(&t.env, &t.token_id).balance(addr)
}

#[test]
fn test_partial_releases_are_pro_rata_and_dust_goes_to_final_unit() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "QTY-PRORATA", &quantity_options(&t, 3));

    let mut paid = std::vec::Vec::new();
    for _ in 0..3 {
        let before = balance(&t, &t.supplier);
        client.confirm_partial_quantity(&t.buyer, &id, &0u32, &1u32);
        paid.push(balance(&t, &t.supplier) - before);
    }

    // 250_000 / 3 = 83_333.33… → 83_333, 83_333, then 83_334 with the dust.
    assert_eq!(paid, std::vec![83_333, 83_333, 83_334]);
    assert_eq!(paid.iter().sum::<i128>(), M0_GROSS);

    assert_eq!(client.get_delivered_quantity(&id, &0u32), (3u32, 3u32));
    let s = client.get_shipment(&id);
    assert_eq!(s.milestones.get(0).unwrap().status, MilestoneStatus::Confirmed);
    assert_eq!(s.released_amount, M0_GROSS);
}

#[test]
fn test_milestone_stays_open_until_expected_quantity_reached() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "QTY-OPEN", &quantity_options(&t, 10));

    client.confirm_partial_quantity(&t.buyer, &id, &0u32, &4u32);
    let m = client.get_milestone(&id, &0u32);
    assert_eq!(m.status, MilestoneStatus::ProofSubmitted);
    assert_eq!(client.get_delivered_quantity(&id, &0u32), (4u32, 10u32));
    assert_eq!(client.get_shipment(&id).released_amount, 100_000);
    assert_eq!(client.get_completion_percentage(&id), 10);
    assert_eq!(client.get_escrow_balance(&id), TOTAL - 100_000);

    client.confirm_partial_quantity(&t.buyer, &id, &0u32, &6u32);
    assert_eq!(
        client.get_milestone(&id, &0u32).status,
        MilestoneStatus::Confirmed
    );
    assert_eq!(client.get_shipment(&id).released_amount, M0_GROSS);
}

#[test]
fn test_full_confirmation_after_partial_pays_only_remainder() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "QTY-REST", &quantity_options(&t, 4));

    let before = balance(&t, &t.supplier);
    client.confirm_partial_quantity(&t.buyer, &id, &0u32, &1u32);
    // Buyer accepts the rest in one go.
    client.confirm_milestone(&t.buyer, &id, &0u32);

    assert_eq!(balance(&t, &t.supplier) - before, M0_GROSS);
    assert_eq!(client.get_shipment(&id).released_amount, M0_GROSS);
}

#[test]
fn test_fees_applied_to_each_partial_release() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_fee_config(&t.buyer, &1_000u32, &t.treasury); // 10%
    let id = create(&t, "QTY-FEES", &quantity_options(&t, 4));

    for _ in 0..3 {
        let supplier_before = balance(&t, &t.supplier);
        let treasury_before = balance(&t, &t.treasury);
        client.confirm_partial_quantity(&t.buyer, &id, &0u32, &1u32);
        // Each unit releases 62_500 gross: 6_250 fee, 56_250 to the supplier.
        assert_eq!(balance(&t, &t.treasury) - treasury_before, 6_250);
        assert_eq!(balance(&t, &t.supplier) - supplier_before, 56_250);
    }

    let treasury_before = balance(&t, &t.treasury);
    client.confirm_partial_quantity(&t.buyer, &id, &0u32, &1u32);
    assert_eq!(balance(&t, &t.treasury) - treasury_before, 6_250);
}

#[test]
fn test_partial_quantity_emits_event_and_audit_entry() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "QTY-EVENT", &quantity_options(&t, 5));

    client.confirm_partial_quantity(&t.buyer, &id, &0u32, &2u32);

    let wanted = Symbol::new(&t.env, "partial_quantity_confirmed");
    let event = t
        .env
        .events()
        .all()
        .iter()
        .find(|e| {
            e.1.get(0)
                .and_then(|v| Symbol::try_from_val(&t.env, &v).ok())
                .map(|s| s == wanted)
                .unwrap_or(false)
        })
        .expect("partial_quantity_confirmed event");
    let data: (u32, u32, u32, u32, i128, i128) =
        <(u32, u32, u32, u32, i128, i128)>::try_from_val(&t.env, &event.2).unwrap();
    assert_eq!(data, (0, 2, 2, 5, 100_000, 0));

    let log = client.get_shipment(&id).audit_log;
    let last = log.get(log.len() - 1).unwrap();
    assert_eq!(last.action, Symbol::new(&t.env, "partial_qty_confirmed"));
}

#[test]
#[should_panic(expected = "delivered quantity exceeds remaining quantity")]
fn test_confirming_more_than_remaining_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "QTY-OVER", &quantity_options(&t, 3));
    client.confirm_partial_quantity(&t.buyer, &id, &0u32, &2u32);
    client.confirm_partial_quantity(&t.buyer, &id, &0u32, &2u32);
}

#[test]
#[should_panic(expected = "delivered quantity must be positive")]
fn test_zero_quantity_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "QTY-ZERO", &quantity_options(&t, 3));
    client.confirm_partial_quantity(&t.buyer, &id, &0u32, &0u32);
}

#[test]
#[should_panic(expected = "milestone is not quantity-based")]
fn test_non_quantity_milestone_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "QTY-NONE", &default_options(&t.env));
    client.confirm_partial_quantity(&t.buyer, &id, &0u32, &1u32);
}

#[test]
#[should_panic(expected = "milestone quantity count must match milestone count")]
fn test_creation_rejects_mismatched_quantity_count() {
    let t = setup();
    let mut opts = default_options(&t.env);
    opts.milestone_quantities = vec![&t.env, 5u32];
    create(&t, "QTY-CFG", &opts);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_non_buyer_cannot_confirm_quantity() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "QTY-AUTH", &quantity_options(&t, 3));
    client.confirm_partial_quantity(&t.supplier, &id, &0u32, &1u32);
}

#[test]
fn test_quantity_confirmation_requires_buyer_signature() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let id = create(&t, "QTY-SIG", &quantity_options(&t, 3));

    t.env.set_auths(&[]);
    assert!(client
        .try_confirm_partial_quantity(&t.buyer, &id, &0u32, &1u32)
        .is_err());
    // The final-unit path must be protected too.
    assert!(client
        .try_confirm_partial_quantity(&t.buyer, &id, &0u32, &3u32)
        .is_err());
}
