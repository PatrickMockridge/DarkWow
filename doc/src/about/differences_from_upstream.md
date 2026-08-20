# What's Different from Upstream DarkFi

DarkWow began as a fork of DarkFi. Both projects share the core zkVM, ZKAS
circuit language, P2P networking stack, and WASM contract runtime. They
diverge on architecture. See the [Philosophy](../philosophy/philosophy.md)
page for the intellectual context behind the divergence.

## Comparison Table

| Feature | Upstream (DarkFi) | DarkWow (this fork) |
|---------|-------------------|---------------------|
| Governance model | Monolithic DAO (ACL-based, token-weighted) | Composable O-Cap primitives (Box, Purse, Identity, Oracle, Attestation, MultiSig) — modular self-governance, no central DAO |
| Privacy model | ACL (reveals identity) | ZK predicates (boolean only) |
| Token distribution | Contributor allocations | Pure PoW mining |
| Consensus | Overlay-DAG | Uncle Merkle |
| Opcodes | Basic set | + LessThanOrEqual, IsNotEqual, BaseDiv (Lean-verified) |
| Supply model | Premine + emission | Zero premine, continuous exponential decay |
| Key model | Daemon-held (RPC delegation) | Sovereign (AccountManager — never delegated, wallet is pure function) |
| State model | Speculative (overlay/diff) | Deterministic (final at commit) |

## Genesis Contracts

Upstream deploys 3 contracts at genesis: `MONEY_CONTRACT_ID`,
`DAO_CONTRACT_ID`, and `DEPLOYOOOR_CONTRACT_ID`. The Money contract
handles 8 functions (FeeV1 through BurnV1) in a single enum across 7
database trees and 6 ZK circuits. The DAO contract encodes 5 functions
(Mint, Propose, Vote, Exec, AuthMoneyTransfer) with 18 parameters fused
into a single `DaoBulla` commitment, 6 ACL keypairs embedded, and
token-weighted voting enforced at the ZK circuit level:
`proposer_limit <= total_funds`, `quorum <= all_vote_value`.

DarkWow deploys 9 contracts at genesis. The single Money contract is
split into [NativeToken](../contract/native_token.md) (consensus-critical:
block rewards, fees, supply audit) and [PromissoryNote](../contract/promissory_note.md)
(DeFi token operations: transfer, mint, burn, freeze). The monolithic
DAO is decomposed into 6 composable O-Cap primitives:

| Upstream DAO Concern | DarkWow O-Cap Primitive |
|---|---|
| Identity (`notes_public_key`) | [Identity](../contract/identity.md) |
| Data feeds and reporting | [Oracle](../contract/oracle.md) |
| Proposal commitment verification | [Attestation](../contract/attestation.md) |
| Treasury with spend_hook | [Purse](../contract/purse.md) |
| Spend restrictions | [Box](../contract/box.md) |
| Exec key authorization | [MultiSig](../contract/multisig.md) |

Upstream has zero composable governance primitives — no identity, oracle,
attestation, purse, box, or multisig contract exists. DarkWow's primitives
compose into arbitrary governance structures, opt-in rather than inherited.

DarkWow explicitly distances itself from upstream founders' direct
political affiliations, including links to the YPG. DarkWow's interest in
Ocalan's thought is limited to a formal structural mapping between his five
principles and cryptographic protocol architecture. DarkWow does not endorse
any specific political movement, militia, or party.

## Material Type System

The architectural differences are encoded materially in the type system.
DarkWow implements a ρ-calculus-derived type system where every primitive
type is a newtype wrapper around a Pallas base field element. This
is not cosmetic — it is the material encoding of capability semantics.

### From Raw Bytes to Typed Wrappers

Upstream uses raw `[u8; 32]` bytes for cryptographic identifiers
(nullifiers, coin commitments, contract IDs, token IDs). DarkWow
wraps every such identifier in a newtype with declared barbs
(observable actions):

| Primitive Type | Wraps | Barb | Prevents |
|---------------|-------|------|----------|
| `Nullifier` | `pallas::Base` | ↓nullify | Confusion with CoinCommitment; zero-as-nullifier injection |
| `CoinCommitment` | `pallas::Base` | ↓commit | Confusion with Nullifier; non-canonical field elements |
| `ContractId` | `pallas::Base` | ↓dispatch | Confusion with AssetId, FuncId; unsigned deployment identity |
| `AssetId` | `pallas::Base` | ↓denominate | Confusion with ContractId; untyped asset tracking |
| `FuncId` | `pallas::Base` | ↓gate | Confusion with ContractId; untyped function dispatch |
| `PublicKey` | `pallas::Point` | ↓verify | (x,y) pair fragmentation; identity point injection |
| `SecretKey` | `pallas::Base` | ↓spend, ↓derive | Confusion with Nullifier; raw key material exposure |

### The Three Layers of Type Fracture

During the type system hardening (July 2026), model structs across
the contract perimeter were converted from type aliases and raw bytes
to newtype wrappers. The Rust compiler enforced the new boundaries
immediately on the host target. The WASM target revealed three
successive layers of latent type fractures:

1. **Entrypoint construction sites** (48 errors across bridge,
   dao_escrow, identity): Contract entrypoints that constructed model
   structs using old raw types — `recipient_pub_x/y` pairs instead of
   `PublicKey`, `[u8; 32]` instead of `DaoEscrowBulla`.

2. **Client builders** (19 errors across dao_escrow, subscription,
   lottery, stablecoin): Builder code that used bare
   `pallas::Base::zero()` as sentinels for what should have been
   typed identifiers — `DaoEscrowBulla`, `ClaimId`, `ProposalId`,
   `SubscriptionId`.

3. **Cross-contract identifiers** (7 errors across stablecoin,
   subscription): Struct fields where `ContractId`, `PublicKey`,
   and `Nullifier` wrappers were inconsistently applied.

Each error site is a place where the old type system could not
express what the contract needed — a type-safe identifier, a
capability semantic, a barb. The fix is not merely mechanical. It
is the physical encoding of the architectural decision to fork.

### Why This Matters for the Wallet

Every typed wrapper feeds back to the wallet's expressiveness. The
wallet is a pure function of its inputs: `WalletState = f(AccountManager,
ChainBlocks)`. When a contract uses typed wrappers, the wallet can:

- Classify capabilities by their discriminants (Path 2 manifest
  resolution — `capability_discriminant`)
- Select coins by token type (AssetId-filtered queries)
- Verify barb coverage (does this capability have ↓spend?)
- Reconstruct coin commitments deterministically (Poseidon hash
  with typed inputs)
- Match nullifiers against held secrets (Nullifier zero-rejection
  prevents false matches)

Every raw `[u8; 32]` that should be a typed wrapper is a capability
that the wallet loses — it cannot classify, cannot verify, cannot
spend with confidence.

### The Contracts That Required Forking

The contracts where the most type fractures were found —
dao_escrow, subscription, identity, bridge — are precisely the
ones that required forking from upstream. They needed type-safe
identifiers (`DaoEscrowBulla`, `ClaimId`, `ProposalId`,
`SubscriptionId`, `CapabilityId`, `ReputationId`) that the raw-byte
system could not provide. These are the foundational governance
and O-Cap primitives — the material infrastructure of self-governance.
The type system encodes that infrastructure at the lowest level of
the codebase.

## What's Inherited (From Upstream)

| Component | Description |
|-----------|-------------|
| **zkVM** | ZK virtual machine for proof generation and verification |
| **ZKAS** | Circuit language and compiler |
| **P2P stack** | Peer discovery, session management, protocol negotiation (daemon only; wallet has its own lightweight P2P client — see [Wallet vs Daemon](../arch/wallet-vs-daemon.md)) |
| **WASM runtime** | In-node WASM execution for smart contracts |
| **Halo2** | Proof system backend (Poseidon/Pallas) — vendored and pinned at a fixed revision |

## Design Changes (This Fork)

### 1. Governance — Composable O-Cap Primitives Instead of a Monolithic DAO

Upstream's architecture has a single monolithic governance DAO. Token
holders vote on operations including native token minting — the same
token that pays block rewards and fees.

DarkWow replaces the monolithic DAO with six composable, genesis-deployed
O-Cap primitives: Box (capability delegation), Purse (fungible container),
Identity (credentials), Oracle (data feeds), Attestation (trust verification),
MultiSig (private threshold voting). These compose — a DAO treasury is a
Purse secured by MultiSig. A membership credential is an Identity stored
in a Box. Governance is not a contract; it is the interaction between
primitives, configured per-user.

### 2. Privacy — ZK Predicates Instead of ACLs

Upstream uses ACL-based voting where participants reveal their public key
and token balance to prove eligibility. DarkWow uses ZK predicates: a voter
proves they meet a condition without revealing their public key, exact
balance, or any identifying information. The verifier learns only the
boolean result.

### 3. Token Distribution — Pure PoW, No Premine

Upstream's launch included token distributions to early contributors,
investors, and SAFT participants. DarkWow has zero premine. Every token
in circulation was mined. The chain starts when the first miner finds
a block, not when insiders unlock.

### 4. Consensus — Uncle Merkle Instead of Overlay/DAG

Upstream uses an overlay-diff architecture: a DAG of events where blocks
are verified speculatively against an in-memory overlay. DarkWow uses
Uncle Merkle consensus: deterministic, stateless verification with no
overlays, no diffs, no rollbacks. Competing blocks earn partial rewards
via uncle inclusion. State is final at commit.

### 5. Sovereign Keys, Deterministic Wallet

Upstream's wallet delegates key material to the `darkfid` daemon via RPC.
DarkWow's wallet holds keys in the `AccountManager` — the daemon never
sees them. Combined with Uncle Merkle forward-only consensus, the wallet
is a pure mathematical function: `WalletState = f(AccountManager, ChainBlocks)`.
Same keys + same chain = identical wallet state, every time.

### 6. ZK Opcodes — Built and Formally Verified

Upstream's zkVM has no `LessThanOrEqual`, `IsNotEqual`, or `BaseDiv`
opcodes. These were built on this fork and formally proven sound in
Lean4 with machine-checkable proofs in `proofs/lean/`.

### 7. P2P Networking — Three-Tier Feature Gate

Upstream has a monolithic P2P stack with hard seed dependency. DarkWow
uses a three-tier feature gate (`net-wallet ⊂ net-node ⊂ net-full`) that
separates essential blockchain infrastructure from optional protocol
extensions. Blockchain nodes operate without seed dependency —
bootstrap via configured peers and PEX gossip.

### 8. Nullifier Storage — Deterministic Sled Markers Instead of Per-Contract SMT

Upstream stores nullifiers inside each contract's Sparse Merkle Tree (SMT),
requiring per-contract Merkle proofs for double-spend checks and coupling
nullifier state to WASM contract execution. The SMT provides Merkle-proof
verifiability of nullifier inclusion — useful for light clients but
unnecessary for replay protection. Every nullifier check requires Poseidon
hashing across the full SMT tree depth (32 levels).

DarkWow stores nullifiers as direct sled key-value markers using
`db_set`/`db_contains_key` — O(1) single sled lookup instead of O(depth)
SMT traversal. Nullifiers are not Merkle-provable in this scheme, but they
do not need to be: nullifiers are already public (emitted in block nullifier
sets), so a Merkle proof adds no privacy benefit. The SMT and `db_set` share
the same underlying sled tree but use disjoint key namespaces (SMT writes
to `BigUint` path keys, application code writes to 32-byte nullifier keys),
meaning an SMT read can never observe a `db_set` write. Every contract that
used the SMT for nullifier reads while writing nullifiers via `db_set` had
a structurally inoperative replay check — a defect inherited from upstream
and discovered during HAZOP analysis.

The backend also introduces an internal presence sentinel (`[0x01]` for
present-but-empty, `[0x00]` for genuinely absent) to distinguish "key was
never written" from "key was written with an empty marker value" — an
ambiguity inherited from upstream where `db_remove` uses `&[]` as a
deletion tombstone. Without this fix, `db_set(key, &[])` — the pattern
every contract used for nullifier markers — was invisible to both
`db_contains_key` and `db_get`. These two issues compounded to silently
bypass nullifier replay protection in 13 contracts at the storage layer;
either would have required a hard fork to fix post-launch.

See [Consensus — Storage Backend Determinism](../arch/consensus/consensus.md)
for the full specification and principles established.

## See Also

- [Consensus Details](../arch/consensus/consensus.md)
- [Opcodes and Formal Verification](../arch/zk/opcodes.md)
- [Privacy Architecture (O-Cap)](../arch/ocap.md)
- [MultiSig — Private Threshold Voting](../contract/multisig.md)
- [Box — Capability Delegation](../contract/box.md)
- [Purse — Fungible Container](../contract/purse.md)
