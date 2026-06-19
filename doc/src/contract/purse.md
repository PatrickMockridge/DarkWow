# Purse — ZK Fungible Asset Container

The Purse contract is the DarkWow equivalent of Agoric's ERTP Purse — a ZK-native
container for fungible assets (tokens). It is an **O-Cap primitive** deployed at
genesis (ContractId counter 8).

## Why Genesis?

In the Agoric stack, ERTP (Purse/Payment) sits below Zoe (contract framework),
which sits below individual contracts. Purse occupies the same position in
DarkWow's stack: it is the primitive that PromissoryNote token balances are
measured in. Every wallet depends on it for balance tracking. Having a canonical
well-known ContractId ensures every contract in the ecosystem can reference it.

## Operations

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `DepositV1` | 0x01 | `deposit_v1.zk` | old_balance + deposit_amount == new_balance (Pedersen additive homomorphism) |
| `WithdrawV1` | 0x02 | `withdraw_v1.zk` | withdraw_amount <= old_balance (LTE), nullifier prevents replay |
| `BalanceV1` | 0x03 | `balance_v1.zk` | Knowledge of purse ownership + balance. Exposes balance_commit for predicate checks |

## Privacy Properties

- **Balance hidden** via Pedersen commitment — only the holder knows the amount
- **Token type hidden** via Poseidon commitment
- **Deposits/withdrawals unlinkable** — fresh blinds per operation
- **Ownership proven via secret knowledge**, not identity
- **Nullifiers prevent double-withdrawal**

## Data Model

```
Purse = {
    purse_id:        poseidon_hash(owner_pub, token_id, nonce),
    balance_commit:  pedersen_commit(balance, blind),
    token_commit:    poseidon_hash(token_id, token_blind),
    owner_commit:    poseidon_hash(owner_pub),
}
```

## Database Trees

| Tree | Purpose |
|------|---------|
| `purses` | Purse records keyed by purse_id |
| `nullifiers` | Spent withdrawal nullifiers (double-spend prevention) |
| `info` | Contract metadata |

## References

- [Object Capability Model](../arch/ocap.md) — Purse in the O-Cap stack
- [Wallet Architecture](../arch/wallet.md) — How the wallet interacts with Purses
- Source: `src/contract/purse/`
