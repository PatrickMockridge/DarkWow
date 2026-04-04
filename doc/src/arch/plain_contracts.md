# Plain Contracts: A Dual-Layer Architecture for Composable Privacy

> **DEPRECATED**: This document describes the historical dual-layer architecture that has been **resolved**.
> All ZK opcode limitations have been addressed:
> - `LessThanOrEqual` (0x55) is **verified sound**
> - `BaseDiv` (0x58) is **implemented**
>
> Plain contracts in `src/contract_plain/` are deprecated. Use ZK contracts in `src/contract/` instead.
> See [Contract Plain Deprecation](contract_plain_deprecation.md) for the resolution.

---

*This document describes DarkFi's dual-layer contract architecture enabling both maximum privacy (ZK) and maximum expressivity (Plain WASM) for real-economy applications.*

## The Problem: ZK Circuit Limitations Create Structural Bias

The current DarkFi ZK contract architecture (`src/contract/`) is constrained by missing or unsound ZK circuit opcodes:

| Opcode | Impact | Status |
|--------|--------|--------|
| `base_div` | Division in ZK circuits | ✅ Mathematically verified (impl pending) |
| `less_than_or_equal` | Range proofs | ✅ **Verified Sound** via Lean 4 |
| `is_equal_base` | Equality checks | ❌ Bug (delta_invert unconstrained) |
| No Keccak/SHA-256 | Limited hash function options | Not available |
| No variable exponentiation | Cannot express exponential functions | Not implemented |

**This creates a structural bias toward mathematically simple operations** - gambling, speculation, and simple DeFi - because these can be expressed in ZK circuits. Real-economy applications like labor markets, insurance, and complex credential systems cannot be built efficiently.

As identified in the [DarkFi Development Uncensored analysis](https://technologytruth.substack.com/p/darkfi-development-uncensored-part-c9b):

> The current ZK-first approach creates inherent structural biases favoring certain types of work (speculative finance) over others (ordinary labor, mutual insurance, real-economy contracts).

## The Solution: Dual-Layer Architecture

DarkFi implements a **dual-layer contract architecture**:

```
┌─────────────────────────────────────────────────────────────────┐
│                   DARKFI CONTRACT LAYERS                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  LAYER 1: ZK CONTRACTS (src/contract/)                          │
│  ─────────────────────────────────────────────                   │
│  • Maximum privacy - state is private                              │
│  • Constrained by circuit expressiveness                         │
│  • Ideal for: gambling, prediction markets, simple DeFi           │
│                                                                   │
│  LAYER 2: PLAIN CONTRACTS (src/contract_plain/)                  │
│  ─────────────────────────────────────────────                   │
│  • Partial transparency - state is public on-chain                 │
│  • Unlimited expressiveness - any Rust arithmetic                 │
│  • Ideal for: labor, insurance, oracle, real-economy             │
│                                                                   │
│  KEY INSIGHT: "A malicious proof is more dangerous than            │
│               a public bug."                                    │
│               - Visible bugs are fixable                         │
│               - Invisible theft via unsound ZK is catastrophic   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Why "Partial Transparency" is Sometimes Better

The principle guiding plain contracts:

> **We prefer plain over ZK-with-unsound-opcodes.**

### The Danger of Unsound ZK Opcodes

With a **plain contract**:
- Incorrect behavior is **visible** on-chain
- Anyone can see if funds are being stolen
- Attackers must act in plain sight

With a **ZK contract using unsound opcodes**:
- A **malicious proof** can appear valid while stealing funds
- The verifier believes the proof is sound when it's not
- Bug in `less_than_or_equal` allows prover to choose `out=0` bypassing intended logic
- Result: **undetectable theft** while the system thinks verification passed

### Security Comparison

| Contract Type | Visibility | Security Guarantee |
|--------------|------------|-------------------|
| ZK (sound opcodes) | Private | Cryptographically enforced |
| ZK (unsound opcodes) | Private | **BROKEN** - malicious proofs accepted |
| Plain WASM | Public | Visibility enables detection and response |

**We will not use unsound ZK opcodes** even if they would provide "privacy". A visible bug is fixable; an invisible theft is catastrophic.

## Composable Privacy: Privacy as a Human Right

For privacy to be truly exceptional and a **human right**, it must be **composable** for **ordinary people doing ordinary jobs**:

### The Structural Bias Problem

If only gambling/speculation contracts can be built with ZK privacy:
- Privacy becomes a **luxury** for those in financial speculation
- Ordinary workers (freelancers, nurses, teachers) cannot get privacy-preserving contracts
- Creates **parallel societies**: formal economy ( surveilled) vs. private economy (only for gamblers)

### Industries Vital to Social Reproduction

These industries are essential for societal survival but are systematically excluded from privacy-preserving systems:

| Industry | Why Vital | Current Privacy Options |
|----------|-----------|------------------------|
| **Healthcare** | Medical decisions should be private | None in DarkFi |
| **Domestic Labor** | Care work, cleaning, cooking | None |
| **Education** | Tutoring, skill training | None |
| **Freelance Work** | Programming, writing, design | Limited |
| **Mutual Insurance** | Community risk pooling | Basic in ZK |
| **Union Organization** | Collective bargaining | None |

### How Plain Contracts Enable Composable Privacy

Plain contracts enable real-economy applications that ZK cannot:

| Plain Contract | What It Enables | Social Reproduction Role |
|----------------|-----------------|--------------------------|
| `subscription_plain` | Tiered access control with bitmask permissions | Content/services subscriptions |
| `labor_market_plain` | Milestone-based freelance escrow | Freelance work contracts |
| `insurance_plain` | Actuarial premium calculations | Mutual aid societies |
| `oracle_plain` | Weighted data aggregation | Community price feeds |
| `attestation_plain` | Hierarchical credential chains | Professional certifications |

## Architecture Overview

### File Structure

```
src/
├── contract/                    # ZK contracts (EXISTING - DO NOT MODIFY)
│   ├── darktoshi_dice/
│   ├── baccarat/
│   ├── roulette/
│   ├── subscription/
│   └── ...
│
├── contract_plain/              # Plain WASM contracts (NEW)
│   ├── subscription/            # ✅ IMPLEMENTED
│   ├── labor_market/           # ⏳ PENDING
│   ├── insurance/              # ⏳ PENDING
│   ├── oracle/                 # ⏳ PENDING
│   └── attestation/            # ⏳ PENDING
│
└── sdk/                         # Shared SDK
```

### Design Principles

1. **DO NOT modify existing contracts** - Keep `src/contract/` as-is
2. **Add new plain contracts as alternatives** - New `src/contract_plain/` folder
3. **"Partial transparency"** - Not fully private like ZK contracts, but enables complex logic
4. **Composition-first** - Plain WASM contracts that can call ZK contracts
5. **Opcode placeholders** - Document where missing opcodes would be used
6. **Privacy compromise documentation** - Every place where privacy is traded for expressivity must be clearly documented

## Privacy Compromise Documentation Pattern

Every plain contract includes a `PRIVACY_TRADEOFFS.md` documenting:

```markdown
# Privacy Tradeoffs

## What This Contract Gives Up

| Feature | ZK Version | Plain Version | Privacy Impact |
|---------|-----------|---------------|----------------|
| Access control | Merkle tree commitment | Public bitmask | All permissions visible on-chain |

## Opcode Dependencies

| Opcode | Status | Fallback | Impact |
|--------|--------|----------|--------|
| `base_div` | NOT IMPLEMENTED | Cross-multiplication workaround | Limited ratio checks |

## Data Visibility

All state is public:
- User subscription tiers visible
- Payment amounts visible
- Access permissions visible
```

## Opcode Soundness Status

This table documents which ZK opcodes are sound vs. unsound (as of Lean 4 formal verification):

| Opcode | Status | Can Use in ZK? |
|--------|--------|-----------------|
| `EcAdd` | ✅ Sound | Yes |
| `EcMul` | ✅ Sound | Yes |
| `PoseidonHash` | ✅ Sound | Yes |
| `SchnorrVerify` | ✅ Sound | Yes |
| `base_div` | ✅ Mathematically verified | Implementation pending |
| `less_than_or_equal` | ✅ **Verified Sound** | **Yes** - Lean 4 exhaustive testing |
| `is_equal_base` | ❌ Bug | No - delta_invert unconstrained when a==b |

**Updated**: `LessThanOrEqual` is now formally verified sound. `BaseDiv` is mathematically verified (Fermat's little theorem). `IsEqualBase` remains buggy.

**Migration path now open**: Contracts previously forced to plain due to unsound `LessThanOrEqual` can now use the ZK version.

## Implementation Status

| Contract | Status | Key Features |
|---------|--------|--------------|
| `subscription_plain` | ✅ Implemented | True bitmask access control, ratio-based rate limiting |
| `labor_market_plain` | ⏳ Pending | Time-weighted payment release, milestone escrow |
| `insurance_plain` | ⏳ Pending | Actuarial premium calculations, claims verification |
| `oracle_plain` | ⏳ Pending | Weighted aggregation, slashable staking |
| `attestation_plain` | ⏳ Pending | Hierarchical credentials, delegation chains |

## Cross-Layer Composition

Plain contracts can call ZK contracts and vice versa:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    CROSS-LAYER COMPOSITION                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ZK Contract (e.g., Money)                                              │
│     │                                                                     │
│     │  ZK: Verify signature, constrain value                              │
│     ▼                                                                     │
│  Plain Contract (e.g., labor_market_plain)                               │
│     │                                                                     │
│     │  Plain: Time-weighted release, milestone tracking                   │
│     │                                                                     │
│     │  Calls Money contract for token transfers                           │
│     ▼                                                                     │
│  Result: Complex real-economy logic + ZK soundness for financial moves  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## ZK Migration Path (COMPLETED)

**COMPLETED**: All ZK opcode limitations have been resolved. Plain contracts are deprecated.

| Plain Contract | ZK Replacement | Status |
|---------------|----------------|--------|
| `subscription_plain` | `subscription` | ✅ Migrated |
| `labor_market_plain` | `labor_market` | ✅ Migrated |
| `insurance_plain` | `insurance_market` | ✅ Migrated |
| `oracle_plain` | `oracle` | ✅ Migrated |
| `attestation_plain` | `attestation` | ✅ Migrated |

See [Contract Plain Deprecation](contract_plain_deprecation.md) for details.
1. Replace native `&` bitmask with ZK Merkle tree constraint
2. Replace `base_div` calls with native `base_div` opcode (now implemented) or cross-multiplication
3. Keep subscription commitments private
4. Maintain ZK soundness for financial operations

## See Also

- [Composability](./composability.md) - Cross-contract patterns
- [zkVM Primitives](./zkvm_primitives.md) - Opcode-level analysis
- [Opcodes and Formal Verification](./opcodes.md) - Lean 4 verification results
- [Subscription Contract](./subscription.md) - ZK version (limited)
- [Privacy Tradeoffs](./privacy_tradeoffs.md) - Security comparison