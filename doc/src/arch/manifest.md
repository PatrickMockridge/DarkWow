# Contract Manifest

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

## How the Wallet Uses the Manifest

### At Scan Time

When the wallet encounters a `DeployV1` call during block scanning:

1. Decode `DeployParamsV1::ix`
2. If `ix[0] == 0x4D`: parse the remaining bytes as manifest TOML
3. Store parsed manifest fields in `contract_metadata` table
4. Populate function signatures, capability types, and action descriptors

### At Position Resolution

`CapabilityResolver::resolve()` currently uses hardcoded per-contract descriptors.
With a manifest:

1. For contracts WITH a parsed manifest: use manifest-declared capability types
   and actions directly
2. For contracts WITHOUT a manifest: fall back to the existing generic path
   (AEAD-decrypted capabilities stored as `unknown` type)

### At CLI Interaction

```
# Show a contract's interface
dwow_wallet contract show <contract_id>

# Output:
#   Contract: dao_escrow (DAO)
#   Functions:
#     initialize (0x00) — Create a new DAO endowment
#     pay_premium (0x01) — Pay premium to a drain-protected pool
#   Capabilities:
#     creator (0x00) — The DAO endowment creator
#     treasury_governor (0x01) — Can propose and vote
#     member (0x02) — DAO member with voting rights
#   Dependencies: native_token_v1

# Invoke with parameter validation
dwow_wallet contract invoke <cid> initialize \
    --params '{"dao_bulla": "...", "endowment_token_id": "...", "enable_drain_protection": true}'
```

## What Stays in Rust

The manifest does NOT replace:

- **ZK circuit binaries** — `.zk.bin` files must still be compiled and available to the prover
- **Client builders** — type-safe Rust wrappers for building contract calls remain in contract crates
- **Complex capability resolvers** — imperative "scan sled tree, match pubkey" logic stays in `capability.rs`

The manifest makes these **optional**. A contract WITH a manifest is auto-discoverable
by any wallet. A contract WITH a Rust client crate additionally gets type-safe builders.
The manifest is the minimum; the crate is the enhancement.

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
