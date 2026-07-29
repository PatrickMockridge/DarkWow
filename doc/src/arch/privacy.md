# Privacy Model

This document defines DarkWow's privacy architecture. It SHALL be read in
conjunction with the [Type System](type-system.md), [Contract WASM Type
System](contract-wasm-type-system.md), and [O-Cap](ocap.md) specifications.

## 0. Aspiration — Full Privacy

The ideal is that no observer learns anything about a transaction beyond "a
valid state transition occurred." The resource identity, the amount, the
participants — all stay within the ZK witness. The chain sees only a nullifier
(to prevent double-spend) and a Merkle root (to anchor the inclusion proof).

## 1. The Extra Dimension

Full privacy requires Merkle inclusion proofs. The resource identity cannot be
exposed as a ZK circuit public input (that would tell the observer WHICH
resource was spent). The ZK circuit proves "I own a resource in this Merkle
tree" and exposes only the Merkle root. The entrypoint verifies that the root
is a known historical root.

This adds a dimension of constraint:

- A Merkle tree must be initialized and persisted across block commits
- Every state transition must call `merkle_add` to grow the tree
- A roots database must store every historical root for lookup
- The ZK circuit must constrain `merkle_root(leaf_pos, path, resource_id)`
- The encoding format between contract-side tree serialization and host-side
  deserialization in `merkle_add` must be byte-identical

## 2. Two Privacy Levels

DarkWow defines two privacy levels. The distinction is about whether resource
identity is visible to observers.

### L1 — Full Privacy (Merkle Inclusion Proofs)

Resource IDs are in the ZK witness. The ZK circuit proves Merkle inclusion
against a known root. An observer sees only a nullifier and a Merkle root —
not which resource was operated on, not by whom, not how much.

L1 is required for **transferable objects-as-capabilities** — resources that
change hands between parties (coins, boxes, purses). Exposing the resource ID
would leak social-graph information. The Merkle root closes this leak.

L1 contracts: PromissoryNote, Box, Purse.

### L2 — Instance Privacy (ZK Public Inputs)

Resource IDs are ZK circuit public inputs. An observer can see WHICH resource
is being operated on — but not how much, by whom, or to whom. Direct KV lookup
replaces Merkle inclusion proof.

L2 is the correct level for **static records** — resources that don't change
hands (identity credentials, attestations, oracle values, multisig groups).
The resource identity IS the public fact. There is no social-graph leak
because there is no transfer.

L2 contracts: Identity, Attestation, Oracle, Multisig.

### The Design Rule

**L1 for transferable o-caps, L2 for static records.** This is not a hierarchy
of quality. L2 is not "worse" than L1 — it is the correct engineering decision
for the contract's domain. Adding Merkle inclusion proofs to a static record
adds constraint without adding privacy benefit.

## 3. The Hardness Ceiling

L1 is not a fixed target. As contracts grow in complexity, the extra dimension
of Merkle inclusion interacts multiplicatively with every other constraint. The
ceiling is discovered empirically: a contract that composes cleanly at L2 may
surface type fractures at L1. The infrastructure improves. The ceiling rises.

## 4. Failure Modes of L1

The extra dimension of constraint produces a characteristic failure pattern:
**nebulous errors with simple root causes.**

Example: PromissoryNote's heavyweight test failed with `ContractError(Internal)`
from `merkle_add`. All 18 error sites returned the same non-descriptive code.
After fixing error propagation (18 distinct codes), the error resolved to
`ContractError(DbGetEmpty)` — the Merkle tree data was never written.

Root cause: `init_contract` initialized Merkle trees inside a `match db_lookup()`
guard. If the handle already existed, all tree initialization was skipped. The
fix was two lines — move the tree init outside the guard.

The pattern:
1. L1 contract fails with a non-descriptive error
2. The error hides behind a generic catch-all
3. Proper error propagation reveals the true failure
4. The root cause is simple — a guard condition, a missing init
5. The fix is trivial — but finding it required self-describing errors

**Lesson**: Error propagation is not optional at L1. Every failure site in a
host function that serves L1 contracts MUST return a distinct error code.

## 5. Architectural Principles — Composition, Encode/Decode, Type System

The following principles govern every ZK circuit ↔ Rust contract boundary.
They emerged from the July 2026 HAZOP analysis of Box and Purse L1 circuits
and apply to every contract that uses domain-separated ZK proofs.

### 5.1 The Metadata-Circuit Boundary as ρ-Calculus Quote/Eval

Every `constrain_instance(X)` in a circuit IS a ρ-calculus quote: the circuit
serializes `X` as a public input. The host verifier reads this public input
and compares it against the metadata function's return value. This comparison
IS the ρ-calculus eval: `decode(encode(X)) == X`.

The metadata function MUST produce byte-identical values for every public
input position. When the circuit computes `X = poseidon_hash(domain, inputs...)`,
the metadata function must produce the SAME `X`. A mismatch in any of: input
count, input order, or domain constant value produces a verification failure.

**Witness-only inputs**: Values that depend on witness-only data (`owner_secret`,
`balance_blind`) cannot be computed by the metadata function. These MUST pass
through the params struct — the caller pre-computes, the circuit constrains
via `constrain_equal_base(circuit_value, params_value)` then
`constrain_instance(params_value)`, and the metadata echoes `params.value`.

**Available inputs**: Values computable from available data (`tx_binding` from
`tx_commitment + tx_nonce`) can be recomputed by the metadata function — but
MUST use the same domain constants as the circuit.

### 5.2 Domain-Separated Poseidon as Type Restoration

Per [type-system.md §2](type-system.md): `poseidon_hash` is a type erasure
boundary. Typed inputs (`Nullifier`, `Commitment`, `AssetId`) all produce the
same output type (`pallas::Base`). Without domain separation,
`poseidon_hash(secret, coin)` and `poseidon_hash(secret, box_id)` produce
indistinguishable outputs — the type distinction is lost at the hash boundary.

Domain constants (`witness_base(N)`) prepended to every hash call restore
type distinctions. `poseidon_hash(DOMAIN_NULLIFIER, ...)` produces a
behaviorally distinct output from `poseidon_hash(DOMAIN_COIN_COMMIT, ...)`.
The domain constant IS the type tag — it proves "this hash was computed for
purpose X, not purpose Y."

The domain constant vocabulary (7 values, cross-circuit):

| Constant | Purpose |
|---|---|
| `witness_base(1)` | Nullifier |
| `witness_base(2)` | Token commitment |
| `witness_base(3)` | Transaction binding |
| `witness_base(4)` | Coin commitment |
| `witness_base(5)` | Merkle leaf |
| `witness_base(6)` | User data encryption |
| `witness_base(7)` | Signature secret / key derivation |

Every `poseidon_hash` call in every circuit MUST prepend the appropriate
domain constant as its first argument. Every Rust-side computation of the
same value MUST include the same domain constant.

### 5.3 The Circuit-Harness-Metadata Triad

Three components must agree on every `constrain_instance` value:
1. **Circuit** (`.zk`): constrains derivation and order, publishes as instances
2. **Harness** (Rust test): provides public inputs matching circuit instances
3. **Metadata** (contract entrypoint): returns same values to host verifier

A mismatch at any vertex of this triangle is a protocol violation. The
harness IS the specification of what the metadata function must return —
if harness and metadata differ, one is wrong relative to the circuit.

### 5.4 Encode/Decode at Every Boundary

Per [contract-wasm-type-system.md §3.1](contract-wasm-type-system.md): three
encoding boundaries exist in every contract. The metadata-circ circuit boundary
adds a fourth: the public input vector.

| Boundary | Invariant |
|---|---|
| External → exec | `Params::decode(encode(params)) == params` |
| Exec → apply | `Update::decode(update_data) == update` |
| Sled state | `Value::decode(db_get(key)) == value` |
| Circuit → metadata | `metadata[i] == proof_instance[i]` for all i |

The fourth boundary is the one the HAZOP found broken. The circuit's
`constrain_instance` order MUST match the metadata function's
`zk_inputs.push()` order, position for position. The proof's instance
column MUST match byte-for-byte.

### 5.5 The Consume+Create Model — Objects as State-Transition Chains

In L2, Box and Purse were persistent mutable containers. A Box came into
existence once and remained — you could Put into the same box_id repeatedly.
A Purse was a persistent balance account — you could Deposit and Withdraw
against the same purse_id forever. The object was a record with mutable state.

In L1, the object itself follows the revoke/issue + nullifier pattern. Every
operation consumes the old state and creates a new one. The "object" doesn't
persist — it's a chain of state transitions linked by a resource ID in the
ZK witness. This is the same model as PromissoryNote's coins: a coin is
created (minted), spent (burned with nullifier). The coin doesn't persist —
it gets consumed and a new coin is created.

**Box — the capability to delegate.** Each Put nullifies the previous box
state and appends a new state leaf to the Merkle tree. Each Take nullifies
the current state. The box_id binds transitions together in the witness
but is never a public input. An observer sees only nullifiers and Merkle
roots — not which box, not by whom, not what's inside.

**Purse — the capability to hold fungible value.** Each Deposit nullifies
the old balance state and creates a new one with balance += amount. Each
Withdraw nullifies and creates with balance -= amount. Balance proves
Merkle inclusion without consumption (read-only). The balance amount is
hidden in a Pedersen commitment; conservation is proven via additive
homomorphism in the circuit.

**The structural pattern.** Every transferable o-cap at L1 follows the same
four-component architecture:

1. **Circuit** constrains cryptographic relationships. Every public input is
   a caller-provided witness — the circuit computes a value, constrains it
   equal to the witness via `constrain_equal_base`, then publishes the witness
   via `constrain_instance`.

2. **Params** carry every value needed by both circuit and metadata. Each
   `constrain_instance` position maps to a field in the params struct.

3. **Metadata** echoes params fields directly — no computation, no domain
   constants, no poseidon_hash, no field arithmetic. Pure echo.

4. **Exec** validates against chain state (nullifier unspent). **Apply**
   writes state (merkle_add, db_set nullifier). No cryptographic computation
   in either handler — the circuit already proved everything.

### 5.6 L1 Design Rule

**L1 for transferable o-caps, L2 for static records.** This is not a
hierarchy of quality. L1 requires Merkle inclusion proofs, nullifier
tracking, and the consume+create model at every state transition. L2
uses direct KV lookup with resource IDs as public inputs. The choice
is determined by whether the resource changes hands between parties.
