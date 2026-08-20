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
Each has a distinct behavioral position. No two SHALL be unified.

| Primitive | Barbs | Scope | Role in Capability Construction |
|-----------|-------|-------|-------------------------------|
| `SecretKey` | `↓spend`, `↓derive` | ν-restricted | Proves knowledge of the name; authorizes action |
| `PublicKey` | `↓verify`, `↓encrypt` | Extrudable | Receives encrypted notes; verifies signatures |
| `Nullifier` | `↓nullify` | Public | Prevents replay; each exercise produces a fresh one |
| `Commitment` | `↓commit` | Public | The commitment face of a capability |
| `ContractId` | `↓dispatch` | Public | Routes a capability to the contract that recognizes it |
| `FuncId` | `↓gate` | Public | Constrains which function can exercise the capability |
| `AssetId` | `↓denominate` | Public | The asset denomination face |
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
    SecretKey(↓spend, ν-restricted),     // Authorization secret — possession proves authority to exercise
    Commitment(↓commit),                 // Public face — binds parameters (quantity N, recipient, asset class) without revealing them
    Nullifier(↓nullify),                 // Single-exercise guard — each exercise SHALL produce a unique nullifier
    ContractId(↓dispatch),               // Routes exercise to the native token contract
    FuncId(↓gate),                       // Constrains exercise to the transfer function
    AssetId(↓denominate),                // Denominates the capability in the native asset class (DRKW)
    MerkleNode(↓prove-inclusion)         // Proves the commitment is included in the recognized set
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
    SecretKey(↓spend, ν-restricted),     // Authorization secret — possession proves authority to vote
    Commitment(↓commit),                 // Public face — binds the governance stake without revealing the holder
    Nullifier(↓nullify),                 // Single-exercise guard — prevents voting twice on the same proposal
    ContractId(↓dispatch),               // Routes exercise to the DAO contract
    FuncId(↓gate),                       // Constrains exercise to the vote function
    AssetId(↓denominate),                // Denominates the capability in the governance asset class
    MerkleNode(↓prove-inclusion)         // Proves the governance stake existed at the snapshot root
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

The Authorization Inversion Theorem states ([type-system.md §6](type-system.md)):

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

### 5.1 Defined Privilege Containment

The capability type system proves **defined privilege containment**, not
least privilege. This is a deliberate trade-off for privacy.

**What IS Proved**

1. **Coverage** (soundness): `requiredBarbs ⊆ compose(primitives)`. The
   capability holder possesses AT LEAST the barbs the action requires. No
   action can execute with fewer barbs than it declares.

2. **Barb preservation** (monotonicity): `compose` is a union — it cannot
   manufacture a barb no primitive carries. Every composed barb traces to
   a specific primitive the holder possesses.

3. **Containment**: The blast radius of any capability is bounded by its
   declared primitives. An action requiring `[Commit, Dispatch, Gate]`
   CANNOT exercise `Spend` or `Nullify` unless those barbs are present
   in the composed primitives. This prevents whole classes of authority-
   escalation exploits.

4. **Pareto-efficiency**: Every primitive has a distinct barb set; no two
   primitives are interchangeable. You cannot substitute one primitive for
   another without changing the composed barb set.

**What Is NOT Proved**

1. **Least privilege** (completeness): The system does NOT prove
   `compose(primitives) ⊆ requiredBarbs` — that the composed barb set is
   exactly what the action needs and nothing more. A capability MAY carry
   barbs beyond those the action requires.

2. **Compositional minimality**: Given `N` primitives covering required
   barbs, there MAY exist a strict subset that also covers them. The
   system does not enforce that you use the smallest possible set.

**Why This Trade-Off?**

In a **reference-mode o-cap system** (Agoric), least authority is enforced
by the object system: an object reference IS the exact authority to invoke
specific methods. You cannot hold "extra" authority because the reference
is typed and unforgeable.

In a **ZK-mode o-cap system** (DarkWow), privacy requires that the
verifier learns nothing about the holder beyond the predicate result.
The predicate P_{r,s} depends ONLY on resource and action, never on
principal identity. This means the system cannot distinguish "holder has
exactly barbs {X, Y}" from "holder has barbs {X, Y, Z}" — both produce
identical proofs. The ZK wrapper hides the extra barbs.

**If proven least authority is your primary concern, Agoric's reference
mode provides it. If privacy is your primary concern — the verifier learns
only that the predicate was satisfied, not who satisfied it or what else
they could do — DarkWow's ZK mode provides it.** The two modes are
bisimulation-equivalent (§5) but optimize for different properties.

**What This Means in Practice**

The containment is still meaningful:
- A multisig approval capability carrying `{Spend}` cannot exercise that
  `Spend` unless the action's `required_barbs` include `Spend`.
- Authority escalation requires collusion between a contract that declares
  broad `required_barbs` AND a capability that carries extra barbs.
- The coverage gate (`wallet_construct` → `resolve_capability_type`)
  enforces this at runtime: uncovered compositions drop the note.
- Every barb traces to a declared primitive; no barb appears from nowhere.

## 6. Capability Lifecycle: Generic Grammar and DarkWow Instantiation

Every object-capability system implements the same lifecycle. The capability
is created, discovered, held, exercised, verified, and consumed. This section
defines the lifecycle in two layers: the generic grammar (applicable to any
o-cap system) and the DarkWow instantiation (ZK mode).

### 6.1 Generic Capability Grammar

The grammar applies to all object-capability systems. Each phase SHALL be
implemented by every DarkWow contract that exercises capabilities.

| Phase | Generic Operation | Type Construction |
|-------|------------------|-------------------|
| **Create** | `ν name. publish!(public_face(name, params))` | Create a fresh name. Publish its public face — a binding commitment to the name and its parameters that reveals neither the name nor the parameters. |
| **Discover** | `transport?(name). receive(name).P` | Receive a capability name through the name transport layer. The receiving process now possesses the name and SHALL construct its type. |
| **Hold** | Store `(name, public_face, inclusion_proof)` | Store a typed record containing every primitive name the capability composes. Each primitive name has a known barb set. The type tells the holder what actions are authorized. |
| **Exercise** | `generate_proof(name, predicate). publish!(consumption_evidence).P` | Generate proof that the holder knows a witness satisfying the capability's predicate language L_{r,s}. Publish consumption evidence that marks this instance as exercised. |
| **Verify** | `verify?(proof, consumption_evidence, predicate).P` | Verify that the proof inhabits the capability type's predicate language and that the consumption evidence is valid and has not been previously published. |
| **Consume** | `consume!(consumption_evidence).P` | Record that the name instance has been exercised. The consumption evidence SHALL prevent re-exercise. Once consumed, the name SHALL NOT authorize further actions. |

### 6.2 DarkWow Instantiation (ZK Mode)

DarkWow instantiates the generic grammar using zero-knowledge proofs over
Halo2 PLONK circuits, AEAD-encrypted note discovery, and nullifier-based
consumption evidence.

| Phase | DarkWow Mechanism |
|-------|------------------|
| **Create** | `poseidon_hash(secret, commitment_parameters)` → Commitment. The commitment is a Pedersen-like hash binding the capability's primitive names without revealing them. |
| **Discover (L1)** | Trajectory identification ([contract-wasm-type-system.md §C.8](contract-wasm-type-system.md)): trial AEAD decryption over new Merkle leaves; the note carries the primitive attributes plus `nullifier`, `merkle_root`, `leaf_position`, `commitment`; the wallet matches the nullifier to a consumed object and records the new leaf. |
| **Discover (L2)** | Flat note discovery ([contract-wasm-type-system.md §B.8](contract-wasm-type-system.md)): trial AEAD decryption over `ContractCall.data`; the note carries capability-identifying fields (`amount`, `token_id`, `owner_commit`) only; no trajectory, no Merkle leaf. |
| **Hold** | `CapRecord` stored in SQLite `held_capabilities` with Merkle inclusion proof in `capability_proofs`. The record carries the typed composition — primitives and barbs — constructed by `wallet_construct`. |
| **Exercise** | Halo2 `Proof::create` over a `ZkCircuit` whose witness is the capability's private names and whose public inputs are the commitment, nullifier, and Merkle root. |
| **Verify** | `Proof::verify` against the circuit's public inputs. The verifier (node, mempool) checks the proof without learning the witness. |
| **Consume** | Nullifier published in `Transaction.nullifiers`. The nullifier is `poseidon_hash(secret, commitment)` — unique per capability instance. A nullifier appearing on-chain SHALL prevent any future exercise of that instance. |

### 6.3 The Wallet as Type Construction Engine

The wallet, as a capability engine, performs type construction at scan time:
it discovers commitments (receives names), resolves contracts via their
stored manifests (identifies predicate languages), and constructs capability
types from the composition of primitives the contract declares. Which discovery
behavior it applies — L1 trajectory identification or L2 flat note discovery —
is selected by the contract's manifest `note_schema` (§6.2), never hardcoded in
the wallet ([wallet.md §2.3](wallet.md)).

The wallet SHALL NOT store a generic `cap_id: String`. It SHALL store a typed
composition — every primitive name in the composition has a known barb. The
capability type tells the wallet what actions the holder is authorized to
perform.

The **Exercise**, **Verify**, and **Consume** phases of this lifecycle —
constructing and authenticating the transaction that exercises a name — are
specified as the wallet's write path in [wallet.md §6](wallet.md), with the
pending-transaction pool (the node-side in-between) in [mempool.md](mempool.md).

## 7. The Manifest as Type Declaration

A contract's manifest ([manifest.md](manifest.md)) declares what capabilities
the contract recognizes and what predicates SHALL be satisfied to exercise
them. This is a **type declaration** — the manifest tells the wallet what
type parameters are required for capability construction.

When the wallet parses a manifest, it learns:

- What primitive types each capability composes (the `primitives` field on `[[capabilities]]`).
- What actions are available (the `[[actions]]` section, each referencing a declared function).
- What ZK circuits verify each action's predicates (the `proof_circuit` field on `[[functions]]`).

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
| Native token coinbase | — | `Composition.lean` | `nativeTokenCoinbaseType` | PROVED |
| DAO vote | §2.2 | `Composition.lean` | `daoVoteType` | PROVED |
| Tender bid | §2.3 | `Composition.lean` | `tenderBidType` | PROVED |
| Purse balance | — | `Composition.lean` | `purseBalanceType` | PROVED |
| Purse withdrawal | — | `Composition.lean` | `purseWithdrawType` | PROVED |
| Identity credential | — | `Composition.lean` | `identityCredentialType` | PROVED |
| Box capability | — | `Composition.lean` | `boxCapType` | PROVED |
| Multisig approval | — | `Composition.lean` | `multisigApprovalType` | PROVED |
| Attestation | — | `Composition.lean` | `attestationType` | PROVED |
| Bridge deposit | — | `Composition.lean` | `bridgeDepositType` | PROVED |
| Bridge withdrawal | — | `Composition.lean` | `bridgeWithdrawType` | PROVED |

Each construction's `coversBarbs` proof is verified by case analysis: every
required barb is shown to be present in the composition of its constituent
primitives. The `dleqProof` primitive (carrying `{↓prove}`) covers the
`↓prove` requirement in `tenderBidType` and `bridgeWithdrawType`. Compositions
that share the same primitive set (e.g., native token transfer and DAO vote)
are distinguished by their Actions and by which subset of the composed barbs
each Action declares as required.

### 9.2 Capability Type Existence

`proofs/lean/src/DarkFi/Capability/Inversion.lean`:

All 12 capability types are proved inhabited (constructively). The
`authorizationInversion_TypeLevel` theorem states the bidirectional:
a capability type exists iff there exist primitives whose composition
covers the resource's required barbs.

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
