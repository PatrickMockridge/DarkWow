# Box — ZK Capability Container (L1)

The capability to delegate. A ZK-native o-cap primitive for linear delegation:
place a capability into a Box, and whoever Takes it receives it. Each operation
consumes the previous state (nullifier) and creates a new one (Merkle leaf).

## Operations

| Op | Opcode | Circuit | Barbs |
|----|--------|---------|-------|
| Put | 0x01 | `put.zk` | spend, nullify, prove-inclusion, commit |
| Take | 0x02 | `take.zk` | spend, nullify, prove-inclusion |

## Architecture

Four-component flow per operation: Circuit constrains → Params carry →
Metadata echoes → Exec validates + Apply writes. Metadata is pure echo —
no computation. See `doc/src/contract/box.md` for the full specification.

## References

- [Box Specification](../../../doc/src/contract/box.md)
- [Privacy Model](../../../doc/src/arch/privacy.md)
- [Safety — Lesson 22](../../../doc/src/dev/contracts/safety.md)
