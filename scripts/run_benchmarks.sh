#!/bin/bash

# ChainSettle Contract Benchmark Runner
# This script helps run instruction cost benchmarks and manage baselines

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CONTRACT_DIR="$PROJECT_ROOT/contracts/chainsetttle"
BASELINE_FILE="$PROJECT_ROOT/benchmarks/baselines.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

show_help() {
    cat << EOF
ChainSettle Contract Benchmark Runner

Usage: $0 [COMMAND]

Commands:
    run             Run benchmarks and check for regressions (default)
    update          Run benchmarks and update baselines
    create          Create a new shipment benchmark
    submit          Submit proof benchmark
    confirm         Confirm milestone benchmark
    dispute         Raise dispute benchmark
    resolve         Resolve dispute benchmark
    cancel          Cancel shipment benchmark
    all             Run all individual benchmarks
    help            Show this help message

Examples:
    $0 run          # Run all benchmarks and check regressions
    $0 update       # Update baseline values
    $0 create       # Run only create_shipment benchmark

Environment Variables:
    UPDATE_BASELINES=1    Update baseline values instead of checking regressions

EOF
}

run_benchmark() {
    local test_name=$1
    print_header "Running $test_name"
    cd "$CONTRACT_DIR"
    cargo test "$test_name" --release -- --nocapture
}

run_all_benchmarks() {
    print_header "🔬 Running All ChainSettle Benchmarks"
    cd "$CONTRACT_DIR"
    cargo test benchmark_all_functions --release -- --nocapture
}

update_baselines() {
    print_header "💾 Updating Baselines"
    cd "$CONTRACT_DIR"
    UPDATE_BASELINES=1 cargo test benchmark_all_functions --release -- --nocapture
    
    if [ -f "$BASELINE_FILE" ]; then
        print_success "Baselines updated successfully!"
        print_info "File: $BASELINE_FILE"
        print_warning "Don't forget to commit the updated baselines.json file!"
    else
        print_error "Failed to create baselines file"
        exit 1
    fi
}

check_baseline_exists() {
    if [ ! -f "$BASELINE_FILE" ]; then
        print_warning "No baseline file found at $BASELINE_FILE"
        print_info "Creating initial baselines..."
        update_baselines
    fi
}

# Main script logic
case "${1:-run}" in
    run)
        check_baseline_exists
        run_all_benchmarks
        ;;
    update)
        update_baselines
        ;;
    create)
        run_benchmark "benchmark_create_shipment_only"
        ;;
    submit)
        run_benchmark "benchmark_submit_proof_only"
        ;;
    confirm)
        run_benchmark "benchmark_confirm_milestone_only"
        ;;
    dispute)
        run_benchmark "benchmark_raise_dispute_only"
        ;;
    resolve)
        run_benchmark "benchmark_resolve_dispute_only"
        ;;
    cancel)
        run_benchmark "benchmark_cancel_shipment_only"
        ;;
    all)
        print_header "🔬 Running Individual Benchmarks"
        run_benchmark "benchmark_create_shipment_only"
        run_benchmark "benchmark_submit_proof_only"
        run_benchmark "benchmark_confirm_milestone_only"
        run_benchmark "benchmark_raise_dispute_only"
        run_benchmark "benchmark_resolve_dispute_only"
        run_benchmark "benchmark_cancel_shipment_only"
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        print_error "Unknown command: $1"
        echo ""
        show_help
        exit 1
        ;;
esac

print_success "Done!"
