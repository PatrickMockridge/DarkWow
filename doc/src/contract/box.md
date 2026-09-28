# Box — ZK Capability Container (L1)

## The Primitive Capability

Box is the capability to delegate — the ZK-native equivalent of Agoric's
Invitation. It holds an arbitrary capability and transfers it via linear
consumption. In L1, the Box is not a persistent mutable container. Each
operation is a consume+create: the old box state is nullified and a new
state leaf is appended to the Merkle tree. The box_id binds state transitions
in the ZK witness but is never exposed as a public input.

## Operations

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `Initialize` | 0x00 | — | Genesis initialization |
| `Put` | 0x01 | `put.zk` | Poseidon ownership proof. Nullifier consumes old state. Merkle inclusion of old state. New state commitment appended. |
| `Take` | 0x02 | `take.zk` | Same as Put. Nullifier consumes current state. |

## Barbs

### Put
| Barb | Mechanism |
|------|-----------|
| `↓spend` | Circuit constrains `owner_pub == poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)` |
| `↓nullify` | Circuit constrains `nullifier == poseidon_hash(DOMAIN_NULLIFIER, owner_secret, box_id, old_state_nonce)` |
| `↓prove-inclusion` | Circuit constrains `merkle_root(leaf_pos, path, leaf) == expected_root` where `leaf = poseidon_hash(DOMAIN_MERKLE_LEAF, box_id, old_contents_commit, old_state_nonce, owner_pub)` |
| `↓commit` | Apply appends new leaf to Merkle tree, marks nullifier in DB |

### Take
Same barbs as Put.

## The Four-Component Flow

Every operation follows the same architectural pattern:

1. **Circuit** (`put.zk` / `take.zk`): All `constrain_instance` values are
   caller-provided witnesses. The circuit computes cryptographic values and
   constrains them equal to the witnesses via `constrain_equal_base`.

2. **Params** (`PutParams` / `TakeParams`): Every `constrain_instance` position
   maps to a field. The caller pre-computes all circuit-derived values
   (nullifier, expected_root, tx_binding) with matching domain constants.
   **What the circuit needs but a verifier does not is not a param.** `box_id` and
   `state_nonce` were, until 2026-09-26, and are now `note:` witness sources served from
   the wallet's own record — §2 promises an observer learns "not which resource was
   operated on", §5.5 says `box_id` "is never a public input", and a value in `Call.data`
   is plaintext committed to by the transaction hash. `scripts/check-l1-wire-conformance.sh`
   holds the rule and declares the residue: the two contents commitments, and
   `new_state_nonce`, which the *prover* must supply because the circuit constrains it to
   `old + 1` and no `note:` field yields a successor.

3. **Metadata** (`get_metadata`): Pure echo — reads `params.field` directly.
   No domain constants, no poseidon_hash, no computation.

4. **Exec** (`process_instruction`): Validates nullifier unspent via
   `db_contains_key`. **Apply** (`process_update`): Appends leaf to Merkle
   tree via `merkle_add`, marks nullifier via `db_set`.

## Data Model

```
box_leaf  = poseidon_hash(DOMAIN_MERKLE_LEAF, box_id, contents_commit, state_nonce, owner_pub)
nullifier = poseidon_hash(DOMAIN_NULLIFIER, owner_secret, box_id, state_nonce)
owner_pub = poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)
```

## The Wire — what a call publishes

`privacy.md` §2 promises that an L1 observer sees "only a nullifier and a Merkle root — not which
resource was operated on, not by whom, not what's inside", §5.5 says `box_id` "is never a public
input", and §2.4's table has Box hiding "which box … in Poseidon commitment". A value in `Call.data`
is none of those things: it is plaintext, and the transaction hash commits to every byte of it. So the
params are the public inputs and nothing else, and this table is the contract of that:

| Call | Carries | Does **not** carry | Header |
|---|---|---|---|
| `Put` | `nullifier`, `expected_root`, `new_leaf`, the four commitment coordinates, `tx_binding`, `tx_nonce`, `leaf_pos`, `merkle_path`, `proof` | `box_id`, `old_state_nonce`, `new_state_nonce`, `old_contents_commit`, **`new_contents_commit`** | 100 bytes |
| `Take` | `nullifier`, `expected_root`, `tx_binding`, `tx_nonce`, `leaf_pos`, `merkle_path`, `proof` | `box_id`, `state_nonce`, **`contents_commit`** | 68 bytes |

**Nothing is declared as still on the wire for either call** — this contract's entries in
`scripts/check-l1-wire-conformance.sh` are gone, and that gate fails if a declaration goes stale, so
the count is the measure rather than the claim.

**How the removed values reach the circuit.** Three routes:

* `box_id`, `state_nonce` and the *consumed* box's contents (`Put`'s `old_contents_commit`, `Take`'s
  `contents_commit`) are `note:` witness sources served from the wallet's own record —
  `CapRecord.object_id`, `.state_nonce`, `.user_data`, mapped in `bin/dww/src/lib.rs`'s
  `cap_record_note_fields` and read out of the note by `bin/dww/src/scan.rs`.
* `new_state_nonce` is `derived:increment:1` — `put.zk:63-64` computes `base_add(old_state_nonce, ONE)`
  and constrains it, so the successor is the circuit's own value and needs no caller input. The reason
  this page used to give for publishing it — *"the prover must supply it … where Purse's circuit
  derives its own"* — was false, and `Purse` was the contract that showed it.
* **`new_contents_commit` is a parameter that is `off_wire`** — the caller chooses what the box will
  hold, the prover binds it, and `encode_params_values` does not write it. Before that tag existed the
  only way to publish it was the only way to supply it, which is why it was on the wire.

**The note now carries what the box holds** — as the **commitment**, never a preimage. `Put` emits
`{commitment, state_nonce, box_id, user_data}` encrypted to the holder, where `user_data` is the
contents commitment the circuit folded into the leaf; `cap_record_note_fields` and `scan.rs` map it to
`CapRecord.user_data`, which is the existing, persisted slot. **This is what makes a box legible to its
own holder**, and it is a separate thing from the wire: a box's contents were never published after
unit 4, and were never *available* either — opaque to observers, which is right, and to their holder,
which is not. The box layer never receives a preimage, so it cannot leak one; the preimage of an
application's commitment belongs to that application.

**And one gap this page cannot paper over**: §C.8.2 requires an L1 note to carry `nullifier`,
`merkle_root` and `leaf_position` for trajectory identification, and Box's schema carries none of them.
That is `OBL-C181`, it is a different gap from the one above, and closing one does not close the other.

## Database Trees

| Tree | Purpose |
|------|---------|
| `nullifiers` | Spent nullifiers (flat DB) |
| `info` | Merkle tree data, root pointers |
| `box_roots` | Historical Merkle roots |

## Circuit Version

Box is L1. There is one circuit per operation — no version suffixes.
Circuits use domain-separated Poseidon hashes (HAZOP RC3).

## Composing Contracts

| Contract | What the Box Delegates |
|----------|----------------------|
| [escrow](escrow.md) | Seller claim authority, buyer refund authority |
| [drain_protection](drain_protection.md) | Spend authority, proposal rights, vote rights |
| [subscription](subscription.md) | Subscription capability |
| [dao_escrow](dao_escrow.md) | Governance roles |

## References

- [Privacy Model](../arch/privacy.md) — L1/L2, consume+create model, architectural principles
- [Contract WASM Type System](../arch/contract-wasm-type-system.md) — Four encoding boundaries
- [O-Cap Model](../arch/ocap.md) — Box in the O-Cap stack
- [Safety](../dev/contracts/safety.md) — the four-component L1 operation architecture
- Source: `src/contract/box/`
