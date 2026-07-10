# Wallet Architecture: Capability Type Construction Engine

This document defines the DarkWow wallet architecture. It SHALL be read
in conjunction with the **[Type System Specification](type-system.md)** and
**[O-Cap: Emergent Types](ocap.md)**. The type system defines primitive types
and their behavioral positions. The o-cap document defines how primitives
compose into capability types. This document defines how the wallet, as a
**capability type construction engine**, discovers primitives at scan time,
composes them into capability types, and presents the user with actions.

## 0. Foundation: The Wallet as a Type Construction Engine

The wallet does not hardcode capability types. It constructs them at scan time
from three inputs:

1. **Primitive names discovered via AEAD decryption.** The wallet holds
   `SecretKey` names (ν-restricted to the holder's declared identity).
   It attempts decryption on every `AeadEncryptedNote` it observes on chain.
   A successful decryption means: the wallet now possesses the primitive names
   inside that note (value, token_id, spend_hook, user_data, blind). This is
   the ρ-calculus input operation — the wallet has received a name.

2. **Contract manifests read from the chain.** The manifest declares what
   primitive types the contract's capabilities compose, what actions are
   available, and what ZK circuits verify each action's predicate. The
   manifest IS the type declaration.

3. **The type system's composition rules** ([ocap.md §2](ocap.md)). Given
   the primitives and the manifest, the wallet constructs the capability
   type. The type tells the wallet what the user can DO.

The wallet SHALL NOT store a generic `cap_id: String`. It SHALL store a
typed composition whose structure is determined by the contract manifest
and the primitives discovered during scan.

## 1. Wallet State as a Pure Function

Per the type system's referential transparency requirement:

```
WalletState = f(AccountManager, ChainBlocks)
```

- **AccountManager** provides the declared identity and per-block derived
  keys ([type-system.md §8.3](type-system.md)). Every derivation step is a
  pure Poseidon hash.
- **ChainBlocks** are the synced P2P blocks stored locally in `wallet.db`.

Given identical inputs, the wallet SHALL produce byte-identical state.
This is possible because every operation in the deterministic pipeline is
a pure function ([type-system.md §7](type-system.md)):

1. Key derivation: `poseidon_hash([secret, cid, instance])` — pure
2. AEAD decryption: deterministic for given ciphertext + key
3. Coin commitment: `poseidon_hash(7 elements)` — pure
4. Nullifier: `poseidon_hash(2 elements)` — pure
5. Merkle tree: ordered append to `BridgeTree` — deterministic
6. Block iteration: sequential by height — deterministic
7. SQLite inserts: `INSERT OR IGNORE` — idempotent

## 2. Scan Paths: Primitives → Capability Types

The wallet discovers capabilities through two scan paths. Both operate on
local chain state (no network fetches, no RPC). Both construct capability
types from discovered primitives.

### 2.1 Path 1: Native Token (Consensus Coinbase)

The native token is the sole special-citizen path because it is the
consensus asset required for fee payment. The scan:

1. Decodes `AeadEncryptedNote` from coinbase call data.
2. Attempts AEAD decryption with each wallet secret.
3. If decryption succeeds: the wallet possesses the coinbase primitive names.
4. Constructs the capability type:

```
Capability(native_token_coinbase, reward) ≡ compose(
    SecretKey(↓spend, ν-restricted),
    Coin(↓commit, value = reward),
    Nullifier(↓nullify),
    ContractId(↓dispatch = NATIVE_TOKEN),
    FuncId(↓gate = PoWRewardV1),
    TokenId(↓denominate = DRKW),
    MerkleNode(↓prove-inclusion)
)
```

5. Stores the typed composition in `held_capabilities`.

### 2.2 Path 2: Generic AEAD (All Other Contracts)

For every contract call in every transaction:

1. Scans `call.data` for `AeadEncryptedNote` structures.
2. Attempts decryption with each wallet secret.
3. If decryption succeeds: the wallet possesses the output's primitive names.
4. Resolves the contract's manifest (reads the type declaration).
5. Constructs the capability type per the manifest's `[[actions]]` section.
6. Stores the typed composition.

Path 2 SHALL be a single generic function. No contract ID lookup. No
per-contract branch. The AEAD authentication tag IS the discriminator.
The manifest IS the type declaration.

## 3. Data Stores

The wallet has a single database: `wallet.db` (SQLite). Each table stores
a specific category of information in the capability type construction
pipeline.

| Table | Contents | Type-Level Role |
|-------|----------|-----------------|
| `chain_blocks` | Synced blocks by height | Raw material for scan |
| `held_capabilities` | Typed capability compositions | Constructed types |
| `capability_proofs` | Merkle proofs for each held capability | `↓prove-inclusion` evidence |
| `scanned_blocks` | Scan progress markers | Pure function checkpoint |
| `merkle_trees` | Capability commitment tree checkpoints | Merkle root state |
| `key_lifecycle` | Additive lifecycle keys | ν-restricted name store |
| `contract_metadata` | On-chain contract manifests | Type declarations |

## 4. Authority: Name Possession Only

The wallet enforces the authority model from [type-system.md §5](type-system.md):
a process SHALL perform action A if and only if it possesses the name for A.

- `AccountManager` provides the declared identity as `OwnedSecretKey`
  ([type-system.md §8.3](type-system.md)). No `::random()` constructor exists
  on `OwnedSecretKey`. The declared identity cannot be fabricated.
- `MiningRecipient` is constructible ONLY from `from_account()`. A bare
  `PublicKey` parsed off the wire cannot become a coinbase recipient.
- `secrets_for_contract()` derives per-contract keys from the declared
  identity. The derivation is deterministic and reproducible.

No ambient authority exists. The wallet SHALL NOT have a "user identity"
field. It SHALL NOT store or reference a principal identifier. Authority
flows ONLY through explicit name possession at the type level.

## 5. Manifest-First Discovery

Contracts carry their own type declarations on-chain — TOML manifests
embedded in deployment transactions. The manifest declares:

- What primitive types the contract's capabilities compose (the `requires`
  fields in `[[actions]]`).
- What actions are available (the `[[functions]]` section).
- What ZK circuits verify each action's predicates (the `[[circuits]]`
  section).
- What parameters each function expects (the `[[parameters]]` section).

The wallet reads the manifest and constructs capability types without
per-contract code. Adding support for a new contract requires zero wallet
code changes. The manifest IS the type declaration.

### 5.1 Manifest Discovery Pipeline

```
DeployV1 tx on chain
  → scan_block_linear() detects 0x4D magic byte
  → ContractManifest::from_deploy_ix() parses TOML
  → wallet.store_manifest() stores type declaration
  → CapabilityResolver reads manifest
  → Wallet constructs capability types from manifest + discovered primitives
  → CLI: contract show / contract invoke --params
```

### 5.2 Trust Model

Manifests can lie. The wallet SHALL defend against this with three layers:

1. **WASM Verification (mechanical):** Parse WASM binary, extract exports
   and circuit data sections, compare against manifest claims.
2. **Genesis Capabilities (social):** Trusted issuers create on-chain
   attestations for contracts they have inspected.
3. **Caveat Emptor (user):** Trust tiers annotate contract displays. The
   wallet SHALL NOT block interaction based on trust tier. It warns. The
   user decides.

## 6. Transaction Construction

When the user invokes a capability (e.g., `dwow_wallet contract invoke
<contract_id> transfer --params '{...}'`), the wallet:

1. Resolves the manifest for `<contract_id>`.
2. Selects held capabilities whose constructed types match the action's
   `requires` field.
3. Generates ZK proofs that the holder knows the primitive names (witnesses)
   satisfying the predicate language L_{r,s}.
4. Computes nullifiers for each consumed capability.
5. Attaches the fee capability (DRKW) per the fee builder.
6. Broadcasts the transaction via P2P gossip.

The capability (the note being exercised) is NOT in the params. The params
are pure business-logic arguments. The wallet automatically selects
capabilities, generates ZK proofs, and attaches nullifiers.

## 7. Soundness

The wallet's type construction is formalized and verified in the Lean4
calculus of constructions at `proofs/lean/src/DarkFi/Capability/Wallet.lean`.
All theorems are proved with zero `sorry`.

### 7.1 Soundness Theorem

`walletConstruct_sound`: If `walletConstruct primitives resource action`
returns `some ct`, then `ct` is a valid `CapabilityType` — the composed
barbs of the primitives cover the resource's required barbs. The proof
extracts the `coversBarbs` field from the constructed type.

### 7.2 Completeness Theorem

`walletConstruct_complete`: If a `CapabilityType` exists for primitives
`p`, resource `r`, and action `s`, then `walletConstruct p r s` returns
`some` (not `none`). The proof rewrites the primitives list and uses
the existing `coversBarbs` proof.

### 7.3 Primitive Preservation

`walletConstruct_preservesPrimitives`: The primitives in the returned
capability type are exactly the primitives passed to the constructor.
No primitives are lost, modified, or added during construction.

### 7.4 Determinism and Idempotence

`walletConstruct_deterministic`: Given identical inputs, the wallet
always produces the same capability type. This is the type-level
expression of the wallet's pure function property (§1).

`walletConstruct_idempotent`: Repeated construction from the same
inputs yields identical results — trivially true by referential
transparency.

### 7.5 Concrete Constructibility

Three concrete proofs verify that the wallet can construct real
capability types:

- `nativeTokenTransfer_constructible`: from `[SecretKey, Coin, Nullifier, ContractId, FuncId, TokenId, MerkleNode]`
- `daoVote_constructible`: same primitives, different resource
- `tenderBid_constructible`: same primitives, different resource

### 7.6 Failure Case

`walletConstruct_rejects_emptyPrimitives`: If no primitives are provided
and the resource requires non-empty barbs, construction correctly returns
`none`. The wallet does not fabricate capabilities from nothing.

### 7.7 Lean4 Source

`proofs/lean/src/DarkFi/Capability/Wallet.lean` — all theorems above.
Run `lake build` in `proofs/lean/` to type-check.

## 8. References

- **[Type System Specification](type-system.md)** — Primitive types, behavioral positions, invariants.
- **[O-Cap: Emergent Types](ocap.md)** — Capability type construction from primitives.
- **[Manifest System](manifest.md)** — Contract type declarations.
- **[Genesis Contracts](genesis.md)** — Capability primitive layer.
- **[Contract Trust Model](contract-trust-model.md)** — WASM verification, attestations, caveat emptor.
