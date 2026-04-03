Anonymous Bridge
================

*DarkFi's universal bridge enables private asset transfers between DarkFi and external blockchains using Object Capability Security.*

## Overview

The DarkFi bridge connects multiple external chains to DarkFi's privacy-preserving ecosystem:

| Chain | Token | Privacy Model | Sell Pitch |
|-------|-------|---------------|------------|
| **Ethereum** | ETH | Transparent | Native asset access |
| **Monero** | XMR | Ring signatures | "The private money" |
| **Zcash** | ZEC | Sapling shielded | "Shield your Zcash once and forever more" |
| **Aztec** | ETH/DAI | Private rollup | "Private DAI and ETH - Aztec's private DeFi made portable" |
| **Litecoin** | LTC | Transparent + MWEB | "The Monero trade pair - move in and out of privacy with LTC" |

## Object Capability Security Model

Unlike traditional VSS-based bridges that require threshold signing, DarkFi uses **Object Capability Security**:

```
VSS-Based Bridge:                          DarkFi OCap Bridge:
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

The foundational chain - ETH is the native gas token and DeFi building block.

**Architecture:**
- Standard Merkle proof verification for ETH deposits
- Deposit contract on Ethereum holds ETH backing
- 12 block confirmations required

**Flow:**
```
User → Bridge deposit contract on Ethereum (irreversible)
     ↓
Ethereum confirms deposit (12 blocks)
     ↓
ZK proof submitted to DarkFi (commitment verified)
     ↓
DarkFi mints wETH to user
```

### Monero

Private money with ring signatures. The natural pairing with DarkFi's privacy model.

**Architecture:**
- Uses DLEq proofs for one-time address ownership
- Relayer observes deposits via view key (cannot spend)
- 10 block confirmations required

**The Sell: "The Private Money"**
- XMR is already private by default
- Bridge to DarkFi maintains privacy
- All subsequent DarkFi transactions are private
- When you unwrap, XMR returns to your Monero address

**Flow:**
```
User → One-time address on Monero (derived from DarkFi identity)
     ↓
Monero TX (10 confirmations, ring signatures hide sender)
     ↓
Relayer observes via view key, constructs DLEq proof
     ↓
ZK proof submitted to DarkFi
     ↓
DarkFi mints wXMR to user
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

Unlike other bridges that require you to re-shield on every transaction, DarkFi's bridge means you **only shield once**:

```
Other bridges:                              DarkFi bridge:
──────────────                             ─────────────
ZEC → Shield on deposit                     ZEC → Shield once
     → Unshield to use                          ↓
     → Re-shield for privacy              DarkFi DeFi (fully private)
     → Unshield to send                       ↓
     → Re-shield... (infinite loop)      wZEC burns → ZEC returns
                                           → Same Zcash address
                                           → Full privacy preserved
```

**Why this matters:**
- Re-shielding exposes your Zcash transaction history
- Each re-shield creates a linkable chain
- DarkFi keeps you in the privacy ecosystem permanently
- Your Zcash shielded history stays private **forever**

**Flow:**
```
User → Sapling shielded address (zaddr)
     ↓
Zcash TX (10 confirmations, fully private)
     ↓
Relayer observes via lightwalletd, constructs Sapling proof
     ↓
ZK proof submitted to DarkFi (anchor + merkle proof verified)
     ↓
DarkFi mints wZEC to user
```

**Constants:**
- Minimum deposit: 0.0001 ZEC (10,000 zatoshi)
- Confirmations: 10 blocks

### Aztec (Private Rollup)

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
│  Private DeFi on DarkFi (wDAI)                  │
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
ZK proof submitted to DarkFi
     ↓
DarkFi mints wETH/wDAI to user
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
LTC → Bridge to DarkFi (privacy)
     ↓
DarkFi DeFi (fully private)
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
ZK proof submitted to DarkFi (merkle proof + optional MWEB verification)
     ↓
DarkFi mints wLTC to user
```

**Constants:**
- Minimum deposit: 0.001 LTC
- Confirmations: 6 blocks

## Universal Bridge Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        DarkFi Bridge                                  │
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
│                    │  DarkFi Contract  │                           │
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
| WithdrawV1 | 0x02 | Claim withdrawal to external chain |
| UpdateConfigV1 | 0x03 | Update bridge configuration |
| CancelWithdrawV1 | 0x04 | Cancel timed-out withdrawal |

## Withdrawal Timeout & Slashing

All withdrawals use a timeout mechanism:

```
User submits withdrawal
     ↓
Relayer picks up pending withdrawal
     ↓
Relayer executes on external chain
     ↓
If timeout (100 blocks) expires without execution:
     - User can cancel and reclaim funds
     - Relayer gets slashed (BRIDGE_CONTRACT_SLASH_AMOUNT)
```

**Trust Model:**
- Relayer is honest-but-curious (can observe but not steal)
- Economic incentives: relayer earns fee, gets slashed if fails
- User can always reclaim via CancelWithdrawV1

## ZK Circuits

| Circuit | Purpose | Chain |
|---------|---------|-------|
| deposit_v1.zk | Prove ETH deposit | Ethereum |
| withdraw_v1.zk | Prove withdrawal authorization | All |
| xmr_deposit_v1.zk | Prove Monero deposit (DLEq) | Monero |
| zec_deposit_v1.zk | Prove Zcash Sapling deposit | Zcash |
| azt_deposit_v1.zk | Prove Aztec rollup deposit | Aztec |
| ltc_deposit_v1.zk | Prove Litecoin deposit (MWEB optional) | Litecoin |

## Data Structures

### DepositParams

```rust
struct DepositParams {
    commitment: IntentCommitment,      // H(secret, amount, bridge_address)
    recipient_pub_x: [u8; 32],         // For address derivation
    recipient_pub_y: [u8; 32],
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

## Integration: Stablecoin & DEX

### Stablecoin Collateral

All bridged assets can serve as collateral for DarkFi's stablecoin:

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
Aztec private pool → DarkFi
         ↓
NETHER redemption rate adjusted by PI Controller
         ↓
Keeps NETHER/USD stable
```

The DAI peg creates natural price signals:
- **Into DarkFi**: External DAI price informs redemption rate
- **Out of DarkFi**: NETHER can be redeemed for DAI

### DEX Multi-Chain Support

The DEX **theoretically supports all bridged tokens**:

```
DEX on DarkFi:
┌─────────────────────────────────────────────────────────────┐
│  XMR/ETH swaps    │  ZEC/LTC swaps    │  DAI/ETH swaps     │
│  (via atomic      │  (shielded to     │  (via Aztec       │
│   swap)            │   transparent)    │   private)         │
└─────────────────────────────────────────────────────────────┘
         ↓                    ↓                    ↓
    wXMR pool             wZEC pool            wDAI pool
         ↓                    ↓                    ↓
    All tradable in DarkFi's privacy layer
```

## Security Model

### Deposit Security

Each chain has specific verification:

| Chain | Verification | Trust Model |
|-------|-------------|-------------|
| Ethereum | Merkle proof | Trustless (once confirmed) |
| Monero | DLEq proof | Honest-but-curious relayer |
| Zcash | Spend proof | Honest-but-curious relayer |
| Aztec | Note proof | Aztec rollup (ZK-verified) |
| Litecoin | Merkle proof | Trustless (once confirmed) |

### Object Capability Properties

1. **Bridge nodes cannot steal**: They never hold user secrets
2. **Self-signed withdrawals**: User alone authorizes via ZK proof
3. **Fresh addresses**: Nonce ensures temporal privacy
4. **Double-spend prevention**: Nullifiers tracked on DarkFi

### Relayer Security

- Relayers observe deposits but **cannot steal** (no spending authority)
- Withdrawal pre-authorization via ZK proof
- Timeout mechanism prevents indefinite withholding
- Slashing discourages relayer misbehavior

## Relayer Services

Each chain has a dedicated relayer service:

| Service | Binary | Status |
|---------|--------|--------|
| XMR Relayer | bin/xmr_relayer/ | Implemented (stubbed RPC) |
| ZEC Relayer | bin/zcash_relayer/ | Implemented (stubbed RPC) |
| AZT Relayer | bin/aztec_relayer/ | Implemented (stubbed RPC) |
| LTC Relayer | bin/litecoin_relayer/ | Implemented (stubbed RPC) |

All relayers follow the same pattern:
1. Observe external chain for deposits
2. Construct ZK proof data
3. Submit to DarkFi bridge contract
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

## References

- [Bridge Contract Dev Docs](../dev/contracts/bridge.md)
- [Monero Integration](./monero.md)
- [Stablecoin](./stablecoin.md)
- [DEX](./dex.md)
- Object Capability Model: <https://en.wikipedia.org/wiki/Object-capability_model>
