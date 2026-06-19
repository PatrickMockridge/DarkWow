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

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `DepositV1` | 0x01 | `deposit_v1.zk` | old_balance + deposit_amount == new_balance (Pedersen additive homomorphism) |
| `WithdrawV1` | 0x02 | `withdraw_v1.zk` | withdraw_amount <= old_balance (LTE), nullifier prevents replay |
| `BalanceV1` | 0x03 | `balance_v1.zk` | Knowledge of purse ownership + balance. Exposes balance_commit for predicate checks |

## Privacy Properties

- **Balance hidden** via Pedersen commitment — only the holder knows the amount
- **Token type hidden** via Poseidon commitment
- **Deposits/withdrawals unlinkable** — fresh blinds per operation
- **Ownership proven via secret knowledge**, not identity — any principal can hold a Purse

## Data Model

```
Purse = {
    purse_id:        poseidon_hash(owner_pub, token_id, nonce),
    balance_commit:  pedersen_commit(balance, blind),
    token_commit:    poseidon_hash(token_id, token_blind),
    owner_commit:    poseidon_hash(owner_pub),
}
```

A Purse has no concept of "who" owns it — only that some principal knows the
owner_secret. That principal could be a person, a DAO voting quorum, a contract's
internal state machine, or a multi-sig budget.

## Database Trees

| Tree | Purpose |
|------|---------|
| `purses` | Purse records keyed by purse_id |
| `nullifiers` | Spent withdrawal nullifiers (double-spend prevention) |
| `info` | Contract metadata |

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
- [Box](box.md) — The single-capability container (non-fungible analogue of Purse)
- Source: `src/contract/purse/`
