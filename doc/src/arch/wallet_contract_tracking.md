# Wallet Contract Tracking Architecture

## Overview

The drk wallet implements a general pattern for tracking smart contract state during blockchain scanning. This document describes the architecture used for DAO tracking, which can serve as a template for other contracts like dao_escrow.

## Core Components

### 1. ScanCache (In-Memory During Scanning)

```rust
pub struct ScanCache {
    // Merkle trees - track on-chain state, checkpoint per block
    pub dao_daos_tree: MerkleTree,           // Append-only: DAO bullas
    pub dao_proposals_tree: MerkleTree,        // Append-only: proposal bullas

    // HashMaps - populated from wallet DB using secret keys
    pub own_daos: HashMap<DaoBulla, (Option<SecretKey>, Option<SecretKey>)>,
    //                                            ^proposals_key       ^votes_key
    pub own_proposals: HashMap<DaoProposalBulla, DaoBulla>,
    // Maps proposal bulla -> DAO bulla it belongs to
}
```

**Key insight**: The wallet maintains local state to know what to look for. Secret keys allow decryption of notes to discover owned items.

### 2. Merkle Trees for State Tracking

Two sled-persisted Merkle trees track on-chain state:

- `SLED_MERKLE_TREES_DAO_DAOS = b"_dao_daos"` - Appended when DAOs are minted
- `SLED_MERKLE_TREES_DAO_PROPOSALS = b"_dao_proposals"` - Appended when proposals are created

Both support:
- `append(node)` - Add new item
- `checkpoint(height)` - Mark state at block height
- `rewind(height)` - Revert to state at height
- `mark()` - Get current leaf position

### 3. Database Schema

```sql
-- DAOs: stores imported DAOs with optional mint confirmation
CREATE TABLE ..._dao_daos (
    bulla BLOB PRIMARY KEY,
    name TEXT UNIQUE,
    params BLOB,                    -- Encrypted DAO params with keys
    leaf_position BLOB,            -- NULL until minted on-chain
    mint_height INTEGER,            -- NULL until confirmed
    tx_hash BLOB,                   -- NULL until confirmed
    call_index INTEGER              -- NULL until confirmed
);

-- Proposals: stores proposals with execution info
CREATE TABLE ..._dao_proposals (
    bulla BLOB PRIMARY KEY,
    dao_bulla BLOB NOT NULL,       -- FK to parent DAO
    proposal BLOB,                  -- Decrypted proposal params
    data BLOB,                     -- Plaintext call data
    leaf_position BLOB,
    money_snapshot_tree BLOB,       -- For vote verification
    nullifiers_smt_snapshot BLOB,
    mint_height INTEGER,
    tx_hash BLOB,
    call_index INTEGER,
    exec_height INTEGER,            -- NULL until executed
    exec_tx_hash BLOB              -- NULL until executed
);

-- Votes: individual votes with height for reorg handling
CREATE TABLE ..._dao_votes (
    vote_id INTEGER PRIMARY KEY,
    proposal_bulla BLOB NOT NULL,   -- FK to proposal
    vote_option INTEGER NOT NULL,   -- 0=no, 1=yes
    yes_vote_blind BLOB,
    all_vote_value BLOB,
    all_vote_blind BLOB,
    block_height INTEGER,
    tx_hash BLOB,
    call_index INTEGER,
    nullifiers BLOB                -- Input nullifiers to prevent double-voting
);
```

## Scanning Flow

```
For each block during sync:
  1. checkpoint(dao_daos_tree, block_height)
  2. checkpoint(dao_proposals_tree, block_height)

  3. For each DAO contract call in block:
     match function:
       Mint =>         append to dao_daos_tree, check ownership
       Propose =>      append to dao_proposals_tree, decrypt note
       Vote =>         find proposal, decrypt vote, store
       Exec =>         update proposal exec_height/tx_hash

  4. insert_merkle_trees() -- persist to sled
```

### Ownership Discovery

The wallet knows about DAOs because:
1. User imports a DAO (stores `DaoParams` with secret keys)
2. During scanning, notes are decrypted using these keys
3. If decryption succeeds, the item belongs to the user

## Reorg Handling

When a reorg is detected:

```rust
reset_to_height(new_height):
  1. dao_daos_tree.rewind(new_height)           // Truncate Merkle tree
  2. dao_proposals_tree.rewind(new_height)

  3. unconfirm_daos_after(new_height)
     -- UPDATE dao_daos SET leaf_position=NULL, mint_height=NULL
     -- WHERE mint_height > new_height

  4. unconfirm_dao_proposals_after(new_height)
     -- UPDATE dao_proposals SET ...=NULL WHERE mint_height > new_height

  5. unexec_dao_proposals_after(new_height)
     -- UPDATE dao_proposals SET exec_height=NULL, exec_tx_hash=NULL
     -- WHERE exec_height > new_height

  6. remove_dao_votes_after(new_height)
     -- DELETE FROM dao_votes WHERE block_height > new_height
```

## Confirmation Flow

When a DAO transaction is confirmed:

```rust
async fn apply_dao_mint_data(...) {
    // 1. Append bulla to Merkle tree
    scan_cache.dao_daos_tree.append(MerkleNode::from(new_bulla.inner()));

    // 2. Check if we own this DAO (by trying to decrypt with our keys)
    if !scan_cache.own_daos.contains_key(new_bulla) {
        return Ok(false);  // Not our DAO
    }

    // 3. Update wallet DB with confirmation info
    self.confirm_dao(
        new_bulla,
        scan_cache.dao_daos_tree.mark().unwrap(),  // leaf position
        tx_hash,
        call_index,
        mint_height,
    ).await
}
```

## Template for Other Contracts

To implement similar tracking for dao_escrow:

1. **Define constants**:
```rust
pub const SLED_MERKLE_TREES_DAO_ESCROW: &[u8] = b"_dao_escrow_something";
```

2. **Add to ScanCache**:
```rust
pub struct ScanCache {
    // ... existing fields ...
    pub dao_escrow_tree: MerkleTree,
    pub own_escrows: HashMap<EscrowBulla, EscrowSecretKeys>,
}
```

3. **Add database tables** following the same schema pattern

4. **Implement scanning handlers**:
```rust
async fn apply_tx_escrow_data(...) {
    match function {
        CreateEscrow => { /* append to tree, check ownership */ }
        UpdateEscrow => { /* update state */ }
        CloseEscrow => { /* mark as closed */ }
    }
}
```

5. **Implement reorg handlers**:
```rust
fn unconfirm_escrows_after(&self, height: &u32) { ... }
fn remove_escrow_votes_after(&self, height: &u32) { ... }
```

## Key Patterns

1. **Secret Key Ownership**: Items are discovered by decrypting notes with stored secret keys
2. **Merkle Trees for History**: Append-only trees with checkpoint/rewind for reorgs
3. **Database for Rich State**: Merkle trees give O(1) membership, DB gives rich queryable state
4. **Nullification on Reorg**: Confirmation fields set to NULL, executed items unmarked

## References

- `bin/drk/src/rpc.rs` - ScanCache implementation
- `bin/drk/src/scanned_blocks.rs` - Reorg handling
- `bin/drk/src/dao.rs` - DAO-specific methods (deleted, see git history)
