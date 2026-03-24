# DarkFi Stablecoin Contract (CDP)

A privacy-preserving collateralized debt position (CDP) stablecoin for DarkFi, inspired by Nethermind's P2P Oracle design.

## Overview

This contract enables creation of a stablecoin (e.g., a USD-pegged token) that is:

- **Privacy-preserving**: All positions, amounts, and identities are hidden via ZK proofs
- **Self-stabilizing**: AMM-based TWAP + PI Controller replaces governance
- **Censorship-resistant**: No trusted price oracles, no centralized control
- **Self-sovereign**: Users control their own collateral and debt

## Design Principles

### Traditional CDP Problems (MakerDAO, etc.)

1. **Oracle dependency**: Single source or median of oracles can be manipulated
2. **Governance overhead**: DAO votes needed for rate adjustments
3. **No privacy**: All positions and amounts are public
4. **Centralization**: Governance can freeze addresses, update parameters

### P2P Oracle Solution

```
Traditional: User → Governance-controlled oracle → Price feed
P2P Oracle:  User → AMM TWAP (NETHER/DRK pool) → Price feed
```

**Key innovations:**

1. **AMM-based TWAP**: The NETHER/DRK constant-product pool itself provides price discovery. TWAP naturally smooths out short-term manipulation.

2. **PI Controller**: A Proportional-Integral controller adjusts redemption rate based on TWAP deviation:
   - TWAP > target (premium): rate increases → less borrowing
   - TWAP < target (discount): rate decreases → more borrowing

3. **Full privacy**: Pedersen commitments hide collateral/debt amounts. Merkle tree stores commitments. ZK proofs verify all operations.

4. **Minimal governance**: The PI controller replaces most governance decisions. Only emergency interventions require DAO action.

## Architecture

### Core Components

| Component | Description |
|-----------|-------------|
| **CDP Engine** | WASM contract managing positions via Sparse Merkle Tree |
| **CDP Notes** | Money contract coins with `spend_hook` pointing to CDP Engine |
| **Stablecoin Token** | Minted/burned exclusively by CDP Engine |
| **PI Controller** | Algorithmic rate adjustment based on TWAP |
| **ZK Circuits** | Open, add collateral, remove, mint, repay, liquidate |

### Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize CDP engine with parameters |
| OpenPositionV1 | 0x01 | Create new collateralized debt position |
| AddCollateralV1 | 0x02 | Add collateral to existing position |
| RemoveCollateralV1 | 0x03 | Remove collateral (if ratio allows) |
| MintStableV1 | 0x04 | Mint stablecoin against collateral |
| RepayStableV1 | 0x05 | Repay debt to unlock collateral |
| LiquidateV1 | 0x06 | Liquidate undercollateralized position |
| UpdateConfigV1 | 0x07 | Update CDP parameters (governance) |

### Data Flow

#### Opening a Position

```
1. User computes: commitment = H(secret, collateral, debt, owner_pub)
2. User provides ZK proof:
   - Commitment is correctly formed
   - Collateral >= minimum
   - Debt <= collateral / min_ratio
3. CDP Engine verifies proof, inserts into SMT
4. CDP Engine mints stablecoins to user
```

#### Liquidation

```
1. Anyone monitors positions for undercollateralization
2. When (collateral * twap) / debt < liquidation_threshold:
   - Liquidator provides ZK proof of undercollateralization
   - CDP Engine burns stablecoins (debt)
   - Liquidator receives collateral (minus penalty)
   - Position is zeroed out
```

## Security Model

### Collateralization Requirements

- **Minimum collateralization**: 150% (15000 basis points)
- **Liquidation threshold**: 130% (13000 basis points)
- **Liquidation penalty**: 10% (1000 basis points)

### PI Controller Parameters

- **Kp (proportional gain)**: 1000
- **Ki (integral gain)**: 100
- **TWAP window**: 1 hour
- **Price deviation threshold**: 5%

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Oracle manipulation | TWAP from AMM is manipulation-resistant |
| Undercollateralization | ZK circuit verifies ratio before minting |
| Griefing attacks | Liquidation requires valid ZK proof + economic incentive |
| Governance capture | PI controller minimizes governance needed |

## Comparison: Traditional vs P2P Oracle CDP

| Aspect | MakerDAO DAI | DarkFi Stablecoin |
|--------|--------------|-------------------|
| Price oracle | Chainlink (centralized) | AMM TWAP (decentralized) |
| Rate governance | DAO voting | PI Controller (algorithmic) |
| Privacy | Public | Full ZK privacy |
| Freeze authority | Maker Foundation | None |
| Collateral | Multiple (ETH, WBTC, etc.) | XMR, DRK (initially) |
| Liquidation | Keeper auctions | Anyone via ZK proof |

## Implementation Status

This is a **draft/placeholder**. The following items need implementation:

### Phase 1: Core CDP Mechanics

- [ ] Position commitment and SMT integration
- [ ] Open position circuit and contract logic
- [ ] Add/remove collateral circuit and logic
- [ ] Mint stable circuit and logic
- [ ] Repay stable circuit and logic
- [ ] Liquidate circuit and logic

### Phase 2: P2P Oracle

- [ ] NETHER/DRK AMM pool integration
- [ ] TWAP calculation circuit
- [ ] PI Controller implementation
- [ ] Redemption rate updates

### Phase 3: CDP Notes

- [ ] Money contract integration
- [ ] Spend hook to CDP Engine
- [ ] User data encoding for commitments

### Phase 4: Testing & Audit

- [ ] Integration tests
- [ ] Fuzzing for edge cases
- [ ] Security audit

## References

- [P2P Oracle Design](https://technologytruth.substack.com/p/nether-say-nether-again)
- [DarkFi Money Contract](../money/)
- [DarkFi SDK](../../sdk/)
- [Halo 2 Documentation](https://halo2.dev/)
- [Poseidon Hash](https://poseidon.hrage.org/)