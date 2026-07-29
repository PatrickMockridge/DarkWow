# Contract WASM Type System

This document defines the DarkWow smart contract WASM type system. It extends the
[Type System Specification](type-system.md) to the contract WASM execution layer.
It SHALL be read in conjunction with the [Type System](type-system.md),
[Capability Composition](composition.md), [Wallet Architecture](wallet.md),
[Contract Manifest](manifest.md), and [O-Cap](ocap.md) specifications.

This specification uses SHALL, MUST, SHALL NOT, MUST NOT per RFC 2119.

## 0. Foundation — Contracts as ρ-Calculus Channels

The ρ-calculus foundation is defined in [type-system.md §0](type-system.md). This
section extends that foundation to contract WASM execution.

### 0.1 Contracts Are Named Channels

A **contract** IS a named channel in the ρ-calculus. The `ContractId`
([type-system.md §8.1](type-system.md)) names the channel. The WASM entrypoints
are input guards on that channel. A `ContractCall { contract_id, data }` is an
output operation `contract_id!(data)`. The contract's `process_instruction` is
the input operation `contract_id?(data).P`.

| ρ-Calculus Construct | Contract WASM Instantiation |
|----------------------|---------------------------|
| Channel `x` | `ContractId` — a `pallas::Base` newtype, Poseidon-derived from deploy key |
| Output `x!(y)` | `ContractCall { contract_id, data }` in a `Transaction` |
| Input `x?(y).P` | `process_instruction(cid, ix)` — the WASM `__entrypoint` export |
| Replication `!P` | The contract WASM is loaded once; every call invokes the same entrypoint |
| Restriction `νx.P` | Per-contract sled trees — keyed by `blake3(contract_id || tree_name)` |

### 0.2 The Contract as a Replicated Process

A contract with four entrypoints is the replicated parallel composition:

```
!init?(ix).P_init | !exec?(ix).P_exec | !apply?(update).P_apply | !metadata?(ix).P_metadata
```

Each entrypoint is a distinct input guard on the same channel. The host invokes
them in strict order: `metadata` → `exec` → (optional `spend_hook`) → `apply`.
Every invocation is an independent WASM instantiation with its own linear memory
and gas budget. No state persists between invocations except through sled trees.

### 0.3 The Fundamental Contract Invariant

> **A contract SHALL accept a ContractCall iff the call's data satisfies
> the contract's declared barbs.**

The contract's manifest declares what barbs each action requires
([manifest.md](manifest.md) — `required_barbs` on `[[actions]]`). The contract's
WASM code SHALL enforce those barbs at runtime. A call that does not satisfy the
declared barbs SHALL return `Err(ContractError::...)`. The host SHALL reject the
block if a canonical call fails ([consensus.md](consensus/consensus.md) Phase 4).

### 0.4 Contracts Are Instances, Not Special Cases

Per [type-system.md §13](type-system.md): a contract name is a human-readable
label for a specific barb composition. It is NOT a special code path. The only
contract with a bespoke wallet path is NativeToken (consensus-critical: block
rewards, fee payment, supply audit). Every other contract — genesis or
user-deployed — SHALL work through the generic manifest-driven machinery.

### 0.5 Contract Taxonomy — Consensus vs Capability

DarkWow distinguishes two categories of contract. The distinction is architectural,
not a privilege hierarchy:

**Consensus contracts** (NativeToken, Deployooor) serve the blockchain itself.
NativeToken SHALL provide coinbases, fee payment, and transfers ONLY. Deployooor
SHALL deploy WASM binaries. Consensus contracts SHALL NOT: freeze assets, enforce
ACLs, implement governance hooks, compose with DeFi contracts, wrap tokens for
other contracts, or dispatch to cross-contract calls. They are rock-dumb by design
— every feature removed is an attack surface eliminated.

**Capability contracts** (PromissoryNote, all genesis and user-deployed contracts)
compose via generated/shared/revoked capabilities. They use manifest-driven wallet
discovery (wallet.md §2, Path 2). Their authorization flows through ZK proofs and
nullifiers, not identity or ACL. PromissoryNote is the DeFi token layer — it mints,
burns, transfers, and revokes capabilities that compose across contracts.

A consensus contract SHALL NOT contain code for any other contract. No bridge
logic. No wrapping. No DeFi composition. If a DeFi contract needs native token,
it builds its own bespoke infrastructure — it wraps the native token's coin type
in its own capability, with its own circuits and its own manifest. This is a
DESIGN RULE, not a limitation. Per the memory rule `native-token-rock-dumb`: coins
ARE capabilities via nullifiers; DeFi complexity lives in PromissoryNote and
user-deployed contracts.

## 1. Contract Entrypoint Types

### 1.1 The Four Canonical Entrypoints

The `dwow_sdk::define_contract!` macro (`src/sdk/src/wasm/entrypoint.rs:82`)
generates four `#[no_mangle] extern "C"` functions. Each corresponds to a
specific `ContractSection` which governs ACL access to host functions.

| Entrypoint | WASM Export | Macro Field | Signature | Barbs Exhibited | ACL Section |
|-----------|------------|------------|-----------|-----------------|-------------|
| `init` | `__initialize` | `init:` | `fn(ContractId, &[u8]) -> ContractResult` | `↓dispatch` | `Deploy` |
| `exec` | `__entrypoint` | `exec:` | `fn(ContractId, &[u8]) -> ContractResult` | `↓dispatch`, `↓gate`, function-specific barbs | `Exec` |
| `apply` | `__update` | `apply:` | `fn(ContractId, &[u8]) -> ContractResult` | `↓commit` (state writes) | `Update` |
| `metadata` | `__metadata` | `metadata:` | `fn(ContractId, &[u8]) -> Vec<u8>` | `↓verify` (returns ZK public inputs; signature pubkeys SHALL be empty) | `Metadata` |

A variant macro `define_contract_with_spend_hook!` (`entrypoint.rs:101`) adds a
fifth export `__spend_hook` for contracts that receive burn callbacks from
PromissoryNote's `RevokeV1` function. Currently only Stablecoin uses this.

The host invokes the sections in strict order per the execution pipeline
(`src/linear/src/execution.rs:296-461`):
1. `runtime.metadata(&call_data)` — calls `__metadata`
2. `runtime.exec(&call_data)` — calls `__entrypoint`
3. If spend hook emitted: `runtime.spend_hook(payload)` → `runtime.apply(ret)`
4. `runtime.apply(&update)` — calls `__update`

Canonical call failure at any step SHALL reject the block. Uncle call failure
produces `success: false` in `uncle_results` but does not reject the block
(`execution.rs:325-332, 406-418`).

### 1.2 Function Dispatch

The first byte of the call data is the **function selector**. The contract SHALL
define an enum implementing `TryFrom<u8>`. Two patterns exist:

**Pattern A — Manual enum** (native_token, promissory_note, deployooor, identity):
```rust
#[repr(u8)]
enum XxxFunction {
    TransferV1 = 0x00,
    BurnV1 = 0x01,
    // ...
}
impl TryFrom<u8> for XxxFunction {
    type Error = ContractError;
    fn try_from(v: u8) -> Result<Self, Self::Error> { /* ... */ }
}
```

**Pattern B — Macro** (`src/sdk/src/primitives.rs:147`):
```rust
define_contract_function!(XxxFunction, u8, ContractError, {
    TransferV1 = 0x00,
    BurnV1 = 0x01,
    // ...
});
```

The dispatch SHALL be exhaustive. `#[deny(unreachable_patterns)]` SHALL be on
the `match` statement. An unknown selector SHALL return `ContractError::InvalidFunction`
(`↓bad-gate`). The contract SHALL NOT silently ignore unknown selectors.

### 1.3 Contract Call Payload

The WASM runtime copies `ContractCall.data` to linear memory. The format written
by the host (`src/runtime/vm_runtime.rs:752-760`) is:

```
[contract_id: 32 bytes] [payload_len: 8 bytes LE u64] [payload: variable]
```

The `payload` begins with the function selector byte. The contract receives the
complete `ContractCall.data` as `ix: &[u8]`. The function selector is `ix[0]`;
parameters are `ix[1..]`.

For contracts that process calls as a tree (PromissoryNote, Purse, Box, MultiSig,
Oracle, Attestation, Identity, Stablecoin, InsuranceMarket), the instruction data
deserializes as `Vec<DarkLeaf<ContractCall>>` and the current call is indexed by
`wasm::util::get_call_index()`.

### 1.4 Parameter Encoding

Parameters SHALL be serializable via `dwow_serial` (`Encodable`/`Decodable`).
Every `*ParamsV1` struct SHALL provide a thin bridge impl that delegates to the
struct's inherent `encode()`/`decode()` methods. The inherent methods SHALL use
fixed byte layouts with validating constructors (§3.1). Derive macros
(`SerialEncodable`/`SerialDecodable`) SHALL NOT be used — they bypass validating
constructors. The parameter type SHALL be a named struct (`XxxParamsV1`), not a
raw tuple or `Vec<u8>`. The contract SHALL validate parameter lengths before
deserialization:

```rust
fn fee_v1(cid: ContractId, params: &[u8]) -> ContractResult {
    // FeeV1 call data: [fee: u64 LE 8 bytes][FeeParamsV1: variable]
    if params.len() < 9 {
        return Err(ContractError::IoError("FeeV1: insufficient call data".to_string()));
    }
    let fee: u64 = deserialize(&params[0..8])?;
    let fee_val: FeeParamsV1 = deserialize(&params[8..])?;
    // ...
}
```

Reference: `src/contract/native_token/src/entrypoint/mod.rs:466-469`.

### 1.5 Return Value Encoding

The `exec` entrypoint writes return data via the `set_return_data` host function:

```rust
wasm::util::set_return_data(&serialize(&(XxxFunction::XxxV1 as u8, update)))
```

This SHALL be a tuple of `(function_selector_byte, serialized_update_struct)`.
The `apply` entrypoint reads this via `update_data[0]` dispatch:

```rust
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = XxxFunction::try_from(update_data[0])?;
    match func {
        XxxFunction::XxxV1 => {
            let update: XxxUpdateV1 = deserialize(&update_data[1..])?;
            apply_xxx(cid, update)
        }
        // ...
    }
}
```

A contract that fails to call `set_return_data` in `exec` produces an empty
`Vec<u8>` — the host-side default from `contract_return_data.take().unwrap_or_default()`
(`vm_runtime.rs:528`). This SHALL cause `apply` to fail with an invalid selector error.

### 1.6 Metadata Contract

The `metadata` entrypoint SHALL return the ZK public inputs the host needs to verify proofs:

```
serialize(Vec<(String, Vec<pallas::Base>)>) || serialize(Vec<PublicKey>)
```

The signature pubkeys component SHALL be empty `vec![]`. Authorization is via
ZK proof + nullifier (ocap.md §6.2) — Schnorr signatures are prohibited in
contract metadata (see contract-standards.md §3 for the full rationale).

The host at `src/linear/src/execution.rs` deserializes this, accumulates
all proofs across all canonical calls, then calls
`crate::zk_verifier::verify_core_tx_with_tables()` for ZK proof verification.

The metadata for a function SHALL match the function's manifest-declared circuits.
Each call's metadata SHALL include ALL ZK proof public inputs for that call.
A function with no proofs SHALL return an empty ZK inputs vector (encoded).

The metadata entrypoint is called BEFORE `exec`, so the host can verify proofs
and signatures before trusting the contract's state transition. A contract whose
metadata fails to include a required proof SHALL cause the block to be rejected.

Reference: `src/contract/native_token/src/entrypoint/mod.rs:198-217` (get_metadata
dispatch), `:220-272` (fee_get_metadata).

## 2. Barbs at Contract Entrypoints

### 2.1 The Barb Model

A **barb** is an observable action ([type-system.md §1.1](type-system.md)). Every
contract entrypoint exhibits specific barbs. The manifest declares the barbs at
the action level (`required_barbs` on `[[actions]]`). The contract WASM code
SHALL enforce those barbs at runtime. The host ACL (§4.6) enforces barb
constraints at the section level.

The mapping from barbs to contract operations:

| Barb | Contract Operation | Entrypoint | Enforcement |
|------|-------------------|------------|-------------|
| `↓dispatch` | Route call to correct contract | `exec` | Host: calls the WASM for the ContractId in the call |
| `↓gate` | Constrain to declared function | `exec` | Contract: function selector dispatch (exhaustive match) |
| `↓verify` | Return ZK public inputs + signature pubkeys | `metadata` | Host: verifies proofs and signatures against metadata |
| `↓spend` | Verify key possession | `exec` | Contract: checks signature_public against held keys |
| `↓nullify` | Prevent double-spend | `exec` | Contract: checks SMT for nullifier reuse |
| `↓prove-inclusion` | Verify Merkle inclusion | `exec` | Contract: checks Merkle root in stored roots tree |
| `↓commit` | Write state update | `apply` | Contract: `db_set`, `merkle_add`, SMT insert |
| `↓denominate` | Validate asset class | `exec` | Contract: compares token_commit against known asset |
| `↓derive` | Derive per-instance key | `exec` | Contract: `poseidon_hash([secret, cid, instance])` |

### 2.2 Exec Barbs

The `exec` entrypoint exhibits the barbs that the manifest declares for the
function's action. For a transfer action declaring
`required_barbs = ["Spend", "Nullify", "Commit", "Dispatch", "Gate", "Denominate"]`,
the contract SHALL:

1. **`↓gate`** — Dispatch to the correct function handler by the selector byte.
2. **`↓spend`** — Verify that the nullifier was produced by knowledge of the
   capability's authorization secret (`poseidon_hash(secret, resource_id)`).
   The ZK circuit proves secret key knowledge via `ec_mul_base(secret, NULLIFIER_K)`
   and constrains the nullifier as a public input. No Schnorr signature is involved.
3. **`↓nullify`** — Check that the input's nullifier is not already in the
   nullifier SMT (`smt.get_leaf(&nullifier) == pallas::Base::zero()`).
4. **`↓prove-inclusion`** — Verify that the input's Merkle root exists in the
   coin roots tree (`db_contains_key(coin_roots_db, &serialize(&merkle_root))`).
5. **`↓denominate`** — Verify token commitments match the expected asset
   (`token_commit == poseidon_hash([zero(), zero()])` for native token).

A barb that is declared but not enforced is a lie — the manifest claims a
security property the code does not provide. A barb that is enforced but not
declared is a hidden constraint — the wallet's coverage gate cannot verify it,
and the capability type construction fails.

Reference: `src/contract/native_token/src/entrypoint/mod.rs:466-499` (fee_v1
barb enforcement), `:768-885` (pow_reward_v1 barb enforcement).

### 2.3 Apply Barbs

The `apply` entrypoint exhibits `↓commit`. It SHALL only write state; it SHALL
NOT perform validation. All validation SHALL have been done in `exec`. The apply
entrypoint:

1. Dispatches on the function selector byte from the return data
2. Deserializes the update struct
3. Writes to sled trees: `db_set`, `merkle_add`, `sparse_merkle_insert_batch`
4. SHALL NOT call `db_get`, `db_contains_key`, or any read operation — reads
   belong in `exec`

The ACL (§4.6) enforces this: `db_set` and `db_del` are only allowed in `Deploy`
and `Update` sections; read operations are denied in `Update`.

### 2.4 Metadata Barbs

The `metadata` entrypoint exhibits `↓verify`. It SHALL:

1. Dispatch on the function selector byte
2. Deserialize the function-specific parameters
3. Surface ALL ZK public inputs for the function's declared circuits (one entry
   per circuit, each with the ordered list of `pallas::Base` public inputs)
4. Return empty signature pubkeys `vec![]` — Schnorr signatures are prohibited in contract metadata (contract-standards.md §3)
5. NOT access state (no `db_get` calls — metadata is a pure function of the call
   data)
6. Return empty vectors (not error) on parameter deserialization failure — the
   host verifier will reject the call because it has no proofs to verify

A metadata entrypoint that returns empty vectors when the manifest declares
`requires_proof = true` is a spec violation — the host has nothing to verify and
must trust the contract's `exec` without ZK verification.

### 2.5 Barb Declaration

Every contract SHALL declare in its manifest which barbs each action requires.
The contract's WASM code SHALL enforce every declared barb. The relationship is
normative:

```
manifest::required_barbs ⊆ watcher_observed_barbs(contract_WASM)
```

A manifest that declares barbs the WASM does not enforce is a false claim. A
contract whose WASM enforces barbs not in the manifest is a hidden constraint.
Both SHALL be detected by WASM verification ([manifest.md](manifest.md) Trust
Model, Layer 2).

Reference: `src/contract/native_token/manifest.toml:62-88` (actions with
required_barbs), `src/contract/identity/manifest.toml` (actions with required_barbs).

## 3. State Type System

### 3.1 Encoding Taxonomy — Four Boundaries

There are FOUR distinct encoding boundaries in a contract. Each uses a different
mechanism. Confusing them is the root cause of every encoding regression found in
the 2026-07-29 HAZOP review. The fourth boundary was discovered during the Box/Purse
L1 HAZOP and is the most frequently broken.

**Boundary 1 — Entrypoint parameters (external → exec).** Call data arrives from
an externally-constructed transaction (Rust host, wallet client, RPC). The exec
entrypoint SHALL deserialize parameters via the type's inherent `decode()` method:

```rust
let params = FooParamsV1::decode(params)?;
```

Each `*ParamsV1` struct SHALL provide both:
- An inherent `pub fn decode(data: &[u8]) -> Result<Self, ContractError>` with
  fixed byte layout and validating constructors
- A thin bridge impl `impl dwow_serial::Decodable for FooParamsV1` that delegates
  to the inherent method (see §3.1.2 for the canonical pattern)

**Boundary 2 — Exec→Apply bridge (internal).** The exec entrypoint produces an
update struct consumed by the apply entrypoint. This bridge SHALL use custom
`encode_*_update_v1()` / `decode_*_update_v1()` functions:

Exec side:
```rust
wasm::util::set_return_data(&encode_foo_update_v1(&update))?;
```

Apply side:
```rust
let update = decode_foo_update_v1(&update_data[1..])?;
```

The custom bridge functions SHALL delegate to the struct's inherent `encode()`/
`decode()` methods. Each `*UpdateV1` struct SHALL also provide thin bridge impls
for `dwow_serial::Encodable`/`Decodable` delegating to the inherent methods,
for use by test code and client helpers (see §3.1.2).

Reference implementations: native_token (encode_fee_update_v1, decode_fee_update_v1
etc. at `src/contract/native_token/src/entrypoint/mod.rs`), purse, box, deployooor.

**Boundary 3 — Sled state (persistence).** Values written to and read from sled
trees SHALL use the type's inherent `encode()`/`decode()` methods directly. These
methods SHALL use fixed byte layouts with validating constructors (§3.1.1).
`dwow_serial` derive macros (`SerialEncodable`/`SerialDecodable`) SHALL NOT be
applied to types stored in sled trees — they bypass validating constructors and
produce opaque byte blobs with no compile-time format guarantee.

Sled-only types (Coin, Nullifier, Purse, BoxId, GroupId, and other internal
state types) SHALL have inherent `encode()`/`decode()` methods. They SHALL NOT
have `dwow_serial` bridge impls — they never cross the exec→apply boundary.

**Summary table:**

| Boundary | Dispatched by | Method | Bridge impl required? |
|----------|-------------|--------|----------------------|
| External → exec | `process_instruction` | `FooParamsV1::decode(params)?` | YES (Decodable) |
| Exec → apply | `process_instruction` → `process_update` | `encode_foo_update_v1()` / `decode_foo_update_v1()` | YES (Enc+Dec) |
| Sled state | `db_set` / `db_get` | `foo.encode()` / `Foo::decode(&data)?` | NO |
| Circuit → metadata | `get_metadata` → `verify_zkp` | `metadata[i] == proof_instance[i]` for all i | N/A (positional match) |

**Boundary 4 — Circuit public inputs → Metadata return value.** The ZK circuit
publishes values via `constrain_instance(X)` in a fixed order. The metadata
function must return a vector of public inputs where `metadata[i] ==
proof_instance[i]` for every position. This is a positional encoding boundary —
the order, count, and byte representation must match exactly.

This boundary is the most frequently broken because values cross from the
circuit domain (zkas assembly, domain-separated Poseidon hashes) to the
Rust domain (contract entrypoint, SDK hash functions). The HAZOP found that
every `constrain_instance` value derived from a `poseidon_hash` call was
mismatched — the circuit used domain-separated hashes (HAZOP RC3) while
the Rust metadata function used bare hashes without domain constants.

**Two patterns for correct crossing (see privacy.md §5.1):**

**Pattern A — Pass through params**: For values that depend on witness-only
inputs (`owner_secret`, `balance_blind`), the caller pre-computes the value,
passes it in the params struct, the circuit constrains
`constrain_equal_base(circuit_computed, params_value)` then
`constrain_instance(params_value)`, and the metadata echoes `params.value`.
This is the only correct pattern when the metadata function lacks the inputs
to compute the value itself.

**Pattern B — Replicate domain constants**: For values computable from
available data, the Rust-side metadata function recomputes with the same
domain constants as the circuit. Every `poseidon_hash` call in the circuit
prepends a `witness_base(N)` domain constant as its first argument; the
Rust-side computation MUST prepend `pallas::Base::from(N)` identically.

**The invariant**: `metadata[i] == proof_instance[i]`. A mismatch in any of:
input count, input order, domain constant value, or hash domain produces a
proof verification failure. The circuit's `constrain_instance` order IS the
schema; the metadata function MUST conform.

### 3.1.1 Explicit Encoding Rules

The explicit encode/decode pattern SHALL follow these rules.

**Encoding pattern:**

```rust
/// Encode a Purse to canonical fixed-offset bytes (ρ-calculus: quote).
fn encode_purse(purse: &Purse) -> Vec<u8> {
    let mut buf = Vec::with_capacity(129);
    buf.push(purse.version);
    buf.extend_from_slice(&purse.purse_id.to_bytes());
    buf.extend_from_slice(&purse.token_commit.to_repr());
    buf.extend_from_slice(&purse.balance_commit.to_bytes());
    buf.extend_from_slice(&purse.owner_commit.to_repr());
    buf
}

/// Decode a Purse from canonical fixed-offset bytes (ρ-calculus: eval).
/// Every field SHALL be validated through its named constructor.
fn decode_purse(data: &[u8]) -> Result<Purse, ContractError> {
    if data.len() != 129 {
        return Err(ContractError::IoError(format!(
            "Purse: expected 129 bytes, got {}", data.len()
        )));
    }
    let version = data[0];
    let purse_id = PurseId::from_bytes(data[1..33].try_into().unwrap())
        .ok_or_else(|| ContractError::IoError("Purse: invalid purse_id".into()))?;
    let token_commit = Option::<pallas::Base>::from(
        pallas::Base::from_repr(data[33..65].try_into().unwrap())
    ).ok_or_else(|| ContractError::IoError("Purse: invalid token_commit".into()))?;
    let balance_commit = Option::<pallas::Point>::from(
        pallas::Point::from_bytes(data[65..97].try_into().unwrap())
    ).ok_or_else(|| ContractError::IoError("Purse: invalid balance_commit".into()))?;
    let owner_commit = Option::<pallas::Base>::from(
        pallas::Base::from_repr(data[97..129].try_into().unwrap())
    ).ok_or_else(|| ContractError::IoError("Purse: invalid owner_commit".into()))?;
    Ok(Purse { version, purse_id, token_commit, balance_commit, owner_commit })
}
```

Rules:
- Fixed byte layout with exact offsets. No variable-length codec.
- Every cryptographic type through its validating constructor (`from_bytes`).
- `Vec<T>` fields SHALL use a fixed-width length prefix (u8 for counts < 256).
- No `unwrap_or` — every validation failure is a `ContractError` with field context.
- Pre-allocate exact capacity in encode: `Vec::with_capacity(FIXED_SIZE)`.

### 3.1.2 Anti-Pattern: Derive-Based serialize/deserialize for State Values

`SerialEncodable`/`SerialDecodable` derive macros SHALL NOT be applied to
types stored in sled trees. The derive macros produce opaque byte blobs with
no compile-time format guarantee. This is equivalent to a raw `unwrap()` or a
bare integer cast — the type's barbs are stripped, and the bytes carry no
behavioral constraints (per type-system.md §2.2: "Bytes Round-Trip Is Forbidden").

**Failure mode:** A Purse struct stored via `serialize(&purse)` was observed to
produce a 9-byte value instead of the expected 129 bytes. The derived `Decodable`
accepted this, producing a corrupt Purse with zeroed fields. The error surfaced
only as `ContractError::IoError("Unknown")` at the WASM boundary — the original
error context was irretrievably lost.

**BEFORE (anti-pattern):**
```rust
#[derive(SerialEncodable, SerialDecodable)]
pub struct Purse { ... }

// Write
wasm::db::db_set(db, &key, &serialize(&purse))?;
// Read
let purse: Purse = deserialize(&data)?;
```

**AFTER (compliant):**
```rust
// No SerialEncodable/SerialDecodable on Purse.
// Explicit encode/decode with validating constructors.

// Write
wasm::db::db_set(db, &purse_id.to_bytes(), &purse.encode())?;
// Read
let purse = Purse::decode(&data)?;
```

**Detection:** A tripwire test SHALL grep for `serialize(&` in `db_set` argument
position on non-scalar types. A match SHALL fail CI.

### 3.1.3 Canonical Bridge Impl Pattern

For every `*ParamsV1` and `*UpdateV1` struct that crosses the exec→apply boundary,
provide thin bridge impls immediately before the struct's `impl` block. These
delegate to the struct's inherent `encode()`/`decode()` methods — they do NOT
bypass validating constructors:

```rust
impl dwow_serial::Encodable for FeeParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for FeeParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
```

These are one-liners. They exist for test code, client helpers, and the rare case
where a Params struct is deserialized via `dwow_serial::deserialize()` in exec
entrypoints. They SHALL NOT be added to sled-only state types (Coin, Nullifier,
Purse, BoxId, GroupId, etc.) — those never cross the exec→apply boundary.

Reference: `src/contract/native_token/src/model/mod.rs` lines 407-408, 490-491,
570-571, 1033-1034.

### 3.2 State Keys Are Typed

Every sled key SHALL use the canonical byte representation of a typed value.
Keys SHALL be constructed via `to_bytes()` / `to_le_bytes()` / `to_repr()`,
not via `dwow_serial::serialize()`. Tree names SHALL be declared in the
manifest's `[[trees]]` section. Key types SHALL be traceable to the contract's
model definitions.

```rust
// Compliant: key is BlockHeight via to_le_bytes
let height_key = verifying_block_height.to_le_bytes();

// Compliant: composite key from typed values
let mut supply_key = TOTAL_SUPPLY.to_vec();
supply_key.extend_from_slice(&params.token_id.to_repr());

// Compliant: Nullifier as DB key via to_bytes
wasm::db::db_get(sigs_db, &nullifier.to_bytes())?;

// Violation: magic string key
let key = b"total_supply_v1";
```

### 3.3 State Values Are Typed

Every sled value SHALL decode to a known type through that type's explicit
`decode()` function. The type SHALL be traceable to the contract's model
definitions (`model/mod.rs`). A value that fails to decode SHALL produce a
typed error identifying the field that failed validation — never a default.

### 3.4 The `unwrap_or` Prohibition

`decode(&data).unwrap_or(default)` and `deserialize(&data).unwrap_or(default)`
SHALL NOT appear in contract code. This pattern silently substitutes a default
when decoding fails, conflating:

- "sled returned corrupt bytes" with "value is zero"
- "encoding format changed" with "state not yet initialized"
- "attacker supplied malformed bytes in a prior block" with "genesis state"

**Violations found in the codebase (2026-07-22):**

| Location | Pattern | Severity | Impact |
|----------|---------|----------|--------|
| `native_token/src/entrypoint/mod.rs:826-828` | Double `unwrap_or(0)` on `TOTAL_SUPPLY` | **Medium** | Supply audit bypass — corrupt state reads as 0 supply. Partially mitigated by independent Pedersen cumulative commitment check (lines 841-873) |
| `promissory_note/src/entrypoint/mod.rs:554-555` | Double `unwrap_or(0)` on per-token coin count | **Low** | Diagnostic counter reset — ZK proof independently constrains value conservation |
| `stablecoin/src/client/mod.rs:506-507` | `current_price.unwrap_or(0)` + `liquidator_reward.unwrap_or(0)` in LiquidateBuilder | **Low** | Client-side footgun — produces zero-price liquidation if caller omits field |
| `insurance_market/src/client/mod.rs:255,328` | `calculate_premium(...).unwrap_or(0)` on arithmetic overflow | **Low-Medium** | Zero-premium coverage purchase. On-chain entrypoint independently validates premium |
| `stablecoin/src/client/initialize_v1.rs:111` | `initial_supply.unwrap_or(0)` in InitializeCallBuilder | **Low** | Client-side footgun — produces zero-supply token initialization |

**Compliant pattern:**

```rust
// BEFORE (violation):
let current_supply: u64 = wasm::db::db_get(info_db, TOTAL_SUPPLY)?
    .map(|data| deserialize(&data).unwrap_or(0))
    .unwrap_or(0);

// AFTER (compliant):
let current_supply: u64 = match wasm::db::db_get(info_db, TOTAL_SUPPLY)? {
    Some(data) => u64::decode(&data).map_err(|e| {
        msg!("[contract::function] Error: Failed to decode TOTAL_SUPPLY: {:?}", e);
        ContractError::IoError("Corrupt state: TOTAL_SUPPLY decoding failed".to_string())
    })?,
    None => {
        msg!("[contract::function] Error: TOTAL_SUPPLY not found in state");
        return Err(ContractError::IoError("Missing state: TOTAL_SUPPLY".to_string()))
    }
};
```

### 3.5 State Initialization

State SHALL be initialized in the `init` entrypoint (`__initialize`, `Deploy`
section). The `init` entrypoint SHALL:

1. Set up all declared sled tree databases via `wasm::db::db_lookup`
2. Seed initial values (genesis state, empty tree roots, version markers)
3. Store ZK circuit binaries via `wasm::db::zkas_db_set`
4. Be idempotent — use `db_contains_key` before initializing to avoid
   overwriting existing state on re-deployment

A state read in `exec` or `apply` that finds no value SHALL return an error,
UNLESS the spec explicitly defines the absent value as semantically meaningful
(e.g., "genesis block has no prior cumulative supply commitment — use identity
point"). When absent IS semantically meaningful, the contract SHALL use an
explicit `match` with a comment explaining the semantics — never `unwrap_or`.

```rust
// Compliant: explicit semantics for "not yet initialized"
let old_cumulative = match wasm::db::db_get(info_db, CUMULATIVE_VALUE_COMMIT)? {
    Some(data) => deserialize::<pallas::Point>(&data)?,
    None => {
        // Genesis block: no prior cumulative supply commitment exists.
        // Use the identity point as the additive identity for Pedersen accumulation.
        pallas::Point::identity()
    }
};
```

Reference: `src/contract/native_token/src/entrypoint/mod.rs:91-189` (init_contract),
`:841-844` (explicit None handling for genesis cumulative supply).

### 3.6 Type-Safe State Wrappers

Contracts SHOULD define typed wrapper functions for state access. Each wrapper
SHALL have exactly one responsibility: one state key, one type, validated decode.

```rust
/// Read the current total supply from state.
/// Returns Err if the key is missing or the stored value fails to decode.
fn read_total_supply(info_db: DbHandle) -> Result<u64, ContractError> {
    match wasm::db::db_get(info_db, &TOTAL_SUPPLY_KEY.to_le_bytes())? {
        Some(data) => {
            let bytes: [u8; 8] = data.try_into().map_err(|_| {
                ContractError::IoError("Corrupt state: TOTAL_SUPPLY wrong size".into())
            })?;
            Ok(u64::from_le_bytes(bytes))
        }
        None => {
            msg!("[contract] Missing state: TOTAL_SUPPLY");
            Err(ContractError::IoError("Missing state: TOTAL_SUPPLY".into()))
        }
    }
}

/// Write the total supply to state.
fn write_total_supply(info_db: DbHandle, supply: u64) -> Result<(), ContractError> {
    wasm::db::db_set(info_db, &TOTAL_SUPPLY_KEY.to_le_bytes(), &supply.to_le_bytes())
}
```

### 3.7 State Key Canonical Encoding

The canonical byte encoding of a state key SHALL be via the type's typed
`to_bytes()` / `to_le_bytes()` / `to_repr()` method, as defined in §3.2.
Keys SHALL be constructed from typed values, not from raw byte buffers.
`BlockHeight` keys SHALL use `BlockHeight::to_le_bytes()` per
[type-system.md §2.3](type-system.md). `ContractId` keys SHALL use
`ContractId::to_bytes()` per [type-system.md §2.2](type-system.md).
Composite keys SHALL concatenate the typed representations of their
components.

### 3.8 Execution Overlay Semantics

Contract state reads and writes operate through a `SledTreeOverlay`
(`src/linear/src/execution.rs`). Within a block:

- **Layer 1 (per-call atomicity):** Each call's writes are checkpointed. On
  failure, the overlay reverts to the pre-call checkpoint. Zero writes from
  failed calls survive.
- **Layer 2 (sequential visibility):** All canonical calls share a SINGLE
  overlay. Call N observes the committed writes of calls 1..N-1. This is the
  sequential execution model — same block order, same state.
- **Layer 3 (block-commit atomicity):** All canonical writes across all calls
  are committed as a single sled cross-tree transaction. Partial block failure
  is not possible — either all writes land, or none do.

Uncle calls execute against isolated pre-block clones. Their diffs are merged
after canonical execution with canonical-wins conflict resolution.

## 4. WASM Host Interface Types

### 4.1 Host Function Architecture

The WASM host provides 29 functions to contracts via the `"env"` import module
(`src/runtime/import/`). All host functions use
`#[link(wasm_import_module = "env")] extern "C"` with raw pointer+length
arguments and `i64` return values.

### 4.2 Host Function Categories

**Database functions** (`src/sdk/src/wasm/db.rs`):

| Function | Signature (WASM side) | Operation | ACL Section |
|----------|----------------------|-----------|-------------|
| `db_init_` | `fn(ptr: *const u8, len: u32) -> i64` | Create new tree | `Deploy` only |
| `db_lookup_` | `fn(ptr: *const u8, len: u32) -> i64` | Get handle to existing tree | `Deploy`, `Metadata`, `Exec`, `Update` |
| `db_get_` | `fn(ptr: *const u8, len: u32) -> i64` | Read value by key | `Deploy`, `Metadata`, `Exec` |
| `db_contains_key_` | `fn(ptr: *const u8, len: u32) -> i64` | Check key existence | `Deploy`, `Metadata`, `Exec` |
| `db_set_` | `fn(ptr: *const u8, len: u32) -> i64` | Write value by key | `Deploy`, `Update` |
| `db_del_` | `fn(ptr: *const u8, len: u32) -> i64` | Delete key | `Deploy`, `Update` |
| `zkas_db_set_` | `fn(ptr: *const u8, len: u32) -> i64` | Store ZK circuit binary | `Deploy` only |

**Utility functions** (`src/sdk/src/wasm/util.rs`):

| Function | Signature | Returns | ACL Section |
|----------|-----------|---------|-------------|
| `set_return_data_` | `fn(ptr: *const u8, len: u32) -> i64` | `0` (success) | `Metadata`, `Exec` |
| `get_object_bytes_` | `fn(ptr: *const u8, len: u32) -> i64` | `0` (success) | `Deploy`, `Metadata`, `Exec`, `Update` |
| `get_object_size_` | `fn(len: u32) -> i64` | size as i64 | `Deploy`, `Metadata`, `Exec`, `Update` |
| `get_verifying_block_height_` | `fn() -> i64` | height as i64 | `Deploy`, `Metadata`, `Exec` |
| `get_block_target_` | `fn() -> i64` | target as i64 | `Deploy`, `Metadata`, `Exec` |
| `get_tx_hash_` | `fn() -> i64` | object index | `Deploy`, `Metadata`, `Exec` |
| `get_call_index_` | `fn() -> i64` | call index as i64 | `Deploy`, `Metadata`, `Exec` |
| `get_blockchain_time_` | `fn() -> i64` | timestamp as i64 | `Deploy`, `Metadata`, `Exec` |
| `get_last_block_height_` | `fn() -> i64` | height as i64 | `Deploy`, `Metadata`, `Exec` |
| `get_tx_` | `fn(ptr: *const u8) -> i64` | object index | `Deploy`, `Metadata`, `Exec` |
| `get_tx_location_` | `fn(ptr: *const u8) -> i64` | object index | `Deploy`, `Metadata`, `Exec` |
| `get_block_hash_` | `fn(height: i64) -> i64` | object index | `Deploy`, `Metadata`, `Exec` |
| `emit_spend_hook_` | `fn(target_cid_ptr, target_cid_len, payload_ptr, payload_len) -> i64` | `0` (success) | `Exec` only |

**Merkle functions** (`src/sdk/src/wasm/merkle.rs`):

| Function | Signature | Operation | ACL Section |
|----------|-----------|-----------|-------------|
| `merkle_add_` | `fn(ptr: *const u8, len: u32) -> i64` | Append leaf to Merkle tree | `Update` only |
| `sparse_merkle_insert_batch_` | `fn(ptr: *const u8, len: u32) -> i64` | Batch insert into SMT | `Update` only |

### 4.3 Object Store Pattern

Data returned from the host to WASM is NOT written directly into WASM memory.
Instead:

1. Host pushes data into `env.objects: Vec<Vec<u8>>`
2. Host returns the index (`objects.len() - 1`) as `i64`
3. Guest calls `get_object_size(index)` to get data length
4. Guest allocates a buffer in WASM memory
5. Guest calls `get_object_bytes(ptr, index)` to copy data from host to WASM

This indirection is the host-side boundary defense — the host never writes
directly into guest memory. The guest controls allocation and the host-side
object store is cleared between WASM invocations.

### 4.4 Error Code Encoding

Host functions signal errors via the `to_builtin!` macro
(`src/sdk/src/error.rs:112-116`): `i64::MIN + error_code`. Error codes are
small positive integers (1-24). Success is `0_i64`. Normal return values
(indices, handles) are non-negative, so `i64::MIN + error_code` cannot collide
with valid returns.

The contract-side `ContractResult` is `Result<(), ContractError>`. The WASM
entrypoint returns `0_i64` on success and a negative error code on failure.
The host maps this back through `to_builtin!` to produce the `ContractResult`.

### 4.5 Width Conversions at the FFI Boundary

WASM uses i64 for its ABI. The host SHALL convert `u64` ↔ `i64` at the boundary:

- `get_verifying_block_height_`: Host converts `BlockHeight` → `i64` via
  `i64::try_from(height.get())`. Guest converts back via `u64::try_from(ret)`
  then `BlockHeight::new(height)`.
- `get_blockchain_time_`: Same conversion for `BlockTimestamp`.
- `get_block_hash_`: Receives `i64` parameter. Host performs
  `u64::try_from(height)` with explicit error on negative values, then
  `BlockHeight::new(height)`.

`try_from` SHALL be used at every width conversion. Bare `as` casts SHALL NOT
appear at the FFI boundary. A value that does not fit in the target type SHALL
be a `ContractError`, not a silent truncation.

### 4.6 Memory Access Discipline

Every host function that reads WASM memory SHALL:

1. Create a `WasmPtr<u8>` slice from the guest pointer via
   `ptr.slice(&memory_view, len)`
2. Copy data into a host-side `Vec<u8>` buffer
3. Deserialize the buffer via `dwow_serial::Decodable`
4. Validate that trailing bytes are zero (trailing bytes = potential injection
   vector — data appended after what the codec consumed)

No host function SHALL trust guest-provided data without validation. The
`Decodable` implementation SHALL consume exactly the declared fields; any
remaining bytes SHALL cause a decode error.

### 4.7 ACL Enforcement

Every host function SHALL enforce which `ContractSection` it can be called from
via `acl_allow(env, &[allowed_sections])`. The four ACL sections correspond to
the four entrypoints:

| Section | Entrypoint | Allowed Operations |
|---------|-----------|-------------------|
| `Deploy` | `__initialize` | `db_init`, `db_lookup`, `db_get`, `db_contains_key`, `db_set`, `db_delete`, `zkas_db_set`, all read-only getters, `get_object_*` |
| `Metadata` | `__metadata` | `db_lookup`, `db_get`, `db_contains_key`, `set_return_data`, all read-only getters, `get_object_*` |
| `Exec` | `__entrypoint` | `db_lookup`, `db_get`, `db_contains_key`, `set_return_data`, `emit_spend_hook`, all read-only getters, `get_object_*` |
| `Update` | `__update` | `db_lookup`, `db_set`, `db_delete`, `merkle_add`, `sparse_merkle_insert_batch`, `get_object_*` |

Key restrictions:
- `db_set` and `db_del` are **only** allowed during `Deploy` and `Update`
- `set_return_data` is **only** allowed during `Metadata` and `Exec`
- `merkle_add` and `sparse_merkle_insert_batch` are **only** allowed during `Update`
- `emit_spend_hook` is **only** allowed during `Exec`
- `zkas_db_set` is **only** allowed during `Deploy`

This is the runtime enforcement of the barb constraints — `↓commit` (state
writes) is only possible in the `Update` section; `↓verify` (returning metadata)
is only possible in `Metadata` and `Exec`.

### 4.8 Gas Metering

The wasmer `Metering` middleware charges one gas unit per WASM opcode. Host
functions additionally charge gas proportional to the operation's cost (data
copy size, ZK validation, SMT operations).

| Limit | Value | Scope |
|-------|-------|-------|
| `GAS_LIMIT` | 400,000,000 | Per contract call |
| `BLOCK_GAS_LIMIT` | 100,000,000,000 | Per block (all calls) |

Gas exhaustion SHALL cause the WASM execution to abort via wasmer trap. The
block SHALL be rejected if a canonical call exhausts gas. The host tracks
`gas_used = GAS_LIMIT - remaining_points` (or `GAS_LIMIT + 1` if exhausted,
to signal the "exhausted" state).

Reference: `src/runtime/vm_runtime.rs:103` (GAS_LIMIT), `src/linear/src/execution.rs:78`
(BLOCK_GAS_LIMIT), `execution.rs:421-428` (gas accumulation check).

## 5. Error Propagation

### 5.1 Error Variants as Barbs

Per [type-system.md §4](type-system.md), every error variant IS a barb of the
system. ContractError variants SHALL be semantically distinct and SHALL trigger
different caller responses:

| Error Variant | Barb | Meaning | Caller Response |
|--------------|------|---------|-----------------|
| `InvalidFunction` | `↓bad-gate` | Unknown function selector | Reject call |
| `IoError` | `↓db-fail` | Sled read/write failure | Fatal — restart node |
| `Custom` | `↓bad-proof` | Application-level validation failure | Reject call |
| `InvalidWitness` | `↓bad-proof` | ZK witness does not satisfy circuit | Reject call |
| `ValueMismatch` | `↓bad-proof` | Value conservation violated | Reject call |
| `DuplicateNullifier` | `↓bad-nullifier` | Nullifier already in SMT | Reject call, ban peer |

### 5.2 ContractResult

`ContractResult` SHALL be `Result<(), ContractError>`. Every WASM entrypoint
SHALL return `ContractResult`. `Ok(())` means the state transition is valid.
`Err(ContractError::...)` means the call is rejected. There is no partial
success — a call either fully validates or fully fails.

### 5.3 The `let _` Prohibition

`let _ = fallible_call()` SHALL NOT appear in contract code. Per
[type-system.md §4.2.1](type-system.md), the `let _` pattern discards the
Result without inspecting it. Every Result SHALL be either:

- Propagated via `?` to the caller
- Matched explicitly (`match` / `if let Err(e)`) with a log at `warn!` or
  `error!` level
- Suppressed with `#[allow(unused_results)]` AND a comment explaining why
  the error is intentionally ignored

### 5.4 The `.ok()` Prohibition

`.ok()` SHALL NOT appear in contract code. Per
[type-system.md §4.2.2](type-system.md), `.ok()` converts `Result<T, E>` to
`Option<T>`, discarding the error reason. In contract code, the reason for
failure IS the signal — "nullifier already spent" vs "Merkle root not found"
vs "database corrupted" demand different responses.

### 5.5 Error Messages

Every error return SHALL be accompanied by a `msg!()` call identifying the
contract, function, and specific failure:

```rust
msg!("[native_token::pow_reward_v1] Error: Duplicate nullifier — coinbase already claimed");
return Err(NativeTokenError::DuplicateNullifier.into())
```

Error messages SHALL contain enough context for debugging: the contract name,
the function name, the specific check that failed, and the values involved.

Reference: `src/contract/native_token/src/entrypoint/mod.rs` (error patterns
throughout), `type-system.md` §4 (error types as barbs), §4.2 (error propagation
audit requirements).

### 5.6 No `.unwrap()` in Contract Entrypoint Code

`.unwrap()` SHALL NOT appear in contract entrypoint code. This includes metadata
functions, exec handlers, apply handlers, and state accessors. Every `Result`
SHALL be propagated via `?` to a context that returns `ContractResult` or
`Result<_, ContractError>`.

**Rationale — genesis contracts are templates.** Genesis contract code is copied
by every new contract developer. A `.unwrap()` that is locally safe (e.g.,
`Vec::write` is infallible on a `Vec<u8>` writer) becomes a production hazard
when copied into a context where it is NOT safe (e.g., a network writer, a
file descriptor, a fixed-size buffer). The copier trusts the genesis pattern
and does not question why `.unwrap()` was used. Type-enforced error propagation
protects downstream copiers from themselves — the compiler rejects misuse, not
the code reviewer.

**Exception — locally provable invariants.** `try_into().unwrap()` on a slice
whose length was validated on the immediately preceding line is permitted
(§9.1 I2). The validation is visible and the invariant is locally provable.
Similarly, `.coordinates().unwrap()` on a `CtOption` whose `.is_none()` was
checked on the preceding line is permitted. The rule targets *invisible*
infallibility — `.encode(&mut Vec<u8>).unwrap()` where `Vec::write` never fails
but the caller cannot see that from the call site.

**The correct pattern:**

```rust
// BEFORE (violation): hidden infallibility — Vec::write never fails
fn helper(params: &[u8]) -> Vec<u8> {
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();  // panics on non-Vec writer
    metadata
}

// AFTER (compliant): type-enforced error propagation
fn helper(params: &[u8]) -> Result<Vec<u8>, ContractError> {
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;  // compiler enforces error handling
    Ok(metadata)
}
```

**Design principle — write code that is safe when copied verbatim.** If a
pattern requires the copier to understand WHY it's safe before using it, the
pattern is wrong. Genesis contracts SHALL use only patterns that are safe
to copy blindly into any context. The `?` operator on `Result` is universally
safe — it either propagates the error or the compiler rejects the call site.
`.unwrap()` is never universally safe — its safety depends on context that
the copier may not understand.

### 5.7 Host Function Error Code Requirements

The host provides functions to contracts via the WASM import interface
(`src/runtime/import/`). Each host function returns an `i64` error code
through the `to_builtin!` macro. The contract maps this back to a
`ContractError` variant via `ContractError::from(ret)`.

`ContractError::Internal` SHALL NOT be used in host functions that have
more than one failure mode. It is a reserved code for genuinely unrecoverable
conditions (WASM memory faults, host environment crashes). Every recoverable
failure — missing data, corrupt data, handle out of bounds, deserialization
failure — SHALL have its own `ContractError` variant with a distinct error code.

**Rationale**: The hard path (contracts using Merkle inclusion proofs, per
[privacy.md](privacy.md)) carries an extra dimension of constraint. Host
functions like `merkle_add` and `sparse_merkle_insert_batch` have 15-20
distinct failure sites. If all return `ContractError::Internal`, every
failure is a multi-hour investigation. Distinct error codes make each failure
self-describing — the variant name identifies the failure site.

**Pattern**: New host functions SHALL follow the `ComponentOperationFailed`
naming convention. Existing functions with `INTERNAL_ERROR` returns SHALL be
audited and replaced with distinct variants. See [safety.md](../dev/contracts/safety.md)
for the host function error code checklist.

## 6. Witness Binding Types

### 6.1 The Witness Binding Gap

A zkas binary's witness section (`ZkBinary.witnesses: Vec<VarType>`) is an
ordered, typed, **unnamed** list. Witness names exist only in the optional
debug section (`ZkBinary.debug_info`) and SHALL NOT be load-bearing. The
manifest's `witness_map` (`[[circuits]]` entries) declares, in slot order,
the source of each witness.

**Currently there is no automated validation that `witness_map` length, order,
or types match `ZkBinary.witnesses`.** A manifest that declares 7 witness slots
for a circuit that has 9 produces a runtime binding failure in the generic
prover, not a deploy-time error.

This SHALL be a deploy-time validation: when the wallet parses a manifest
during deploy scan, it SHALL cross-check that every `[[circuits]]` entry's
`witness_map` matches the corresponding `ZkBinary.witnesses` in length and
compatible types.

### 6.2 Witness Binding Rule

The manifest's `witness_map` SHALL declare one entry per witness slot, in slot
order. Each entry names the source of the witness value. The closed vocabulary:

| Source | Meaning | Compatible `VarType`(s) |
|--------|---------|------------------------|
| `secret` | Capability spending key, resolved via AccountManager key coordinates | `Base` |
| `note:<field>` | Field from the decrypted AEAD note, matched against `note_schema` | `Base`, `Uint64`, `Uint32`, `Scalar` |
| `param:<field>` | Field from the action's `[[parameters]]` | per parameter type |
| `merkle_path` | Inclusion proof siblings from `capability_proofs` | `MerklePath` |
| `leaf_position` | Capability's leaf position in the Merkle tree | `Uint32` |
| `blind` | Fresh blind derived from `Seed` ([wallet.md §6.1](wallet.md)) | `Base`, `Scalar` |
| `tx_commitment` | Transaction binding commitment | `Base` |
| `tx_nonce` | Transaction nonce | `Base` |

### 6.3 Witness Type Checking

The generic prover (`src/sdk/src/prover.rs`) SHALL type-check every
`witness_map` entry against the slot's declared `VarType`. A mismatch SHALL be
a typed error barb (`↓bad-proof`), never a fallback. The mapping from
`VarType` to valid source types:

| `VarType` | Valid Source Types |
|-----------|-------------------|
| `Base` (0x10) | `secret`, `note:<u64/base>`, `param:<base>`, `tx_commitment`, `tx_nonce` |
| `Scalar` (0x12) | `blind`, `note:<scalar>`, `param:<scalar>` |
| `Uint32` (0x30) | `leaf_position`, `note:<u32>`, `param:<u32>` |
| `Uint64` (0x31) | `note:<u64>`, `param:<u64>` |
| `MerklePath` (0x20) | `merkle_path` |
| `EcPoint` (0x01) | `note:<point>`, `param:<point>` |
| `EcFixedPoint` (0x02) | (reserved for circuit constants) |
| `EcFixedPointShort` (0x03) | (reserved for circuit constants) |
| `EcFixedPointBase` (0x04) | (reserved for circuit constants) |

`Any` (0xff) SHALL NOT appear in a witness map — it is a circuit-level escape
hatch that disables type checking. A circuit that uses `Any` witnesses SHALL
document why static typing is insufficient.

### 6.4 Circuit Binary Delivery

Genesis contracts' zkas binaries are embedded at compile time via `include_bytes!`
in the `init` entrypoint. User-deployed contracts' binaries travel in the
`DeployV1` payload. The wallet extracts them during deploy scan by scanning the
WASM blob byte-by-byte for `ZkBinary` magic bytes `[0x0B, 0x01, 0xB1, 0x35]`
(`src/zkas/compiler.rs` `MAGIC_BYTES`). Extracted binaries are stored in the
wallet's `zkas_binaries` table keyed by `(ContractId, namespace, circuit_name)`.

### 6.5 VarType Vocabulary

The 15 witness variable types from `src/zkas/types.rs`:

| Code | Name | Description |
|------|------|-------------|
| 0x00 | `Dummy` | No witness — placeholder |
| 0x01 | `EcPoint` | Uncompressed elliptic curve point |
| 0x02 | `EcFixedPoint` | Fixed-base point (precomputed multiples) |
| 0x03 | `EcFixedPointShort` | Short fixed-base point |
| 0x04 | `EcFixedPointBase` | Base-point scalar multiplication |
| 0x05 | `EcNiPoint` | Non-identity point (validated) |
| 0x10 | `Base` | Pallas base field element (Fp) |
| 0x11 | `BaseArray` | Array of Pallas base field elements |
| 0x12 | `Scalar` | Pallas scalar field element (Fq) |
| 0x13 | `ScalarArray` | Array of Pallas scalar field elements |
| 0x20 | `MerklePath` | Merkle proof path (sibling hashes) |
| 0x21 | `SparseMerklePath` | Sparse Merkle proof path |
| 0x30 | `Uint32` | 32-bit unsigned integer |
| 0x31 | `Uint64` | 64-bit unsigned integer |
| 0xff | `Any` | Any type (disables type checking) |

Reference: `src/zkas/types.rs` (VarType enum), `src/zkas/decoder.rs:52`
(ZkBinary struct), `src/sdk/src/prover.rs` (witness binding), `wallet.md`
§6.4.1 (witness-binding rule for generic prover).

## 7. Cross-Contract Composition

### 7.1 Composition Rule

When contract A, executing function `f_A` with declared barbs `B_A`, invokes
contract B's function `f_B` with declared barbs `B_B`, the composite execution
exhibits barbs `B_A ∪ B_B`. The composite execution SHALL succeed only if all
barbs in the union are satisfied.

This follows from the composition algebra in [composition.md §1.2](composition.md):
`compose` is barb union over the primitive list. Cross-contract composition is
the same operation applied at the contract level — the composite state
transition's barb set is the union of the component barbs.

### 7.2 Inter-Contract Call Interface

A contract SHALL invoke another contract through the host. The host SHALL:

1. **Checkpoint** the calling contract's overlay state (`overlay.checkpoint()`)
2. **Invoke** the called contract: `metadata` → `exec` → `apply`
3. **Rollback** to checkpoint if the called contract returns `Err`
4. **Commit** the called contract's state writes if it returns `Ok`

The caller SHALL NOT directly access the callee's sled trees. The callee's
state writes are committed to the shared overlay on success and are visible to
subsequent calls within the same block (Layer 2 sequential visibility, §3.8).

### 7.3 Inter-Contract State Isolation

Contract A's sled trees SHALL NOT be directly accessible to Contract B. The
sled tree name includes the contract ID hash — `blake3(contract_id || tree_name)`
— so Contract A's `"coins"` tree and Contract B's `"coins"` tree are physically
separate sled trees. The host SHALL enforce this via `db_lookup(cid, tree_name)`,
which derives the tree handle from the caller's `ContractId`.

The exception is the **spend hook** mechanism: PromissoryNote's `BurnV1` can
emit a spend hook targeting another contract's `__spend_hook` entrypoint. The
target contract's WASM is loaded and its `__spend_hook` + `__update` are invoked
with the payload from the burn call. The target contract accesses its OWN state
through its own sled trees — it never accesses the caller's state. Currently
only Stablecoin receives spend hooks (to track burned CDP stablecoins).

### 7.4 Re-Entrancy

Contract A SHALL NOT invoke Contract A recursively with the same call context.
The host SHALL detect re-entrancy (same `ContractId` appearing in the call
stack for the same transaction) and reject it with `ContractError::InvalidFunction`.

This is a consensus requirement: re-entrancy in a ZK execution model would
create circular dependencies in the state transition proof, making it impossible
to construct a valid witness.

## 8. Wallet-Contract Interface Contract

### 8.1 The Two Scan Paths

The wallet discovers capabilities through two scan paths
([wallet.md §2](wallet.md)):

- **Path 1 (Native Token):** Bespoke. Hardcoded scan for coinbase rewards,
  transfers, burns, fee payments. Uses compiled-in Rust types from the
  native_token contract crate.
- **Path 2 (Manifest-Driven):** Generic. Trial AEAD decryption → manifest
  resolution → barb composition → `wallet_construct` → `TypedCapability`.
  Works for every contract with a manifest. Zero per-contract code.

### 8.2 Note Format Contract

A contract that produces capabilities SHALL encrypt the capability's primitive
names in an `AeadEncryptedNote`. The note SHALL be placed in the contract's
return data (via `set_return_data`). The note's plaintext SHALL contain fields
matching the manifest's `note_schema`.

The wallet discovers the note during scan by:
1. Deserializing call data from `ContractCall.data`
2. Scanning for `AeadEncryptedNote` structures in the deserialized params
3. Attempting AEAD decryption with each wallet secret
4. On success: decoding the plaintext via `decode_note_by_schema()` against the
   manifest's `note_schema`

A capability whose note is not AEAD-encrypted is invisible to the wallet — it
cannot discover capabilities it cannot decrypt. A note whose plaintext does not
match the declared `note_schema` SHALL cause the wallet to drop the capability
(unknown format → cannot construct type).

### 8.3 Manifest Truthfulness

A contract's manifest SHALL truthfully declare:

1. **Every function the contract exports.** A function in the WASM but not in the
   manifest is a hidden function — the wallet cannot discover it. A function in the
   manifest but not in the WASM is a lie.
2. **Every capability the contract recognizes.** A capability the contract creates
   but doesn't declare cannot be typed by the wallet. A capability declared but
   never created is clutter.
3. **Every barb each action requires.** A barb that is declared but not enforced is
   a security lie — the coverage gate passes but the contract doesn't actually
   check. A barb that is enforced but not declared is a hidden constraint — the
   wallet's coverage gate rejects capabilities that would actually work.
4. **Every state tree the contract writes to.** A tree used but not declared is a
   hidden side effect — it consumes disk but the manifest gives no hint. A tree
   declared but not used is clutter.
5. **Every ZK circuit the contract's proofs use.** A function declared with
   `requires_proof = true` but no matching `[[circuits]]` entry cannot be
   proven by the generic prover.

The wallet's WASM verification layer ([manifest.md](manifest.md) Trust Model,
Layer 2) SHALL mechanically verify that manifest function declarations match
WASM exports. Circuit cross-checking against zkas binary witness counts is a
planned extension (§6.1).

### 8.4 Scan Determinism

The wallet's scan SHALL be deterministic given the same chain state and the same
`AccountManager` ([wallet.md §1](wallet.md)). Contracts SHALL NOT introduce
non-determinism through:

- **Timestamps:** `get_blockchain_time()` returns the block timestamp from the
  header. It SHALL be used only for time-dependent logic (e.g., auction expiry).
  It SHALL NOT affect capability construction.
- **Random values:** WASM has no access to entropy sources. All randomness SHALL
  come from the transaction's explicit `Seed` — never from ambient sources.
- **External calls:** The WASM has no network access. All data SHALL come from
  the call data, the block header, or the sled state.

### 8.5 The Coverage Gate

Per [wallet.md §2.2](wallet.md) and [composition.md §1.3](composition.md), the
wallet's Path 2 applies the **coverage gate**: `wallet_construct` checks whether
the composed barbs of the declared primitives cover the action's `required_barbs`.
If coverage fails, the capability is dropped — it is not a valid type.

The contract SHALL ensure that every function that declares `required_barbs`
enforces those barbs in its `exec` entrypoint. A function whose required barbs
are not enforced in WASM SHALL NOT declare them in the manifest. The coverage
gate is only as strong as the contract's enforcement — a gate that passes
because the manifest made false claims is a security vulnerability.

Reference: `wallet.md` §2.2, `composition.md` §4, `capability.rs:342-368`
(wallet_construct).

## 9. Compiler-Enforced and Mechanically Verifiable Invariants

### 9.1 Compiler-Enforced (Rust Type System)

**I1 — Function dispatch exhaustiveness.** The `match` on function selector SHALL
cover all enum variants. `#[deny(unreachable_patterns)]` SHALL be on the match
statement in `process_instruction` and `process_update`.

**I2 — ContractResult propagation.** Every fallible operation SHALL use `?`.
`unwrap()` SHALL only appear where the invariant is locally provable (e.g.,
`PublicKey::from_secret` on a known secret, `xy()` on a known non-identity point).

**I3 — No `unwrap_or` on consensus state.** The compiler SHALL reject
`unwrap_or(0)` on state that crosses the sled boundary — by using newtypes
lacking `From<u64>` and `Default`, the same technique as `BlockHeight`
([type-system.md §2.3](type-system.md)).

**I4 — Named parameter structs.** Every function parameter SHALL be a named
struct with inherent `encode()`/`decode()` methods AND thin bridge impls for
`dwow_serial::Encodable`/`Decodable` that delegate to the inherent methods
(see §3.1.3). `#[derive(SerialEncodable, SerialDecodable)]` SHALL NOT be used
— it bypasses validating constructors (§3.1.1). Raw `Vec<u8>` SHALL NOT be
passed as parameters without deserialization to a named type.

### 9.2 Mechanically Verifiable (Manifest-WASM Cross-Check)

**I5 — Function export match.** Every function declared in the manifest's
`[[functions]]` SHALL correspond to a branch in the contract's `process_instruction`
dispatch. Verification: parse the WASM binary, extract exports, compare against
manifest function names. This is Layer 2 WASM verification
([manifest.md](manifest.md) Trust Model).

**I6 — State tree declaration.** Every sled tree name constant used in the
contract (`NATIVE_TOKEN_CONTRACT_COINS_TREE`, etc.) SHALL be declared in the
manifest's `[[trees]]` section. Verification: grep for `_TREE` / `_TREES`
constants in contract source, compare against manifest tree names.

**I7 — Circuit declaration.** Every ZK circuit namespace constant used in the
contract (`NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V1`, etc.) SHALL be declared in
the manifest's `[[circuits]]` section. Verification: grep for `_ZKAS_` /
`_NS_V1` constants, compare against manifest circuit names.

### 9.3 Human-Verifiable (Code Review)

**I8 — No `let _ =` on Result.** `let _ = any_fallible_call()` SHALL NOT appear
in contract entrypoint code. Verification: `grep -r "let _ =" src/contract/*/src/entrypoint/`.

**I9 — No `.ok()` in contract code.** `.ok()` SHALL NOT appear in contract
entrypoint code. Verification: `grep -r "\.ok()" src/contract/*/src/entrypoint/`.

**I10 — Errors carry context.** Every `Err(...)` return in contract entrypoint
code SHALL be preceded (within 5 lines) by a `msg!()` call identifying the
contract, function, and specific failure. Verification: manual review.

**I11 — No `unwrap_or` on sled reads.** `db_get(...)?.map(|d| deserialize(&d).unwrap_or(...)).unwrap_or(...)`
SHALL NOT appear. Every sled read SHALL use explicit `match` with error
propagation (§3.4). Verification: `grep -r "unwrap_or" src/contract/*/src/entrypoint/`.

## 10. Non-Unifiable Types in Contract Code

These pairs SHALL NOT be unified in contract code. The compiler SHALL reject
any attempt to use the left type where the right type is expected.

| Type | SHALL NOT be treated as | Reason |
|------|------------------------|--------|
| `BlockHeight` | `u64` | Nominal consensus scalar — height ≠ amount ≠ supply ([type-system.md §2.3](type-system.md)) |
| `BlockReward` | `u64` | Nominal consensus scalar — reward ≠ height |
| `BlockTarget` | `u32` | Nominal consensus scalar — target ≠ difficulty |
| `ContractId` | `[u8; 32]` | `↓dispatch` ≠ no barbs ([type-system.md §8.4](type-system.md)) |
| `Nullifier` | `pallas::Base` | `↓nullify` ≠ no barbs |
| `PublicKey` | `pallas::Point` | One validates identity, one does not |
| `AssetId` | `pallas::Base` | `↓denominate` ≠ no barbs |
| `FuncId` | `pallas::Base` | `↓gate` ≠ no barbs |
| `MerkleNode` | `pallas::Base` | `↓prove-inclusion` ≠ no barbs |
| `Coin` (newtype over `pallas::Base`) | `pallas::Base` | `↓commit` ≠ no barbs |
| `Serialized value` | `Raw bytes` | A `db_get` value SHALL be deserialized to a known type, never passed as `Vec<u8>` |
| `DbHandle` (opaque i32) | `i32` / `u32` | The handle is an opaque resource; arithmetic on handles is meaningless |

## 11. Contract Type Patterns (Normative)

This section defines the canonical patterns that SHALL appear in every contract.
These are not suggestions — they are the specification. Deviation SHALL be
justified in the contract's documentation.

### 11.1 Function Enum

```rust
/// Function selectors for Example contract.
#[repr(u8)]
enum ExampleFunction {
    InitializeV1 = 0x00,
    TransferV1 = 0x01,
    BurnV1 = 0x02,
}

impl TryFrom<u8> for ExampleFunction {
    type Error = ContractError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::TransferV1),
            0x02 => Ok(Self::BurnV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}
```

### 11.2 Entrypoint Registration

```rust
dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);
```

### 11.3 Exec Dispatch

```rust
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    if ix.is_empty() {
        msg!("[example::process_instruction] Error: Empty call data");
        return Err(ContractError::IoError("Empty call data".to_string()));
    }
    let func = ExampleFunction::try_from(ix[0])?;
    let params = &ix[1..];

    match func {
        ExampleFunction::InitializeV1 => initialize_v1(cid, params),
        ExampleFunction::TransferV1 => transfer_v1(cid, params),
        ExampleFunction::BurnV1 => burn_v1(cid, params),
    }
}
```

### 11.4 Apply Dispatch

The apply entrypoint uses custom `decode_*_update_v1()` functions to deserialize
the update from the bridge (Boundary 2 of §3.1). These SHALL delegate to the
struct's inherent `decode()` method.

```rust
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    if update_data.is_empty() {
        msg!("[example::process_update] Error: Empty update data");
        return Err(ContractError::IoError("Empty update data".to_string()));
    }
    let func = ExampleFunction::try_from(update_data[0])?;

    match func {
        ExampleFunction::TransferV1 => {
            let update = decode_transfer_update_v1(&update_data[1..])?;
            apply_transfer(cid, update)
        }
        // ...
    }
}
```

The `decode_transfer_update_v1` helper is a thin wrapper:
```rust
fn decode_transfer_update_v1(data: &[u8]) -> Result<TransferUpdateV1, ContractError> {
    TransferUpdateV1::decode(data)
}
```

This pattern isolates the bridge encoding from the struct's inherent method,
making it straightforward to change either independently. See native_token
(`src/contract/native_token/src/entrypoint/mod.rs`), purse, box, and deployooor
for reference implementations.

### 11.5 Metadata Dispatch

Metadata helper functions SHALL return `Result<Vec<u8>, ContractError>` and use
`?` propagation — never `.unwrap()`. The main `get_metadata` dispatch SHALL
propagate errors from helpers via `?`. This is the pattern used by the canonical
genesis contracts: Purse, Box, Identity, Oracle, Attestation, and Deployooor.

```rust
/// Per-function metadata helper — returns encoded ZK public inputs + signatures.
fn transfer_metadata(_cid: ContractId, params: &[u8]) -> Result<Vec<u8>, ContractError> {
    let params = TransferParamsV1::decode(params).map_err(|_| {
        // Empty metadata on decode failure — host will reject (no proofs to verify).
        return ContractError::IoError("transfer_metadata: decode failed".into());
    })?;

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![]; // Schnorr signatures prohibited

    // ... populate zk_public_inputs from params ...

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

/// Main metadata dispatch — host entrypoint.
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let func = match ExampleFunction::try_from(ix[0]) {
        Ok(f) => f,
        Err(_) => {
            wasm::util::set_return_data(&vec![]);
            return Ok(()); // unknown function → empty metadata → host rejection
        }
    };
    let params = &ix[1..];

    let metadata = match func {
        ExampleFunction::TransferV1 => transfer_metadata(cid, params),
        // ...
    }?; // ? propagates errors from helpers

    wasm::util::set_return_data(&metadata)
}
```

Key rules:
- Helper functions return `Result<Vec<u8>, ContractError>`, never bare `Vec<u8>`
- `.encode(&mut metadata)?` propagates encoding errors — no `.unwrap()`
- The caller uses `?` on the match expression to propagate helper errors
- Empty metadata on decode failure SHALL use explicit `return Ok(vec![])` or a
  typed error — not a silent `.unwrap()` that panics when copied to a non-Vec writer
- This pattern makes it IMPOSSIBLE to write `.unwrap()` by accident — the compiler
  requires `?` or explicit handling because the return type is `Result`

### 11.6 State Accessor

State values crossing the sled boundary SHALL use inherent `encode()`/`decode()`
methods (Boundary 3 of §3.1), never `dwow_serial::serialize()`/`deserialize()`.
Keys SHALL use typed `to_bytes()`/`to_le_bytes()` methods per §3.2.

```rust
/// Tree name constant — SHALL be declared in manifest's `[[trees]]`.
const EXAMPLE_TREE: &str = "example";

/// Key — SHALL use typed to_le_bytes per §3.2.
const TOTAL_SUPPLY_KEY: u64 = 0;

fn read_u64_state(cid: ContractId, tree: &str, key: &[u8]) -> Result<u64, ContractError> {
    let db = wasm::db::db_lookup(cid, tree)?;
    match wasm::db::db_get(db, key)? {
        Some(data) => {
            let bytes: [u8; 8] = data.try_into().map_err(|_| {
                ContractError::IoError("Corrupt state: wrong size".to_string())
            })?;
            Ok(u64::from_le_bytes(bytes))
        }
        None => {
            msg!("[example] Error: State key {:?} not found", key);
            Err(ContractError::IoError("Missing state key".to_string()))
        }
    }
}
```

### 11.7 Function Handler (exec)

```rust
fn transfer_v1(cid: ContractId, params: &[u8]) -> ContractResult {
    // 1. Deserialize and validate parameters via inherent decode() (Boundary 1)
    let pr = TransferParamsV1::decode(params)?;

    // 2. Enforce declared barbs
    // ↓denominate: verify token commitment
    if pr.input.token_commit != expected_token_commit {
        msg!("[example::transfer_v1] Error: Token commitment mismatch");
        return Err(ExampleError::TokenMismatch.into());
    }

    // ↓nullify: check nullifier SMT
    let nullifiers_db = wasm::db::db_lookup(cid, NULLIFIERS_TREE)?;
    let smt = /* build SMT from nullifiers_db */;
    if smt.get_leaf(&pr.input.nullifier.inner()) != pallas::Base::zero() {
        msg!("[example::transfer_v1] Error: Duplicate nullifier");
        return Err(ExampleError::DuplicateNullifier.into());
    }

    // ↓prove-inclusion: verify Merkle root (use typed to_bytes, not serialize)
    let roots_db = wasm::db::db_lookup(cid, ROOTS_TREE)?;
    if !wasm::db::db_contains_key(roots_db, &pr.input.merkle_root.to_bytes())? {
        msg!("[example::transfer_v1] Error: Merkle root not found");
        return Err(ExampleError::MerkleRootNotFound.into());
    }

    // 3. Produce state update
    let update = TransferUpdateV1 {
        nullifier: pr.input.nullifier,
        coin: pr.output.coin,
    };

    // 4. Return update via custom bridge function (Boundary 2)
    wasm::util::set_return_data(&encode_transfer_update_v1(&update))?;
    Ok(())
}
```

### 11.8 Function Handler (apply)

The apply handler receives a fully-decoded update struct from the bridge
(Boundary 2 of §3.1). Host function arguments (`merkle_add`, `sparse_merkle_insert_batch`)
use `dwow_serial::serialize()` — this is the host FFI boundary (Boundary 4),
distinct from the three contract boundaries. The types being serialized are
`DbHandle` + primitive values, not contract-defined structs.

```rust
fn apply_transfer(cid: ContractId, update: TransferUpdateV1) -> ContractResult {
    // 1. Write nullifier to SMT
    let nullifiers_db = wasm::db::db_lookup(cid, NULLIFIERS_TREE)?;
    wasm::merkle::sparse_merkle_insert_batch(
        &serialize(&(nullifiers_db, vec![update.nullifier.inner()]))
    )?;

    // 2. Write coin to Merkle tree
    let coins_db = wasm::db::db_lookup(cid, COINS_TREE)?;
    wasm::merkle::merkle_add(&serialize(&(coins_db, update.coin.inner())))?;

    Ok(())
}
```

## 12. References

- [Type System Specification](type-system.md) — Primitive types, barbs, nominal newtypes, invariants.
- [Capability Composition](composition.md) — Barb union, composition algebra, wallet as composition engine.
- [Wallet Architecture](wallet.md) — Scan paths, capability construction, pure function model.
- [Contract Manifest](manifest.md) — Interface declarations, TOML format, lifecycle stages.
- [O-Cap: Emergent Types](ocap.md) — Capability lifecycle grammar, ZK instantiation.
- [Consensus Specification](consensus/consensus.md) — Phase 4 (WASM execution), execution ordering, atomicity.
- [Mempool Specification](mempool.md) — Verified transaction admission.
- Meredith, L.G. and Radestock, M. (2005). "A Reflective Higher-Order Calculus." *ENTCS*.
- Miller, M.S. (2006). *Robust Composition.* PhD dissertation, Johns Hopkins University.
