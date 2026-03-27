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

## ZK Circuits

The stablecoin contract uses ZK circuits for privacy-preserving position management:

### open_position_v1.zk

Proves a valid collateralized position can be opened:
- **Public inputs**: position_commitment, owner_pub, collateral_type, position_nullifier
- **Private inputs**: owner_secret, collateral_amount, debt_amount
- **Verification**: Owner key valid, collateral/debt commitments correct, ratios satisfied

### mint_stable_v1.zk

Proves stablecoin can be minted against collateral:
- **Public inputs**: old_commitment, new_commitment, position_nullifier, mint_amount
- **Private inputs**: owner_secret, old_collateral, old_debt, new_collateral, new_debt
- **Verification**: Nullifier valid, debt arithmetic correct, collateral unchanged

### liquidate_v1.zk

Proves a position can be liquidated:
- **Public inputs**: old_commitment, new_commitment, position_nullifier, collateral_amount, debt_amount
- **Private inputs**: owner_secret, liquidator_reward
- **Verification**: Position undercollateralized, liquidation penalty correct

## Base Field Arithmetic

ZK circuits operate in a finite field — the Pallas field defined by prime `p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1`. All arithmetic wraps at `p`, which breaks normal integer intuitions:

```zk
# In the field, p-1 ≡ -1, so comparisons must be carefully designed
# Ratio comparisons (e.g., collateral/debt >= 2.0) require cross-multiplication
# Never use division in a circuit — always cross-multiply
```

**Why this matters for stablecoin**: The collateralization ratio check `collateral / debt >= min_ratio` must be expressed as field arithmetic. Direct division (`BaseDiv`) doesn't exist — instead we prove `collateral * 1 >= min_ratio * debt` via cross-multiplication using `base_mul` + `less_than_strict`.

**The pattern (from `dao/exec.zk` lines 118-126)**:
```zk
# To prove: collateral/debt >= 2.0 (200% collateralization)
# We prove: collateral >= 2 * debt
# I.e., collateral - 2*debt >= 0 as integers (not field elements!)

lhs = base_mul(collateral, 1);          # collateral * 1
rhs = base_mul(liquidation_threshold, debt_value);  # threshold * debt
less_than_strict(rhs, lhs);             # Prove: rhs < lhs
# If this passes, we know: threshold * debt < collateral (as integers, not field!)
```

**Why this works**: Cross-multiplication avoids division entirely. We compute `threshold * debt` and prove it's less than `collateral`. The field wraparound doesn't break this because we're proving a relation between products, not computing a quotient.

**The core challenge**: Field wraparound means `a - b` as field subtraction is not the same as `a - b` as integer subtraction when `a < b`. The `less_than_strict` opcode constrains this correctly, but only when the inputs are bounded to prevent wraparound.

**See**: [Field Arithmetic Constraints](../../../doc/src/arch/field_arithmetic.md) for the full treatment.

## Opcode Discovery and Validation

**Opcode discovery must go hand-in-hand with building functionality** — not precede it.

When building the stablecoin contract's `open_position_v1.zk` circuit, we discovered that:
1. The 200% collateralization check requires comparing `collateral_value` against `liquidation_threshold * debt_value` — a ratio comparison
2. The `LessThanStrict` opcode could constrain the relation but not return a value for further logic
3. Cross-multiplication avoids the need for `BaseDiv`, but requires bounding inputs to prevent field wraparound
4. `LessThanOrEqual` was needed to return a 0/1 result for further constraints

**The correct workflow**:
1. Build the circuit with what exists
2. When a constraint can't be expressed, document the opcode gap
3. Implement the new opcode only when the actual use case is known
4. Validate the opcode against the specific circuit that needs it — not in isolation

The stablecoin contract was instrumental in driving `LessThanOrEqual` integration — the actual collateralization check is what made the opcode gap visible.

## Reasoned Opcodes

The stablecoin circuits use these zkVM opcodes:

**Implemented (available now):**
- `EcMulShort(value, point)` — Pedersen commitments (`value * VALUE_COMMIT`)
- `EcAdd(a, b)` — Adding EC points for Pedersen commitments
- `BaseAdd`, `BaseSub`, `BaseMul` — Field arithmetic for debt calculations
- `LessThanStrict`, `LessThanLoose` — Comparison constraints (constrain only, no return value)
- `LessThanOrEqual(a, b)` — Returns 1 if `a <= b`, 0 otherwise (**experimental** — grey-market goods)
- `RangeCheck`, `BoolCheck` — Range proofs

> **Note on ratio checks**: Liquidation ratio checks (e.g., `collateral / debt < threshold`) do NOT need a `BaseDiv` opcode. As demonstrated in `dao/exec.zk` lines 118-126, ratio checks use cross-multiplication: prove `a/b < c/d` by asserting `a*d < b*c` via `base_mul` + `less_than_strict`. The TWAP price is expected to be supplied as an external oracle input, not computed in-circuit.

**See also**: [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md) for full reasoning on comparison opcodes.

## Opcode Safety

**`LessThanOrEqual` is a grey-market good — buyer beware.**

`LessThanOrEqual` (0x55) and `IsEqualBase` (0x54) are implemented in the zkVM and integrated in `open_position_v1.zk` and `liquidate_v1.zk`. They are **not production-ready**:

| Concern | Status |
|---------|--------|
| Isolation testing | Pass — the opcode works in isolation |
| Integration tests | None — no end-to-end test for stablecoin lifecycle |
| Formal audit | Not started |
| Delta-invert soundness | Unresolved — may be unsound near field boundary |
| Blast radius if broken | **Critical** — collateralization ratios can be spoofed, enabling undercollateralized positions |

**The stablecoin's risk profile is higher than identity's**: A broken `LessThanOrEqual` in identity allows a holder to claim they meet a threshold when they don't. A broken `LessThanOrEqual` in stablecoin allows creating a position with 50% collateral when 200% is required — directly draining the system.

**What production readiness requires**:
1. Integration test: open position → add collateral → mint stable → repay → liquidate (adversarial)
2. Formal soundness proof or concrete bound on delta-invert failure
3. Audit by a ZK circuit expert with formal verification
4. Fuzzing with adversarial inputs: values near `p`, near 0, overflow cases

**Current status**: The stablecoin uses these opcodes because the alternative (cross-multiplication alone, which only constrains but doesn't return a value) is insufficient for the full collateralization logic. But the system's security relies on the opcode being correct.

**See**: [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md) for the full delta-invert analysis.

## Key Blockers

| Blocker | Severity | Description |
|---------|----------|-------------|
| `LessThanOrEqual` soundness | **Critical** | Delta-invert concern may allow undercollateralized positions |
| No P2P oracle | **Critical** | NETHER/DRK AMM pool for TWAP doesn't exist on-chain |
| CDP Note integration | **High** | Money contract's `spend_hook` to CDP engine not implemented |
| No integration test | **High** | Cannot verify full lifecycle: open → add → mint → repay → liquidate |
| `liquidate_v1.zk` unreviewed | **Medium** | Circuit uses experimental opcode, needs audit |

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

## MVP Status

**Blocked on Architecture** — `LessThanOrEqual` is integrated (experimental). Ratio checks use cross-multiplication (no `BaseDiv` needed). The primary blockers are oracle and CDP integration.

| Circuit | Status | Notes |
|---------|--------|-------|
| `open_position_v1.zk` | Verified | Uses `less_than_or_equal` for 200% collateralization check. **Experimental opcode.** |
| `mint_stable_v1.zk` | Corrected | Base arithmetic uses existing `base_add` opcode |
| `liquidate_v1.zk` | Partial | Uses `less_than_or_equal` for reward bounds check. Ratio check uses cross-multiplication. **Experimental opcode.** |

### Blockers

1. **`LessThanOrEqual` is experimental** — Grey-market goods; no integration tests, delta-invert soundness concern. See [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md) for what production readiness requires.
2. **No P2P oracle** — The NETHER/DRK AMM pool for TWAP price discovery does not yet exist on-chain. TWAP price is expected as an external oracle input.
3. **CDP Note integration stubbed** — The money contract's `spend_hook` pointing to the CDP engine is not implemented.

### What It Needs

First: P2P oracle / AMM integration to supply TWAP price. Second: CDP Note integration with money contract. Third: integration testing of the full lifecycle.

> **Note on division**: Ratio checks like `collateral / debt < threshold` use cross-multiplication (see `dao/exec.zk`), not `BaseDiv`. `BaseDiv` is not a blocker.

**See**: [Contract MVP Status](../../../doc/src/arch/mvp_status.md) for the full cross-contract analysis.

## References

- [P2P Oracle Design](https://technologytruth.substack.com/p/nether-say-nether-again)
- [DarkFi Money Contract](../money/)
- [DarkFi SDK](../../../src/sdk/)
- [Halo 2 Documentation](https://halo2.dev/)
- [Poseidon Hash](https://poseidon.hrage.org/)
- [Contract MVP Status](../../../doc/src/arch/mvp_status.md)