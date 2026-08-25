# Contract WASM Standards & Best Practices

This document codifies the standards and best practices learned from the
systematic migration of all 32 contracts from derive-based `SerialEncodable`/
`SerialDecodable` to explicit `encode()`/`decode()` methods. It SHALL be read
in conjunction with the [Contract WASM Type System](contract-wasm-type-system.md)
and [Type System](type-system.md) specifications.

This specification uses SHALL, MUST, SHALL NOT, MUST NOT per RFC 2119.

## 0. The Exemplar Principle

**Genesis contracts are templates.** Every new contract developer copies from
existing code. A pattern that is locally safe but unsafe when copied blindly
is a production hazard — someone will copy it into a context where it fails
and say "I was just following the genesis contract pattern."

Every pattern in a genesis contract SHALL be safe to copy verbatim into any
context. The compiler SHALL be the enforcer of safety, not the code reviewer's
knowledge of which call sites are infallible.

## 1. Explicit Encoding — No Derive Macros for State Values

### 1.1 The Rule

`SerialEncodable`/`SerialDecodable` derive macros SHALL NOT be applied to
types stored in sled trees or crossing the WASM boundary. Derive macros
bypass validating constructors: `Nullifier`'s derived `Decodable` reads
`pallas::Base` directly — it never calls `Nullifier::from_bytes()` and
silently accepts zero and non-canonical values.

Every type SHALL have explicit `encode()` and `decode()` methods with a
fixed canonical byte layout. Each cryptographic field SHALL route through
its named constructor (`from_bytes`, `from_repr`).

### 1.2 The Pattern

```rust
// AFTER (compliant) — see src/contract/box/src/model/mod.rs
pub struct BoxRecord {
    pub version: u8,
    pub box_id: BoxId,
    pub contents_commit: pallas::Base,
    pub is_empty: bool,
}

impl BoxRecord {
    pub const ENCODED_SIZE: usize = 66;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.box_id.to_bytes());       // ← validating
        buf.extend_from_slice(&self.contents_commit.to_repr()); // ← validating
        buf.push(self.is_empty as u8);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "BoxRecord: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let box_id = BoxId::from_bytes(data[1..33].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("BoxRecord: invalid box_id".into()))?;
        let contents_commit = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[33..65].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("BoxRecord: invalid contents_commit".into()))?;
        // ...
    }
}
```

### 1.3 Rules

- Fixed byte layout with exact offsets. No variable-length codec.
- Every cryptographic type through its validating constructor (`from_bytes`, `from_repr`).
- `Vec<T>` fields SHALL use a fixed-width length prefix (u8 for counts < 256).
- No `unwrap_or` — every validation failure is a `ContractError` with field context.
- Pre-allocate exact capacity in encode: `Vec::with_capacity(FIXED_SIZE)`.
- Use `extend_from_slice` in encode, never the old `encode(&mut writer)` pattern.
- `decode()` takes `&[u8]`, never `&mut Cursor` or `&mut Read`.

### 1.4 Reference Implementations

| Contract | File | Notes |
|----------|------|-------|
| Box | `src/contract/box/src/model/mod.rs` | `BoxRecord` — 66-byte fixed, validating constructors |
| Purse | `src/contract/purse/src/model/mod.rs` | `Purse` — 129-byte fixed, Pedersen commitments |
| Identity | `src/contract/identity/src/model/mod.rs` | Multi-type example with per-field validation |
| Deployooor | `src/contract/deployooor/src/model/mod.rs` | Clean, simple params encode/decode |

## 2. No `.unwrap()` in Entrypoint Code

### 2.1 The Rule

`.unwrap()` SHALL NOT appear in contract entrypoint code. This includes
metadata functions, exec handlers, apply handlers, and state accessors.

Every `Result` SHALL be propagated via `?` to a context that returns
`ContractResult` or `Result<_, ContractError>`.

**Rationale:** `.encode(&mut Vec<u8>).unwrap()` is technically safe because
`Vec::write` never fails. But the copier does not know this — they see
`.unwrap()` and may use the same pattern on a network writer, a file
descriptor, or a fixed-size buffer where it panics in production. The `?`
operator is universally safe — the compiler enforces error handling.

### 2.2 The Correct Pattern

Metadata helper functions SHALL return `Result<Vec<u8>, ContractError>` and
use `?` propagation:

```rust
// COMPLIANT — see src/contract/purse/src/entrypoint/mod.rs:88-103
fn purse_deposit_get_metadata_v1(
    params: DepositParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // ... populate zk_inputs from params ...
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;   // ← ? not .unwrap()
    let sigs: Vec<pallas::Base> = vec![];
    sigs.encode(&mut metadata)?;        // ← ? not .unwrap()
    Ok(metadata)
}
```

The main `get_metadata` dispatcher SHALL propagate with `?` on the match:

```rust
// COMPLIANT — see src/contract/purse/src/entrypoint/mod.rs or
// src/contract/box/src/entrypoint/mod.rs
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    // ...
    let metadata = match func {
        PurseFunction::DepositV1 => purse_deposit_get_metadata_v1(params),
        PurseFunction::WithdrawV1 => purse_withdraw_get_metadata_v1(params),
        _ => Ok(vec![]),
    }?;  // ← ? propagates errors from all arms
    wasm::util::set_return_data(&metadata)
}
```

### 2.3 The Exception — Locally Provable Invariants

`try_into().unwrap()` on a slice whose length was validated on the
immediately preceding line is permitted. The validation is visible and
the invariant is locally provable. The rule targets *invisible* infallibility
— `.encode(&mut Vec<u8>).unwrap()` where the caller cannot see from the
call site that the operation cannot fail.

### 2.4 Before/After Reference

The `promissory_note` and `native_token` genesis contracts were fixed to
match this standard. See:

- `src/contract/promissory_note/src/entrypoint/mod.rs` — 6 helpers changed
  from `-> Vec<u8>` to `-> Result<Vec<u8>, ContractError>`, 12 `.unwrap()`
  replaced with `?`.
- `src/contract/native_token/src/entrypoint/mod.rs` — 6 helpers changed,
  13 `.unwrap()` replaced with `?`.

The majority pattern (6 of 9 genesis contracts) already used `?`. The fix
brought the remaining 2 into conformance.

## 3. Type Safety — Newtypes for Primitive IDs

### 3.1 The Rule

Primitive ID types (`BoxId`, `SubscriptionId`, `ClaimId`, `DaoEscrowBulla`,
etc.) and sled-only state types (Commitment, Nullifier, Purse, etc.) SHALL have
standalone `encode()`/`decode()` methods. These types SHALL NOT have
`dwow_serial::Encodable`/`Decodable` bridge impls — they never cross the
exec→apply boundary (see contract-wasm-type-system.md §3.1 Boundary 3).

`*ParamsV1` and `*UpdateV1` structs SHALL retain thin bridge impls delegating
to their inherent methods per contract-wasm-type-system.md §3.1.3 — these are
required by test code, client helpers, and the exec→apply boundary.

### 3.2 The Pattern

```rust
// COMPLIANT — see src/contract/box/src/model/mod.rs
impl BoxId {
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(BoxId)
    }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "BoxId: expected 32 bytes, got {}", data.len()
            )));
        }
        Self::from_bytes(data.try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("BoxId: invalid field element".into()))
    }
}
```

## 4. Entrypoint Architecture

### 4.1 The Four Entrypoints

Every contract SHALL use `dwow_sdk::define_contract!` with four handlers:

| Entrypoint | Macro Field | Returns | What it does |
|-----------|------------|---------|-------------|
| `init` | `init:` | `ContractResult` | Set up sled trees, seed initial state |
| `exec` | `exec:` | `ContractResult` | Validate state transition, set return data |
| `apply` | `apply:` | `ContractResult` | Write state updates, SHALL NOT read |
| `metadata` | `metadata:` | `ContractResult` | Return ZK public inputs, use `set_return_data` |

### 4.2 Function Dispatch

```rust
// COMPLIANT — see src/contract/purse/src/entrypoint.rs
#[repr(u8)]
enum PurseFunction {
    DepositV1 = 0x00,
    WithdrawV1 = 0x01,
    BalanceV1 = 0x02,
}

impl TryFrom<u8> for PurseFunction {
    type Error = ContractError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::DepositV1),
            0x01 => Ok(Self::WithdrawV1),
            0x02 => Ok(Self::BalanceV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}
```

### 4.3 Return Data Convention

The `exec` entrypoint SHALL call `wasm::util::set_return_data(&encoded_update)`.
The `apply` entrypoint SHALL dispatch on `update_data[0]` to route to the
correct handler.

SHALL NOT use `let _ = wasm::util::set_return_data(...)` — errors must be
propagated. SHALL use `wasm::util::set_return_data(...)?` or handle the
error explicitly.

Reference fix: `src/contract/otc_swap/src/entrypoint.rs` — 5 `let _ =`
violations replaced with `?` propagation.

## 5. `let _ =` and Error Suppression

### 5.1 The Rule

`let _ = fallible_call()` SHALL NOT appear in contract entrypoint code.
Every `Result` SHALL be:
- Propagated via `?` to the caller, or
- Matched explicitly (`match` / `if let Err(e)`) with a log at `warn!` or
  `error!` level.

### 5.2 Detected Violations

Found and fixed during the migration:

| Contract | File | Lines | Fix |
|----------|------|-------|-----|
| otc_swap | `entrypoint.rs:311-330` | 5 `let _ =` on `set_return_data` | Changed to `?` |

Pattern: `let _ = wasm::util::set_return_data(&update);` →
`wasm::util::set_return_data(&update)?;`

## 6. Client-Side Code

### 6.1 Client Builders

Client-side builder structs (`src/contract/<name>/src/client/mod.rs`) SHALL
NOT derive `SerialEncodable`/`SerialDecodable`. These types are used for
call construction and do not cross the sled boundary. Remove the derives
and the `use dwow_serial::...` import.

Fixed in: `bearer_bond`, `dao_escrow`, `drain_protection`, `native_token`,
`promissory_note`.

## 7. Signature Encoding

### 7.1 The Rule

The `Signature` type in `src/sdk/src/crypto/schnorr.rs` SHALL have explicit
`encode()`/`decode()` methods. Contract model code that uses `dwow_serial::serialize`
to encode a `Signature` field SHALL use `self.signature.encode()` instead.

### 7.2 The Pattern

```rust
// COMPLIANT — see src/sdk/src/crypto/schnorr.rs
impl Signature {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(&self.commit.to_bytes());
        buf.extend_from_slice(&self.response.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() != 64 { return None; }
        let commit = pallas::Point::from_bytes(
            data[0..32].try_into().ok()?
        ).into_option()?;
        let response = pallas::Scalar::from_repr(
            *<&[u8; 32]>::try_from(&data[32..64]).ok()?
        ).into_option()?;
        Some(Self { commit, response })
    }
}
```

Fixed in: `betting_stake` (model `InitializeParamsV1::encode/decode`),
`darkbet_exchange` (3 params structs).

## 8. Anti-Patterns Discovered

### 8.1 `unwrap_or` on Sled Reads

Every sled read SHALL use explicit `match` with error propagation, never
`unwrap_or(0)` or `unwrap_or_else(|| ...)`:

```rust
// BEFORE (anti-pattern):
let house_edge = wasm::db::db_get(info_db, HOUSE_EDGE_KEY)?
    .unwrap_or_else(|| DEFAULT_HOUSE_EDGE.to_le_bytes().to_vec());

// AFTER (compliant):
let stored_house_edge = match wasm::db::db_get(info_db, HOUSE_EDGE_KEY)? {
    Some(bytes) => u32::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
        ContractError::IoError("Corrupt state: house_edge wrong size".into())
    })?),
    None => DEFAULT_HOUSE_EDGE,
};
```

Reference fix: `src/contract/baccarat/src/entrypoint/commit_bet_v1.rs:156-163`.

### 8.2 Dead Imports

`use dwow_serial::{SerialDecodable, SerialEncodable};` imports that remain
after all derives have been removed SHALL be deleted.

Found and fixed in: `darkbet_exchange` model, `test-harness` harness/box.rs.

## 9. Nullifier Storage Standard

This is the single repo convention for how a contract records a spent nullifier.
It is a **standard** (a convention every contract follows), not a hard
SHALL/SHALL NOT specification — the `Nullifier` type and its `↓nullify` barb
are specified in [type-system.md](type-system.md) and
[contract-wasm-type-system.md](contract-wasm-type-system.md); this section records
the concrete storage pattern. It applies to every contract that tracks nullifier
replay, with no exceptions.

### 9.1 The Convention

- **Write:** `db_mark_spent(db, &nullifier.to_bytes())` — writes the non-empty
  marker `&[1]`. The helper is defined in `src/sdk/src/wasm/db.rs`.
- **Read:** `db_contains_key(db, &nullifier.to_bytes())`.

Never write `db_set(db, &nullifier.to_bytes(), &[])`. An empty value is invisible
to `db_contains_key` and `db_get` (the sled backend treats empty values as
"key absent"), so an empty marker silently bypasses replay protection.

### 9.2 No Sparse Merkle Tree for Nullifiers

Nullifier replay tracking does NOT use the Sparse Merkle Tree (SMT). The SMT
was removed from nullifier handling because it was vestigial: nullifiers are
already public (emitted in the block nullifier set), and Merkle-proofability
adds no privacy or replay benefit. The SMT is used only where a genuine Merkle
inclusion proof is required (commitment trees), never for the boolean
"spent / not-spent" marker.

### 9.3 Two Layers, One Authoritative

The consensus layer (`chain_state.spent_nullifiers` BTreeSet +
`chain_state.nullifier_set` BTreeMap with height tracking, in
`src/linear/src/chain_state.rs`) is the authoritative replay set, enforced at
block validation. The per-contract `db_mark_spent` marker is consistent
defense-in-depth — it is not the primary gate, but it must be present and
correct in every contract.

### 9.4 The Law Behind the Standard

This standard is the corollary of a single invariant — the **Representation
Faithfulness Law** ([type-system.md §0.1](type-system.md)): a barb is faithfully
encoded iff its witness is a distinguished (non-degenerate) element. The `&[1]`
marker is the distinguished witness of `↓nullify`; the empty value `&[]` is the
canonical "absent" witness and cannot mark. §9.1's "never `db_set(..., &[])`"
and §C.3.5's zero-nullifier rejection are the two instances of this law.

Mechanized in `proofs/lean/src/DarkFi/Combinatorial/NullifierStorage.lean`:
`markSpent_faithful`, `markEmpty_not_spent`, `markEmpty_never_adds`,
`markSpent_sound`, `markSpent_monotone`, `markSpent_idempotent`,
`faithful_iff_nonempty`.

## 10. Migration Checklist

When bringing a contract up to this standard, verify:

- [ ] Zero `SerialEncodable`/`SerialDecodable` derives in `model/mod.rs`
- [ ] Zero `SerialEncodable`/`SerialDecodable` derives in `entrypoint/`
- [ ] Zero `SerialEncodable`/`SerialDecodable` derives in `client/mod.rs`
- [ ] Every params struct has `pub fn encode(&self) -> Vec<u8>`
- [ ] Every params struct has `pub fn decode(data: &[u8]) -> Result<Self, ContractError>`
- [ ] Every state struct has `pub fn encode(&self) -> Vec<u8>`
- [ ] Every state struct has `pub fn decode(data: &[u8]) -> Result<Self, ContractError>`
- [ ] Every primitive ID type has `encode()` and `decode()`
- [ ] Every `*ParamsV1` and `*UpdateV1` struct has thin Encodable+Decodable bridge
      impls delegating to inherent encode()/decode() per contract-wasm-type-system.md §3.1.3
- [ ] No bridge impls on sled-only state types (Commitment, Nullifier, Purse, BoxId, etc.)
- [ ] Zero `.unwrap()` in entrypoint metadata functions
- [ ] All metadata helpers return `Result<Vec<u8>, ContractError>`
- [ ] No `let _ =` in entrypoint code
- [ ] No `unwrap_or` on sled reads in entrypoint
- [ ] Nullifiers written via `db_mark_spent`, never `db_set(..., &[])` (§9)
- [ ] Nullifiers read via `db_contains_key` (§9)
- [ ] Entrypoint uses `set_return_data(...)?` not `let _ = set_return_data(...)`
- [ ] Encode methods use `extend_from_slice`, never `encode(&mut writer)`
- [ ] Decode methods take `&[u8]`, never `&mut Cursor`
- [ ] `cargo check -p <crate>` passes
- [ ] `cargo test -p <crate> --lib` passes

## 11. References

- [Contract WASM Type System](contract-wasm-type-system.md) — Foundational spec, §3.1 (explicit encoding), §11 (canonical patterns)
- [Type System](type-system.md) — Primitive types, barbs, nominal newtypes, non-unifiable pairs
- [Genesis](genesis.md) — Genesis contract list, ContractId derivation
- [Safety](../dev/contracts/safety.md) — Security lessons and flakey patterns
- [Wallet Architecture](wallet.md) — Scan paths, capability construction, pure function model
