#!/bin/bash
# Compilation checkpoint (RG-18)
# Verifies that both lib and test targets compile.
# Usage: ./check_compiles.sh
# Exit 0 = clean compile, Exit 1 = errors found

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
