# Box — ZK Capability Container

The Box contract is the DarkWow equivalent of capability delegation — put a
capability into a Box, and whoever Takes it receives it. It is an **O-Cap
primitive** deployed at genesis (ContractId counter 9).

## Why Genesis?

In the Agoric o-cap model, Invitations are Payments that grant access to a
specific contract interaction. Box is the ZK-native equivalent: it holds an
arbitrary capability and transfers it via linear consumption (nullifier).
Having a canonical well-known ContractId makes Box available as a composable
primitive for every contract in the ecosystem — any contract can deposit a
capability into a Box, and any wallet can Take from a Box it holds the secret for.

## Operations

| Operation | Opcode | Circuit | Privacy Path | What It Proves |
|-----------|--------|---------|-------------|---------------|
| `InitializeV1` | 0x00 | — | — | Initialize the Box contract (genesis primitive) |
| `PutV1` | 0x01 | `put_v1.zk` | Proven | Box was empty, now contains H(capability_data). Ownership via `ec_mul_base(secret, NULLIFIER_K)` key derivation. box_id exposed as public input. |
| `TakeV1` | 0x02 | `take_v1.zk` | Proven | Knowledge of box_secret. Box consumed via nullifier. Contents not empty. box_id exposed as public input. |
| `PutV3` | 0x03 | `put_v3.zk` | **Hard** | Merkle inclusion of old box state. Ownership via `poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)` constrained against owner_pub in witness. Nullifier `poseidon_hash(DOMAIN_NULLIFIER, owner_secret, box_id, old_state_nonce)` consumes old state. New contents commitment appended to Merkle tree. Observer sees only `[nullifier, merkle_root, new_contents_commit, tx_binding, tx_nonce]`. |
| `TakeV3` | 0x04 | `take_v3.zk` | **Hard** | Merkle inclusion of current box state. Same Poseidon-based ownership proof. Nullifier consumes state. Observer sees only `[nullifier, merkle_root, tx_binding, tx_nonce]`. |

## Privacy Model

### Proven Path (V1)

The V1 circuits expose `box_id` as a ZK public input. An observer can see
WHICH box is being operated on (though not by whom or with what contents).
This is the proven path per privacy.md §2 — proven hardness without the
extra dimension of Merkle inclusion constraint.

### Hard Path (V3)

The V3 circuits move `box_id` into the ZK witness and bind it into a Merkle
leaf: `poseidon_hash(DOMAIN_SIGNATURE_SECRET, box_id, contents_commit, state_nonce)`.
The `merkle_root` opcode (Sinsemilla, depth 32) proves inclusion of this leaf
in the box state tree. Only the Merkle root is exposed as a public input.

Every state transition (Put or Take) follows the append-only + nullifier
model per ocap.md §6.2:

1. The old state leaf is proven to exist in the Merkle tree via inclusion proof
2. A nullifier `poseidon_hash(DOMAIN_NULLIFIER, owner_secret, box_id, old_state_nonce)`
   is published, consuming the old state
3. A new leaf representing the new state is appended to the Merkle tree

An observer cannot determine:
- Which box is being operated on (box identity is in the Merkle leaf hash, hidden behind the root)
- Who owns the box (owner_pub is constrained in-witness, never exposed)
- Whether two operations target the same box (nullifiers are unlinkable)
- What capability is inside the box (contents_commit is a Poseidon hash)

## Data Model

### V1 (Proven Path)

```
BoxRecord = {
    version:          u8,
    box_id:           Poseidon hash of creator public key and nonce,
    contents_commit:  Poseidon hash of capability data,
    is_empty:         bool,
}
```

Stored in flat DB `"boxes"` keyed by `box_id.to_bytes()`.

### V3 (Hard Path)

Each box state transition produces a Merkle leaf:

```
box_leaf = poseidon_hash(DOMAIN_SIGNATURE_SECRET, box_id, contents_commit, state_nonce)
nullifier = poseidon_hash(DOMAIN_NULLIFIER, owner_secret, box_id, old_state_nonce)
```

The box tree is an append-only BridgeTree (depth 32, Sinsemilla hash). Each
Put/Take appends one leaf. Nullifiers are stored in a Poseidon Sparse Merkle
Tree (depth 255) for replay prevention.

## Database Trees

| Tree | Purpose | V1 | V3 |
|------|---------|----|-----|
| `boxes` | Box records keyed by box_id | ✓ | ✓ |
| `nullifiers` | Spent nullifiers (SMT in V3) | ✓ | ✓ |
| `info` | Contract metadata, Merkle tree data, root pointers | ✓ | ✓ |
| `box_roots` | Historical Merkle roots | — | ✓ |
| `nullifier_roots` | Historical nullifier SMT roots | — | ✓ |

## Ownership Proof

V3 uses Poseidon-based ownership: `derived_owner = poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)`
constrained against `owner_pub` in the ZK witness. Neither value is exposed
as a public input. This is the o-cap authorization model — possession of the
secret IS authority.

V1 uses EC key derivation: `owner_pub = ec_mul_base(owner_secret, NULLIFIER_K)`.
This proves knowledge of the secret through the discrete log relation but
requires an EC operation in the circuit.

## Circuit Version Management

| Version | Purpose | Registered |
|---------|---------|-----------|
| V1 | Baseline (proven path) | ✓ |
| V2 | Domain-separated Poseidon hashes (HAZOP RC3) | ✓ |
| V3 | Hard path — Merkle inclusion + nullifier per state transition | ✓ |

## Composing Contracts

Four contracts compose with Box, using it to delegate capabilities without revealing
what is being transferred. Box is a genesis primitive — deployed once at genesis
(counter 9) and every contract calls it as a child.

| Contract | What the Box Delegates | Child Calls |
|----------|----------------------|-------------|
| [escrow](escrow.md) | Seller claim authority, buyer refund authority | TakeV1 on Claim, TakeV1 on Refund |
| [drain_protection](drain_protection.md) | Spend authority, proposal rights, vote rights | TakeV1 on Propose/Vote/Transfer |
| [subscription](subscription.md) | Subscription capability (READ/WRITE/CANCEL/RENEW/ADMIN) | TakeV1 on VerifyAccess |
| [dao_escrow](dao_escrow.md) | Four governance roles: member_vote, board_treasury, board_endowment, dispute_arbitrator | TakeV1 on Propose/Vote/TreasurySpend/EndowmentWithdraw/DisputeResolve |

## References

- [Object Capability Model](../arch/ocap.md) — Box in the O-Cap stack
- [Privacy Model](../arch/privacy.md) — Hard path vs proven path
- [Contract WASM Type System](../arch/contract-wasm-type-system.md) — Encoding boundaries, error propagation
- [Wallet Architecture](../arch/wallet.md) — How the wallet interacts with Boxes
- Source: `src/contract/box/`
