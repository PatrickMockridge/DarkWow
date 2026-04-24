#!/bin/bash
# Stop the 5-node linear-testnet Docker stack

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "=== Stopping DarkFi Linear-Testnet ==="

docker-compose down

echo "Stack stopped."