# ChainSettle Contract Gas/Instruction Benchmarks

## Overview

This document describes the gas/instruction cost benchmark suite for the ChainSettle smart contract. The suite measures Soroban instruction consumption for all public state-changing functions and enforces regression thresholds in CI.

## Motivation

Tracking instruction costs helps:
- **Prevent performance regressions**: Catch accidental increases in gas costs
- **Optimize contract code**: Identify expensive operations
- **Plan capacity**: Understand resource requirements for different configurations
- **Maintain predictability**: Ensure consistent performance for users

## Benchmarked Functions

All six state-changing public functions are benchmarked:

1. **`create_shipment`** - Creates a new escrow shipment with milestones
2. **`submit_proof`** - Supplier/logistics submits proof of milestone completion
3. **`confirm_milestone`** - Buyer confirms milestone and releases payment
4. **`raise_dispute`** - Buyer raises a dispute on a milestone
5. **`resolve_dispute`** - Arbiter resolves a disputed milestone
6. **`cancel_shipment`** - Buyer cancels shipment and receives refund

## Test Configurations

Each function is tested with two milestone configurations:

- **Minimum**: 1 milestone (simplest case)
- **Maximum**: 10 milestones (typical maximum complexity)

This ensures we capture both best-case and worst-case instruction costs.

## Regression Threshold

**10% increase** is the maximum allowed deviation from baseline.

- ✅ **Pass**: New instructions ≤ baseline × 1.10
- ❌ **Fail**: New instructions > baseline × 1.10

## Running Benchmarks

### Quick Start

```bash
# Run all benchmarks and check for regressions
./scripts/run_benchmarks.sh run

# Update baselines after intentional changes
./scripts/run_benchmarks.sh update
```

### Manual Commands

```bash
# Run all benchmarks
cd contracts/chainsetttle
cargo test benchmark_all_functions --release -- --nocapture

# Update baselines
UPDATE_BASELINES=1 cargo test benchmark_all_functions --release -- --nocapture

# Run individual function benchmarks
cargo test benchmark_create_shipment_only --release -- --nocapture
cargo test benchmark_submit_proof_only --release -- --nocapture
cargo test benchmark_confirm_milestone_only --release -- --nocapture
cargo test benchmark_raise_dispute_only --release -- --nocapture
cargo test benchmark_resolve_dispute_only --release -- --nocapture
cargo test benchmark_cancel_shipment_only --release -- --nocapture
```

## Baseline Management

### Baseline File Location

`benchmarks/baselines.json`

### Baseline File Format

```json
{
  "version": "1.0.0",
  "timestamp": "2024-01-01T00:00:00+00:00",
  "baselines": [
    {
      "function": "create_shipment",
      "milestones": 1,
      "instructions": 1234567
    },
    {
      "function": "create_shipment",
      "milestones": 10,
      "instructions": 2345678
    }
  ]
}
```

### When to Update Baselines

✅ **Update baselines when:**
- Implementing performance optimizations (instructions decrease)
- Adding necessary features (instructions increase, but document why)
- Upgrading Soroban SDK (instruction costs may change)
- Refactoring code that affects instruction counts

❌ **Never update baselines to:**
- Hide regressions without investigation
- "Make CI pass" without understanding why costs increased
- Bypass the 10% threshold without justification

### How to Update Baselines

1. Run benchmarks with update flag:
   ```bash
   ./scripts/run_benchmarks.sh update
   ```

2. Review the changes:
   ```bash
   git diff benchmarks/baselines.json
   ```

3. Commit with explanation:
   ```bash
   git add benchmarks/baselines.json
   git commit -m "chore: update benchmarks after [reason]"
   ```

## CI Integration

### Workflows

The benchmark suite runs in two CI workflows:

1. **`.github/workflows/benchmarks.yml`** - Dedicated benchmark workflow
2. **`.github/workflows/coverage.yml`** - Combined with test coverage

### CI Behavior

- ✅ **Pass**: All functions within 10% of baseline
- ❌ **Fail**: Any function exceeds 10% threshold
- ⚠️ **Warning**: Baseline file missing (must be created)

### CI Output Example

```
🔬 Running ChainSettle Contract Benchmarks
================================================================================

📦 Benchmarking with 1 milestone:
  create_shipment (milestones=1): 1234567 instructions
  submit_proof (milestones=1): 234567 instructions
  confirm_milestone (milestones=1): 345678 instructions
  raise_dispute (milestones=1): 456789 instructions
  resolve_dispute (milestones=1): 567890 instructions
  cancel_shipment (milestones=1): 678901 instructions

📦 Benchmarking with 10 milestones:
  create_shipment (milestones=10): 2345678 instructions
  submit_proof (milestones=10): 345678 instructions
  confirm_milestone (milestones=10): 456789 instructions
  raise_dispute (milestones=10): 567890 instructions
  resolve_dispute (milestones=10): 678901 instructions
  cancel_shipment (milestones=10): 789012 instructions

================================================================================

📊 Regression Check (threshold: +10.0%)
--------------------------------------------------------------------------------
✅ PASS create_shipment (m=1): 1234567 → 1234567 (+0.00%)
✅ PASS create_shipment (m=10): 2345678 → 2345678 (+0.00%)
✅ PASS submit_proof (m=1): 234567 → 234567 (+0.00%)
✅ PASS submit_proof (m=10): 345678 → 345678 (+0.00%)
✅ PASS confirm_milestone (m=1): 345678 → 345678 (+0.00%)
✅ PASS confirm_milestone (m=10): 456789 → 456789 (+0.00%)
✅ PASS raise_dispute (m=1): 456789 → 456789 (+0.00%)
✅ PASS raise_dispute (m=10): 567890 → 567890 (+0.00%)
✅ PASS resolve_dispute (m=1): 567890 → 567890 (+0.00%)
✅ PASS resolve_dispute (m=10): 678901 → 678901 (+0.00%)
✅ PASS cancel_shipment (m=1): 678901 → 678901 (+0.00%)
✅ PASS cancel_shipment (m=10): 789012 → 789012 (+0.00%)
--------------------------------------------------------------------------------

✅ All benchmarks passed!
```

## Implementation Details

### Measurement Approach

The benchmark suite uses Soroban SDK's built-in budget metering:

```rust
fn measure_instructions<F>(env: &Env, f: F) -> u64
where
    F: FnOnce(),
{
    // Reset budget to get clean measurement
    env.budget().reset_unlimited();
    
    // Execute the function
    f();
    
    // Get CPU instructions consumed
    env.budget().cpu_instruction_cost()
}
```

### Test Isolation

Each benchmark:
1. Creates a fresh test environment
2. Sets up necessary state (tokens, accounts, etc.)
3. Measures only the target function
4. Runs in release mode for accurate measurements

### Determinism

Instruction counts are deterministic for the same:
- Soroban SDK version
- Rust compiler version
- Contract code
- Input parameters

Small variations (<1%) may occur due to environment differences, which is why we use a 10% threshold.

## Troubleshooting

### "No baseline found" Error

**Problem**: `benchmarks/baselines.json` doesn't exist or is empty.

**Solution**:
```bash
./scripts/run_benchmarks.sh update
git add benchmarks/baselines.json
git commit -m "chore: add initial benchmark baselines"
```

### Benchmark Fails in CI but Passes Locally

**Problem**: Baseline file not committed or out of sync.

**Solution**:
```bash
# Ensure baseline file is committed
git add benchmarks/baselines.json
git commit -m "chore: update benchmark baselines"
git push
```

### Unexpected Regression

**Problem**: Function exceeds 10% threshold after code changes.

**Investigation Steps**:
1. Review recent code changes
2. Check if new logic was added
3. Profile the function to identify bottlenecks
4. Consider optimization opportunities

**Resolution**:
- If regression is unintentional: optimize code
- If regression is necessary: document reason and update baseline

### Instructions Vary Between Runs

**Problem**: Instruction counts differ slightly between runs.

**Explanation**: Small variations (<1%) are normal due to:
- Test environment differences
- Rust compiler optimizations
- SDK version differences

**Solution**: The 10% threshold accounts for this variance.

## Best Practices

### For Developers

1. **Run benchmarks before committing**: Catch regressions early
2. **Document intentional increases**: Explain why instructions increased
3. **Optimize hot paths**: Focus on frequently-called functions
4. **Test edge cases**: Ensure benchmarks cover realistic scenarios

### For Reviewers

1. **Check baseline changes**: Verify they're justified
2. **Review instruction deltas**: Understand performance impact
3. **Question large increases**: Investigate >5% increases
4. **Approve optimizations**: Celebrate instruction reductions

### For CI/CD

1. **Always run benchmarks**: Include in every PR
2. **Block on regressions**: Don't merge if benchmarks fail
3. **Archive results**: Keep historical data for analysis
4. **Alert on trends**: Monitor gradual increases over time

## Future Enhancements

Potential improvements to the benchmark suite:

- [ ] Memory consumption tracking
- [ ] Storage operation costs
- [ ] Network call overhead
- [ ] Comparative benchmarks (before/after)
- [ ] Historical trend visualization
- [ ] Automated performance reports
- [ ] Per-PR benchmark comparisons
- [ ] Gas cost estimation in USD

## References

- [Soroban Documentation](https://soroban.stellar.org/)
- [Soroban SDK Budget Metering](https://docs.rs/soroban-sdk/latest/soroban_sdk/budget/)
- [ChainSettle Contract Source](./contracts/chainsetttle/src/lib.rs)
- [Benchmark Implementation](./contracts/chainsetttle/src/benchmarks.rs)

## Support

For questions or issues with the benchmark suite:

1. Check this documentation
2. Review [benchmarks/README.md](./benchmarks/README.md)
3. Open an issue on GitHub
4. Contact the ChainSettle team

---

**Last Updated**: 2024-01-01  
**Benchmark Suite Version**: 1.0.0
