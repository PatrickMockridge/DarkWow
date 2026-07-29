# Purse — ZK Fungible Capability Container (L1)

The capability to hold fungible value. A ZK-native o-cap primitive for balance
tracking with Pedersen commitment hiding. Each Deposit/Withdraw consumes the
previous state (nullifier) and creates a new one (Merkle leaf). Balance proves
inclusion without consumption.

## Operations

| Op | Opcode | Circuit | Barbs |
|----|--------|---------|-------|
| Deposit | 0x01 | `deposit.zk` | spend, nullify, prove-inclusion, denominate, conserve, commit |
| Withdraw | 0x02 | `withdraw.zk` | Same + bounds (amount > 0, amount <= balance) |
| Balance | 0x03 | `balance.zk` | prove-inclusion (read-only, no nullifier) |

## Architecture

Four-component flow per operation: Circuit constrains → Params carry →
Metadata echoes → Exec validates + Apply writes. Metadata is pure echo —
no computation. See `doc/src/contract/purse.md` for the full specification.

## References

- [Purse Specification](../../../doc/src/contract/purse.md)
- [Privacy Model](../../../doc/src/arch/privacy.md)
- [Safety — Lesson 22](../../../doc/src/dev/contracts/safety.md)
