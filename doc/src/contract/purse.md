# Purse — ZK Fungible Capability Container (L1)

The Purse contract is the DarkWow equivalent of Agoric's ERTP Purse — the
primitive that holds fungible capabilities (tokens, budget allocations, treasury
shares). It is an **O-Cap primitive** deployed at genesis (ContractId counter 8).
Purse is L1: resource IDs are in the ZK witness, Merkle inclusion proofs hide
which purse is being operated on.

## Why Genesis?

In the o-cap model, every principal — wallet, DAO, contract, budget, treasury —
holds capabilities. A fungible capability (an amount of tokens) is held in a
Purse. A Purse can belong to a wallet (personal balance), a DAO (treasury), a
contract (escrow), or a budget (allocated funds). It is the fungible analogue
of Box (which holds a single transferable capability).

## Operations

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `Initialize` | 0x00 | — | Initialize the Purse contract (genesis primitive) |
| `Deposit` | 0x01 | `deposit.zk` | Merkle inclusion of old purse state + Pedersen conservation (old + deposit == new) + nullifier. purse_id in ZK witness. |
| `Withdraw` | 0x02 | `withdraw.zk` | Merkle inclusion + withdrawal bounds (amount > 0, amount <= balance) + Pedersen conservation + nullifier. purse_id in ZK witness. |
| `Balance` | 0x03 | `balance.zk` | Merkle inclusion of current purse state. Read-only — no nullifier, no state change. |

## Privacy Model

Purse is L1 — full privacy via Merkle inclusion proofs. The resource identity
(`purse_id`) is bound into a Merkle leaf: `poseidon_hash(DOMAIN_SIGNATURE_SECRET,
purse_id, balance, state_nonce)`. The `merkle_root` opcode proves inclusion.
Only the Merkle root is exposed as a public input.

Every state transition (Deposit or Withdraw) follows the append-only + nullifier
model:

1. The old state leaf is proven to exist via Merkle inclusion proof
2. A nullifier `poseidon_hash(DOMAIN_NULLIFIER, owner_secret, purse_id, state_nonce)`
   consumes the old state
3. A new leaf with the updated balance is appended to the Merkle tree

Balance conservation is proven via Pedersen additive homomorphism in-circuit:
`old_commit + delta_commit == new_commit`.

The Balance operation is read-only — it proves Merkle inclusion of the current
state without consuming it (no nullifier).

An observer cannot determine which purse is being operated on, the balance amount
(hidden in Pedersen commitment), the token type (hidden in Poseidon commitment),
or who owns the purse.

### Ownership Proof

Poseidon-based: `derived_owner = poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)`
constrained against `owner_pub` in the ZK witness. Neither value is a public input.

## Data Model

Each state transition produces a Merkle leaf:

```
purse_leaf = poseidon_hash(DOMAIN_SIGNATURE_SECRET, purse_id, balance, state_nonce)
nullifier = poseidon_hash(DOMAIN_NULLIFIER, owner_secret, purse_id, state_nonce)
```

The purse tree is an append-only BridgeTree (depth 32, Sinsemilla hash).
Nullifiers are stored in a Poseidon Sparse Merkle Tree (depth 255).

## Database Trees

| Tree | Purpose |
|------|---------|
| `purses` | Purse records |
| `nullifiers` | Spent nullifiers (SMT) |
| `info` | Contract metadata, Merkle tree data, root pointers |
| `purse_roots` | Historical Merkle roots |
| `nullifier_roots` | Historical nullifier SMT roots |

## Composing Contracts

Contracts compose with Purse to replace manual `u64` arithmetic on aggregate
counters. Purse is a genesis primitive — deployed once at genesis (counter 8)
and every contract calls it as a child.

| Contract | What the Purse Tracks | Child Calls |
|----------|----------------------|-------------|
| [escrow](escrow.md) | Locked escrow funds | Deposit on Fund |
| [drain_protection](drain_protection.md) | Protected fund total | Deposit/Withdraw on Transfer |
| [dao_escrow](dao_escrow.md) | Treasury, pool, and endowment balances | Deposit on PayPremium, Withdraw on TreasurySpend/EndowmentWithdraw |
| [subscription](subscription.md) | Subscription deposit | Deposit on Subscribe, Withdraw on Cancel |
| [pool_stake](pool_stake.md) | Pool total, member stakes, coverage | Deposit on Join, Withdraw on Slash |
| [betting_stake](betting_stake.md) | Table pool, staker positions, earnings | Deposit on Stake, Withdraw on Unstake |
| [relayer_endowment](relayer_endowment.md) | Deployed capital, per-deployment fees | Deposit on Deploy, Withdraw on Settle |
| [labor_market](labor_market.md) | Job payment escrow | Deposit on CreateJob, Withdraw on ConfirmDelivery |
| [bridge](bridge.md) | Total deposited, total withdrawn | Deposit on Deposit, Withdraw on Withdraw |
| [stablecoin](stablecoin.md) | Total debt, total collateral, fees | Deposit/Withdraw on Mint/Repay/Liquidate |

## References

- [Object Capability Model](../arch/ocap.md) — Purse in the O-Cap stack
- [Privacy Model](../arch/privacy.md) — L1 vs L2
- [Contract WASM Type System](../arch/contract-wasm-type-system.md) — Encoding boundaries
- [Box](box.md) — The single-capability container (non-fungible analogue of Purse)
- Source: `src/contract/purse/`
