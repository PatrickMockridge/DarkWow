# Type System: First-Principles Derivation

This document defines the DarkWow type system from first principles. It is the
foundational specification to which all type decisions in the implementation plan
must trace. Every type fracture identified in the codebase is, at root, a violation
of one of the principles derived here.

## 0. Foundational Calculus

The type system is derived from the **ρ-calculus** — the reflective higher-order
π-calculus (Milner 1991, Meredith/Radestock 2005). The ρ-calculus extends the
π-calculus with one additional property: **names themselves are processes, and
processes are names.** Names can be quoted, inspected as data, and passed as
messages. This reflective property is what makes the calculus suitable for modeling
cryptographic capabilities: a capability IS a name, and that name can be passed,
restricted, and observed.

The primitive operations:

| Operation | Notation | Meaning |
|-----------|----------|---------|
| Inaction | `0` | The stopped process |
| Output | `x!(y)` | Send name `y` on channel `x` |
| Input | `x?(y).P` | Receive name `y` on channel `x`, then behave as `P` |
| Restriction | `νx.P` | Create a fresh name `x` with scope `P` |
| Replication | `!P` | Replicate `P` arbitrarily many times |
| Reflection | `quote(x)` | Treat name `x` as data (reflect) |
| Dereference | `eval(x)` | Treat data `x` as a name (reify) |

In the blockchain context:
- **A channel** is a contract instance (a sled tree + WASM entrypoint)
- **A name** is a capability (a secret key whose possession authorizes action)
- **Output** is posting a commitment (placing a name's public face on-chain)
- **Input** is discovering a commitment via AEAD decryption (receiving a name)
- **Restriction** is deriving a per-instance key (scoping a name to a contract)
- **Replication** is the nullifier SMT (a name can be consumed exactly once;
  replication models the infinite supply of fresh names)

## 1. What Is a Type in This System?

**A type is a behavioral position in a concurrent interaction graph.**

A type `T` constrains three things:
1. **Domain** — what names a process can hold (its internal state)
2. **Barbed interface** — what names a process can observe (its external API)
3. **Scope mobility** — what names a process can extrude (its authority boundary)

Formally, for a process `P` at type `T`:

```
Γ ⊢ P : T
```

means: in naming context `Γ`, process `P` exhibits the behavioral position `T`.
`T` determines:
- Which barbs (observable actions) `P` may exhibit
- Which names `P` may receive or send
- Which names `P` may restrict

### 1.1 Barbs as Observable Actions

A **barb** is an observable action that a process can perform. In the ρ-calculus,
a process `P` exhibits barb `↓x` (read "P barbs on x") if `P` can engage in an
input or output action on channel `x`. In DarkWow:

| Barb | Cryptographic Meaning |
|------|----------------------|
| `↓spend` | Can authorize a token transfer (possesses SpendingKey) |
| `↓view` | Can decrypt notes (possesses ViewingKey) |
| `↓nullify` | Can prevent replay (publishes Nullifier) |
| `↓commit` | Can create a capability (publishes Commitment) |
| `↓prove` | Can satisfy a ZK predicate (generates Proof) |
| `↓verify` | Can check a ZK proof (validates Proof) |

These barbs are **behaviorally distinct**. A process that can spend cannot
necessarily view. A process that can nullify cannot necessarily prove. They
are different positions in the interaction graph.

### 1.2 Bisimulation as Type Equivalence

Two processes `P` and `Q` are **bisimilar** (written `P ∼ Q`) if an observer
cannot distinguish them by interacting with them. Formally: for every action
`P` can take, `Q` can take a matching action leading to bisimilar states, and
vice versa.

**Type Equivalence Principle:** Two types `T₁` and `T₂` may be unified if and
only if there exists a bisimulation between all processes at type `T₁` and all
processes at type `T₂`. If a process at `T₁` exhibits barb `↓x` that no process
at `T₂` can match, the types must remain distinct.

## 2. Type Distinction Principle

**Two types SHALL NOT be unified if there exists ANY context where a process
holding a name of type T₁ would exhibit observably different behavior from a
process holding a name of type T₂.**

COROLLARY: If a type represents a cryptographic capability, it MUST be a distinct
nominal type. The behavioral positions are provably different under bisimulation:

| Type Pair | Distinguishing Barb | Why They Can't Unify |
|-----------|--------------------|-----------------------|
| `SecretKey` ≠ `[u8; 32]` | `↓spend` | One can authorize spend, the other is passive data |
| `Nullifier` ≠ `[u8; 32]` | `↓nullify` | One prevents replay, the other is a byte container |
| `Coin` ≠ `[u8; 32]` | `↓commit` | One is a Poseidon hash of attributes, the other is opaque |
| `PublicKey` ≠ `[u8; 32]` | `↓verify` | One can verify signatures, the other is a byte container |
| `ContractId` ≠ `[u8; 32]` | `↓dispatch` | One routes contract calls, the other is opaque |

This is why the chain-level `Nullifier(pub [u8; 32])` is a type error under the
ρ-calculus: it erases the `↓nullify` barb, treating a replay-prevention capability
as indistinguishable from a generic byte container. The behavioral position is
lost, and with it, the compiler's ability to distinguish "this prevents replay"
from "this is a byte buffer."

### 2.1 Concrete Violation: The Bytes Round-Trip

The miner bridge in `registry/model.rs` performs:

```
pallas::Base → to_repr() → [u8; 32] → Nullifier::from_bytes()
```

Under the ρ-calculus, this is:

```
ν secret. (nullify!(secret) | byte_container!(quote(secret)))
```

The `↓nullify` barb is extruded through a byte channel where it becomes
observable as raw data, then re-wrapped. The middle step — `[u8; 32]` — has NO
behavioral constraints. Any process can produce any 32 bytes and inject them
into the nullifier construction site. The type distinction Principle is violated:
`Nullifier` and `[u8; 32]` have been unified at the byte boundary, erasing the
behavioral difference.

**The fix** (Phase 1 of the implementation plan) restores the type distinction:
`Nullifier::new(secret, coin)` produces a `Nullifier` directly, never passing
through the untyped byte channel. The barb `↓nullify` remains within the typed
domain throughout.

## 3. What Generic Types SHALL NOT Express

Generic parameters (`T`, `impl Trait`) abstract over the behavioral position of a
name. This abstraction is ONLY permissible when all three conditions hold:

**Condition (a):** The function's behavior does NOT depend on the specific barbs
of `T`. A function `serialize<T: Encodable>(x: &T) -> Vec<u8>` satisfies this:
it reads the structure of `T` without caring what `T` can DO. A function
`verify<T: ZkVerifiable>(proof: &T) -> bool` does NOT: it must know the
predicate language L_{r,s} that `T` proves, which is specific to the capability.

**Condition (b):** `T` is not crossing a restriction boundary (ν-scope). A
`SecretKey` created by `ν secret` (derived per-instance) must not be passed
through a generic interface that extrudes it beyond its declared scope. The
`OwnedSecretKey` type enforces this at the Rust level by having no `::random()`
constructor and only crate-visible construction paths.

**Condition (c):** `T` is not a cryptographic capability with distinct security
semantics. A `Nullifier` has the `↓nullify` barb; a `Coin` has the `↓commit`
barb. A generic `process_capability<T>(x: T)` that accepted either would violate
type distinction because it would unify processes with different observable
behavior.

**VIOLATION EXAMPLE:**
```rust
fn process<T: AsRef<[u8]>>(data: T) { ... }
// Called as:
process(secret_key)  // SecretKey crosses a ν-boundary into a byte interface
```
This is forbidden: `SecretKey` carries the `↓spend` barb; `AsRef<[u8]>` erases it.

**CORRECT PATTERN:**
```rust
fn authorize(key: SecretKey, commitment: Coin) -> Nullifier { ... }
// Each parameter is a distinct nominal type. The compiler enforces:
// - SecretKey ≠ Coin ≠ Nullifier
// - No byte-level unification
// - Every barb preserved through the call graph
```

## 4. Error Types as Observable Barbs

Each error variant IS an observable action (a barb) of the system.

When a process can fail in ways that demand DIFFERENT responses from its
containing context, those failures MUST be distinct types. They represent
DISTINCT BARBS under bisimulation — an observer can distinguish which failure
occurred and must take context-specific action.

| Error | Barb | Observable By | Context Response |
|-------|------|---------------|------------------|
| `InvalidNullifier` | ↓bad-nullifier | Mempool, Chain | Reject transaction |
| `NullifierAlreadySpent` | ↓double-spend | Chain | Block is invalid |
| `InvalidProof` | ↓bad-proof | Contract VM | Reject call |
| `KeyDerivationFailed` | ↓bad-derive | Wallet | Skip note, do not crash |
| `DatabaseCorrupted` | ↓db-fail | Infrastructure | Fatal — restart from genesis |

These barbs are NOT interchangeable. A `NullifierAlreadySpent` failure requires
block-level rejection (consensus). A `KeyDerivationFailed` failure requires
note-level skipping (wallet). Unifying them under a single `IoError(String)` type
erases the behavioral distinction — the caller cannot distinguish "this is a
consensus failure" from "this note isn't mine" from "the database is corrupt."

**This is why Rule 6 exists:** Error types must be specific. `ContractError` with
a generic `IoError(String)` variant is acceptable only when the error has no
behavioral significance. Every error with behavioral significance must have its
own variant.

**This is why `unwrap_or_default()` is forbidden (Rule 1):** It erases the error
barb entirely. The context that should have observed `↓bad-derive` instead
observes nothing — the process silently continues with a ZERO key. No barb, no
diagnosis, no recovery.

## 5. Authority = Name Possession

A process CAN perform action `A` if and only if it POSSESSES the name
(capability) for `A`. This is enforced by the type system: the function
signature REQUIRES the capability type as a parameter.

**No ambient authority exists.** There are no global admin keys, no upgrade
proxies, no "owner" addresses, no `sudo` functions. Authority flows ONLY through
explicit name passing at the type level.

In DarkWow, this is realized through the `OwnedSecretKey` and `MiningRecipient`
types:

```rust
// Authority to mine: you MUST hold an OwnedSecretKey from AccountManager
pub fn from_account(mgr: &AccountManager, height: u32) -> Result<Self, String> {
    let owned = mgr.default_owned()?;  // ← ν-bound: only the declared identity
    // ...
}

// Authority to spend: you MUST present the SecretKey matching the commitment
pub fn nullifier(&self) -> Nullifier {
    Nullifier::from(poseidon_hash([self.secret.inner(), self.coin.inner()]))
    // ↑ secret is available because OwnCoin was constructed from a decrypted note
}
```

Compare with the forbidden pattern (ambient authority):
```rust
// FORBIDDEN: anyone with ANY [u8; 32] can "authorize" — no name possession required
fn authorize_mining(bytes: [u8; 32]) -> MiningRecipient { ... }
```

The type distinction between `OwnedSecretKey` and `[u8; 32]` is not cosmetic —
it enforces the authority model. You cannot call `from_account` with a random
byte array. You must possess the name.

## 6. Connection to the Authorization Inversion Theorem

The Authorization Inversion Theorem (ocap.md, Substack article) states:

> **Theorem 1.** An ACL-based authorization system A(p, r, s) can be inverted to
> a privacy-preserving O-Cap scheme A'(π, r, s) if and only if there exists a
> ZK proof system for the language L_{r,s} = { w : P_{r,s}(w) = 1 } with proofs
> simulatable without knowledge of w.

In the ρ-calculus framing, this becomes a type-level statement:

**The type of a capability IS the predicate language it proves.**

```
CapabilityType(r, s) ≡ L_{r,s}
```

Where `L_{r,s}` is the ZK proof language for predicate `P_{r,s}` over resource
`r` and action `s`. The capability type encodes:
- What must be proven (the predicate `P_{r,s}`)
- What the verifier observes (the barb `↓prove`)
- What is hidden (the witness `w`)

This means every capability gets its own type — or at minimum, its own type
parameter. A "can transfer up to 100 tokens" capability is a different type from
a "can vote on proposal X" capability, because the predicate languages are
different, and therefore the barbs the verifier can observe are different.

In DarkWow's current implementation, the generic `CapRecord` with a `cap_id:
String` (bs58-encoded) erases this type distinction. Every capability — token,
governance vote, insurance claim, attestation — is stored as the same
undifferentiated record. This is the type-theoretic root of the capability
engine's unsoundness (see Implementation Plan, "Deferred: Capability Engine").

### 6.1 The Two Modes as Type Refinement

The O-Cap model has two realizations (ocap.md):
- **Reference Mode (Agoric):** The capability IS an object reference. `typeof
  invitation === Payment`. The type is checked at runtime by the object system.
- **ZK Mode (DarkWow):** The capability IS a secret whose knowledge can be
  proven. The type is the ZK circuit that verifies the predicate.

Under bisimulation, these are the SAME model at different levels of abstraction.
Agoric's `Payment` type and DarkWow's `NativeTokenTransfer` ZK circuit both
exhibit the same barb: `↓spend`. The difference is what is REVEALED by the barb:
- Agoric: `↓spend(payment_id=0x123, amount=100, brand=Moola)` — public
- DarkWow: `↓spend(π, nullifier=0xabc)` — everything else hidden

The Authorization Inversion Theorem guarantees the conversion is bidirectional
(if-and-only-if). The type system must preserve this: a ZK capability type must
be refinable to a plaintext capability type, and vice versa, by adding or
removing the zero-knowledge wrapper.

## 7. Invariant: What The Compiler Must Enforce

Any code that compiles MUST satisfy these five invariants, derived from the
ρ-calculus semantics:

1. **No name is used without being received or created.** (Name possession
   discipline.) A function that takes no `SecretKey` parameter cannot sign.
   A function that takes no `Nullifier` parameter cannot check replay.
   Authority is explicit in the type signature.

2. **No distinct behavioral positions are unified under a single type.**
   (Type Distinction Principle, §2.) `Nullifier` cannot be `[u8; 32]`.
   `SecretKey` cannot be `AsRef<[u8]>`. The compiler must reject any attempt
   to pass a capability through an untyped channel.

3. **No restricted name crosses its declared scope boundary.** (ν-scope
   discipline.) A `SecretKey` derived for contract instance `A` must not
   be usable in contract instance `B`. `derive_instance` enforces this by
   binding the key to `(contract_id, instance_seed)`.

4. **All error barbs are distinguishable by their containing contexts.**
   (Error Type Principle, §4.) A consensus failure and a wallet note-skip
   are different barbs. They must be different error types. The caller
   must be able to match on which failure occurred.

5. **Authority to perform cryptographic operations is represented by
   possession of the corresponding cryptographic key type.** (§5.)
   No ambient authority. No global state. No `sudo`. Every authorization
   path is traceable through the type graph.

## 8. Relationship to the Implementation Plan

Every fracture identified in the implementation plan traces to a violation of
one of these invariants:

| Fracture | Invariant Violated | Fix |
|----------|--------------------|-----|
| Chain-level `Nullifier([u8; 32])` | #2 (type distinction) | Delete, use `Nullifier(pallas::Base)` |
| `derive_instance` zero-key bug | #4 (error barbs indistinguishable) | Return `Result`, reject non-canonical |
| `PublicKey::xy()` panic | #4 (error barbs indistinguishable) | Return `Option` or `Result` |
| `Transaction::hash()` panic | #4 (error barbs indistinguishable) | `unwrap_or_else` with logging |
| Mempool `HashSet<Vec<u8>>` | #2 (type distinction) | `BTreeSet<Nullifier>` |
| Chain_state `HashMap<[u8; 32], u64>` | #2 (type distinction) | `BTreeMap<Nullifier, u64>` |
| `ContractCall.contract_id: [u8; 32]` | #2 (type distinction) | `ContractId` |
| Miner fallback `Nullifier([0u8; 32])` | #3 (scope violation — zero is not a valid name) | `Option<Nullifier>` |
| `CoinAttributes.spend_hook: pallas::Base` | #2 (type distinction) | `FuncId` |
| Capability engine `cap_id: String` (bs58) | #2 + #5 (all capability types unified) | Deferred to separate work stream |

## 9. Namespace: What Types Exist

From the ρ-calculus primitives and the Authorization Inversion Theorem, the
following type hierarchy emerges. Every type maps to a behavioral position.

### 9.1 Cryptographic Primitive Types (Names)

| Type | Inner | Barbs | Scope | Upstream Match |
|------|-------|-------|-------|---------------|
| `SecretKey` | `pallas::Base` | `↓spend`, `↓derive` | ν-restricted to holder | ✓ |
| `PublicKey` | `pallas::Point` | `↓verify`, `↓encrypt` | May be extruded (public) | ✓ |
| `Nullifier` | `pallas::Base` | `↓nullify` (prevents replay) | Public (must be revealed to consume) | ✓ |
| `Coin` | `pallas::Base` | `↓commit` (represents value) | Public (on-chain commitment) | ✓ |
| `ContractId` | `pallas::Base` | `↓dispatch` (routes calls) | Public (deployed contract address) | ✓ |
| `TokenId` | `pallas::Base` | `↓denominate` (identifies asset) | Public | ✓ |
| `FuncId` | `pallas::Base` | `↓gate` (authorizes spend hook) | Public | ✓ |
| `MerkleNode` | `pallas::Base` | `↓prove-inclusion` | Public | ✓ |

### 9.2 Structural Types (Channels)

| Type | Inner | Barbs | Notes |
|------|-------|-------|-------|
| `BlockHeader` | struct | `↓validate-pow` | blake3 hashes for merkle roots |
| `Transaction` | struct | `↓process` | Contains typed Nullifiers, ContractCalls |
| `ContractCall` | struct | `↓invoke` | contract_id: ContractId (typed) |
| `AeadEncryptedNote` | struct | `↓discover` | ChaCha20Poly1305 + Sapling DH |

### 9.3 Authority Types (ν-restricted)

| Type | Inner | Barbs | Construction |
|------|-------|-------|-------------|
| `OwnedSecretKey` | `SecretKey` | `↓spend` (only if declared) | `from_declared_bytes` only |
| `MiningRecipient` | `PublicKey` + `OwnedSecretKey` | `↓mine` | `from_account` only |
| `AccountManager` | `Vec<Account>` | `↓identity` | From keys.toml |

### 9.4 Types That Must Remain Distinct (Non-Unifiable Pairs)

These pairs may NOT be unified under any generic interface, trait bound, or
type alias:

- `Nullifier` ≠ `[u8; 32]`
- `Nullifier` ≠ `IntentNullifier` (different predicate languages)
- `Coin` ≠ `[u8; 32]`
- `SecretKey` ≠ `[u8; 32]`
- `SecretKey` ≠ `pallas::Base` (one barbs, one doesn't)
- `PublicKey` ≠ `pallas::Point` (one validates identity, one doesn't)
- `ContractId` ≠ `[u8; 32]`

## 10. References

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
