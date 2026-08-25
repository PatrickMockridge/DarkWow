# Contract Deployment Pipeline

## Overview

This document describes how `dwowd` initializes on the linear-master branch (Uncle Merkle consensus). Understanding this pipeline is helpful for debugging contract testing and deployment issues.

The startup sequence has two phases: genesis bootstrap (state initialization) and runtime consensus (block production and validation via Uncle Merkle).

## Architecture

`dwowd` is the DarkWow blockchain node. It handles:
- Consensus (Uncle Merkle PoW)
- Transaction validation
- Smart contract execution (ZK proofs + WASM state transitions)
- P2P networking

## Contract Types

### Genesis Contracts

Nine contracts are deployed at genesis, each at a deterministic ContractId derived from
`poseidon_hash([42, 0, counter])`:

| Counter | Contract | Crate | Consensus Role |
|---------|----------|-------|----------------|
| 2 | Deployooor | `dwow_deployooor_contract` | Infrastructure (consensus-critical) |
| 3 | Promissory Note | `dwow_promissory_note_contract` | Ecosystem infrastructure |
| 4 | NativeToken | `dwow_native_token_contract` | Consensus-critical |
| 5 | Identity | `dwow_identity_contract` | Ecosystem infrastructure |
| 6 | Oracle | `dwow_oracle_contract` | Ecosystem infrastructure |
| 7 | Attestation | `dwow_attestation_contract` | Ecosystem infrastructure |
| 8 | Purse | `dwow_purse_contract` | Ecosystem infrastructure |
| 9 | Box | `dwow_box_contract` | Ecosystem infrastructure |
| 10 | MultiSig | `dwow_multisig_contract` | Ecosystem infrastructure |

**Characteristics:**
- ContractID known at compile time (static constants in `src/sdk/src/crypto/contract_id.rs`)
- WASM binary embedded via `include_bytes!()` in `bin/dwowd/src/lib.rs`
- Carried as deployment transactions inside the genesis block (positions 1-9) via `build_genesis_deployment_txs()`
- Only Deployooor and NativeToken are consensus-critical — the chain cannot function without them
- PromissoryNote, Identity, Oracle, Attestation, Purse, Box, and MultiSig are ecosystem infrastructure — they provide canonical well-known ContractIds for composable O-Cap primitives (DeFi, credentials, data feeds, trust verification, fungible containers, capability delegation, and private threshold voting)

### WASM Contracts (Post-Genesis)

All other contracts are deployed post-genesis via the Deployooor contract:

| Contract | Crate | ContractID | Deployed By |
|----------|-------|------------|-------------|
| DAO Escrow | `dwow_dao_escrow_contract` | Derived from deployer's pubkey | Deployooor |
| DEX | `dwow_dex_contract` | Derived from deployer's pubkey | Deployooor |
| Stablecoin | `dwow_stablecoin_contract` | Derived from deployer's pubkey | Deployooor |
| *(all others)* | `dwow_*_contract` | Derived from deployer's pubkey | Deployooor |

**Characteristics:**
- ContractID unknown until deployment (derived at runtime from deployer's public key)
- WASM binary deployed via transaction to Deployooor
- Deployable by any user — no special permissions required

---

## Genesis Bootstrap Sequence

`init_genesis()` in `bin/dwowd/src/lib.rs:653` builds the genesis block Bitcoin-style:
a coinbase transaction at position 0, followed by 9 contract deployment transactions
at positions 1-9, all carried inside the genesis block. Genesis follows the same
block construction path as every other block — no special bootstrap case.

**Step 1 — Build coinbase (position 0):** `expected_reward(BlockHeight::GENESIS)`
computes the initial reward. `build_linear_coinbase()` constructs the transaction
with a ZK proof (Mint_V1), nullifier, and encrypted note. Same code path as every
subsequent block.

**Step 2 — Build deployment transactions (positions 1-9):**
`build_genesis_deployment_txs()` at line 589 constructs 9 transactions, each
targeting the Deployooor contract via `DEPLOYOOOR_CONTRACT_ID`. Order: Deployooor,
NativeToken, PromissoryNote, Identity, Oracle, Attestation, Purse, Box, MultiSig.
Each uses a fixed deterministic key (from `Base(1)`) — binding is by table position,
not by key.

**Step 3 — Build header:** Height = `BlockHeight::GENESIS` (1), target =
`BlockTarget::MAX`, timestamp = 0 (deterministic). Merkle root is blake3 over all
10 transactions. RandomX key derived via `blake3::hash(&height.to_le_bytes())`.

**Step 4 — Mine and commit:** RandomX VM created, nonce found, hash verified.
`accept_block()` executes WASM, verifies PoW, and atomically commits to sled.

**Step 5 — Return genesis block hash (`HeaderHash`):** Used by merge-mining RPC
for chain_id and P2P broadcasting. A syncing node materializes contracts by
executing this same genesis block via P2P sync — contracts ride inside the block,
not deployed separately.

ContractIds are deterministic because `build_genesis_deployment_txs()` uses a
fixed position-based table with a deterministic key, not a deployer public key.

---

## Runtime Consensus

After genesis bootstrap, the node switches to linear Uncle Merkle consensus:

- **Block production**: Miners submit solved headers via stratum → `dwowd` assembles blocks
- **Block application**: `CChainState::apply_block_with_uncles()` validates PoW, processes transactions
- **State storage**: Plain sled trees (no overlay/diff rollback — state changes are final)
- **Fork handling**: Uncle blocks earn pin rewards rather than being orphaned
- **Sync**: Full block sync via the unified `SyncServer`/`SyncPeer` rail (port+2)

---

## NativeToken Contract Functions

The NativeToken contract handles all consensus-critical token operations:

| ID | Function | Purpose |
|----|----------|---------|
| 0x00 | *(REMOVED)* | FeeV1 — returns InvalidFunction |
| 0x01 | MintV1 | DISABLED — walled off behind PoWRewardV1 (consensus-locked coinbase) |
| 0x02 | BurnV1 | Destroy commitments with nullifier |
| 0x03 | TransferV1 | Private transfers |
| 0x04 | SpendV1 | Spend with change output |
| 0x05 | PoWRewardV1 | Block rewards for miners |
| 0x06 | FeeCollectV1 | Fee collection and accumulator management |
| 0x08 | FeeV2 | Fee payment with threshold proofs |

---

## Key Differences from Upstream (Overlay-DAG)

The upstream overlay-DAG architecture deployed 4 native contracts (Money, DAO, Deployooor, MoneyV2) and used overlay/diff-based runtime consensus with speculative state.

This fork:
- Deploys 9 genesis contracts (see [Genesis Contracts](genesis.md) for the full list). All other governance and DeFi contracts are WASM deployed post-genesis via Deployooor
- Uses Uncle Merkle consensus for block production — deterministic, no speculative state
- Stores state in plain sled trees — every state change is final

---

## Related

- [Testing Overview](../dev/testing/overview.md) — Four-level testing taxonomy
- [NativeToken Contract](../contract/native_token.md) — Contract overview
