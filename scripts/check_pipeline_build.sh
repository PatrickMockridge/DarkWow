#!/bin/bash
# Pre-build guard — compile every surface the Docker test pipeline builds, on the host,
# BEFORE running test_pipeline.sh (and before you push: the pipeline builds from
# origin/linear-master, not your working tree).
#
# The pipeline's Docker build compiles THREE surfaces, and a type migration can break any:
#   1. dwowd         — the node binary            (bin/dwowd)          [host target]
#   2. dwow_wallet   — the wallet binary          (bin/dww)            [host target]
#   3. every contract's on-chain entrypoint       (src/contract/*)     [wasm32-unknown-unknown]
#
# Why routine fast checks miss this whole class:
#   - `cargo check -p dwowd --lib` checks only dwowd's lib — never dwow_wallet, never a
#     contract entrypoint.
#   - The entrypoint module is gated `#[cfg(not(feature = "no-entrypoint"))]`. dwowd's
#     contract deps use features=["client","no-entrypoint"], contract integration tests run
#     `--features no-entrypoint,client`, and `make test`/`clippy` use `--all-features` — all of
#     which turn no-entrypoint ON and thereby REMOVE the entrypoint from the build.
#   So the only place these compile to their real target is the Docker build — expensive and
#   post-push. This guard compiles all three locally in seconds.
#
# Usage:
#   ./scripts/check_pipeline_build.sh                                # everything
#   ./scripts/check_pipeline_build.sh dwow_wallet                    # one host binary
#   ./scripts/check_pipeline_build.sh dwow_promissory_note_contract  # one contract (wasm32)
#
# Contract entrypoints are checked PER-PACKAGE in separate invocations: ~13 contracts depend
# on dwow_promissory_note_contract with features=["no-entrypoint"], so a single combined
# `cargo check -p X -p Y` would let cargo feature unification switch no-entrypoint ON and hide
# the very entrypoint we need to compile. Separate invocations = default features = entrypoint
# ON, matching the Dockerfile's chained `cargo build -p X && cargo build -p Y`.

set -uo pipefail
export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}"

CONTRACT_DIR="src/contract"
WASM_TARGET="wasm32-unknown-unknown"
HOST_BINS=(dwowd dwow_wallet)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Ensure the wasm target is installed (idempotent; the Dockerfile does the same).
if ! rustup target list --installed 2>/dev/null | grep -qx "$WASM_TARGET"; then
    echo -e "${YELLOW}Installing $WASM_TARGET target...${NC}"
    rustup target add "$WASM_TARGET"
fi

OK=0
FAILED=0
FAILED_LABELS=()

# run_check <label> <cargo check args...> — capture output, print errors only on failure.
run_check() {
    local label="$1"; shift
    local out
    if out=$(cargo check "$@" 2>&1); then
        echo -e "${GREEN}OK:${NC}   $label"
        OK=$((OK + 1))
    else
        echo -e "${RED}FAIL:${NC} $label"
        echo "$out" | grep -E "error(\[|:)|-->|expected|found|no method|the trait" | sed 's/^/    /'
        FAILED=$((FAILED + 1))
        FAILED_LABELS+=("$label")
    fi
}

check_host() { run_check "$1 (host)"   -p "$1"; }
check_wasm() { run_check "$1 (wasm32)" --target "$WASM_TARGET" -p "$1"; }

# Package name for a contract dir. `grep -m1` takes the [package] name and ignores any later
# `[[bin]] name = ...` line (e.g. stablecoin's gen_init_params) — the two-name trap.
pkg_name() { grep -m1 '^name = ' "$1/Cargo.toml" | cut -d'"' -f2; }

is_host_bin() { local p; for p in "${HOST_BINS[@]}"; do [ "$p" = "$1" ] && return 0; done; return 1; }

if [ "$#" -ge 1 ]; then
    echo "=== Checking $1 ==="
    echo ""
    if is_host_bin "$1"; then check_host "$1"; else check_wasm "$1"; fi
else
    echo "=== Host binaries (node + wallet) ==="
    echo ""
    for b in "${HOST_BINS[@]}"; do check_host "$b"; done
    # Feature-gate compile check: net-node is the middle P2P tier (net-wallet +
    # refine-session). It is compiled transitively through net-full in every
    # dwowd build, but never standalone. This gate catches bit-rot in the
    # standalone cfg combination (MOC item 1, close-out of the [PARTIAL] doc entry).
    run_check "net-node (standalone feature gate)" -p dwow_core --no-default-features \
        --features "net-node, async-serial, blockchain"
    echo ""
    echo "=== Contract entrypoints -> $WASM_TARGET ==="
    echo ""
    for dir in "$CONTRACT_DIR"/*/; do
        dir="${dir%/}"
        # Skip the shared test-harness (not an on-chain contract) and dirs without a Cargo.toml.
        [ "$(basename "$dir")" = "test-harness" ] && continue
        [ -f "$dir/Cargo.toml" ] || continue
        pkg="$(pkg_name "$dir")"
        [ -n "$pkg" ] || { echo -e "${YELLOW}SKIP:${NC} $dir (no package name)"; continue; }
        check_wasm "$pkg"
    done
fi

echo ""
echo "========================================"
echo -e "Summary: ${GREEN}$OK OK${NC}, ${RED}$FAILED failed${NC}"
if [ "$FAILED" -gt 0 ]; then
    echo -e "Failed: ${RED}${FAILED_LABELS[*]}${NC}"
fi
echo "========================================"

[ "$FAILED" -eq 0 ] || exit 1
exit 0
