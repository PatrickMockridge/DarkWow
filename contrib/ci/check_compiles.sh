#!/bin/bash
# Compilation checkpoint (RG-18)
# Verifies that both lib and test targets compile.
# Usage: ./check_compiles.sh
# Exit 0 = clean compile, Exit 1 = errors found
#
# WIRING, decided 2026-09-25: NOT wired into `scripts/run-all-tests.sh`, deliberately.
# Under the test schema's own partition this is partition A — a fact the compiler
# establishes — and `doc/src/dev/testing/production-test-standard.md:339` says of
# partition A that "tests here SHALL be removed — the compiler IS the test". Presenting a
# compilation as evidence of conformance is the error that clause names.
#
# It is not a loss: the umbrella already builds everything through `make test` and its
# "pre-build guard (dwowd + wallet + 32 contracts→wasm32)" gate, which fail on the same
# condition with the real compiler output. Keep this script for a quick manual check; do
# not count it as a test.

set -euo pipefail

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

echo "=== Compilation Checkpoint (RG-18) ==="
echo "Target: cargo check -p dwowd --lib --tests"
echo ""

if cargo check -p dwowd --lib --tests 2>&1; then
    echo ""
    echo "Compilation: PASS"
    exit 0
else
    echo ""
    echo "Compilation: FAIL"
    exit 1
fi
