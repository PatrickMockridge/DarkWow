# Purse — ZK Fungible Capability Container (L1)

## The Capability

Purse is the capability to **hold fungible value** — the ZK-native equivalent of
Agoric's ERTP Purse. In L1, the Purse is not a persistent balance account: each
operation is a **consume+create** — the old purse state is nullified and a new
state leaf is appended to the Merkle tree. The `purse_id` binds state transitions
in the ZK witness but is never exposed as a public input. The balance amount is
hidden in a Pedersen commitment; conservation is proven via additive
homomorphism in the circuit.

**Trust tier:** ecosystem infrastructure (genesis counter 8). Not
consensus-critical.

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | `initialize` | — | Genesis initialization (seeds trees, stores circuits) |
| `0x01` | `deposit` | `Deposit` | Deposit fungible value — consumes old state, creates new Merkle leaf |
| `0x02` | `withdraw` | `Withdraw` | Withdraw fungible value — enforces bounds, consumes old state |
| `0x03` | `balance` | `Balance` | Prove balance without consuming — read-only Merkle inclusion |

## Barbs

### `deposit`

| Barb | Mechanism |
|------|-----------|
| `↓spend` | `owner_pub == poseidon_hash(7, owner_secret)` (`DOMAIN_SIGNATURE_SECRET = witness_base(7)`) |
| `↓nullify` | `nullifier == poseidon_hash(1, owner_secret, purse_id, state_nonce)` (`DOMAIN_NULLIFIER = witness_base(1)`) |
| `↓prove-inclusion` | `merkle_root(leaf_pos, path, old_leaf) == expected_root` where `old_leaf = poseidon_hash(5, purse_id, old_balance, state_nonce)` (`DOMAIN_MERKLE_LEAF = witness_base(5)`) |
| `↓conserve` | Pedersen homomorphism: `old_commit + deposit_commit == new_commit` (point equality) |
| `↓commit` | `new_leaf == poseidon_hash(5, purse_id, new_balance, state_nonce)`; `new_balance == old_balance + deposit_amount`; Apply appends via `merkle_add` + marks nullifier |

### `withdraw`

Same as `deposit`, with the conservation direction reversed, plus:

| `↓bound` | `withdraw_amount > 0` and `withdraw_amount <= old_balance`; `new_balance == old_balance - withdraw_amount` |
| `↓conserve` | `new_commit + withdraw_commit == old_commit` |

### `balance`

| Barb | Mechanism |
|------|-----------|
| `↓spend` | `owner_pub == poseidon_hash(7, owner_secret)` |
| `↓prove-inclusion` | `purse_leaf = poseidon_hash(5, purse_id, balance, state_nonce)`; merkle root == `expected_root` |
| `↓denominate` | `derived_purse_id == poseidon_hash(4, owner_pub, token_id, purse_id)` (`DOMAIN_COIN_COMMIT = witness_base(4)`); `token_commit == poseidon_hash(2, token_id, token_blind)` (`DOMAIN_TOK_COMMIT = witness_base(2)`) |

`balance` is read-only — no nullifier, no consumption.

Every circuit binds the transaction: `tx_binding == poseidon_hash(3, tx_commitment, tx_nonce)` (`DOMAIN_TX_BINDING = witness_base(3)`).

## The Four-Component Flow

1. **Circuit** (`deposit.zk` / `withdraw.zk` / `balance.zk`) — computes
   cryptographic values and constrains them equal to caller-provided witnesses.
2. **Params** (`DepositParams` / `WithdrawParams` / `BalanceParams`) — the caller
   pre-computes every `constrain_instance` field (nullifier, `expected_root`,
   Pedersen coordinates, `new_leaf`, `tx_binding`).
3. **Metadata** (`get_metadata`) — pure echo; extracts the `constrain_instance`
   values from params in circuit order.
4. **Exec** (`process_instruction`) — validates the nullifier is unspent (Deposit/
   Withdraw) and the `expected_root` exists in `purse_roots` (skipped on the first,
   EMPTY-root operation). Balance has no nullifier check. **Apply**
   (`process_update`) — writes only: `merkle_add` + `db_mark_spent` (Deposit/Withdraw);
   Balance is a no-op write.

## Data Model

```
owner_pub       = poseidon_hash(7, owner_secret)                         # DOMAIN_SIGNATURE_SECRET
nullifier       = poseidon_hash(1, owner_secret, purse_id, state_nonce)  # DOMAIN_NULLIFIER
purse_leaf      = poseidon_hash(5, purse_id, balance, state_nonce)       # DOMAIN_MERKLE_LEAF
token_commit    = poseidon_hash(2, token_id, token_blind)                # DOMAIN_TOK_COMMIT
derived_purse_id = poseidon_hash(4, owner_pub, token_id, purse_id)       # DOMAIN_COIN_COMMIT (Balance)
balance_commit  = pedersen_commit(balance, balance_blind)                # Pedersen (V, R)
tx_binding      = poseidon_hash(3, tx_commitment, tx_nonce)              # DOMAIN_TX_BINDING
```

## State Trees

| Tree | Purpose |
|------|---------|
| `nullifiers` | Spent nullifier records |
| `purse_roots` | Historical Merkle roots for inclusion proofs |
| `info` | Merkle tree state and root pointers |

## Capabilities & Actions

| Capability | Discriminant | Primitives | Note schema |
|------------|--------------|------------|-------------|
| `purse_capability` | `0` | `SecretKey`, `Commitment`, `Nullifier`, `MerkleNode`, `ContractId`, `FuncId`, `AssetId` | `{ token_id: pallas_base, balance: u64, commitment: pallas_base }` |

| Action | Requires | Consumes | Produces | Barbs |
|--------|----------|----------|----------|-------|
| `deposit` | — | `purse_capability` | `purse_capability` | `Spend, Nullify, ProveInclusion, Commit, Dispatch, Gate` |
| `withdraw` | — | `purse_capability` | `purse_capability` | `Spend, Nullify, ProveInclusion, Commit, Dispatch, Gate` |
| `balance` | `any(purse_capability)` | — | — | `Spend, ProveInclusion, Dispatch, Gate` |

## Authorization

`deposit`/`withdraw` consume and re-produce a `purse_capability`
(`x?(old).νnew.(y!(new) | ...)` — the balance rolls forward). `balance` observes
the capability without consuming it (`x?(y).observe!(y)`). Authority is the
`owner_secret`: the holder who can produce a valid `owner_pub` and nullifier for
`purse_id` holds the purse. Conservation is enforced in-circuit by the Pedersen
homomorphism, so a holder cannot create or destroy balance.

## References

- [Purse Specification](../../../doc/src/contract/purse.md)
- [Contract Manifest](../../../doc/src/arch/manifest.md)
- [Contract WASM Type System](../../../doc/src/arch/contract-wasm-type-system.md) — Part A
- [Type System](../../../doc/src/arch/type-system.md)
- [Privacy Model](../../../doc/src/arch/privacy.md) — L1 consume+create
- [Box](box.md) — single-capability container
- Source: `src/contract/purse/`
