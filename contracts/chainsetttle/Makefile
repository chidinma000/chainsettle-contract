# ChainSettle Contract — Makefile
# ============================================================
# Usage:
#   make test         → run all unit tests
#   make build        → compile to .wasm
#   make optimize     → optimize .wasm for deployment
#   make deploy-testnet → deploy to Stellar Testnet
#   make clean        → remove build artifacts
# ============================================================

NETWORK        = testnet
CONTRACT_NAME  = chainsetttle
WASM_PATH      = target/wasm32v1-none/release/$(CONTRACT_NAME).wasm
WASM_OPT_PATH  = target/wasm32v1-none/release/$(CONTRACT_NAME).optimized.wasm

.PHONY: test build optimize deploy-testnet clean fmt check

## Run all unit tests
test:
	cargo test

## Build the contract to .wasm (release mode)
build:
	stellar contract build

## Optimize the compiled .wasm for smaller deployment size
optimize: build
	stellar contract optimize --wasm $(WASM_PATH)
	@echo "Optimized wasm: $(WASM_OPT_PATH)"

## Deploy the optimized contract to Stellar Testnet
## Requires: STELLAR_ACCOUNT env variable to be set
deploy-testnet: optimize
	stellar contract deploy \
		--wasm $(WASM_OPT_PATH) \
		--source $(STELLAR_ACCOUNT) \
		--network $(NETWORK)

## Format Rust code
fmt:
	cargo fmt

## Check for compile errors without building
check:
	cargo check

## Clean build artifacts
clean:
	cargo clean
