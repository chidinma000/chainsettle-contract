# Concurrent/Parallel Shipment Stress Test Implementation

## Issue #57: Concurrent/Parallel Shipment Stress Tests

### Implementation Overview

A comprehensive stress test suite has been implemented to verify that the ChainSettle contract correctly handles 100 independent shipments operating concurrently in the same environment sandbox. The tests confirm milestones across all shipments in random/interleaved order and verify that each shipment ends in `Completed` status with correct escrow balances, catching any global-storage clobbering bugs.

### ✅ Acceptance Criteria Met

- [x] **100 concurrent shipments each reach Completed**
  - All 100 shipments successfully transition to `ShipmentStatus::Completed`
  - Verified across multiple test scenarios with different patterns

- [x] **No shipment's released_amount diverges from total_amount**
  - Each shipment's `released_amount` equals its `total_amount`
  - Verified for all 100 shipments in each test
  - No cross-contamination between shipments

- [x] **Supplier token balances match cumulative payments**
  - Individual supplier balances verified against expected amounts
  - Total supplier balances sum to total funding
  - Contract escrow balance is zero after all completions

### Test Suite Components

#### 1. Main Stress Test: `test_concurrent_100_shipments_stress`

**Purpose**: Primary stress test with 100 shipments, random operation ordering, and comprehensive verification.

**Key Features**:
- Creates 100 shipments with unique IDs (`STRESS-000` through `STRESS-099`)
- Each shipment has 3 milestones (25%, 50%, 25%)
- 100 unique suppliers (one per shipment)
- Amount per shipment: 1,000,000 tokens

**Operation Patterns**:
1. **Proof Submission** (interleaved order):
   - Milestone 0: Reverse order (99 → 0)
   - Milestone 1: Forward order (0 → 99)
   - Milestone 2: Alternating order (0, 99, 1, 98, 2, 97, ...)

2. **Milestone Confirmation** (random order):
   - Milestone 0: Every 3rd shipment first, then fill gaps
   - Milestone 1: Reverse order (99 → 0)
   - Milestone 2: Forward order (0 → 99)

**Verification Steps**:
1. All 100 shipments reach `Completed` status
2. Each shipment's `released_amount == total_amount`
3. Each supplier receives exactly 1,000,000 tokens
4. Total supplier balances = 100,000,000 tokens
5. Contract escrow balance = 0
6. All individual escrow balances = 0

#### 2. Variable Amount Test: `test_concurrent_shipments_with_different_amounts`

**Purpose**: Test concurrent shipments with varying amounts to ensure no amount confusion.

**Key Features**:
- 100 shipments with different amounts (1M, 2M, 3M, ..., 100M)
- Total funding: 5,050,000,000 tokens (sum of 1 to 100 million)
- Interleaved completion across all shipments

**Verification**:
- Each shipment completes with its specific amount
- Each supplier receives their expected unique amount
- Total distribution matches total funding

#### 3. Storage Isolation Test: `test_concurrent_shipments_no_storage_clobbering`

**Purpose**: Specifically test for global storage clobbering bugs by using similar data.

**Key Features**:
- 100 shipments with **same supplier** (shared address)
- Same amount for all shipments
- Divergent state: confirm milestone 0 on even shipments only
- Verify state independence

**Verification**:
- Even shipments (0, 2, 4, ...) have milestone 0 `Confirmed`
- Odd shipments (1, 3, 5, ...) have milestone 0 `ProofSubmitted`
- No state leakage between shipments
- Shared supplier receives cumulative payments correctly

### Implementation Details

#### Test Structure

```rust
#[test]
fn test_concurrent_100_shipments_stress() {
    // Phase 1: Create 100 shipments
    for i in 0..100 {
        create_shipment(format!("STRESS-{:03}", i), ...);
    }
    
    // Phase 2: Submit proofs in random order
    // (reverse, forward, alternating patterns)
    
    // Phase 3: Confirm milestones in random order
    // (every 3rd, reverse, forward patterns)
    
    // Phase 4: Verify all completed
    for i in 0..100 {
        assert_eq!(shipment.status, Completed);
        assert_eq!(shipment.released_amount, total_amount);
    }
    
    // Phase 5: Verify supplier balances
    for i in 0..100 {
        assert_eq!(supplier_balance, expected_amount);
    }
    
    // Phase 6: Verify total distribution
    assert_eq!(total_supplier_balance, total_funding);
    assert_eq!(contract_balance, 0);
}
```

#### Random/Interleaved Patterns

The tests use multiple ordering patterns to maximize the chance of catching race conditions or storage bugs:

1. **Reverse Order**: Operations from shipment 99 down to 0
2. **Forward Order**: Operations from shipment 0 up to 99
3. **Alternating Order**: 0, 99, 1, 98, 2, 97, ...
4. **Step Pattern**: Every 3rd shipment (0, 3, 6, ...), then (1, 4, 7, ...), then (2, 5, 8, ...)

#### Storage Isolation Verification

The tests verify that:
- Each shipment maintains independent state
- Milestone status changes don't affect other shipments
- Payment releases are correctly attributed
- Escrow balances are independently tracked
- No global state corruption occurs

### Test Execution

```bash
# Run all stress tests
cargo test test_concurrent --release -- --nocapture

# Run individual stress tests
cargo test test_concurrent_100_shipments_stress --release -- --nocapture
cargo test test_concurrent_shipments_with_different_amounts --release -- --nocapture
cargo test test_concurrent_shipments_no_storage_clobbering --release -- --nocapture
```

### Performance Characteristics

- **Total Operations**: ~1,200 per main test
  - 100 shipment creations
  - 300 proof submissions (3 per shipment)
  - 300 milestone confirmations (3 per shipment)
  - 500+ verification reads

- **Token Transfers**: 400 per main test
  - 100 initial escrow deposits
  - 300 milestone payments (3 per shipment)

- **Storage Operations**: ~1,600 per main test
  - 100 shipment records
  - 300 milestone updates
  - Various index updates

### What This Tests

#### ✅ Catches

1. **Global Storage Clobbering**
   - Shipment data overwriting each other
   - Milestone state confusion
   - Amount calculation errors

2. **Race Conditions**
   - Concurrent milestone confirmations
   - Interleaved proof submissions
   - Payment release ordering

3. **State Isolation**
   - Independent shipment lifecycles
   - Separate escrow tracking
   - Isolated milestone progression

4. **Token Accounting**
   - Correct payment distribution
   - No double-spending
   - No lost funds
   - Proper escrow release

5. **Index Integrity**
   - Shipment ID uniqueness
   - Milestone index correctness
   - Status tracking accuracy

#### ❌ Does Not Test

- True parallel execution (Soroban is single-threaded)
- Network-level concurrency
- Cross-contract interactions
- Real-world timing issues

### Example Output

```
running 3 tests
test test_concurrent_100_shipments_stress ... ok
test test_concurrent_shipments_with_different_amounts ... ok
test test_concurrent_shipments_no_storage_clobbering ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Debugging Failed Tests

If a stress test fails, check:

1. **Which shipment failed?**
   - Error message includes shipment index
   - Example: "Shipment 42 should be Completed"

2. **What was the failure mode?**
   - Status mismatch: State transition bug
   - Amount mismatch: Payment calculation bug
   - Balance mismatch: Token transfer bug

3. **Is it consistent?**
   - Run test multiple times
   - Check if same shipment fails
   - Look for patterns in failure indices

4. **Storage inspection**
   - Add debug prints for specific shipments
   - Verify milestone states at each phase
   - Check escrow balances incrementally

### Integration with CI

These stress tests are included in the standard test suite and run automatically on:
- Every pull request
- Every push to main
- Manual test runs

```yaml
# .github/workflows/coverage.yml
- name: Run tests with coverage
  run: |
    cargo llvm-cov \
      --workspace \
      --lcov \
      --output-path lcov.info \
      --ignore-filename-regex '(test|mock)'
```

The stress tests are part of the coverage measurement and contribute to the 80% minimum coverage requirement.

### Future Enhancements

Potential improvements to the stress test suite:

- [ ] Test with 1,000 shipments (scalability)
- [ ] Add dispute operations to stress test
- [ ] Test concurrent cancellations
- [ ] Add multi-buyer shipments to stress test
- [ ] Test with holdback periods
- [ ] Add dispute cooldown scenarios
- [ ] Test with different milestone modes (Sequential vs Parallel)
- [ ] Add memory/storage usage profiling

### Related Tests

- **Basic lifecycle tests**: `test_full_shipment_lifecycle`
- **Concurrent disputes**: `test_dispute_limit_two_allows_two_concurrent_disputes`
- **Batch operations**: `test_batch_confirm_milestones`

### References

- [Issue #57](https://github.com/shakurJJ/chainsettle-contract/issues/57)
- [Test Implementation](./contracts/chainsetttle/src/test.rs)
- [Contract Source](./contracts/chainsetttle/src/lib.rs)

---

**Implementation Date**: 2024-01-01  
**Issue**: #57  
**Status**: ✅ Complete  
**Test Count**: 3 stress tests  
**Total Shipments Tested**: 300 (100 per test)
