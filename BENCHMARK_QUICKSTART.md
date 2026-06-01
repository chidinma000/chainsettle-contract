# Benchmark Quick Start Guide

## 🚀 Getting Started in 3 Steps

### 1. Generate Initial Baselines

```bash
cd contracts/chainsetttle
UPDATE_BASELINES=1 cargo test benchmark_all_functions --release -- --nocapture
```

This will:
- Run all 6 functions with 1 and 10 milestones (12 total benchmarks)
- Measure instruction costs
- Save baselines to `benchmarks/baselines.json`

### 2. Commit the Baselines

```bash
git add ../../benchmarks/baselines.json
git commit -m "chore: add initial benchmark baselines"
git push
```

### 3. Run Benchmarks on Future Changes

```bash
# Check for regressions (fails if >10% increase)
./scripts/run_benchmarks.sh run

# Or manually:
cd contracts/chainsetttle
cargo test benchmark_all_functions --release -- --nocapture
```

## 📊 Understanding the Output

### Successful Run
```
✅ PASS create_shipment (m=1): 1234567 → 1234567 (+0.00%)
✅ PASS submit_proof (m=10): 345678 → 350000 (+1.54%)
```
- Function stayed within 10% threshold
- Small increases (<10%) are acceptable

### Failed Run
```
❌ FAIL confirm_milestone (m=10): 456789 → 550000 (+20.40%)
```
- Function exceeded 10% threshold
- Investigate the cause before updating baselines

## 🔧 Common Commands

```bash
# Run all benchmarks
./scripts/run_benchmarks.sh run

# Update baselines (after intentional changes)
./scripts/run_benchmarks.sh update

# Run specific function benchmark
./scripts/run_benchmarks.sh create    # create_shipment
./scripts/run_benchmarks.sh submit    # submit_proof
./scripts/run_benchmarks.sh confirm   # confirm_milestone
./scripts/run_benchmarks.sh dispute   # raise_dispute
./scripts/run_benchmarks.sh resolve   # resolve_dispute
./scripts/run_benchmarks.sh cancel    # cancel_shipment
```

## 🤔 When to Update Baselines

### ✅ Update When:
- You've optimized code (instructions decrease)
- You've added necessary features (document why instructions increased)
- You've upgraded Soroban SDK

### ❌ Don't Update When:
- CI fails and you don't know why
- You want to "make CI pass" without investigation
- Instructions increased unexpectedly

## 🐛 Troubleshooting

### "No baseline found"
```bash
# Create initial baselines
UPDATE_BASELINES=1 cargo test benchmark_all_functions --release -- --nocapture
git add benchmarks/baselines.json
git commit -m "chore: add benchmark baselines"
```

### Benchmark fails in CI but passes locally
```bash
# Ensure baselines are committed
git add benchmarks/baselines.json
git commit -m "chore: update benchmark baselines"
git push
```

### Unexpected regression
1. Review recent code changes
2. Run individual benchmark to isolate issue:
   ```bash
   ./scripts/run_benchmarks.sh [function-name]
   ```
3. Profile the function to identify bottlenecks
4. Optimize or document why increase is necessary

## 📚 More Information

- **Detailed docs**: [BENCHMARKS.md](./BENCHMARKS.md)
- **Benchmark README**: [benchmarks/README.md](./benchmarks/README.md)
- **Implementation summary**: [BENCHMARK_IMPLEMENTATION_SUMMARY.md](./BENCHMARK_IMPLEMENTATION_SUMMARY.md)

## 💡 Pro Tips

1. **Always run in release mode** for accurate measurements:
   ```bash
   cargo test benchmark_all_functions --release -- --nocapture
   ```

2. **Check benchmarks before committing** to catch regressions early

3. **Document baseline updates** in commit messages:
   ```bash
   git commit -m "chore: update benchmarks after optimization X"
   ```

4. **Review CI output** on every PR to monitor performance trends

---

**Need help?** Check the full documentation in [BENCHMARKS.md](./BENCHMARKS.md)
