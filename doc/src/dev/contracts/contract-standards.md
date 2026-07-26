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

### Encode ZK Inputs Only — No Schnorr Signatures

The metadata wire format is: `Encoded(Vec<(String, Vec<Base>)>) ++ Encoded(Vec<PublicKey>)`.
Both components MUST be encoded. The ZK public inputs component MUST be populated.
The signature pubkeys component SHALL be empty `vec![]`.

- **ZK public inputs**: circuit namespace → instance values. Must match `constrain_instance` order in the `.zk` circuit.
- **Signature pubkeys**: SHALL be `Vec::new()` (empty). Schnorr signatures are PROHIBITED in contract metadata.

DarkWow contracts authorize via ZK proofs + nullifiers (o-cap model). Every ZK circuit
proves secret key knowledge via `ec_mul_base(secret, NULLIFIER_K)` — this IS the authorization.
A Schnorr signature adds no security and actively harms privacy by deanonymizing the signer
to every verifier. Per ocap.md §2: "The verifier observes only: the predicate result, the
nullifier, and the commitment's inclusion proof. Nothing else." A Schnorr pubkey in metadata
is observable identity information that violates this guarantee.

**Rationale from the red team audit (2026-07-26):** Schnorr signatures in contract metadata
violate ~52 explicit statements across ocap.md, type-system.md, wallet.md, manifest.md,
and contract-wasm-type-system.md. The Authorization Inversion Theorem (type-system.md §6)
requires ZK proofs to invert ACL into O-Cap. Schnorr signatures are not ZK proofs — they
reveal the signer's public key, reverting to identity-based (ACL) authorization. A contract
that returns non-empty signature pubkeys in metadata is non-compliant.

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

Tests SHALL follow: genesis → coinbase → contract call → witness →
transaction → block → accept_block → state check. The DarkLeaf call tree is
extracted from the witness by the execution layer — no client-side wrapping is
needed. Tests verifying only call_data generation without accept_block routing
do not exercise contract function behavior.

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

## 12. Call Data Format — DarkLeaf Tree Architecture

This section documents a critical architectural decision: where the DarkLeaf call tree
lives, why it is not in `ContractCall.data`, and how WASM contracts receive it.

### 12.1 The Two Consumers

Two different subsystems read contract call data, with different needs:

| Consumer | Needs | Reads |
|----------|-------|-------|
| **Chain-level** (coinbase detection, fee extraction, supply verification, block validation) | Function selector byte only | `c.data[0]` — checks `0x00` (FeeV1), `0x05` (PoWRewardV1), `0x06` (FeeCollectV1) |
| **Contract-level** (WASM entrypoints) | Full DarkLeaf call tree for cross-contract child call validation | `calls[child_idx].data` — validates child function selectors, ContractIds, value commitments |

Forcing both consumers to use the same byte representation (Option C: putting the
DarkLeaf tree in `ContractCall.data`) creates false coupling: the chain must unwrap
trees to read selector bytes, and ~20 consensus-critical sites must change. It also
duplicates data (each of N contract calls carries N copies of the same tree).

### 12.2 Chosen Architecture: Tree in Witness, Extracted at Execution

The DarkLeaf call tree (`Vec<DarkLeaf<ContractCall>>`) is the canonical structure of
a core transaction (`dwow_core::tx::Transaction.calls`). The full core transaction is
serialized and stored in the chain transaction's `witness` field
(`type-system.md §8.2`). This witness is:

- **Signed**: The transaction author's Schnorr signatures cover the call data
- **ZK-proven**: Each call's ZK proofs bind to the call's public inputs
- **Reconciled**: `decode_and_reconcile()` verifies that chain-level `ContractCall`
  fields match the witness-embedded core transaction

At execution time (`execution.rs`), the tree is extracted from the witness and passed
to WASM as `job.call_data`. This means:

```
Chain ContractCall.data → raw [fn_code] + params (chain-level operations)
Transaction.witness      → full core tx (authentication + tree storage)
job.call_data            → serialized Vec<DarkLeaf<ContractCall>> (WASM contracts)
```

### 12.3 Why Not in ContractCall.data

The red/blue team audit (2026-07-26) evaluated two approaches:

**Option C (rejected): Put DarkLeaf tree in `ContractCall.data`.**
- ~20 chain-level `data[0]` reads must be refactored (consensus fork risk)
- Tree is P2P-malleable — reconciliation only checks inner payload, not tree indexes
- N copies of the same tree for N contract calls (data duplication)
- Deployooor post-processing and fee extraction must navigate trees to read selectors

**Option B (chosen): Extract tree from witness at execution.rs.**
- 1 site changed (execution.rs), zero consensus-critical sites touched
- Tree is cryptographically attested (signed + ZK-proven in witness)
- Zero data duplication — one tree in witness, served to all calls
- Chain-level code unchanged — continues to read raw `data[0]` for selector checks
- Consistent with existing execution-layer mediation (metadata extraction, spend hooks,
  Deployooor post-processing)

### 12.4 Contract Entrypoint Patterns

With the tree arriving at WASM via execution.rs, contracts use the DarkLeaf
deserialization pattern documented in `contract-wasm-type-system.md §1.3`:

```rust
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = FooFunction::try_from(self_.data[0])?;
    // ... dispatch on func, deserialize params from self_.data[1..]
}
```

For cross-contract child call validation:
```rust
let child_idx = /* index of child call in tree */;
let child_call = &calls[child_idx].data;
if child_call.data[0] != 0x04 {  // TransferV1
    return Err(...);
}
validate_child_contract_id(&child_call.contract_id, &stored_cid)?;
```

NativeToken uses `ix[0]` dispatch (per its consensus-critical role, no child calls).
The execution layer passes raw `call.data` for NativeToken specifically.

### 12.5 Contract Standard for Tree Access

Contracts that perform cross-contract child call validation SHALL:

1. **Validate child function selectors** against expected values (not accept any selector)
2. **Validate child ContractIds** against stored configuration (per §11)
3. **Validate child value commitments** where applicable (Pedersen commitment checks)
4. **Not trust tree topology as authority** — parent/child relationships are structural
   hints authenticated by the witness; authorization flows through ZK proofs and
   nullifiers, not through tree position
5. **Not access sibling calls without explicit authorization** — tree visibility is for
   validation of child calls the contract creates, not for ambient observation of
   unrelated calls in the same transaction

