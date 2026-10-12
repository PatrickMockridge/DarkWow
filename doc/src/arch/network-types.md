# Network Types

DarkWow defines three network configurations. Two share the same code path
(`Dwowd::init_linear()`) — only TOML settings differ.

## darkwow-devnet (formerly linear-testnet)

**Local development network** — optimized for fast iteration.

| Setting | Value |
|---------|-------|
| Magic bytes | `[241, 227, 197, 179]` |
| Port range | 28xxx (`rpc_listen = "tcp://127.0.0.1:28345"`) |
| Sync | `skip_sync = true`, `localnet = true` |
| Difficulty | Fixed: `pow_fixed_difficulty = 1` |
| P2P | inherits the default `outbound_connections = 8` (the section does not override it) |
| Purpose | Single-node local dev, contract testing, merge mining tests |

```bash
cargo run -p dwowd -- --network darkwow-devnet
```

## darkwow-testnet

**Public testnet** — multi-node coordination with real P2P networking.

| Setting | Value |
|---------|-------|
| Magic bytes | `DRKW` |
| Port range | 31xxx |
| Sync | Full P2P sync, `localnet = false` |
| Difficulty | Variable (adjusts to hashrate) |
| P2P | `outbound_connections = 8`, seed nodes via lilith |
| Purpose | Public coordination, pre-mainnet testing |

```bash
cargo run -p dwowd -- --network darkwow-testnet
```

Requires lilith seed nodes for peer discovery and a persistent `p2p_hostlist.tsv`.

## mainnet

**Production network** — placeholder configuration, not yet live.

| Setting | Value |
|---------|-------|
| Magic bytes | TBD |
| Port range | TBD |
| Sync | Full P2P sync |
| Difficulty | Variable |
| Purpose | Production |

## Why Two Networks?

- **darkwow-devnet:** Fast local iteration. No sync delay, no P2P peers needed,
  fixed difficulty for deterministic testing. Ideal for contract development,
  merge mining tests, and CI pipelines.

- **darkwow-testnet:** Public coordination. Real P2P networking, variable
  difficulty that responds to hashrate, seed nodes for peer discovery.
  Tests the same code path as mainnet will use.

Both run the same `init_linear()` code path. The difference is purely in the
TOML configuration file (`dwowd_config.toml`), which can be overridden per-network.

## Configuration File

The TOML config (`bin/dwowd/dwowd_config.toml`) defines per-network sections:

```toml
network = "darkwow-testnet"  # the config file's default; the CLI default is darkwow-devnet (`--network`)

[network_config."darkwow-devnet"]
database = "~/.local/share/dwow/dwowd/darkwow-devnet"
# ... 28xxx ports, localnet, skip_sync

[network_config."darkwow-testnet"]
database = "~/.local/share/dwow/dwowd/darkwow-testnet"
# ... 31xxx ports, P2P seeds, variable difficulty
```

Select the network at runtime with `--network`:

```bash
dwowd --network darkwow-devnet    # local dev
dwowd --network darkwow-testnet   # public testnet
```
