# Native Mining + Contract Workflow

Run a dwowd fullnode on the public DarkWow testnet, mine DRKW coins with xmrig to
a local wallet, then deploy and interact with any of the 28+ repo smart contracts.
No Docker required — everything runs natively on the host.

This workflow is fully automated via `native-workflow.sh` and also documented
step-by-step below for manual runs.

## Prerequisites

- **Rust toolchain** (stable, via rustup)
- **Build dependencies** (clang, openssl, pkg-config, cmake)
- **xmrig** (RandomX CPU miner) — [xmrig.com/download](https://xmrig.com/download)
- **Built binaries:** `make` from the repo root
- **Config:** `bin/dwowd/dwowd_config.toml` with stratum enabled and `tcp+tls`
  in `active_profiles`

Check your config before starting:

```bash
grep -A2 'stratum_rpc' ~/.config/dwow/dwowd_config.toml | grep -q rpc_listen \
    && echo "stratum: OK" || echo "stratum: MISSING"
grep active_profiles ~/.config/dwow/dwowd_config.toml | grep -q tcp+tls \
    && echo "tcp+tls: OK" || echo "tcp+tls: MISSING"
```

## Quick Start (Automated)

```bash
# Run the full automated workflow
./contrib/docker/testnet-node/native-workflow.sh

# Skip xmrig (e.g., in CI or if mining separately)
SKIP_XMRIG=true ./contrib/docker/testnet-node/native-workflow.sh

# Verbose output
VERBOSE=true ./contrib/docker/testnet-node/native-workflow.sh
```

## Manual Step-by-Step

### 1. Build Everything

```bash
make
```

This compiles dwowd, dwow_wallet (wallet CLI), and all 28 contract WASMs.

### 2. Start dwowd

```bash
RUST_MIN_STACK=67108864 ./target/release/dwowd --network darkwow-testnet &
```

The daemon connects to public testnet seeds (`lilith0.dark.fi:31340`,
`lilith1.dark.fi:31340`), syncs the blockchain, and auto-generates a mining
keypair on first start.

### 3. Wait for Sync

Poll the RPC until the chain height advances:

```bash
# RPC uses raw TCP JSON-RPC, not HTTP
exec 3<>/dev/tcp/127.0.0.1/31345
echo '{"method":"blockchain.info","params":[],"id":1}' >&3
cat <&3
exec 3>&-
```

Wait until `height` > 0 and `peer_count` > 0.

### 4. Set Up Wallet

Generate a keypair as described in [Wallet Architecture](../arch/wallet.md).
Use `-n darkwow-testnet` for the public testnet.

### 5. Import Mining Secret

dwowd auto-generates a mining keypair on first start. The secret is stored at
`~/.local/share/dwow/dwowd/darkwow-testnet/mining_secret`. Import it into dwow_wallet
so mining rewards are spendable:

```bash
# Read the auto-generated mining secret
MINING_SECRET=$(cat ~/.local/share/dwow/dwowd/darkwow-testnet/mining_secret)

# Import into wallet
./target/release/dwow_wallet -n darkwow-testnet wallet import-secret "$MINING_SECRET"
```

Verify the mining address matches:

```bash
# This should match dwowd's mining address
cat ~/.local/share/dwow/dwowd/darkwow-testnet/mining_address
```

### 6. Start Mining

```bash
MINING_ADDR=$(cat ~/.local/share/dwow/dwowd/darkwow-testnet/mining_address)
xmrig -o stratum+tcp://127.0.0.1:31347 -u "$MINING_ADDR" -a rx/0 -t 1 --keepalive &
```

Adjust `-t` to the number of CPU threads you want to allocate.

### 7. Wait for Blocks

Mine until a few blocks are produced:

```bash
for i in $(seq 1 60); do
    HEIGHT=$(exec 3<>/dev/tcp/127.0.0.1/31345; echo '{"method":"blockchain.info","params":[],"id":1}' >&3; timeout 3 cat <&3 | grep -o '"height":[0-9]*' | cut -d: -f2)
    echo "Height: $HEIGHT"
    [ "$HEIGHT" -ge 5 ] && break
    sleep 10
done
```

### 8. Scan for Coins

```bash
./target/release/dwow_wallet -n darkwow-testnet scan
```

### 9. Check Balance

```bash
./target/release/dwow_wallet -n darkwow-testnet wallet balance
```

### 10. Deploy a Contract (Example: Escrow)

Promissory Note is deployed at genesis — no need to deploy it. For other
contracts, use the standard deploy flow:

```bash
# Generate deploy authority
DEPLOY_AUTH=$(./target/release/dwow_wallet -n darkwow-testnet contract generate-deploy)

# Deploy the contract
./target/release/dwow_wallet -n darkwow-testnet contract deploy "$DEPLOY_AUTH" \
    ./src/contract/escrow/escrow.wasm | \
    ./target/release/dwow_wallet -n darkwow-testnet broadcast

# After getting the ContractId from the deploy output, register it:
./target/release/dwow_wallet -n darkwow-testnet contract register escrow <ContractId>
```

### 11. Send a Transfer

```bash
# Self-transfer 100,000,000 DARK (0.1 DRKW)
ADDR=$(./target/release/dwow_wallet -n darkwow-testnet wallet address | tail -1)
./target/release/dwow_wallet -n darkwow-testnet transfer 100000000 DRKW "$ADDR" | \
    ./target/release/dwow_wallet -n darkwow-testnet broadcast
```

Note: Every transaction requires a 42,000,000 DARK fee.

### 12. Invoke a Contract Function

```bash
# Invoke a function on a registered contract
./target/release/dwow_wallet -n darkwow-testnet contract invoke <ContractId> <function>
```

## Contract Commands Reference

All 28 repo contracts are recognized by the wallet. Commands:

| Command | Description |
|---------|-------------|
| `contract list` | List registered contracts and deploy authorities |
| `contract generate-deploy` | Generate a new deploy authority |
| `contract deploy <secret> <wasm>` | Deploy a WASM contract |
| `contract register <name> <id>` | Register a deployed ContractId |
| `contract invoke <id> <function> [--params <json>]` | Call a contract function |
| `contract lock <secret>` | Lock a deployed contract (immutable) |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `NETWORK` | `darkwow-testnet` | Blockchain network |
| `RPC_PORT` | `31345` | dwowd JSON-RPC port |
| `STRATUM_PORT` | `31347` | Stratum mining port |
| `MINING_THREADS` | `2` | xmrig thread count |
| `TARGET_HEIGHT` | `10` | Target block height for sync |
| `SKIP_XMRIG` | `false` | Skip xmrig start |
| `TIMEOUT` | `600` | Max seconds per wait loop |
| `VERBOSE` | `false` | Verbose output |

## Data Layout

| Path | Contents |
|------|----------|
| `~/.config/dwow/dwowd_config.toml` | dwowd configuration |
| `~/.local/share/dwow/dwowd/darkwow-testnet/` | Blockchain database |
| `~/.local/share/dwow/dwowd/darkwow-testnet/mining_address` | Auto-generated mining address |
| `~/.local/share/dwow/dwowd/darkwow-testnet/mining_secret` | Auto-generated mining secret |
| `~/.local/share/dwow/dww/darkwow-testnet/` | Wallet database |

## Troubleshooting

**dwowd won't start.** Check `RUST_MIN_STACK=67108864` is set (RandomX requires large
stack frames). Verify `dwowd_config.toml` has `tcp+tls` in active_profiles.

**No peers.** Seeds may be unreachable. Verify DNS: `nslookup lilith0.dark.fi`.
Check that `tcp+tls://0.0.0.0:31342` inbound is configured.

**No mining rewards.** Mining is competitive. On a public testnet, rewards arrive when
your node finds a block. Check `xmrig` output for accepted shares. Block reward follows
exponential decay from ~13.84 DRKW at height 1. The wallet address must match the
mining address stored in dwowd's database.

**Contract deploy fails.** The fee is 42,000,000 DARK — ensure your balance covers it.
Check that `promissory_note.wasm` exists at `src/contract/promissory_note/` (built by `make contracts`).

**Wallet balance shows zero after mining.** Run `dwow_wallet scan` first — the wallet must
scan the blockchain to find coins belonging to its keys.

**Port already in use.** Another `dwowd` process may be running. Check:
`ss -tlnp | grep -E '31345|31347|31342'`
