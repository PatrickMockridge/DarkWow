#!/bin/bash
# Generate 5 wallets for the linear-testnet nodes
# Each wallet will be used by its respective node for minting rewards

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WALLETS_DIR="$SCRIPT_DIR"

echo "=== Generating 5 DarkFi Wallets for Linear-Testnet ==="
echo ""

# Array of node indices
NODES=(0 1 2 3 4)

# Array of RPC ports
PORTS=(28345 28346 28347 28348 28349)

# Generate 5 wallets
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

    echo "Created wallet config: $WALLET_DIR/drk${i}.toml"
done

echo ""
echo "=== Wallet Configuration Summary ==="
for i in "${NODES[@]}"; do
    WALLET_DIR="$WALLETS_DIR/wallet${i}"
    echo "Wallet $i: $WALLET_DIR/drk${i}.toml (RPC: 127.0.0.1:${PORTS[$i]})"
done

echo ""
echo "To initialize wallets with keys, run:"
echo "  for i in 0 1 2 3 4; do"
echo "    ./target/debug/drk -c wallets/wallet\$i/drk\$i.toml wallet init"
echo "    ./target/debug/drk -c wallets/wallet\$i/drk\$i.toml wallet address"
echo "  done"
echo ""
echo "Then update docker-compose.yml with generated addresses."