# DarkFi Money: Fork vs Bridge

## Decision: Fork, Not Bridge

We're doing a hard fork of the money contract, not building a bridge between versions.

---

## Why Not a Bridge

### What a Bridge Would Do

Accept burns from both master and dev versions, converting them to the new format.

### Why We're Not Doing It

1. **A bridge inherits the design debt** - Master circuits have incomplete binding. A bridge that accepts both keeps that incompleteness.

2. **Two code paths forever** - Maintaining bridge logic indefinitely doubles attack surface and maintenance burden.

3. **Doesn't solve the root issue** - The bridge would still rely on external verification layers for master proofs.

4. **zkas architecture** - Without opcode composition (`verify_proof`), a bridge is always a manual, imperfect workaround.

---

## The Fork Decision

```
Instead of:  Master ◄──► Bridge ◄──► Dev (forever coupled)
We choose:   Master (legacy) ──► Dev (clean break)
```

### Why This Is Correct

1. **Clean solution** - One circuit, one security model, no compromises
2. **No inheritance of debt** - Dev money stands on its own
3. **Easier audit** - Single path, no bridge logic to verify
4. **Future-proof** - New features won't inherit master design

---

## What We're Forking For

**Clean, self-contained circuit design.**

Not:
- ❌ "We're unsafe and need to fix"
- ❌ "There's an active exploit"
- ❌ "Master is broken"

But:
- ✅ "We want provably correct circuits"
- ✅ "We want defense in depth"
- ✅ "We want auditability without tracing layers"

---

## Migration Path

```
Phase 1: Deploy dev money
├── Dev money contract deployed
├── New applications use dev
└── Master remains functional (legacy)

Phase 2: Migration
├── Users migrate at their pace
├── No forced migration deadline initially
└── Master enters maintenance mode

Phase 3: Deprecation
├── As usage shifts to dev
├── Master contract deprecated
└── Fork is complete
```

---

## Security Properties

| | Master | Dev |
|---|--------|-----|
| Self-contained circuit | ❌ Relies on external | ✅ Complete in circuit |
| Defense in depth | ❌ Single layer | ✅ Layered |
| Clean audit | ⚠️ Multi-layer | ✅ Single circuit |

---

## See Also

- [Money Vulnerability Analysis](./money-vulnerability-analysis.md) - Full reasoning for the fork
- [Security Analysis](./security-analysis.md) - Audit details
- [Public Key Constraint Hook](./pubkey-constraint-hook.md) - Prevention mechanism
