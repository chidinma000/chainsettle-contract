# Deployment Runbook — ChainSettle Contract

This runbook describes steps for verifying, building, and deploying the ChainSettle Soroban contract to testnet or mainnet, plus post-deploy verification and rollback guidance.

> Intended audience: release engineer, contract maintainer, and on-call operator.

## Pre-deploy checklist

- [ ] All unit and integration tests pass locally (`cargo test -p chainsetttle`).
- [ ] Security review / audit completed and `docs/SECURITY.md` reviewed.
- [ ] WASM size is within network limits (verify `wasm32-unknown-unknown` artifact size).
- [ ] Admin key material stored in secure HSM / hardware wallet and out-of-band escrow.
- [ ] Recovery/emergency contact list available (ops, security, core devs).
- [ ] Release tag created and changelog updated.
- [ ] CI artifacts and checks (lint, formatting) are green.

## Build steps

1. Build the contract target for Soroban:

```bash
# from repo root
cargo build --release -p chainsetttle --target wasm32-unknown-unknown
```

2. Locate the produced WASM (target may vary):

```bash
ls -lh target/wasm32-unknown-unknown/release/*.wasm
```

3. (Optional) Optimize WASM size:

- Use `wasm-opt -Oz` (binaryen) if available.

```bash
wasm-opt -Oz -o chainsettle-opt.wasm target/wasm32-unknown-unknown/release/chainsettle_contract.wasm
```

## Deploy to Testnet (example)

1. Ensure you have the admin key unlocked (Freighter, HSM, or secret file) and network is set to testnet endpoint.
2. Publish the contract with soroban CLI or SDK:

```bash
soroban contract deploy --wasm chainsettle-opt.wasm --network testnet --source <ADMIN_ADDRESS>
```

3. Call `init` right after deployment to set admin/address:

```bash
soroban invoke --wasm chainsettle-opt.wasm --func init --args <ADMIN_ADDRESS> --source <ADMIN_ADDRESS> --network testnet
```

Expected outputs: transaction hash and contract address. Record the deployed contract ID.

## Post-deploy verification

- Verify contract address exists and `get_contract_stats` (or equivalent read endpoint) returns expected metadata.
- Call `get_admin_log` and `is_paused` read functions to ensure admin is set and contract is unpaused.
- Create a small test shipment with a tiny amount and run the happy-path (create, submit_proof, confirm) end-to-end to validate transfers.

Example commands (pseudo):

```bash
# read-only call examples
soroban contract invoke --wasm chainsettle-opt.wasm --func get_admin_log --network testnet
soroban contract invoke --wasm chainsettle-opt.wasm --func is_paused --network testnet
```

## Rollback plan

Because the contract does not currently support on-chain upgrade protections in all deployments, rollback equals redeploying a fixed contract and migrating state where necessary.

1. Identify the last known-good WASM and tag (from CI artifacts).
2. Deploy the known-good WASM to a new contract address.
3. If necessary, run a state migration script (off-chain) to re-create active shipments on the new contract (note: migrating escrow balances requires cooperation from buyers — funds cannot be moved between contract instances without on-chain transfers by the original token holders).
4. Notify users and coordinate a maintenance window.

## Emergency recovery

If there is an immediate threat (compromised admin key, critical bug allowing fund drain), follow the emergency contact list and use the `pause`/`emergency_recover` flow as governed by the contract and governance policy.

## Rollout and monitoring

- After deploy, monitor the following for 24-72 hours:
  - Transaction failure rate for contract invocation
  - Unexpected peaks in `release` events
  - Treasury / fee flows
- Integrate alerts for abnormal outflows or repeated arbiter rejections.

## Contacts & escalation

- Core devs: dev-team@chainsettle.example
- Security: security@chainsettle.example
- Ops/On-call: ops@chainsettle.example


## Notes

- This runbook is intentionally generic. Replace `soroban` CLI examples with your org's deployment automation (CI/CD, Argo, or custodian workflows) as appropriate.
- Ensure admin keys are stored in an HSM or hardware wallet and never kept in plaintext in CI logs.
