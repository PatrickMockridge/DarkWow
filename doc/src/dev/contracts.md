# Smart Contracts

DarkWow implements several privacy-preserving smart contracts. Each contract follows the same architecture: WASM execution, ZK proof verification, and object-capability security.

## Contract Overview

| Contract | Purpose | Key Features |
|---------|---------|--------------|
| [PromissoryNote](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/promissory_note/README.md) | DeFi token layer | Bearer instruments, Pedersen commitments |
| [NativeToken](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/native_token/README.md) | Consensus token layer | Fee payments, block rewards |
| [Oracle](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/oracle/README.md) | External data feeds | Stake-based attestations |
| [DAO-Escrow](../src/contract/dao_escrow/README.md) | Community insurance | DAO-governed endowment |
| [Auction](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/auction/README.md) | Sealed-bid auctions | Escrow integration |
| [Attestation](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/attestation/README.md) | Credential verification | Predicate-based claims |
| [Tender](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/tender/README.md) | Request for proposals | O-Cap capability gating |
| [Labor Market](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/labor_market/README.md) | Service marketplaces | Milestone-based payments |
| [Bridge](dev/contracts/bridge.md) | Cross-chain transfers | Object Capability Security |
| [DEX](dev/contracts/dex.md) | Atomic swap DAO | Minimal viable information |
| [Identity](dev/contracts/identity.md) | Credential proofs | Selective disclosure |
| [Stablecoin](dev/contracts/stablecoin.md) | Collateralized debt | Monero backing |
| [Escrow](../src/contract/escrow/README.md) | Timelock escrow | Conditional payments |
| [Subscription](../src/contract/subscription/README.md) | Recurring payments | Time-based billing |
| [Drain Protection](../src/contract/drain_protection/README.md) | Wallet security | Gradual withdrawal limits |
| [Pool Stake](../src/contract/pool_stake/README.md) | Staking pool | Pooled validator stakes |
| [Relayer Endowment](../src/contract/relayer_endowment/README.md) | Relayer funding | Sustainable relay infrastructure |
| [Insurance Market](../src/contract/insurance_market/README.md) | Risk markets | Underwriting with O-Cap verification |
| [Bearer Bond](../src/contract/bearer_bond/README.md) | Fixed-income instruments | Interest-bearing bonds |
| [Baccarat](../src/contract/baccarat/README.md) | Gaming | ZK card game |
| [Darktoshi Dice](../src/contract/darktoshi_dice/README.md) | Gaming | ZK dice rolls |
| [Roulette](../src/contract/roulette/README.md) | Gaming | ZK roulette |
| [Slot](../src/contract/slot/README.md) | Gaming | ZK slot machine |
| [Lottery](../src/contract/lottery/README.md) | Gaming | ZK lottery |
| [Betting Stake](../src/contract/betting_stake/README.md) | Gaming | ZK sports betting |
| [Game Room](../src/contract/game_room/README.md) | Gaming | Multi-game lobby |
| [Darkbet Exchange](../src/contract/darkbet_exchange/README.md) | Prediction market | Binary options with O-Cap |
| [OTC Swap](../src/contract/otc_swap/README.md) | P2P trading | OTC token swaps |
| [Deployooor](../src/contract/deployooor/README.md) | Contract deployment | Production deployment path |

## Core Principles

### 1. Object-Capability (O-Cap) Security

Instead of VSS/threshold signing, users hold secrets directly. Capabilities are conferred by possessing objects:

```rust
// Capability derived from holding a secret key
let capability = wasm::capability::deriveCapability(secret_key, scope)?;
```

### 2. Two-Phase Execution

Contracts use a `process_instruction`/`process_update` pattern:

```rust
// Phase 1: process_instruction (readonly)
// - Verify ZK proofs
// - Validate state transitions
// - Read from database (db_get, db_lookup)

// Phase 2: process_update (writeonly)
// - Apply state changes
// - Write to database (db_set)
```

### 3. ZK-First Privacy

Private data stays private; only ZK proofs are verified on-chain:

```rust
// Public inputs verified via ZK proof
let metadata = vec![
    params.oracle_id,
    params.value,
];
wasm::util::set_return_data(&metadata)?;
```

### 4. Deterministic State

All addresses and commitments use cryptographic hashing:

```rust
let id = poseidon_hash([
    pubkey_x,
    pubkey_y,
    nonce,
    amount,
]);
```

## Common Patterns

### Function Enum Definition

```rust
dwow_sdk::define_contract_function!(ContractFunction {
    InitializeV1 = 0x00,
    DoSomethingV1 = 0x01,
    DoSomethingElseV1 = 0x02,
});
```

### Data Structures

```rust
use dwow_serial::{SerialEncodable, SerialDecodable};
use dwow_sdk::pasta::pallas;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SomeParamsV1 {
    pub id: pallas::Base,
    pub value: pallas::Base,
    pub data: Vec<u8>,
}
```

### Database Operations

```rust
// Initialize a new database tree (in init_contract only)
let db = wasm::db::db_init(cid, TREE_NAME)?;

// Get handle to existing database
let db = wasm::db::db_lookup(cid, TREE_NAME)?;

// Read data (returns Option<Vec<u8>>)
let data = wasm::db::db_get(db, &key)?;
let obj: MyStruct = match data {
    Some(bytes) => deserialize(&bytes)?,
    None => return Err(ContractError::NotFound.into()),
};

// Write data
wasm::db::db_set(db, &key, &serialize(&obj)?)?;
```

### Error Handling

```rust
use dwow_sdk::error::{ContractError, ContractResult};

impl From<MyError> for ContractError {
    fn from(e: MyError) -> Self {
        match e {
            MyError::NotFound => Self::Custom(1),
            MyError::InvalidState => Self::Custom(2),
        }
    }
}
```

### Block Height

```rust
// Get current block height (for timestamp-dependent logic)
let current_block = wasm::util::get_verifying_block_height()? as u64;
```

## Contract Structure

```
src/contract/<name>/
├── proof/                  # ZK proof circuits (.zk files)
├── src/
│   ├── client/           # Client-side transaction builders
│   ├── entrypoint.rs    # WASM contract implementation
│   ├── error.rs         # Error definitions
│   ├── lib.rs            # Contract enum and constants
│   └── model/            # Data structures (mod.rs)
├── tests/
│   └── integration.rs    # Integration tests
├── Cargo.toml
└── Makefile
```

## Building and Testing

```bash
# Build WASM contract
cargo build -p darkfi_<name>_contract

# Run integration tests
cargo test -p darkfi_<name>_contract --test integration

# Run all contract tests
cargo test -p darkfi_<name>_contract
```

## Debugging Contracts

When debugging contract issues, check:

1. **Import correctness** - Use `dwow_sdk::crypto::ContractId`, not `pasta_prelude`
2. **Database handles** - Use `db_lookup` for existing DBs, `db_init` only in initialization
3. **Deserialization** - `db_get` returns `Option<Vec<u8>>`, must deserialize
4. **Type prefixes** - Use `pasta::pallas::Base` not `pallas::Base`
5. **Function enums** - Use `Function::try_from(data[0])` for switch statements

### Common Error Patterns

```
error[E0433]: could not find `zk` in `wasm`
  → ZK verification is runtime-only; comment out wasm::zk::verify_zk_proof calls

error[E0433]: could not find `chain` in `wasm`
  → Use wasm::util::get_verifying_block_height() instead

error[E0308]: mismatched types
  → db_get returns Option<Vec<u8>>, not the struct directly

error[E0560]: struct has no field named `pubkey`
  → Struct uses pub_x/pub_y coordinates, not PublicKey
```

## ZK Circuits

ZK proofs verify contract logic without revealing secrets:

```zk
circuit some_proof_v1(prover: Witness) {
    // Public inputs (verified on-chain)
    commitment: Scalar = prover.pub_input("commitment");

    // Private inputs (known only to prover)
    secret: Scalar = prover.witness("secret");

    // Verification
    computed: Scalar = poseidon_hash(secret);
    assert_equal(computed, commitment);
}
```

## Security Principles

1. **Object Capability**: Instead of VSS/threshold signing, users hold secrets directly
2. **Minimal Viable Information**: Only reveal what's necessary
3. **ZK-First**: Private data stays private, only proofs verified on-chain
4. **Deterministic**: Address derivation and commitments use cryptographic hashing
5. **Atomic Transactions**: If any function call fails, entire transaction is rejected

## Further Reading

- [Contract Inherent Safety](contracts/safety.md) — Design principles for safe contracts, NativeToken/PromissoryNote separation of concerns, and hardening lessons from security review
- [Contract Standards](contracts/standards.md) — ZK circuit primitives, token layer architecture, security requirements
- [Contract Composability](../contract/composability.md) — Cross-contract child call patterns and amount validation
- [Python Contract Simulations](testing/python-simulations.md) — Smoke test layer: model state machines in Python to catch design bugs before reaching the testnet