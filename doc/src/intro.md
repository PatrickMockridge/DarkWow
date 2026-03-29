# DarkFi Development Fork

**WARNING: This branch contains experimental, unaudited smart contracts. Do NOT deploy or use these contracts with real funds. They are for research and educational purposes only.**

This is a development fork of the official DarkFi repository. **Development occurs on the `master` branch** (`PatrickM123/darkfi:master`).

This fork contains all additions compared to official DarkFi master:

## New Smart Contracts

| Contract | Description | Status |
|----------|-------------|--------|
| **attestation** | Generalized attestation and claims system | ✅ Complete |
| **atomic_swap** | Cross-chain atomic swaps via HTLC | ✅ Complete |
| **bridge** | Cross-chain asset transfers with Ocaps | ✅ Complete |
| **dao_escrow** | DAO with three modes (Escrow/Treasury/Endowment) | ✅ Complete |
| **dex** | Atomic swap DAO with incremental transparency | ✅ Complete |
| **drain_protection** | Endowment/treasury protections (8 best practices) | ✅ Complete |
| **escrow** | Hashed Timelock Contract variant | ✅ Complete |
| **identity** | ZK credential proofs using competency DAGs | ✅ Uses safemath (Level 0 zk_only) |
| **labor_market** | Job/labor market with escrow and DAO governance | ✅ Complete |
| **auction** | Privacy-preserving auction using escrow for bids | ✅ Complete |
| **oracle** | Push-model oracle with attestation integration | ✅ Complete |
| **tender** | Sealed-bid tendering with competency verification | ✅ Complete |
| **stablecoin** | Synthetix-style pooled debt with safemath | ✅ Uses safemath |
| **subscription** | Member subscription with DAO treasury | ✅ Complete |

## Key Technical Changes

- **DrainProtection**: 8 optional best practices (graduated tiers, exit queue, circuit breaker, guardian pause, observation period, split proposals, no-loss reserve, dead man's switch). All features configurable by contract deployer and controllable by DAO members via governance.
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

## Security Status

All contracts are **EXPERIMENTAL** and **UNAUDITED**. Known security issues are documented in [Security Analysis](arch/security-analysis.md). The official DarkFi repository should be consulted for the canonical, production-ready state.

## Building

```bash
# Clone this fork
git clone https://codeberg.org/PatrickM123/darkfi
cd darkfi

# Build documentation
cd doc
mdbook build
```
