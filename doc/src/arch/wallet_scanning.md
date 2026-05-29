# Wallet Scanning

The DarkWow wallet scanner (`bin/drk/src/rpc.rs`) scans blocks to detect coins belonging to the wallet.

## Overview

Scanning is the process of:
1. Fetching blocks from the blockchain via RPC
2. Iterating through transactions and contract calls
3. Decrypting notes to detect coins owned by the wallet
4. Storing discovered coins in the wallet database

## Scanning Modes

The wallet supports two scanning modes:

### Regular Blockchain Scanning

Uses `scan_blocks()` which processes `BlockInfo` structures:
- Full `Transaction` type with `DarkLeaf<ContractCall>` wrappers
- `ContractCall` includes `children_indexes` for cross-contract calls
- ZK proofs are available for verification

### Linear Blockchain Scanning

Uses `scan_blocks_linear()` which processes `LinearBlockAdapter`:
- Simplified translation layer for linear blocks
- `ContractCall` lacks `children_indexes` (no child call traversal)
- ZK proofs are trusted (dwowd validated before block inclusion)

## Scanning Flow

```
1. For each block height from current to latest:
   2. Fetch block via RPC (get_block or get_block_linear)
   3. For each transaction:
      4. For each contract call:
         5. Route to contract-specific handler
         6. Decrypt notes using wallet secrets
         7. If coin belongs to wallet, insert into database
   8. Update merkle trees and state
   9. Flush database
```

## Contract Handlers

### NATIVE_TOKEN_CONTRACT_ID

**Handler:** `apply_tx_native_token_data_linear()`

Handles mining rewards (PoWRewardV1, opcode 0x05):
- Decrypts `NativeNote` using wallet secrets
- Creates `CoinRecord` for the reward coin
- Uses DRKW token (`pallas::Base::zero()`)

### PROMISSORY_NOTE_CONTRACT_ID

**Handler:** `apply_tx_money_data_linear()`

Handles token transfers (TransferV1, opcode 0x04):
- Decrypts `PromissoryNote` using wallet secrets
- Creates `CoinRecord` for each output belonging to wallet
- Supports multiple token types

**Note:** MintV1 (opcode 0x02) is logged but outputs are not automatically added (minting creates coins that the recipient must claim via other means).

### DAO_ESCROW_CONTRACT_ID

**Detection:** Function code matching (0x00-0x08)

DAO operations are logged for observability. Actual token transfers from DAO operations appear as PromissoryNote calls, so the PromissoryNote handler captures those coins.

### BEARER_BOND_CONTRACT_ID

**Detection:** Function code matching (0x00-0x06)

**Opcodes:**
| Opcode | Function | Handler Behavior |
|--------|----------|-----------------|
| `0x00` | IssueStakeV1 | BlindOutput_V1 outputs decrypted as `BondCoin` notes |
| `0x01` | TransferStakeV1 | Burn_V1 + BlindOutput_V1 — new BondCoin outputs decrypted |
| `0x02` | DeclareProfitsV1 | Logged for observability (no coin outputs) |
| `0x03` | ClaimProfitsV1 | BlindOutput_V1 profit payout coins decrypted |
| `0x04` | UnstakeV1 | Burn_V1 (stake consumed) + Redeem_V1 (receipt coin) |
| `0x05` | BurnStakeV1 | Burn_V1 — stake coins destroyed |
| `0x06` | ProveCoverageV1 | Logged for governance audit trail |

BondCoin notes are encrypted using the same AEAD scheme as Promissory Note.
The scanner decrypts BlindOutput_V1 outputs to detect wallet-owned BondCoin
records. Bond metadata (principal, last_claim_block, maturity_block, issuer_contract)
travels as plaintext on BondCoin outside the ZK coin commitment.

## Coin Detection

Coins are detected via **note decryption**:

1. Each note is encrypted using AEAD with a symmetric key derived from the recipient's secret
2. The wallet tries each of its secrets to decrypt
3. If decryption succeeds, the note belongs to the wallet
4. The coin is created from note attributes and stored

## Key Types

| Type | Purpose |
|------|---------|
| `CoinRecord` | Stored coin with value, token, secrets, merkle proof |
| `ScanCache` | Temporary scanning state (merkle trees, secrets, etc) |
| `PromissoryNote` | Encrypted note for PromissoryNote transfers |
| `NativeNote` | Encrypted note for native token operations |

## Wallet Database

Coins are stored in sled database trees:

| Tree | Contents |
|------|----------|
| `money_merkle_trees` | Merkle trees for coin membership |
| `money_smt_tree` | Sparse Merkle tree for nullifiers |
| `coins` | Coin records indexed by coin_id |

## ZK Proof Verification

**The wallet scanner does NOT verify ZK proofs.**

This is by design because:
1. dwowd validates all ZK proofs before including transactions in blocks
2. The scanner only needs to detect wallet-owned coins, not verify proofs
3. Verification would require loading WASM binaries and ZK circuits, significantly increasing complexity

## Linear Blockchain Considerations

The linear blockchain scanning has unique characteristics:

1. **No children_indexes**: DAO child call traversal not possible
2. **Trusted verification**: dwowd has already validated proofs
3. **Runtime contract IDs**: PromissoryNote and DAO-ESCROW use `OnceLock` for ContractId registration

## RPC Endpoints Used

| Endpoint | Purpose |
|----------|---------|
| `blockchain.get_block` | Fetch regular blocks |
| `blockchain.get_block_linear` | Fetch linear blocks |
| `blockchain.last_confirmed_block` | Get latest block height |
| `blockchain.get_difficulty` | Get current difficulty |