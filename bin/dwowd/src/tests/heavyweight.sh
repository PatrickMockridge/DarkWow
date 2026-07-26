#!/bin/bash
# Heavyweight Test Runner — runs Level 2 ZK proof tests for DarkWow contracts.
#
# Usage:
#   ./heavyweight.sh --all                    Run all 43 tests
#   ./heavyweight.sh --dex                    Run a single contract test
#   ./heavyweight.sh --dex --auction          Run multiple tests
#   ./heavyweight.sh --block-execution        Run all 8 block execution tests
#   ./heavyweight.sh --dex --nocapture        Show test output
#   ./heavyweight.sh --help                   List all flags
#
# Environment:
#   RAYON_NUM_THREADS=10   Parallelism for ZK proof generation
#   RUST_MIN_STACK=67108864 Stack size for halo2 proving key intensity

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
cd "$REPO_ROOT"

export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}"
export RUST_MIN_STACK="${RUST_MIN_STACK:-67108864}"

CARGO_CMD="cargo test --release -p dwowd"
TEST_FILTERS=()
NOCAPTURE=""

# ── Flag parsing ───────────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        # ── Group flags ────────────────────────────────────────────────────────
        --all)
            TEST_FILTERS+=("test_heavyweight_" "test_relayer_lifecycle_heavyweight")
            ;;

        --block-execution)
            TEST_FILTERS+=(
                "test_heavyweight_canonical_exec"
                "test_heavyweight_coinbase_rejects_wrong_reward"
                "test_heavyweight_uncle_exec"
                "test_heavyweight_mixed_exec"
                "test_heavyweight_multi_uncle"
                "test_heavyweight_uncle_depth"
                "test_heavyweight_empty_uncle"
                "test_heavyweight_invalid_uncle_proof"
            )
            ;;

        # ── Contract flags ─────────────────────────────────────────────────────
        --promissory-note)      TEST_FILTERS+=("test_heavyweight_promissory_note") ;;
        --purse)                TEST_FILTERS+=("test_heavyweight_purse") ;;
        --dex)                  TEST_FILTERS+=("test_heavyweight_dex") ;;
        --native-token)         TEST_FILTERS+=("test_heavyweight_native_token") ;;
        --auction)              TEST_FILTERS+=("test_heavyweight_auction") ;;
        --escrow)               TEST_FILTERS+=("test_heavyweight_escrow") ;;
        --metadata)             TEST_FILTERS+=("test_heavyweight_metadata") ;;
        --stablecoin)           TEST_FILTERS+=("test_heavyweight_stablecoin") ;;
        --bridge)               TEST_FILTERS+=("test_heavyweight_bridge") ;;
        --labor-market)         TEST_FILTERS+=("test_heavyweight_labor_market") ;;
        --attestation)          TEST_FILTERS+=("test_heavyweight_attestation") ;;
        --tender)               TEST_FILTERS+=("test_heavyweight_tender") ;;
        --subscription)         TEST_FILTERS+=("test_heavyweight_subscription") ;;
        --oracle)               TEST_FILTERS+=("test_heavyweight_oracle") ;;
        --pool-stake)           TEST_FILTERS+=("test_heavyweight_pool_stake") ;;
        --relayer-endowment)    TEST_FILTERS+=("test_heavyweight_relayer_endowment") ;;
        --slot)                 TEST_FILTERS+=("test_heavyweight_slot") ;;
        --deployooor)           TEST_FILTERS+=("test_heavyweight_deployooor") ;;
        --drain-protection)     TEST_FILTERS+=("test_heavyweight_drain_protection") ;;
        --game-room)            TEST_FILTERS+=("test_heavyweight_game_room") ;;
        --insurance-market)     TEST_FILTERS+=("test_heavyweight_insurance_market") ;;
        --baccarat)             TEST_FILTERS+=("test_heavyweight_baccarat") ;;
        --betting-stake)        TEST_FILTERS+=("test_heavyweight_betting_stake") ;;
        --box)                  TEST_FILTERS+=("test_heavyweight_box") ;;
        --darkbet-exchange)     TEST_FILTERS+=("test_heavyweight_darkbet_exchange") ;;
        --darktoshi-dice)       TEST_FILTERS+=("test_heavyweight_darktoshi_dice") ;;
        --lottery)              TEST_FILTERS+=("test_heavyweight_lottery") ;;
        --multisig)             TEST_FILTERS+=("test_heavyweight_multisig") ;;
        --roulette)             TEST_FILTERS+=("test_heavyweight_roulette") ;;
        --dao-escrow)           TEST_FILTERS+=("test_heavyweight_dao_escrow") ;;
        --identity)             TEST_FILTERS+=("test_heavyweight_identity") ;;
        --bearer-bond)          TEST_FILTERS+=("test_heavyweight_bearer_bond") ;;
        --otc-swap)             TEST_FILTERS+=("test_heavyweight_otc_swap") ;;

        # ── Integration flags ──────────────────────────────────────────────────
        --recruitment)          TEST_FILTERS+=("test_heavyweight_recruitment_pipeline") ;;
        --relayer)              TEST_FILTERS+=("test_relayer_lifecycle_heavyweight") ;;

        # ── Meta flags ─────────────────────────────────────────────────────────
        --nocapture)            NOCAPTURE="--nocapture" ;;
        --help)
            echo "Usage: heavyweight.sh [flags...]"
            echo ""
            echo "Contract tests (32):"
            echo "  --promissory-note   --dex               --native-token"
            echo "  --auction           --escrow            --metadata"
            echo "  --stablecoin        --bridge            --labor-market"
            echo "  --attestation        --tender            --subscription"
            echo "  --oracle            --pool-stake        --relayer-endowment"
            echo "  --slot              --deployooor        --drain-protection"
            echo "  --game-room         --insurance-market  --baccarat"
            echo "  --betting-stake     --darkbet-exchange  --darktoshi-dice"
            echo "  --lottery           --roulette          --dao-escrow"
            echo "  --identity          --bearer-bond       --otc-swap"
            echo "  --box               --purse             --multisig"
            echo ""
            echo "Block execution tests (1 group):"
            echo "  --block-execution   (8 tests: canonical, uncle, mixed, multi-uncle,"
            echo "                       depth, empty-uncle, invalid-uncle-proof, wrong-reward)"
            echo ""
            echo "Integration tests (2):"
            echo "  --recruitment       Cross-contract recruitment pipeline"
            echo "  --relayer           Relayer lifecycle"
            echo ""
            echo "Meta flags:"
            echo "  --all               Run all 43 heavyweight tests"
            echo "  --nocapture         Show test output (println diagnostics)"
            echo "  --help              This help"
            echo ""
            echo "Examples:"
            echo "  $0 --dex                    # Single contract"
            echo "  $0 --dex --auction          # Multiple contracts"
            echo "  $0 --block-execution        # All block execution tests"
            echo "  $0 --dex --nocapture        # Show test output"
            echo "  $0 --all                    # Everything"
            exit 0
            ;;
        *)  echo "Unknown flag: $1. Use --help for usage." >&2; exit 1 ;;
    esac
    shift
done

# ── Build cargo test filter ────────────────────────────────────────────────────

if [ ${#TEST_FILTERS[@]} -eq 0 ]; then
    echo "No tests selected. Use --help for usage." >&2
    exit 1
fi

FILTER_STRING=""
for f in "${TEST_FILTERS[@]}"; do
    FILTER_STRING="${FILTER_STRING}${f} "
done

echo "Running: $CARGO_CMD -- $FILTER_STRING $NOCAPTURE"
echo ""
exec $CARGO_CMD -- $FILTER_STRING $NOCAPTURE
