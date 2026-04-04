# Contract Plain — DEPRECATED

> **DEPRECATED**: This directory is deprecated. All functionality has been migrated to ZK contracts in `../contract/`.
>
> The ZK opcodes these contracts worked around (`base_div`, `less_than_or_equal`) are now sound and implemented.
> See [Migration](#migration-to-zk-contracts) below.

---

## Why This Directory Exists (Historical Context)

The existing DarkFi contracts (`src/contract/`) are built inside ZK circuits. This gives strong privacy, but forces a painful tradeoff: **only operations expressible in ZK constraints can be used**.

This creates a **structural bias toward gambling and speculation** — games, prediction markets, and simple DeFi where the math maps easily to circuits. Real-economy applications like labor, insurance, healthcare, and education cannot be built efficiently because they require:

| Operation | ZK Status | Why It Matters |
|-----------|-----------|---------------|
| Division (`base_div`) | ✅ Mathematically verified (implementation pending) | Time-weighted payments, premium calculations, coverage ratios |
| `less_than_or_equal` | ✅ **Verified Sound** (Lean 4) | Range proofs with Boolean return |
| Variable exponentiation (`power`) | Not implemented | Complex risk models |
| Standard hashes (SHA-256, Keccak) | Not available | Integration with external systems |

**When a ZK opcode is unsound, a malicious proof is more dangerous than a public bug.** A bug in plain code is visible and fixable. A false proof appears valid while draining funds.

## Our Solution: Dual-Layer Architecture

```
src/contract/          → ZK contracts (privacy-preserving, circuit-limited)
src/contract_plain/    → Plain WASM contracts (partial transparency, full expressivity)
```

### Layer 1: ZK Contracts (existing `src/contract/`)
- Maximum privacy — all state hidden in commitments
- Use when: privacy is critical, operations are simple (gambling, voting, swaps)
- Constraint: limited by what ZK circuits can express

### Layer 2: Plain Contracts (new `src/contract_plain/`)
- State is **public on-chain**, but logic is unrestricted
- Use when: real-economy operations require division, complex ratios, or unsound ZK opcodes
- Advantage: full Rust expressiveness, composition with ZK contracts
- Tradeoff: privacy sacrificed for correctness

## Security Principle

> **We prefer plain over ZK-with-unsound-opcodes.**

With a **plain contract**:
- Incorrect behavior is **visible** on-chain
- Anyone can detect theft or bugs immediately
- Attackers must act in plain sight

With a **ZK contract using unsound opcodes**:
- A **malicious proof** passes verification while stealing funds
- The system believes everything is correct when it's not
- Result: undetectable theft that the protocol thinks it caught

## What We Give Up

Every plain contract documents its tradeoffs in `PRIVACY_TRADEOFFS.md`:

| Data | ZK Version | Plain Version |
|------|------------|---------------|
| Payment amounts | Hidden in commitments | Public on-chain |
| Access permissions | Hidden in Merkle tree | Public bitmask |
| Time ratios | Not available (no `base_div`) | Public (native division) |

For **ordinary people doing ordinary jobs** — freelance work, healthcare, mutual insurance — public amounts are acceptable. What matters is that the **logic is correct and auditable**.

## The Composition Path

Plain contracts are **not replacements** for ZK contracts — they are **complement**. A labor contract could:

1. Use ZK contracts for identity/attestation (privacy-preserving)
2. Use plain contracts for payment escrow (requires `base_div`)
3. Use ZK contracts for signature verification (sound)

This is **composable privacy** — different operations use different privacy levels based on what the operation actually needs.

## Contracts in This Directory

| Contract | Purpose | Key ZK Limitation Avoided | ZK Replacement |
|----------|---------|---------------------------|---------------|
| `subscription/` | Access control with bitmask permissions | `base_div` for ratio checks | `../contract/subscription/` |
| `labor_market/` | Freelance escrow with milestones | `base_div` for time-weighted release | `../contract/labor_market/` |
| `insurance/` | Mutual insurance with actuarial math | `base_div` for premium/claim ratios | `../contract/insurance_market/` |
| `oracle/` | Data aggregation with slashable staking | `base_div` for weighted averages | `../contract/oracle/` |
| `attestation/` | Credential chains with delegation | `base_div` for delegation ratios | `../contract/attestation/` |

## Migration to ZK Contracts

**COMPLETED**: All ZK opcodes are now sound and implemented:

| Opcode | Status | Lean 4 Proof |
|--------|--------|--------------|
| `LessThanOrEqual` (0x55) | ✅ **SOUND** | `proofs/lean/src/Main.lean` |
| `BaseDiv` (0x58) | ✅ **IMPLEMENTED** | `proofs/lean/src/Main.lean` |
| `IsEqualBase` (0x54) | ⚠️ Minor issue (doesn't enable false proofs) | `proofs/lean/src/Main.lean` |

Plain contracts have been **deprecated**. Use the ZK versions in `../contract/` instead.

### Why ZK Over Plain?

| Aspect | Plain Contract | ZK Contract |
|--------|---------------|-------------|
| Privacy | Partial (amounts public) | Full (commitments hidden) |
| Expressivity | Full Rust | Limited to circuit constraints |
| Auditability | Visible on-chain | Requires proof verification |
| Safety | Bug visible | Bug hidden but proof still verifies |

The ZK contracts now have equivalent functionality with full privacy.

## Further Reading

- [ZK Contract Architecture](../../doc/src/arch/contract_architecture.md) — ZK contract design patterns
- [Composability](../../doc/src/arch/composability.md) — Cross-contract composition
- [Opcodes Reference](../../doc/src/arch/opcodes.md) — Opcode soundness verification (Lean 4 proofs)

## Further Reading

- [Plain Contracts Architecture](../../doc/src/arch/plain_contracts.md) — Technical architecture details
- [Composability](../../doc/src/arch/composability.md) — How plain and ZK contracts compose
- [Parallel Societies](../../doc/src/arch/parallel_societies.md) — Privacy for social reproduction
- [Opcodes Reference](../../doc/src/arch/opcodes.md) — Opcode soundness verification