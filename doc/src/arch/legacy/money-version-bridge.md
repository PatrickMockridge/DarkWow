> **ARCHIVED**: This documents the original overlay-DAG architecture. The current consensus mechanism is [Uncle Merkle](../uncle_merkle.md).

# DarkWow Money: Fork vs Bridge

> **Historical: Explains bridge vs fork decision from 2024, money_v2 is now deployed**

---

## Decision: Fork, Not Bridge

We maintain `money_v2` as a separate contract alongside the original `money` (v1).
This is a hard fork of the money contract, not a bridge between versions.

---

## Repository Structure

```
src/contract/
├── money/           # Original DarkWow money contract (v1)
├── money_v2/        # Our secure version with fixes (v2) - STANDARD GOING FORWARD
├── dao/
├── dao_escrow/
└── ...
```

### Why Two Versions?

- **`money` (v1)**: Original DarkWow contract, maintained for network compatibility
- **`money_v2`**: Our secure version with self-contained circuit design, **our standard for this fork**

---

## Why Not a Bridge

### What a Bridge Would Do

Accept burns from both v1 and v2 versions, converting them to a unified format.

### Why We're Not Doing It

1. **A bridge inherits the design debt** - v1 circuits have incomplete binding. A bridge that accepts both keeps that incompleteness.

2. **Two code paths forever** - Maintaining bridge logic indefinitely doubles attack surface and maintenance burden.

3. **Doesn't solve the root issue** - The bridge would still rely on external verification layers for v1 proofs.

4. **zkas architecture** - Without opcode composition (`verify_proof`), a bridge is always a manual, imperfect workaround.

---

## The Fork Decision

```
Instead of:  v1 ◄──► Bridge ◄──► v2 (forever coupled)
We choose:   v1 (legacy) ──► v2 (clean break)
```

### Why This Is Correct

1. **Clean solution** - One circuit, one security model, no compromises
2. **No inheritance of debt** - v2 money stands on its own
3. **Easier audit** - Single path, no bridge logic to verify
4. **Future-proof** - New features won't inherit v1 design constraints

---

## What We're Forking For

**Clean, self-contained circuit design.**

Not:
- ❌ "We're unsafe and need to fix"
- ❌ "There's an active exploit"
- ❌ "v1 is broken"

But:
- ✅ "We want provably correct circuits"
- ✅ "We want defense in depth"
- ✅ "We want auditability without tracing layers"

---

## Migration Path

```
Phase 1: Deploy money_v2
├── money_v2 contract deployed
├── New applications use money_v2
└── money (v1) remains functional (legacy)

Phase 2: Migration
├── Users migrate at their pace
├── No forced migration deadline initially
└── v1 enters maintenance mode

Phase 3: Deprecation
├── As usage shifts to v2
├── v1 contract deprecated
└── Fork is complete
```

---

## Security Properties

| | money (v1) | money_v2 |
|---|--------|-----|
| Namespace | `Fee_V1`, `Burn_V1`, etc. | `Fee_V2`, `Burn_V2`, etc. |
| Self-contained circuit | ❌ Relies on external | ✅ Complete in circuit |
| Defense in depth | ❌ Single layer | ✅ Layered |
| Clean audit | ⚠️ Multi-layer | ✅ Single circuit |
| constrain_equal_base | ❌ Missing | ✅ Present |

---

## See Also

- [Money Vulnerability Analysis](./money-vulnerability-analysis.md) - Full reasoning for the fork
- [Security Analysis](./security-analysis.md) - Audit details
- [Public Key Constraint Hook](./pubkey-constraint-hook.md) - Prevention mechanism
