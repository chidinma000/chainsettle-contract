# ChainSettle Contract Benchmarks

This directory contains baseline instruction counts for all public contract functions.

## Overview

The benchmark suite measures Soroban instruction consumption for:
- `create_shipment`
- `submit_proof`
- `confirm_milestone`
- `raise_dispute`
- `resolve_dispute`
- `cancel_shipment`

Each function is tested with both minimum (1 milestone) and maximum (10 milestones) configurations.

## Running Benchmarks

### Run all benchmarks and check for regressions
```bash
cd contracts/chainsetttle
cargo test benchmark_all_functions --release -- --nocapture
```

### Update baselines (after intentional changes)
```bash
cd contracts/chainsetttle
UPDATE_BASELINES=1 cargo test benchmark_all_functions --release -- --nocapture
```

### Run individual function benchmarks
```bash
cargo test benchmark_create_shipment_only --release -- --nocapture
cargo test benchmark_submit_proof_only --release -- --nocapture
cargo test benchmark_confirm_milestone_only --release -- --nocapture
cargo test benchmark_raise_dispute_only --release -- --nocapture
cargo test benchmark_resolve_dispute_only --release -- --nocapture
cargo test benchmark_cancel_shipment_only --release -- --nocapture
```

## Regression Threshold

The CI will fail if any function exceeds its baseline by more than **10%**.

## Baseline File

Baselines are stored in `benchmarks/baselines.json` with the following structure:

```json
{
  "version": "1.0.0",
  "timestamp": "2024-01-01T00:00:00Z",
  "baselines": [
    {
      "function": "create_shipment",
      "milestones": 1,
      "instructions": 1000000
    }
  ]
}
```

## CI Integration

The benchmark suite is integrated into CI via `.github/workflows/coverage.yml`. The workflow:
1. Runs all benchmarks
2. Compares results against committed baselines
3. Fails if any function exceeds threshold
4. Prints human-readable results in CI logs

## Interpreting Results

Example output:
```
📊 Regression Check (threshold: +10%)
--------------------------------------------------------------------------------
✅ PASS create_shipment (m=1): 1000000 → 1050000 (+5.00%)
❌ FAIL submit_proof (m=10): 500000 → 600000 (+20.00%)
--------------------------------------------------------------------------------
```

- ✅ PASS: Function is within acceptable range
- ❌ FAIL: Function exceeded 10% threshold (regression detected)
- ⚠️: No baseline found for this function/configuration

## When to Update Baselines

Update baselines when:
- Adding new optimizations (instructions should decrease)
- Adding necessary features (instructions may increase, but document why)
- Upgrading Soroban SDK (instruction costs may change)

**Never** update baselines to hide regressions without investigation.

## Troubleshooting

### "No baseline found" warning
Run `UPDATE_BASELINES=1 cargo test benchmark_all_functions --release -- --nocapture` to create initial baselines.

### Benchmark fails in CI but passes locally
Ensure you've committed the updated `benchmarks/baselines.json` file.

### Instructions vary between runs
Small variations (<1%) are normal. The 10% threshold accounts for this.
