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
