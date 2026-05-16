# Universal Relayer

> **USE AT YOUR OWN RISK.** The universal relayer handles real funds across multiple blockchains. It has undergone internal simulation-based review but has NOT been independently audited. Running a relayer carries operational, financial, and security risks. See [AUDIT.md](../../src/contract/AUDIT.md) for contract-level findings.

*Multi-chain relayer service for executing DarkWow bridge withdrawals to external blockchains.*

## Overview

The Universal Relayer monitors the DarkWow bridge contract for pending withdrawals and executes the corresponding transactions on external chains (Ethereum, Monero, Zcash, Aztec, Litecoin). It is the operational backbone of DarkWow's cross-chain bridge.

```
┌──────────────────────────────────────────────────────────────┐
│                   Universal Relayer                           │
├──────────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │ Ethereum │  │  Monero  │  │  Zcash   │  │  Aztec   │    │
│  │ Executor │  │ Executor │  │ Executor │  │ Executor │    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘    │
│       └──────────────┴────────────┴──────────────┘          │
│                          │                                    │
│               ┌──────────┴──────────┐                        │
│               │   Watcher/Poller    │                        │
│               └──────────┬──────────┘                        │
│                          │                                    │
└──────────────────────────┼──────────────────────────────────┘
                           │
               ┌───────────┴───────────┐
               │  DarkWow Bridge       │
               │  (Pending Withdrawals) │
               └───────────────────────┘
```

## Architecture

| Component | Purpose |
|-----------|---------|
| **Watcher** | Polls DarkWow for new pending withdrawals and monitors block height |
| **Executor Registry** | Routes withdrawals to the correct chain executor |
| **Chain Executors** | One per supported chain — builds, signs, and broadcasts external chain transactions |
| **Stake Manager** | Tracks relayer stake, locked coverage, and slash history |
| **Feed Manager** | Computes withdrawal fees based on market conditions and chain |
| **Health Monitor** | Watchdog process that detects stalls and triggers recovery (planned) |

## Supported Chains

| Chain | Token | Confirmations | Privacy Model |
|-------|-------|---------------|---------------|
| Ethereum | ETH | 12 | Transparent |
| Monero | XMR | 10 | Ring signatures |
| Zcash | ZEC | 10 | Sapling shielded |
| Aztec | ETH/DAI | 5 | Private rollup |
| Litecoin | LTC | 6 | Transparent + MWEB |

## Hardening Features (May 2026)

The following hardening features from the May 2026 security review are relevant to relayer operators:

### Withdrawal Reassignment

If a relayer goes offline after accepting a withdrawal, other relayers can claim the stuck withdrawal via `ReassignWithdrawalV1` (bridge opcode `0x09`) after `reassignable_after` blocks. The original relayer is partially slashed. Operators should monitor for reassigned withdrawals as a signal of infrastructure problems.

### Fee Caps

The bridge contract enforces `MAX_FEE_BP = 1000` (10% maximum). Users can specify tighter caps via `max_fee_bp` in their withdrawal parameters. Relay operators should publish their fee schedules via the `relayer_getFeeSchedule` JSON-RPC endpoint.

### Force Settlement (Endowment)

Backers who deployed capital via the relayer_endowment contract can force pro-rata fee settlement after 1000 blocks of relayer inactivity (`ForceSettleV1`). Relay operators should settle fees regularly to avoid forced settlements, which signal poor operational health to backers.

### Proportional Slashing

Slash penalties now scale with withdrawal amount: `max(1_000_000, amount * 1000 / 10000)` — i.e., 10% of withdrawal value with a 1 DAI floor. Previously a flat 1 DAI regardless of amount.

## Configuration

### Minimal Config

```toml
[darkfi]
dwowd_url = "http://127.0.0.1:8543"
poll_interval_secs = 10

[ethereum]
enabled = true
node_url = "https://mainnet.infura.io/v3/YOUR_KEY"
relayer_private_key = "0x..."

[relayer]
timeout_blocks = 100
fee_percentage = 1

[fee_limits]
max_fee_bp = 1000
min_fee = 100
```

### Key Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `poll_interval_secs` | 10 | How often to check for new withdrawals |
| `timeout_blocks` | 100 | Blocks before withdrawal can be cancelled |
| `fee_percentage` | 1 | Fee taken from withdrawals (1 = 1%) |
| `max_fee_bp` | 1000 | Maximum fee in basis points (10%) |
| `min_fee` | 100 | Minimum fee floor |

## JSON-RPC Endpoints

| Method | Description |
|--------|-------------|
| `relayer_getFeeSchedule` | Returns current fee schedule (planned) |
| `relayer_getStakeProof` | Returns proof of relayer stake coverage (planned) |
| `relayer_getPoolStatus` | Returns pool membership and reputation scores (planned) |

## Operations

### Build

```bash
cargo build -p universal_relayer --release
```

### Run

```bash
./universal_relayer --config universal_relayer_config.toml start
./universal_relayer --config universal_relayer_config.toml status
```

### Main Loop

```
1. Poll DarkWow for pending withdrawals
2. Skip timed-out withdrawals (current_height > timeout_height)
3. Route to correct chain executor
4. Execute on external chain (sign + broadcast tx)
5. Verify confirmation count reached
6. Mark withdrawal complete
7. Settle fees to endowment (if configured)
8. Sleep for poll_interval_secs
```

## Security

### Trust Model

Relayers are **honest-but-curious**:
- CAN observe all pending withdrawals
- CAN learn recipient addresses when executing
- CANNOT steal funds (no signing authority for deposits)
- CANNOT double-spend (nullifiers prevent this)

### Best Practices

1. Use a dedicated machine/VM — do not share with other services
2. Never expose private keys in config files — use environment variables or HSMs
3. Monitor disk space on full nodes — chain data grows continuously
4. Set up alerting for failed withdrawals and reassigned withdrawals
5. Keep software updated — subscribe to release notifications
6. Settle endowment fees regularly — avoid force settlements
7. Publish fee schedule — transparency attracts users
8. Test with small amounts before handling significant volume

### Key Custody

- **Ethereum/Aztec/Litecoin**: Private key signs withdrawal transactions. Compromise means loss of gas funds and ability to front-run withdrawals.
- **Monero**: View key only — cannot spend funds. Safe to share with relayer.
- **Zcash**: Viewing key for shielded pools. Cannot spend funds.

### Slashing Risks

Relayers are slashed for:
1. Failing to execute a guaranteed withdrawal within timeout (`SLASH_BP` = 10% of withdrawal amount, min `MIN_SLASH` = 1 DAI)
2. Abandoning a withdrawal that gets reassigned (50% of slash amount)

## See Also

- [Bridge Contract README](../../src/contract/bridge/README.md)
- [Relayer Endowment README](../../src/contract/relayer_endowment/README.md)
- [Security Audit](../../src/contract/AUDIT.md)
- [Relayer Operations Guide](../../doc/src/relayer/relayer.md)
- [Relayer Economics](../../doc/src/relayer/relayer_economics.md)
