# Universal Relayer

*Running a relayer service for DarkFi's cross-chain atomic swaps*

## Overview

The **Universal Relayer** is a service that executes withdrawals from the DarkFi bridge to external blockchains. It monitors the DarkFi bridge contract for pending withdrawals and executes the corresponding transactions on Ethereum, Monero, Zcash, Aztec, and Litecoin.

Unlike traditional bridges that require threshold multi-signature schemes, DarkFi uses an **Object Capability Security** model where:

- Users prove knowledge of a secret to authorize withdrawals
- Relayers execute transactions on external chains using the revealed secret
- Users can cancel timed-out withdrawals to reclaim funds

```
┌─────────────────────────────────────────────────────────────────┐
│                     Universal Relayer                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐           │
│  │  Ethereum   │   │   Monero    │   │   Zcash     │           │
│  │  Executor   │   │  Executor   │   │  Executor   │           │
│  └──────┬──────┘   └──────┬──────┘   └──────┬──────┘           │
│         │                  │                  │                   │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐           │
│  │   Aztec     │   │  Litecoin   │   │   Watcher   │           │
│  │  Executor   │   │  Executor   │   │             │           │
│  └─────────────┘   └─────────────┘   └──────┬──────┘           │
│                                              │                   │
│                                    ┌─────────┴─────────┐         │
│                                    │  ExecutorRegistry │         │
│                                    └─────────┬─────────┘         │
│                                              │                   │
└──────────────────────────────────────────────┼───────────────────┘
                                               │
                                    ┌──────────┴──────────┐
                                    │   DarkFi Bridge     │
                                    │   (Deposit/Withdraw) │
                                    └─────────────────────┘
```

## Why Run a Relayer?

Relayers provide an essential service to the DarkFi ecosystem:

| Benefit | Description |
|---------|-------------|
| **Cross-Chain Liquidity** | Enables movement of assets between DarkFi and external chains |
| **Privacy Preservation** | Users can move assets in and out of DarkFi's privacy layer |
| **Fee Income** | Relayers earn fees (configurable percentage) on each withdrawal |
| **Network Security** | Economic incentives ensure withdrawals are executed promptly |
| **Atomic Swap Enablement** | Powers the trustless exchange of assets across chains |

### Relayer Revenue Model

```
Withdrawal Amount: 100 XMR
Relayer Fee: 1%
User Receives: 99 XMR
Relayer Earns: 1 XMR

Note: Fee percentage is configurable in relayer settings
```

### Capital Backing

Relayers can increase their coverage capacity through external capital backing:

- **[Pool Stake Contract](pool_stake.md)**: Join staking pools to provide shared coverage for guaranteed withdrawals
- **[Relayer Endowment Contract](endowment.md)**: Accept external capital from backers in exchange for a share of fees

See [Relayer Economics](relayer_economics.md) for the full economic model.

## Supported Chains

| Chain | Native Token | Privacy Model | Minimum Withdrawal |
|-------|-------------|---------------|-------------------|
| **Ethereum** | ETH | Transparent | 0.001 ETH |
| **Monero** | XMR | Ring signatures | 0.001 XMR |
| **Zcash** | ZEC | Sapling shielded | 0.0001 ZEC |
| **Aztec** | ETH/DAI | Private rollup | 0.001 ETH |
| **Litecoin** | LTC | Transparent + MWEB | 0.001 LTC |

## Withdrawal Flow

```
1. User submits withdrawal on DarkFi:
   ├── Secret is revealed to authorize withdrawal
   ├── Pending withdrawal created with timeout (100 blocks)
   └── User's funds locked in bridge contract

2. Relayer observes pending withdrawal:
   ├── Polls DarkFi for new withdrawals
   ├── Selects appropriate executor by chain type
   └── Verifies withdrawal is not timed out

3. Relayer executes on external chain:
   ├── Derives recipient address from recipient_hash
   ├── Signs and broadcasts transaction
   └── Marks withdrawal as processed

4. Confirmation and completion:
   ├── Relayer verifies transaction confirmation
   ├── Withdrawal marked complete in bridge
   └── User receives funds on external chain

5. Timeout handling (if relayer fails):
   ├── User waits for timeout (100 blocks)
   ├── User calls CancelWithdrawV1
   └── Funds returned to user's DarkFi wallet
```

## Hardware Requirements

Running a relayer requires maintaining full nodes for the chains you're servicing. Here's what you need:

### Minimum Hardware (Single Chain)

| Resource | Requirement |
|----------|-------------|
| **CPU** | 2 cores |
| **RAM** | 4 GB |
| **Storage** | 50-100 GB depending on chain |
| **Network** | 10 Mbps stable |

### Recommended Hardware (Multi-Chain ZEC + XMR + Aztec)

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| **CPU** | 4 cores | 8+ cores |
| **RAM** | 8 GB | 16 GB |
| **Storage** | 200 GB | 400 GB |
| **Network** | 50 GB/month | 200+ GB/month |
| **Monthly Cost** | ~$40-80/mo (VPS) | ~$150-300/mo (dedicated) |

### Per-Chain Requirements

#### Monero (XMR)

| Resource | Minimum | Notes |
|----------|---------|-------|
| **Storage** | 50 GB (pruned) | Full node: 100GB+ |
| **RAM** | 4 GB | |
| **Bandwidth** | ~200 MB/day | Initial sync: ~1 GB |

- Requires **monerod** running with view key
- Requires **monero-wallet-rpc** for wallet access
- Cannot use third-party RPC (privacy coins require own node)

#### Zcash (ZEC)

| Resource | Minimum | Notes |
|----------|---------|-------|
| **Storage** | 30 GB (pruned) | Full node: 80GB+ |
| **RAM** | 4 GB | |
| **Bandwidth** | ~1 GB/day | |

- Requires **zcashd** with wallet support
- For **shielded transactions**: requires full node with viewing key
- For **transparent transactions**: can use third-party RPC

#### Aztec (ETH/DAI)

| Resource | Minimum | Notes |
|----------|---------|-------|
| **Storage** | ~100 GB (if running own geth) | Or use third-party RPC |
| **RAM** | 4 GB | If running own geth |
| **Network** | Access to Ethereum RPC | Infura/Alchemy sufficient |

- Aztec is an **Ethereum L2**, not a separate chain
- Only needs Ethereum RPC access (no Aztec full node exists)
- Can use Infura/Alchemy for both observing and broadcasting

#### Ethereum (ETH)

| Resource | Minimum | Notes |
|----------|---------|-------|
| **Storage** | ~100 GB (pruned archive) | Or use third-party RPC |
| **RAM** | 4 GB | If running own geth |

- Can use Infura/Alchemy for both observing and broadcasting
- Only need own node if you want full control

#### Litecoin (LTC)

| Resource | Minimum | Notes |
|----------|---------|-------|
| **Storage** | 20 GB (pruned) | |
| **RAM** | 2 GB | |
| **Bandwidth** | ~500 MB/day | |

- Similar to Bitcoin but faster blocks (2.5 min)
- MWEB provides confidential transactions

### Practical VPS Options

| Provider | Specs | Monthly Cost | Good For |
|----------|-------|-------------|----------|
| **Hetzner AX41-NVMe** | Ryzen 5, 64GB RAM, 2x1TB NVMe | ~€40 | Single chain |
| **Hetzner AX62** | Ryzen 7, 128GB RAM, 2x2TB NVMe | ~€90 | Multi-chain |
| **KimSufi KS-HV2** | Xeon, 32GB RAM, 4x3TB HDD | ~€30 | Budget LTC/XMR |
| **Contabo S45** | Ryzen 7, 64GB RAM, 3.2TB NVMe | ~€25 | Budget multi-chain |

## Installation

### Build from Source

```bash
# Clone the repository
git clone https://codeberg.org/darkrenaissance/darkfi.git
cd darkfi

# Build the universal relayer
cargo build -p universal_relayer --release

# Binary will be at target/release/universal_relayer
```

### Prerequisites

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install dependencies (Ubuntu/Debian)
sudo apt install build-essential pkg-config libssl-dev
```

## Configuration

### Create Configuration File

Create `universal_relayer_config.toml`:

```toml
[darkfi]
darkfid_url = "http://127.0.0.1:8543"
poll_interval_secs = 10
max_concurrent_withdrawals = 10

[ethereum]
enabled = true
node_url = "https://mainnet.infura.io/v3/YOUR_KEY"
relayer_private_key = "0x..."
max_gas_gwei = 50
max_gas = 21000

[monero]
enabled = true
wallet_rpc_url = "http://127.0.0.1:18083"
node_rpc_url = "http://127.0.0.1:18081"
view_key = "your_view_key_here"
fee_address = "your_xmr_address"
min_confirmations = 10

[zcash]
enabled = true
node_rpc_url = "http://127.0.0.1:8232"
shielded_pool = true
min_confirmations = 10

[aztec]
enabled = true
rollup_address = "0x..."
sequencer_url = "https://aztec.network"
min_confirmations = 5

[litecoin]
enabled = true
node_rpc_url = "http://127.0.0.1:9332"
rpc_user = "user"
rpc_pass = "pass"
min_confirmations = 6

[relayer]
timeout_blocks = 100
fee_percentage = 1
```

### Configuration Fields Explained

#### DarkFi Connection

| Field | Default | Description |
|-------|---------|-------------|
| `darkfid_url` | `http://127.0.0.1:8543` | DarkFi node JSON-RPC endpoint |
| `poll_interval_secs` | 10 | How often to check for new withdrawals |
| `max_concurrent_withdrawals` | 10 | Max simultaneous withdrawal executions |

#### Ethereum

| Field | Required | Description |
|-------|----------|-------------|
| `enabled` | Yes | Enable ETH withdrawals |
| `node_url` | Yes | Ethereum RPC URL (Infura/Alchemy) |
| `relayer_private_key` | Yes | Private key for signing ETH transactions |
| `max_gas_gwei` | Yes | Maximum gas price in gwei |
| `max_gas` | Yes | Maximum gas limit per transaction |

#### Monero

| Field | Required | Description |
|-------|----------|-------------|
| `enabled` | Yes | Enable XMR withdrawals |
| `wallet_rpc_url` | Yes | Monero wallet RPC endpoint |
| `node_rpc_url` | Yes | Monero daemon RPC endpoint |
| `view_key` | Yes | View key for observing deposits |
| `fee_address` | Yes | Address for relayer fees |

#### Zcash

| Field | Required | Description |
|-------|----------|-------------|
| `enabled` | Yes | Enable ZEC withdrawals |
| `node_rpc_url` | Yes | Zcash daemon RPC endpoint |
| `shielded_pool` | No | Use shielded (z-addrs) or transparent |

#### Aztec

| Field | Required | Description |
|-------|----------|-------------|
| `enabled` | Yes | Enable Aztec withdrawals |
| `rollup_address` | Yes | Aztec rollup contract address |
| `sequencer_url` | Yes | Aztec sequencer API endpoint |

#### Litecoin

| Field | Required | Description |
|-------|----------|-------------|
| `enabled` | Yes | Enable LTC withdrawals |
| `node_rpc_url` | Yes | Litecoin daemon RPC endpoint |
| `rpc_user` | Yes | RPC username |
| `rpc_pass` | Yes | RPC password |

#### Relayer Settings

| Field | Default | Description |
|-------|---------|-------------|
| `timeout_blocks` | 100 | Blocks before withdrawal can be cancelled |
| `fee_percentage` | 1 | Fee taken from withdrawals (1 = 1%) |

## Running the Relayer

### Start the Relayer

```bash
# Basic usage
./universal_relayer --config universal_relayer_config.toml start

# With verbose logging
./universal_relayer --config universal_relayer_config.toml --verbose start

# Show status
./universal_relayer --config universal_relayer_config.toml status
```

### CLI Commands

```bash
# Start the relayer (default if no subcommand given)
universal_relayer start

# Show relayer status
universal_relayer status

# Derive a bridge address for testing
universal_relayer derive-address <pub_x> <pub_y> <nonce>
```

### Docker (Optional)

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build -p universal_relayer --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/universal_relayer /usr/local/bin/
ENTRYPOINT ["universal_relayer"]
```

## Operation

### Startup Sequence

```
1. Load and validate configuration
2. Initialize executor registry for enabled chains
3. Connect to DarkFi via JSON-RPC
4. Begin polling for pending withdrawals
5. Main loop: poll → process → sleep
```

### Main Loop

```rust
loop {
    // Get current block height
    let current_height = watcher.get_current_height().await?;

    // Fetch pending withdrawals
    let pending = watcher.get_pending_withdrawals().await?;

    for withdrawal in pending {
        // Skip if timed out
        if withdrawal.is_timed_out(current_height) {
            continue;
        }

        // Get executor for this chain
        let executor = executors.get_executor(withdrawal.chain);

        // Execute withdrawal
        match executor.execute(&withdrawal).await {
            Ok(tx_hash) => {
                watcher.mark_processed(&withdrawal.id);
            }
            Err(e) => {
                // Log error, continue with next
            }
        }
    }

    // Wait before next poll
    sleep(Duration::from_secs(poll_interval)).await;
}
```

### Monitoring

```bash
# View logs (systemd)
journalctl -u universal_relayer -f

# Check status
./universal_relayer status

# View pending withdrawals (via darkfid RPC)
curl -X POST http://127.0.0.1:8543 -d '{
  "jsonrpc": "2.0",
  "method": "bridge.get_pending_withdrawals",
  "params": [],
  "id": 1
}'
```

## Security Considerations

### Relayer Trust Model

```
┌─────────────────────────────────────────────────────────────────┐
│                    Relayer Trust Model                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Relayer is "honest-but-curious":                               │
│                                                                 │
│  ✓ CAN observe all pending withdrawals                          │
│  ✓ CAN learn recipient addresses when executing                  │
│  ✗ CANNOT steal funds (no signing authority for deposits)      │
│  ✗ CANNOT double-spend (nullifiers prevent this)                │
│                                                                 │
│  Economic Incentives:                                           │
│  • Earn fees on successful withdrawals                           │
│  • Get slashed (lose stake) if withdrawal times out              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Key Security Points

1. **Private Key Security**: Your relayer private key signs withdrawals. Keep it secure.

2. **View Keys are Observation-Only**:
   - Monero view key can only see incoming transactions
   - Cannot spend funds
   - Can be safely shared with relayer

3. **Timeout Protection**: Users can cancel after 100 blocks if relayer fails

4. **Fresh Addresses**: Each deposit uses a nonce-derived address, preventing cross-deposit correlation

### Best Practices

```
1. Use a dedicated machine/VM for the relayer
2. Never expose private keys in configuration files (use environment variables)
3. Monitor disk space on full nodes
4. Set up alerting for failed withdrawals
5. Keep software updated
6. Use hardware wallets for large reserves
```

## Integration with DarkFi Bridge

The relayer is part of DarkFi's bridge architecture:

```
External Chain          DarkFi                Relayer
     │                    │                     │
     │  User deposits     │                     │
     │───────────────────>│                     │
     │                    │                     │
     │                    │  Polls for deposits │
     │                   │<────────────────────│
     │                    │                     │
     │                    │  Observes deposit   │
     │                    │────────────────────>│
     │                    │                     │
     │                    │  Constructs proof   │
     │                    │<────────────────────│
     │                    │                     │
     │                    │  Submits to bridge  │
     │                    │────────────────────>│
     │                    │                     │
     │  User withdraws    │                     │
     │───────────────────>│                     │
     │                    │                     │
     │                    │  Pending withdrawal │
     │                    │<────────────────────│
     │                    │                     │
     │                    │  Executes on chain  │
     │<─────────────────────────────────────────│
     │                    │                     │
```

See [Bridge Documentation](../arch/bridge.md) for detailed architecture.

## Troubleshooting

### Common Issues

#### Connection Refused to DarkFi

```bash
Error: Failed to connect to darkfid: Connection refused

# Solution: Ensure darkfid is running
systemctl status darkfid
# or
darkfid --config darkfid_config.toml
```

#### Invalid Private Key (Ethereum)

```bash
Error: Ethereum: RPC error: Invalid params

# Solution: Ensure private key is hex-encoded with 0x prefix
# e.g., "0x1234..." not "1234..."
```

#### Monero View Key Not Working

```bash
Error: Monero error: Invalid view key

# Solution: Ensure view key is correctly formatted (64 character hex)
# Can be obtained via: monero-wallet-cli show_view_key
```

#### Insufficient Funds for Gas (Ethereum)

```bash
Error: Ethereum: insufficient funds for gas

# Solution: Ensure ETH balance on relayer address
# Watch gas prices if consistently failing
```

### Debug Mode

```bash
# Enable debug logging
./universal_relayer --verbose --config config.toml start

# This will show:
# - Detailed withdrawal processing
# - RPC calls to external chains
# - Confirmation verification steps
```

## References

- [Bridge Architecture](../arch/bridge.md) - Detailed bridge documentation
- [Atomic Swaps](../testnet/atomic-swap.md) - How atomic swaps work in DarkFi
- [Object Capability Security](https://en.wikipedia.org/wiki/Object-capability_model) - Security model explanation
- [Monero Documentation](https://www.getmonero.org/get-started/accepting/) - Monero setup
- [Zcash Documentation](https://zcash.readthedocs.io/) - Zcash setup
- [Aztec Documentation](https://docs.aztec.network/) - Aztec rollup integration
