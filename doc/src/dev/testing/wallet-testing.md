# Wallet Testing in Dockernet

How to run a wallet container in the dockernet pipeline and test transactions.
Every command verified against source. Every guardrail documented. No
"winging it."

## Architecture

The dockernet runs `dwowd` mining nodes and a `dwow_wallet` container on a
shared Docker bridge network. The wallet is a thick client — it fetches blocks
from `dwowd` via raw TCP JSON-RPC, decrypts coinbase notes with its secret key,
and discovers coins. It builds transactions locally and broadcasts them to
`dwowd` for mempool submission.

### Key sharing

Pipeline Phase 3 generates a DarkWow keypair. The hex secret is written to
`/tmp/dwow_mining_secret`. Docker compose bind-mounts this file into every
node container at `/run/secrets/mining_secret` and sets
`WALLET_SECRET_FILE=/run/secrets/mining_secret`. Each node's entrypoint reads
the file and writes it to the miner's key storage. The wallet container
converts the hex secret to bs58 and imports it via `wallet import-secrets`.

**All containers use the same keypair.** The wallet can decrypt every coinbase
note because they were encrypted with the same public key.

### Verify the key sharing

```bash
# Mining secret must exist before docker compose starts
test -f /tmp/dwow_mining_secret && echo "OK" || echo "MISSING — pipeline Phase 3 may have failed"

# Inside any node container
docker exec dwow-node0 cat /run/secrets/mining_secret | wc -c
# Must output 64 (64 hex chars = 32 bytes)

# Inside wallet container
docker exec dwow-wallet-1 cat /run/secrets/mining_secret | wc -c
# Must output 64 — same secret
```

## Pre-Flight Checklist

Before running the pipeline, verify:

| # | Check | Command |
|---|-------|---------|
| 1 | Base image has `xxd` | `docker run --rm darkwow-base:24.04 which xxd` |
| 2 | Latest code pushed | `git push origin linear-master --dry-run` |
| 3 | Thread vars set | `echo "DWOW_RAYON_THREADS=$DWOW_RAYON_THREADS MINING_THREADS=$MINING_THREADS"` |
| 4 | No stale containers | `docker ps --filter name=dwow --format '{{.Names}}'` (should be empty) |

## Pipeline Start

```bash
# Remove base image if Dockerfile.base was modified
docker rmi darkwow-base:24.04

# Start with thread containment + fresh build
DWOW_RAYON_THREADS=2 MINING_THREADS=1 RAYON_NUM_THREADS=10 \
  ./contrib/docker/darkwow-testnet/test_pipeline.sh \
  --mode native --nodes 2 --with-wallet 1 --no-cache
```

**Pipeline phases:** clean → prerequisites → wallet generation → build → start
containers → container verification → RPC health → mining activity → block
production → report.

Expected: 24 PASS, 0 FAIL. Height ≥ 2 after mining phase.

## Post-Start Verification

### Guardrail 1: Tools present
```bash
docker exec dwow-wallet-1 which xxd bs58
# Must output:
#   /usr/bin/xxd
#   /usr/local/bin/bs58
```
**If either missing: STOP.** The base image is stale. Remove and rebuild.

### Guardrail 2: Mining secret in nodes
```bash
docker exec dwow-node0 cat /run/secrets/mining_secret | wc -c
# Must output: 64
```
**If not 64: STOP.** The bind-mount or pipeline secret generation failed.

### Guardrail 3: RPC reachable
```bash
docker exec dwow-node0 bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.last_confirmed_block\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3'
# Must contain: "result":
```
**If "method not found": STOP.** Docker image is stale (`--no-cache` needed).

## Wallet Operations

All commands use the container config path. Binary is at `/app/dwow_wallet`.

### Scan for coins
```bash
docker exec dwow-wallet-1 /app/dwow_wallet -c /root/.config/dwow/drk.toml scan
```
Expected: iterates blocks 1..N, "Found coinbase tx, attempting note decryption..."
for each block. Finishes with "Finished scanning blockchain."

### Check balance
```bash
docker exec dwow-wallet-1 /app/dwow_wallet -c /root/.config/dwow/drk.toml wallet balance
```
Expected: prettytable with token balances, or "No unspent balances found" if
scan hasn't discovered coins yet.

**Guardrail 4: Coins found**
If balance shows "No unspent balances" after scan: **STOP.** The wallet secret
doesn't match the mining key. Check key sharing steps above.

### Get address
```bash
docker exec dwow-wallet-1 /app/dwow_wallet -c /root/.config/dwow/drk.toml wallet address
```

### Transfer
```bash
ADDR=$(docker exec dwow-wallet-1 /app/dwow_wallet -c /root/.config/dwow/drk.toml wallet address)
docker exec dwow-wallet-1 /app/dwow_wallet -c /root/.config/dwow/drk.toml transfer 1.0 DRKW "$ADDR"
```
Arguments: `<amount> <token> <recipient> [spend_hook] [user_data] [--half_split]`
All positional (not flags). Token alias "DRKW" is registered at wallet init.
Output: base64-encoded transaction on stdout.

### Broadcast
```bash
TX=$(docker exec dwow-wallet-1 /app/dwow_wallet -c /root/.config/dwow/drk.toml transfer 1.0 DRKW "$ADDR")
echo "$TX" | docker exec -i dwow-wallet-1 /app/dwow_wallet -c /root/.config/dwow/drk.toml broadcast
```
**Note:** `docker exec -i` is required for stdin pipe. Without `-i`, broadcast
reads empty stdin and fails.

Broadcast flow: simulate → mark inputs spent → submit → return txid.

### Verify after transfer
```bash
docker exec dwow-wallet-1 /app/dwow_wallet -c /root/.config/dwow/drk.toml scan
docker exec dwow-wallet-1 /app/dwow_wallet -c /root/.config/dwow/drk.toml wallet balance
```

## Thread Containment

| Variable | Default | Controls |
|----------|---------|----------|
| `DWOW_RAYON_THREADS` | 2 | Rayon thread pool (RandomX dataset init + sled) |
| `MINING_THREADS` | 1 | xmrig mining threads per node |

Set before running the pipeline. These are passed through docker-compose to
the entrypoint which exports `RAYON_NUM_THREADS`. Verified in container:
```bash
docker exec dwow-node0 sh -c 'echo $RAYON_NUM_THREADS'
# Must output: 2
```

## Known Failure Modes

| Symptom | Root cause | Fix |
|---------|-----------|-----|
| `xxd: not found` in wallet logs | Base image cache miss | `docker rmi darkwow-base:24.04` |
| `method not found` RPC error | Docker image cache miss | `--no-cache` |
| Wallet scan: no coins found | Node + wallet use different keys | Verify `/tmp/dwow_mining_secret` exists before compose |
| Broadcast `Error reading stdin` | Missing `-i` flag | Use `docker exec -i` |
| `Token not found: DRKW` | Wallet pre-dates alias fix | Rebuild wallet image |
| Computer freezes | Thread exhaustion | Set `DWOW_RAYON_THREADS=1` |

## Tear Down

```bash
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml \
  --profile native down -v
docker stop dwow-wallet-1 2>/dev/null
docker rm dwow-wallet-1 2>/dev/null
docker volume rm darkwow-testnet_wallet_data_1 2>/dev/null
```
