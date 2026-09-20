#!/bin/bash
# DarkWow unified integration test umbrella.
#
# Two tiers, sequential. Each non-zero gate exits 1; nothing after it runs.
# Color-coded per-gate status, modeled on check_pipeline_build.sh.
#
# Usage:
#   ./scripts/run-all-tests.sh            # Tier 1 (fast, hermetic)
#   ./scripts/run-all-tests.sh --tier 1   # same
#   ./scripts/run-all-tests.sh --tier 2   # Tier 1 + Docker pipeline
#
# Tier 1 — fast + hermetic (seconds to minutes, no Docker, no network)
# Tier 2 — heavyweight E2E (Docker devnet, minutes to hours)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}"
export RUST_MIN_STACK="${RUST_MIN_STACK:-67108864}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASSED=0
FAILED=0
FAILED_GATES=()

run_gate() {
    local label="$1"; shift
    echo ""
    echo -e "=== ${label} ==="
    if "$@"; then
        echo -e "${GREEN}PASS:${NC} ${label}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}FAIL:${NC} ${label} (exit $?)"
        FAILED=$((FAILED + 1))
        FAILED_GATES+=("$label")
        exit 1
    fi
}

TIER="${1:-}${2:-}"  # allows "--tier 2" as two args: $1=--tier $2=2

# Static circuit audits first — fail fast before any build work.
run_gate "circuit metadata alignment"     bash "$SCRIPT_DIR/check-circuit-metadata-alignment.sh"
run_gate "circuit domain separation"      bash "$SCRIPT_DIR/check-circuit-domain-separation.sh"
# OBL-Z1: the Orchard-class rule. The other two circuit gates are structural (counts, prefix
# presence); this is the only one that asks whether an exposed public input is *determined*.
# It is currently RED with 33 untriaged sites — see doc/src/arch/verification-hazop.md, OBL-Z1.
run_gate "circuit instance derivation"    bash "$SCRIPT_DIR/check-circuit-instance-derivation.sh"

run_gate "build contract ZK circuits"     "$SCRIPT_DIR/build-contract-zk.sh"
run_gate "pre-build guard (dwowd + wallet + 32 contracts→wasm32)" \
                                          "$SCRIPT_DIR/check_pipeline_build.sh"
run_gate "Rust tests (make test)"          make test

# Lake requires its own working directory.
#
# NOTE: the target is `DarkFi`, not a bare `lake build`. A bare `lake build` builds "the default
# facet of the root package" — which for this package is nothing at all: it exits 0 without
# compiling a single module. The gate below said `lake build` and therefore passed for as long
# as it existed while 24 of 50 modules did not compile. `lake build DarkFi` is what actually
# type-checks the proofs. See proofs/lean/README.md.
run_gate "Lean proofs (lake build DarkFi)" bash -c 'cd proofs/lean && lake build DarkFi'

# The assumption boundary. Runs after the build it depends on: the budget check walks the
# compiled environment, so it needs `lake build DarkFi` to have succeeded. `--require-collector`
# makes a failure to run the collector fatal, so a red build cannot present itself as a clean
# boundary.
run_gate "Lean assumption boundary (axioms/budgets)" \
                                          python3 script/check_lean_axioms.py --require-collector

run_gate "Python: pipeline model"          python3 contrib/model/pipeline_model.py
run_gate "Python: supply chain model"      python3 contrib/model/supply_chain_model.py

if [ "$TIER" = "--tier 2" ]; then
    run_gate "Docker test pipeline (native, 2 wallets)" \
        "$REPO_ROOT/contrib/docker/darkwow-testnet/test_pipeline.sh" --mode native --with-wallet 2
fi

echo ""
echo "========================================"
echo -e "Umbrella summary: ${GREEN}${PASSED} passed${NC}, ${RED}${FAILED} failed${NC}"
if [ "$FAILED" -gt 0 ]; then
    echo -e "Failed gates: ${RED}${FAILED_GATES[*]}${NC}"
fi
echo "========================================"

[ "$FAILED" -eq 0 ] || exit 1
exit 0
