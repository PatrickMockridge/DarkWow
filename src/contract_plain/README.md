# Contract Plain — Partial Transparency for Real-Economy Applications

## Why This Directory Exists

The existing DarkFi contracts (`src/contract/`) are built inside ZK circuits. This gives strong privacy, but forces a painful tradeoff: **only operations expressible in ZK constraints can be used**.

This creates a **structural bias toward gambling and speculation** — games, prediction markets, and simple DeFi where the math maps easily to circuits. Real-economy applications like labor, insurance, healthcare, and education cannot be built efficiently because they require:

| Operation | ZK Status | Why It Matters |
|-----------|-----------|---------------|
| Division (`base_div`) | Not implemented | Time-weighted payments, premium calculations, coverage ratios |
| `less_than_or_equal` | **Unsound bug** | Could allow false proofs that steal funds |
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

| Contract | Purpose | Key ZK Limitation Avoided |
|----------|---------|---------------------------|
| `subscription/` | Access control with bitmask permissions | `base_div` for ratio checks |
| `labor_market/` | Freelance escrow with milestones | `base_div` for time-weighted release |
| `insurance/` | Mutual insurance with actuarial math | `base_div` for premium/claim ratios |
| `oracle/` | Data aggregation with slashable staking | `base_div` for weighted averages |
| `attestation/` | Credential chains with delegation | `base_div` for delegation ratios |

## Future Path

When ZK opcodes become sound and complete:
1. `base_div` lands in the ZKVM → plain contracts can port back
2. `less_than_or_equal` is fixed → ZK contracts gain range proofs
3. `set_membership` is added → private data aggregation becomes possible

Until then: **plain before unsound ZK**.

## Further Reading

- [Plain Contracts Architecture](../../doc/src/arch/plain_contracts.md) — Technical architecture details
- [Composability](../../doc/src/arch/composability.md) — How plain and ZK contracts compose
- [Parallel Societies](../../doc/src/arch/parallel_societies.md) — Privacy for social reproduction
- [Experimental Opcodes](../../doc/src/arch/experimental-opcodes.md) — Analysis of missing ZK opcodes