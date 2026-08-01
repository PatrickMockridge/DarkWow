Anonymous Bridge
================

## Purse Composition

The Bridge composes with the genesis [Purse](purse.md) primitive. Total deposited
and total withdrawn are tracked in Purses rather than raw `u64` counters. Deposit
calls `Purse::DepositV1`. Withdraw calls `Purse::WithdrawV1`. The Purse contract
handles balance integrity — the bridge validates only that the child call targets
the correct genesis Purse ContractId.

> **Note:** This document describes the full bridge design and architecture. The bridge contract and multi-chain relayer service are implemented and tested (Level 1/2/3). DLEq proof verification is implemented behind the `bridge-verify` feature gate (see src/contract/bridge/README.md#feature-gates). See [Security Audit](audit.md) for the May 2026 hardening details.

*DarkWow's universal bridge enables private asset transfers between DarkWow and external blockchains using Object Capability Security.*

## Overview

The DarkWow bridge connects multiple external chains to DarkWow's privacy-preserving ecosystem:

| Chain | Token | Privacy Model | Sell Pitch |
|-------|-------|---------------|------------|
| **Ethereum** | ETH | Transparent | Native asset access |
| **Monero** | XMR | Ring signatures | "The private money" |
| **Zcash** | ZEC | Sapling shielded | "Shield your Zcash once and forever more" |
| **Aztec** | ETH/DAI | Private rollup | "Private DAI and ETH - Aztec's private DeFi made portable" |
| **Litecoin** | LTC | Transparent + MWEB | "The Monero trade pair - move in and out of privacy with LTC" |

## Object Capability Security Model

Unlike traditional VSS-based bridges that require threshold signing, DarkWow uses **Object Capability Security**:

```
VSS-Based Bridge:                          DarkWow OCap Bridge:
─────────────────                          ──────────────────
User deposits → VSS nodes hold shards      User knows secret → Derive bridge_address
User withdraws → n-of-m threshold           User withdraws → Self-signed ZK proof
                                        ↑
                                        Bridge nodes see NOTHING useful
```

**Key advantages:**
- **No VSS shards to steal**: Bridge nodes cannot reconstruct secrets
- **Fast withdrawals**: No threshold coordination, just ZK proof verification
- **Censorship resistant**: No threshold can block valid withdrawals
- **Fresh addresses**: Temporal privacy via nonce per deposit

## Supported Chains

### Ethereum

Native Ethereum support for ETH transfers via Merkle proof verification.

**Architecture:**
- Standard Merkle proof authenticates deposit inclusion in ETH block
- ZK circuit verifies: commitment = H(secret, amount, bridge_address)
- 12 block confirmations required

**When to use:**
- Simple ETH transfers where Aztec adds too much complexity
- Assets that don't have Aztec support

**Best Practice: ETH → Aztec → DarkWow**

For privacy-preserving ETH transfers, the recommended path is:

```
ETH → Aztec (private rollup) → DarkWow
```

This provides:
- **Full privacy** on Ethereum L1 via Aztec's private rollup
- **Transaction amounts hidden**
- **Counterparties hidden**
- **Full DeFi history private**

Not all assets support Aztec bridging. For those that don't (like Lido staked ETH, rocket pool ETH, etc.), direct Ethereum bridging remains available.

See [Aztec section](#aztec-private-rollup) for the preferred privacy path.

### Monero

Private money with ring signatures. The natural pairing with DarkWow's privacy model.

**Architecture:**
- Uses DLEq proofs for one-time address ownership
- Relayer observes deposits via view key (cannot spend)
- 10 block confirmations required

**The Sell: "The Private Money"**
- XMR is already private by default
- Bridge to DarkWow maintains privacy
- All subsequent DarkWow transactions are private
- When you unwrap, XMR returns to your Monero address

**Flow:**
```
User → One-time address on Monero (derived from DarkWow identity)
     ↓
Monero TX (10 confirmations, ring signatures hide sender)
     ↓
Relayer observes via view key, constructs DLEq proof
     ↓
ZK proof submitted to DarkWow
     ↓
DarkWow mints wXMR to user
```

**Constants:**
- Minimum deposit: 0.001 XMR
- Confirmations: 10 blocks

### Zcash

Sapling shielded transactions with Groth16 zk-SNARKs. Maximum privacy for Zcash holders.

**Architecture:**
- Full Sapling integration with spend/output proofs
- Nullifiers prevent double-spending
- Merkle proof verifies note commitment in Sapling tree
- 10 block confirmations required

**The Sell: "Shield Your Zcash Once and Forever More"**

Unlike other bridges that require you to re-shield on every transaction, DarkWow's bridge means you **only shield once**:

```
Other bridges:                              DarkWow bridge:
──────────────                             ─────────────
ZEC → Shield on deposit                     ZEC → Shield once
     → Unshield to use                          ↓
     → Re-shield for privacy              DarkWow DeFi (fully private)
     → Unshield to send                       ↓
     → Re-shield... (infinite loop)      wZEC burns → ZEC returns
                                           → Same Zcash address
                                           → Full privacy preserved
```

**Why this matters:**
- Re-shielding exposes your Zcash transaction history
- Each re-shield creates a linkable chain
- DarkWow keeps you in the privacy ecosystem permanently
- Your Zcash shielded history stays private **forever**

**Flow:**
```
User → Sapling shielded address (zaddr)
     ↓
Zcash TX (10 confirmations, fully private)
     ↓
Relayer observes via lightwalletd, constructs Sapling proof
     ↓
ZK proof submitted to DarkWow (anchor + merkle proof verified)
     ↓
DarkWow mints wZEC to user
```

**Constants:**
- Minimum deposit: 0.0001 ZEC (10,000 zatoshi)
- Confirmations: 10 blocks

### Aztec (Private Rollup)

**Preferred for ETH and DAI** - provides full privacy on Ethereum L1 via ZK rollup.

Ethereum ZK rollup for private ETH and ERC-20 (DAI) transfers. Combines Ethereum security with privacy.

**Architecture:**
- Notes encrypted on Ethereum L1
- Nullifiers prevent double-spending
- Data availability on Ethereum
- 5 Ethereum block confirmations after rollup

**The Sell: "Private DAI and ETH - Aztec's Private DeFi Made Portable"**

Aztec keeps your DeFi history private:
- Transaction amounts hidden
- Counterparties hidden
- Full history private on Ethereum L1

```
Your DAI on Aztec:
┌─────────────────────────────────────────────────┐
│  Deposit DAI → Aztec private rollup              │
│       ↓                                          │
│  Private DeFi on DarkWow (wDAI)                  │
│       ↓                                          │
│  Withdraw DAI → Aztec (same address)             │
│       ↓                                          │
│  Your full DeFi history is private!             │
└─────────────────────────────────────────────────┘
```

**Supported Assets:**
- ETH (asset_id = 0)
- DAI (asset_id = 1)

**Flow:**
```
User → Aztec bridge contract on Ethereum
     ↓
Aztec rollup processes (private TX)
     ↓
Relayer observes rollup, fetches note data
     ↓
ZK proof submitted to DarkWow
     ↓
DarkWow mints wETH/wDAI to user
```

**Constants:**
- Minimum deposit: 0.001 ETH or equivalent
- Confirmations: 5 blocks after rollup

### Litecoin

Bitcoin's silver - similar to Bitcoin but with faster blocks and MWEB privacy.

**Architecture:**
- Standard UTXO model (like Bitcoin)
- MimbleWimble Extension Blocks (MWEB) for confidential transactions
- Faster block time: 2.5 minutes vs Bitcoin's 10 minutes
- 6 block confirmations (~15 minutes total)

**The Sell: "The Monero Trade Pair - Move In and Out of Privacy with LTC"**

Litecoin is the natural segueway to Monero:
- **LTC/XMR** is a popular trade pair on exchanges
- Lower fees than Bitcoin for privacy transitions
- MWEB adds confidentiality when needed
- Faster settlement than Bitcoin
- Already used as a stepping stone to/from Monero

```
Monero traders already use LTC as a bridge:
XMR → Sell for LTC (lower fees than BTC)
LTC → Bridge to DarkWow (privacy)
     ↓
DarkWow DeFi (fully private)
     ↓
Bridge to LTC → Buy XMR
```

**MWEB Support:**
Litecoin's MimbleWimble Extension Blocks provide:
- Confidential transaction amounts
- Pedersen commitments instead of transparent UTXOs
- Range proofs for amount validity
- Privacy similar to Monero's ring signatures

**Flow:**
```
User → Bridge address on Litecoin (transparent or MWEB)
     ↓
Litecoin TX confirmed (6 blocks)
     ↓
Relayer observes via Litecoin RPC
     ↓
ZK proof submitted to DarkWow (merkle proof + optional MWEB verification)
     ↓
DarkWow mints wLTC to user
```

**Constants:**
- Minimum deposit: 0.001 LTC
- Confirmations: 6 blocks

## Universal Bridge Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        DarkWow Bridge                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐             │
│  │ Ethereum │  │  Monero  │  │  Zcash   │  │  Aztec   │  ┌────────┐│
│  │   ETH    │  │   XMR    │  │   ZEC    │  │ ETH/DAI  │  │Litecoin││
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘  └───┬────┘│
│       │             │             │             │             │      │
│       └─────────────┴─────────────┴─────────────┴─────────────┘      │
│                                 │                                    │
│                    ┌────────────┴────────────┐                      │
│                    │    Object Capability     │                      │
│                    │   Security Model        │                      │
│                    │                         │                      │
│                    │  User knows secret       │                      │
│                    │  Bridge = H(secret)     │                      │
│                    │  Withdraw = ZK proof   │                      │
│                    └────────────┬────────────┘                      │
│                                 │                                    │
│                                 ▼                                    │
│                    ┌────────────────────┐                           │
│                    │  DarkWow Contract  │                           │
│                    │  • Deposit verify │                           │
│                    │  • Mint wAsset    │                           │
│                    │  • Withdraw burn  │                           │
│                    │  • Nullifiers     │                           │
│                    └────────────────────┘                           │
│                                 │                                    │
│                                 ▼                                    │
│                    ┌────────────────────┐                           │
│                    │   Relayer Network  │                           │
│                    │  Execute withdraw │                           │
│                    │  Timeout/slash    │                           │
│                    └────────────────────┘                           │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize bridge state |
| DepositV1 | 0x01 | Register external chain deposit |
| WithdrawV1 | 0x02 | Request withdrawal to external chain |
| UpdateConfigV1 | 0x03 | Update bridge configuration |
| CancelWithdrawV1 | 0x04 | Cancel timed-out withdrawal |
| ExecuteGuaranteedWithdrawV1 | 0x05 | Execute guaranteed withdrawal with pool stake |
| CreateHtlcV1 | 0x06 | Create HTLC swap for cross-chain atomic swap |
| ClaimHtlcV1 | 0x07 | Claim HTLC with ZK proof of preimage (no plaintext secret) |
| RefundHtlcV1 | 0x08 | Refund HTLC swap after timelock expiry |
| ReassignWithdrawalV1 | 0x09 | Reassign stuck withdrawal to a new relayer |
| RegisterRelayerV1 | 0x0a | Register a relayer pubkey with the bridge |
| AcceptWithdrawalV1 | 0x0b | Accept a pending withdrawal as a relayer |
| VerifyRelayerReputationV1 | 0x0c | Query relayer reputation on-chain (read-only) |
| RegisterFeeScheduleV1 | 0x0d | Register a fee schedule commitment |
| GovernanceReportV1 | 0x0e | Per-chain accounting report — proves no unbacked minting (BaseDiv, cold) |

## Withdrawal Timeout & Slashing

All withdrawals use a timeout mechanism with two execution modes:

**Standard Mode (feed_mode = 0)**: User pays a fee; relayer executes. No refund on failure.

**Guaranteed Mode (feed_mode = 1)**: User pays fee + premium; if relayer fails to execute by timeout, user claims refund + slashing compensation.

```
User submits withdrawal (with optional max_fee_bp cap)
     ↓
Relayer picks up pending withdrawal (checks GUARANTEED_PENDING ≤ MAX_GUARANTEED_TOTAL)
     ↓
Relayer executes on external chain
     ↓
If timeout (100 blocks) expires without execution:
     - User can cancel and reclaim funds
     - Relayer gets slashed: max(MIN_SLASH, amount × SLASH_BP / BP_PRECISION)
     - Another relayer can reassign via ReassignWithdrawalV1 (0x09)
```

**Proportional Slashing**: Slash scales with withdrawal amount. `MIN_SLASH = 1_000_000` (floor), `SLASH_BP = 1000` (10%), `BP_PRECISION = 10000`.

**Fee Caps**: Bridge enforces `MAX_FEE_BP = 1000` (10%). Users can set tighter `max_fee_bp` per withdrawal.

**Circuit Breaker**: Guaranteed withdrawals are capped by `MAX_GUARANTEED_TOTAL`. A `GUARANTEED_PENDING` counter prevents over-acceptance.

**Trust Model (Updated):**
- Relayer is honest-but-curious (can observe but not steal)
- Economic incentives: relayer earns fee, gets proportionally slashed if fails
- User can cancel, reassign, or claim refund depending on feed_mode
- Fee caps prevent monopoly pricing abuse

## Balance Sheet & Governance Report

The bridge maintains per-deployment balance sheet counters in the config DB:

| Key | Purpose |
|-----|---------|
| `total_deposited` | Total wrapped tokens minted (incremented on DepositV1) |
| `total_withdrawn` | Total wrapped tokens burned (incremented on WithdrawV1) |
| `outstanding` | `total_deposited - total_withdrawn` (computed on GovernanceReportV1) |
| `governance_reports` | Historical governance reports for public audit |

### GovernanceReportV1 (0x0e) — Internal Accounting Proof

Unlike the stablecoin, the bridge **cannot** prove on-chain that external chain
deposits exist — the collateral (BTC, XMR, ZEC, etc.) lives on external chains.
The governance report proves **internal accounting consistency**: the bridge is
not minting unbacked wrapped tokens out of thin air.

1. **On-chain verification**: Reads `total_deposited` and `total_withdrawn` from
   the config DB. Rejects the report if the reporter's params don't match.
2. **Outstanding computation**: `outstanding = total_deposited - total_withdrawn`
3. **Accounting consistency**: Enforces `total_deposited >= total_withdrawn`
   (no negative outstanding — would indicate tokens created from nothing).
4. **Persistence**: The verified report is stored in the `governance_reports`
   tree keyed by `poseidon_hash(total_deposited, total_withdrawn, outstanding, block)`,
   providing an on-chain audit trail.

**Relationship to stablecoin**: Both contracts implement governance reports,
but with different security guarantees. The stablecoin verifies
`total_collateral >= outstanding` where collateral is locked on the same chain.
The bridge verifies `total_deposited >= total_withdrawn` where "collateral" is
external. Full reserve proof for the bridge requires external chain verification
by auditors or light clients.

## ZK Circuits

| Circuit | Purpose | Chain | Notes |
|---------|---------|-------|-------|
| deposit_v1.zk | Prove ETH deposit | Ethereum | Uses `merkle_root` opcode with `MerklePath` type |
| withdraw_v1.zk | Prove withdrawal authorization | All | Uses `sparse_merkle_root` with `SparseMerklePath` type, `token_minimum` public input |
| xmr_deposit_v1.zk | Prove Monero deposit (DLEq) | Monero | |
| zec_deposit_v1.zk | Prove Zcash Sapling deposit | Zcash | |
| azt_deposit_v1.zk | Prove Aztec rollup deposit | Aztec | |
| ltc_deposit_v1.zk | Prove Litecoin deposit (MWEB optional) | Litecoin | |

## Data Structures

### DepositParams

```rust
struct DepositParams {
    commitment: IntentCommitment,      // H(secret, amount, bridge_address)
    recipient: PublicKey,               // For address derivation
    bridge_nonce: u64,                  // Fresh address per deposit
    chain: ExternalChain,               // Target chain
    external_block_hash: [u8; 32],
    merkle_proof: Vec<[u8; 32]>,       // Chain-specific merkle proof
    external_state_root: [u8; 32],
    fee: u64,
    proof: Vec<u8>,                     // ZK proof
    xmr_proof: Option<XmrDepositProof>,    // Monero-specific
    zec_proof: Option<ZcashDepositProof>,   // Zcash-specific
    azt_proof: Option<AztecDepositProof>,  // Aztec-specific
    ltc_proof: Option<LitecoinDepositProof>, // Litecoin-specific
}
```

### ExternalChain Enum

```rust
enum ExternalChain {
    Ethereum,
    Monero,
    Zcash,
    Aztec,
    Litecoin,
}
```

## Promissory Note Lifecycle Integration

The bridge is a **token mover** in the Promissory Note ecosystem — it moves wrapped
tokens between users but does not mint, burn, or redeem them.

### Why the Bridge Uses TransferV1

All bridge PN child calls use **TransferV1 (0x04)** exclusively:

| Operation | PN Child Call | What Actually Happens |
|-----------|--------------|----------------------|
| Deposit | TransferV1 | Wrapped tokens transferred from bridge pool to user |
| Withdraw | TransferV1 | Wrapped tokens transferred back to bridge pool |
| Cancel | TransferV1 | Refund via transfer from bridge pool |
| ExecuteGuaranteed | TransferV1 | Guaranteed withdrawal transfer |

This is architecturally correct: the bridge's source of truth is the **external chain**
(Ethereum, Monero, Zcash, etc.), not the PN contract. Tokens are pre-minted to the bridge
pool at initialization and moved via transfers. The actual creation/destruction happens
on the external chain — deposits lock assets there, withdrawals release them there.

### Withdrawal vs Redemption

Bridge **withdrawals** and PN **redemption** are distinct operations:

| Property | Bridge Withdrawal | PN Redemption (RedeemV1) |
|----------|------------------|--------------------------|
| What happens to token | Transferred to bridge pool | Burned via ZK proof |
| Receipt? | None (external chain tx is proof) | Zero-value receipt coin |
| Source of truth | External chain | PN contract |
| Requires relayer? | Yes (external tx broadcast) | No |
| Used by | Bridge only | Any issuer contract |

The bridge does not need RedeemV1 because its lifecyle is external-chain-native.
PN RedeemV1 is for issuer contracts (like stablecoin) that manage tokens entirely
within DarkWow.

### Cross-Contract Validation

Uses `validate_child_contract_id` (gated on non-zero config — allows test deployments
before PN is deployed) and `validate_child_value_commit` on withdrawal operations.

## Integration: Stablecoin & DEX

### Stablecoin Collateral

All bridged assets can serve as collateral for DarkWow's stablecoin:

```
Bridge Asset → Collateral → Mint Stablecoin (NETHER)
     │
     ├─ wXMR collateral (privacy-native)
     ├─ wETH collateral (liquid, widely held)
     ├─ wZEC collateral (shielded)
     ├─ wDAI collateral (via Aztec, private stablecoin!)
     └─ wLTC collateral (trade pair liquidity)
```

### Bridged DAI as Price Anchor

**Bridged DAI (via Aztec)** is particularly valuable:

```
External DAI/USD ≈ $1.00
         ↓
Aztec private pool → DarkWow
         ↓
NETHER redemption rate adjusted by PI Controller
         ↓
Keeps NETHER/USD stable
```

The DAI peg creates natural price signals:
- **Into DarkWow**: External DAI price informs redemption rate
- **Out of DarkWow**: NETHER can be redeemed for DAI

### DEX Multi-Chain Support

The DEX **theoretically supports all bridged tokens**:

```
DEX on DarkWow:
┌─────────────────────────────────────────────────────────────┐
│  XMR/ETH swaps    │  ZEC/LTC swaps    │  DAI/ETH swaps     │
│  (via atomic      │  (shielded to     │  (via Aztec       │
│   swap)            │   transparent)    │   private)         │
└─────────────────────────────────────────────────────────────┘
         ↓                    ↓                    ↓
    wXMR pool             wZEC pool            wDAI pool
         ↓                    ↓                    ↓
    All tradable in DarkWow's privacy layer
```

## Security Model

### Deposit Security

Deposit verification operates at two levels: (1) the DepositV2 ZK circuit verifies the
internal commitment derivation (`poseidon_hash(secret, amount, bridge_address)`), and
(2) chain-specific verification functions check the external chain proof. The current
implementation status:

| Chain | Current Verification | Target (Mainnet) | Trust Model |
|-------|---------------------|-------------------|-------------|
| Ethereum | Host-delegated (no in-contract check) | Merkle-Patricia proof via light client | Trustless (once implemented) |
| Monero | Structural checks only (DLEq stubbed) | DLEq proof (FIXME(dleq)) | Honest-but-curious relayer |
| Zcash | Structural checks only (non-empty proofs) | Groth16 spend/output proof verification | Trustless (once implemented) |
| Aztec | Structural checks only (non-empty proofs) | PLONK note proof verification | Aztec rollup (ZK-verified) |
| Litecoin | Structural checks only (non-empty proofs) | Merkle proof + Bulletproof range proof | Trustless (once implemented) |

**Current limitations (see `entrypoint.rs` for FIXME/TODO markers):**
- Ethereum deposit verification is delegated to the host validator runtime — if the host
  verifier is disabled, deposits are accepted without cryptographic proof.
- Monero DLEq proof (`FIXME(dleq)`): any caller can claim any Monero deposit. Requires
  Montgomery curve operations in the zkVM before it can be implemented in-circuit.
- Zcash/Aztec/Litecoin: proof verification checks only that proof bytes are non-empty.
  Actual Groth16/PLONK verification requires pairing operations in the zkVM.
- All chains benefit from the internal DepositV2 circuit which verifies the DarkWow-side
  commitment derivation, the double-deposit check, and the chain-event uniqueness index
  (`blake3(chain_id || external_block_hash)`).

### HAZOP Hardening Features

The bridge underwent HAZOP remediation (2026-07-31) adding defense-in-depth:

1. **In-contract withdrawal deposit check (H-12):** The withdrawal path now verifies
   the deposit commitment exists in the bridge's deposits tree before allowing the
   withdrawal. Previously this was trusted to the host ZK verifier — a bypassed host
   verifier could allow withdrawals without proving deposit ownership.

2. **Nullifier recovery on cancellation (HAZOP-05):** When a withdrawal is cancelled
   after timeout, the nullifier is deleted from the nullifiers tree, restoring the
   deposit for re-use. Previously the nullifier was permanently spent, locking the
   deposit forever even though the withdrawal never completed.

3. **Chain-event uniqueness (HAZOP-13):** Each external chain deposit event is indexed
   by `blake3(chain_id || external_block_hash)` in a dedicated tree. The same external
   event cannot be deposited multiple times with different DarkFi commitments (varying
   `recipient_pub`, `nonce`, `secret`).

4. **Governance config sanity bounds (M-8):** Governance-configurable values have
   hardcoded maximums: `min_confirmations ≤ 10,000` (~7 days), fees ≤ 1,000,000,000,000
   (1M DRKW). Prevents governance from DoS-attacking deposits by setting confirmations
   to `u32::MAX` or fees to `u64::MAX`.

5. **V2 circuit migration (RC3):** All 12 operations now have domain-separated V2 ZK
   circuits. `get_metadata` routes to V2 namespaces. The CI gate
   (`scripts/check-circuit-domain-separation.sh`) prevents new undifferentiated hashes.

### Object Capability Properties

1. **Bridge nodes cannot steal**: They never hold user secrets
2. **Self-signed withdrawals**: User proves secret knowledge to authorize (secret revealed to relayer for external chain execution)
3. **Fresh addresses**: Nonce ensures temporal privacy
4. **Double-spend prevention**: Nullifiers tracked on DarkWow

### Relayer Security

- Relayers observe deposits but **cannot steal** (no spending authority)
- User proves secret knowledge to authorize withdrawal
- Secret revealed to relayer (required for external chain execution — inherent to HTLC)
- Timeout mechanism prevents indefinite withholding
- Slashing discourages relayer misbehavior

## Relayer Services

Each chain has a dedicated relayer service:

| Service | Binary | Status |
|---------|--------|--------|
| XMR Relayer | bin/xmr_relayer/ | Relayer binary with source tree |
| ZEC Relayer | bin/zcash_relayer/ | Relayer binary with source tree |
| AZT Relayer | bin/aztec_relayer/ | Relayer binary with source tree |
| LTC Relayer | bin/litecoin_relayer/ | Relayer binary with source tree |

All relayers follow the same pattern:
1. Observe external chain for deposits
2. Construct ZK proof data
3. Submit to DarkWow bridge contract
4. Execute withdrawals on external chain
5. Handle timeouts and slashing

## File Structure

```
src/contract/bridge/
├── proof/
│   ├── deposit_v1.zk          # Ethereum deposit
│   ├── withdraw_v1.zk          # Withdrawal
│   ├── xmr_deposit_v1.zk       # Monero deposit
│   ├── zec_deposit_v1.zk       # Zcash deposit
│   ├── azt_deposit_v1.zk        # Aztec deposit
│   └── ltc_deposit_v1.zk        # Litecoin deposit
├── src/
│   ├── model/mod.rs             # Data structures
│   ├── entrypoint.rs           # Contract logic
│   ├── lib.rs                  # Constants
│   └── error.rs                # Errors
└── tests/

bin/xmr_relayer/        # Monero relayer
bin/zcash_relayer/      # Zcash relayer
bin/aztec_relayer/      # Aztec relayer
bin/litecoin_relayer/   # Litecoin relayer
```

## Opcode Requirements: What's Needed vs What's Not

### Bridge Opcodes

The bridge circuits use proven, production-ready opcodes:

| Opcode | Used In Bridge | Purpose |
|--------|---------------|---------|
| `poseidon_hash` | ✅ All deposits | Commitment derivation |
| `merkle_root` | ✅ All deposits | Block inclusion proofs |
| `ec_mul_base` | ✅ All deposits | Public key derivation |
| `ec_get_x` / `ec_get_y` | ✅ All deposits | Point coordinate extraction |
| `constrain_equal_base` | ✅ All deposits | Equality constraints |
| `range_check` | ✅ All deposits | Amount validation |
| `base_div` | ✅ GovernanceReportV1 | `outstanding = deposited - withdrawn` verification |

**The bridge works because deposits/withdrawals are atomic:**
- Either the ZK proof verifies and tokens are minted, or it fails and nothing happens
- Deposit/withdrawal circuits need only hash constraints + merkle proofs + range checks
- `GovernanceReportV1` uses `base_div` for cold/precise accounting verification

### Where Advanced Opcodes ARE Needed

The **DEX Level 1+** (order book matching) and **advanced features** need these:

| Opcode | Status | Needed For |
|--------|--------|-----------|
| `base_div` | ✅ Implemented (0x58) | Price ratio calculations (`alice_price = request / offer`) |
| `LessThanOrEqual` | ✅ Verified Sound (0x55) | Boolean returns for conditional logic |
| `schnorr_verify` | Not implemented | In-circuit signature verification |

### The Current Status

```
Current DEX (Level 0 - Atomic Swaps):
├── Works NOW with existing opcodes
├── No price ratio calculations
├── No Boolean returns needed
└── Just atomic swap verification

Future DEX (Level 1 - Order Matching):
├── ✅ base_div implemented
├── ✅ LessThanOrEqual verified sound
├── NEEDS schnorr_verify for in-circuit signatures
└── NOT blocked by comparison opcodes
```

### Bridge = Not Blocked by Opcodes ✅

The **bridge is not held up by missing opcodes**. Deposit/withdrawal circuits
use atomic swap semantics:
- Deposit verification = merkle proof + hash constraints
- Withdrawal authorization = proof of secret knowledge
- `GovernanceReportV1` uses `base_div` (already implemented) for accounting proofs

**This is why the bridge is complete while the DEX is still developing.**

**On Secret Revelation in Withdrawals**:

Like atomic swaps, bridge withdrawals require the user to reveal their secret so the relayer can execute the transaction on the external chain. This is NOT a privacy regression — it's inherent to cross-chain HTLC design:

```
User submits withdrawal (proves knowledge of secret)
     ↓
Relayer sees secret, executes on external chain
     ↓
Funds released to user on external chain
```

**Why this is secure**:
- Fresh address per deposit → no cross-deposit linkability
- Nullifiers prevent double-spend
- Relayer cannot steal (no signing authority, only observation)
- Secret revelation required by HTLC semantics (counterparty needs secret)

**No bloom filter problem**:
- Each deposit/withdrawal uses a fresh secret
- No key reuse across transactions
- Observer cannot correlate deposits

See [DEX documentation](dex.md) for more on opcode limitations affecting the order book.

## Node Requirements for Cross-Chain Operations

### User Node Types

The bridge is designed to work with **light clients**, not full nodes:

| Operation | Required Node | Why |
|-----------|--------------|-----|
| **Deposit** | Light client or indexer | Only need Merkle proof of deposit inclusion |
| **Withdraw** | DarkWow full node | ZK proof verification happens on DarkWow contract |
| **Monitor deposits** | Light wallet (view key) or indexer | Monero/Zcash: view-key light clients work |
| **Execute withdrawals** | Relayer service | External chain tx broadcast (full nodes) |

### Full Node vs Light Client

**Full nodes** (DarkWow validator, Ethereum geth) are needed for:
- Validating ZK proofs on DarkWow side
- Broadcasting withdrawal transactions to external chains (relayers)
- Tracking nullifier state to prevent double-spends

**Light clients** (or indexers) are sufficient for:
- Detecting deposits on external chains
- Generating Merkle proofs of deposit inclusion
- Verifying block confirmations (SPV-style)

### Practical Architecture

```
Deposit Flow (User Side):
┌──────────────────────────────────────────────────────────────┐
│ User needs:                                                   │
│   - Light client or indexer access to external chain        │
│   - NOT a full node                                          │
│                                                              │
│ Examples:                                                    │
│   - Ethereum: Infura/Alchemy RPC (light) or block explorer  │
│   - Monero: View key + remote node (no full sync needed)     │
│   - Zcash: lightwalletd or block explorer API                │
└──────────────────────────────────────────────────────────────┘

Withdraw Flow (User Side):
┌──────────────────────────────────────────────────────────────┐
│ User needs:                                                   │
│   - DarkWow full node access (to submit proofs)               │
│   - NOT required to run own node (can use RPC)               │
└──────────────────────────────────────────────────────────────┘

Relayer (Separate Service):
┌──────────────────────────────────────────────────────────────┐
│ Relayer needs:                                               │
│   - Full node on external chain (to broadcast withdrawals)   │
│   - DarkWow full node access (to observe withdrawal events)   │
└──────────────────────────────────────────────────────────────┘
```

### External Chain Requirements

| Chain | User Needs for Deposit | Relayer Needs for Withdraw |
|-------|----------------------|---------------------------|
| Ethereum | RPC for Merkle proof (Infura/Alchemy) | Full geth/nethermind node |
| Monero | View key + remote node | Full monerod node |
| Zcash | lightwalletd or block explorer | Full zcashd node |
| Aztec | Rollup data availability | Aztec sequencer API |
| Litecoin | RPC + optional MWEB | Full litecoind node |

**Bottom line**: Users do **NOT** need to run full nodes for bridge deposits. They need:
1. Light client or RPC access to external chain (for Merkle proofs)
2. Access to DarkWow full node (for submitting proofs)

Relayers run the actual full nodes on external chains to execute withdrawals.

## References

- [Bridge Contract Dev Docs](../dev/contracts/bridge.md)
- [Relayer Documentation](../relayer/relayer.md)
- [Slashing & Economic Security](../arch/slashing.md)
- [Monero Integration](../arch/monero.md)
- [Stablecoin](./stablecoin.md)
- [DEX](./dex.md)
- Object Capability Model: <https://en.wikipedia.org/wiki/Object-capability_model>

## See Also

- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis
