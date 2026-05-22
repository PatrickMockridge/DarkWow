# Monero Integration

DarkWow's Monero integration spans three key areas: **merge mining** for security, **bridging** for asset transfer, and **stable collateralization** for price stability. These components work together to enable XMR as a foundational primitive for DarkWow's privacy-preserving financial system.

## Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        DarkWow ↔ Monero                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│   Merge Mining          Bridging & Wrapping      Stable Collateral    │
│   ───────────          ─────────────────         ────────────────    │
│   • DarkWow security    • XMR → wXMR             • wXMR → collateral │
│   • RandomX PoW       • wXMR → XMR             • Mint stablecoins  │
│   • Aux chain merge   • Trustless deposits     • Pooled debt model │
│   • Shared security   • Relayer execution      • PI controller      │
│   • Monero anchor finality                                            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

| Component | Purpose | Location | Doc |
|-----------|---------|----------|-----|
| Merge Mining | Secure DarkWow via RandomX PoW | `bin/dwowd/src/rpc/mm_rpc.rs`, `bin/dwow-p2pool-adaptor/` | [Merge Mining](../testnet/merge-mining.md) |
| Bridge Contract | Cross-chain asset transfer | `src/contract/bridge/` | [Bridge Contract](../dev/contracts/bridge.md) |
| Stablecoin | Collateralized debt positions | `src/contract/stablecoin/` | [Stablecoin](../contract/stablecoin.md) |

## 1. Merge Mining

DarkWow uses Monero's CryptoNight-RandomX proof-of-work algorithm for merge mining. This allows Monero miners to also secure DarkWow without additional energy cost.

**Key Components:**

| Component | File | Purpose |
|-----------|------|---------|
| `mm_rpc` handler | `bin/dwowd/src/rpc/mm_rpc.rs` | p2pool merge mining JSON-RPC (aux block template + solution submission) |
| `MergeMiningRpcHandler` | `bin/dwowd/src/rpc/mm_rpc.rs` | RequestHandler impl for merge mining protocol |
| Native p2pool adaptor | `bin/dwow-p2pool-adaptor/src/rpc.rs` | monerod-compatible RPC translating p2pool → dwowd stratum |
| Header translate | `bin/dwow-p2pool-adaptor/src/translate.rs` | 227-byte DarkWow ↔ Monero block format translation |
| Linear `BlockHeader` | `src/linear/src/block.rs` | Anchor fields: `anchor_monero_height`, `anchor_monero_hash`, `finality_flags` |
| `FinalityConfig` | `src/linear/src/finality.rs` | Monero finality config (`monero_enabled`, `monero_min_confirmations`) |
| Stratum handler | `bin/dwowd/src/rpc/stratum.rs` | xmrig stratum login/submit with RandomX PoW verification |
| Monero anchor verify | `src/linear/src/monero/verify.rs` | `verify_monero_anchor()` — dual-mode verification with fallback |
| Monero RPC client | `src/linear/src/monero/rpc.rs` | monerod JSON-RPC client (`get_block`, `get_block_count`) |
| Monero module root | `src/linear/src/monero/mod.rs` | Re-exports `verify_monero_anchor`, RPC functions, error types |
| P2P Monero verify | `bin/dwowd/src/proto/linear_broadcast.rs` | Monero anchor verification during P2P block propagation |

**Architecture After DAG-to-Linear Migration (May 2026):**

The `src/blockchain/monero/` directory was removed during the DAG-to-linear migration (commit `abbcf967e`). Merge mining now uses the linear blockchain's `BlockHeader` anchor fields (`anchor_monero_height`, `anchor_monero_hash`, `finality_flags`) for Monero integration rather than a dedicated `MoneroPowData` type. The RandomX proof-of-work verification is handled directly by `PoWConsensus::verify_proof()`.

**How Merge Mining Works (Current):**

1. p2pool connects to dwowd's mm_rpc JSON-RPC handler (port 31348)
2. p2pool calls `merge_mining_get_aux_block` to get a DarkWow block template (227-byte mining blob with nonce=0)
3. p2pool injects the aux data into Monero blocks and miners find RandomX solutions
4. When a Monero block is found with DarkWow aux data, p2pool calls `merge_mining_submit_solution`
5. dwowd reconstructs the block with the found nonce, verifies RandomX PoW, and inserts the block into the linear chain

**Three Merge Mining Pathways:**

| Pathway | Description | Status |
|---------|-------------|--------|
| Native adaptor | `dwow-p2pool-adaptor` presents dwowd as monerod to p2pool | Working |
| mm_rpc merge mining | p2pool → dwowd mm_rpc direct (full merge pathway) | Working |
| Solo stratum | xmrig → dwowd stratum directly | Working |

**Reference:** [Merge Mining (User Guide)](../testnet/merge-mining.md)

### Protocol Architecture: Two Separate Protocols

Merge mining uses **two completely separate protocols** — p2pool communicates
with dwowd via HTTP JSON-RPC, not the DarkWow P2P network. This is by design.

```
MERGE MINING SIDE                           DARKFI P2P SIDE
══════════════════                         ══════════════════

p2pool                                     dwowd node0 (mm_rpc receiver)
  │                                           │
  │  HTTP JSON-RPC                            │
  ├─ merge_mining_get_chain_id ──────────────►│
  ├─ merge_mining_get_aux_block ─────────────►│
  │                                           │  constructs block template
  │◄────────────── aux_blob, aux_hash ────────┤
  │                                           │
  │  (injects aux data into Monero block)     │
  │  (xmrig mines Monero block)               │
  │                                           │
  ├─ merge_mining_submit_solution ───────────►│
  │                                           │  validates: check_aux_chains,
  │                                           │  RandomX hash, coinbase proof
  │                                           │  registry.submit()
  │                                           │
  │                                           ├─ validator.append_proposal()
  │                                           ├─ p2p.broadcast(ExtendedProposalMessage)
  │                                           │     │
  │                                           │     ▼
  │                                           │  ┌──────────────────────────┐
  │                                           │  │  DarkWow P2P Network     │
  │                                           │  │  (magic bytes, version    │
  │                                           │  │   handshake, hostlist,    │
  │                                           │  │   block/tx propagation)   │
  │                                           │  ├─ node1 ◄─────────────────┤
  │                                           │  ├─ node2 ◄─────────────────┤
  │                                           │  └─ ...   ◄─────────────────┘
  │                                           │
  │◄── result ────────────────────────────────┤
```

**Key insight**: p2pool connects to exactly one dwowd node via HTTP JSON-RPC.
That dwowd node handles all DarkWow-side P2P — validating the merge-mined
block, then broadcasting it to all P2P peers via [`p2p.broadcast()`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/p2p.rs).
Other dwowd nodes receive the block as a normal `ExtendedProposalMessage` and
validate it through the standard block validation path (stateless ZK
verification + PoW check on `PowData::Monero`).

Block propagation from merge-mined origin to the entire P2P mesh is automatic.
No special p2pool discovery mechanism is needed.

#### Why p2pool Cannot Join the P2P Network

DarkWow's P2P network enforces three strict barriers:

1. **Magic bytes** — every P2P message begins with a 4-byte network identifier.
   Mismatch causes immediate disconnect and peer ban. Implemented in
   [`src/net/channel.rs`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/channel.rs).

2. **Version handshake** — `app_name`, `app_version.major`, and
   `app_version.minor` must match exactly on both sides. A non-dwowd peer
   would be rejected during the `ProtocolVersion` handshake in
   [`src/net/protocol/protocol_version.rs`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/protocol/protocol_version.rs).

3. **Custom serialization** — all P2P messages use the `dwow_serial` binary
   format, not JSON or protobuf. A non-Rust implementation would need to
   reimplement the full DarkWow wire protocol.

Making p2pool speak the DarkWow P2P protocol would require forking p2pool and
implementing the DarkWow wire format. This is unnecessary — the RPC bridge
architecture handles everything correctly.

#### Call Chain: p2pool Submission → Linear Block Insertion

```
1. p2pool POST merge_mining_submit_solution → bin/dwowd/src/rpc/mm_rpc.rs
2. MergeMiningRpcHandler: deserialize 227-byte blob, extract nonce, validate height
3. Reconstruct BlockHeader with template data (ZK coinbase, merkle roots)
4. PoWConsensus::verify_proof() — RandomX hash vs target
5. Caribina anchor (best-effort Arweave anchoring)
6. LinearBlockchain::insert_validated_block(&block)
   ├─ verify_proof(header)  — RandomX PoW verification
   ├─ verify_transactions() — ZK proof verification
   └─ append to canonical chain
7. p2p.broadcast via protocol handlers (automatic via P2P mesh)
```

#### Fault Tolerance

p2pool submits to only one dwowd node. For production, operators can
configure p2pool to round-robin across multiple dwowd mm_rpc endpoints.
In the Docker testnet, auto-restart policies handle process failures.

| Scenario | Impact | Mitigation |
|----------|--------|-----------|
| dwowd mm_rpc node crashes | p2pool can't submit solutions | Restart policy; point p2pool at another node |
| p2pool crashes | Merge mining stops | Container restart policy |
| monerod crashes | p2pool can't get templates | Container restart policy |

### Monero Anchor Finality

Merge-mined DarkWow blocks carry a Monero anchor: the height and hash of the
Monero block that contained the DarkWow aux data. Once that Monero block
accumulates `monero_min_confirmations` confirmations, the DarkWow block is
**finalized** — it cannot be reorganized. This borrows Monero's cumulative
RandomX difficulty as a security backstop.

#### Dual-Finality Architecture

Monero anchoring coexists with Caribina (Arweave) anchoring. Both are
independent, additive finality layers:

```
BlockHeader
├── anchor_tx_id: [u8; 32]        ← Caribina (Arweave)
├── anchor_monero_height: u64     ← Monero (p2pool)
├── anchor_monero_hash: [u8; 32]  ← Monero (p2pool)
└── finality_flags: u8            ← CARIBNIA=0x01 | MONERO=0x02 | SIGNALED=0x04
```

| Property | Caribina (Arweave) | Monero (p2pool) |
|----------|-------------------|-----------------|
| Source | All miners (stratum + merge) | Merge miners only (p2pool) |
| Anchor data | Arweave TX ID | Monero block height + hash |
| Verification | Arweave gateway HTTP | monerod JSON-RPC or plausibility |
| Settlement | ~2 min | ~6 min (3 Monero blocks) |
| Cost | Free (ArDrive Turbo) | Free (merge mining side-effect) |

**Key design decisions:**
- **Both can coexist**: a single block can carry both anchors. `finality_flags`
  is set via `fc.mine_flags()` unconditionally — if both are enabled, both
  bits are set regardless of whether individual anchors succeed.
- **Verifiers check independently**: `should_verify_anchor()` and
  `should_verify_monero_anchor()` each check their respective flag bit AND
  actual anchor data. A block with `FINALITY_MONERO` set but
  `anchor_monero_height == 0` fails verification.
- **No race conditions**: Caribina anchoring is a HTTP POST during block
  assembly. Monero anchoring is data extraction from p2pool's JSON-RPC
  params. They operate on different fields of the same `BlockHeader` and
  are set in sequence (Monero first, then Caribina) before the unconditional
  `finality_flags` assignment.

#### Verification Modes

| Mode | Trigger | Behavior |
|------|---------|----------|
| **Lightweight** (default) | `monerod_url` not configured | Accepts any anchor with height > 0 and ≤ 5,000,000 |
| **Full RPC** | `monerod_url` configured | Queries monerod JSON-RPC: verifies hash matches actual Monero block at height, checks `tip >= height + min_confirmations - 1` |

#### Configuration

```toml
[network_config."darkwow-testnet".finality]
mode = "always"               # FinalityMode: native | always | signaled
monero_enabled = true         # Enable Monero anchoring
monero_min_confirmations = 3  # Monero confirmations before finality
monerod_url = "http://127.0.0.1:18081/json_rpc"  # Optional full verification
```

CLI flags:

| Flag | Purpose |
|------|---------|
| `--finality-enable-monero` | Enable Monero p2pool anchoring |
| `--monero-min-confirmations <N>` | Confirmations required before finality (default: 3) |
| `--monerod-rpc-url <URL>` | monerod JSON-RPC endpoint for full verification |

#### Anchor Population Flow

In `mm_submit_solution()` (`bin/dwowd/src/rpc/mm_rpc.rs`), three independent
steps run in sequence during merge mining block assembly:

1. **Monero anchor** (when `should_anchor_monero()`): extracts `height` (f64)
   and `hash` (hex string) from p2pool's JSON-RPC params. Populates
   `anchor_monero_height` and `anchor_monero_hash`.
2. **Caribina anchor** (when `should_anchor()`): POSTs block hash to Arweave
   via ArDrive Turbo. Populates `anchor_tx_id`.
3. **Unconditional flags**: `block.header.finality_flags = fc.mine_flags()`
   is always set, regardless of whether either anchor succeeded. Verifiers
   independently check both the flag bit AND the actual anchor data.

#### P2P Verification

When a block is received via P2P broadcast (`linear_broadcast.rs`), Monero
anchor verification runs alongside Caribina verification:

```rust
if msg.block.header.anchor_monero_height != 0
    && blockchain.finality_config.should_verify_monero_anchor(
        msg.block.header.finality_flags)
{
    dwow_linear::verify_monero_anchor(
        msg.block.header.anchor_monero_height,
        &msg.block.header.anchor_monero_hash,
        msg.block.header.timestamp,
        blockchain.finality_config.monerod_url.as_deref(),
        blockchain.finality_config.monero_min_confirmations,
    )?;
}
```

Failed verification causes the block to be skipped (not inserted). The
`anchor_monero_height != 0` guard prevents verification of blocks that
don't carry Monero anchors, even if the `FINALITY_MONERO` flag is set.

#### Block Conflict Protection

Both `src/linear/src/blockchain.rs` and `bin/dwowd/src/blockchain.rs` reject
insertion of blocks that would replace an existing block carrying either type
of anchor:

```rust
if self.finality_config.should_enforce(existing.header.finality_flags)
    && (existing.header.anchor_tx_id != [0u8; 32]
        || existing.header.anchor_monero_height != 0)
```

An anchored block at height N permanently wins against any later candidate.

## 2. Bridging & Wrapping

The bridge contract enables moving XMR between Monero and DarkWow using an **Object Capability Security** model instead of VSS-based approaches.

### Object Capability vs VSS

| Aspect | VSS-Based Bridge | DarkWow OCap Bridge |
|--------|------------------|---------------------|
| Key custody | Distributed shards | User-held secrets |
| Withdrawal speed | Slow (threshold round) | Fast (self-signed ZK) |
| Node compromise | Catastrophic | Impossible |
| Censorship | Threshold can block | Cannot block |
| Complexity | High (DKG) | Low (hashing) |

### XMR → DarkWow (Deposit)

```
User → One-Time Address on Monero
      ↓
Monero TX (10 confirmations)
      ↓
Relayer observes via view key (cannot spend)
      ↓
ZK Proof (DLEq + Merkle proof + confirmations)
      ↓
Bridge Contract: verify + mint wXMR
```

**Deposit Security:**
- **One-time addresses**: Each deposit uses a fresh address derived from user's DarkWow identity + nonce
- **DLEq proofs**: Prove ownership of the one-time address private key without revealing it
- **Merkle proofs**: Prove the transaction exists in a Monero block
- **Confirmations**: 10 blocks required before deposit is recognized

### DarkWow → XMR (Withdrawal)

```
User burns wXMR on DarkWow
      ↓
Pending withdrawal created (100 block timeout)
      ↓
Relayer picks up withdrawal
      ↓
Relayer broadcasts TX to Monero
      ↓
If timeout expires → User can cancel and reclaim
```

**Trust Model:**
- **Relayer**: Honest-but-curious (view key only), economically motivated to execute
- **Timeout**: 100 blocks — prevents relayer censorship
- **Slashing**: Relayer loses funds if they fail to execute

### Bridge Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize bridge state |
| DepositV1 | 0x01 | Register external chain deposit |
| WithdrawV1 | 0x02 | Claim withdrawal to external chain |
| UpdateConfigV1 | 0x03 | Update bridge operators |
| CancelWithdrawV1 | 0x04 | Cancel timed-out withdrawal |

### Relayer Service

The `bin/xmr_relayer/` binary handles the observation and execution layer:

```
bin/xmr_relayer/
├── src/
│   ├── main.rs           # CLI (start, derive-address, status)
│   ├── monero_rpc.rs     # Monero wallet/node RPC client
│   ├── proof.rs          # ZK proof construction
│   └── withdrawal.rs      # Withdrawal execution + timeout
└── xmr_relayer_config.toml
```

**Reference:** [Bridge Contract](../contract/bridge.md), [Bridge Contract (Dev)](../dev/contracts/bridge.md)

## 3. Stable Collateralization

wXMR can be used as collateral in DarkWow's [stablecoin contract](../contract/stablecoin.md) to mint privacy-preserving stablecoins.

### Pooled Debt Model

DarkWow uses a **Pooled Debt** (Synthetix-style) model rather than individual CDPs:

**Advantages for Privacy:**
- No individual position tracking — no position IDs that leak information
- Liquidation is "pool had shortfall" not "this person was liquidated"
- Simpler ZK circuits

### Full Flow: XMR → Stablecoin

```
1. XMR → wXMR (Bridge Deposit):
   User deposits XMR to bridge one-time address
   Relayer observes + verifies via DLEq proof
   DarkWow mints wXMR to user

2. wXMR → Collateral (Stablecoin DepositCollateral):
   User deposits wXMR into stablecoin CollateralPool
   CollateralPool tracks wXMR deposits by type
   User receives debt shares representing pool proportion

3. Collateral → Stablecoin (MintStable):
   User locks collateral + pays stability fee
   Receives stablecoin (e.g., USD-stable)
   Must maintain collateralization ratio (≥150%)

4. Stablecoin → Collateral (RepayStable + WithdrawCollateral):
   User repays stablecoin debt
   Withdraws wXMR collateral proportionally

5. wXMR → XMR (Bridge Withdraw):
   User burns wXMR on DarkWow bridge
   Relayer executes withdrawal on Monero within timeout
```

### Collateral Types

The stablecoin supports multiple collateral types:

```rust
pub enum CollateralType {
    Xmr,  // wXMR
    Drkw, // Native DRKW
}
```

### Price Feed

XMR/USD price is used for collateral valuation:

- **TWAP from AMM**: When an XMR/DRKW or XMR/USD pool exists, TWAP is used
- **Fallback**: ~$150/USD per XMR (until pool exists)
- **PI Controller**: Algorithmic redemption rate adjustment for stability

**Reference:** [Stablecoin Contract](../contract/stablecoin.md), [Stablecoin Contract (Dev)](../dev/contracts/stablecoin.md)

## Technical Reference

### Monero Cryptonote Differences from Ethereum

| Aspect | Ethereum | Monero |
|--------|----------|--------|
| Address type | Regular public keys | One-time addresses |
| Ownership proof | ECDSA signatures | DLEq proofs |
| Hash function | Keccak256 | cn_fast_hash (Keccak256) |
| Privacy | Transparent | Ring signatures |
| Smart contracts | Yes | No (relayer model) |

### Key Files

| File | Purpose |
|------|---------|
| `bin/dwowd/src/rpc/mm_rpc.rs` | Merge mining JSON-RPC handler (p2pool protocol) |
| `bin/dwow-p2pool-adaptor/src/translate.rs` | 227-byte header ↔ Monero block format translation |
| `bin/dwow-p2pool-adaptor/src/rpc.rs` | monerod-compatible RPC for p2pool |
| `src/linear/src/block.rs` | BlockHeader with Monero anchor fields |
| `src/linear/src/finality.rs` | FinalityConfig (Monero anchoring config + flag bits) |
| `src/linear/src/monero/mod.rs` | Monero module root (re-exports) |
| `src/linear/src/monero/verify.rs` | `verify_monero_anchor()` — dual-mode verification |
| `src/linear/src/monero/rpc.rs` | monerod JSON-RPC client (`get_block`, `get_block_count`) |
| `bin/dwowd/src/proto/linear_broadcast.rs` | P2P block broadcast with Monero anchor verification |
| `bin/dwowd/src/main.rs` | CLI flags: `--finality-enable-monero`, `--monerod-rpc-url` |
| `src/contract/bridge/src/model/mod.rs` | DepositParams, XmrDepositProof |
| `src/contract/bridge/src/entrypoint.rs` | Bridge contract implementation |
| `src/contract/stablecoin/src/model/mod.rs` | CollateralType, CollateralPool |
| `bin/xmr_relayer/src/withdrawal.rs` | Withdrawal handling + timeout |

### Constants

**Bridge:**
- `BRIDGE_CONTRACT_XMR_CONFIRMATIONS`: 10 blocks
- `BRIDGE_CONTRACT_WITHDRAWAL_TIMEOUT_BLOCKS`: 100 blocks
- `BRIDGE_CONTRACT_SLASH_AMOUNT`: 1,000,000 units

**Stablecoin:**
- `CDP_MIN_COLLATERALIZATION_RATIO`: 150% (15000 bp)
- `STABLECOIN_XMR_USD_PRICE_FALLBACK`: ~$150

**Finality:**
- `FINALITY_CARIBNIA`: 0x01 flag bit (Arweave anchor present)
- `FINALITY_MONERO`: 0x02 flag bit (Monero anchor present)
- `FINALITY_SIGNALED`: 0x04 flag bit (finality enforcement required)
- `MAX_PLAUSIBLE_MONERO_HEIGHT`: 5,000,000 blocks (lightweight plausibility cap)
- `monero_min_confirmations` default: 3 (~6 minutes on Monero)

## See Also

- [Merge Mining (User Guide)](../testnet/merge-mining.md)
- [Bridge Architecture](../contract/bridge.md)
- [Bridge Contract (Dev)](../dev/contracts/bridge.md)
- [Stablecoin](../contract/stablecoin.md)
- [Stablecoin Contract (Dev)](../dev/contracts/stablecoin.md)
- [Atomic Swap](../contract/atomic_swap.md)
