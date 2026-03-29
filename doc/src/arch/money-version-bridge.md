# DarkFi Money: The Fork Decision

## Executive Summary

**We are NOT building a bridge between master and dev money. We are forking.**

This document explains why the bridge approach was rejected and why the hard fork is the correct decision.

---

## Why Not a Bridge?

### The Bridge Would Inherit the Vulnerability

A bridge that accepts both master and dev proofs would:

1. **Accept master proofs** - which have the vulnerable pattern
2. **Cannot fix the vulnerability** - it's baked into the proof
3. **Create false confidence** - users think they're secure, but the vulnerability is still present

The only way to "fix" a master proof is to re-verify it at the Rust layer:

```rust
// If we try to add binding verification at the bridge layer...
let derived_pub = PublicKey::from_secret(signature_secret);
if derived_pub != input.signature_public {
    return Err(BridgeError::Unbound.into());
}
```

But this defeats the purpose of ZK circuits - we're adding trust assumptions back in.

### Maintaining Two Paths Is Technical Debt

A bridge that handles both versions:
- Doubles the code to maintain
- Doubles the attack surface
- Creates complexity that will compound over time
- Is a permanent patch for a broken circuit

### zkas Can't Help

Even if we wanted to implement a proper bridge, zkas has no opcode composition:

```
No verify_proof opcode  ──►  Cannot verify proofs from other circuits inline
No interface types      ──►  Cannot define "Money::Burn" as an interface
No version negotiation  ──►  Cannot accept multiple versions transparently
```

Without these primitives, a bridge is always a manual, imperfect workaround.

---

## The Correct Decision: Hard Fork

Instead of:

```
Master ◄──► Bridge ◄──► Dev
    │              │
    └── Broken ────┘
```

We choose:

```
Master (deprecated) ──► Dev (new standard)
```

### Why This Is Correct

1. **Clean break** - No legacy code carrying the vulnerability
2. **Clear security model** - Only dev, only secure
3. **Easier to audit** - One path, not two
4. **No false confidence** - No "mostly secure" bridge

### The Migration Path

```
Phase 1: Deploy dev money
├── Dev money contract deployed
├── New coins use dev
└── Master still functional but deprecated

Phase 2: Migration event
├── Coordinated migration ceremony
├── Users burn master coins, mint dev coins
├── Snapshot of master state transferred
└── Migration ends

Phase 3: Deprecation
├── Master contract deprecated
├── All value in dev money
└── Fork complete
```

---

## What We Gain From the Fork

### Security

| | Master | Dev |
|---|--------|-----|
| Issue 19 vulnerability | ❌ Present | ✅ Fixed |
| Signature binding | ❌ None | ✅ Enforced |
| Nullifier replay | ⚠️ Possible | ✅ Prevented |

### Simplicity

| | Master | Dev |
|---|--------|-----|
| Code paths | 1 (vulnerable) | 1 (secure) |
| Attack surface | Larger | Smaller |
| Audit complexity | High (vulnerability hidden) | Low (circuit enforces) |

### Privacy (NOT Sacrificed)

The fix does NOT add privacy leakage:
- `signature_public` was already public in master
- The binding only proves what you already knew
- No new information is revealed

---

## What We Lose

1. **Instant backward compatibility** - Old proofs won't work
2. **Coordinated upgrade required** - Migration ceremony needed
3. **Some users may lose funds** - If they can't/won't migrate

But these are acceptable losses because:
- The vulnerability in master is FUNDAMENTAL
- You can't patch a circuit without changing it
- The migration is a one-time cost

---

## The Philosophical Point

**A ZK circuit that doesn't enforce its invariants is broken.**

Master money's circuit says "I'm proving knowledge of signature_secret" but doesn't actually enforce that the signature_public matches. This is:

1. A bug, not a feature
2. Not something a bridge can fix
3. Only fixable via circuit change

Dev money fixes this. The fork is the correct response to a broken circuit.

---

## See Also

- [Money Vulnerability Analysis](./money-vulnerability-analysis.md)
- [Security Analysis: Issue 19](./security-analysis.md#issue-19-missing-public-key-constraint-majormd--fixed)
- [Public Key Constraint Hook](./pubkey-constraint-hook.md)
- [zkas Opcode Limitations](./opcode_universe.md)
