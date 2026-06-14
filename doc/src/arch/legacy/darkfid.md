# DarkWow Daemon (dwowd)

> **DEPRECATED**: This document describes the legacy `darkfid` daemon
> architecture (original fork/overlay consensus). The current daemon is
> `dwowd`. See [DarkWow Daemon (dwowd)](dwowd.md) for the current
> architecture documentation.

## Overview

dwowd is the DarkWow blockchain node software - the daemon that runs the full DarkWow network. It handles consensus, transaction validation, smart contract execution, and P2P networking.

## Key Responsibilities

### 1. Blockchain Consensus
- Proof-of-Work consensus via RandomX algorithm
- Block validation and propagation
- Longest chain rule implementation

### 2. Transaction Processing
- JSON-RPC API for wallet interactions
- Transaction validation and mempool
- Block assembly and mining

### 3. Smart Contract Execution
- ZK proof verification (zkVM)
- WASM contract runtime (wasmi)
- State transitions and Merkle tree updates

### 4. P2P Networking
- Node discovery and connection management
- Block and transaction broadcasting
- Stratum server for mining pool

## Native vs WASM Contracts

dwowd ships with three **genesis contracts** compiled directly into the binary:

| Contract | Purpose | ContractID |
|----------|---------|------------|
| NativeToken | Fee payment, block rewards | Hardcoded |
| Deployooor | WASM contract deployment | Hardcoded |
| Promissory Note | Universal DeFi primitive (not consensus-critical) | Hardcoded |

Genesis contracts are deployed at startup and have their WASM binaries embedded via `include_bytes!()`.

**Other WASM contracts** (Stablecoin, Identity, DEX, etc.) are deployed dynamically via the Deployooor contract after the node starts.

## Startup Sequence

See [DarkWow Contract Deployment Pipeline](./arch/dwowd_contract_pipeline.md) for detailed startup flow.

```
dwowd startup
     │
     ├── Load/create sled database
     ├── Load PKs and VKs from disk cache
     ├── Inject VKs into overlay (native contracts only)
     └── Deploy native contracts (NativeToken, DAO, Deployooor)
          │
          └── Node ready, awaiting commands
```

## Configuration

dwowd reads configuration from a TOML file:

```toml
# Example dwowd.toml
[server]
jsonrpc_host = "127.0.0.1"
jsonrpc_port = 48345

[p2p]
listen_address = "/ip4/0.0.0.0/tcp/4444"
seeds = ["..."]

[consensus]
pow_difficulty = 1000000

[mining]
stratum_port = 48347
```

## Running a Node

### Localnet (for development)
```bash
./dwowd -c contrib/localnet/dwowd-single-node/dwowd.toml
```

### Testnet
```bash
./dwowd -c config/testnet/dwowd.toml
```

### Mainnet
```bash
./dwowd -c config/mainnet/dwowd.toml
```

## Key Files and Directories

| Path | Description |
|------|-------------|
| `bin/dwowd/` | dwowd binary |
| `contrib/localnet/` | Local development configuration |
| `src/validator/` | Validator implementation |
| `src/contract/` | Contract implementations |
| `bin/dwowd/src/tests/harness.rs` | Test harness initialization |

## Related Documentation

- [DarkWow Daemon (dwowd)](dwowd.md) — Current daemon architecture (replaces this doc)
- [Contract Deployment Pipeline](./arch/dwowd_contract_pipeline.md) - How contracts are deployed and VKs injected
- [Testing Overview](./dev/testing/overview.md) - Four-level testing taxonomy
- [JSON-RPC API](../clients/dwowd_jsonrpc.md) - Wallet API reference
