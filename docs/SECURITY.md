# Security model & threat analysis

This document describes the security model of the ChainSettle Soroban escrow contract, including trust assumptions, threat vectors, mitigations, and known limitations.

> **Scope**: This covers on-chain logic in `contracts/chainsetttle` (milestones, escrow holds/releases, disputes/arbiter flow, and admin controls).
> **Out of scope**: Off-chain services (backend/indexers), client-side UI, and IPFS content integrity (proofs are treated as opaque hashes).

---

## 1. Trust assumptions

### 1.1 Parties and what they can do

ChainSettle shipments involve:

- **Buyer(s)**: create the shipment (escrow funding), confirm milestones, raise disputes, cancel shipments, and participate in two-party proposals for admin/arbiter changes and milestone amendments.
- **Supplier**: submits shipment proofs for *supplier-owned milestones* and receives payments when milestones are approved/confirmed.
- **Logistics**: submits proofs for *logistics-owned milestones*.
- **Arbiter**: resolves disputes for milestones by either approving (releasing payment) or rejecting (resetting milestone to Pending).
- **Admin**: governance and safety controls (pause/unpause, configurable limits, fee config, blacklist/whitelist, emergency recovery, and optional upgrade/role-transfer flows).

All state-changing functions use Soroban authorization (`require_auth`) for the caller role that must approve the action.

### 1.2 Trust boundaries

- **On-chain escrow trust**: the contract itself is trusted to hold balances at the contract address and to execute transfers only according to its internal state machine.
- **Token trust**: the contract assumes the configured Stellar Asset Contract (e.g., USDC SAC) behaves as an honest token implementation and follows standard `transfer` semantics.
- **Proof trust**: the contract treats `proof_hash` values as opaque identifiers. It does **not** fetch or validate IPFS content. Therefore, correctness of proofs is enforced socially/operationally (buyer confirmation, arbiter judgment), not cryptographically on-chain.
- **Arbiter trust**: the arbiter is trusted to resolve disputes according to off-chain evidence and the agreed business logic. A malicious arbiter can affect outcomes for disputed milestones (see threats).
- **Admin trust**: the admin is trusted to configure and govern safety controls within the intended operational model. Admin has a privileged ability via `emergency_recover` and (depending on deployed configuration) `upgrade`.

---

## 2. Threat vectors & mitigations

The table below lists major threat vectors and how the contract mitigates them.

| Threat vector | What an attacker tries to do | Mitigation(s) implemented in contract | Residual risk / notes |
|---|---|---|---|
| **Reentrancy / re-entrant token callbacks** | Trigger nested execution during token transfers to bypass state checks | Soroban contract execution is synchronous and authorization/state checks occur before transfers; contract updates milestone/shipment state before calling token `transfer`. There is no explicit external call back into this contract as part of state transitions. | If a non-standard token performs unexpected behaviors (or if future code introduces callback patterns), reentrancy-like issues could reappear. Token behavior is assumed standard. |
| **Front-running / transaction ordering** | Observe a pending confirmation/dispute and submit conflicting actions first | `require_auth` binds permissions to specific roles. Milestone status transitions are state-checked (e.g., `confirm_milestone` requires `ProofSubmitted`, `raise_dispute` requires `ProofSubmitted` or `ConfirmedHeld`). Once a milestone status changes, other transactions fail. | An attacker can still race *their own* authorized actions (e.g., buyer may confirm before raising dispute). The protocol assumes parties act honestly and within operational time windows. |
| **Escrow drain via unauthorized transfer** | Attempt to move escrowed funds to an attacker without meeting milestone/arbiter conditions | Funds are only transferred to supplier (or refund to buyer/admin in specific flows) after verifying shipment status, milestone status, and caller permissions. `confirm_milestone`, `release_held_payment`, `resolve_dispute(approve=true)`, and auto-confirmation paths gate transfers by milestone state.

Additionally, the contract tracks `total_escrowed` and decrements it on releases/cancellations. | If an attacker compromises an authorized role key (buyer, arbiter, supplier/logistics, admin), they can trigger the corresponding allowed state transitions. The model assumes signature security. |
| **Arbiter collusion or malicious arbiter** | Use arbiter authority to approve incorrectly (supplier gets paid) or reject correctly (buyer withholds) | `resolve_dispute` is strictly limited to:
- arbiter address must match `shipment.arbiter`
- milestone must be in `Disputed`
- `approve=true` releases payment; `approve=false` resets milestone to `Pending`.

A configurable **dispute bond** can deter frivolous approvals/rejections: on reject, the bond is forfeited to the supplier; on approve, the bond is returned to the buyer.

Optional **dispute cooldown** limits rapid dispute churn. | If arbiter colludes with a party, they can still affect outcomes for disputed milestones; the bond only provides economic deterrence, not cryptographic guarantees. Buyers should choose an arbiter aligned to the dispute process. |
| **TTL / state archival causing stuck funds** | Rely on storage TTL expiry to prevent dispute resolution or releases | The contract uses persistent storage with extended TTL semantics (via storage extend/TTL helpers in code paths) and includes time-based features:
- milestones can be released after `holdback_ledgers`
- auto-confirmation requires `auto_confirm_ledgers` expiry
- emergency recovery is gated by a recovery threshold from `created_at`.

Admin/backend can extend TTL as part of operational procedures (per README security considerations). | If TTL extension is not maintained operationally for long-lived shipments, some entries may become unavailable or may cause operational failure. Emergency recovery can rescue remaining escrow (to admin) but may not follow the original dispute intent. |
| **Front-running arbiter rotation / amendment** | Attempt to change dispute resolution or milestone terms mid-flight | Arbiter rotation and milestone amendments require **both parties** to agree (buyer and supplier; or relevant pair) before applying. Proposals are stored in temporary storage and applied only when both agreement flags match. | Temporary proposal storage may still be subject to TTL/expiration if shipments are long-lived, but it requires explicit participation from both sides. |
| **Admin misuse / privilege escalation** | Admin config changes or emergency recovery to extract funds beyond intended governance | Admin actions are gated by `require_auth` and `assert_admin`.

Mitigations include:
- explicit `pause` switch that stops most state-changing ops
- circuit breaker and limit configuration to reduce outflow risk
- emergency recovery only after `RECOVERY_THRESHOLD_LEDGERS` and only for shipments still in `Active`

Also, admin role transfer uses two-step nomination/acceptance.

| This is not eliminable: admin can execute emergency recovery and (if deployed) upgrade. Operational governance and key management are part of trust assumptions. |

The acceptance criteria for this issue require at least five threat vectors; the first five rows cover: reentrancy, front-running, arbiter collusion, escrow drain, and TTL/expiry.

---

## 3. Mitigation details (by feature)

### 3.1 Authorization model

- Buyer-only actions: `confirm_milestone`, `raise_dispute`, `cancel_shipment`, and amendment proposals as buyer side.
- Supplier/logistics-only actions: `submit_proof` requires caller to equal configured supplier or logistics.
- Arbiter-only actions: `resolve_dispute` requires the caller equals `shipment.arbiter`.
- Admin-only actions: pause/unpause, fee config, circuit breaker, whitelists/blacklists, and emergency recovery.

This eliminates classic “unauthorized state transition → funds transfer” paths.

### 3.2 State machine gating for money movement

Money transfers occur only when the milestone/shipment are in specific statuses:

- Milestone release to supplier:
  - `confirm_milestone` (if no holdback)
  - `release_held_payment` (if holdback expired)
  - `resolve_dispute(approve=true)` (from `Disputed`)
  - `claim_auto_confirmation` (after auto-confirm window)

- Refunds/cancellations:
  - `cancel_shipment` refunds remaining escrow to primary buyer (primary buyer is `buyers[0]`)
  - `supplier_cancel` pays penalty to supplier and refunds remainder to primary buyer
  - `emergency_recover` transfers remaining escrow to admin

### 3.3 Circuit breaker and outflow limiting

When configured, the contract maintains a sliding window of recent outflow and prevents any payment that would exceed `CircuitBreakerLimit` within the current `CircuitBreakerWindow`.

### 3.4 Dispute handling hardening

- Disputes are gated by:
  - shipment status being `Active`
  - milestone status being `ProofSubmitted` or `ConfirmedHeld`
  - open dispute count not exceeding `MaxConcurrentDisputes`
  - optional auto-confirm/dispute windows to reduce race conditions
- Resolution is strictly single-step by arbiter, with bond-based deterrence.

---

## 4. Known limitations & deferred security items

The following limitations are explicitly recognized:

- **No on-chain verification of proofs**: `proof_hash` is only a string/hash and is not validated against IPFS or any external system. The correctness of evidence is assumed to be handled by the dispute process.
- **Admin/upgrade trust model**: the contract contains an `upgrade` entrypoint that can replace the deployed WASM when invoked by the admin. This is a powerful capability and must be governed via operational security and key management.
- **Escrow recovery semantics**: `emergency_recover` transfers remaining escrow to the **admin** address after a fixed ledger threshold, regardless of who should ultimately receive funds under the original commercial agreement.
- **No sequential milestone enforcement when configured**: milestone execution can be `Parallel` (independent) or `Sequential` (previous milestone must be confirmed/resolved). Sequential mode is optional and depends on the shipment `milestone_mode` chosen at creation.
- **Rounding / fee effects**: milestone payments and penalties are integer/bps-based; in edge cases, rounding may result in small discrepancies.
- **No oracle / no external time validation beyond ledger sequence**: timeouts are based on Soroban ledger sequence numbers. There is no oracle or off-chain reconciliation on-chain.
- **Potential operational dependency on TTL extension**: long-lived shipments may require backend/admin TTL extension to ensure persistent storage stays readable.

---

## 5. Responsible disclosure

Security issues should be reported responsibly.

- **Contact**: `security@chainsettle.example` (replace with the project’s actual disclosure email before public release)
- **Reporting guidelines**:
  - Include the affected contract function(s) and a minimal reproduction scenario.
  - Provide attack assumptions (e.g., compromised arbiter key vs. honest-but-curious party).
  - State whether the issue impacts funds safety, liveness, or confidentiality (confidentiality is generally not applicable because data is public on-chain).

---

## 6. Appendix: Key on-chain parameters

Operational security is influenced by these configurable parameters:

- `fee_bps`, `treasury`
- `holdback_ledgers`
- `dispute_cooldown_ledgers`, `MaxConcurrentDisputes`
- `late_penalty_bps_per_ledger`
- `auto_confirm_ledgers`
- `dispute_bond_amount`
- `CircuitBreakerLimit`, `CircuitBreakerWindow`
- `EscalationThreshold`
- optional `response_deadline`, `penalty_bps` in `CancelPolicy`

---

*End of document.*

