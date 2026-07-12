#!/bin/bash
# Compile every contract's on-chain entrypoint to wasm32 — a fast pre-pipeline guard.
#
# Why this exists:
#   The contract `entrypoint` module is gated `#[cfg(not(feature = "no-entrypoint"))]`,
#   so it is compiled ONLY when `no-entrypoint` is OFF. Every fast local check turns it
#   ON and thus hides the entrypoint:
#     - `cargo check -p dwowd`         (dwowd deps use features=["client","no-entrypoint"])
#     - contract integration tests     (--features no-entrypoint,client)
#     - `make test` / `make clippy`    (--all-features enables no-entrypoint)
#   The only place entrypoints are compiled to their real target (wasm32) is the Docker
#   pipeline build — expensive and only after you push. This script mirrors that build
#   locally, in seconds, so a contract entrypoint that has drifted from its (typed) model
#   is caught BEFORE running contrib/docker/darkwow-testnet/test_pipeline.sh and before push
#   (the Docker build compiles from origin/linear-master, not your working tree).
#
# Usage:
#   ./scripts/check_contracts_wasm.sh                              # check all contracts
#   ./scripts/check_contracts_wasm.sh dwow_promissory_note_contract  # check one package
#
# IMPORTANT — each contract is checked in a SEPARATE `cargo check` invocation. A single
# combined `cargo check -p X -p Y ...` must not be used: ~13 contracts depend on
# dwow_promissory_note_contract with features=["no-entrypoint"], so cargo feature
# unification in one resolve would switch no-entrypoint ON for it and hide the very
# entrypoint we need to compile. Per-package invocations = default features = entrypoint ON,
# matching the Dockerfile's chained `cargo build -p X && cargo build -p Y`.

set -uo pipefail

export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}"

CONTRACT_DIR="src/contract"
TARGET="wasm32-unknown-unknown"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Ensure the wasm target is installed (idempotent; the Dockerfile does the same).
if ! rustup target list --installed 2>/dev/null | grep -qx "$TARGET"; then
    echo -e "${YELLOW}Installing $TARGET target...${NC}"
    rustup target add "$TARGET"
fi

OK=0
FAILED=0
FAILED_PKGS=()

# Check one contract package's entrypoint compiles to wasm32 (default features).
check_pkg() {
    local pkg="$1"
    # Capture output so we can print it only on failure.
    local out
    if out=$(cargo check --target "$TARGET" -p "$pkg" 2>&1); then
        echo -e "${GREEN}OK:${NC}   $pkg"
        OK=$((OK + 1))
    else
        echo -e "${RED}FAIL:${NC} $pkg"
        # Surface the compiler errors (drop noisy 'Compiling'/'Checking'/warning lines).
        echo "$out" | grep -E "error(\[|:)|-->|expected|found|no method|the trait" | sed 's/^/    /'
        FAILED=$((FAILED + 1))
        FAILED_PKGS+=("$pkg")
    fi
}

# Resolve the package name for one contract dir. Uses `grep -m1` so the [package] name
# is taken and any later `[[bin]] name = ...` line (e.g. stablecoin's gen_init_params) is
# ignored — the same two-name trap that breaks stablecoin's Makefile.
pkg_name() {
    grep -m1 '^name = ' "$1/Cargo.toml" | cut -d'"' -f2
}

if [ "$#" -ge 1 ]; then
    echo "=== Checking entrypoint compiles to $TARGET: $1 ==="
    echo ""
    check_pkg "$1"
else
    echo "=== Checking all contract entrypoints compile to $TARGET ==="
    echo ""
    for dir in "$CONTRACT_DIR"/*/; do
        dir="${dir%/}"
        # Skip the shared test-harness (not an on-chain contract) and anything without a Cargo.toml.
        [ "$(basename "$dir")" = "test-harness" ] && continue
        [ -f "$dir/Cargo.toml" ] || continue
        pkg="$(pkg_name "$dir")"
        [ -n "$pkg" ] || { echo -e "${YELLOW}SKIP:${NC} $dir (no package name)"; continue; }
        check_pkg "$pkg"
    done
fi

echo ""
echo "========================================"
echo -e "Summary: ${GREEN}$OK OK${NC}, ${RED}$FAILED failed${NC}"
if [ "$FAILED" -gt 0 ]; then
    echo -e "Failed: ${RED}${FAILED_PKGS[*]}${NC}"
fi
echo "========================================"

[ "$FAILED" -eq 0 ] || exit 1
exit 0
