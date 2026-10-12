# ZK Engineering Posture

## Over-Engineered, Deliberately

DarkWow ships ZK proofs on functions where a Schnorr signature would be
cryptographically sufficient. This is not an accident or an oversight — it is a
deliberate engineering posture. We document it here so future readers understand
the trade-off and the reasoning.

## The Tier System

Every ZK circuit in the codebase falls into one of three tiers:

### Tier 3 — ZK is genuinely required

Value hiding (Pedersen commitments), Merkle proofs of set membership,
selective disclosure of attributes ("age ≥ 18" without revealing age),
commit-reveal schemes for provable fairness, cross-chain deposit proofs.
These circuits hide data that **must** be hidden for the function to deliver
its privacy guarantees. No signature scheme can replace them.

Examples: Purse balance proofs, Promissory Note commitment spends, Identity
credential claims, Bridge deposit proofs, gambling commit-reveal.

### Tier 2 — ZK for application logic, identity could be Schnorr

Circuits that combine two concerns:
1. Value privacy or Merkle proofs (genuinely needs ZK)
2. Identity/key-derivation proof (could be a Schnorr signature)

The identity portion (~200–500 constraints of Poseidon hash + coordinate
checks) is mixed into the same circuit as the application logic. Splitting them
would mean two verification steps (Schnorr + ZK) instead of one — a latency
trade-off, not a security one. The system chooses single-proof atomicity.

Examples: Stablecoin CDP operations, DEX swaps, auction bids, escrow
claims, OTC swap execution.

### Tier 1 — ZK is pure ceremony, Schnorr would suffice

Circuits that prove exactly one thing: "I know the secret key for this public
key." The public key is exposed as a public input. The circuit derives it,
constrains the coordinates, and computes a nullifier. **No value is hidden.
No Merkle proof is verified. No private data enters the circuit.**

A Schnorr signature over `(contract_id, function_code, tx_hash)` proves the
exact same fact at ~0.1% of the verification cost.

Examples across ~15 contracts: governance config updates, house authorization
in gambling contracts, drain protection proposals/votes, oracle registration,
attestation creation, subscription cancellation.

## Why We Keep Tier 1 ZK Anyway

### 1. Market expectation

Users evaluating a "ZK blockchain" expect ZK proofs on contract calls. A
signature-only call path, even where cryptographically sufficient, reads as
"this blockchain doesn't actually use ZK." The market does not distinguish
between "ZK is unnecessary here" and "the project cut corners." We ship the
proof.

### 2. Over-engineered ≠ under-engineered

Shipping unnecessary ZK is a completeness surplus. Shipping insufficient ZK is
a security deficit. The former is a performance cost we can optimize later; the
latter is a vulnerability that can't be patched without a hard fork. When the
choice is between over-engineering and under-engineering a privacy system, we
choose over-engineering every time.

**But "every contract call path carries a ZK proof — no asterisks, no exceptions" is not true, and
the exceptions are named rather than denied** (`F-614`). `native_token` publishes eight endpoints and
**four** carry no proof: `mint` (0x01), `pow_reward` (0x05), `fee_collect` (0x06) and `uncle_mint`
(0x07). They are plaintext by design and their circuits were deleted — the coinbase, the fee pot and
the uncle mint are consensus accounting that the host computes from values the block already
commits to, and a proof over them would attest to arithmetic the validator performs anyway.

So the honest form of the claim is narrower and stronger: **every path that carries *private*
state — every note, commitment, nullifier or capability — is proven**, and the unproven endpoints are
the ones whose inputs are already public. `requires_proof` in each manifest is the field that says
which is which, and `bin/dwowd/src/tests/specs/native_token_spec.rs`'s `is_zk: false` on
`FeeCollectV1` is the same fact from the test side.

### 3. Uniform verification path

Every contract call follows the same verification flow: extract ZK public
inputs, verify the proof, apply state update. Heterogeneous auth (some calls
ZK, some calls Schnorr) creates two code paths, two failure modes, two audit
surfaces. Uniformity has engineering value — especially in a system where the
VM, the prover, and the verifier are all under active development.

### 4. Future-proofing

A Tier 1 circuit today may become Tier 2 tomorrow. If `UpdateConfig` later
needs to prove something about the *previous* config without revealing it,
the ZK infrastructure is already in place. Retrofitting ZK into a
signature-only call path is harder than upgrading an existing circuit.

### 5. Remove cleanly, not urgently

The analysis exists. The circuits are cataloged. If verification cost ever
becomes a bottleneck, Tier 1 circuits can be replaced with Schnorr signatures
on a per-contract basis with zero protocol changes — just a contract upgrade
that switches `get_metadata` from `zk_public_inputs` to `signature_pubkeys`.
The escape hatch is documented. There is no rush to use it.

## Computational Reality

A Tier 1 Halo2 PLONK proof costs ~50–200ms to verify. A Schnorr signature
costs ~60–100µs. The delta is real (~1000×) but the absolute cost per proof
is small enough that it does not dominate block verification time under
current throughput. If and when that changes, the migration path is:

1. Switch `get_metadata` to return `signature_pubkeys` instead of ZK inputs
2. Use `(action_id, public_key)` tuples or boolean state for replay protection
3. Remove the `.zk` circuit file and its compilation target
4. Update the client builder to skip proof generation

No consensus change. No hard fork. No new host functions.

## See Also

- [Contract Trust Model](contract-trust-model.md) — don't trust, verify
- [Overview](overview.md) — system architecture
- [Formal Specification](formal-specification.md) — what the system proves
