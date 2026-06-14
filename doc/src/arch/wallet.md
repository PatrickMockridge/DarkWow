# Wallet Architecture

## Design Philosophy

The DarkWow wallet follows the same architecture as Bitcoin Core
(bitcoind + bitcoin-cli): **process separation between daemon and wallet**.

- `dwowd` is the full-node daemon. It syncs the chain via P2P, stores blocks
  locally in sled, validates consensus, and exposes a JSON-RPC interface.
- `dwow_wallet` is a command-line wallet. It connects to `dwowd` via localhost
  RPC for block data and transaction broadcast. Everything else — key
  management, scanning, balance queries, transaction building — is pure
  local computation over SQLite and sled.

The wallet never syncs the chain itself. It reads blocks from the already-synced
chain that `dwowd` maintains on the same machine. This is **not** SPV or light
client — the full chain is local. It is process separation: the daemon handles
chain sync and consensus, the wallet handles application logic.

## Two Things the Wallet Tracks

1. **Native Token** — the consensus asset. Used for fee payments and coinbase
   rewards. The only asset that requires Merkle proof tracking for fee spending.
   Stored in the `coins` table.

2. **Capabilities** — everything else. Every contract output that the wallet
   discovers via AEAD decryption. PN, BB, escrow, auction, all 25+ contracts.
   Stored in the `capabilities` table.

There is no third category. No contract gets its own table, its own tree,
or its own dedicated methods. Native Token is the sole special citizen —
and only because it is the consensus asset needed for fee payment.

## Merkle Proofs and Nullifiers Are Universal

Every DarkWow transaction — transfer, burn, mint, redeem, swap, deploy —
requires Merkle proofs and nullifiers. This is the protocol, not a native-token
special case. BurnV1 proves coin inclusion via Merkle root and publishes a
nullifier. TransferV1 does both: burns with Merkle proof + nullifier, creates
new blind outputs. The wallet caches Merkle trees for scan efficiency, but
Merkle proofs are not a "native token feature" — they are how every
transaction authorizes itself.

## Capability-First Scanning

The wallet discovers everything — native token and capabilities alike — by
attempting AEAD decryption on every output in every transaction. The AEAD
authentication tag IS the discriminator. Successful decryption proves the
output belongs to this wallet, regardless of which contract produced it.

All contracts use the same AEAD encryption primitive (ChaCha20Poly1305 +
Sapling DH). The wallet's byte-level scanner handles any contract that
produces `AeadEncryptedNote` outputs — **new contracts work without
wallet code changes**.

### Scan Architecture

```
for each transaction in block:
    # Path 1: Native Token coinbase (consensus reward)
    if coinbase present:
        try AEAD decrypt with wallet secrets
        if success → store in coins table with Merkle proof

    # Path 2: Generic AEAD (everything else — PN, BB, all 25+ contracts)
    for each contract call:
        scan call.data byte-by-byte for AeadEncryptedNote patterns
        for each note found:
            for each wallet secret:
                if AEAD decrypt succeeds:
                    store in capabilities table
```

Path 2 is a single generic function. No contract ID lookup. No per-contract
branch. No "optimized handler" dispatch. The byte-level AEAD scan handles
every contract uniformly.

## Object Capabilities and the Wallet

The wallet is an **object-capability (o-cap) native architecture** — it derives
capabilities from on-chain state rather than authenticating identities against
access control lists. This is Mark Miller's model of authorization: a capability
is an **unforgeable reference** that combines **designation** (what the object is)
with **authority** (what the holder can do with it).

### The Four-Part Capability Pattern

Every output the wallet discovers follows the same cryptographic pattern:

| Component | Function | Wallet Role |
|-----------|----------|-------------|
| **Commitment** | `H(secret, params)` | Stored on-chain. The wallet decrypts it to discover ownership. |
| **Nullifier** | `H(secret, commitment)` | Published when exercising the capability. The wallet detects spends by scanning for nullifiers. |
| **Proof** | ZK proof of secret knowledge | Built by the wallet when the user wants to exercise a capability. |
| **Revocation** | Issuer invalidates | Optional. Checked during capability resolution. |

The lifecycle: **Discover** (AEAD decrypt) → **Hold** (store in DB) → **Exercise**
(build ZK proof + publish nullifier) → **Detect Spend** (scan nullifier set).

Capabilities are **consumable** (destroyed on use via nullifier) or
**non-consumable** (persist, like mint authorities or receipt coins).
A consumable capability can be exercised exactly once — the nullifier
provides cryptographic replay protection.

### Capability Resolution

The wallet resolves capabilities by scanning on-chain state and matching
against the user's secrets. This is a pure local computation — no network
requests, no identity queries:

1. **Collect user's public keys** from the wallet database
2. **Scan block outputs** via generic AEAD decryption — every decrypted output
   is a capability (regardless of which contract produced it)
3. **Match against on-chain state** — for native token, check Merkle proofs;
   for everything else, the AEAD tag IS the proof of ownership
4. **Derive available actions** — which contract functions the user can call
   given their current capabilities

The wallet never asks "who is this user?" It asks "what can these secrets do?"
The secrets are the capability. The proof IS the authority. See
[O-Cap & Composable Privacy](ocap.md) for the full mathematical foundation
including the Authorization Inversion Theorem.

### ZK + Conditional Circuits: The Mathematical Foundation

The Authorization Inversion Theorem ([ocap.md](ocap.md)) proves that
capability-based authorization is mathematically equivalent to having a ZK
proof system for the predicate defined by the capability:

**A'(π, r, s) = ∃ w : P_{r,s}(w) = 1**

Where P_{r,s} is a predicate over the witness w, and the proof π reveals only
the predicate result — not w, not the holder's identity. The witness is known
only to the prover and is cryptographically unlinkable to any principal.

DarkWow's ZK circuits implement this directly. A transfer proves: "I know a
secret whose commitment is in the Merkle tree, and the nullifier hasn't been
spent." The verifier learns only that the proof is valid — not which coin,
not the value, not the token type, not the holder. This is privacy-by-construction:
the circuit's public inputs are exactly what must be revealed for consensus
validation; everything else stays in the witness.

Conditional circuits (`LessThanOrEqual`, `IsNotEqual`) extend this to
predicate evaluation: "I have balance ≥ threshold" or "my credential has not
expired." The return value is a single bit — authorized or not. The attribute
values never leave the witness. For the full opcode-level specification, see
[zkVM Primitives](arch/zk/zkvm_primitives.md).

### Why O-Caps, Not ACLs

Access control lists answer "WHO has access to X?" O-caps answer "Can you
PROVE you have access to X?" The difference is fundamental:

- ACLs require storing and authenticating identities — a privacy catastrophe
  on a public ledger
- O-caps require only that the holder can produce a valid proof against a
  public commitment — identity is never involved

The wallet is the user's capability browser. It shows what the user can DO,
not who the user IS. Every contract interaction — transfer, redeem, vote,
bid, stake — is a ZK predicate evaluation. The wallet builds the proof.
The system verifies it. Identity is never on the chain.

## Async is for Chain Sync Only

The only async operations in the wallet are RPC calls to `dwowd`:
- `scan_blocks()` — fetches blocks in a loop
- `broadcast_tx()` — submits transactions to the mempool
- `DwowdRpcClient` methods — all RPC

Everything else is synchronous: keygen, balance, transfer, scanning logic,
SQLite/sled reads and writes.

## Transaction Building

The wallet builds transactions for specific contract functions. These are
legitimate contract-specific operations — they call named opcodes:

| Method | Contract | Opcode | What It Does |
|--------|----------|--------|--------------|
| `transfer()` | PN | TransferV1 (0x04) | Atomic burn + blind output |
| `redeem()` | PN | RedeemV1 (0x01) | Close lifecycle, create receipt |
| `burn()` | PN | BurnV1 (0x03) | Destroy value, publish nullifier |
| `create_token()` | PN | TokenMintV1 (0x00) | Create token type |
| `mint_tokens()` | PN | MintV1 (0x02) | Mint coins with backing proof |
| `init_swap()` / `join_swap()` | PN | OtcSwapV1 (0x05) | Atomic peer-to-peer exchange |

Transaction builders follow a common pattern:
1. Look up inputs from wallet DB
2. Fetch secrets, Merkle proof, leaf position, blinds
3. Load ZK binary, generate proofs
4. Encode params, wrap in ContractCall
5. Attach Native Token fee
6. Return signed Transaction

## Contract Discovery and Interaction

The wallet discovers contract interfaces through **contract manifests** — TOML
documents that describe functions, capability types, actions, state trees, and
ZK circuits. Manifests enable any wallet to interact with any contract without
hardcoded Rust knowledge, the same way Ethereum's JSON ABI makes contract
interfaces usable without decompiling bytecode.

### Manifest Lifecycle

1. **Authoring**: Each contract has a `manifest.toml` in its source directory.
   29 manifests exist across the DarkWow contract ecosystem.

2. **Deployment**: The deployer passes the manifest via `--manifest` flag.
   The manifest is TOML-serialized, prefixed with magic byte `0x4D`, and placed
   in `DeployParamsV1::ix`.

3. **Scanning**: During block scan, the wallet detects the `0x4D` prefix,
   parses the TOML via `ContractManifest::from_toml()`, and stores it in the
   `contract_metadata` table.

4. **Resolution**: `ManifestResolver` reads the stored manifest and answers
   queries: function lookup by name or opcode, capability lookup by name or
   discriminant, action requirements, parameter schemas.

5. **CLI**: `dwow_wallet contract show <cid>` prints the full interface.
   `dwow_wallet contract invoke <cid> <fn> --params <json>` validates
   parameters against the manifest's schema before building the transaction.

For genesis contracts, the wallet has hardcoded handling for Native Token and
Deployooor. Promissory Note's manifest is the primary one used for dynamic
capability discovery — the wallet resolves token capabilities, transfer
requirements, and redemption rules from its manifest.

See [Contract Manifest](manifest.md) for the full specification, format, and
implementation status.

## Data Stores

| Store | Type | Contents |
|-------|------|----------|
| `cache` | sled | Block pointers, Merkle tree checkpoints, nullifier SMT, scanned block tracker |
| `wallet` | SQLite | Native token coins, capabilities, secrets, addresses, transaction history, contract registry |

## Database Schema

| Table | Purpose |
|-------|---------|
| `addresses` | User keypairs (public + secret) |
| `coins` | Native token outputs with Merkle proofs (fee spending) |
| `capabilities` | **Universal capability store** — every AEAD-decrypted output from every contract |
| `secrets` | Trial decryption secrets (all contracts) |
| `transactions_history` | Sent transactions with status |
| `contract_registry` | Known contract name → contract_id mappings |
| `contract_metadata` | On-chain metadata (name, symbol, deployer) |
| `contract_interactions` | Record of contract function calls |
| `scanned_blocks` | Last scanned block height |
| `deploy_authorities` | Deploy keys for user-deployed contracts |

## CLI

```
dwow_wallet keygen                              Generate new keypair
dwow_wallet balance                             Show balances
dwow_wallet address                             Show default address
dwow_wallet addresses                           List all addresses
dwow_wallet transfer <amt> <token> <rcpt>       Send funds via Promissory Note
dwow_wallet scan                                Scan blockchain for wallet outputs
dwow_wallet broadcast                           Broadcast a transaction
dwow_wallet position                            Show capabilities and available actions

# Contract deployment and discovery (manifest-based)
dwow_wallet contract deploy <auth> <wasm>       Deploy a WASM contract
    --manifest manifest.toml                    Attach a contract manifest
dwow_wallet contract show <contract_id>         Display contract interface from manifest
dwow_wallet contract invoke <id> <fn>           Call a contract function
    --params params.json                        With parameter validation
dwow_wallet contract register <name> <id>       Register contract name→ID mapping
```
