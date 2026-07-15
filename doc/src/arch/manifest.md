# Contract Manifest

## Implementation Status

| Layer | Status | Location |
|-------|--------|----------|
| Specification | Complete | This document |
| Python model | Complete (13 tests) | `contrib/model/wallet_model.py` |
| Rust data types | Complete (6/6 unit tests) | `src/sdk/src/manifest.rs` |
| TOML manifests | Complete (32 contracts) | `src/contract/*/manifest.toml` |
| Wallet resolver | Complete | `bin/dww/src/manifest_resolver.rs` |
| CLI `contract show` | Complete | `bin/dww/src/dispatch.rs` |
| Deploy `--manifest` flag | Complete | `bin/dww/src/dispatch.rs` (build_deploy_ix) |
| Scan-side integration | Complete | `bin/dww/src/rpc.rs` (DeployV1 handler) |
| CapabilityResolver fallback | Complete | `bin/dww/src/capability.rs` (resolve()) |
| On-chain manifest hash | Pending | Deployooor hardening |

32 contracts have manifests: 9 genesis (see [Genesis Contracts](genesis.md)), plus
23 user-deployed contracts. All 32 round-trip through the Python parser.

The full data pipeline is wired: authoring → deployment → scanning → storage →
resolution → query. This document describes the complete system as implemented.

## Rationale

Every DarkWow contract exports functions, references ZK circuits, writes to named
sled trees, and defines capability types. All of this information is **already
public** — it's embedded in the WASM binary, visible in proof circuit names, and
observable in sled keys. The manifest makes it readable without reverse-engineering.

Without a standard manifest, the ecosystem fragments: each wallet must decompile
each contract's WASM to discover its interface. Sophisticated actors with
reverse-engineering capability gain an advantage. Smaller wallets fall behind.
The manifest levels the field — every wallet reads the same interface from the
same place on chain.

This is the same principle as Ethereum's JSON ABI: the ABI reveals nothing the
bytecode doesn't already contain, but makes it usable without decompilation.
DarkWow's manifest does the same for object capabilities — it makes the
capability graph usable without decompiling WASM.

### What the Manifest Does NOT Reveal

The manifest describes capability **types** and action **structure** — not
specific capability **instances**. A DAO-Escrow manifest declares that the
contract has a `creator` capability with discriminant `0x00`, but does not
reveal *who* the creator is or *how many* endowments exist. Instance data —
the actual capabilities held by specific users — remains **encrypted via AEAD**
and is only discoverable by the holder's wallet through trial decryption.

The manifest is the **schema**, not the **data**.

## Specification

### Location

The manifest is a TOML document embedded in the `ix` (deployment instruction)
field of `DeployParamsV1`. It is stored **unencrypted** so that any wallet
scanning the deploy transaction can read it.

The `ix` field currently holds opaque bytes. When the first byte is `0x4D`
(ASCII `M`, for "manifest"), the remaining bytes are a TOML document. When the
first byte is anything else, the `ix` is interpreted as legacy opaque data.

**Encoding constraint**: The TOML bytes after the `0x4D` prefix SHALL be valid
UTF-8. The wallet's `from_deploy_ix()` uses `std::str::from_utf8()` to decode
the bytes. Non-UTF-8 bytes cause the manifest to be silently treated as absent
(functionally equivalent to no `0x4D` prefix).

### Format

```toml
[contract]
name = "dao_escrow"
category = "DAO"
description = "DAO-governed endowment with DrainProtection"
version = "1.0.0"
dependencies = ["native_token_v1"]

# --- Functions ---
# Every function the contract exposes. These map to WASM exports
# and correspond to ZK circuit proofs.

[[functions]]
name = "initialize"
code = 0x00
description = "Create a new DAO endowment"
requires_proof = true
proof_circuit = "init_v1"

[[functions]]
name = "pay_premium"
code = 0x01
description = "Pay premium to a drain-protected pool"
requires_proof = true
proof_circuit = "pay_premium_v1"

[[functions]]
name = "enable_drain_protection"
code = 0x02
description = "Enable drain protection on an endowment"
requires_proof = true
proof_circuit = "enable_drain_protection_v1"

# --- Capabilities ---
# Capability types this contract defines. Each has a discriminant
# (matching the circuit's capability encoding) and a name.

[[capabilities]]
discriminant = 0x00
name = "creator"
description = "The DAO endowment creator — can enable drain protection"

[[capabilities]]
discriminant = 0x01
name = "treasury_governor"
description = "Can propose and vote on fund allocation"

[[capabilities]]
discriminant = 0x02
name = "member"
description = "DAO member with voting rights"

# --- Actions ---
# For each function that exercises capabilities, declare what
# capabilities it requires, consumes, and produces.

[[actions]]
function = "initialize"
requires = { type = "none" }
consumes = []
produces = [
    { name = "creator", description = "Endowment creator capability" },
]

[[actions]]
function = "pay_premium"
requires = { type = "any", capabilities = ["creator", "treasury_governor"] }
consumes = []
produces = [
    { name = "receipt", description = "Premium payment confirmation" },
]

[[actions]]
function = "enable_drain_protection"
requires = { type = "all", capabilities = ["creator"] }
consumes = []
produces = [
    { name = "protected_endowment", description = "Drain-protected endowment reference" },
]

# --- State Trees ---
# Named sled trees the contract writes to. The wallet uses these
# to locate on-chain state for capability resolution.

[[trees]]
name = "daos"
description = "Active DAO endowments — keyed by bulla"

[[trees]]
name = "drain_protection"
description = "DrainProtection configurations — keyed by fund_id"

# --- ZK Circuits ---
# Proof circuits the contract references. These correspond to
# `.zk.bin` files compiled from `.zk` source.

[[circuits]]
name = "init_v1"
namespace = "dao_escrow"

[[circuits]]
name = "pay_premium_v1"
namespace = "dao_escrow"

[[circuits]]
name = "enable_drain_protection_v1"
namespace = "dao_escrow"

# --- Parameters ---
# Optional: parameter schemas for function calls. These inform
# the wallet's CLI and UX about required and optional parameters.

[[parameters]]
function = "initialize"
fields = [
    { name = "dao_bulla", type = "pallas_base" },
    { name = "endowment_token_id", type = "pallas_base" },
    { name = "owner_pubkey", type = "public_key", optional = true },
    { name = "bulla_blind", type = "pallas_base", optional = true },
    { name = "enable_drain_protection", type = "bool" },
]

[[parameters]]
function = "pay_premium"
fields = [
    { name = "dao_escrow_bulla", type = "pallas_base" },
    { name = "drain_protection_bulla", type = "pallas_base" },
    { name = "amount", type = "u64" },
]
```

### Capability Expression Types

The `requires` field on actions uses a simple recursive type:

```toml
# No capabilities required
requires = { type = "none" }

# Any one of the listed capabilities is sufficient
requires = { type = "any", capabilities = ["creator", "member"] }

# All listed capabilities are required
requires = { type = "all", capabilities = ["creator", "treasury_governor"] }

# Must NOT hold the listed capability
requires = { type = "not", capability = "member" }

# At least `count` of the listed capabilities are required
requires = { type = "threshold", count = 2, total = 3, capabilities = ["a", "b", "c"] }
```

### Parameter Types

| Type | Description | Encoding |
|------|-------------|----------|
| `u64` | 64-bit unsigned integer | Little-endian |
| `pallas_base` | Pallas base field element | 32 bytes, little-endian |
| `pallas_scalar` | Pallas scalar field element | 32 bytes, little-endian |
| `public_key` | Compressed public key | 32 bytes |
| `contract_id` | Contract identifier | 32 bytes |
| `bool` | Boolean | 1 byte (0 or 1) |
| `string` | UTF-8 string | Length-prefixed varint |
| `bytes` | Opaque bytes | Length-prefixed varint |

### Typed Capability Fields

The manifest is the type declaration ([ocap.md §7](ocap.md)). For the wallet's
capability engine to *construct* a capability type from a manifest — the scan's
coverage gate ([wallet.md §2.2](wallet.md)) and the write path's generic prover
([wallet.md §6.4.1](wallet.md)) — the declarations SHALL carry the type
parameters themselves. Three fields provide them. Their vocabularies are
closed: every name SHALL come from the referenced table, and an unknown name
is a parse error, not a passthrough.

**`[[capabilities]].primitives`** — the primitive types this capability
composes. Names are drawn from the primitive table
([type-system.md §8.1](type-system.md), mirrored by the capability SDK's
`Primitive` enum): `SecretKey`, `PublicKey`, `Nullifier`, `Commitment`,
`ContractId`, `FuncId`, `AssetId`, `MerkleNode`, `OwnedSecretKey`,
`MiningRecipient`.

**`[[capabilities]].note_schema`** — the ordered field layout of the
capability's AEAD-encrypted note. Each entry is `{ name, type }` with `type`
drawn from the Parameter Types table above. A schema for a capability that
proves Merkle inclusion SHALL include the leaf field
`{ name = "commitment", type = "pallas_base" }`.

**`[[actions]].required_barbs`** — the barbs the action's predicate requires.
Names are drawn from the barb table ([type-system.md §1.1](type-system.md),
mirrored by the capability SDK's `Barb` enum), e.g. `Spend`, `Nullify`,
`Commit`, `Prove`, `Verify`, `Dispatch`, `Gate`, `Denominate`,
`ProveInclusion`, `Encrypt`.

The wallet's scan constructs the capability type by passing the declared
primitives and required barbs through `wallet_construct`
([wallet.md §7.1](wallet.md)). If the primitives do not cover the barbs, the
composition is not a valid capability type and the note is dropped — the
contract's declaration is at fault, never the wallet
([wallet.md §9](wallet.md), [type-system.md §13](type-system.md)).

**`[[circuits]].witness_map`** — the ordered witness-binding declaration for
the generic prover ([wallet.md §6.4.1](wallet.md)). A zkas binary's witness
section is an ordered, typed, unnamed list; `witness_map` names the source of
each slot, in slot order: `note:<field>`, `param:<field>`, `secret`,
`merkle_path`, `leaf_position`, `blind`, `tx_commitment`, `tx_nonce`. The
capability SDK type-checks every entry against the slot's declared witness
type and rejects the construction on any mismatch.

Example (generic capability — delegate an action to another holder):

```toml
[[capabilities]]
discriminant = 0
name = "delegation_right"
description = "Authority to delegate an action to another holder — consumable"
primitives = ["SecretKey", "Commitment", "Nullifier", "ContractId", "FuncId", "AssetId", "MerkleNode"]
note_schema = [
    { name = "quantity", type = "u64" },
    { name = "commitment", type = "pallas_base" },
]

[[actions]]
function = "delegate"
requires = { type = "any", capabilities = ["delegation_right"] }
consumes = ["delegation_right"]
produces = [{ name = "delegation_right", description = "Delegation right transferred to the recipient" }]
required_barbs = ["Spend", "Nullify", "Commit", "Dispatch", "Gate", "Denominate"]

[[circuits]]
name = "Delegate_V1"
namespace = "example_contract"
witness_map = [
    "secret",
    "note:quantity",
    "blind",
    "merkle_path",
    "leaf_position",
    "tx_commitment",
    "tx_nonce",
]
```

The primitive names in `primitives` are drawn from the closed vocabulary
above. The capability name (`"delegation_right"`) is a human-readable label
chosen by the contract author — it SHALL be unique within the contract's
declared capabilities. The action name (`"delegate"`) SHALL match a declared
function name in `[[functions]]`.

These fields describe capability **types**, never instances — the same
schema/data split as the rest of the manifest. Declaring them requires
reading the contract's actual circuits and note layouts; they SHALL NOT be
copied from another contract's manifest.

### Circuit Binary Delivery

The generic prover needs the zkas binary named by each `[[circuits]]` entry:

- **Genesis contracts** — binaries are embedded at compile time (Stage 1
  below), alongside the embedded manifest.
- **User-deployed contracts** — binaries travel in the `DeployV1` payload.
  The wallet extracts them when it scans the deploy transaction and stores
  them in its `zkas_binaries` store keyed by
  `(ContractId, namespace, circuit name)` ([wallet.md §3](wallet.md)).

No contract crate needs to be linked into the wallet for its proofs to be
constructed.

## Full Lifecycle

The manifest flows through the system in six stages:

### 1. Authoring

The contract developer writes a `manifest.toml` in their contract's source directory.
This file lives alongside the contract's Rust source and is committed to the repo.
For genesis contracts (Promissory Note, Identity, Oracle, Attestation, Native Token, Deployooor), the manifest
is embedded at compile time. For user-deployed contracts, it's provided at deploy
time.

### 2. Deployment

The deployer passes the manifest to the wallet via the `--manifest` flag:

```
dwow_wallet contract deploy <auth> <wasm> --manifest manifest.toml
```

Implementation: `bin/dww/src/dispatch.rs` — `build_deploy_ix()` reads the TOML file
via `std::fs::read_to_string`, parses it with `ContractManifest::from_toml()`, and
encodes it with `to_deploy_ix()` (0x4D magic byte + TOML bytes). The resulting bytes
are placed in `DeployParamsV1::ix`. If the TOML is invalid, the error is returned
to the user before any deploy transaction is built.

### 3. Scanning

When the wallet scans a block and encounters a `DeployV1` call:

Implementation: `bin/dww/src/rpc.rs` — `scan_block_linear()`, DeployV1 handler.

1. Extract `ix` from `DeployParamsV1` (already decoded from contract call data)
2. Call `ContractManifest::from_deploy_ix(&params.ix)` — returns `None` if no
   `0x4D` prefix, `Some(Err(...))` if malformed, `Some(Ok(manifest))` on success
3. On success: call `manifest.to_toml()` to serialize, store via
   `wallet.store_manifest(&contract_id_str, &manifest_json)`
4. On malformed: log the error, continue — the contract is still usable via
   generic AEAD discovery
5. On absent: skip silently — may still have legacy `ContractMetadata`

All operations are synchronous local SQLite writes. No async, no network.

### 4. Resolution

Implementation: `bin/dww/src/capability.rs` — `CapabilityResolver::resolve()`.

During position resolution, the wallet processes capabilities from the generic
AEAD scan. For each contract WITHOUT a hardcoded Rust capability descriptor,
it checks for a stored manifest:

1. Call `wallet.get_contract_manifest(&contract_id_str)` — SQLite read
2. If manifest exists: create `ManifestResolver`, iterate declared capabilities,
   produce typed `Capability` structs with manifest-derived names and discriminants
3. If no manifest: fall through to existing opaque generic path (capabilities
   surfaced as `unknown` type)

This means a contract deployed WITH a manifest immediately shows named
capabilities in `dwow_wallet position` after scanning, even without a
Rust client crate or hardcoded descriptor.

### 5. Query

Implementation: `bin/dww/src/dispatch.rs` — `Contract::Show` handler,
`bin/dww/src/manifest_resolver.rs` — `ManifestResolver::describe()`.

```
dwow_wallet contract show <contract_id>
```

Reads the stored manifest via `get_contract_manifest()`, creates a
`ManifestResolver`, and prints the full interface: contract name, category,
version, description, functions (with opcodes and proof circuits), capabilities
(with discriminants), actions (with require/consume/produce), state trees,
ZK circuits, dependencies, and parameter schemas.

### 6. Invocation

Implementation: `bin/dww/src/manifest_resolver.rs` — `ManifestResolver::validate_params()`.

```
dwow_wallet contract invoke <cid> pay_premium \
    --params '{"dao_escrow_bulla": "...", "amount": 1000}'
```

Before building the transaction, `validate_params()` checks the JSON params
against the manifest's parameter schema: required fields present, types match
(`u64` is a number, `pallas_base` is a string of ≥32 chars, `bool` is boolean).
Invalid params are reported immediately — no wasted ZK proof generation.

## Trust Model

**Don't trust, verify.** A manifest is self-reported by the contract deployer.
A malicious deployer can include a deceptive manifest. The wallet applies a
[three-layer trust model](contract-trust-model.md) to every contract:

1. **Trust Tier** — Who deployed this? (Genesis/Self/Attested/Unverified)
2. **WASM Verification** — Does the manifest match the binary? (Mechanical)
3. **Attestation** — Does the binary do what it claims? (Social, on-chain)

The manifest is the *claim*. Verification and attestation are the *checks*.

### Why Manifests Need Trust Verification

The manifest model gives DarkWow a unified ecosystem — every wallet reads the
same on-chain manifest for every contract. But it also makes manifests an attack
surface. A deployed contract can ship a manifest claiming "this is a DEX" while
the actual WASM exports `steal_funds()`. The wallet cannot prevent this by design
— the contract is already deployed, its WASM is immutable, and the manifest is
just a TOML file.

DarkWow's defense is **adversarial caveat emptor**: verify what you can mechanically,
consult on-chain reputation for the rest, and let the user decide.

### The Three Layers in Detail

**Layer 1 — Trust Tier**: The wallet classifies every contract into one of four
tiers based on how it was deployed. [Nine contracts](genesis.md) are hardcoded
at genesis and carry `[GENESIS]` status — they are the cryptographic
primitives the trust model
itself depends on. Contracts deployed by a key the user controls are `[OWN]`.
Contracts with on-chain attestations from trusted issuers are `[ATTESTED by X]`.
Everything else is `[UNVERIFIED]`.

**Layer 2 — WASM Verification (mechanical)**: The wallet parses the deployed WASM
binary, extracts exported function names and ZK circuit data sections, and compares
them against the manifest. A mismatch — a claimed function not in the WASM, or a
circuit in the WASM not in the manifest — is flagged. This catches manifest lies
but not sophisticated deception where the WASM exports the claimed function but
it doesn't do what the name implies.

**Layer 3 — Attestation (social, on-chain)**: Identity and Attestation are genesis
contracts precisely so that trusted issuers can vouch for contracts on-chain. An
attester who has inspected a contract's source and behavior can create an
`AttestationV1` record binding their identity to the contract's manifest claims.
The wallet resolves these during `contract show`. Attestations carry the issuer's
reputation — a slashed or revoked issuer's attestations become worthless.

### Caveat Emptor

The wallet **never blocks interaction** based on trust tier or WASM verification
results. It annotates. It warns. The user decides. This is the same posture as
Bitcoin Core displaying "unconfirmed" on a zero-conf transaction — information,
not policy. DarkWow's position is that the chain is an adversarial environment
and the wallet is a navigation tool, not a guardian.

## How the Wallet Uses the Manifest — Summary

The manifest is a **declarative contract interface** that flows through the
wallet in six stages. Each stage is a single function call into existing
modules — no new async, no new dependencies. The data pipeline is:

```
DeployV1 tx on chain
  → scan_block_linear() detects 0x4D prefix        [rpc.rs]
  → ContractManifest::from_deploy_ix() parses TOML  [sdk/src/manifest.rs]
  → wallet.store_manifest() stores in SQLite        [walletdb.rs]
  → CapabilityResolver reads manifest               [capability.rs]
  → ManifestResolver answers queries                [manifest_resolver.rs]
  → CLI: contract show / contract invoke            [dispatch.rs]
```

## What Stays in Rust

The manifest does NOT replace:

- **ZK circuit binaries** — `.zk.bin` files SHALL still be compiled; they reach
  the wallet's generic prover embedded (genesis) or via the `DeployV1` payload
  (user-deployed) — see Circuit Binary Delivery above
- **The one bespoke citizen** — NativeToken's hardcoded client
  ([wallet.md §6.4](wallet.md)); consensus-critical, not per-contract business logic

Contract client crates (type-safe Rust wrappers for building contract calls)
remain in contract crates as **optional tooling** for non-wallet consumers.
The wallet does not require, link, or invoke them: wallet-side invocation is
manifest-driven end-to-end — typed capability fields for discovery, the
`witness_map` and the generic prover for construction
([wallet.md §6.4.1](wallet.md)). A contract WITH a manifest is fully usable
by any wallet; a contract WITH a client crate additionally gets type-safe
builders for its own tests and tools.

## See Also

- [Contract Trust Model](contract-trust-model.md) — Three-layer verification: don't trust, verify
- [Wallet Architecture](wallet.md) — How the wallet uses manifests for contract discovery
- [Object Capabilities](ocap.md) — The O-Cap model that manifests describe

## Backwards Compatibility

- Contracts deployed WITHOUT a manifest continue to work exactly as before.
  The `ix` field without the `0x4D` prefix is interpreted as legacy opaque data.
- Wallets that don't understand manifests continue to work — they skip the
  manifest parsing and fall back to existing behavior.
- The manifest format is versioned via the `version` field in `[contract]`.
  Wallets can check the version before parsing.

## Relationship to Existing Infrastructure

| Existing System | Manifest Equivalent |
|-----------------|---------------------|
| `ContractMetadata` (on-chain) | `[contract]` section |
| `ContractMetadataRegistry` (hardcoded) | `[[functions]]` section |
| `CapabilityDescriptor` (Rust fn) | `[[capabilities]]` + `[[actions]]` |
| Sled tree name constants | `[[trees]]` section |
| ZK circuit constants | `[[circuits]]` section |
| `Contract::dependencies()` | `dependencies` field |
| (none) | `[[parameters]]` section |
