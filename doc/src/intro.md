# DarkFi Development Fork

**WARNING: This branch contains experimental, unaudited smart contracts. Do NOT deploy or use these contracts with real funds. They are for research and educational purposes only.**

This is a development fork of the official DarkFi repository. **Development occurs on the `master` branch** (`PatrickM123/darkfi:master`).

This fork contains all additions compared to official DarkFi master:

## New Smart Contracts

### Workstream 1: ZK Contracts (Current Opcodes)

ZK contracts using the existing opcode set. These prioritize **maximum privacy** but are constrained by circuit limitations.

| Contract | Description | Status |
|----------|-------------|--------|
| **attestation** | Generalized attestation and claims system | ✅ Complete + safemath predicates |
| **atomic_swap** | Cross-chain atomic swaps via HTLC | ✅ Complete |
| **baccarat** | Privacy-preserving Baccarat casino game | ✅ Complete |
| **betting_stake** | LP staking for betting contracts | ✅ Complete |
| **block_height_prediction** | PoW-backed block height betting | ✅ Complete |
| **bridge** | Cross-chain asset transfers with Ocaps | ✅ Complete |
| **dao_escrow** | DAO with three modes (Escrow/Treasury/Endowment) + optional DrainProtection | ✅ Complete |
| **darkbet_exchange** | Unified betting exchange (order-book + AMM) | ✅ Complete |
| **darktoshi_dice** | Privacy-preserving Satoshi Dice clone | ✅ Complete |
| **dex** | Atomic swap DAO with incremental transparency | ✅ Complete |
| **drain_protection** | Endowment/treasury protections (8 best practices) | ✅ Complete |
| **escrow** | Hashed Timelock Contract variant | ✅ Complete |
| **game_room** | Generalized betting and pot management | ✅ Complete |
| **identity** | ZK credential proofs using competency DAGs | ✅ Level 0 zk_only + Level 1 selective (bounded equation) |
| **insurance_market** | Decentralized insurance marketplace | ✅ Complete |
| **labor_market** | Job/labor market with escrow and DAO governance | ✅ Complete |
| **lottery** | Configurable lottery (bridge between BettingStake and Insurance) | ✅ Complete |
| **auction** | Privacy-preserving auction using escrow for bids | ✅ Complete |
| **oracle** | Push-model oracle with attestation integration | ✅ Complete |
| **roulette** | Privacy-preserving roulette casino game | ✅ Complete |
| **slot** | Composable slot machine with modular paytables | ✅ Complete |
| **tender** | Sealed-bid tendering with competency verification | ✅ Complete |
| **stablecoin** | Synthetix-style pooled debt with safemath | ✅ Uses safemath |
| **subscription** | Member subscription with DAO treasury | ✅ Complete |

### Workstream 2: Plain Contracts (Full Opcode Suite)

Plain WASM contracts planning for the full opcode suite. These use **partial transparency** to overcome current ZK limitations. When `base_div`, `less_than_or_equal` (sound), and other missing opcodes are available, these can be ported to ZK.

| Contract | Description | Status | Planning For |
|----------|-------------|--------|-------------|
| **subscription_plain** | Access control with bitmask permissions | ✅ Complete | `base_div` |
| **labor_market_plain** | Escrow with time-weighted release | ✅ Complete | `base_div` |
| **insurance_plain** | Actuarial calculations for premiums/claims | ✅ Complete | `base_div` |
| **oracle_plain** | Weighted aggregation with slashable staking | ✅ Complete | `base_div`, `set_membership` |
| **attestation_plain** | Delegation chains with depth limits | ✅ Complete | `base_div` |

**Principle**: "A malicious proof is more dangerous than a public bug." Plain contracts use native Rust where ZK opcodes are unsound or missing. See [Plain Contracts Architecture](arch/plain_contracts.md).

## Key Technical Changes

- **DarkBet Exchange**: Unified betting contract with order-book (back/lay) and AMM pool modes. Replaces prediction_market.
- **DrainProtection**: 8 optional best practices (graduated tiers, exit queue, circuit breaker, guardian pause, observation period, split proposals, no-loss reserve, dead man's switch). All features configurable by contract deployer and controllable by DAO members via governance.
- **Baccarat/Roulette/Slot**: Privacy-preserving casino games using cumulative PoW block hash entropy for dealing. Slot uses same composability pattern as Baccarat (Commit → Reveal → Settle) with swappable paytables and reel configurations.
- **Lottery**: Configurable lottery bridging BettingStake and Insurance market problem spaces
- **Insurance Market**: Decentralized insurance marketplace with risk markets ecosystem
- **Stablecoin refactor**: Synthetix-style pooled debt model (replacing individual CDPs)
- **Identity refactor**: Level 0 (zk_only) + Level 1 selective disclosure (bounded equation)
- **Safemath integration**: Production-ready ZK arithmetic templates as LessThanOrEqual workaround
- **Bridge Merkle fix**: Real `merkle_root` opcode (not fake proof)
- **Entropy module**: Shared `darkfi_sdk::crypto::entropy` for provable randomness across contracts
- **Game Room**: Generalized betting and pot management contract. App developers build poker rooms, backgammon rooms, etc. on top using the SDK. Room owner uses escrow-DAO for game rules and dispute resolution.

## Architecture Documentation

- [Experimental Opcodes](arch/experimental-opcodes.md) — Grey-market opcode analysis with gate soundness issues
- [Merkle Depth](arch/merkle_depth.md) — Fixed-depth limitations and workarounds
- [Composability](arch/composability.md) — Smart contract composition patterns with plain contracts integration
- [Safemath](arch/safemath.md) — ZK arithmetic templates (LessThanOrEqual workaround)
- [Field Arithmetic](arch/field_arithmetic.md) — zkVM primitive analysis
- [DarkBet Exchange](arch/darkbet_exchange.md) — Unified betting exchange (order-book + AMM)
- [Entropy Module](arch/entropy.md) — Provable randomness via block hash entropy
- [Provable Randomness](arch/provable_randomness.md) — PoW randomness analysis with casino game case studies
- [Game Room App Layer](arch/game_room_app_layer.md) — SDK integration guide for app developers
- [Plain Contracts Architecture](arch/plain_contracts.md) — Dual-layer ZK/plain contract design
- [Parallel Societies](arch/parallel_societies.md) — Privacy for social reproduction industries (labor, healthcare, insurance, education)

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
|----------|---------|------------------|
| stablecoin | Collateralization checks | Safemath `assert_lte` |
| identity | Threshold predicates | Safemath `assert_lte` (Level 0 zk_only), Bounded equation (Level 1) |
| dex | Partial fills | Safemath `less_than_strict` assertion |
| dao | Ratio checks | Cross-multiplication pattern |
| **bridge** | All deposit/withdraw operations | ✅ No workarounds needed! |

### What Would Be Fully Composable With Ideal Opcodes

| Contract | Feature | Needs |
|----------|---------|-------|
| stablecoin | Return Boolean for liquidation priority | LessThanOrEqual |
| identity | Level 1 selective disclosure (bounded equation) | ✅ Available now! |
| dex | Fill amount as value for further constraints | LessThanOrEqual |
| escrow | Atomic swap with partial fill Boolean return | LessThanOrEqual |
| dex | Price ratio calculations for order matching | `base_div` |

### Bridge = Opcode-Independent ✅

**The bridge is NOT held up by missing opcodes.**

The bridge uses **atomic swap semantics** which only need:
- Hash constraints (poseidon_hash)
- Merkle proofs (merkle_root)
- Range checks (range_check)

No division, no Boolean returns, no complex arithmetic. The bridge "just works" because atomic operations don't need the advanced opcodes.

See [Bridge Architecture](arch/bridge.md) for details on why the bridge doesn't need advanced opcodes.

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
