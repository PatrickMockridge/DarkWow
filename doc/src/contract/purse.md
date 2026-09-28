# Purse — ZK Fungible Capability Container (L1)

## The Primitive Capability

Purse is the capability to hold fungible value — the ZK-native equivalent of
Agoric's ERTP Purse. In L1, the Purse is not a persistent balance account.
Each operation is a consume+create: the old purse state is nullified and a
new state leaf is appended to the Merkle tree. The purse_id binds state
transitions in the ZK witness but is never exposed as a public input.
The balance amount is hidden in a Pedersen commitment; conservation is
proven via additive homomorphism in the circuit.

## Operations

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `Initialize` | 0x00 | — | Genesis initialization |
| `Deposit` | 0x01 | `deposit.zk` | Poseidon ownership. Nullifier consumes old state. Merkle inclusion of old state. Pedersen conservation: `old_commit + deposit_commit == new_commit`. |
| `Withdraw` | 0x02 | `withdraw.zk` | Same as Deposit plus: `withdraw_amount > 0`, `withdraw_amount <= old_balance`. |
| `Balance` | 0x03 | `balance.zk` | Merkle inclusion of current state. Read-only — no nullifier, no consumption. |

## Barbs

### Deposit
| Barb | Mechanism |
|------|-----------|
| `↓spend` | Circuit constrains `owner_pub == poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)` |
| `↓nullify` | Circuit constrains `nullifier == poseidon_hash(DOMAIN_NULLIFIER, owner_secret, purse_id, state_nonce)` |
| `↓prove-inclusion` | Circuit constrains `merkle_root(leaf_pos, path, leaf) == expected_root` |
| `↓conserve` | Circuit constrains Pedersen homomorphism: `old_commit + deposit_commit == new_commit` |
| `↓commit` | Apply appends new leaf to Merkle tree, marks nullifier |

### Withdraw
Same as Deposit plus:
| `↓bound` | Circuit constrains `withdraw_amount > 0` and `withdraw_amount <= old_balance` |

### Balance
| Barb | Mechanism |
|------|-----------|
| `↓prove-inclusion` | Circuit constrains Merkle inclusion of current state |
| `↓denominate` | Circuit constrains `token_commit == poseidon_hash(DOMAIN_TOK_COMMIT, asset_id, token_blind)` (`balance.zk:66-68`) |

No `↓nullify` — read-only operation. `↓denominate` is listed here rather than under Deposit because
`balance.zk` is the only Purse circuit that derives `token_commit`; `deposit.zk` and `withdraw.zk` carry
no `DOMAIN_TOK_COMMIT` and no `token_blind`, and the manifest requires no `Denominate` barb on any
operation (`purse/manifest.toml`, `[[actions]]`).

## The Four-Component Flow

Identical structure to Box (see box.md for full description):

1. **Circuit**: All `constrain_instance` values are caller-provided witnesses.
2. **Params**: Every constrain_instance position maps to a field. Caller pre-computes
   nullifier, expected_root, Pedersen commitment coordinates, derived IDs.
   **What the circuit needs but a verifier does not is not a param.** `purse_id` and
   `state_nonce` were, until 2026-09-26, and are now `note:` witness sources served from
   the wallet's own record — `privacy.md` §2 promises an observer learns "not which
   resource was operated on" and §5.5 says the object id "is never a public input", so a
   value in the call data (which the transaction hash commits to byte-for-byte) is a leak
   with no verifier on the other side of it. `scripts/check-l1-wire-conformance.sh` holds
   the rule and declares the residue — the balances, blocked on the note's `value` type,
   which `encode_params_values` requires to be `u64` while a witness slot is a field element.
3. **Metadata**: Pure echo — `params.field` only.
4. **Exec**: Nullifier check. **Apply**: merkle_add, db_set.

## Data Model

```
purse_leaf  = poseidon_hash(DOMAIN_MERKLE_LEAF, purse_id, balance, state_nonce, owner_pub)
nullifier   = poseidon_hash(DOMAIN_NULLIFIER, owner_secret, purse_id, state_nonce)
owner_pub    = poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)
balance_commit = pedersen_commit(balance, balance_blind)
```

## The Wire — what a call publishes

`privacy.md` §2 promises that an L1 observer sees "only a nullifier and a Merkle root — not which
resource was operated on, not by whom, not how much", §2.4's table has Purse hiding the balance "in
Pedersen commitment … not which purse or how much", and §5.5 says the object id "is never a public
input". A value in `Call.data` is plaintext, committed to byte-for-byte by the transaction hash, so the
params are the public inputs and the values a *note* is built from — and nothing else:

| Call | Carries | Does **not** carry | Header |
|---|---|---|---|
| `Deposit` | `nullifier`, `expected_root`, `new_leaf`, the four commitment coordinates, `tx_binding`, `tx_nonce`, `leaf_pos`, `merkle_path`, `proof`, `derived_purse_id` | `purse_id`, `state_nonce`, `old_balance`, `new_balance`, `asset_id`, **`deposit_amount`** | 228 bytes |
| `Withdraw` | as `Deposit` | `purse_id`, `state_nonce`, `old_balance`, `new_balance`, `asset_id`, **`withdraw_amount`** | 228 bytes |
| `Balance` | `derived_purse_id`, `expected_root`, `token_commit`, the balance commitment coordinates, `tx_binding`, `tx_nonce`, `leaf_pos`, `merkle_path`, `proof` | `purse_id`, `asset_id`, `balance`, `state_nonce` | 164 bytes |

**Nothing is declared as still on the wire for `Deposit` or `Withdraw`** — this contract's entries in
`scripts/check-l1-wire-conformance.sh` are gone, and that gate fails if a declaration goes stale, so the
count is the measure rather than the claim.

**How each value reaches the circuit.** Three routes, and the difference between them is the point:

* `purse_id`, `state_nonce`, `asset_id`, `old_balance` are `note:` witness sources served from the
  wallet's own record — `CapRecord.object_id`, `.state_nonce`, `.asset_id`, `.value`, mapped in
  `bin/dww/src/lib.rs`'s `cap_record_note_fields` and read out of the note by `bin/dww/src/scan.rs`.
  The note carries `value` as a `pallas_base` the circuit computed, and the scan converts it to the
  record's `u64` **with a check** — an absent field is zero, an unreadable one refuses the note.
* `new_balance` is `derived:base_add:1,3` (deposit) / `derived:base_sub:1,3` (withdraw): the circuit's
  own arithmetic applied to `old_balance` and the amount, so the generic prover computes what
  `deposit.zk:94-95` constrains.
* **`deposit_amount`/`withdraw_amount` are parameters that are `off_wire`** — the caller supplies them,
  the prover binds them, and `encode_params_values` does not write them. There was no way to say this
  before 2026-09-28, which is the single reason the balances were published for so long: the value that
  had to leave the wire was the one the *caller* chooses, and `witness = N` means "the caller need not
  supply it", not "do not publish it" (`OBL-C179`).

**And the §C.8.2 gap is open here too**: an L1 note SHALL carry `nullifier`, `merkle_root` and
`leaf_position`, and this schema carries none of them. That is `OBL-C181`, it is a *different* gap from
the one this section closes, and closing one does not close the other.

## Database Trees

| Tree | Purpose |
|------|---------|
| `nullifiers` | Spent nullifiers (flat DB) |
| `info` | Merkle tree data, root pointers |
| `purse_roots` | Historical Merkle roots |

These three are what `init_contract` creates (`purse/src/entrypoint/mod.rs:24-27`) and what the manifest
declares; there is no `purses` tree. The `Purse { version, purse_id, token_commit, balance_commit,
owner_commit }` struct in `purse/src/model/mod.rs:108` is not persisted by any entrypoint — its own doc
comment says so — and `apply` writes only the Merkle leaf, the nullifier, and the block anchor.

## Circuit Version

Purse is L1. One circuit per operation — no version suffixes.
Domain-separated Poseidon hashes throughout.

## Composing Contracts

| Contract | What the Purse Tracks |
|----------|----------------------|
| [escrow](escrow.md) | Locked escrow funds |
| [drain_protection](drain_protection.md) | Protected fund total |
| [dao_escrow](dao_escrow.md) | Treasury, pool, endowment balances |
| [subscription](subscription.md) | Subscription deposit |
| [pool_stake](pool_stake.md) | Pool total, member stakes |
| [bridge](bridge.md) | Total deposited, total withdrawn |
| [stablecoin](stablecoin.md) | Total debt, collateral, fees |

## References

- [Privacy Model](../arch/privacy.md) — L1/L2, consume+create model, architectural principles
- [Contract WASM Type System](../arch/contract-wasm-type-system.md) — Four encoding boundaries
- [O-Cap Model](../arch/ocap.md) — Purse in the O-Cap stack
- [Safety](../dev/contracts/safety.md) — the four-component L1 operation architecture
- [Box](box.md) — Single-capability container
- Source: `src/contract/purse/`
