# Privacy Model

This document defines DarkWow's privacy architecture and the design decisions
that flow from it. It SHALL be read in conjunction with the [Type
System](type-system.md), [Contract WASM Type System](contract-wasm-type-system.md),
and [O-Cap](ocap.md) specifications.

## 0. Aspiration — Full Privacy

The ideal is that no observer learns anything about a transaction beyond "a
valid state transition occurred." The resource identity, the amount, the
participants — all stay within the ZK witness. The chain sees only a nullifier
(to prevent double-spend) and a Merkle root (to anchor the inclusion proof).

## 1. The Extra Dimension

Full privacy requires Merkle inclusion proofs. The resource identity cannot be
exposed as a ZK circuit public input (that would tell the observer WHICH
resource was spent). So the entrypoint cannot do a direct key-value lookup by
ID. Instead, the ZK circuit proves "I own a resource in this Merkle tree" and
exposes only the Merkle root. The entrypoint verifies that the root is a known
historical root.

This adds a dimension of constraint:

- A Merkle tree must be initialized and persisted across block commits
- Every state transition must call `merkle_add` to grow the tree
- A roots database must store every historical root for lookup
- The ZK circuit must constrain `merkle_root(leaf_pos, path, resource_id)`
- The encoding format between manual tree serialization and host-side
  deserialization in `merkle_add` must be byte-identical

Each of these is a failure point. Each exercises the type system, the
serialization layer, and the encoding/decoding infrastructure more rigorously
than any path that does not require Merkle inclusion.

## 2. The Pragmatic Trade-Off

Full privacy (the **hard path**) is the goal. But it carries an extra dimension
of constraint that challenges the type system, serialization, and composition
infrastructure at their limits.

The alternative (the **proven path**) exposes the resource ID as a circuit
public input. An observer can see WHICH resource is being operated on — but
not how much, by whom, or to whom. The resource is instantiated publicly once,
then the same visible instance changes hands via nullifiers. Direct KV lookup
replaces Merkle inclusion proof. The type system, serialization, and encoding
infrastructure are exercised at a lower intensity.

DarkWow's position:

> **Proven hardness on the proven path is better than unproven hardness on the
> hard path.** The type system is not negotiable — no unsafe casts, no
> derive-based encoding, no serialization hacks to chase a privacy level that
> the infrastructure cannot yet safely support. If the extra dimension of
> constraint reveals a fracture in the type infrastructure, the infrastructure
> is fixed first. Contracts operate at the privacy level the infrastructure
> can safely provide.

## 3. The Hardness Ceiling

The hard path is not a fixed target. As contracts grow in complexity — more
state, more composition, more cross-contract calls — the extra dimension of
Merkle inclusion interacts multiplicatively with every other constraint. The
hardness ceiling is unknown and likely unknowable in advance. It is discovered
empirically: a contract that composes cleanly at the proven path may surface
type fractures at the hard path.

This is an ongoing problem. The infrastructure will improve. The ceiling will
rise. But there will always be a point where the extra dimension demands more
from the type system than it can safely give. At that point, the proven path
is not a failure — it is the correct engineering decision.
