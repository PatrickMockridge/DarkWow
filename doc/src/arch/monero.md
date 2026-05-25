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
| Merge Mining | Secure DarkWow via RandomX PoW | `bin/dwowd/src/rpc/mm_rpc.rs` | [Merge Mining](../testnet/merge-mining.md) |
| Bridge Contract | Cross-chain asset transfer | `src/contract/bridge/` | [Bridge Contract](../dev/contracts/bridge.md) |
| Stablecoin | Collateralized debt positions | `src/contract/stablecoin/` | [Stablecoin](../contract/stablecoin.md) |


For DarkWow merge mining — architecture, protocol, three pathways, economics,
setup guide, and Monero-anchor finality — see the consolidated
[Merge Mining](merge-mining.md) chapter.

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
