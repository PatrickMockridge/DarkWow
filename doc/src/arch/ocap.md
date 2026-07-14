# O-Cap: Emergent Types from the ρ-Calculus

This document defines the object-capability model for DarkWow. It SHALL be read
in conjunction with the **[Type System Specification](type-system.md)**. The
type system defines the primitive types and their behavioral positions. This
document defines how those primitives compose into capabilities, and how
capability types emerge from interaction patterns.

## 0. Foundation: The ρ-Calculus and Behavioral Types

The DarkWow type system is derived from the ρ-calculus. A **type** is a
behavioral position in a concurrent interaction graph ([type-system.md §1](type-system.md)).
A process at type `T` exhibits specific barbs (observable actions). Two types
are distinct if and only if there exists a context where processes at those
types exhibit observably different behavior (bisimulation).

A **capability** is a name in the ρ-calculus — an unforgeable, passable,
quotable entity that a process can hold, send, receive, or restrict. The
possession of a name IS the authority to perform the actions that name enables
([type-system.md §5](type-system.md)).

A **capability type** is a composition of primitive types. It is not designed
upfront — it emerges from the interaction patterns of the processes that hold,
exercise, and verify the capability. The type encodes:

1. What primitive names the capability composes (its domain).
2. What barbs the composed capability exhibits (its interface).
3. What the verifier observes (the predicate result — nothing else).
4. What remains hidden (the witness w, the principal identity).

## 1. Primitive Types Available for Composition

Per [type-system.md §8.1](type-system.md), the following primitive types exist.
Each has a distinct behavioral position. No two may be unified.

| Primitive | Barbs | Scope | Role in Capability Construction |
|-----------|-------|-------|-------------------------------|
| `SecretKey` | `↓spend`, `↓derive` | ν-restricted | Proves knowledge of the name; authorizes action |
| `PublicKey` | `↓verify`, `↓encrypt` | Extrudable | Receives encrypted notes; verifies signatures |
| `Nullifier` | `↓nullify` | Public | Prevents replay; each exercise produces a fresh one |
| `Coin` | `↓commit` | Public | Represents value on-chain; the commitment face of a capability |
| `ContractId` | `↓dispatch` | Public | Routes a capability to the contract that recognizes it |
| `FuncId` | `↓gate` | Public | Constrains which function can exercise the capability |
| `TokenId` | `↓denominate` | Public | Identifies the asset type the capability controls |
| `MerkleNode` | `↓prove-inclusion` | Public | Proves the capability exists in the recognized set |

## 2. Capability Type Construction

A capability type is constructed by composition of primitive types. The
general form:

```
CapabilityType(action, resource) ≡ compose(primitives...)
```

Where `action` is what the holder can do (transfer, vote, bid, attest) and
`resource` is what the action applies to (token amount, proposal, tender lot,
credential schema).

The composition is NOT an arbitrary tuple. It SHALL satisfy:

1. **Name possession.** Every primitive name required for the action must be
   held by the process constructing the type (the wallet, during scan).

2. **Barb preservation.** Every primitive's barbs must be preserved across
   the composition. If `Nullifier` is erased to `[u8; 32]` at any boundary,
   the composition collapses — the capability engine cannot distinguish
   "this prevents replay" from "this is a byte buffer."

3. **Predicate language.** The composition defines a predicate language
   L_{r,s} = { w : P_{r,s}(w) = 1 }. The zero-knowledge proof that the
   holder knows `w` IS the capability exercise.

4. **Minimal disclosure.** The verifier observes only: the predicate result
   (1/0), the nullifier (ensuring single-use), and the commitment's inclusion
   proof. Nothing else.

### 2.1 Construction Example: Native Token Transfer

The capability "can transfer up to N native tokens" composes:

```
Capability(native_token_transfer, N) ≡ compose(
    SecretKey(↓spend, ν-restricted),     // "I know the spending key"
    Coin(↓commit),                       // "This commitment represents N value"
    Nullifier(↓nullify),                 // "I can prove I haven't spent it before"
    ContractId(↓dispatch),               // "This is a native token contract call"
    FuncId(↓gate),                       // "This is a transfer function (not burn, not fee)"
    TokenId(↓denominate),                // "This is the native token (not a wrapped asset)"
    MerkleNode(↓prove-inclusion)         // "This commitment is in the recognized set"
)
```

The predicate language L_{transfer, N} that this capability proves:
- The holder knows `w = (secret, coin_attributes, merkle_path)`.
- The commitment `C = poseidon_hash(pk.x, pk.y, value, token_id, spend_hook, user_data, blind)` matches the one in the Merkle tree.
- The value is ≤ N (the holder's balance).
- The nullifier `nf = poseidon_hash(secret, C)` has not been published before.

The verifier observes: predicate result = 1, nullifier = nf, Merkle root valid.
The verifier learns: nothing about the holder, the amount (beyond ≤ N), or which
specific coin was spent.

### 2.2 Construction Example: DAO Vote

The capability "can vote on proposal X" composes:

```
Capability(dao_vote, proposal_X) ≡ compose(
    SecretKey(↓spend, ν-restricted),     // "I know the voting key"
    Coin(↓commit),                       // "I hold governance tokens"
    Nullifier(↓nullify),                 // "I haven't voted on this proposal before"
    ContractId(↓dispatch),               // "This is the DAO contract"
    FuncId(↓gate),                       // "This is the vote function"
    TokenId(↓denominate),                // "This is the governance token"
    MerkleNode(↓prove-inclusion)         // "My tokens existed at snapshot time"
)
```

The predicate language differs from native token transfer: the token must be
the governance token, the function must be Vote, the nullifier must be scoped
to the proposal, and the snapshot Merkle root must match the proposal's
creation block. These are different behavioral positions — therefore different
capability types.

### 2.3 Construction Example: Tender Bid

The capability "can submit a sealed bid to tender Y" composes the DAO vote
capability as a SUB-CAPABILITY:

```
Capability(tender_bid, tender_Y) ≡ compose(
    Capability(identity_credential, qualified_contractor),  // ← emergent sub-type
    SecretKey(↓spend, ν-restricted),
    Nullifier(↓nullify),
    ContractId(↓dispatch),
    FuncId(↓gate),
    MerkleNode(↓prove-inclusion)
)
```

The `Capability(identity_credential, qualified_contractor)` is itself an
emergent type constructed from the Identity contract's primitives. The tender
capability composes it as a sub-capability — the holder must prove BOTH that
they hold the identity credential AND that they can submit the bid. This is
capability chaining: types compose, and the composition is itself a type.

## 3. The Authorization Inversion Theorem as Type Construction

The Authorization Inversion Theorem states ([type-system.md §6](type-system.md),
[ocap-original.md](ocap.md)):

> An ACL-based authorization system A(p, r, s) can be inverted to a
> privacy-preserving O-Cap scheme A'(π, r, s) if and only if there exists a
> ZK proof system for the language L_{r,s} = { w : P_{r,s}(w) = 1 } with
> proofs simulatable without knowledge of w.

Under the type system, this becomes a **type construction rule**:

**The type of a capability IS the predicate language it proves.**

```
CapabilityType(r, s) ≡ L_{r,s}
```

This is not a metaphor. The type is the set of witnesses that satisfy the
predicate. A value of this type is a proof that the holder knows such a
witness. The compiler enforces that only processes possessing the required
primitive names can construct a value of this type.

### 3.1 The ACL → O-Cap Mapping as Type Refinement

The theorem constructs a capability type from an ACL entry:

```
ACL entry: (p, r, s) ∈ L
    → witness: w_p (a secret bound to principal p)
    → predicate: P_{r,s}(w) = 1 iff w = w_p for some p authorized for (r, s)
    → language: L_{r,s} = { w_p : (p, r, s) ∈ L }
    → capability type: CapabilityType(r, s) ≡ L_{r,s}
```

The key insight: the predicate P_{r,s} depends ONLY on resource and action,
never on principal identity. The witness w is the secret that the principal
holds. The ZK proof demonstrates knowledge of w without revealing which w
(and therefore without revealing which principal).

## 4. Subtyping and Capability Refinement

From the composition structure, subtyping relationships emerge naturally:

**Attenuation.** A capability to transfer "up to N tokens" is a subtype of
a capability to transfer "up to M tokens" where M ≤ N (you can always spend
less than you have). The attenuated capability has a stronger predicate:
`value ≤ M` instead of `value ≤ N`.

**Delegation.** A capability can be delegated by re-encrypting the note to
the delegate's public key. The delegate receives the same primitive names
and can construct the same capability type. The type is unchanged; only
the holder changes.

**Composition.** Two capabilities can compose when their contract IDs and
function IDs are compatible. The composed type exhibits the union of their
barbs. The ZK proof must satisfy all sub-predicates.

## 5. The Two Modes as Type Refinement

The o-cap model has two realizations at different levels of the type hierarchy:

**Reference Mode (Agoric).** The capability IS an object reference. The type
is checked at runtime by the object system (`typeof invitation === Payment`).

**ZK Mode (DarkWow).** The capability IS a secret whose knowledge can be
proven in zero-knowledge. The type is the ZK circuit that verifies the
predicate.

These are the SAME model under bisimulation. Agoric's `Payment` type and
DarkWow's `NativeTokenTransfer` circuit both exhibit `↓spend`. The difference
is what the barb reveals:

| Aspect | Reference Mode | ZK Mode |
|--------|---------------|---------|
| Type representation | Object reference | ZK circuit (predicate language) |
| Authority proof | Pass the reference | Generate ZK proof |
| Replay prevention | Linear object (burn/deposit) | Nullifier |
| What verifier learns | Which object, amount, parties | Predicate result + nullifier only |
| Barbs exhibited | `↓spend(payment_id, amount, brand)` | `↓spend(π, nullifier)` |

The Authorization Inversion Theorem guarantees the conversion is bidirectional
(if-and-only-if). A ZK capability type SHALL be refinable to a plaintext
capability type, and vice versa, by adding or removing the zero-knowledge
wrapper. This is type refinement: the same behavioral position at different
levels of disclosure.

## 6. Capability Composition as a Calculus of Constructions

The capability grammar (Commitment → Nullifier → Proof → Revocation) is not
an arbitrary protocol — it is a **calculus of constructions** over the
primitive type system. Each operation in the lifecycle maps to a type-level
operation:

| Lifecycle Phase | Process Calculus | Type Construction |
|-----------------|-----------------|-------------------|
| **Issue** | `ν secret. commit!(poseidon_hash(secret, params))` | Create fresh name, output commitment |
| **Discover** | `block?(tx). aead_decrypt(note, secret).P` | Receive name via AEAD, construct capability type |
| **Hold** | Store secret + commitment + merkle_proof in wallet | Store typed CapRecord with all primitive names |
| **Exercise** | `ν nullifier. prove!(π, nullifier).P` | Generate ZK proof inhabiting the capability type |
| **Verify** | `verify?(π, nullifier). check(π).P` | Type-check: does proof inhabit L_{r,s}? |
| **Revoke** | `revoke!(nullifier).P` | Invalidate the type instance |

The wallet, as a capability engine, performs type construction at scan time:
it discovers commitments (receives names), resolves contracts (identifies
predicate languages), and constructs capability types from the composition
of primitives the contract declares.

The wallet SHALL NOT store a generic `cap_id: String`. It SHALL store a typed
composition whose structure is determined by the contract manifest and the
primitives discovered during AEAD scan. Every primitive name in the composition
has a known barb. The capability type tells the wallet what the user can DO.

The **Exercise**, **Verify**, and **Revoke** phases of this lifecycle — constructing and
authenticating the transaction that consumes a name — are specified as the wallet's write
path in [wallet.md §6](wallet.md), with the pending-transaction pool (the node-side
in-between) in [mempool.md](mempool.md).

## 7. The Manifest as Type Declaration

A contract's manifest ([manifest.md](manifest.md)) declares what capabilities
the contract recognizes and what predicates must be satisfied to exercise them.
This is a **type declaration** — the manifest tells the wallet what type
parameters are required for capability construction.

When the wallet parses a manifest, it learns:

- What primitive types the contract's capabilities compose (the `requires` fields).
- What actions are available (the `[[actions]]` section).
- What ZK circuits verify each action's predicates.

The manifest enables the wallet to construct capability types without
per-contract code. The manifest IS the type declaration. The wallet reads it
and constructs the type. No hardcoded contract list. No per-contract methods.

## 8. Toward a Calculus of Constructions

The emergent type patterns described in this document — primitive types
composing into capability types, attenuated subtypes, delegated instances,
cross-contract chaining — form a **calculus of constructions**. This calculus
can be formalized in a dependently-typed language (Lean4) where:

- **Types are terms.** Every primitive type and capability type is a term in
  the calculus.
- **Dependent types.** The capability type for "transfer up to N tokens"
  depends on the value N: `Capability(transfer, N: u64)`.
- **Propositions as types.** Proving knowledge of a witness = inhabiting a
  type. The ZK proof is a term of type `L_{r,s}`.
- **Bisimulation as propositional equality.** Two capability types are equal
  if and only if all processes at those types are bisimilar.

The executable Python model (Phase T.3 of the implementation plan) is the
discovery tool for this calculus. It models capability interactions as
processes, discovers types from behavior, and tests bisimulation. The Lean4
formalization (Phase T.4) proves that the calculus is sound — that every type
distinction is necessary (pareto-efficient), and that no unsound type can be
constructed from the primitives.

## 9. Formal Verification

The capability type constructions defined in this document are formalized
in the Lean4 calculus of constructions at `proofs/lean/src/DarkFi/Capability/`.

### 9.1 Construction Verifications

| Construction | § | Lean4 Module | Theorem | Status |
|-------------|---|-------------|---------|--------|
| Native token transfer | §2.1 | `Composition.lean` | `nativeTokenTransferType` | PROVED |
| DAO vote | §2.2 | `Composition.lean` | `daoVoteType` | PROVED |
| Tender bid | — | `Composition.lean` | `tenderBidType` | PROVED |

Each construction's `coversBarbs` proof is verified by case analysis: every
required barb is shown to be present in the composition of its constituent
primitives. The composed barb sets are:

- Native token transfer: `{↓spend, ↓derive, ↓commit, ↓nullify, ↓dispatch, ↓gate, ↓denominate, ↓proveInclusion}`
- DAO vote: `{↓spend, ↓derive, ↓commit, ↓nullify, ↓dispatch, ↓gate, ↓denominate, ↓proveInclusion}`
- Tender bid: `{↓spend, ↓derive, ↓commit, ↓nullify, ↓dispatch, ↓gate, ↓denominate, ↓proveInclusion}` (plus `↓prove` for identity credential sub-capability)

The DAO vote and native token transfer have identical composed barb sets
but are distinguished by their Resources (different `requiredBarbs`) and
Actions (different names). This is type-level bisimulation: same structure,
different behavioral positions.

### 9.2 Capability Type Existence

`proofs/lean/src/DarkFi/Capability/Inversion.lean`:

- `nativeTokenTransferExists`: The native token transfer capability type
  is inhabited (constructively).
- `daoVoteExists`: The DAO vote capability type is inhabited.
- `tenderBidExists`: The tender bid capability type is inhabited.

### 9.3 Wallet Constructibility

`proofs/lean/src/DarkFi/Capability/Wallet.lean`:

- `nativeTokenTransfer_constructible`: The wallet can construct the native
  token transfer type from its primitives.
- `daoVote_constructible`: The wallet can construct the DAO vote type.
- `tenderBid_constructible`: The wallet can construct the tender bid type.

These proofs confirm that the wallet's type construction (§7 of wallet.md)
is sound: given the primitives discovered via AEAD scan and the manifest
read from chain, the wallet produces a valid capability type in the calculus.

### 9.4 Lean4 Source

All formal verification modules:
- `proofs/lean/src/DarkFi/Capability/Types.lean` — 14 barbs, 13 types
- `proofs/lean/src/DarkFi/Capability/Composition.lean` — composition + constructions
- `proofs/lean/src/DarkFi/Capability/Pareto.lean` — pareto-efficiency
- `proofs/lean/src/DarkFi/Capability/Distinction.lean` — non-unifiable pairs
- `proofs/lean/src/DarkFi/Capability/Inversion.lean` — Authorization Inversion
- `proofs/lean/src/DarkFi/Capability/Wallet.lean` — wallet soundness

Run `lake build` in `proofs/lean/` to type-check all modules.

## 10. References

- **[Type System Specification](type-system.md)** — Primitive types, behavioral positions, compiler-enforced invariants.
- **[Wallet Architecture](wallet.md)** — Wallet as pure function, manifest-first design, scan paths.
- **[Manifest System](manifest.md)** — Contract interface declaration, capability grammar.
- Meredith, L.G. and Radestock, M. (2005). "A Reflective Higher-Order Calculus." *ENTCS*.
- Miller, M.S. (2006). *Robust Composition.* PhD dissertation, Johns Hopkins University.
- "The Zero-Knowledge Authorization Inversion Theorem" — [technologytruth.substack.com](https://technologytruth.substack.com/p/the-zero-knowledge-authorization)
