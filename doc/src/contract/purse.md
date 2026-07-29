# Purse — ZK Fungible Capability Container

The Purse contract is the DarkWow equivalent of Agoric's ERTP Purse — the
primitive that holds fungible capabilities (tokens, budget allocations, treasury
shares). It is an **O-Cap primitive** deployed at genesis (ContractId counter 8).

## Why Genesis?

In the o-cap model, every principal — wallet, DAO, contract, budget, treasury —
holds capabilities. A fungible capability (an amount of tokens) is held in a
Purse. A Purse can belong to a wallet (personal balance), a DAO (treasury), a
contract (escrow), or a budget (allocated funds). It is the fungible analogue
of Box (which holds a single transferable capability).

In the Agoric stack, ERTP sits below Zoe. Purse sits at the same level in
DarkWow's stack: below PromissoryNote (which creates tokens), below Deployooor
(which deploys contracts), at the primitive layer where any actor can hold and
transfer fungible value. A canonical well-known ContractId makes it available
as a composable primitive for every contract.

## Operations

| Operation | Opcode | Circuit | Privacy Path | What It Proves |
|-----------|--------|---------|-------------|---------------|
| `InitializeV1` | 0x00 | — | — | Initialize the Purse contract (genesis primitive) |
| `DepositV1` | 0x01 | `deposit_v1.zk` | Proven | old_balance + deposit_amount == new_balance (Pedersen additive homomorphism). purse_id exposed as public input. |
| `WithdrawV1` | 0x02 | `withdraw_v1.zk` | Proven | withdraw_amount <= old_balance (LTE), withdraw_amount > 0. nullifier prevents replay. purse_id exposed. |
| `BalanceV1` | 0x03 | `balance_v1.zk` | Proven | Knowledge of purse ownership + balance. Exposes balance_commit for predicate checks. |
| `DepositV3` | 0x04 | `deposit_v3.zk` | **Hard** | Merkle inclusion of old purse state + Pedersen conservation + nullifier. purse_id in ZK witness. Observer sees `[nullifier_old, merkle_root, old_x, old_y, new_x, tx_binding, tx_nonce, new_y]`. |
| `WithdrawV3` | 0x05 | `withdraw_v3.zk` | **Hard** | Merkle inclusion + withdrawal bounds (amount > 0, amount <= balance) + Pedersen conservation + nullifier. purse_id in ZK witness. |
| `BalanceV3` | 0x06 | `balance_v3.zk` | **Hard** | Merkle inclusion of current purse state. Read-only — no nullifier, no state change. Exposes derived_purse_id, balance_commit, token_commit. |

## Privacy Model

### Proven Path (V1)

The V1 circuits expose `purse_id` as a ZK public input. An observer can see
WHICH purse is being operated on. Balance values are hidden via Pedersen
commitments (additively homomorphic), and token types are hidden via Poseidon
commitments. The owner's identity is in the ZK witness.

### Hard Path (V3)

The V3 circuits move `purse_id` into the ZK witness and bind it into a Merkle
leaf: `poseidon_hash(DOMAIN_SIGNATURE_SECRET, purse_id, balance, state_nonce)`.
The `merkle_root` opcode (Sinsemilla, depth 32) proves inclusion. Only the
Merkle root is exposed as a public input.

Every state transition (Deposit or Withdraw) follows the append-only + nullifier
model per ocap.md §6.2:

1. The old state leaf is proven to exist via Merkle inclusion proof
2. A nullifier `poseidon_hash(DOMAIN_NULLIFIER, owner_secret, purse_id, state_nonce)`
   consumes the old state
3. A new leaf with the updated balance is appended to the Merkle tree

Balance conservation is proven via Pedersen additive homomorphism in-circuit:
`old_commit + delta_commit == new_commit`.

The BalanceV3 operation is read-only — it proves Merkle inclusion of the
current state without consuming it (no nullifier).

An observer cannot determine:
- Which purse is being operated on
- The balance amount (hidden in Pedersen commitment)
- The token type (hidden in Poseidon commitment)
- Who owns the purse

## Data Model

### V1 (Proven Path)

```
Purse = {
    purse_id:        poseidon_hash(owner_pub, token_id, nonce),
    balance_commit:  pedersen_commit(balance, blind),
    token_commit:    poseidon_hash(token_id, token_blind),
    owner_commit:    poseidon_hash(owner_pub),
}
```

Stored in flat DB `"purses"` keyed by `purse_id.to_bytes()`.

### V3 (Hard Path)

Each state transition produces a Merkle leaf:

```
purse_leaf = poseidon_hash(DOMAIN_SIGNATURE_SECRET, purse_id, balance, state_nonce)
nullifier = poseidon_hash(DOMAIN_NULLIFIER, owner_secret, purse_id, state_nonce)
```

The purse tree is an append-only BridgeTree (depth 32, Sinsemilla hash).
Nullifiers are stored in a Poseidon Sparse Merkle Tree (depth 255).

## Database Trees

| Tree | Purpose | V1 | V3 |
|------|---------|----|-----|
| `purses` | Purse records keyed by purse_id | ✓ | ✓ |
| `nullifiers` | Spent nullifiers (SMT in V3) | ✓ | ✓ |
| `info` | Contract metadata, Merkle tree data, root pointers | ✓ | ✓ |
| `purse_roots` | Historical Merkle roots | — | ✓ |
| `nullifier_roots` | Historical nullifier SMT roots | — | ✓ |

## Ownership Proof

V3 uses Poseidon-based ownership: `derived_owner = poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)`
constrained against `owner_pub` in the ZK witness. Neither value is exposed
as a public input.

## Circuit Version Management

| Version | Purpose | Registered |
|---------|---------|-----------|
| V1 | Baseline (proven path) | ✓ |
| V2 | Domain-separated Poseidon hashes (HAZOP RC3) | ✓ |
| V3 | Hard path — Merkle inclusion + nullifier per state transition | ✓ |

## Composing Contracts

Ten contracts compose with Purse, using it to replace manual `u64` arithmetic on
aggregate counters. Purse is a genesis primitive — it's deployed once at genesis
(counter 8) and every contract calls it as a child.

| Contract | What the Purse Tracks | Child Calls |
|----------|----------------------|-------------|
| [escrow](escrow.md) | Locked escrow funds | DepositV1 on Fund |
| [drain_protection](drain_protection.md) | Protected fund total | DepositV1/WithdrawV1 on Transfer |
| [dao_escrow](dao_escrow.md) | Treasury, pool, and endowment balances | DepositV1 on PayPremium, WithdrawV1 on TreasurySpend/EndowmentWithdraw |
| [subscription](subscription.md) | Subscription deposit | DepositV1 on Subscribe, WithdrawV1 on Cancel |
| [pool_stake](pool_stake.md) | Pool total, member stakes, coverage | DepositV1 on Join, WithdrawV1 on Slash |
| [betting_stake](betting_stake.md) | Table pool, staker positions, earnings | DepositV1 on Stake, WithdrawV1 on Unstake |
| [relayer_endowment](relayer_endowment.md) | Deployed capital, per-deployment fees | DepositV1 on Deploy, WithdrawV1 on Settle |
| [labor_market](labor_market.md) | Job payment escrow | DepositV1 on CreateJob, WithdrawV1 on ConfirmDelivery |
| [bridge](bridge.md) | Total deposited, total withdrawn | DepositV1 on Deposit, WithdrawV1 on Withdraw |
| [stablecoin](stablecoin.md) | Total debt, total collateral, fees | DepositV1/WithdrawV1 on Mint/Repay/Liquidate |

## References

- [Object Capability Model](../arch/ocap.md) — Purse in the O-Cap stack
- [Privacy Model](../arch/privacy.md) — Hard path vs proven path
- [Contract WASM Type System](../arch/contract-wasm-type-system.md) — Encoding boundaries, error propagation
- [Box](box.md) — The single-capability container (non-fungible analogue of Purse)
- Source: `src/contract/purse/`
