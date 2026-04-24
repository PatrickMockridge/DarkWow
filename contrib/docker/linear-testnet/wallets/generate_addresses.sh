#!/bin/bash
# Full setup script for 5 wallets with their addresses
# Run this after the Docker stack is running to generate wallet addresses

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WALLETS_DIR="$SCRIPT_DIR"

echo "=== Full Wallet Setup for Linear-Testnet ==="
echo ""

# Check if drk binary exists
if [ ! -f "../../target/debug/drk" ]; then
    echo "ERROR: drk binary not found at ../../target/debug/drk"
    echo "Build it with: cargo build -p drk"
    exit 1
fi

# Array of node indices
NODES=(0 1 2 3 4)

# Array of RPC ports
PORTS=(28345 28346 28347 28348 28349)

# Check if stack is running
echo "Checking if linear-testnet nodes are running..."
for i in 0 1 2 3 4; do
    if ! curl -s -f -X POST "http://localhost:${PORTS[$i]}" -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' > /dev/null 2>&1; then
        echo "ERROR: Node $i is not responding on port ${PORTS[$i]}"
        echo "Start the stack first: cd ../ && ./scripts/start.sh"
        exit 1
    fi
done
echo "All nodes are responding!"
echo ""

# Generate wallets
declare -a ADDRESSES

for i in "${NODES[@]}"; do
    WALLET_DIR="$WALLETS_DIR/wallet${i}"
    mkdir -p "$WALLET_DIR"

    # Create drk config for this wallet
    cat > "$WALLET_DIR/drk${i}.toml" << EOF
network = "linear-testnet"

[network_config."linear-testnet"]
cache_path = "~/.local/share/darkfi/drk/linear-testnet/wallet${i}/cache"
wallet_path = "~/.local/share/darkfi/drk/linear-testnet/wallet${i}/wallet.db"
wallet_pass = "testpassword123"
endpoint = "tcp://127.0.0.1:${PORTS[$i]}"
history_path = "~/.local/share/darkfi/drk/linear-testnet/wallet${i}/history.txt"
EOF

    echo "Initializing wallet ${i}..."
    # Initialize wallet (creates if not exists)
    ./target/debug/drk -c "$WALLET_DIR/drk${i}.toml" wallet init 2>/dev/null || true

    # Get address
    ADDR=$(./target/debug/drk -c "$WALLET_DIR/drk${i}.toml" wallet address 2>/dev/null | head -1 | tr -d '[:space:]')
    ADDRESSES[$i]=$ADDR

    echo "  Address: $ADDR"
done

echo ""
echo "=== Generated Addresses ==="
for i in "${NODES[@]}"; do
    echo "wallet${i}: ${ADDRESSES[$i]}"
done

# Save addresses to a file for reference
cat > "$WALLETS_DIR/addresses.txt" << EOF
# Linear-Testnet Wallet Addresses
# Generated on $(date)
wallet0=${ADDRESSES[0]}
wallet1=${ADDRESSES[1]}
wallet2=${ADDRESSES[2]}
wallet3=${ADDRESSES[3]}
wallet4=${ADDRESSES[4]}
EOF

echo ""
echo "Addresses saved to: $WALLETS_DIR/addresses.txt"

# Print updated docker-compose env vars
echo ""
echo "=== To use these addresses with Docker ==="
echo "Run containers with WALLET_ADDR values:"
echo ""
echo "WALLET_ADDR_0=${ADDRESSES[0]}"
echo "WALLET_ADDR_1=${ADDRESSES[1]}"
echo "WALLET_ADDR_2=${ADDRESSES[2]}"
echo "WALLET_ADDR_3=${ADDRESSES[3]}"
echo "WALLET_ADDR_4=${ADDRESSES[4]}"
echo ""
echo "Or update docker-compose.yml with these addresses."