# DarkWow Type System

This document defines the DarkWow type system. It is the specification
to which all implementation shall conform. It uses SHALL, MUST, SHALL NOT,
MUST NOT per RFC 2119.

## 0. Foundational Calculus

The type system derives from the **ρ-calculus** — the reflective higher-order
π-calculus. The ρ-calculus extends the π-calculus with one property: names are
processes and processes are names. Names can be quoted, inspected as data, and
passed as messages. This reflective property is what makes the calculus suitable
for cryptographic capabilities: a capability IS a name, and that name can be
passed, restricted, and observed.

The primitive operations:

| Operation | Notation | Meaning |
|-----------|----------|---------|
| Inaction | `0` | The stopped process |
| Output | `x!(y)` | Send name `y` on channel `x` |
| Input | `x?(y).P` | Receive name `y` on channel `x`, then behave as `P` |
| Restriction | `νx.P` | Create fresh name `x` with scope `P` |
| Replication | `!P` | Replicate `P` arbitrarily many times |
| Reflection | `quote(x)` | Treat name `x` as data |
| Dereference | `eval(x)` | Treat data `x` as a name |

In the blockchain context:
- A **channel** is a contract instance (sled tree + WASM entrypoint).
- A **name** is a capability (a secret key whose possession authorizes action).
- **Output** is posting a commitment (placing a name's public face on-chain).
- **Input** is discovering a commitment via AEAD decryption (receiving a name).
- **Restriction** is deriving a per-instance key (scoping a name to a contract).
- **Replication** is the nullifier SMT (a name consumed exactly once; replication
  models the infinite supply of fresh names).

## 1. Definition of a Type

**A type is a behavioral position in a concurrent interaction graph.**

A type `T` constrains three things for any process `P` typed at `T`:

1. **Domain** — what names `P` can hold.
2. **Barbed interface** — what actions `P` can observe and perform.
3. **Scope mobility** — what names `P` can extrude beyond its declared boundary.

```
Γ ⊢ P : T
```

means: in naming context `Γ`, process `P` occupies behavioral position `T`.

### 1.1 Barbs

A **barb** is an observable action. In the ρ-calculus, process `P` exhibits
barb `↓x` if `P` can engage in input or output on channel `x`.

Every type SHALL define the barbs that processes at that type may exhibit.
No type SHALL exhibit a barb that its definition does not declare.

| Barb | Meaning |
|------|---------|
| `↓spend` | Can authorize a value transfer (possesses SpendingKey) |
| `↓view` | Can decrypt notes (possesses ViewingKey) |
| `↓nullify` | Can prevent replay (produces a Nullifier) |
| `↓commit` | Can create a capability (produces a Commitment) |
| `↓prove` | Can satisfy a ZK predicate (generates a Proof) |
| `↓verify` | Can check a ZK proof or signature (validates a Proof) |
| `↓dispatch` | Can route a call (identifies a contract) |
| `↓gate` | Can authorize a spend hook (identifies a function) |
| `↓denominate` | Can identify an asset type (identifies a token) |
| `↓prove-inclusion` | Can prove membership in a set (Merkle proof) |
| `↓encrypt` | Can produce ciphertext for a recipient (DH key agreement) |
| `↓derive` | Can produce a scoped sub-key (per-instance derivation) |
| `↓discover` | Can detect own outputs (AEAD decryption) |
| `↓mine` | Can produce a valid coinbase (possesses MiningRecipient) |

### 1.2 Bisimulation

Two processes `P` and `Q` are bisimilar (`P ∼ Q`) if an observer cannot
distinguish them through interaction. For every action `P` can take, `Q`
can take a matching action leading to bisimilar states, and vice versa.

## 2. Type Distinction Principle

**Two types SHALL NOT be unified if there exists any context where a process
holding a name of type T₁ exhibits observably different behavior from a process
holding a name of type T₂.**

If a process at type `T₁` exhibits barb `↓x` that no process at type `T₂` can
match, the types MUST remain distinct. The compiler MUST reject any attempt to
use a value of type `T₁` where type `T₂` is expected.

### 2.1 Cryptographic Types Are Nominal

Every cryptographic capability SHALL be a distinct nominal type. The compiler
SHALL NOT accept a `Nullifier` where a `SecretKey` is required. The compiler
SHALL NOT accept `[u8; 32]` where a `Nullifier` is required. The behavioral
positions are provably different under bisimulation:

- `SecretKey` exhibits `↓spend` and `↓derive`. `[u8; 32]` exhibits neither.
- `Nullifier` exhibits `↓nullify`. `[u8; 32]` exhibits no barbs.
- `Coin` exhibits `↓commit`. `[u8; 32]` exhibits no barbs.
- `PublicKey` exhibits `↓verify` and `↓encrypt`. `pallas::Point` exhibits neither.
- `ContractId` exhibits `↓dispatch`. `[u8; 32]` exhibits no barbs.

### 2.2 Bytes Round-Trip Is Forbidden

No type SHALL be converted to `[u8; 32]` and back across a module boundary.
The intermediate `[u8; 32]` has no behavioral constraints — any process can
produce any 32 bytes. This erases the type distinction and SHALL NOT compile.

The correct path is: construct the typed value directly and pass it across
the boundary as itself. The constructor SHALL validate the input. No `From`
impl SHALL bypass validation.

Conversion to bytes is permitted ONLY at persistence boundaries (sled, SQLite).
The conversion SHALL use `Type::from_bytes()` which SHALL validate. Reading
back from persistence SHALL validate through `Type::from_bytes()`. No code
path SHALL construct a type by directly accessing a `pub` field.

## 3. Generic Types and Capabilities

A generic parameter `T` abstracts over the behavioral position of a name. This
abstraction is permitted ONLY when all three conditions hold:

**(a)** The function's behavior does NOT depend on the specific barbs of `T`.

**(b)** `T` does not cross a restriction boundary (ν-scope). A name created
by restriction SHALL NOT be extruded through a generic interface that erases
its scope.

**(c)** `T` is not a cryptographic capability. Capabilities have distinct
security semantics; a generic interface that accepts any capability erases
the distinction between `↓spend`, `↓nullify`, and `↓prove`.

ANY function that accepts `impl AsRef<[u8]>` and is callable with a
`SecretKey`, `Nullifier`, or `Coin` SHALL NOT compile. The trait bound
erases the barb. The behavioral position is lost.

## 4. Error Types

Every error variant IS a barb of the system. When a process can fail in ways
that demand different responses from its containing context, those failures
MUST be distinct types.

| Error Barb | Observable By | Context Response |
|------------|---------------|------------------|
| `↓bad-nullifier` | Mempool, Chain | Reject transaction |
| `↓double-spend` | Chain | Block is invalid |
| `↓bad-proof` | Contract VM | Reject call |
| `↓bad-derive` | Wallet | Skip note, do not crash |
| `↓db-fail` | Infrastructure | Fatal — restart |

These barbs SHALL NOT be unified. A `↓double-spend` failure requires
block-level rejection. A `↓bad-derive` failure requires note-level skipping.
Unifying them under a single error type erases the behavioral distinction —
the caller cannot distinguish "consensus failure" from "this note is not mine."

No function SHALL discard an error silently. `unwrap_or_default()` SHALL NOT
appear in any cryptographic path. `.ok()` chains that discard the error reason
SHALL NOT appear in any cryptographic path. Every `Result` SHALL be propagated
to a context that can respond appropriately.

## 5. Authority

**A process SHALL perform action A if and only if it possesses the name for A.**

The function signature SHALL require the capability type as a parameter.
No ambient authority exists. There are no global admin keys, no upgrade
proxies, no `owner` addresses. Authority flows ONLY through explicit name
passing at the type level.

A function that takes no `SecretKey` parameter SHALL NOT sign. A function
that takes no `Nullifier` parameter SHALL NOT check replay. A function whose
signature accepts `[u8; 32]` instead of `OwnedSecretKey` SHALL NOT authorize
mining — the compiler SHALL reject it because `[u8; 32]` is not a capability.

## 6. The Capability Engine: Emergent Types from Sound Primitives

The Authorization Inversion Theorem establishes:

> An ACL-based authorization system A(p, r, s) can be inverted to a
> privacy-preserving O-Cap scheme A'(π, r, s) if and only if there exists a
> ZK proof system for the language L_{r,s} = { w : P_{r,s}(w) = 1 } with
> proofs simulatable without knowledge of w.

Under the ρ-calculus, this becomes a type-level requirement:

**The type of a capability IS the predicate language it proves.**

```
CapabilityType(r, s) ≡ L_{r,s}
```

Where `L_{r,s}` is the ZK proof language for predicate `P_{r,s}` over resource
`r` and action `s`. The capability type encodes:

- What must be proven (the predicate `P_{r,s}`).
- What the verifier observes (the barb `↓prove`).
- What is hidden (the witness `w`).

### 6.1 Capability Types Are Emergent

A capability type — "can transfer up to 100 native tokens," "can vote on
proposal X," "can submit a sealed bid to tender Y" — is not a primitive.
It is constructed by composition of primitive types:

```
Capability(can_transfer_100_native_tokens) ≡
    compose(
        Nullifier(↓nullify),
        Coin(↓commit),
        TokenId(↓denominate),
        FuncId(↓gate),
        ContractId(↓dispatch),
        SecretKey(↓spend, ν-restricted)
    )
```

The wallet, as a capability engine, constructs these emergent types at scan
time: it discovers a commitment via AEAD decryption, resolves the contract
via its manifest, and derives the capability's type from the composition of
the primitives the contract declares. The wallet never stores a generic
`cap_id: String` — it stores a typed composition.

### 6.2 Primitive Soundness Is a Prerequisite

The construction in §6.1 is mathematically sound IF AND ONLY IF every
primitive type preserves its barbs across every module boundary.

If `Nullifier` is unified with `[u8; 32]` at any boundary, the composition
collapses. The wallet cannot determine whether a given 32-byte value is a
`Nullifier` (exhibiting `↓nullify`, preventing replay), a `Coin` (exhibiting
`↓commit`, representing value), or an opaque byte buffer (exhibiting no barbs).
All three are behaviorally distinct under bisimulation (§2). Unifying them
under `[u8; 32]` makes all three indistinguishable.

Strict type boundaries are not a preference. They are the minimum viable
foundation for the capability engine. Without them, emergent capability
types cannot be constructed — because the primitive types they compose from
have had their barbs erased.

### 6.3 The Two Modes

The O-Cap model has two realizations:

- **Reference Mode (Agoric):** The capability IS an object reference. The type
  is checked at runtime by the object system.
- **ZK Mode (DarkWow):** The capability IS a secret whose knowledge can be
  proven in zero-knowledge. The type is the ZK circuit that verifies the
  predicate.

Under bisimulation, these are the SAME model. Agoric's `Payment` type and
DarkWow's `NativeTokenTransfer` circuit both exhibit `↓spend`. The difference
is what the barb reveals: Agoric reveals the payment identity, amount, and
brand; DarkWow reveals only the predicate result and nullifier.

The Authorization Inversion Theorem guarantees conversion is bidirectional.
The type system SHALL preserve this: a ZK capability type SHALL be refinable
to a plaintext capability type, and vice versa, by adding or removing the
zero-knowledge wrapper.

## 7. Compiler-Enforced Invariants

Every program that compiles SHALL satisfy these five invariants:

1. **Name possession.** No name shall be used without being received or
   created. Authority is explicit in the type signature.

2. **Type distinction.** No two distinct behavioral positions shall be
   unified under a single type. `Nullifier` SHALL NOT be `[u8; 32]`.
   `SecretKey` SHALL NOT be `AsRef<[u8]>`.

3. **Scope restriction.** No restricted name shall cross its declared
   scope boundary. A `SecretKey` derived for contract instance `A` SHALL NOT
   be usable in contract instance `B`.

4. **Error barb distinguishability.** All error conditions that demand
   different context responses shall be different types. The caller SHALL
   be able to match on which failure occurred.

5. **Authority-through-possession.** Authority to perform cryptographic
   operations SHALL be represented by possession of the corresponding
   cryptographic key type. No ambient authority.

## 8. Type Namespace

Every type in the DarkWow type system, its inner representation, the barbs
it exhibits, its scope, and its construction rules.

### 8.1 Cryptographic Primitive Types

| Type | Inner | Barbs | Scope | Construction |
|------|-------|-------|-------|-------------|
| `SecretKey` | `pallas::Base` | `↓spend`, `↓derive` | ν-restricted to holder | `from_bytes` (validates), `derive_instance` (binds to contract+instance) |
| `PublicKey` | `pallas::Point` | `↓verify`, `↓encrypt` | Extrudable | `from_secret`, `from_bytes` (rejects identity) |
| `Nullifier` | `pallas::Base` | `↓nullify` | Public | `new(secret, coin_hash)` only. `from_bytes` SHALL reject zero. |
| `Coin` | `pallas::Base` | `↓commit` | Public | `from_attributes(pk, value, token_id, spend_hook, user_data, blind)` |
| `ContractId` | `pallas::Base` | `↓dispatch` | Public | `derive(deploy_key)` or well-known constant |
| `TokenId` | `pallas::Base` | `↓denominate` | Public | `derive(auth_parent, user_data, blind)` or well-known constant |
| `FuncId` | `pallas::Base` | `↓gate` | Public | `from(contract_id, func_code)` |
| `MerkleNode` | `pallas::Base` | `↓prove-inclusion` | Public | Tree insertion |

### 8.2 Structural Types

| Type | Composition | Barbs |
|------|------------|-------|
| `Transaction` | `{ calls, proofs, signatures, nullifiers: Vec<Nullifier> }` | `↓process` |
| `ContractCall` | `{ contract_id: ContractId, data: Vec<u8> }` | `↓invoke` |
| `CoinbaseTransaction` | `{ proof, public_inputs, coin: Coin, nullifier: Nullifier, encrypted_note }` | `↓mine` |
| `BlockHeader` | `{ merkle_root, previous, height, ... }` — all merkle roots SHALL be `blake3::Hash` | `↓validate-pow` |
| `AeadEncryptedNote` | `{ ciphertext, ephem_public: PublicKey }` | `↓discover` |

### 8.3 Authority Types

| Type | Inner | Barbs | Construction |
|------|-------|-------|-------------|
| `OwnedSecretKey` | `SecretKey` | `↓spend` (only if declared) | `from_declared_bytes`. No `::random()`. No `From<SecretKey>`. |
| `MiningRecipient` | `PublicKey` + `OwnedSecretKey` | `↓mine` | `from_account`. No `From<PublicKey>`. |
| `AccountManager` | `Vec<Account>` | `↓identity` | `open(keys_path, network, profile)` |

### 8.4 Non-Unifiable Pairs

These pairs SHALL NOT be unified under any generic interface, trait bound,
`From` impl, `Deref` impl, or type alias. The compiler SHALL reject any
code that treats the left type as the right type.

| Type | SHALL NOT be treated as | Reason |
|------|------------------------|--------|
| `Nullifier` | `[u8; 32]` | `↓nullify` ≠ no barbs |
| `Nullifier` | `IntentNullifier` | Different predicate languages |
| `Coin` | `[u8; 32]` | `↓commit` ≠ no barbs |
| `SecretKey` | `[u8; 32]` | `↓spend`, `↓derive` ≠ no barbs |
| `SecretKey` | `pallas::Base` | One barbs, one does not |
| `PublicKey` | `pallas::Point` | One validates identity, one does not |
| `ContractId` | `[u8; 32]` | `↓dispatch` ≠ no barbs |
| `FuncId` | `pallas::Base` | `↓gate` ≠ no barbs |
| `TokenId` | `pallas::Base` | `↓denominate` ≠ no barbs |
| `OwnedSecretKey` | `SecretKey` | `↓spend` requires declaration; `SecretKey` may be random |

### 8.5 Shared Derives

Every newtype over `pallas::Base` in §8.1 SHALL derive:

```
Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable
```

`ContractId` and `MerkleNode` SHALL additionally derive `Ord, PartialOrd`.
`Nullifier` SHALL additionally derive `Ord, PartialOrd`.

No type in §8.1 SHALL derive `Hash`, `Default`, or `From<pallas::Base>`.
The `From<pallas::Base>` impl erases the type distinction — any field element
could become any capability. Construction SHALL use named constructors that
enforce validation (zero-rejection, canonical encoding, identity rejection).

Serialization for chain persistence (serde `Serialize`/`Deserialize`) SHALL
be implemented manually via `to_bytes()`/`from_bytes()` for each type. No
type SHALL derive serde directly — `pallas::Base` does not implement serde.

## 10. Verified Properties

The type system defined in this document is formalized in the Lean4 calculus
of constructions at `proofs/lean/src/DarkFi/Capability/`. The following
theorems are proved or stated with explicit verification status.

### 10.1 Pareto-Efficiency of the Primitive Type Namespace

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Pareto.lean`

`primitiveTypesAreParetoEfficient`: All 12 primitive types have pairwise
distinct barb sets. No type distinction can be removed without losing
behavioral information. Proof: `dec_trivial` over the finite list of
`Finset Barb` values.

15 named pair-distinction theorems provide human-readable cross-references
for each pair in §8.1 and §8.3 (e.g., `secretKey_distinct_from_nullifier`,
`ownedSecretKey_distinct_from_miningRecipient`).

`barbEqualityImpliesTypeEquality`: If two primitive types have identical
barb sets, they are the same type. This is the contrapositive of
pareto-efficiency — no accidental unification is possible.

### 10.2 Non-Unifiable Pairs

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Distinction.lean`

All 10 pairs in §8.4 are proved distinct (`native_decide`). The conjunction
`allUnifiablePairsProved` bundles them for single-reference verification:
Nullifier ≠ [u8; 32], Coin ≠ [u8; 32], SecretKey ≠ [u8; 32], ContractId ≠
[u8; 32], PublicKey ≠ pallas::Point, SecretKey ≠ pallas::Base, FuncId ≠
pallas::Base, TokenId ≠ pallas::Base, Nullifier ≠ IntentNullifier,
OwnedSecretKey ≠ SecretKey.

### 10.3 Barb Preservation Under Composition

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Composition.lean`

`barbPreservation`: If a primitive type `p` is in the composition list, then
every barb of `p` is in the composed barb set. Proof: structural induction
on the primitive list. This guarantees that composing capability types does
not erase barbs — the fundamental requirement for emergent type construction.

### 10.4 Authorization Inversion (Type-Level)

**Status:** PROVED (type-level). `proofs/lean/src/DarkFi/Capability/Inversion.lean`

`authorizationInversion_TypeLevel`: For every resource `r` and action `s`,
there exists a capability type `CapabilityType r s` iff there exists a list
of primitives whose composition covers `r.requiredBarbs`. Proof: iff
construction (both directions).

The ZK soundness bridge is stated as `circuitSoundnessBridge`: if a circuit
exists for `(r, s)` whose `constrain_instance` calls cover the required
barbs, then the capability type is inhabited. This is an axiom referencing
the manual circuit audit in `proofs/lean/src/DarkFi/Circuits/` (120 circuits,
all `constrain_instance` calls verified for instance-derivation binding).

`capabilityPredicateBypass_prevention`: A capability requiring `↓prove`
MUST have that barb covered by its composition. This closes HAZOP Pattern 4
("capability predicate result is free witness; provenance unverified").

### 10.5 Wallet Type Construction Soundness and Completeness

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Wallet.lean`

`walletConstruct_sound`: If `walletConstruct` returns a capability type, the
required barbs are covered by the composed primitives.

`walletConstruct_complete`: If a `CapabilityType` exists for primitives `p`
and resource `r`, then `walletConstruct p r` returns `some` (not `none`).

`walletConstruct_preservesPrimitives`: The primitives returned are exactly
the primitives passed in — no loss, no modification.

Three concrete constructibility proofs verify that native token transfer,
DAO vote, and tender bid capability types are constructible from their
respective primitive lists.

### 10.6 Full ZK Proof System Model

**Status:** FUTURE WORK. Not yet formalized.

The type-level Authorization Inversion is proved. The full ZK proof system
model (Halo2 constraint semantics, polynomial commitments, Fiat-Shamir
transform) in Lean4 is future work. When complete, `circuitSoundnessBridge`
will be replaced with a proved theorem referencing the Halo2 formalization.

## 11. References

- Meredith, L.G. and Radestock, M. (2005). "A Reflective Higher-Order Calculus."
  *Electronic Notes in Theoretical Computer Science*, 141(5), 49-67.
- Milner, R. (1999). *Communicating and Mobile Systems: the π-Calculus.*
  Cambridge University Press.
- Miller, M.S. (2006). *Robust Composition: Towards a Unified Approach to Access
  Control and Concurrency Control.* PhD dissertation, Johns Hopkins University.
- "The Zero-Knowledge Authorization Inversion Theorem" —
  [technologytruth.substack.com/p/the-zero-knowledge-authorization](https://technologytruth.substack.com/p/the-zero-knowledge-authorization)
- Sangiorgi, D. and Walker, D. (2001). *The π-Calculus: A Theory of Mobile
  Processes.* Cambridge University Press.
- Bradner, S. (1997). "Key words for use in RFCs to Indicate Requirement
  Levels." RFC 2119.
