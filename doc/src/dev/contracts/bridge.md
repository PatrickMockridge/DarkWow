# Bridge Contract

Anonymous bridge contract for cross-chain asset transfers using **Object Capability Security** instead of VSS.

## Overview

The bridge contract enables privacy-preserving transfers between DarkFi and external blockchains (initially Ethereum).

**Key Innovation**: DarkFi replaces traditional VSS (Verifiable Secret Sharing) with **deterministic address derivation** - users hold their own secrets, bridge nodes cannot steal funds.

## The VSS Problem

Traditional bridges use VSS for custody:
```
User deposits → VSS nodes hold secret shards → Withdrawal requires n-of-m threshold
```

**Vulnerabilities:**
- VSS Node Compromise: Any t of n nodes can reconstruct secret and steal funds
- Centralization: Threshold nodes can censor withdrawals
- Slow: Threshold signing required for each withdrawal

## The Object Capability Solution

DarkFi bridge uses deterministic address derivation:
```
User knows secret → Derive bridge_address = H(recipient_identity, nonce) → Deposit
User knows secret → Compute nullifier = H(secret) → Withdraw (self-signed)
```

**Advantages:**
- No shared secrets: Bridge nodes cannot know user's bridge secret
- Fast withdrawals: No threshold coordination, just ZK proof verification
- Censorship resistant: User alone authorizes, no gatekeepers

## Security Comparison

| Aspect | VSS-Based Bridge | DarkFi OCap Bridge |
|--------|------------------|--------------------|
| Key custody | Distributed shards | User-held secrets |
| Withdrawal speed | Slow (round) | Fast (self-signed) |
| Node compromise | Catastrophic | Impossible |
| Censorship | Threshold can block | Cannot block |
| Complexity | High (DKG) | Low (hashing) |

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize bridge state |
| DepositV1 | 0x01 | Register external chain deposit |
| WithdrawV1 | 0x02 | Claim withdrawal to external chain |
| UpdateConfigV1 | 0x03 | Update bridge operators/threshold |

## ZK Circuits

- `deposit_v1.zk`: Prove deposit is valid without revealing recipient
- `withdraw_v1.zk`: Prove withdrawal authorization without revealing secret

## Structure

```
src/contract/bridge/
├── proof/
│   ├── deposit_v1.zk
│   └── withdraw_v1.zk
├── src/
│   ├── client/mod.rs      # DepositBuilder, WithdrawBuilder
│   ├── entrypoint.rs     # Contract implementation
│   ├── error.rs
│   ├── lib.rs            # BridgeFunction enum
│   └── model/mod.rs      # DepositParams, WithdrawParams
├── tests/
├── Cargo.toml
└── Makefile
```

## Building

```bash
cd src/contract/bridge
make        # Build WASM contract
make proof  # Compile ZK circuits
cargo test  # Run tests
```

## References

- [Bridge Architecture](../../arch/bridge.md)
- [Object Capability Security](https://en.wikipedia.org/wiki/Object-capability_model)
