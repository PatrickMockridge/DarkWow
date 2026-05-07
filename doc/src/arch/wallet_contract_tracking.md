# Wallet Contract Tracking Architecture

## Overview

The drk wallet implements a general pattern for tracking smart contract state during blockchain scanning. This document describes the architecture used for contract interaction, which can serve as a template for understanding how the wallet matches and processes different contracts.

## Native Contract Architecture

DarkWow uses a multi-contract model where different contracts handle different aspects of the financial system:

### Contract Hierarchy

```
┌─────────────────────────────────────────────────────────────┐
│                    Native Token (DRKW)                       │
│  - Genesis PoW token                                         │
│  - Used for fees and network security                        │
│  - Functions: FeeV1, MintV1, BurnV1, TransferV1, PoWRewardV1 │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Money V3 (DeFi/ERC-20)                   │
│  - User-deployed token contracts                             │
│  - Functions: TokenMintV1, AuthTokenMintV1, MintV1, BurnV1,  │
│               TransferV1                                     │
│  - Depends on: Native Token (for fee payment)               │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 DAO Escrow (Treasury/Endowment)              │
│  - User-deployed DAO with treasury management                │
│  - Functions: InitializeV1, UpdateV1, PayPremiumV1,         │
│    WithdrawV1, EndowmentWithdrawV1, TreasurySpendV1,         │
│    EnableDrainProtectionV1, ProposeClaimV1, VoteClaimV1     │
│  - Depends on: Money V3 (for token transfers)               │
└─────────────────────────────────────────────────────────────┘
```

## Contract Matching During Scanning

### How Contracts Are Identified

Each contract call contains:
1. `contract_id`: A unique identifier for the contract (32 bytes)
2. `data`: A byte vector where the first byte is the function opcode

```rust
struct ContractCall {
    contract_id: ContractId,  // 32-byte identifier
    data: Vec<u8>,            // First byte = function code
}
```

### Function Opcodes

**Native Token (NATIVE_TOKEN_CONTRACT_ID - hardcoded genesis):**
| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | FeeV1 | Attach network fee to transaction |
| 0x01 | MintV1 | Mint new DRKW tokens |
| 0x02 | BurnV1 | Burn DRKW tokens |
| 0x03 | TransferV1 | Transfer DRKW tokens |
| 0x04 | SpendV1 | Spend a DRKW coin |
| 0x05 | PoWRewardV1 | Block reward for miners |

**Money V3 (user-deployed, runtime-registered):**
| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | TokenMintV1 | Create new token with supply |
| 0x01 | AuthTokenMintV1 | Mint tokens with authorization |
| 0x02 | MintV1 | Mint tokens (authenticated) |
| 0x03 | BurnV1 | Burn tokens |
| 0x04 | TransferV1 | Transfer tokens |

**DAO Escrow (user-deployed):**
| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | InitializeV1 | Create new DAO |
| 0x01 | UpdateV1 | Update DAO config |
| 0x02 | PayPremiumV1 | Pay membership premium |
| 0x03 | WithdrawV1 | Withdraw from DAO treasury |
| 0x04 | EndowmentWithdrawV1 | Withdraw from endowment |
| 0x05 | TreasurySpendV1 | Spend from treasury |
| 0x06 | EnableDrainProtectionV1 | Enable/disable drain protection |
| 0x07 | ProposeClaimV1 | Propose a claim |
| 0x08 | VoteClaimV1 | Vote on a claim |

### Scanning Flow: Contract Matching

In `scan_block()`, each transaction is processed call by call:

```rust
async fn scan_block(&self, scan_cache: &mut ScanCache, block: &BlockInfo) -> Result<()> {
    for tx in block.txs.iter() {
        for (i, call) in tx.calls.iter().enumerate() {
            // Match by ContractId
            if call.data.contract_id == *MONEY_V3_CONTRACT_ID.get().unwrap() {
                // Handle Money V3 functions
                match call.data.data[0] {
                    0x04 => self.apply_tx_money_data_transfer(...),  // TransferV1
                    0x02 => self.apply_tx_money_data_mint(...),      // MintV1
                    // ...
                }
            } else if call.data.contract_id == *NATIVE_TOKEN_CONTRACT_ID {
                // Handle Native Token functions
            } else if call.data.contract_id == *DAO_ESCROW_CONTRACT_ID {
                // Handle DAO Escrow functions
            }
        }
    }
}
```

## Contract Interaction: Parent-Child Calls

### DarkTree Structure

Contracts can have parent-child relationships using `DarkTree`. The `parent_index` field indicates which call is the parent:

```rust
struct ContractCallAdapter {
    contract_id: ContractId,
    data: Vec<u8>,
    parent_index: Option<usize>,  // None = root, Some(n) = child of call n
}
```

### Spend Hook: Automatic Child Calls

When a Money V3 TransferV1 output coin has a non-zero `spend_hook`, a child call is automatically created:

```rust
fn create_spend_hook_call(
    spend_hook: pallas::Base,      // ContractId as field element
    user_data: pallas::Base,      // Parameters for hook
) -> Option<ContractCall> {
    if spend_hook == pallas::Base::zero() {
        return None;
    }

    let hook_contract_id = ContractId::from(spend_hook);

    // Function code 0x00 = generic, params in user_data
    let mut data = vec![0x00u8];
    data.extend_from_slice(&user_data.to_repr());

    Some(ContractCall { contract_id: hook_contract_id, data })
}
```

### Transaction Building with Children

When building a transfer transaction with a spend_hook:

```rust
// Create spend_hook child call
let child_tree = if let Some(hook_call) = create_spend_hook_call(spend_hook_out, user_data_out) {
    let hook_leaf = ContractCallLeaf { call: hook_call, proofs: vec![] };
    let tree = DarkTree::new(hook_leaf, vec![], None, None);
    vec![tree]
} else {
    vec![]
};

// Build with money_leaf as parent, child_tree as children
let mut tx_builder = TransactionBuilder::new(money_leaf, child_tree)?;

// Fee call is a sibling (no parent_index relationship)
tx_builder.append(fee_leaf, vec![])?;
```

## Contract Registry

The wallet uses a `ContractRegistry` to manage deployed contracts and their dependencies:

```rust
pub trait Contract: Send + Sync {
    fn contract_id(&self) -> ContractId;
    fn name(&self) -> &'static str;
    fn dependencies(&self) -> Vec<ContractId>;  // e.g., Money V3 needs Native Token
    fn is_initialized(&self) -> bool;
}
```

### Registry Features

- **Dependency Resolution**: Automatically resolves transitive dependencies
- **Initialization Tracking**: Tracks whether contracts are registered at runtime
- **Instantiation Checking**: Verifies all dependencies are available before use

### Registered Contracts

| Contract | ID Source | Dependencies |
|----------|----------|--------------|
| NativeToken | Hardcoded genesis | None |
| MoneyV3 | Runtime (OnceLock) | NativeToken (for fees) |
| DaoEscrow | Runtime (OnceLock) | MoneyV3 (for transfers) |

## ZK Proof Verification During Scanning

The wallet verifies ZK proofs before processing transaction data:

```rust
fn verify_tx_zkps(
    tx: &Transaction,
    zkbin_data: &[(ContractId, String, Vec<u8>, Vec<pallas::Base>)],
    log: &mut Vec<String>,
) {
    // BlockInfo.zkbin_data contains: (contract_id, zkas_ns, zkbin_bytes, instances)
    let zkbin_by_contract: BTreeMap<_, Vec<_>> = zkbin_data.iter().fold(
        BTreeMap::new(),
        |mut acc, (cid, ns, bytes, instances)| {
            acc.entry(cid.to_bytes())
               .or_insert_with(Vec::new)
               .push((ns.clone(), bytes.clone(), instances.clone()));
            acc
        }
    );

    for (call_idx, call_leaf) in tx.calls.iter().enumerate() {
        let proofs = match tx.proofs.get(call_idx) {
            Some(p) => p,
            None => continue,
        };

        for (proof_idx, proof) in proofs.iter().enumerate() {
            // verify_zkp() from src/zk/verifier.rs
            match verify_zkp(proof, zkbin_bytes, instances) {
                ZkVerifyResult::Ok => { /* log success */ },
                ZkVerifyResult::InvalidProof => { /* log warning */ },
            }
        }
    }
}
```

## Core Components

### 1. ScanCache (In-Memory During Scanning)

```rust
pub struct ScanCache {
    // Merkle trees - track on-chain state, checkpoint per block
    pub money_tree: MerkleTree,           // Money Merkle tree (coins)
    pub money_smt: CacheSmt,              // Sparse Merkle tree (nullifiers)

    // Secrets for decryption
    pub notes_secrets: Vec<SecretKey>,     // Private keys for note decryption

    // Tracked items
    pub owncoins_nullifiers: BTreeMap<[u8; 32], ([u8; 32], Position)>,
    pub own_tokens: Vec<TokenId>,         // Tokens we hold
    pub own_deploy_auths: HashMap<[u8; 32], SecretKey>,

    // Logging
    pub messages_buffer: Vec<String>,
}
```

**Key insight**: The wallet maintains local state to know what to look for. Secret keys allow decryption of notes to discover owned items.

### 2. Merkle Trees for State Tracking

Two types of Merkle trees are used:

- **MerkleTree**: Append-only tree for coin membership proofs
- **SparseMerkleTree (SMT)**: Key-value store for nullifiers and state

Both support:
- `checkpoint(height)` - Mark state at block height
- `rewind(height)` - Revert to state at height
- `mark()` - Get current leaf position

### 3. Database Schema (WalletDB)

```sql
-- Coins: stores all coins we've received
CREATE TABLE money_coins (
    coin_id TEXT PRIMARY KEY,           -- bs58-encoded coin hash
    value INTEGER NOT NULL,              -- Token amount
    token_id TEXT NOT NULL,             -- bs58-encoded token ID
    spend_hook TEXT,                     -- bs58-encoded spend_hook (NULL = none)
    user_data TEXT,                      -- bs58-encoded user_data
    leaf_position INTEGER,              -- Merkle tree position
    secret TEXT NOT NULL,               -- bs58-encoded private key
    coin_blind TEXT NOT NULL,           -- bs58-encoded coin blind
    value_blind TEXT NOT NULL,          -- bs58-encoded value blind
    token_blind TEXT NOT NULL,         -- bs58-encoded token blind
    spent BOOLEAN DEFAULT FALSE,        -- Whether coin is spent
    spent_at_height INTEGER,            -- Height when spent (NULL if unspent)
    created_at_height INTEGER NOT NULL, -- Block height when created
    UNIQUE(coin_id, created_at_height)
);

-- Tokens: metadata about tokens we've encountered
CREATE TABLE money_tokens (
    token_id TEXT PRIMARY KEY,
    token_name TEXT,
    mint_authority TEXT,                 -- Who can mint
    token_blind TEXT,                   -- Public token blind
    is_frozen BOOLEAN DEFAULT FALSE,
    freeze_height INTEGER
);

-- Transaction history
CREATE TABLE tx_history (
    tx_hash TEXT PRIMARY KEY,
    status TEXT NOT NULL,               -- "Broadcasted", "Confirmed", "Failed"
    block_height INTEGER,               -- NULL if not confirmed
    FOREIGN KEY (block_height) REFERENCES scanned_blocks(height)
);
```

## Reorg Handling

When a reorg is detected:

```rust
async fn reset_to_height(&self, new_height: u32, buf: &mut Vec<String>) -> Result<()> {
    // 1. Rewind Merkle trees
    self.cache.merkle_tree_rewind(new_height);

    // 2. Unconfirm transactions after height
    self.unconfirm_txs_after(new_height);

    // 3. Unmark spent coins
    self.unmark_spent_coins_after(new_height);

    // 4. Clear state inverse diffs
    self.clear_state_diffs_after(new_height);
}
```

## Confirmation Flow

When a transaction is confirmed:

```rust
async fn apply_tx_money_data(...) {
    // 1. Decrypt notes using stored secrets
    for secret in scan_cache.notes_secrets {
        if let Ok(decrypted_note) = output.note.decrypt::<MoneyV3Note>(secret) {
            // 2. Create coin record
            let coin_record = CoinRecord { ... };

            // 3. Insert into wallet DB with Merkle proof
            self.wallet.insert_coin(&coin_record, &merkle_proof)?;
        }
    }
}
```

## Template for Adding New Contracts

To implement tracking for a new contract:

1. **Define constants** in `contract_imports.rs`:
```rust
pub static NEW_CONTRACT_ID: std::sync::OnceLock<ContractId> = std::sync::OnceLock::new();

pub enum NewContractOpcodes {
    Function1 = 0x00,
    Function2 = 0x01,
}
```

2. **Implement Contract trait** in `contract_registry.rs`:
```rust
pub struct NewContract;

impl Contract for NewContract {
    fn contract_id(&self) -> ContractId {
        *NEW_CONTRACT_ID.get().unwrap()
    }
    fn name(&self) -> &'static str { "NewContract" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { NEW_CONTRACT_ID.get().is_some() }
}
```

3. **Add scanning handler** in `rpc.rs`:
```rust
async fn apply_tx_new_contract_data(...) -> Result<bool> {
    match function_code {
        0x00 => { /* handle Function1 */ }
        0x01 => { /* handle Function2 */ }
    }
}
```

4. **Wire into scan_block()**:
```rust
if call.data.contract_id == *NEW_CONTRACT_ID.get().unwrap() {
    self.apply_tx_new_contract_data(...).await?;
}
```

## Key Patterns

1. **ContractId Matching**: Use `OnceLock` for runtime-registered contracts
2. **Function Opcode Dispatch**: First byte of `data` determines function
3. **Secret Key Ownership**: Items discovered by decrypting with stored secrets
4. **Merkle Trees for History**: Checkpoint/rewind for reorg handling
5. **ZK Verification**: Verify proofs before trusting transaction data
6. **Parent-Child Calls**: Use `DarkTree` with `parent_index` for call relationships
7. **Spend Hooks**: Automatic child calls triggered by coin attributes

## References

- `bin/drk/src/rpc.rs` - ScanCache and scanning implementation
- `bin/drk/src/contract_imports.rs` - Contract definitions and opcodes
- `bin/drk/src/contract_registry.rs` - Contract registry system
- `bin/drk/src/transfer.rs` - Transfer transaction building with spend hooks
- `src/zk/verifier.rs` - ZK proof verification
