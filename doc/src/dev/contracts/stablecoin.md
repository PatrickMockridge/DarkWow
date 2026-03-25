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
| MintStableV1 | 0x02 | Mint stablecoin against collateral |
| LiquidateV1 | 0x03 | Liquidate undercollateralized position |
| UpdateConfigV1 | 0x04 | Update parameters |

## ZK Circuits

- `open_position_v1.zk`: Prove valid collateral without revealing amount
- `mint_stable_v1.zk`: Prove collateral ratio without revealing full position
- `liquidate_v1.zk`: Prove liquidation is valid

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
│   ├── open_position_v1.zk
│   ├── mint_stable_v1.zk
│   └── liquidate_v1.zk
├── src/
│   ├── client/mod.rs
│   ├── entrypoint.rs
│   ├── error.rs
│   ├── lib.rs
│   └── model/mod.rs
├── tests/
├── Cargo.toml
└── Makefile
```

## Building

```bash
cd src/contract/stablecoin
make
make proof
cargo test
```

## References

- [Stablecoin Architecture](../../arch/stablecoin.md)
- [Nethermind P2P Oracle](https://github.com/NethermindEth/p2p-oracle)
- [MakerDAO DSR](https://docs.makerdao.com/)
