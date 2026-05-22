# Caribina — Arweave-Anchored Finality Widget

Caribina is an **independent finality layer** for DarkWow that anchors blocks to
Arweave's proof-of-storage consensus chain. It provides a second, orthogonal
hardening barrier against 51% attacks — completely separate from RandomX PoW
and the Monero merge-mining anchor.

Unlike the Monero anchoring finality gadget (which requires p2pool merge mining),
Caribina works for **all** miners — native RandomX miners and merge miners alike.
No p2pool, no Monero node, no AR token funding required.

## Motivation

A 51% attacker who controls the majority of RandomX hashpower can rewrite DarkWow's
chain history. The Monero anchoring finality gadget protects against this, but only
for blocks that reference a confirmed Monero anchor — which requires running p2pool
and a Monero node.

Caribina adds a completely independent finality path:

- **Free**: ArDrive Turbo accepts small uploads (< 100KB) from unfunded wallets
- **Trivial key cycling**: Per-block Ed25519 key generation takes microseconds
- **Fast settlement**: Arweave blocks finalize in ~2 minutes (1 DarkWow block)
  vs ~6 minutes for Monero anchoring (3 Monero blocks)
- **No infrastructure**: Just an HTTP POST to ArDrive Turbo

An attacker who controls RandomX hashpower cannot forge Arweave timestamps.
Arweave uses proof-of-storage consensus — a completely different mechanism.

## How It Works

```
Miner finds block at height H with hash B
    │
    ▼
1. Generate fresh Ed25519 keypair (microseconds)
2. Build ANS-104 DataItem containing: block_hash || timestamp || height
3. Sign DataItem with the fresh key
4. POST signed DataItem to https://upload.ardrive.io/v1/tx/arweave
5. Receive TX ID (32-byte SHA-256 of signature)
6. Set anchor_tx_id in block header
7. Broadcast block
    │
    ▼
Other nodes verify:
   a. Fetch TX by ID from Arweave gateway (GET https://arweave.net/{tx_id})
   b. Check: stored data contains correct block_hash + height + timestamp
   c. Check: Arweave block timestamp ≈ DarkWow block timestamp (±30 min tolerance)
   d. Check: Ed25519 signature is valid
   e. If all pass → block is final (cannot be reorganized)
```

## ANS-104 DataItem Format

Caribina uses Arweave's ANS-104 binary transaction format with Ed25519 signatures
(signature type 2):

```
Bytes   0-1:  signature_type = 2 (u16 LE)
Bytes  2-65:  signature (64 bytes, Ed25519)
Bytes 66-97:  owner / public key (32 bytes)
Byte     98:  target presence (always 0)
Byte     99:  anchor presence (always 0)
Bytes100-107: tag count (u64 LE, 0 for minimum overhead)
Bytes108-115: tag bytes (u64 LE, 0 for minimum overhead)
Bytes  116+:  data payload (block_hash || timestamp || height = 48 bytes)
```

Total: 164 bytes per anchor. Well within ArDrive Turbo's free tier (< 100KB).

Signature data is computed using Arweave's **deepHash** construction
(SHA-384 merkle-like accumulation over the list of DataItem fields).

## deepHash Construction

Per ANS-104 §2.1, the signing data is:

```
deepHash(["dataitem", "1", "2", owner, target, anchor, tags, data])
```

Where:
- `deepHash(list)` = pair-wise SHA-384 accumulation tagged with "list" + count
- `deepHash(blob)` = SHA-384(tag || SHA-384(data)) where tag = SHA-384("blob" + length)

The Arweave transaction ID is `SHA-256(raw_signature)` — the raw 64-byte
signature bytes, not the full DataItem.

## Fork Choice with Caribina

Caribina adds a **finality constraint** to fork choice, identical in structure to
the Monero anchoring gadget:

```
Forks → [Finality Filter: drop forks conflicting with finalized blocks]
              │
              ▼
         Valid forks → best_fork_index() by targets_rank, hashes_rank
              │
              ▼
         Best fork becomes canonical
```

A block with `anchor_tx_id != [0u8; 32]` is considered anchored. Once the Arweave
block containing it settles (default: 1 DarkWow block), the block is **final** and
cannot be replaced by any competing fork — even one with superior PoW rank.

The finality check in `insert_block()`:
```rust
// Don't replace an anchored block at the same height
if existing.header.anchor_tx_id != [0u8; 32] {
    return Err(LinearError::AnchoredBlockConflict);
}
```

## Integration Points

| Component | What it does |
|-----------|-------------|
| **Miner** (`bin/dwowd/src/rpc/miner.rs`) | Anchors after PoW, before broadcast |
| **Stratum** (`bin/dwowd/src/rpc/stratum.rs`) | Anchors after PoW verification, before insert |
| **P2P Handler** (`bin/dwowd/src/proto/linear_broadcast.rs`) | Verifies anchors on received blocks, rejects invalid |
| **Blockchain** (`src/linear/src/blockchain.rs`) | Rejects insertion of blocks replacing anchored ones |
| **BlockHeader** (`src/linear/src/block.rs`) | Carries `anchor_tx_id: [u8; 32]` (zero = no anchor) |

The `anchor_tx_id` field is **excluded from the mining blob** — it is set after
PoW is found, so the block hash does not change after anchoring.

## Finality Flow

```mermaid
sequenceDiagram
    participant Miner
    participant dwowd
    participant ArDrive as ArDrive Turbo
    participant Arweave as Arweave Gateway
    participant Peer as P2P Peer

    Miner->>dwowd: Submit PoW solution
    dwowd->>dwowd: Verify PoW, assemble block
    dwowd->>ArDrive: POST ANS-104 DataItem (hash || timestamp || height)
    ArDrive-->>dwowd: TX ID (32 bytes)
    dwowd->>dwowd: Set anchor_tx_id in header
    dwowd->>dwowd: Set finality_flags |= CARIBNIA
    dwowd->>Peer: Broadcast block
    Peer->>Arweave: GET /{anchor_tx_id}
    Arweave-->>Peer: DataItem + signature
    Peer->>Peer: Verify deepHash signature
    Peer->>Peer: Verify payload matches block
    Peer->>Peer: insert_block() — enforce if mode=Always
```

## Fork Choice with Finality Filter

```mermaid
flowchart TD
    A[Incoming Forks] --> B{Finality Filter}
    B -->|"Conflicts with anchored block?"| C[Drop Fork]
    B -->|"No conflict"| D[Valid Forks]
    D --> E["best_fork_index()"]
    E --> F["Canonical Chain"]
```

## Mode Decision Flow

```mermaid
flowchart TD
    Q["finality.mode?"]
    Q -->|"native"| N[Native:<br/>Skip anchoring<br/>Skip verification<br/>Trust PoW only]
    Q -->|"always"| A[Always:<br/>Anchor every block<br/>Verify all anchors<br/>Enforce all anchors]
    Q -->|"signaled"| S[Signaled:<br/>Anchor + set SIGNALED flag<br/>Verify signaled blocks<br/>Enforce signaled blocks]
```

## Configuration

Finality is configured per-network in `dwowd_config.toml`, with CLI flags
available to override.

### TOML Configuration

```toml
[network_config."darkwow-testnet".finality]
# Mode: "always" (default) | "native" | "signaled"
mode = "always"

# Enable Caribina Arweave anchoring (default: true)
caribina_enabled = true

# Enable Monero anchoring via p2pool (default: false)
monero_enabled = false

# Monero confirmations before finality (default: 3)
monero_min_confirmations = 3

# monerod JSON-RPC URL for full anchor verification (optional)
# monerod_url = "http://127.0.0.1:18081/json_rpc"
```

### CLI Flags

All TOML settings can be overridden from the command line:

```
--finality-mode <MODE>
    Finality enforcement mode: "always" (default), "native", or "signaled"
    - always: Anchor every mined block to Arweave and enforce anchors on received blocks
    - native: Trust PoW only — ignore all anchors
    - signaled: Only enforce finality when a block signals it requires it

--finality-disable-caribina
    Disable Caribina Arweave anchoring entirely

--finality-enable-monero
    Enable Monero p2pool anchoring (default: false)

--monero-min-confirmations <N>
    Monero confirmations required before finality (default: 3)

--monerod-rpc-url <URL>
    monerod JSON-RPC endpoint for full anchor verification
    (e.g. http://127.0.0.1:18081/json_rpc)
```

CLI flags take precedence over TOML. Example:

```bash
# Run with native mode (no finality) — useful for testing
dwowd --network darkwow-testnet --finality-mode native

# Disable only Caribina anchoring, keep enforcement
dwowd --network darkwow-testnet --finality-disable-caribina

# Enable Monero p2pool anchoring with full monerod verification
dwowd --network darkwow-testnet --finality-enable-monero \
    --monerod-rpc-url http://127.0.0.1:18081/json_rpc
```

### Mode Reference

| Mode | Mine with anchor? | Verify on receive? | Enforce on conflict? |
|------|:---:|:---:|:---:|
| **native** | No | No | No — trust PoW only |
| **always** (default) | Yes (Caribina + Monero if enabled) | Yes | Yes — all anchored blocks |
| **signaled** | Yes + flag | Only signaled | Only signaled |

### Default Behavior

By default, nodes run in **always** mode with **Caribina enabled**. This means:

- Every mined block is anchored to Arweave
- Every received block with a non-zero `anchor_tx_id` is verified
- Anchored blocks cannot be replaced (enforced at `insert_block()`)
- No configuration required — maximum resilience out of the box

To disable finality entirely (e.g. for local testing where Arweave HTTP calls
are unwanted latency):

```bash
dwowd --finality-mode native
```

The `signaled` mode is designed for gradual adoption: miners opt in per-block
by setting the `FINALITY_SIGNALED` flag. Nodes in signaled mode only enforce
finality on blocks that carry the flag — un-signaled blocks follow normal PoW
fork choice.

## Comparison with Monero Anchoring

| Property | Monero Anchor (p2pool) | Caribina (Arweave) |
|----------|----------------------|---------------------|
| Status | Implemented | Implemented |
| Requires p2pool | Yes | No |
| Requires Monero node | Yes (for full verification) | No |
| Requires funding | No (merge mining) | No (ArDrive free tier) |
| Settlement time | ~6 min (3 Monero blocks) | ~2 min (1 DarkWow block) |
| Consensus mechanism | RandomX PoW | Proof-of-Storage |
| Key management | Monero wallet | Per-block Ed25519 cycle |
| Protects native miners | No | Yes |
| Protects merge miners | Yes | Yes |
| Verification | monerod RPC or plausibility | Arweave gateway HTTP |

## Toy Model Results

The merge mining toy model (`contrib/docker/darkwow-testnet/merge_mining_model.py`)
includes Caribina as `ConsensusMode.CARIBINA`. Under a 51% attack with 10x attacker
hashpower:

| Mode | Blocks replaced | Blocks protected | Attacker fork |
|------|----------------|-----------------|---------------|
| **NATIVE** | 5/5 (100%) | 0 | Accepted |
| **ANCHOR** (Monero) | 1/5 (20%) | 4 | Accepted (partial) |
| **CARIBINA** (Arweave) | 0/5 (0%) | 5 | **Rejected** |

Caribina is the only mode where the attacker's fork is completely rejected — all
blocks are protected from the first confirmation onward.

## Source Files

| File | Purpose |
|------|---------|
| `src/linear/src/caribina/mod.rs` | Module root |
| `src/linear/src/caribina/data_item.rs` | ANS-104 DataItem binary format |
| `src/linear/src/caribina/wallet.rs` | Ed25519 key generation and signing |
| `src/linear/src/caribina/anchor.rs` | ArDrive Turbo HTTP POST |
| `src/linear/src/caribina/verify.rs` | Arweave gateway verification |
| `src/linear/src/block.rs` | `anchor_tx_id` field in BlockHeader |
| `src/linear/src/blockchain.rs` | Finality constraint in `insert_block()` |
| `src/linear/src/consensus.rs` | PoW consensus (unchanged — Caribina is a constraint overlay) |
| `bin/dwowd/src/rpc/miner.rs` | Mining integration |
| `bin/dwowd/src/rpc/stratum.rs` | Stratum integration |
| `bin/dwowd/src/proto/linear_broadcast.rs` | P2P verification |

## See Also

- [Monero Anchoring Finality](mining-tokenomics.md#anchoring-finality-gadget) — the p2pool-based finality layer
- [Merge Mining Toy Model](../../../contrib/docker/darkwow-testnet/merge_mining_model.py) — includes CARIBINA consensus mode
- [Toy Model README](../../../contrib/docker/darkwow-testnet/merge_mining_model_README.md)
- [ANS-104 Specification](https://github.com/ArweaveTeam/arweave-standards/blob/master/ans/ANS-104.md)
- [ArDrive Turbo](https://ardrive.io/turbo/)
