# Contract Metadata

DarkWow contracts carry self-declared on-chain metadata — name, symbol, category, and
description — embedded in the `ix` field of `DeployParamsV1`. The wallet discovers this
metadata during block scanning and surfaces it in contract listings.

All metadata is displayed as `[UNVERIFIED]` in wallet UIs. The metadata slot is designed
to accommodate future attestation-based verification: DAOs, auditors, and
identity-verified professionals can append signed attestations vouching for (or warning
against) a contract. The data container and display layer ship now; verification logic
is deferred.

## Architecture

```
DeployParamsV1 {
    wasm_bincode: Vec<u8>,     // WASM binary
    public_key: PublicKey,     // Deployer's public key
    ix: Vec<u8>,               // ContractMetadata::to_ix_bytes()
}
                    │
                    ▼
          ContractMetadata {
              name: String,
              symbol: Option<String>,
              category: Category,
              description: Option<String>,
              public: bool,
              attestations: Vec<AttestationRef>,  // empty for MVP
          }
                    │
                    ▼
    ┌───────────────────────────────────────┐
    │  Deployooor::DeployV1 (0x00)          │
    │  Stores WASM, derives ContractId,     │
    │  calls __initialize(ix)               │
    └───────────────────────────────────────┘
                    │
                    ▼
    ┌───────────────────────────────────────┐
    │  Wallet scan (scan_block_linear)       │
    │  Detects DeployV1 calls, decodes      │
    │  metadata from ix, persists to DB      │
    └───────────────────────────────────────┘
```

### Why `ix`?

`DeployParamsV1::ix` is part of every deployment payload but was previously unused.
It passes through to the contract's `__initialize` function — contracts that ignore
their init payload (25 of 28) see the metadata bytes as a no-op. This means metadata
requires zero changes to existing contracts and zero extra transactions.

## Types

**Location:** `src/sdk/src/deploy.rs`

### Category

```rust
pub enum Category {
    Token, Stablecoin, DEX, DAO, Gaming, Identity,
    Infrastructure, Finance, Labor, Insurance, Oracle, Bridge, Other,
}
```

### AttestationRef

A pointer to an on-chain attestation. Empty for MVP — populated when attestation
verification is implemented.

```rust
pub struct AttestationRef {
    pub attestation_id: [u8; 32],  // Poseidon hash of the attestation
    pub issuer_pubkey: PublicKey,  // Must have Identity::RegisterIssuerV1
    pub attested_at: u64,          // Block height of attestation creation
}
```

### ContractMetadata

```rust
pub struct ContractMetadata {
    pub name: String,
    pub symbol: Option<String>,
    pub category: Category,
    pub description: Option<String>,
    pub public: bool,              // false = unlisted, hidden from public views
    pub attestations: Vec<AttestationRef>,  // empty for MVP
}

impl ContractMetadata {
    pub fn to_ix_bytes(&self) -> Vec<u8>;
    pub fn from_ix_bytes(bytes: &[u8]) -> Option<Self>;
}
```

## Database Schema

**Location:** `bin/drk/wallet.sql`

### contract_metadata

| Column | Type | Purpose |
|--------|------|---------|
| contract_id | TEXT PK | ContractId (bs58 encoded) |
| name | TEXT | Human-readable name |
| symbol | TEXT | Token symbol (optional) |
| category | TEXT | Category enum variant |
| description | TEXT | Human-readable description (optional) |
| public | INTEGER | 1 = visible, 0 = unlisted |
| deployer_pubkey | TEXT | Deployer's public key (bs58) |
| deploy_height | INTEGER | Block height of deployment |
| attestations_json | TEXT | JSON array of AttestationRef (empty `[]` for MVP) |
| lock_status | TEXT | `"unlocked"` or `"locked"` |

### contract_interactions

Records wallet-initiated contract calls for history display.

| Column | Type | Purpose |
|--------|------|---------|
| contract_id | TEXT | Target contract |
| function_name | TEXT | Function called |
| tx_hash | TEXT | Transaction hash |
| block_height | INTEGER | Block height (null if pending) |
| timestamp | INTEGER | Unix timestamp |

## Current Implementation

| Component | File | Status |
|-----------|------|--------|
| Metadata types + serialization | `src/sdk/src/deploy.rs` | Done |
| DB schema (2 tables + indices) | `bin/drk/wallet.sql` | Done |
| DB CRUD operations | `bin/drk/src/walletdb.rs` | Done |
| Scan detection (DeployV1 → metadata) | `bin/drk/src/rpc.rs` | Done |
| Transaction history recording | `bin/drk/src/rpc.rs` | Done |
| Contract interaction recording | `bin/drk/src/rpc.rs` | Done |
| Unit tests (serialization + DB) | `bin/drk/tests/contract_metadata_tests.rs` | 9 tests |
| Level 1 blockchain test | `bin/dwowd/src/tests/pipeline.rs` | Done |
| Level 2 blockchain test (ZK proofs) | `bin/dwowd/src/tests/heavyweight_pipeline.rs` | Done |
| CLI views (per-category, contract info) | — | Deferred |
| Terminal attestation display | — | Deferred |

### Wallet Flow

1. **Scan**: `scan_block_linear()` detects `Deployooor::DeployV1` (function code `0x00`),
   decodes `DeployParamsV1`, extracts `ContractMetadata` from `ix`, persists to
   `contract_metadata` table. Contracts without metadata get a generic `Contract-XXXX`
   name and `Other` category with `public: false`.

2. **History**: Wallet-relevant transactions are recorded to `transactions_history`
   during scan with the serialized transaction blob.

3. **Interactions**: `broadcast_tx()` records each contract call to
   `contract_interactions` for per-contract history views.

## Future Expansion: Verified Attestations

The `attestations: Vec<AttestationRef>` field is the extension point for moving from
self-declared labels to verified claims. Each existing contract listed below provides
a specific piece of the attestation pipeline. The metadata slot is designed so these
can be composed without changing the on-chain metadata format.

### Attestation Contract

Provides the **CreateAttestationV1 → VerifyClaimV1** pipeline. An attestor creates an
attestation (a commitment to a claim or condition); a claimant proves they satisfy the
predicate without revealing identity. This is the core primitive: when an auditor
attests "contract X is safe," that attestation is a cryptographically verifiable claim
that the wallet can check before removing the `[UNVERIFIED]` marker.

**Location:** `src/contract/attestation/`
**Functions:** CreateAttestationV1, MakeClaimV1, VerifyClaimV1, ConsumeClaimV1, RevokeClaimV1

### Identity Contract

Provides **O-Cap authorization**: prove capabilities without revealing identity. Before
an attestation means anything, the attester must be a registered issuer — someone the
community trusts to vouch for contracts. `RegisterIssuerV1` registers identity-verified
public keys; `IssueCredentialV1` delegates specific capabilities (e.g., "can audit
stablecoins"). The wallet can check that attestations come from issuers with matching
credentials.

**Location:** `src/contract/identity/`
**Functions:** RegisterIssuerV1, IssueCredentialV1, RevokeCredentialV1,
UpdateReputationV1, VerifyCredentialV1

### Insurance Market Contract

Provides **capital-backed attestations**. An underwriter posts a bond to cover a risk
category (e.g., "stablecoin solvency"). The bond creates economic skin-in-the-game:
if the underwriter attests to a contract that later proves faulty, the bond is
slashable. This moves attestations from reputation-only to capital-guaranteed.

**Location:** `src/contract/insurance_market/`
**Functions:** ProposeCoverV1, UnderwriteCoverV1, PurchaseCoverV1, SubmitClaimV1,
ArbitrateClaimV1, WithdrawCoverV1

### Escrow Contract

Provides **HTLC-based attestation escrow**. An attestor locks capital in a hashed
timelock contract conditional on the attestation being valid. If a challenger proves
the attestation is false before the timeout, they claim the escrowed funds. This
enables adversarial attestation markets — anyone can challenge a bad attestation and
profit from exposing fraud.

**Location:** `src/contract/escrow/`
**Functions:** CreateEscrowV1, FundEscrowV1, ClaimEscrowV1, RefundEscrowV1

### DAO-Escrow Contract

Provides **community-governed endowment pools**. A DAO can maintain a treasury that
funds attestation campaigns — paying auditors to review contracts, covering insurance
premiums for widely-used contracts, or voting to endorse specific contracts as
community-verified. The DAO's vote becomes a collective attestation with the
endowment's economic weight behind it.

**Location:** `src/contract/dao_escrow/`
**Functions:** ProposeMembershipV1, VoteOnMembershipV1, ProposeV1, VoteV1,
ExecuteV1, ContributeV1, ClaimRefundV1, MintV1

### Composition Path (Deferred)

```
1. Identity::RegisterIssuerV1     → establishes trusted attesters
2. Identity::IssueCredentialV1    → delegates audit capability to attesters
3. Attestation::CreateAttestationV1 → attester vouches for contract metadata
4. Insurance::UnderwriteCoverV1   → attester posts capital bond (optional)
5. Escrow::CreateEscrowV1         → challenger locks funds for dispute (optional)
6. DAO-Escrow::VoteV1             → community endorses contract (optional)

Wallet verifies: issuer credential + attestation validity + bond coverage
Metadata displayed without [UNVERIFIED] when sufficient attestation weight exists.
```

## Testing

Contract metadata is tested at Level 1 (unit + lightweight blockchain) and
Level 2 (heavyweight ZK proofs). See [Testing Overview](../dev/testing/overview.md)
for the four-level taxonomy and command reference.

## Security Properties

- **No identity leakage**: Metadata is self-declared and public by design. No private
  data is stored.
- **O-Cap alignment**: Future attestations use object-capability authorization —
  attesters prove capabilities without revealing identity.
- **Blast radius containment**: A malicious metadata payload (garbage bytes) produces
  a `ContractMetadataRecord` with `Other` category and `public: false` — no crash,
  no panic, no exploit surface.
- **Deterministic**: Metadata is stored in the block and verified by every node
  identically. Same block = same metadata. Uncle Merkle guarantees consistency.

## See Also

- [Deployooor Contract](../contract/deployooor.md) — Deployment system
- [Architecture Overview](overview.md)
- [Testing: Level 1 (Lightweight)](../dev/testing/level-1-lightweight.md)
- [Testing: Level 2 (Heavyweight)](../dev/testing/level-2-heavyweight.md)
- [O-Cap Authorization](ocap.md)
- [Identity Contract](identity.md)
