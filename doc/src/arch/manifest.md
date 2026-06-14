# Contract Manifest

## Implementation Status

| Layer | Status | Location |
|-------|--------|----------|
| Specification | Complete | This document |
| Python model | Complete (13 tests) | `contrib/model/wallet_model.py` |
| Rust data types | Complete (6/6 unit tests) | `src/sdk/src/manifest.rs` |
| TOML manifests | Complete (29 contracts) | `src/contract/*/manifest.toml` |
| Wallet resolver | Complete | `bin/drk/src/manifest_resolver.rs` |
| CLI `contract show` | Complete | `bin/drk/src/dispatch.rs` |
| Deploy `--manifest` flag | Complete | `bin/drk/src/dispatch.rs` (build_deploy_ix) |
| Scan-side integration | Complete | `bin/drk/src/rpc.rs` (DeployV1 handler) |
| CapabilityResolver fallback | Complete | `bin/drk/src/capability.rs` (resolve()) |
| On-chain manifest hash | Pending | Deployooor hardening |

29 contracts have manifests: 3 genesis (Promissory Note with full capability descriptors,
Native Token + Deployooor as FYI), 19 with Rust capability descriptors, 7 FYI.
All 29 round-trip through the Python parser.

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

## Full Lifecycle

The manifest flows through the system in six stages:

### 1. Authoring

The contract developer writes a `manifest.toml` in their contract's source directory.
This file lives alongside the contract's Rust source and is committed to the repo.
For genesis contracts (Promissory Note, Native Token, Deployooor), the manifest
is embedded at compile time. For user-deployed contracts, it's provided at deploy
time.

### 2. Deployment

The deployer passes the manifest to the wallet via the `--manifest` flag:

```
dwow_wallet contract deploy <auth> <wasm> --manifest manifest.toml
```

Implementation: `bin/drk/src/dispatch.rs` — `build_deploy_ix()` reads the TOML file
via `std::fs::read_to_string`, parses it with `ContractManifest::from_toml()`, and
encodes it with `to_deploy_ix()` (0x4D magic byte + TOML bytes). The resulting bytes
are placed in `DeployParamsV1::ix`. If the TOML is invalid, the error is returned
to the user before any deploy transaction is built.

### 3. Scanning

When the wallet scans a block and encounters a `DeployV1` call:

Implementation: `bin/drk/src/rpc.rs` — `scan_block_linear()`, DeployV1 handler.

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

Implementation: `bin/drk/src/capability.rs` — `CapabilityResolver::resolve()`.

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

Implementation: `bin/drk/src/dispatch.rs` — `Contract::Show` handler,
`bin/drk/src/manifest_resolver.rs` — `ManifestResolver::describe()`.

```
dwow_wallet contract show <contract_id>
```

Reads the stored manifest via `get_contract_manifest()`, creates a
`ManifestResolver`, and prints the full interface: contract name, category,
version, description, functions (with opcodes and proof circuits), capabilities
(with discriminants), actions (with require/consume/produce), state trees,
ZK circuits, dependencies, and parameter schemas.

### 6. Invocation

Implementation: `bin/drk/src/manifest_resolver.rs` — `ManifestResolver::validate_params()`.

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
3. **Attestation** — Does the binary do what it claims? (Social, deferred)

The manifest is the *claim*. Verification and attestation are the *checks*.
See [Contract Trust Model](contract-trust-model.md) for the full specification.

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

- **ZK circuit binaries** — `.zk.bin` files must still be compiled and available to the prover
- **Client builders** — type-safe Rust wrappers for building contract calls remain in contract crates
- **Complex capability resolvers** — imperative "scan sled tree, match pubkey" logic stays in `capability.rs`

The manifest makes these **optional**. A contract WITH a manifest is auto-discoverable
by any wallet. A contract WITH a Rust client crate additionally gets type-safe builders.
The manifest is the minimum; the crate is the enhancement.

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
