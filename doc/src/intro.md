# DarkFi Development Fork

**WARNING: This branch contains experimental, unaudited smart contracts. Do NOT deploy or use these contracts with real funds. They are for research and educational purposes only.**

This is a development fork of the official DarkFi repository. **Development occurs on the `master` branch** (`PatrickM123/darkfi:master`).

This fork contains all additions compared to official DarkFi master:

## Smart Contracts

ZK contracts using the existing opcode set. These prioritize **maximum privacy** with full ZK expressiveness.

| Contract | Description | Status |
|----------|-------------|--------|
| **attestation** | Generalized attestation and claims system | ✅ Complete |
| **atomic_swap** | Cross-chain atomic swaps via HTLC | ✅ Complete |
| **auction** | Privacy-preserving auction using escrow for bids | ✅ Complete |
| **baccarat** | Privacy-preserving Baccarat casino game | ✅ Complete |
| **betting_stake** | LP staking for betting contracts | ✅ Complete |
| **block_height_prediction** | PoW-backed block height betting | ✅ Complete |
| **bridge** | Cross-chain asset transfers with Ocaps | ✅ Complete |
| **dao** | DAO with voting and treasury management | ✅ Complete |
| **dao_escrow** | DAO with three modes (Escrow/Treasury/Endowment) + optional DrainProtection | ✅ Complete |
| **darkbet_exchange** | Unified betting exchange (order-book + AMM) | ✅ Complete |
| **darktoshi_dice** | Privacy-preserving Satoshi Dice clone | ✅ Complete |
| **dex** | Atomic swap DAO with incremental transparency | ✅ Complete |
| **drain_protection** | Endowment/treasury protections (8 best practices) | ✅ Complete |
| **escrow** | Hashed Timelock Contract variant | ✅ Complete |
| **game_room** | Generalized betting and pot management | ✅ Complete |
| **identity** | ZK credential proofs using competency DAGs | ✅ Level 0 zk_only + Level 1 selective |
| **insurance_market** | Decentralized insurance marketplace | ✅ Complete |
| **labor_market** | Job/labor market with escrow and DAO governance | ✅ Complete |
| **lottery** | Configurable lottery (bridge between BettingStake and Insurance) | ✅ Complete |
| **money** | Original DarkFi money (v1) - upstream legacy | No |
| **money_v2** | Secure version with constrain_equal_base | ✅ Standard |
| **oracle** | Push-model oracle with attestation integration | ✅ Complete |
| **roulette** | Privacy-preserving roulette casino game | ✅ Complete |
| **slot** | Composable slot machine with modular paytables | ✅ Complete |
| **stablecoin** | Synthetix-style pooled debt | ✅ Complete |
| **subscription** | Member subscription with DAO treasury | ✅ Complete |
| **tender** | Sealed-bid tendering with competency verification | ✅ Complete |

> **Note**: Plain contracts in `src/contract_plain/` have been **deprecated**. The ZK opcodes they workaround (`LessThanOrEqual`, `BaseDiv`) are now verified sound and implemented. See [Contract Plain Deprecation](arch/contract_plain_deprecation.md).

## Key Technical Changes

- **DarkBet Exchange**: Unified betting contract with order-book (back/lay) and AMM pool modes. Replaces prediction_market.
- **DrainProtection**: 8 optional best practices (graduated tiers, exit queue, circuit breaker, guardian pause, observation period, split proposals, no-loss reserve, dead man's switch). All features configurable by contract deployer and controllable by DAO members via governance.
- **Baccarat/Roulette/Slot**: Privacy-preserving casino games using cumulative PoW block hash entropy for dealing. Slot uses same composability pattern as Baccarat (Commit → Reveal → Settle) with swappable paytables and reel configurations.
- **Lottery**: Configurable lottery bridging BettingStake and Insurance market problem spaces
- **Insurance Market**: Decentralized insurance marketplace with risk markets ecosystem
- **Stablecoin refactor**: Synthetix-style pooled debt model (replacing individual CDPs)
- **Identity refactor**: Level 0 (zk_only) + Level 1 selective disclosure (bounded equation)
- **Safemath integration**: Legacy ZK arithmetic templates (LessThanOrEqual now verified sound - safemath still useful for assertion-only patterns)
- **Bridge Merkle fix**: Real `merkle_root` opcode (not fake proof)
- **Entropy module**: Shared `darkfi_sdk::crypto::entropy` for provable randomness across contracts
- **Game Room**: Generalized betting and pot management contract. App developers build poker rooms, backgammon rooms, etc. on top using the SDK. Room owner uses escrow-DAO for game rules and dispute resolution.

## Architecture Documentation

- [Opcodes and Formal Verification](arch/opcodes.md) — Opcode soundness verification with Lean 4 proofs
- [Merkle Depth](arch/merkle_depth.md) — Fixed-depth limitations and workarounds
- [Composability](arch/composability.md) — Smart contract composition patterns
- [Contract Plain Deprecation](arch/contract_plain_deprecation.md) — Resolution of dual-layer architecture
- [Safemath](arch/safemath.md) — ZK arithmetic templates for assertion-only comparisons
- [Field Arithmetic](arch/field_arithmetic.md) — zkVM primitive analysis
- [DarkBet Exchange](arch/darkbet_exchange.md) — Unified betting exchange (order-book + AMM)
- [Entropy Module](arch/entropy.md) — Provable randomness via block hash entropy
- [Provable Randomness](arch/provable_randomness.md) — PoW randomness analysis with casino game case studies
- [Game Room App Layer](arch/game_room_app_layer.md) — SDK integration guide for app developers
- [Plain Contracts Architecture](arch/plain_contracts.md) — [DEPRECATED] Dual-layer ZK/plain contract design
- [O-Cap & Composable Privacy](arch/ocap.md) — Privacy for social reproduction industries (labor, healthcare, insurance, education)

## Security Status

All contracts are **EXPERIMENTAL** and **UNAUDITED**. Known security issues are documented in [Security Analysis](arch/security-analysis.md). The official DarkFi repository should be consulted for the canonical, production-ready state.

## Technical Debt: Opcode Layer

### Current Status (Updated)

Most comparison opcodes are now **formally verified** or **implemented**:

| Opcode | Status | Notes |
|--------|--------|-------|
| `LessThanOrEqual` (0x55) | ✅ **Verified Sound** | Lean 4 exhaustive testing |
| `LessThanStrict` (0x51) | ✅ Sound | Constrain-only, inherently safe |
| `LessThanLoose` (0x52) | ✅ Sound | Constrain-only |
| `NotBase` (0x56) | ✅ Verified | Production-ready |
| `BaseLtStrict` (0x57) | ✅ Verified | Production-ready |
| `BaseDiv` (0x58) | ✅ **Implemented** | Binary exponentiation (Fermat's theorem) |
| `IsEqualBase` (0x54) | ❌ Bug | Delta-invert unconstrained when `a == b` - do not use |

### Remaining Issues

**IsEqualBase (0x54)** has a bug: when `a == b`, `delta_invert` is unconstrained. Use `ConstrainEqualBase` for assertion-only checks.

### What Works Now

| Contract | Feature | Status |
|----------|---------|--------|
| stablecoin | Collateralization checks | ✅ LessThanOrEqual or Safemath |
| identity | Threshold predicates | ✅ LessThanOrEqual verified |
| dex | Partial fills | ✅ LessThanOrEqual verified |
| dao | Ratio checks | ✅ BaseDiv or cross-multiplication |
| **bridge** | All deposit/withdraw operations | ✅ No workarounds needed! |

### Migration Complete

Plain contracts have been **deprecated** in favor of ZK contracts since:
- `LessThanOrEqual` is formally verified sound
- `BaseDiv` is implemented

See [Contract Plain Deprecation](arch/contract_plain_deprecation.md) for details.

### Bridge = Opcode-Independent ✅

**The bridge is NOT held up by missing opcodes.**

The bridge uses **atomic swap semantics** which only need:
- Hash constraints (poseidon_hash)
- Merkle proofs (merkle_root)
- Range checks (range_check)

No division, no Boolean returns, no complex arithmetic. The bridge "just works" because atomic operations don't need the advanced opcodes.

See [Bridge Architecture](arch/bridge.md) for details on why the bridge doesn't need advanced opcodes.

**See**: [Safemath](arch/safemath.md) and [Opcodes Reference](arch/opcodes.md) for full analysis.

## Building

```bash
# Clone this fork
git clone https://codeberg.org/PatrickM123/darkfi
cd darkfi

# Build documentation
cd doc
mdbook build
```
