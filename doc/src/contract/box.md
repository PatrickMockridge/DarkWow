# Box — ZK Capability Container (L1)

The Box contract is the DarkWow equivalent of capability delegation — put a
capability into a Box, and whoever Takes it receives it. It is an **O-Cap
primitive** deployed at genesis (ContractId counter 9). Box is L1: resource
IDs are in the ZK witness, Merkle inclusion proofs hide which box is being
operated on.

## Why Genesis?

In the Agoric o-cap model, Invitations are Payments that grant access to a
specific contract interaction. Box is the ZK-native equivalent: it holds an
arbitrary capability and transfers it via linear consumption (nullifier).
Having a canonical well-known ContractId makes Box available as a composable
primitive for every contract in the ecosystem.

## Operations

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `Initialize` | 0x00 | — | Initialize the Box contract (genesis primitive) |
| `Put` | 0x01 | `put.zk` | Merkle inclusion of old box state. Poseidon-based ownership proof. Nullifier consumes old state. New contents commitment appended to Merkle tree. |
| `Take` | 0x02 | `take.zk` | Merkle inclusion of current box state. Poseidon-based ownership proof. Nullifier consumes state. |

Observer sees only `[nullifier, merkle_root, ...]` — not which box, not by whom,
not what capability is inside.

## Privacy Model

Box is L1 — full privacy via Merkle inclusion proofs. The resource identity
(`box_id`) is bound into a Merkle leaf: `poseidon_hash(DOMAIN_SIGNATURE_SECRET,
box_id, contents_commit, state_nonce)`. The `merkle_root` opcode (Sinsemilla,
depth 32) proves inclusion. Only the Merkle root is exposed as a public input.

Every state transition follows the append-only + nullifier model:

1. The old state leaf is proven to exist via Merkle inclusion proof
2. A nullifier `poseidon_hash(DOMAIN_NULLIFIER, owner_secret, box_id, old_state_nonce)`
   consumes the old state
3. A new leaf representing the new state is appended to the Merkle tree

An observer cannot determine which box is being operated on, who owns it,
whether two operations target the same box, or what capability is inside.

### Ownership Proof

Poseidon-based: `derived_owner = poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)`
constrained against `owner_pub` in the ZK witness. Neither value is a public input.
This is the o-cap authorization model — possession of the secret IS authority.

## Data Model

Each state transition produces a Merkle leaf:

```
box_leaf = poseidon_hash(DOMAIN_SIGNATURE_SECRET, box_id, contents_commit, state_nonce)
nullifier = poseidon_hash(DOMAIN_NULLIFIER, owner_secret, box_id, old_state_nonce)
```

The box tree is an append-only BridgeTree (depth 32, Sinsemilla hash). Each
Put/Take appends one leaf. Nullifiers are stored in a Poseidon Sparse Merkle
Tree (depth 255) for replay prevention.

## Database Trees

| Tree | Purpose |
|------|---------|
| `boxes` | Box records |
| `nullifiers` | Spent nullifiers (SMT) |
| `info` | Contract metadata, Merkle tree data, root pointers |
| `box_roots` | Historical Merkle roots |
| `nullifier_roots` | Historical nullifier SMT roots |

## Composing Contracts

Contracts compose with Box to delegate capabilities without revealing what is
being transferred. Box is a genesis primitive — deployed once at genesis
(counter 9) and every contract calls it as a child.

| Contract | What the Box Delegates | Child Calls |
|----------|----------------------|-------------|
| [escrow](escrow.md) | Seller claim authority, buyer refund authority | Take on Claim, Take on Refund |
| [drain_protection](drain_protection.md) | Spend authority, proposal rights, vote rights | Take on Propose/Vote/Transfer |
| [subscription](subscription.md) | Subscription capability (READ/WRITE/CANCEL/RENEW/ADMIN) | Take on VerifyAccess |
| [dao_escrow](dao_escrow.md) | Four governance roles: member_vote, board_treasury, board_endowment, dispute_arbitrator | Take on Propose/Vote/TreasurySpend/EndowmentWithdraw/DisputeResolve |

## References

- [Object Capability Model](../arch/ocap.md) — Box in the O-Cap stack
- [Privacy Model](../arch/privacy.md) — L1 vs L2
- [Contract WASM Type System](../arch/contract-wasm-type-system.md) — Encoding boundaries
- [Wallet Architecture](../arch/wallet.md) — How the wallet interacts with Boxes
- Source: `src/contract/box/`
