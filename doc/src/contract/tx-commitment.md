# Transaction Commitment: Binding Proofs Without Breaking Privacy

Every ZK proof in DarkWow is a self-contained cryptographic statement: "I
know a witness that satisfies this circuit." Without binding, an adversary
can take a proof from transaction A and combine it with a proof from
transaction B — breaking the atomicity of contract operations and enabling
cross-transaction proof recombination attacks.

The **transaction commitment** (`tx_commitment`) binds every ZK proof in a
transaction to that transaction's specific call set. The binding is
enforced by the ZK circuit itself, but the binding value is never shared
between proofs — eliminating the linkability that a naive shared public
input would create.

---

## The Attack It Prevents

Consider a transaction with two operations: burn an old commitment to spend it,
and mint a new commitment for the recipient. An adversary sees both proofs
on-chain. Without transaction binding, the adversary could:

1. Take the burn proof from Alice's transaction (proving she destroyed her
   commitment)
2. Take the mint proof from Bob's transaction (creating Bob's output)
3. Combine them into a new transaction that spends Alice's commitment to create
   Bob's output

Both proofs verify independently. The burn proof doesn't know it was meant
to be paired with Alice's mint output, not Bob's. The mint proof doesn't
know which burn it was paired with. An observer sees valid proofs and
accepts the combined transaction.

The `tx_commitment` prevents this by cryptographically binding every proof
to the full set of contract calls in its transaction. A proof created for
transaction A cannot be used in transaction B — the binding wouldn't match.

---

## Design

### The Commitment

```
tx_commitment = blake3(encode(call_1) || encode(call_2) || ... || encode(call_n))
```

The `tx_commitment` is a Blake3 hash of all `ContractCall` data in the
transaction — contract IDs, function codes, and parameters. Proofs and
signatures are **excluded** from the hash to avoid a circular dependency:
proofs are created *after* the commitment is known, and the commitment
can't include the proofs it will later bind.

This is computed once by `TransactionBuilder::build()` and stored in the
`Transaction.tx_commitment` field. It is known to the prover (who builds
the transaction) and to every node that processes the block.

### The Nullifier Scheme

A naive design would expose `tx_commitment` directly as a public input
on every proof:

```zk
constrain_instance(tx_commitment);  // BAD: all proofs share this value
```

This creates a deterministic link between every proof in the same
transaction. An observer groups proofs by their `tx_commitment` value.

Instead, each proof derives a **unique binding value** using a per-proof
random nonce:

```
tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)
```

where `DOMAIN_TX_BINDING = 3` (`DRK_POSEIDON_DOMAIN_TX_BINDING`,
`src/sdk/src/crypto/constants.rs:57`; inlined in `.zk` source as
`witness_base(3)` — see `circuit-versioning.md`). This document previously
wrote the two-argument form, which no circuit and no client derives.

Where:
- `tx_commitment` — **private witness**. Known to the prover and the node,
  but never exposed as a ZK public input.
- `tx_nonce` — **public input**. A random `pallas::Base` value, unique per
  proof. Generated fresh by the prover for each proof in the transaction.
- `tx_binding` — **public input**. The Poseidon hash binding the proof
  to the transaction without revealing which transaction.

### Circuit Pattern

Every ZK circuit (127 circuits across all contracts) implements:

```zk
Base tx_commitment;     // private — prover supplies the real commitment
Base tx_nonce;           // public — random per proof
Base tx_binding;         // public — derived binding value

tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce);
constrain_instance(tx_binding);
constrain_instance(tx_nonce);
```

### Verification

**Stage 4 is specified here and is NOT implemented.** Read this section as
the requirement, not as a description of the node. Measured on
`linear-master @ d775e37c6d` (github issue #3, filed 2026-10-03; register
`OBL-C198`): of the four stages below, (1) the circuit derivation and (2)
`metadata()`'s publication exist, (3) proof↔metadata agreement exists in
`src/linear/src/zk_verifier.rs`, and (4) the check this section describes
does not exist anywhere — nothing in `zk_verifier.rs` or `execution.rs`
compares a published `tx_binding` against anything derived from the
transaction, and no host import exposes `tx_commitment` to WASM at all.
`core_tx.tx_commitment` *is* in scope at the reconciliation site
(`zk_verifier.rs:274-289`), so the check is a local addition rather than a
data-plumbing project. Until it lands, a proof is bound to a
`tx_commitment` the prover chose, and a proof lifted from one transaction
verifies in another.

The node processing a transaction already knows `tx_commitment` (it's in
the `Transaction` struct). For each proof, the node:

1. Reads `tx_nonce` from the proof's public inputs
2. Computes `expected = poseidon_hash(DOMAIN_TX_BINDING, tx.tx_commitment, tx_nonce)`
3. Verifies `expected == tx_binding`

If a proof was created for a different transaction, the `tx_commitment`
would differ, `poseidon_hash` would produce a different result, and
verification would fail.

---

## Privacy Analysis

### What Is Hidden

**Transaction linkability is eliminated.** Two proofs in the same transaction
have different random `tx_nonce` values, producing different `tx_binding`
values. An observer who sees:

```
Proof A: tx_binding = 0x3a7b..., tx_nonce = 0x9f2c...
Proof B: tx_binding = 0x84e1..., tx_nonce = 0x15d3...
```

...cannot determine whether `0x3a7b... = poseidon_hash(T, 0x9f2c...)` and
`0x84e1... = poseidon_hash(T, 0x15d3...)` derive from the same `T`, because
`T` (the `tx_commitment`) is never revealed. This is the hiding property of
Poseidon as a cryptographic hash — without knowing the preimage, you cannot
verify a hash-preimage relationship.

### What Is Revealed

- **`tx_nonce`** reveals nothing — it's a random field element with no
  relationship to any on-chain state.
- **`tx_binding`** reveals nothing — it's a hash output that cannot be
  inverted or linked without knowing `tx_commitment`.
- The **number** of proofs in a transaction remains visible at the
  transaction structure level (the `Transaction` struct carries its proofs
  in a `Vec`), not from the ZK proof public inputs.

### Comparison

| | Before (raw `tx_commitment`) | After (nullifier scheme) |
|---|---|---|
| Proofs linkable to same tx? | **Yes** — same `tx_commitment` on all proofs | **No** — different `tx_nonce` per proof, different `tx_binding` |
| Proof recombination prevented? | No — stage 4 absent (`OBL-C198`) | **Not by this mechanism** — stage 4 is specified and not implemented (`OBL-C198`) |
| Additional public inputs per proof | 1 | 2 |
| Additional circuit constraints | 0 | 1 `poseidon_hash` |

### The Binding vs. Privacy Trade-off

Perfect proof independence means proofs have no binding — they're
combinable across transactions. Perfect binding with a shared public
input means proofs are linkable. The nullifier scheme achieves both
properties simultaneously:

- **Binding** is enforced by the ZK circuit (the proof must know
  `tx_commitment` to produce a valid `tx_binding`).
- **Unlinkability** is preserved by the per-proof random nonce (different
  proofs produce different public outputs from the same private
  `tx_commitment`).

---

## Contract Impact

The `tx_commitment` hardening touches every contract in the system.
Each contract's ZK circuits and client builders are updated:

### Circuit Layer

Every `.zk` circuit file (127 across all contracts) includes the
`tx_commitment`/`tx_nonce`/`tx_binding` witness declarations and the
`poseidon_hash` derivation constraint. What the circuit enforces is that
`tx_binding` is *a* hash of *some* `tx_commitment` the prover supplied —
**not** that it is the hash of the enclosing transaction's commitment,
because no node-side check compares the two (`OBL-C198`). The claim that a
proof deriving its binding from the wrong `tx_commitment` "will not verify"
is true only once that check exists.

### Client Layer

Each contract's client builder computes the binding:

```rust
// DRK_POSEIDON_DOMAIN_TX_BINDING = 3; the two-argument form this document
// used to show matches no client and no circuit.
let tx_binding = poseidon_hash([DRK_POSEIDON_DOMAIN_TX_BINDING, input.tx_commitment, input.tx_nonce]);
```

The `CallInput`/`CallData` struct carries `tx_commitment` (supplied by the
wallet) and `tx_nonce` (generated fresh per proof). The `Revealed`/`PublicInputs`
struct exposes `tx_binding` and `tx_nonce` as public inputs.

### Wallet Layer

The wallet computes `tx_commitment` from the transaction's call set and
generates a random `tx_nonce` for each proof. These are passed to each
contract's client builder during transaction construction.

---

## Scope

Measured 2026-10-04 against `linear-master @ d775e37c6d` (github issue #3,
register `OBL-C198`). Where a line below describes something that does not
exist, it says so rather than describing the intent.

- **127 ZK circuits** across all contracts — each derives and instances its
  `tx_binding`; the binding **binds nothing to the enclosing transaction**
  until the node-side check exists (see §Verification)
- **~22 contract client crates** — builders compute per-proof binding
- **Transaction struct** — stores `tx_commitment`. It does **not** provide
  `compute_tx_binding()`: that name exists only as per-contract client
  helpers, never on `Transaction` (`src/tx/mod.rs`). This scope line
  claimed otherwise, and no such method was ever there to be called
- **Execution layer** — does **not** verify per-proof binding against the
  transaction commitment; that is the missing stage above
