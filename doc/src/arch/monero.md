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
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

| Component | Purpose | Location | Doc |
|-----------|---------|----------|-----|
| Merge Mining | Secure DarkWow via RandomX PoW | `src/blockchain/monero/` | [Merge Mining](../testnet/merge-mining.md) |
| Bridge Contract | Cross-chain asset transfer | `src/contract/bridge/` | [Bridge Contract](../dev/contracts/bridge.md) |
| Stablecoin | Collateralized debt positions | `src/contract/stablecoin/` | [Stablecoin](../contract/stablecoin.md) |

## 1. Merge Mining

DarkWow uses Monero's CryptoNight-RandomX proof-of-work algorithm for merge mining. This allows Monero miners to also secure DarkWow without additional energy cost.

**Key Components:**

| Component | File | Purpose |
|-----------|------|---------|
| `MoneroPowData` | `src/blockchain/monero/mod.rs` | Parses and serializes Monero PoW data |
| `cn_fast_hash` | `src/blockchain/monero/utils.rs` | Keccak256 hash function used by Monero |
| `tree_hash` | `src/blockchain/monero/utils.rs` | Merkle tree root computation |
| `merkle_proof` | `src/blockchain/monero/merkle_proof.rs` | Merkle proof verification |

**How Merge Mining Works:**

1. Monero miner constructs a block with DarkWow aux chain data in the coinbase
2. DarkWow reads the `MoneroPowData` from the block template
3. DarkWow computes the aux chain Merkle root and verifies the PoW
4. Both chains share security — attacking one requires attacking both

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

#### Call Chain: p2pool Submission → P2P Propagation

```
1. p2pool POST merge_mining_submit_solution → bin/darkfid/src/rpc/xmr.rs
2. MmRpcHandler: MoneroPowData::new, check_aux_chains, block.sign
3. registry.submit(&mut validator, &subscribers, &p2p, block)
4. validator.append_proposal → Consensus::append_proposal
   ├─ verify_proposal → verify_block → validate_block
   │  ├─ header.validate_powdata()  — coinbase + aux chain merkle proofs
   │  ├─ PoWModule.verify_block_hash — RandomX hash vs mine target
   │  ├─ verify_transactions — ZK proof verification
   │  └─ overlay.add_block(block)
   └─ fork.append_proposal(proposal)
5. p2p.broadcast(&ExtendedProposalMessage{proposal, zkbin_data})
6. Peer node receives ExtendedProposalMessage
   ├─ sync::verify_block (stateless ZK check with embedded zkbin_data)
   ├─ validator.append_proposal (same validation as step 4)
   └─ re-broadcast to all peers except sender
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
| `src/blockchain/monero/mod.rs` | MoneroPowData, block parsing |
| `src/blockchain/monero/utils.rs` | cn_fast_hash, tree_hash |
| `src/blockchain/monero/keccak.rs` | Keccak256 state serialization |
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

## See Also

- [Merge Mining (User Guide)](../testnet/merge-mining.md)
- [Bridge Architecture](../contract/bridge.md)
- [Bridge Contract (Dev)](../dev/contracts/bridge.md)
- [Stablecoin](../contract/stablecoin.md)
- [Stablecoin Contract (Dev)](../dev/contracts/stablecoin.md)
- [Atomic Swap](../contract/atomic_swap.md)
