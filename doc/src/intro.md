# DarkFi Development Fork

**WARNING: This branch contains experimental, unaudited smart contracts. Do NOT deploy or use these contracts with real funds. They are for research and educational purposes only.**

This is a development fork of the official DarkFi repository. **Development occurs on the `master` branch** (`PatrickM123/darkfi:master`).

This fork contains all additions compared to official DarkFi master:

## New Smart Contracts

| Contract | Description | Status |
|----------|-------------|--------|
| **attestation** | Generalized attestation and claims system | ✅ Complete |
| **atomic_swap** | Cross-chain atomic swaps via HTLC | ✅ Complete |
| **baccarat** | Privacy-preserving Baccarat casino game | ✅ Complete |
| **bridge** | Cross-chain asset transfers with Ocaps | ✅ Complete |
| **dao_escrow** | DAO with three modes (Escrow/Treasury/Endowment) | ✅ Complete |
| **dex** | Atomic swap DAO with incremental transparency | ✅ Complete |
| **drain_protection** | Endowment/treasury protections (8 best practices) | ✅ Complete |
| **escrow** | Hashed Timelock Contract variant | ✅ Complete |
| **identity** | ZK credential proofs using competency DAGs | ✅ Uses safemath (Level 0 zk_only) |
| **insurance_market** | Decentralized insurance marketplace | ✅ Complete |
| **labor_market** | Job/labor market with escrow and DAO governance | ✅ Complete |
| **auction** | Privacy-preserving auction using escrow for bids | ✅ Complete |
| **oracle** | Push-model oracle with attestation integration | ✅ Complete |
| **prediction_market** | AMM-based prediction market | ✅ Complete |
| **block_height_prediction** | PoW-backed block height betting | ✅ PoC |
| **tender** | Sealed-bid tendering with competency verification | ✅ Complete |
| **stablecoin** | Synthetix-style pooled debt with safemath | ✅ Uses safemath |
| **subscription** | Member subscription with DAO treasury | ✅ Complete |

## Key Technical Changes

- **DrainProtection**: 8 optional best practices (graduated tiers, exit queue, circuit breaker, guardian pause, observation period, split proposals, no-loss reserve, dead man's switch). All features configurable by contract deployer and controllable by DAO members via governance.
- **Baccarat contract**: Privacy-preserving casino game using cumulative PoW block hash entropy for card dealing
- **Prediction Market**: AMM-based prediction market with PoW-backed resolution
- **Insurance Market**: Decentralized insurance marketplace with risk markets ecosystem
- **Stablecoin refactor**: Synthetix-style pooled debt model (replacing individual CDPs)
- **Identity refactor**: Level 0 (zk_only) with safemath assertion gadgets
- **Safemath integration**: Production-ready ZK arithmetic templates as LessThanOrEqual workaround
- **Bridge Merkle fix**: Real `merkle_root` opcode (not fake proof)

## Architecture Documentation

- [Experimental Opcodes](arch/experimental-opcodes.md) — Grey-market opcode analysis with gate soundness issues
- [Merkle Depth](arch/merkle_depth.md) — Fixed-depth limitations and workarounds
- [Composability](arch/composability.md) — Smart contract composition patterns
- [Safemath](arch/safemath.md) — ZK arithmetic templates (LessThanOrEqual workaround)
- [Field Arithmetic](arch/field_arithmetic.md) — zkVM primitive analysis
- [Baccarat](arch/baccarat.md) — Casino game using cumulative PoW entropy
- [Prediction Market](arch/prediction_market.md) — AMM-based prediction market
- [Provable Randomness](arch/provable_randomness.md) — PoW randomness analysis with Baccarat case study

## Security Status

All contracts are **EXPERIMENTAL** and **UNAUDITED**. Known security issues are documented in [Security Analysis](arch/security-analysis.md). The official DarkFi repository should be consulted for the canonical, production-ready state.

## Technical Debt: Opcode Layer

**⚠️ Important**: Several contracts use **workaround patterns** because ideal opcodes are not yet production-ready.

### The Problem

DarkFi's zkVM operates in the Pallas field where `LessThanOrEqual` and `IsEqualBase` are **implemented but have unverified soundness**:

| Opcode | Issue | Impact |
|--------|-------|--------|
| `LessThanOrEqual` (0x55) | Gate soundness unverified | Returns Boolean for composability |
| `IsEqualBase` (0x54) | Delta-invert issue when `a == b` | Returns Boolean for composability |

### The Workaround

Contracts use **safemath assertion gadgets** (`assert_lte_u64_v1.zk`):
- Pattern: prove `a <= b` via `a < b + 1` using `less_than_strict`
- Production-ready (uses only proven opcodes)
- **BUT**: Constrain-only, no Boolean return value
- Cannot replace LessThanOrEqual when Boolean is needed for downstream logic

### Full Composability Requires

Once the following are formally verified:
1. **`LessThanOrEqual` soundness** → Returns 0/1 Boolean for full circuit composability
2. **`IsEqualBase` soundness** → Correct equality checks in all cases
3. **`base_div`** → True division in circuits (currently uses cross-multiplication workaround)

### What Works Now (with workarounds)

| Contract | Feature | Workaround Used |
|----------|---------|----------------|
| stablecoin | Collateralization checks | Safemath `assert_lte` |
| identity | Threshold predicates | Safemath `assert_lte` (Level 0 zk_only) |
| dex | Partial fills | Safemath `less_than_strict` assertion |
| dao | Ratio checks | Cross-multiplication pattern |

### What Would Be Fully Composable With Ideal Opcodes

| Contract | Feature | Needs |
|----------|---------|--------|
| stablecoin | Return Boolean for liquidation priority | LessThanOrEqual |
| identity | Level 1 selective disclosure (reveal predicate) | LessThanOrEqual + IsEqualBase |
| dex | Fill amount as value for further constraints | LessThanOrEqual |
| escrow | Atomic swap with partial fill Boolean return | LessThanOrEqual |

**See**: [Safemath](arch/safemath.md) and [Experimental Opcodes](arch/experimental-opcodes.md) for full analysis.

## Building

```bash
# Clone this fork
git clone https://codeberg.org/PatrickM123/darkfi
cd darkfi

# Build documentation
cd doc
mdbook build
```
