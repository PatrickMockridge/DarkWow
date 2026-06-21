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

Consider a transaction with two operations: burn an old coin to spend it,
and mint a new coin for the recipient. An adversary sees both proofs
on-chain. Without transaction binding, the adversary could:

1. Take the burn proof from Alice's transaction (proving she destroyed her
   coin)
2. Take the mint proof from Bob's transaction (creating Bob's output)
3. Combine them into a new transaction that spends Alice's coin to create
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
tx_binding = poseidon_hash(tx_commitment, tx_nonce)
```

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

tx_binding = poseidon_hash(tx_commitment, tx_nonce);
constrain_instance(tx_binding);
constrain_instance(tx_nonce);
```

### Verification

The node processing a transaction already knows `tx_commitment` (it's in
the `Transaction` struct). For each proof, the node:

1. Reads `tx_nonce` from the proof's public inputs
2. Computes `expected = poseidon_hash(tx.tx_commitment, tx_nonce)`
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
| Proof recombination prevented? | Yes — binding enforced | Yes — binding enforced |
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
`poseidon_hash` derivation constraint. The binding is enforced at the
circuit level — a proof that doesn't properly derive `tx_binding` from the
correct `tx_commitment` will not verify.

### Client Layer

Each contract's client builder computes the binding:

```rust
let tx_binding = poseidon_hash([input.tx_commitment, input.tx_nonce]);
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

- **127 ZK circuits** across all contracts — binding enforced at proof level
- **~22 contract client crates** — builders compute per-proof binding
- **Transaction struct** — stores `tx_commitment`, provides `compute_tx_binding()`
- **Execution layer** — verifies per-proof binding against transaction commitment
