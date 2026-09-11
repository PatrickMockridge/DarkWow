# dww

DarkWow wallet CLI. A full-node wallet that holds the complete blockchain on
local disk, scans for capabilities, manages keys via the AccountManager, and
builds transactions. Uses manifest-first architecture — contracts carry
their own interfaces on-chain; zero wallet code changes for new contracts.

## Building

```shell
make
```

## Usage

The wallet is normally invoked through the `darkwow` launcher
(`/app/darkwow` in containers, `darkwow` on PATH otherwise), which execs
this binary verbatim:

```shell
darkwow wallet balance
darkwow wallet -c dww_config.toml -n darkwow-devnet wallet balance
```

Or run the binary directly:

```shell
./target/release/dwow_wallet --help
./target/release/dwow_wallet -c dww_config.toml -n darkwow-devnet wallet balance
```

Commands do one thing and exit; `daemon` is the exception (persistent
sync+scan + unix-socket JSON-RPC).

| Command | Description |
|---------|-------------|
| `wallet initialize` | Initialize the wallet DB. |
| `wallet balance [--porcelain]` | Per-asset balances (tab-separated; `--porcelain` = `asset_id | amount`). |
| `wallet address` / `addresses` | Default wallet address / all derived addresses. |
| `wallet default-address <i>` | Address at index `i`. |
| `wallet secrets` | Show declared secrets. |
| `wallet tree` | Merkle tree state. |
| `wallet capabilities` (`coins`) | Held capabilities table (Asset ID / Aliases / Value / Spend Hook / User Data). |
| `sync init` / `sync status` | Start P2P sync / show local height vs network tip. |
| `scan` | Scan synced blocks for capabilities. |
| `transfer <amount> <token> <recipient>` | Build a native transfer, print base64, auto-broadcast. |
| `broadcast` | Broadcast a transaction read **binary** from stdin. |
| `redeem <cap_id>` / `burn <cap_ids>` | Dispatch to an error directing `contract invoke <cid> redeem`. |
| `contract deploy <auth> <wasm>` | Deploy a contract. |
| `contract show <cid>` | Show contract state. |
| `contract lock <auth>` | Lock a contract. |
| `contract invoke <cid> <function>` | Generic contract call (the spend path for everything non-native). |
| `position` | Capabilities held and available actions. |
| `diagnostic` | P2P, sync, chain, seed connectivity report. |
| `daemon` | Persistent sync + auto-scan + unix-socket JSON-RPC at `/tmp/dww-{network}.sock`. |

## Documentation

- [Node Operator Guide](../../doc/src/for-node-operators.md) — Wallet node operation
- [Wallet Architecture](../../doc/src/arch/wallet.md) — Full wallet specification
- [Local Devnet Setup](../../doc/src/localnet-dev.md) — Mining and balance checks
