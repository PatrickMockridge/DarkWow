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
| `↓denominate` | Circuit constrains `token_commit == poseidon_hash(DOMAIN_TOK_COMMIT, asset_id, token_blind)` |
| `↓conserve` | Circuit constrains Pedersen homomorphism: `old_commit + deposit_commit == new_commit` |
| `↓commit` | Apply appends new leaf to Merkle tree, marks nullifier |

### Withdraw
Same as Deposit plus:
| `↓bound` | Circuit constrains `withdraw_amount > 0` and `withdraw_amount <= old_balance` |

### Balance
| `↓prove-inclusion` | Circuit constrains Merkle inclusion of current state |
No `↓nullify` — read-only operation.

## The Four-Component Flow

Identical structure to Box (see box.md for full description):

1. **Circuit**: All `constrain_instance` values are caller-provided witnesses.
2. **Params**: Every constrain_instance position maps to a field. Caller pre-computes
   nullifier, expected_root, Pedersen commitment coordinates, derived IDs.
3. **Metadata**: Pure echo — `params.field` only.
4. **Exec**: Nullifier check. **Apply**: merkle_add, db_set.

## Data Model

```
purse_leaf  = poseidon_hash(DOMAIN_SIGNATURE_SECRET, purse_id, balance, state_nonce)
nullifier   = poseidon_hash(DOMAIN_NULLIFIER, owner_secret, purse_id, state_nonce)
owner_pub    = poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)
balance_commit = pedersen_commit(balance, balance_blind)
```

## Database Trees

| Tree | Purpose |
|------|---------|
| `purses` | Purse records |
| `nullifiers` | Spent nullifiers (flat DB) |
| `info` | Merkle tree data, root pointers |
| `purse_roots` | Historical Merkle roots |

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
- [Safety](../dev/contracts/safety.md) — Lesson 22: four-component architecture
- [Box](box.md) — Single-capability container
- Source: `src/contract/purse/`
