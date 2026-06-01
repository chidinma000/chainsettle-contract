# Issue #73 Implementation Plan: Replace panic! with Typed Errors

## Overview

Replace all `panic!()` calls in the contract with typed `ChainSettleError` enum returns using Soroban's `Result` pattern. This makes errors programmatically distinguishable by clients.

## Current State Analysis

### Existing Error Enum

```rust
#[contracttype]
#[derive(Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum ChainSettleError {
    ShipmentAlreadyExists = 1,
    ShipmentNotFound = 2,
    Unauthorized = 3,
    InvalidMilestoneIndex = 4,
    InvalidMilestoneStatus = 5,
    ShipmentNotActive = 6,
    InvalidPercentages = 7,
    InvalidAmount = 8,
    DisputeAlreadyOpen = 9,
    DeadlineNotBreached = 10,
    FeeTooHigh = 11,
    PreviousMilestoneNotComplete = 12,
    ContractPaused = 13,
    DisputeCooldownActive = 14,
    TransferDisallowed = 15,
    CircuitBreakerTripped = 16,
}
```

### Panic Categories Found

1. **Unauthorized** (~15 occurrences)
   - Admin checks
   - Buyer/supplier/arbiter validation
   - Blacklist checks

2. **Invalid Parameters** (~10 occurrences)
   - Empty buyers list
   - Zero/negative amounts
   - Invalid percentages
   - Invalid milestone indices

3. **State Validation** (~20 occurrences)
   - Shipment not active
   - Wrong milestone status
   - Dispute state conflicts

4. **Business Logic** (~10 occurrences)
   - Cooldown periods
   - Deadline checks
   - Auto-confirmation conflicts

## Required Changes

### 1. Add Missing Error Variants

```rust
#[contracttype]
#[derive(Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum ChainSettleError {
    // Existing...
    ShipmentAlreadyExists = 1,
    ShipmentNotFound = 2,
    Unauthorized = 3,
    InvalidMilestoneIndex = 4,
    InvalidMilestoneStatus = 5,
    ShipmentNotActive = 6,
    InvalidPercentages = 7,
    InvalidAmount = 8,
    DisputeAlreadyOpen = 9,
    DeadlineNotBreached = 10,
    FeeTooHigh = 11,
    PreviousMilestoneNotComplete = 12,
    ContractPaused = 13,
    DisputeCooldownActive = 14,
    TransferDisallowed = 15,
    CircuitBreakerTripped = 16,
    
    // New variants needed:
    EmptyBuyersList = 17,
    MaxShipmentValueExceeded = 18,
    InvalidMultiSigParameters = 19,
    MultisigNotConfigured = 20,
    AlreadyApproved = 21,
    InvalidMinMilestonePercent = 22,
    TopUpNotAllowed = 23,
    ProofNotSubmitted = 24,
    AutoConfirmed = 25,
    HoldbackNotExpired = 26,
    CannotDisputeStatus = 27,
    DisputeWindowClosed = 28,
    MilestoneNotDisputed = 29,
    DisputeMustBeResolved = 30,
    SupplierCancellationDisabled = 31,
    AmendmentNotPending = 32,
    ArbiterRotationNotPending = 33,
    InvalidProposalStatus = 34,
    RecoveryThresholdNotMet = 35,
}
```

### 2. Update Function Signatures

**Before:**
```rust
pub fn create_shipment(
    env: Env,
    shipment_id: String,
    // ... parameters
) -> String
```

**After:**
```rust
pub fn create_shipment(
    env: Env,
    shipment_id: String,
    // ... parameters
) -> Result<String, ChainSettleError>
```

### 3. Replace panic! Patterns

**Pattern 1: Simple panic**
```rust
// Before
if buyers.is_empty() {
    panic!("at least one buyer is required");
}

// After
if buyers.is_empty() {
    return Err(ChainSettleError::EmptyBuyersList);
}
```

**Pattern 2: unwrap_or_else panic**
```rust
// Before
let admin: Address = env.storage().instance()
    .get(&DataKey::Admin)
    .unwrap_or_else(|| panic!("unauthorized"));

// After
let admin: Address = env.storage().instance()
    .get(&DataKey::Admin)
    .ok_or(ChainSettleError::Unauthorized)?;
```

**Pattern 3: Helper function panics**
```rust
// Before
fn assert_admin(env: &Env, admin: &Address) {
    let stored: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic!("unauthorized"));
    if admin != &stored {
        panic!("unauthorized");
    }
}

// After
fn assert_admin(env: &Env, admin: &Address) -> Result<(), ChainSettleError> {
    let stored: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(ChainSettleError::Unauthorized)?;
    if admin != &stored {
        return Err(ChainSettleError::Unauthorized);
    }
    Ok(())
}
```

### 4. Update Helper Functions

All helper functions that currently panic must return `Result`:

- `assert_admin()` → `Result<(), ChainSettleError>`
- `assert_not_paused()` → `Result<(), ChainSettleError>`
- `assert_is_buyer()` → `Result<(), ChainSettleError>`
- `get_shipment_or_panic()` → `get_shipment() -> Result<Shipment, ChainSettleError>`

### 5. Update Test Assertions

**Before:**
```rust
#[test]
#[should_panic(expected = "unauthorized")]
fn test_unauthorized() {
    // ... test code
}
```

**After:**
```rust
#[test]
fn test_unauthorized() {
    let result = client.try_some_function(...);
    assert_eq!(result, Err(Ok(ChainSettleError::Unauthorized)));
}
```

Note: Soroban wraps contract errors in `Err(Ok(error))` due to SDK conventions.

## Implementation Steps

### Phase 1: Extend Error Enum (1 file)
- [ ] Add all missing error variants to `ChainSettleError`
- [ ] Document each error variant
- [ ] Ensure no duplicate error codes

### Phase 2: Update Helper Functions (1 file)
- [ ] Convert `assert_admin` to return `Result`
- [ ] Convert `assert_not_paused` to return `Result`
- [ ] Convert `assert_is_buyer` to return `Result`
- [ ] Convert `get_shipment_or_panic` to return `Result`
- [ ] Update all helper function call sites with `?` operator

### Phase 3: Update Public Functions (1 file, ~30 functions)
- [ ] Change return types to `Result<T, ChainSettleError>`
- [ ] Replace all `panic!()` with `return Err(...)`
- [ ] Replace all `unwrap_or_else(|| panic!(...))` with `ok_or(...)?`
- [ ] Add `?` operator to helper function calls
- [ ] Ensure all code paths return `Ok(...)` on success

### Phase 4: Update Tests (~2000 lines)
- [ ] Remove `#[should_panic]` attributes
- [ ] Update assertions to check `Err(Ok(ChainSettleError::...))`
- [ ] Update success case assertions to unwrap `Ok(...)`
- [ ] Verify all tests still pass

### Phase 5: Verification
- [ ] Search for remaining `panic!` calls (should be zero)
- [ ] Run full test suite
- [ ] Check compilation warnings
- [ ] Verify error codes are sequential and documented

## Detailed Panic Mapping

### Unauthorized (→ ChainSettleError::Unauthorized)
- Admin validation failures
- Buyer/supplier/arbiter mismatches
- Blacklist violations
- Token whitelist violations

### Invalid Parameters
- `"at least one buyer is required"` → `EmptyBuyersList`
- `"amount must be greater than zero"` → `InvalidAmount`
- `"additional_amount must be greater than zero"` → `InvalidAmount`
- `"total amount exceeds maximum shipment value"` → `MaxShipmentValueExceeded`
- `"milestone percentages must sum to 100"` → `InvalidPercentages`
- `"InvalidPercentages"` (min percent) → `InvalidPercentages`
- `"invalid milestone index"` → `InvalidMilestoneIndex`
- `"fee_bps exceeds maximum of 1000"` → `FeeTooHigh`
- `"min_milestone_percent must be between 1 and 100"` → `InvalidMinMilestonePercent`
- `"invalid multi-sig parameters"` → `InvalidMultiSigParameters`

### State Validation
- `"shipment already exists"` → `ShipmentAlreadyExists`
- `"shipment is not active"` → `ShipmentNotActive`
- `"milestone is not in pending status"` → `InvalidMilestoneStatus`
- `"milestone proof not yet submitted"` → `ProofNotSubmitted`
- `"milestone is not in ConfirmedHeld status"` → `InvalidMilestoneStatus`
- `"milestone is not in disputed status"` → `MilestoneNotDisputed`
- `"DisputeAlreadyOpen"` → `DisputeAlreadyOpen`

### Business Logic
- `"previous milestone not yet complete"` → `PreviousMilestoneNotComplete`
- `"top-up disallowed: shipment is not active"` → `TopUpNotAllowed`
- `"milestone has auto-confirmed; use claim_auto_confirmation"` → `AutoConfirmed`
- `"milestone has auto-confirmed; dispute window closed"` → `DisputeWindowClosed`
- `"holdback period not yet expired"` → `HoldbackNotExpired`
- `"dispute cooldown period has not elapsed"` → `DisputeCooldownActive`
- `"can only dispute a submitted or held proof"` → `CannotDisputeStatus`
- `"cannot cancel: dispute must be resolved first"` → `DisputeMustBeResolved`
- `"supplier cancellation not enabled for this shipment"` → `SupplierCancellationDisabled`
- `"buyer response deadline has not passed"` → `DeadlineNotBreached`
- `"already approved by this admin"` → `AlreadyApproved`
- `"multisig admin not configured"` → `MultisigNotConfigured`

### Amendment/Rotation
- `"no pending amendment"` → `AmendmentNotPending`
- `"no pending arbiter rotation"` → `ArbiterRotationNotPending`
- `"amendment already agreed by this party"` → `AlreadyApproved`
- `"arbiter rotation already agreed by this party"` → `AlreadyApproved`

### Recovery
- `"recovery threshold not met"` → `RecoveryThresholdNotMet`

## Testing Strategy

### 1. Compilation Test
```bash
cargo check --all-targets
# Should compile with zero errors
```

### 2. Panic Search
```bash
grep -r "panic!" contracts/chainsetttle/src/lib.rs
# Should return zero results
```

### 3. Test Suite
```bash
cargo test --release
# All tests should pass
```

### 4. Error Code Verification
```rust
#[test]
fn test_error_codes_sequential() {
    // Verify no duplicate error codes
    // Verify all codes are documented
}
```

## Example Refactoring

### Before (create_shipment excerpt)
```rust
pub fn create_shipment(
    env: Env,
    shipment_id: String,
    buyers: Vec<Address>,
    // ... other params
) -> String {
    Self::assert_not_paused(&env);
    
    if buyers.is_empty() {
        panic!("at least one buyer is required");
    }
    
    if total_amount <= 0 {
        panic!("amount must be greater than zero");
    }
    
    // ... rest of function
    
    shipment_id
}
```

### After (create_shipment excerpt)
```rust
pub fn create_shipment(
    env: Env,
    shipment_id: String,
    buyers: Vec<Address>,
    // ... other params
) -> Result<String, ChainSettleError> {
    Self::assert_not_paused(&env)?;
    
    if buyers.is_empty() {
        return Err(ChainSettleError::EmptyBuyersList);
    }
    
    if total_amount <= 0 {
        return Err(ChainSettleError::InvalidAmount);
    }
    
    // ... rest of function
    
    Ok(shipment_id)
}
```

## Benefits

1. **Type Safety**: Clients can pattern match on specific errors
2. **Better DX**: Clear error codes instead of string parsing
3. **Debugging**: Easier to trace error sources
4. **Testing**: More precise test assertions
5. **Documentation**: Error enum serves as error documentation
6. **Interoperability**: Standard Soroban error pattern

## Risks & Mitigation

### Risk: Breaking Changes
**Impact**: All client code must update to handle `Result`
**Mitigation**: This is a major version change (v2.0.0)

### Risk: Test Complexity
**Impact**: ~100+ test updates needed
**Mitigation**: Systematic approach, update in batches

### Risk: Missing Error Cases
**Impact**: Some panics might be missed
**Mitigation**: Comprehensive grep search + code review

## Estimated Effort

- **Phase 1**: 30 minutes (error enum)
- **Phase 2**: 1 hour (helper functions)
- **Phase 3**: 3-4 hours (public functions)
- **Phase 4**: 2-3 hours (tests)
- **Phase 5**: 30 minutes (verification)

**Total**: 7-9 hours

## Acceptance Criteria Checklist

- [ ] Zero `panic!` macro calls in lib.rs
- [ ] All public functions return `Result<T, ChainSettleError>`
- [ ] All error paths map to correct error variant
- [ ] Existing tests updated and passing
- [ ] New error variants added for uncovered panics
- [ ] Error enum documented
- [ ] No compilation warnings
- [ ] Full test suite passes

## Next Steps

1. Review and approve this implementation plan
2. Create a feature branch: `feat/issue-73-typed-errors`
3. Implement phases 1-5 systematically
4. Submit PR with comprehensive testing
5. Update documentation and changelog

---

**Status**: Planning Complete  
**Ready for Implementation**: Yes  
**Breaking Change**: Yes (requires v2.0.0)
