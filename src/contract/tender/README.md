# Tender Contract

A privacy-preserving sealed-bid tendering system that integrates with Identity for O-Cap authorization and Labor Market for job execution.

## O-Cap Authorization

This contract supports **O-Cap (Object Capability)** authorization, enabling **private qualification** where participants prove capabilities without revealing identity.

### Key Innovation

Instead of revealing company identity, past performance, or detailed credentials:

- **Workers** prove: "I am a qualified contractor" (capability)
- **Requesters** define: "I require specific capabilities for this tender"

**Nothing else is revealed.**

## The Elegant Weave: Complete O-Cap Pipeline

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    COMPLETE O-CAP PIPELINE                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  IDENTITY CONTRACT                                                           │
│       │                                                                      │
│       ├── RegisterCapability("qualified_contractor")                        │
│       ├── IssueCapability(alice, "qualified_contractor")                     │
│       └── CreateClaimDAG("senior_engineer") → DAG competency                │
│              │                                                               │
│              ▼                                                               │
│  TENDER CONTRACT                                                             │
│       │                                                                      │
│       ├── CreateTenderWithCapability(                                       │
│       │   required_capability: "qualified_contractor",                     │
│       │   required_dag_id: Some("senior_engineer")                          │
│       │ )                                                                  │
│       │                                                                      │
│       └── SubmitBidWithCapability(                                          │
│           capability_proof: ZK(VerifyCapability("qualified_contractor"))     │
│           dag_proof: ZK(CreateClaimDAG("senior_engineer"))                 │
│           )                                                                 │
│              │                                                               │
│              ▼                                                               │
│  LABOR MARKET CONTRACT                                                       │
│       │                                                                      │
│       └── AcceptJobWithCapability(                                          │
│           capability_proof: ZK(VerifyCapability("qualified_contractor"))     │
│           )                                                                 │
│              │                                                               │
│              ▼                                                               │
│  INSURANCE MARKET CONTRACT                                                  │
│       │                                                                      │
│       ├── UnderwriteWithCapabilityV1(                                      │
│       │   proof: ZK(VerifyCapability("underwriter_capability"))            │
│       │ )                                                                  │
│       │                                                                      │
│       └── PurchaseCoverageWithCapabilityV1(                                 │
│           proof: ZK(VerifyCapability("low_risk_profile"))                  │
│           )                                                                 │
│                                                                              │
│  RESULT: Alice proved → qualified_contractor → senior_engineer DAG          │
│          WITHOUT revealing: identity, employer, exact credentials           │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Function Reference

### Standard Functions (0x00-0x06)

| Function | Opcode | Description |
|----------|--------|-------------|
| CreateTenderV1 | 0x00 | Requester creates a new tender (references attestation) |
| SubmitBidV1 | 0x01 | Worker submits a sealed bid with claim_id |
| RevealBidV1 | 0x02 | Worker reveals their bid amount |
| CloseTenderV1 | 0x03 | Requester closes bidding, starts reveal period |
| SelectWinnerV1 | 0x04 | Requester selects winning bid |
| CancelTenderV1 | 0x05 | Requester cancels tender |
| RejectBidV1 | 0x06 | Requester rejects a revealed bid |

### O-Cap Enabled Functions (0x07-0x08)

| Function | Opcode | Description |
|----------|--------|-------------|
| CreateTenderWithCapabilityV1 | 0x07 | Create tender requiring specific capability |
| SubmitBidWithCapabilityV1 | 0x08 | Submit bid with capability proof |

## State Machines

### Tender State Machine

```
Created ──[SubmitBid]──> Bidding ──[Close]──> Revealed ──[Select]──> Awarded
                                               │
                                               └──[Cancel]──> Cancelled
```

### Bid State Machine

```
Sealed ──[Reveal]──> Revealed ──[Accept]──> Accepted
  │                        │
  └──[Timeout]──> Expired  └──[Reject]──> Rejected
```

## O-Cap Integration

### How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│              TENDER + IDENTITY O-CAP INTEGRATION                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. ISSUER (e.g., professional association)                    │
│     │                                                             │
│     │ Issues capability: "qualified_contractor"                   │
│     │   { competencies: [audit, security, code_review], ... }  │
│     │───────────────────────────────► Alice                      │
│                                                                   │
│  2. Alice registers capability:                                  │
│     Capability: "qualified_contractor"                           │
│     Requirement: credential.competencies.contains(audit)        │
│     │───────────────────────────────► Identity Contract          │
│                                                                   │
│  3. Requester creates tender with capability requirement:        │
│     │                                                             │
│     │ CreateTenderWithCapability(                               │
│     │   required_capability: "qualified_contractor",            │
│     │   required_dag_id: Some("senior_engineer")                │
│     │ )───────────────────────────────► Tender Contract          │
│                                                                   │
│  4. Alice SUBMITS BID with capability proof:                     │
│     │                                                             │
│     │ SubmitBidWithCapability(                                  │
│     │   capability_proof: ZK(VerifyCapability("qualified_...")) │
│     │   dag_proof: ZK(CreateClaimDAG("senior_engineer"))        │
│     │ )───────────────────────────────► Tender Contract          │
│                                                                   │
│  5. Tender contract VERIFIES (via Identity):                    │
│     │                                                             │
│     │ verify_capability(                                          │
│     │   capability_id: "qualified_contractor",                  │
│     │   proof: Alice's_proof                                    │
│     │ )─────────────────────────────────────────────────────►    │
│     │                                                             │
│     │ Result: ✓ Alice can bid                                    │
│     │         ✗ Alice's identity NOT revealed                     │
│     │         ✗ Alice's competencies NOT revealed (only >= required)│
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Tender with Capability Requirements

```rust
// Create a tender requiring specific capabilities
let (params, tender_id) = CreateTenderWithCapabilityV1Builder::new(
    requester_pubkey,
    "Security Audit for DeFi Protocol",
    spec_hash,
    attestation_id,
    1000,   // min_bid
    50000,  // max_bid
    bid_deadline,
    reveal_deadline,
    delivery_deadline,
)
.required_capability(Some(qualified_contractor_cap_id))
.required_dag_id(Some(senior_engineer_dag_id))
.build();

// Worker submits bid with capability proof
let (params, bid_id) = SubmitBidWithCapabilityV1Builder::new(
    tender_id,
    worker_pubkey,
    5000,   // bid_amount
)
.capability_proof(worker_capability_proof)
.capability_predicate_result(pallas::Base::one())
.build();
```

## Integration with Other Contracts

| Contract | Integration |
|----------|-------------|
| Identity | O-Cap authorization via capabilities and DAGs |
| Labor Market | Winner automatically creates a job |
| Attestation | Legacy competency verification (phase 0) |

## Data Structures

### Tender (Extended with O-Cap)

```rust
pub struct Tender {
    // ... existing fields ...
    /// Required capability ID for bidders (None = any bidder via attestation)
    pub required_capability: Option<[u8; 32]>,
    /// Required DAG ID for multi-path qualification (None = no DAG requirement)
    pub required_dag_id: Option<[u8; 32]>,
}
```

## O-Cap Error Types

| Error | Code | Description |
|-------|------|-------------|
| `CapabilityRequired` | 28 | Operation requires a capability that was not provided |
| `CapabilityNotMet` | 29 | Capability proof does not satisfy the tender's requirement |
| `InvalidCapability` | 30 | The capability proof is malformed or invalid |
| `DAGRequirementNotMet` | 31 | DAG proof does not satisfy the requirement |

## ZK Circuits

| Circuit | Purpose |
|---------|---------|
| `create_tender_v1.zk` | Proves requester knows secret key |
| `submit_bid_v1.zk` | Proves bid with attestation claim |
| `reveal_bid_v1.zk` | Reveals sealed bid amount |
| `select_winner_v1.zk` | Proves winner selection is valid |
| `submit_bid_with_capability_v1.zk` | Proves bid with capability (NEW) |

## File Structure

```
src/contract/tender/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Function enum, constants (0x07-0x08 added)
│   ├── error.rs                 # Error types (O-Cap errors added)
│   ├── entrypoint.rs            # exec/apply dispatch
│   ├── model/
│   │   └── mod.rs               # Data models + O-Cap params
│   └── client/
│       └── mod.rs               # Client builders
└── proof/
    ├── create_tender_v1.zk
    ├── submit_bid_v1.zk
    ├── reveal_bid_v1.zk
    ├── select_winner_v1.zk
    └── submit_bid_with_capability_v1.zk  # NEW
```

## Building

```bash
# Build WASM
cargo build --target wasm32-unknown-unknown --release -p darkfi_tender_contract

# Run tests
cargo test -p darkfi_tender_contract
```

## See Also

- [Identity Contract](../identity/README.md) - O-Cap authorization primitive
- [Labor Market Contract](../labor_market/README.md) - Job execution after tender award
- [Insurance Market Contract](../insurance_market/README.md) - Underwriting and coverage
- [O-Cap Architecture](../../doc/src/arch/ocap.md) - The O-Cap paradigm