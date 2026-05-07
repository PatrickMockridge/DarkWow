# Stablecoin Contract

Monero-collateralized stablecoin using P2P Oracle for price discovery.

## Overview

The stablecoin contract enables minting a privacy-preserving stablecoin (e.g., a USD peg) backed by Monero collateralization.

**Key Innovation**: P2P Oracle for price discovery instead of trusted price feeds - avoids oracles being front-run or manipulated.

## Architecture

Based on the Nethermind P2P Oracle design with DarkWow privacy. Uses **Pooled Debt Model** (Synthetix-style) where all collateral backs all debt.

```
┌─────────────────────────────────────────────────────────────────┐
│                 Stablecoin System                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  User Deposits Collateral                                         │
│       │                                                           │
│       ▼                                                           │
│  ┌─────────────┐    ┌──────────────┐                            │
│  │ MoneyV3     │───→│ Stablecoin   │                            │
│  │ (Token)     │    │ (CDP Engine) │                            │
│  │ collateral  │    │              │                            │
│  │ receipt     │    │ Pooled Debt  │                            │
│  └─────────────┘    └──────────────┘                            │
│                           │                                       │
│       ┌───────────────────┴───────────────────┐                  │
│       ▼                                       ▼                  │
│  ┌─────────────┐                      ┌─────────────┐         │
│  │ MintStable  │                      │ Liquidate   │         │
│  │ (burn col,  │                      │ (burn USDx, │         │
│  │  mint USDx) │                      │  seize col) │         │
│  └─────────────┘                      └─────────────┘         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Pooled Debt Model

Unlike individual CDP models (MakerDAO), this uses pooled debt:
- **All collateral backs all debt** (no individual positions)
- **Simpler ZK circuits** (no per-position Merkle proofs)
- **Better privacy** (no position IDs that could leak)
- **Entire pool is liquidatable** (not individual positions)

See [model/mod.rs](../../contract/stablecoin/src/model/mod.rs) for `StablecoinModel` enum (PooledDebt, Liquity, Fractional, IndividualCdp).

## MoneyV3 Integration

Stablecoin uses **MoneyV3** for token management via `spend_hook`:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Cross-Contract Flow                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. OPEN POSITION                                                     │
│     User ──► Stablecoin::OpenPositionV1                              │
│              │                                                        │
│              └───► MoneyV3::MintV1 (mint collateral receipt tokens)   │
│                       spend_hook = stablecoin_contract_id               │
│                                                                      │
│  2. MINT STABLE                                                       │
│     User ──► MoneyV3::BurnV1 (burn collateral tokens)                 │
│              │                                                        │
│              └───► spend_hook ──► Stablecoin::exec()                  │
│                                        │                              │
│                                        └───► MoneyV3::MintV1 (mint USDx)│
│                                                                      │
│  3. REPAY STABLE                                                      │
│     User ──► MoneyV3::BurnV1 (burn USDx)                              │
│              │                                                        │
│              └───► spend_hook ──► Stablecoin::exec()                  │
│                                        │                              │
│                                        └───► MoneyV3::MintV1 (mint col)│
│                                                                      │
│  4. LIQUIDATE                                                         │
│     User ──► MoneyV3::BurnV1 (burn USDx to cover debt)               │
│              │                                                        │
│              └───► spend_hook ──► Stablecoin::exec() (seizure)       │
│                                        │                              │
│                                        └───► MoneyV3::MintV1 (col seized)
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### spend_hook Mechanism

When a token is burned with `spend_hook`:
1. MoneyV3 verifies the burn proof
2. MoneyV3 calls `stablecoin.exec(user_data)`
3. If stablecoin returns success, the burn is finalized
4. If stablecoin returns error, the entire transaction aborts

This enables **atomic cross-contract operations**:
- Burn collateral token → Mint stablecoin (in one transaction)
- Burn stablecoin → Receive collateral (in one transaction)

## P2P Oracle: Avoiding Front-Running

Traditional AMM/Oracle problems:
- Uniswap TWAPs can be manipulated with flash loans
- Chainlink price feeds can be front-run
- Centralized oracles create single points of failure

P2P Oracle solution:
- Lenders and borrowers negotiate directly
- Price agreed upon at loan creation
- No on-chain price to manipulate
- Privacy-preserving (amounts/tokens hidden)

## Key Components

### Collateral Tracker

Monitors collateral positions:
- Locked XMR amount per position
- Real-time value tracking
- Liquidation threshold alerts

### P2P Price Oracle

Bilateral price negotiation:
- Borrower proposes terms
- Lender accepts terms
- Price locked at agreement time
- No on-chain price exposure

### Stability Mechanism

Maintains peg stability:
- Over-collateralization required (e.g., 150%)
- Liquidation if ratio drops below threshold
- Penalty mechanism for undercollateralized positions

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize stablecoin |
| OpenPositionV1 | 0x01 | Open collateralized position |
| AddCollateralV1 | 0x02 | Add collateral to position |
| RemoveCollateralV1 | 0x03 | Remove collateral from position |
| MintStableV1 | 0x04 | Mint stablecoin against collateral |
| RepayStableV1 | 0x05 | Repay stablecoin debt |
| LiquidateV1 | 0x06 | Liquidate undercollateralized position |
| UpdateConfigV1 | 0x07 | Update parameters |

## ZK Circuits

All stablecoin circuits use **Poseidon-only** design for maximum security and simplicity.

| Circuit | Purpose | Key Properties |
|---------|---------|----------------|
| `open_position_v1.zk` | Prove valid collateral position | Poseidon commitments, no EC |
| `mint_stable_v1.zk` | Prove collateral ratio | Poseidon commitments, no EC |
| `liquidate_v1.zk` | Prove valid liquidation | Poseidon commitments, no EC |

### Circuit Design Principles

**Poseidon-only** (no EC operations):
- Public key: `poseidon_hash(owner_secret)`
- Commitments: `poseidon_hash(value, blind)`
- Nullifiers: `poseidon_hash(owner_secret, position_commitment)`

This avoids the heap bug issues that affected MoneyV2 circuits. See [standards.md](./standards.md) for full analysis.

### Cold Circuits (BaseDiv)

Circuits `governance_report_v1.zk` and `accrue_interest_v1.zk` use `BaseDiv` (0x58) — a **DarkWow addition** not present in upstream DarkFi's zkVM — for precision interest calculations. These are executed rarely (monthly) so the attack surface is minimal.

## Function Reference

| Function | ID | Description | ZK Circuit |
|----------|-----|-------------|------------|
| InitializeV1 | 0x00 | Initialize stablecoin, create MoneyV3 token | - |
| OpenPositionV1 | 0x01 | Open collateralized position | open_position_v1.zk |
| AddCollateralV1 | 0x02 | Add collateral to position | (pending) |
| RemoveCollateralV1 | 0x03 | Remove collateral from position | (pending) |
| MintStableV1 | 0x04 | Mint stablecoin against collateral | mint_stable_v1.zk |
| RepayStableV1 | 0x05 | Repay stablecoin debt | (pending) |
| LiquidateV1 | 0x06 | Liquidate undercollateralized position | liquidate_v1.zk |
| AccrueInterestV1 | 0x07 | Accrue position interest | accrue_interest_v1.zk |
| GovernanceReportV1 | 0x08 | Update governance parameters | governance_report_v1.zk |
| UpdateConfigV1 | 0x09 | Update contract parameters | - |

## Liquidation Flow (Pooled Debt Model)

In the pooled debt model, liquidation is **global** - the entire pool is either healthy or liquidatable:

```
┌─────────────────────────────────────────────────────────────────┐
│                 Pooled Liquidation Flow                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. Pool becomes undercollateralized (ratio < threshold)          │
│     → Global check, not per-position                              │
│                                                                   │
│  2. Liquidator calls MoneyV3::BurnV1                              │
│     → spend_hook = stablecoin_contract_id                         │
│     → user_data encodes seizure parameters                       │
│                                                                   │
│  3. Stablecoin::exec() callback verifies:                          │
│     → Pool ratio is below threshold                              │
│     → Debt coverage is valid                                     │
│     → Seizure calculation correct                               │
│                                                                   │
│  4. Contract executes:                                            │
│     → Burn USDx (debt coverage)                                  │
│     → Release seized collateral proportionally                   │
│     → Liquidator receives via MoneyV3::MintV1                     │
│                                                                   │
│  NOTE: Individual positions are NOT tracked.                     │
│        The pool itself is the only state.                       │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Difference from Individual CDP Model

| Aspect | Individual CDP (MakerDAO) | Pooled Debt (This) |
|--------|--------------------------|-------------------|
| State | Per-position tracking | Global pool only |
| Liquidation | Per-position | Entire pool |
| Privacy | Position IDs leak | No positions |
| ZK Complexity | Complex per-position | Simple global |
| Risk | Individual position | Whole pool risk |

## Structure

```
src/contract/stablecoin/
├── proof/
│   ├── open_position_v1.zk      # Poseidon-only
│   ├── mint_stable_v1.zk        # Poseidon-only
│   ├── liquidate_v1.zk          # Poseidon-only
│   ├── accrue_interest_v1.zk     # Cold circuit (BaseDiv)
│   └── governance_report_v1.zk  # Cold circuit (BaseDiv)
├── src/
│   ├── client/
│   │   ├── mod.rs               # Exports TokenMintCallInput from MoneyV3
│   │   ├── initialize_v1.rs     # Creates MoneyV3 token (USDx)
│   │   ├── open_position_v1.rs  # CollateralMintBuilder
│   │   ├── mint_stable_v1.rs    # CollateralBurnBuilder
│   │   └── liquidate_v1.rs      # spend_hook documentation
│   ├── entrypoint.rs            # WASM entrypoint
│   ├── error.rs
│   ├── lib.rs                   # Function enum, constants
│   └── model/
│       └── mod.rs               # Data types, InitializeParams
├── Cargo.toml                   # Depends on darkfi_money_v3_contract
├── Makefile
└── tests/
```

### Client API Integration Points

| Function | MoneyV3 Integration |
|----------|-------------------|
| `InitializeV1` | Creates stablecoin token type in MoneyV3 |
| `OpenPositionV1` | Mints collateral receipt tokens via MoneyV3::MintV1 |
| `MintStableV1` | Burns collateral via MoneyV3::BurnV1 with spend_hook |
| `RepayStableV1` | Burns stablecoin, mints collateral via spend_hook |
| `LiquidateV1` | Burns stablecoin, seized collateral via spend_hook |

## Building

```bash
cd src/contract/stablecoin
make          # Compile ZK circuits
cargo build --features client  # Build with client APIs
cargo test    # Run tests
```

### Client API Usage

```rust
use darkfi_stablecoin_contract::client::{
    initialize_v1::InitializeCallBuilder,
    open_position_v1::CollateralMintBuilder,
    mint_stable_v1::CollateralBurnBuilder,
};

// Initialize and create MoneyV3 token
let init_debris = InitializeCallBuilder {
    model: StablecoinModel::PooledDebt,
    create_token: true,
    token_symbol: "USDx".into(),
    ..Default::default()
}.build();

// Open position, mint collateral receipt
let mint_debris = CollateralMintBuilder {
    owner_pub: user_public_key,
    collateral_amount: 1000,
    collateral_type: CollateralType::Xmr,
    token_id: collateral_token_id,
    stablecoin_contract_id,
    user_data: position_commitment,
}.build();

// Mint stablecoin by burning collateral
let burn_debris = CollateralBurnBuilder {
    collateral_coin: receipt_coin,
    owner_secret: user_secret,
    mint_amount: 500,
    stablecoin_contract_id: stablecoin_id,
    stablecoin_token_id: usdx_token_id,
    collateral_token_id,
}.build();
```

## References

- [MoneyV3 Integration](money_v3.md) - Token contract used by stablecoin
- [Contract Standards](standards.md) - Poseidon-only, spend_hook design
- [Stablecoin Architecture](../../contract/stablecoin.md)
- [Nethermind P2P Oracle](https://github.com/NethermindEth/p2p-oracle)
- [MakerDAO DSR](https://docs.makerdao.com/)
