# Contract Standards

Minimum required standards and best practices for every DarkWow WASM contract.
Any contract that does not meet these standards is considered incomplete.

## 1. Entrypoint Contract — Required Exports

Every contract MUST export four functions via the `define_contract!` macro:

| Export | Purpose |
|--------|---------|
| `__initialize` | Set up DB trees, store config, register ZK binaries |
| `get_metadata` | Return ZK public inputs + signature pubkeys for host verification |
| `process_instruction` | Validate inputs, perform state checks, return an update |
| `process_update` | Apply the update to contract state |

## 2. `__initialize` / `init_contract`

### Empty `ix` Handling

Contracts deployed via `deploy_contract()` (tests, direct deployment) receive empty initialization
data. `init_contract` MUST handle this gracefully:

```rust
pub fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    // Always check for empty ix — deploy_contract() passes empty data.
    let params: InitializeParams = if ix.is_empty() {
        InitializeParams::default()
    } else {
        deserialize(ix).map_err(|_| ContractError::IoError("Invalid init params".to_string()))?
    };
    // ... init trees, store config, register ZK binaries ...
}
```

**Anti-patterns** (rejected):
- `let _ix: &[u8]` — ignores initialization data entirely, contract cannot be configured at deploy
- `deserialize(ix)?` — crashes on empty input, contract cannot be deployed via `deploy_contract()`

### Required Initialization Steps

Every `init_contract` MUST:
1. Init all DB trees declared by the contract
2. Store version info (`CARGO_PKG_VERSION`)
3. Store configuration parameters from `params`
4. Register all ZK circuit binaries via `wasm::db::zkas_db_set`
5. Log a message confirming successful initialization

Reference: `native_token/src/entrypoint/mod.rs` `init_contract`.

## 3. `get_metadata`

### Always Call `set_return_data`

Every code path MUST call `set_return_data` before returning. The host reads return data
regardless of whether the function succeeded or failed. Returning `Ok(())` without calling
`set_return_data` leaves stale buffer data, causing the host to attempt ZK verification with
invalid or empty public inputs.

**Correct**:
```rust
fn my_get_metadata(cid: ContractId, params: &[u8]) -> Vec<u8> {
    let Ok(p) = deserialize::<MyParams>(params) else {
        // Always return empty metadata on error — never Ok(()) without set_return_data.
        return vec![];
    };
    let mut metadata = vec![];
    // ... build ZK public inputs ...
    zk_public_inputs.encode(&mut metadata).unwrap();
    let signature_pubkeys: Vec<PublicKey> = Vec::new();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}
```

**Anti-pattern** (rejected):
```rust
Err(_) => return Ok(()),  // Stale buffer, host sees garbage or empty data
```

### Encode Both ZK Inputs and Signature Pubkeys

The metadata wire format is: `Encoded(Vec<(String, Vec<Base>)>) ++ Encoded(Vec<PublicKey>)`.
Both components MUST be encoded even if empty.

- **ZK public inputs**: circuit namespace → instance values. Must match `constrain_instance` order in the `.zk` circuit.
- **Signature pubkeys**: `Vec<PublicKey>` — empty vec if the contract authorizes purely via ZK proofs (o-cap model).

Per the o-cap specification, contracts authorize via ZK proofs (capabilities). Schnorr signatures
are OPTIONAL. An empty `Vec<PublicKey>` is valid and the host handles it.

### Handle Unknown Function Selectors

`get_metadata` dispatches on `ix[0]` (the function selector byte). Unknown selectors MUST
return empty metadata (not panic, not return without `set_return_data`):

```rust
let func = match MyContractFunction::try_from(ix[0]) {
    Ok(f) => f,
    Err(_) => {
        wasm::util::set_return_data(&vec![]);
        return Ok(());
    }
};
```

## 4. Cross-Contract Calls

### Child Contract ID Validation

When making cross-contract calls (via `create_child_call`), the contract MUST validate
the child's `ContractId` against a stored configuration value. Never hardcode a child
contract ID in the circuit.

```rust
let pn_cid = get_stored_config::<ContractId>(info_db, PN_CONTRACT_ID_KEY)?;
if pn_cid != ContractId::ZERO {
    validate_child_contract_id(Some(pn_cid), &child_call)?;
}
```

**`ContractId::ZERO` bypass**: If the child CID has never been configured (stored as zero),
the validation is skipped. This is a SAFETY GAP — contracts deployed without child CID
configuration will accept any child contract ID. Production deployments MUST configure
child CIDs before accepting user transactions.

### Runtime Config vs SDK Constants

Use runtime configuration (stored in contract DB, settable via `UpdateConfig`) for
cross-contract references that may change across deployments. Use SDK constants
(`dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID`) only for genesis contracts that are
guaranteed to have the same ID on every network.

## 5. ZK Proof Coverage

Every state-mutating endpoint MUST have a corresponding ZK circuit. The circuit's
`constrain_instance` calls define the public input order that `get_metadata` must match.

- Circuit namespace: `{CONTRACT}_ZKAS_{FUNCTION}_NS_V2`
- Public inputs: declared in `get_metadata` as `(namespace_string, Vec<Base>)`
- Proof verification: host loads `.zk.bin` from contracts tree, verifies against metadata inputs

Endpoints that are purely administrative (config updates, parameter changes) may omit ZK
proofs if they are authorized by the contract owner's context (e.g., the caller IS the
contract itself, verified via `validate_child_contract_id`).

## 6. O-Cap Principles

### Capabilities via Nullifiers

A capability in DarkWow is: **knowledge of a secret that produces a valid nullifier + ZK proof**.
Proving knowledge of the secret IS the authorization. No ACLs, no role tables, no address
whitelists.

- **Nullifier**: `poseidon_hash([secret, commitment])` — consumed once, prevents replay
- **ZK proof**: proves the nullifier was correctly derived from the secret and commitment
- **Commitment**: the on-chain state being consumed (e.g., a coin, an escrow, a bet)

### No Trusted External State

Contracts MUST NOT depend on externally-configured "trusted" roots or snapshots of other
contracts' state. Cross-contract capability verification should use:

1. **ZK proof composition** (future): combine circuits to prove state in another contract
2. **Nullifier-based consumption**: the nullifier itself proves the capability was valid
   when it was created — the consuming contract verifies the nullifier, not the source state

The DEX "trusted Merkle root" pattern is a documented TEMPORARY WORKAROUND for the absence
of cross-contract ZK composition. New contracts MUST NOT adopt this pattern.

### Self-Verifying Proofs

A contract call should carry all the data needed to verify it. The host verifies ZK proofs
against public inputs from metadata. The contract verifies business logic (state transitions).
Neither should depend on data that only exists in another contract's tree at execution time.

## 7. Type System

Consensus-critical scalars MUST use nominal newtypes, never raw integers:

| Domain | Type | Not |
|--------|------|-----|
| Block height | `BlockHeight(u32)` | `u32` |
| Block reward | `BlockReward(u64)` | `u64` |
| Contract ID | `ContractId([u8; 32])` | `[u8; 32]` |
| Nullifier | `Nullifier(Base)` | `Base` |
| Coin commitment | `CoinCommitment(Base)` | `Base` |
| Intent commitment | `IntentCommitment([u8; 32])` | `[u8; 32]` |

Never cast integers across domain boundaries. Use `BlockHeight::new()`, `BlockReward::new()`,
and `.get()` accessors at the boundaries.

## Template

```rust
// Minimum viable init_contract
pub fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    let params: InitializeParams = if ix.is_empty() {
        InitializeParams::default()
    } else {
        deserialize(ix).map_err(|_| ContractError::IoError("Invalid init params".to_string()))?
    };

    let info_db = wasm::db::db_init(cid, CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, DB_VERSION_KEY, env!("CARGO_PKG_VERSION").as_bytes())?;
    // ... init remaining trees, store config, register ZK binaries ...

    msg!("[contract::init] Initialized successfully");
    Ok(())
}

// Minimum viable get_metadata
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    if ix.is_empty() {
        wasm::util::set_return_data(&vec![]);
        return Ok(());
    }
    let func = match MyFunction::try_from(ix[0]) {
        Ok(f) => f,
        Err(_) => {
            wasm::util::set_return_data(&vec![]);
            return Ok(());
        }
    };
    let metadata = match func {
        MyFunction::DoThingV1 => do_thing_get_metadata(ix),
        // ...
    };
    wasm::util::set_return_data(&metadata);
    Ok(())
}

// Minimum viable metadata function
fn do_thing_get_metadata(params: &[u8]) -> Vec<u8> {
    let Ok(p) = deserialize::<DoThingParams>(&params[1..]) else {
        return vec![];
    };
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_public_inputs.push((
        CONTRACT_ZKAS_DO_THING_NS_V2.to_string(),
        vec![p.nullifier.inner(), p.commitment.inner()],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    let signature_pubkeys: Vec<PublicKey> = Vec::new();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}
```

## 8. Exec/Apply Separation

Contracts use a two-phase execution model. `process_instruction` (exec) validates and computes
the update. `process_update` (apply) writes the update to state. The exec phase SHALL NOT write
to the database — all writes happen in apply. An exec-phase write that persists when the
transaction fails in another contract call creates orphaned state.

## 9. Error Handling

- No `assert!` in WASM — use `return Err(...)` instead (panic crashes the transaction)
- No `let _ = fallible_call()` — every Result propagated or matched
- No `.ok()` — never discard error information
- No `unwrap()` — only where invariant is locally provable
- Every `Err(...)` return preceded by `msg!()` with contract, function, and failure details
- Deserialization failure in `get_metadata` MUST call `set_return_data(&vec![])` before
  `return Ok(())` — returning without it is a ZK proof bypass (HIGH severity)

## 10. Harness Requirements

Every contract test harness SHALL meet TIER A: real ZK proofs using production client code,
capable of passing through `accept_block` verification. TIER C harnesses (empty_witnesses
pattern) SHALL be upgraded to TIER A before any test claims to exercise contract functions
through the 9-step production path.

Tests SHALL follow: genesis → coinbase → contract call → wrap_call_data → witness →
transaction → block → accept_block → state check. Tests verifying only call_data generation
without accept_block routing do not exercise contract function behavior.

## 11. Deployment Configuration

### Child Contract ID Configuration

Contracts that make cross-contract calls MUST support post-deployment configuration
of child contract IDs. The `[0u8; 32]` placeholder pattern is permitted during
initialization ONLY when paired with an `UpdateConfig` function that allows the
operator to set the real contract ID before accepting user transactions.

Contracts MUST NOT permanently accept `ContractId::ZERO` as a valid child contract
ID. The `if cid != ContractId::ZERO { validate }` bypass pattern SHALL be
replaced with explicit configuration checks that fail-closed: if the child CID
has not been configured, cross-contract calls SHALL be rejected.

