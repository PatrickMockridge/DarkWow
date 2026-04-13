# Stablecoin Contract

Monero-collateralized stablecoin using P2P Oracle for price discovery.

## Overview

The stablecoin contract enables minting a privacy-preserving stablecoin (e.g., a USD peg) backed by Monero collateralization.

**Key Innovation**: P2P Oracle for price discovery instead of trusted price feeds - avoids oracles being front-run or manipulated.

## Architecture

Based on the Nethermind P2P Oracle design with DarkFi privacy:

```
┌─────────────────────────────────────────────────────────────────┐
│                 Stablecoin System                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  COLLATERAL (XMR)                                               │
│       │                                                           │
│       │  Lock in contract                                        │
│       ▼                                                           │
│  ┌─────────────┐    ┌──────────────┐                            │
│  │ Collateral  │───→│ Price Oracle │                            │
│  │ Tracker     │    │ (P2P)        │                            │
│  └─────────────┘    └──────────────┘                            │
│       │                    │                                      │
│       │                    ▼                                      │
│       │            ┌──────────────┐                               │
│       │            │ Stability    │                               │
│       │            │ Mechanism    │                               │
│       │            └──────────────┘                               │
│       │                    │                                      │
│       ▼                    ▼                                      │
│  ┌─────────────────────────────────────┐                         │
│  │         Stablecoin (USDx)           │                         │
│  │  - Anonymous transfers               │                         │
│  │  - Collateral ratio tracking         │                         │
│  │  - Liquidation mechanism            │                         │
│  └─────────────────────────────────────┘                         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

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

### MoneyV3 Integration

Stablecoin uses MoneyV3 for token management:

```
Stablecoin Contract          MoneyV3 Contract
┌─────────────────┐         ┌─────────────────┐
│ CDP Mechanics    │──┐      │ Token Types     │
│ (collateral,     │  │      │ (USDx, collat)  │
│  debt, liquidation)         │                 │
└─────────────────┘  │      └─────────────────┘
                      │      ┌─────────────────┐
                      └───→  │ Mint/Burn       │
                             │ (via spend_hook)│
                             └─────────────────┘
```

- **InitializeV1**: Creates MoneyV3 token type for stablecoin
- **OpenPositionV1**: Mints collateral receipt tokens (MoneyV3 MintV1)
- **MintStableV1**: Burns collateral tokens, mints stablecoin (MoneyV3)
- **LiquidateV1**: Seizure via spend_hook callback, rewards via MoneyV3

### Cold Circuits (BaseDiv)

Circuits `governance_report_v1.zk` and `accrue_interest_v1.zk` use BaseDiv for precision interest calculations. These are executed rarely (monthly) so the attack surface is minimal.

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

## Liquidation Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                 Liquidation Flow                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. Position undercollateralized (ratio < 150%)                   │
│     → Liquidator detects opportunity                             │
│                                                                   │
│  2. Liquidator submits liquidation proof                          │
│     → ZK proof: position is undercollateralized                   │
│     → ZK proof: liquidator is authorized                          │
│                                                                   │
│  3. Contract executes:                                            │
│     → Liquidator pays stablecoin (debt)                          │
│     → Liquidator receives collateral (XMR)                        │
│     → Position closed                                             │
│                                                                   │
│  4. Bonus to liquidator (e.g., 10% of collateral)                 │
│     → Incentivizes active liquidation                             │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

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
│   │   ├── mod.rs
│   │   ├── open_position_v1.rs  # Poseidon commitment, BaseBlind
│   │   ├── mint_stable_v1.rs    # Poseidon commitment, BaseBlind
│   │   └── liquidate_v1.rs      # Poseidon commitment, BaseBlind
│   ├── entrypoint.rs            # WASM entrypoint
│   ├── error.rs
│   ├── lib.rs                   # Function enum, constants
│   └── model/
│       └── mod.rs               # Data types
├── Cargo.toml                   # Depends on darkfi_money_v3_contract
├── Makefile
└── tests/
```

## Building

```bash
cd src/contract/stablecoin
make          # Compile ZK circuits
cargo build --features client  # Build with client APIs
cargo test    # Run tests
```

**Note**: Client APIs use `BaseBlind` (Poseidon) instead of `ScalarBlind` (EC).
This is consistent with MoneyV3 and avoids EC heap bugs.

## References

- [Stablecoin Architecture](../../arch/stablecoin.md)
- [Nethermind P2P Oracle](https://github.com/NethermindEth/p2p-oracle)
- [MakerDAO DSR](https://docs.makerdao.com/)
