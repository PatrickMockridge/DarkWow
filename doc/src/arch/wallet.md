# Wallet Architecture: Capability Type Construction Engine

This document defines the DarkWow wallet architecture. It SHALL be read
in conjunction with the **[Type System Specification](type-system.md)** and
**[O-Cap: Emergent Types](ocap.md)**. The type system defines primitive types
and their behavioral positions. The o-cap document defines how primitives
compose into capability types. This document defines how the wallet, as a
**capability type construction engine**, discovers primitives at scan time,
composes them into capability types, and presents the user with actions.

## Current Implementation Status

The shipping wallet (`bin/dww`) implements a subset of the full ρ-calculus
architecture described in §§0-9 below. This section documents what works today.

### What Works [IMPLEMENTED]

- **Path 1 (Native Token):** Full scan + write for DRKW. Coinbase discovery,
  transfers, burns, fee payments — all with real ZK proofs. `scan.rs:343-508`,
  `lib.rs:797-1001`.
- **Path 2 Read (Manifest Discovery):** The scan path resolves manifests, applies
  coverage gates, and constructs typed CapRecords for all contracts. `scan.rs:588-713`.
- **AccountManager:** Declared identity, BIP39/BIP32, key derivation, key coordinates.
  `crates/dwow-accounts/src/lib.rs`.
- **wallet_construct:** The composition function exists in Rust
  (`src/sdk/src/capability.rs:462-488`) and is proven sound in Lean4
  (`proofs/lean/src/DarkFi/Capability/Wallet.lean`). Used during scan.
- **Manifest pipeline:** Author → deploy → scan → store → resolve → query.
  Full lifecycle implemented per `manifest.md`.
- **Database schema:** held_capabilities, capability_proofs, scanned_blocks,
  merkle_trees, key_lifecycle, contract_metadata, zkas_binaries — all implemented.
- **Trust tiers:** Genesis / SelfDeployed / Attested / Unverified — implemented
  in `capability.rs` with CLI display.
- **CLI:** initialize, balance, address, scan, sync, contract show, contract deploy,
  contract lock, capabilities, tree, position, diagnostic, broadcast.
- **Full node:** P2P sync via GetTip/GetBlocks (same protocol as mining nodes),
  local SQLite DB. Zero RPC dependency for scan operations.
- **Generic prover (§6.4.1):** Manifest-driven proof construction for any contract
  with a stored manifest. SDK-side witness-binding (`prover.rs`,
  `encode_params_by_schema`), wallet-side `ProverImpl` (`prover_impl.rs`),
  `zkas_binaries` store with genesis circuit embed. Capability selection by
  asset_id via `resolve_transfer_contract`. Non-native transfers route through
  `invoke_contract` → manifest → prover — ONE path, zero per-contract code.
- **P2P three-tier feature gate:** net-wallet (dww) and net-full (dwowd)
  active; net-node compiled transitively; send-side fan-out gossip implemented
  (`linear_broadcast.rs:206-256`), receive-side relay intentionally flood
  (`type-system.md` §10.2); standalone compile gate in CI.

### What Is Spec-Only [VISION]

- **Provisional state (§6.5):** Capability spend-state lifecycle
  (Unspent/Reserved/Spent) not implemented; a simpler immediate-revoke model
  (`mark_tx_exercise`) is used instead.
- **Write-path barb-cover selection (§6.2):** `wallet_construct` is not yet
  used for capability selection on the write path.
- **Seed discipline (§6.1):** `OsRng` is used directly in dispatch; explicit
  `Seed` plumbing through the pure construction function is not complete.
- **Write-path Lean4 obligations (§7.8):** `construct_sound`, `construct_deterministic`,
  `nullifier_completeness` are stated but not yet proved.

### What the Shipping Wallet Can Do

- Initialize a wallet with BIP39 seed phrase
- Sync chain state via P2P (GetTip/GetBlocks — same protocol as mining nodes)
- Scan blocks for DRKW — coinbase rewards, transfers, fees, burns
- Show balance, held capabilities, and merkle tree state
- Send DRKW transfers with full ZK burn/mint proofs
- Deploy WASM contracts via Deployooor
- Inspect any contract's on-chain manifest (`contract show`)
- Invoke non-ZK contract functions generically via manifest resolution
- All operations are local — the wallet is a full node, no RPC dependency

---

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

### 0.1 Component Architecture

The wallet is composed of four components. **The wallet core is all a pure
function**: its two computations — the scan (§2) and transaction construction
(§6.1) — are referentially transparent, and everything else the core does is
gather their inputs and persist their outputs. The core delegates to three
components, each with a defined interface. This decomposition is normative;
every implementation change SHALL be attributable to exactly one component.

```
┌────────────────────────────────────────────────────────────────┐
│ Wallet core (bin/dww) — owns wallet.db                         │
│   pure:  scan_block(...)                          (read, §2)   │
│          Transaction = f(caps, action, params,                 │
│                          secrets, seed)          (write, §6.1) │
│   shell: DB reads gather inputs; DB writes persist outputs     │
└───────┬───────────────────────┬───────────────────────┬────────┘
        │                       │                       │
        ▼                       ▼                       ▼
  AccountManager            dwowd-sdk             Capability SDK
  (crates/dwow-accounts)    (src/sdk: crypto/,    (src/sdk: capability.rs,
  declared identity;        tx.rs, blockchain.rs,  manifest.rs,
  key derivation and        deploy.rs, wasm/)      contract_client.rs,
  resolution                typed primitives;      generic prover)
                            tx assembly            ALL manifest paths
```

**Wallet core** (`bin/dww`). Owns `wallet.db`. Purity applies to the scan and
construction functions; the core's DB reads gather those functions' inputs and
its DB writes persist their outputs. The core SHALL contain zero contract
names. The only contract-specific citizens are **native_token** (the
consensus-critical asset, §2.1/§6.4) and **deployooor** (deployment
infrastructure — without it, manifest discovery is impossible). The nine
genesis `ContractId`s appear only in the trust-tier display table (§5.2).
Nothing else in the core names, imports, or branches on a contract.

**AccountManager** (`crates/dwow-accounts`). The declared identity (§4). ALL
secret resolution is delegated to it — `find_owner`, `resolve_key`,
`secrets_for_contract` — and every derivation step is a pure Poseidon hash
([type-system.md §8.3](type-system.md)). The wallet core SHALL NOT store raw
secrets; it stores key *coordinates* and resolves them through the
AccountManager at the moment of use.

**dwowd-sdk** (`src/sdk`: `crypto/`, `tx.rs`, `blockchain.rs`, `deploy.rs`,
`wasm/`). The nominal cryptographic primitive types
([type-system.md §8.1](type-system.md)), transaction assembly, and chain data
structures. Barb preservation holds across this boundary: no value crosses it
as `[u8; 32]` ([type-system.md §2.2](type-system.md)).

**Capability SDK** (`src/sdk`: `capability.rs`, `manifest.rs`,
`contract_client.rs`, and the generic prover). A distinct architectural
component — physically inside the `dwow-sdk` crate, logically separate from
the dwowd-sdk. It owns ALL manifest paths: manifest parsing and resolution
(the type declarations, §5), `wallet_construct` (the one composition function,
§7.1), generic action dispatch (`ManifestContractClient`), and generic proof
construction (§6.4). The wallet core calls the capability SDK; it SHALL NOT
reimplement, inline, or bypass any of these functions.

**Interface rules (normative):**

1. The wallet core SHALL NOT pass contract names into the capability SDK.
   Contract identity enters exclusively as a `ContractId` plus the stored
   manifest resolved by that id.
2. Function names and contract names are distinct namespaces and SHALL NOT
   be conflated. An action is addressed as `(ContractId, action_name)` where
   `action_name` is declared by the manifest.
3. Per-contract knowledge lives in exactly two places: the contract's own
   crate, and the contract's manifest. The manifest is the only one of the
   two the wallet reads. A contract crate is optional tooling
   ([manifest.md](manifest.md)); the wallet SHALL NOT require one.
4. The capability SDK is contract-agnostic: given the same manifest and the
   same primitives, it behaves identically whichever contract they came from.
   Specific barb compositions used by one contract are not "that contract's
   capability" — they are constructions any present or future contract may
   declare.

### 0.1.1 Crate Dependency Graph

The wallet's four components form a strict dependency ordering. Each arrow is a
compile-time dependency; no component SHALL reverse or bypass an arrow.

```
dwow-accounts (crates/)     ← zero deps beyond dwow-sdk crypto types
       ↑                        (SecretKey, PublicKey, ContractId — pure key math)
dwow-sdk (src/sdk/)         ← dwow-serial, pasta_curves, rand_core
       ↑                      (no dwow_core — the SDK is protocol-layer types;
       │                       the node crate implements ZK/VM/runtime on them)
bin/dww                      ← dwow-sdk, dwow-accounts, dwow_core,
       ↑                       dwow_native_token_contract (ONLY contract dep)
bin/dwowd                    ← bin/dww (test-only), dwow_core,
                               all contract crates (genesis pre-deployment)
```

Rules:
- `dwow-sdk` SHALL NOT depend on `dwow_core`. It defines the protocol types
  (`Transaction`, `ContractCall`, `AeadEncryptedNote`, `Nullifier`, `Commitment`,
  `AssetId`, `MerkleNode`); the node crate (`dwow_core`) implements the ZK
  prover, the VM, and the runtime that operate ON those types.
- `bin/dww` depends on `dwow_core` for ZK proving (`ZkBinary`, `ProvingKey`,
  `Proof`) and transaction assembly (`TransactionBuilder`) — these are
  wallet-side construction operations that require the concrete ZK machinery.
- The generic prover's **architecture** (witness-binding rules, manifest
  resolution, `CircuitWitnessMap`, `CapabilityProvider` trait) lives in the
  SDK (`src/sdk/src/prover.rs`). Its **concrete implementation** (`ZkBinary::decode`,
  `ProvingKey::build`, `Proof::create`) lives in the wallet — the SDK defines
  the contract; the wallet implements it.
- The wallet SHALL depend on EXACTLY ONE contract crate:
  `dwow_native_token_contract`. This is the bespoke-citizen exception
  (wallet.md §6.4). All other contracts enter through their stored manifest.
  The Deployooor contract is accessed through its `ContractId` (a constant
  in the SDK, not a crate dependency).

### 0.1.2 Import Rules Per Component

| Component | MAY import from | SHALL NOT import from | Rationale |
|---|---|---|---|
| **Wallet core** (`bin/dww`) | dwow-sdk, dwow-accounts, dwow_core, native_token crate | Contract crates (except native_token); contract names as string literals (except genesis ID table, §5.2) | Native token is the one bespoke citizen |
| **AccountManager** (`crates/dwow-accounts`) | dwow-sdk (crypto types only: `SecretKey`, `PublicKey`, `ContractId`) | dwow_core, bin/dww, any contract crate | Pure key derivation — no chain, no wallet, no contracts |
| **dwowd-sdk** (`src/sdk`: `crypto/`, `tx.rs`, `blockchain.rs`, `deploy.rs`, `wasm/`) | dwow-serial, pasta_curves, rand_core | dwow_core, bin/dww, contract crates | Protocol-layer types only — no ZK/VM/runtime |
| **Capability SDK** (`src/sdk`: `capability.rs`, `manifest.rs`, `contract_client.rs`, `prover.rs`) | dwow-sdk (internal), dwow-serial | dwow_core (traits only — no concrete ZK types); native_token crate | Architecture separate from implementation; contract-agnostic |

Additional rules:
- The Capability SDK SHALL NOT import, reference, or special-case
  `native_token`. The two paths (bespoke and generic) meet ONLY in the wallet
  core's shell, which routes by `ContractId`.
- No component SHALL import `bin/dww` from any other component (the wallet is
  the top of the dependency graph; nothing depends on it except test crates).

### 0.1.3 Interface Contracts

The boundary between the wallet core and the Capability SDK SHALL be:
- `wallet_construct(primitives: &[Primitive], required_barbs: &[Barb]) -> Option<TypedCapability>` — the ONE composition function (type-system.md §6.3, ocap.md §2)
- `ManifestContractClient::build(function, params, wallet_state) -> Result<(Vec<u8>, Vec<Vec<u8>>), String>` — generic action dispatch
- `ProverContext::build_proof(cap: &dyn CapabilityProvider, params: &str) -> Result<Vec<Proof>, Error>` — generic proof construction (§6.4.1)
- `CircuitWitnessMap::from_manifest(entries: &[String]) -> CircuitWitnessMap` — witness-binding rule parser

The boundary between the Capability SDK and the wallet's state SHALL be the
`WalletStateProvider` trait (`src/sdk/src/contract_client.rs`) — the ONLY
surface through which generic contract clients read wallet state. Its
methods SHALL be exactly:
- `default_address() -> Result<String, String>` — the wallet's default receiving address
- `held_capabilities_by_asset(asset_id: &str) -> Result<Vec<CapInfo>, String>` — held capability records for an asset (§6.2 selection input)
- `get_merkle_proof(cap_id: &str) -> Result<MerkleProofInfo, String>` — inclusion proof siblings + leaf position for a held capability
- `get_secret() -> Result<String, String>` — the default wallet secret (bs58), resolved at the moment of use (§4)
- `load_zkas_binary(contract_id: &str, namespace: &str, circuit_name: &str) -> Option<Vec<u8>>` — zkas circuit bytes from the wallet's `zkas_binaries` store (§3, §6.4.1 step 3)
- `generate_proof(contract_id: &str, witness_map: &CircuitWitnessMap, zkas_bytes: &[u8], seed: [u8; 32]) -> Result<Vec<u8>, String>` — proof construction via the wallet's concrete ProverImpl (§6.4.1); the SDK owns witness binding, the wallet owns the ZK machinery

A contract client that needs wallet state not expressible through this trait
SHALL NOT be given a side channel — the trait is extended by MoC review, or
the contract's manifest is corrected (§2.2).

The boundary between the wallet core and the AccountManager SHALL be:
- `resolve_key(coords: &KeyCoordinates) -> Result<OwnedSecretKey, Error>` — resolve stored coordinates to secrets at the moment of use (§4)
- `find_owner(cid: &ContractId, instance_seed: &[u8], pubkey: &PublicKey) -> Option<KeyCoordinates>` — discover key ownership at scan time (§2.1)
- `secrets() -> Vec<SecretKey>` — master secrets for trial decryption (Path 1)
- `secrets_for_contract(cid: &ContractId, instance_seed: &[u8]) -> Result<Vec<SecretKey>, Error>` — per-instance derived secrets (Path 1)

The boundary between the wallet core and dwowd-sdk SHALL be:
- All cryptographic primitive types SHALL cross this boundary by their nominal
  newtype, never as `[u8; 32]` or `pallas::Base` (type-system.md §2.2)
- `Transaction`, `ContractCall`, `ContractCallLeaf` — transaction assembly types
- `AeadEncryptedNote`, `Nullifier`, `Commitment`, `AssetId`, `MerkleNode` — protocol types
- `TransactionBuilder`, `DarkForest`, `DarkLeaf` — transaction tree construction

### 0.1.4 Module Map Per Component

**Wallet core** (`bin/dww/src/`):
```
lib.rs              — Dww struct, pure functions, DB shell, initialize_wallet
cap_selection.rs    — barb-cover input selection from held capabilities
scan.rs             — Path 1 (native) + Path 2 (manifest) pure scan functions
dispatch.rs         — CLI command dispatch (shell layer)
fee_builder.rs      — fee attachment and transaction finalization
walletdb.rs         — CapRecord persistence, held_capabilities queries
contract_imports.rs — ContractId lookup table (genesis trust tier, §5.2)
contract_metadata.rs — trimmed registry (native_token + deployooor only)
capability.rs       — wallet capability browser (ocap.md)
manifest_resolver.rs — on-chain manifest queries (RPC-free; wallet IS a full node)
deploy.rs           — contract deployment via Deployooor
rpc_server.rs       — JSON-RPC interface (firewalled to a single justified purpose)
sync_task.rs        — P2P chain sync (GetTip/GetBlocks, same protocol as mining nodes)
```

**Capability SDK** (`src/sdk/src/`):
```
capability.rs        — wallet_construct, Primitive, Barb, TypedCapability
manifest.rs          — ContractManifest, manifest parsing, typed-field resolution
contract_client.rs   — ManifestContractClient, ContractClient trait, WalletStateProvider
prover.rs            — generic prover architecture, witness-binding rules
```

**dwowd-sdk** (`src/sdk/src/` — physically same crate, logically distinct component):
```
crypto/     — nominal primitives (SecretKey, PublicKey, Nullifier, Commitment, AssetId,
              MerkleNode, ContractId, FuncId), AEAD notes, keypair, pedersen, poseidon
tx.rs       — Transaction, ContractCall types
blockchain.rs — block/chain data structures, reward schedule
deploy.rs   — DeployParamsV1, deploy instruction types
wasm/       — WASM host functions
```

**AccountManager** (`crates/dwow-accounts/`):
```
lib.rs      — AccountManager, OwnedSecretKey, KeyCoordinates, MiningRecipient
```

### 0.1.5 Purity Rules (extension of §6.1)

1. **Seed rule**: The `Seed` SHALL be drawn by the shell (dispatch or RPC handler)
   and passed down. No function below the shell SHALL draw from ambient
   randomness (`OsRng`, `thread_rng`, or any system entropy source). This makes
   every construction below the shell byte-deterministic given the Seed.
2. **Shell isolation**: The shell gathers inputs (reads the wallet DB for held
   capabilities and Merkle proofs, draws a fresh `Seed`) and passes them to the
   pure construction function `f`. The shell performs I/O; `f` does not. This
   is the functional-core/imperative-shell discipline (§6.1).
3. **Proving randomness**: The Halo2 proving randomness SHALL also derive from
   `Seed`. The existing `deterministic_zk_enabled()` flag (seeds `StdRng(0)`)
   is a pipeline-level override; in production, the Seed supplies the entropy.

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
3. Capability commitment: `poseidon_hash(7 elements)` — pure
4. Nullifier: `poseidon_hash(2 elements)` — pure
5. Merkle tree: ordered append to `BridgeTree` — deterministic
6. Block iteration: sequential by height — deterministic
7. SQLite inserts: `INSERT OR IGNORE` — idempotent

This section specifies **confirmed** wallet state. The write path (§6) adds a provisional
layer for transactions in flight; the refinement `WalletState = ConfirmedState ⊕
ProvisionalState` (§6.5) preserves this guarantee — provisional state never mutates the
pure function defined here.

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
    Commitment(↓commit, value = reward),
    Nullifier(↓nullify),
    ContractId(↓dispatch = NATIVE_TOKEN),
    FuncId(↓gate = PoWRewardV1),
    AssetId(↓denominate = DRKW),
    MerkleNode(↓prove-inclusion)
)
```

5. Stores the typed composition in `held_capabilities`.

### 2.2 Path 2: Manifest-Driven Capability Construction (All Other Contracts)

There is no "generic" capability. Every capability has a specific type
construction composing specific primitive types. The wallet constructs
these types from the contract's manifest, which declares what capabilities
the contract recognizes and what primitives each capability composes.

For every contract call in every transaction:

1. Scans `call.data` for `AeadEncryptedNote` structures.
2. Attempts decryption with each wallet secret. If decryption succeeds:
   the wallet possesses the primitive names inside that note.
3. Reads the contract's manifest from on-chain storage. The manifest
   declares what capabilities the contract recognizes and what primitive
   types each capability composes (`[[capabilities]]` + `[[actions]]`).
4. Matches the decrypted note against a declared capability in the
   manifest. The manifest tells the wallet what type to construct.
5. Constructs the specific capability type per the manifest's
   declaration — composing the primitives the manifest names.
6. Stores the typed composition in `held_capabilities`.

**Coverage gate**: Step 5 SHALL apply `wallet_construct` — the pure function
that checks whether the composed barbs of the declared primitives cover the
action's required barbs. If coverage fails, the note is dropped. This is the
**coverage gate**: an uncovered composition is not a valid capability type.
The fix is always to correct the contract's manifest declarations, never to
add wallet code ([type-system.md §13](type-system.md)).

The manifest IS the type declaration. Every contract SHALL declare its
capabilities in its manifest. The wallet SHALL NOT need per-contract code
because the manifest provides the type structure. The AEAD decryption
identifies WHICH capability the wallet holds; the manifest identifies
WHAT TYPE that capability is.

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
| `zkas_binaries` | Circuit binaries keyed by (ContractId, namespace, circuit name) | Predicate languages L_{r,s} for the generic prover (§6.4) |

Genesis contracts' circuit binaries are embedded at compile time
([manifest.md](manifest.md), Stage 1); user-deployed contracts' binaries are
extracted from the `DeployV1` payload when the deploy transaction is scanned
and stored in `zkas_binaries`. Held capabilities discovered through Path 2
(§2.2) store their AccountManager key *coordinates* (§0.1), never raw secrets.

The write path adds **provisional** stores (§6.5): a pending-transaction record and a
per-capability spend-state (`Unspent`/`Reserved`/`Spent`). These hold no confirmed
authority — they are reconciled into the tables above when a block is scanned.

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

## 6. The Write Path: Transaction Construction (Exercise)

Sections 1–5 specify the wallet's **read path** — Discover and Hold. This section
specifies the **write path** — Exercise — the formal dual of the scan. Where the scan
*receives* a name (the AEAD input operation, §2) and constructs a capability type, the
write path *exercises* a held capability: it generates the zero-knowledge proof that
inhabits the capability type's predicate language L_{r,s} ([ocap.md §6](ocap.md)),
publishes the nullifier that consumes the name, and emits an authenticated `Transaction`
([type-system.md §8.2](type-system.md)).

The write path SHALL uphold the same discipline the read path does: it is a **pure
function** (§6.1), it selects inputs by **barb coverage** (§6.2), it has exactly **one
bespoke citizen** (§6.4), and — because broadcasting a transaction creates state that is
not yet on the chain — it introduces a formally-delimited **provisional state** (§6.5)
that reconciles against confirmed state without ever mutating it.

### 6.1 Exercise Is a Pure Function

Per the referential-transparency requirement of §1, transaction construction SHALL be a
pure function:

```
Transaction = f(SelectedCapabilities, Action, Params, Secrets, Seed)
```

- **SelectedCapabilities** — the held capabilities being exercised (§6.2), each with its
  primitive names, commitment, and Merkle inclusion proof.
- **Action** — the manifest-declared action being invoked, with its
  `requires`/`consumes`/`produces` and its `proof_circuit` ([manifest.md](manifest.md)).
- **Params** — pure business-logic arguments. The capability being exercised is NEVER in
  the params; the wallet selects it (§6.2).
- **Secrets** — the ν-restricted spending names from the `AccountManager` (§4).
- **Seed** — the explicit randomness name. Every blind, every ephemeral key, and the
  Halo2 proving randomness SHALL be derived from `Seed`. No construction step SHALL draw
  from ambient randomness.

Given identical inputs, `f` SHALL produce a byte-identical transaction. This is the
write-path analogue of §1's guarantee that "AEAD decryption is deterministic for a given
ciphertext + key": by making `Seed` an explicit name rather than ambient authority, the
whole pipeline — capability selection, note encryption, nullifier derivation, ZK proving,
signing — is a reproducible pure computation. The effectful shell that gathers the inputs
(reads the wallet DB for held capabilities and Merkle proofs, draws a fresh `Seed`) SHALL
be separated from `f`; only the shell performs I/O. This is the functional-core /
imperative-shell discipline the scan already follows.

### 6.2 Barb-Cover Input Selection

The wallet SHALL select `SelectedCapabilities` so that the union of their barbs covers the
Action's required barbs, using the same `wallet_construct` composition rule the read path
uses at discovery ([ocap.md §2](ocap.md), [type-system.md §6.1](type-system.md)):

```
covers( ⋃ barbs(SelectedCapabilities), requiredBarbs(Action) ) = true
```

Selection by asset value alone SHALL NOT satisfy this requirement: a capability is
eligible only if its constructed type's barbs cover the action's `requires`. This is the
write-path use of the soundness invariant `walletConstruct_sound` (§7.1) — the read path
proves the invariant when it *constructs* a held type; the write path enforces it when it
*exercises* one. A capability in the `Reserved` state (§6.5) SHALL NOT be selected.

### 6.3 The Construction Pipeline

For an invocation `dwow_wallet contract invoke <contract_id> <action> --params '{...}'`,
`f` SHALL:

1. Resolve the manifest for `<contract_id>` (§5; the manifest is the type declaration).
2. Select held capabilities whose composed barbs cover the action's `requires` (§6.2).
3. Derive all blinds and proving randomness from `Seed` (§6.1).
4. Compute the **nullifier** for each consumed capability and publish every such nullifier
   in `Transaction.nullifiers`. A consumed capability whose nullifier is absent from
   `Transaction.nullifiers` SHALL be a construction error (nullifier completeness, §7.8).
5. Generate the ZK proof(s) that the holder knows the witnesses satisfying L_{r,s}, using
   the action's `proof_circuit` (§6.4).
6. Attach the fee capability (DRKW) per the fee builder; the fee input's nullifier is
   likewise published in `Transaction.nullifiers`.
7. Sign the transaction (Schnorr over `calls` + `proofs`); an unsigned transaction SHALL
   NOT be broadcast.
8. Return the authenticated `Transaction { calls, proofs, signatures, tx_commitment,
   nullifiers }` ([type-system.md §8.2](type-system.md)). Broadcast (§6.5) is the shell's
   responsibility, not `f`'s.

The capability (the note being exercised) is NOT in the params. The params are pure
business-logic arguments. The wallet automatically selects capabilities, generates ZK
proofs, and publishes nullifiers.

### 6.4 One Bespoke Citizen, Write Side

Per §9 (read side) and [type-system.md §13](type-system.md), the write path SHALL have
exactly one bespoke citizen: **NativeToken**, the consensus-critical asset. Native
transfers, burns, and fee payments SHALL be constructed by the hardcoded NativeToken
client. Every other contract — genesis or user-deployed — SHALL be constructed
generically from its manifest by the capability SDK's **generic prover** (§0.1): no
per-contract wallet code, and no compiled-in proof builder, is required
([manifest.md](manifest.md); the wallet is a generic capability engine). Adding a second
bespoke construction path breaks the write path's design exactly as it would break the
scan (§9).

#### 6.4.1 The Generic Prover

Given `(ContractId, action, params)` and the selected capabilities (§6.2), the
capability SDK SHALL construct the proof(s) as follows:

1. Resolve the stored manifest by `ContractId` (§5).
2. Find the manifest action and its function; the function's `proof_circuit`
   names a `[[circuits]]` entry `(name, namespace)`.
3. Load the zkas binary for `(ContractId, namespace, name)` from the
   `zkas_binaries` store (§3) — embedded at compile time for genesis
   contracts, extracted from the `DeployV1` payload at deploy-scan time for
   user-deployed contracts.
4. Decode it (`ZkBinary::decode`), obtaining the circuit's ordered witness
   list (`ZkBinary.witnesses: Vec<VarType>`).
5. Bind every witness slot per the **witness-binding rule** below.
6. Build the proving key (`ProvingKey::build`, cacheable per circuit) and
   create the proof, with all proving randomness derived from `Seed` (§6.1).

**Witness-binding rule.** A zkas binary's witness section is an *ordered,
typed, unnamed* list — heap names exist only in the optional debug section
and SHALL NOT be load-bearing. Binding is therefore ordered and
manifest-declared: the manifest's `[[circuits]]` entry SHALL carry a
`witness_map` — one entry per witness slot, in slot order, each naming its
source ([manifest.md](manifest.md), Typed Capability Fields):

| Source | Meaning | Typical `VarType` |
|--------|---------|-------------------|
| `note:<field>` | Field from the selected capability's decrypted note (`note_schema`) | `Base`, `Uint64` |
| `param:<field>` | Field from the action's `[[parameters]]` | per parameter type |
| `secret` | The capability's spending key, resolved via AccountManager key coordinates (§0.1) | `Base` |
| `merkle_path` | The capability's inclusion proof (`capability_proofs`, §3) | `MerklePath` |
| `leaf_position` | The capability's leaf position | `Uint32` |
| `blind` | A fresh blind derived from `Seed` (§6.1) | `Base`, `Scalar` |
| `tx_commitment`, `tx_nonce` | The transaction binding names | `Base` |

The capability SDK SHALL type-check every binding against the slot's declared
`VarType` and SHALL reject the construction with a typed error barb — never a
fallback — on any arity or type mismatch. This is the write-path dual of the
read path's coverage gate (§2.2): an unbindable circuit is a type error in the
*contract's declarations*. Fix the manifest, not the wallet (§9).

**Public inputs.** The prover SHALL derive the proof's public inputs by
evaluating the witnessed circuit through the existing trace machinery
(`ZkCircuit::enable_trace`, the `constrain_instance` opcode outputs) — the
same evaluation the verifier's metadata call performs node-side. No
per-contract Rust computes public inputs on the generic path.

### 6.5 Provisional State: The In-Between

Broadcasting a transaction creates state that is real to the wallet but not yet on the
chain: the transaction is pending, and the capabilities it consumes are spoken-for but
not yet spent. §1 defines `WalletState` as a pure function of confirmed chain blocks; the
write path refines this **without weakening it**:

```
WalletState   = ConfirmedState ⊕ ProvisionalState
ConfirmedState = f(AccountManager, ChainBlocks)      -- §1, unchanged
```

`ConfirmedState` remains exactly the pure function of §1. `ProvisionalState` is a separate,
monotonically-reconciled layer holding pending transactions and reserved capabilities.
The governing invariant:

> **ProvisionalState SHALL NOT mutate ConfirmedState.** Only scanning a block (§2) promotes
> a provisional fact to a confirmed fact. A pending transaction that confirms collapses
> into `ConfirmedState` (its outputs discovered by scan, its inputs' nullifiers observed);
> a pending transaction that is dropped is discarded, and its reservations are released.

**Capability spend-state lifecycle.** Each held capability SHALL carry a spend-state:

```
Unspent ──broadcast──▶ Reserved ──confirmation──▶ Spent
                          │
                          └────────drop──────────▶ Unspent
```

`Reserved` excludes the capability from selection (§6.2) but is provisional, not a chain
fact — the on-chain consumption evidence is the nullifier (§6.3 step 4), observed only
when the block is scanned. The binary `revoked` marker of the read path is the `Spent`
terminal state *as observed by scan*; the write path adds the `Reserved` in-between so the
wallet does not re-select a capability it has already spoken for.

**Transaction status lifecycle.** Each transaction the wallet builds SHALL carry a status:

```
Built ─▶ Broadcast ─▶ Pending ─▶ Mined ─▶ Confirmed(≥N)
                          │
                          └────────────▶ Dropped / Replaced
```

Transitions are driven by two observers: (a) mempool observation — the node-side
in-between, [mempool.md](mempool.md) — advances `Broadcast → Pending` and detects
`Dropped`; (b) block scan (§2) advances `Pending → Mined → Confirmed` and, on
confirmation, reconciles the consumed capabilities to `Spent` and discovers the produced
ones. A terminal `Dropped` SHALL release the transaction's reservations
(`Reserved → Unspent`).

The stores backing `ProvisionalState` — a pending-transaction record and the capability
spend-state — extend §3's data stores. They are provisional and hold no confirmed
authority; authority still flows only through name possession (§4) realized on-chain.

### 6.6 Formal Properties

The write path's construction is subject to the soundness obligations of §7.8:
`construct_sound` (the built proofs inhabit L_{r,s}), `construct_deterministic` (identical
inputs, including `Seed`, yield a byte-identical transaction), and `nullifier_completeness`
(every consumed capability's nullifier is published). Transaction *authentication* — that
no unverified transaction is ever admitted to the mempool or accepted into a block — is a
consensus invariant specified in [mempool.md](mempool.md) and enforced at both mempool
admission and block acceptance.

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

- `nativeTokenTransfer_constructible`: from `[SecretKey, Commitment, Nullifier, ContractId, FuncId, AssetId, MerkleNode]`
- `daoVote_constructible`: same primitives, different resource
- `tenderBid_constructible`: same primitives, different resource

### 7.6 Failure Case

`walletConstruct_rejects_emptyPrimitives`: If no primitives are provided
and the resource requires non-empty barbs, construction correctly returns
`none`. The wallet does not fabricate capabilities from nothing.

### 7.7 Lean4 Source

`proofs/lean/src/DarkFi/Capability/Wallet.lean` — all theorems above.
Run `lake build` in `proofs/lean/` to type-check.

### 7.8 Write-Path Obligations (Exercise)

The write path (§6) is subject to the following obligations — the exercise-time duals of
§7.1, §7.4, and the nullifier discipline. They are to be discharged in
`proofs/lean/src/DarkFi/Capability/Wallet.lean` (proved, or stated as explicit
future-work in the manner of [type-system.md §11.6](type-system.md)):

- **`construct_sound`** — if `f(SelectedCapabilities, Action, Params, Secrets, Seed)`
  returns a transaction, the proofs it carries inhabit the predicate language L_{r,s} of
  the action's capability type; equivalently, the composed barbs of the selected
  capabilities cover the action's `requiredBarbs` (§6.2). Dual of `walletConstruct_sound`
  (§7.1).
- **`construct_deterministic`** — given identical `(SelectedCapabilities, Action, Params,
  Secrets, Seed)`, `f` returns a byte-identical transaction (§6.1). The write-path
  expression of the wallet's pure-function property; dual of
  `walletConstruct_deterministic` (§7.4).
- **`nullifier_completeness`** — for every capability in `SelectedCapabilities` consumed
  by the transaction, its nullifier appears in `Transaction.nullifiers` (§6.3 step 4).
  This is the property the mempool relies on for double-spend detection
  ([mempool.md](mempool.md)).

## 8. References

- **[Type System Specification](type-system.md)** — Primitive types, behavioral positions, invariants.
- **[O-Cap: Emergent Types](ocap.md)** — Capability type construction from primitives.
- **[Manifest System](manifest.md)** — Contract type declarations.
- **[Mempool: The Pending-Transaction Pool](mempool.md)** — Verified admission, the node-side in-between, observability.
- **[Genesis Contracts](genesis.md)** — Capability primitive layer.
- **[Contract Trust Model](contract-trust-model.md)** — WASM verification, attestations, caveat emptor.

## 9. Design Lesson: Scan Path Discipline

The wallet has exactly **one** bespoke scan path: NativeToken (Path 1). This is
not a precedent. It is an exception justified by NativeToken's unique role as the
consensus-critical asset (block rewards, fee payment, supply audit).

Every other contract — including all genesis contracts — SHALL work through the
**generic Path 2**: AEAD decryption → manifest resolution → barb composition →
`wallet_construct` → `TypedCapability`. Adding a second bespoke scan path breaks
the wallet's design as a generic capability construction engine.
Any pull request adding a second bespoke scan path SHALL be rejected
without review — the fix is always in the contract's type definitions,
never in the wallet.

**Why this matters:** The wallet.md spec states "Adding support for a new contract
requires zero wallet code changes." If a contract seems to need a bespoke scan
path, one of two things is wrong:

1. **The contract's types are under-specified.** If primitives are raw
   `pallas::Base` instead of typed `Nullifier`/`PublicKey`/`ContractId`, the
   generic machinery cannot resolve them. Fix the types, not the wallet.

2. **The barb composition is invalid.** If `wallet_construct` returns `None`,
   the primitives don't cover the required barbs. This is a type error — the
   contract's design is at fault, not the wallet's.

**Example (Box):** Box is "the linear o-cap delegation primitive." But from the
wallet's perspective, Box is simply a specific barb composition:
`{SecretKey, Nullifier, ContractId, FuncId, MerkleNode}` covering
`{spend, nullify, dispatch, gate, proveInclusion}`. The wallet's generic
`wallet_construct` handles this without any Box-specific code. The name "Box"
is documentation — it tells humans the *intent*. The *type* is fully determined
by the primitives.

**Enforcement:** Before merging any change that adds a contract-specific branch
to `bin/dww/src/scan.rs`, verify that the contract cannot work through the
generic Path 2. The fix is always to tighten the
contract's type definitions, not to add wallet code.
