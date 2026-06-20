# Wallet Testing in Dockernet

How to run a wallet container in the dockernet pipeline and test transactions.
Every command verified against source. Every guardrail documented.

## Architecture

The dockernet runs `dwowd` mining nodes and a `dwow_wallet` container on a
shared Docker bridge network. The wallet is a **full P2P node** — it connects
to the seed (lilith), discovers peers via hostlist, syncs the chain via
GetTip/GetBlocks, scans blocks locally with AEAD decryption, and discovers
coins and capabilities. It builds transactions locally and broadcasts them
via P2P gossip (TxMessage). **Zero RPC.**

```
Docker bridge (darkwow-testnet_dwow-local)
─────────────────────────────────────────────
lilith (seed)   node0 (miner)   node1 (miner)   dwow-wallet-1 (full node)
31340           31342           31343            31360
```

The wallet container runs `/app/dwow_wallet` with config at
`/root/.config/dwow/dww_config.toml`. The config has a `[net]` section with
`seeds = ["tcp+tls://lilith:31340"]`, `localnet = true`, and matching
`magic_bytes = [68, 82, 75, 87]`.

### Secret Provisioning

Pipeline Phase 4 generates N independent DarkWow keypairs (one per wallet).
Each hex secret is written to `/tmp/dwow_mining_secret_$i`. The pipeline
bind-mounts the indexed file into the corresponding wallet container at
`/run/secrets/mining_secret:ro`. The entrypoint converts hex to bs58 and
imports it via `wallet import-secrets`.

**FORWARD_DESTINATION**: The pipeline sets `FORWARD_DESTINATION` to wallet-1's
address. Mining nodes encrypt coinbase AEAD notes to this public key — the
miner never knows the wallet's secret, only its address. Wallet-1 decrypts
the coinbase during AEAD scan with its matching secret. Wallet-2 (and above)
are funded via a transfer from wallet-1 in `phase_wallet_transfer`.

**No key sharing**: The miner encrypts TO the wallet's public key. It does
not hold the wallet's secret. Only the wallet can decrypt. The miner's
consensus keypair and the wallet's spending keypair are cryptographically
independent. See [dwowd Coinbase Forwarding](../../dwowd.md#coinbase-forwarding).

### Verify the key sharing

```bash
# Mining secrets must exist before docker compose starts
test -f /tmp/dwow_mining_secret_1 && echo "OK" || echo "MISSING — secret provisioning failed"

# Secret must be 64 hex chars (32 bytes)
test "$(wc -c < /tmp/dwow_mining_secret_1)" -eq 64 && echo "OK" || echo "BAD LENGTH"

# Verify wallet address matches FORWARD_DESTINATION
docker exec dwow-wallet-1 /app/dwow_wallet wallet address
# Must output the address passed as FORWARD_DESTINATION
```

## Pre-Flight Checklist

| # | Check | Command |
|---|-------|---------|
| 1 | Latest code pushed | `git push origin linear-master --dry-run` |
| 2 | No stale containers | `docker ps --filter name=dwow --format '{{.Names}}'` (should be empty) |
| 3 | Secret provisioned | `test -f /tmp/dwow_mining_secret_1 && test $(wc -c < /tmp/dwow_mining_secret_1) -eq 64` |
| 4 | Python model passes | `python3 contrib/model/wallet_model.py` (87/87) |

## Pipeline Start

```bash
# Full clean build with wallet container (N=1: verify only. N>=2: + transfer test)
FORWARD_DESTINATION="<wallet_bs58_address>" \
  ./contrib/docker/darkwow-testnet/test_pipeline.sh \
  --mode native --with-wallet 2 --fresh
```

**Pipeline phases (--with-wallet):** clean (1) → build (2) → validate prereqs (3) →
generate wallet (4) → start containers (5) → verify containers (6) → RPC health (7) →
mining activity (8) → block production (9) → wallet verify (10: sync/scan/balance) →
wallet transfer (11: wallet-1→wallet-2) → report (20).

Expected: 20+ PASS, 0 FAIL. Height ≥ 2 after mining phase. Wallet container
(`dwow-wallet-1`) running.

## Wallet Operations

Use `wallet-shell.sh` for consistent interaction:

```bash
source contrib/docker/darkwow-testnet/wallet-shell.sh
```

`wal()` wraps: `docker exec "dwow-wallet-$N" /app/dwow_wallet "$@"`

### Sync

```bash
wal 1 sync init       # Connect to seeds, start P2P sync
wal 1 sync status     # Local height, network tip, sync status
```

Expected: `sync init` → "P2P sync started." `sync status` → shows local height > 0
and network tip.

**Guardrail 1: P2P connected**
If `sync status` shows "P2P connected: no": STOP. The wallet config is missing
the `[net]` section or the seed address is wrong.

**Guardrail 2: Peers discovered**
If "Local chain height: 0" persists after sync init: STOP. The wallet connected
to lilith but has no peers. Mining nodes may not have registered with the seed
yet. Wait 30s and retry.

### Scan

```bash
wal 1 scan
```

Expected: iterates blocks, processes coinbase and contract calls, finds
capabilities. No "RPC" mentions in output — all local chain reads.

**Guardrail 3: Blocks scanned**
If scan shows "Chain height: 0": STOP. The wallet hasn't synced any blocks.
Run `sync status` first.

### Balance

```bash
wal 1 wallet balance
```

Expected: prettytable with DRKW balance > 0, or "No unspent balances found"
if scan hasn't discovered coins yet.

**Guardrail 4: Coins found**
If balance shows "No unspent balances" after scan: STOP. The wallet secret
doesn't match FORWARD_DESTINATION. See Secret Provisioning above.

### Address

```bash
wal 1 wallet address
```

### Transfer

```bash
ADDR=$(wal 1 wallet address)
wal 1 transfer 1.0 DRKW "$ADDR"
```

Arguments: `<amount> <token> <recipient> [spend_hook] [user_data] [--half_split]`.
Token alias "DRKW" is registered at wallet init. Output: base64-encoded
transaction on stdout.

### Broadcast

The transfer command builds the transaction and broadcasts it via P2P gossip
automatically. For manual broadcast:

```bash
TX=$(wal 1 transfer 1.0 DRKW "$ADDR")
echo "$TX" | wal 1 broadcast
```

Broadcast flow: serialize tx → `p2p.broadcast(&TxMessage)` → return txid.

**Note:** `docker exec -i` is required for stdin pipe. Without `-i`, broadcast
reads empty stdin and fails.

### Verify after transfer

```bash
wal 1 scan
wal 1 wallet balance
```

## Known Failure Modes

| Symptom | Root cause | Fix |
|---------|-----------|-----|
| `P2P not configured` | Config missing `[net]` section | Rebuild wallet image with `--fresh` |
| Wallet scan: no coins found | Secret mismatch (FM11) | Verify wallet address = FORWARD_DESTINATION |
| `sync status`: height 0 after init | No peers or seed unreachable | Wait for mining nodes to register with seed |
| `sync status`: P2P connected: no | `[net]` section missing or seeds wrong | Check `/root/.config/dwow/dww_config.toml` in container |
| `Token not found: DRKW` | Wallet not initialized | Run `wal 1 wallet initialize` |
| Broadcast `Error reading stdin` | Missing `-i` flag | Use `docker exec -i` |
| Broadcast succeeds but tx not in block | P2P gossip not reaching miners | Check peer connectivity with `sync status` |

## Testing vs Production

This dockernet pattern uses several testing expediencies documented in
[Level 3: Local Docker → Public Testnet → Mainnet Transition](level-3-localnet.md#local-docker--public-testnet--mainnet-transition).
Key differences:

| Aspect | Dockernet Testing | Production |
|--------|------------------|------------|
| `localnet` | `true` (TLS verification disabled) | `false` |
| Secret sharing | Same keypair for miner + wallet | Separate keypairs |
| Seed address | `tcp+tls://lilith:31340` (Docker DNS) | Public DNS seeds |
| Wallet location | Docker container on bridge network | Native binary on user machine |
| Hostlist resolution | Docker embedded DNS | System DNS / public IPs |
| Broadcast confirmation | Not tested automatically | Full spend→mine→confirm cycle in CI |

## Tear Down

```bash
docker compose --profile native --profile wallet -p darkwow-testnet down -v
docker rm -f dwow-wallet-1 2>/dev/null
docker volume rm wallet_data_1 2>/dev/null
rm -f /tmp/dwow_mining_secret
```
