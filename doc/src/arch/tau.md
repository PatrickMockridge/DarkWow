# Tau Task Delegation with O-Cap Authorization

*Tau is a privacy-preserving task delegation system that integrates with the tender/labor market pipeline using Object Capabilities (O-Cap) for authorization.*

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Tau + O-Cap Delegation System                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   TENDER              LABOR MARKET              TAU                           │
│   Contract              Contract          Task Delegation                       │
│       │                   │                    │                              │
│       ▼                   ▼                    ▼                              │
│  ┌─────────┐        ┌─────────┐         ┌─────────┐                         │
│  │ Award   │        │ Create  │         │ Create  │                         │
│  │ winner  │──────▶│ job     │────────▶│ task    │                         │
│  │         │        │         │         │ + cap   │                         │
│  └─────────┘        └─────────┘         │ req     │                         │
│                                          │ + mode  │                         │
│                                          └────┬────┘                         │
│                                               │                              │
│                                               ▼                              │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │              WORKER CLAIMS TASK                              │            │
│  │                                                              │            │
│  │  1. Worker proves: can_work_on_task (identity hidden)        │            │
│  │  2. Verification Mode determines HOW:                        │            │
│  │     - OffChain: Local check (fast, for trusted workers)      │            │
│  │     - OnChain: ZK verification via Identity (secure, new)     │            │
│  │  3. Task assigned to worker's capability                     │            │
│  │                                                              │            │
│  └─────────────────────────────────────────────────────────────┘            │
│                                               │                              │
│                                               ▼                              │
│                                          ┌─────────┐                        │
│                                          │ Complete│                        │
│                                          │ work    │                        │
│                                          └────┬────┘                        │
│                                               │                              │
│                                               ▼                              │
│                                       Labor Market Payment                    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Design Philosophy

### The Problem with Traditional Task Assignment

In traditional task management systems (Asana, Linear, Jira, etc.):

1. **Identity is always revealed**: When you assign a task, the system knows WHO you are
2. **Capability is implicit**: You see "Alice worked on X" but not "Alice has 10 years experience in Y"
3. **No separation of concerns**: Task assignment exposes both identity AND qualifications
4. **Trust is binary**: Either you have access or you don't

### O-Cap Solution: Authorization Inversion

Tau applies the **Authorization Inversion** principle from O-Cap:

| Traditional | O-Cap Tau |
|-------------|-----------|
| "Who are you?" | "What can you prove?" |
| Assign to alice | Assign to `can_code_rust` capability |
| Reveals identity | Reveals only qualification |
| Can't hide qualifications | Can prove qualification without identity |

### The Verification Mode Dial

Not all workers are equal - some are new/unproven, others are trusted. Tau provides a **dial** between security and speed:

```
┌─────────────────────────────────────────────────────────────────┐
│                  Verification Mode Dial                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│   OFF-CHAIN (Hot Path)          ON-CHAIN (Cold Path)              │
│   ┌─────────────────┐           ┌─────────────────┐               │
│   │ Fast           │           │ Secure          │               │
│   │ Cheap          │           │ Verified         │               │
│   │ Local check    │◀─────────▶│ ZK proof        │               │
│   └─────────────────┘           └─────────────────┘               │
│                                                                   │
│   FOR: Trusted workers        FOR: New/unproven workers           │
│   - Past contributors         - Never worked with                  │
│   - Verified identity         - No established trust               │
│   - Low-risk tasks           - High-value tasks                   │
│                                                                   │
│   WHO DECIDES: The person PAYING for the work                     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

**Key insight**: The **person paying** (PM/employer) decides the verification mode, not the worker. This is equitable - the payer takes on the risk of off-chain verification and chooses accordingly.

## Core Concepts

### Capability-Based Task Requirements

Tasks can require specific capabilities to be claimed:

```rust
struct TaskInfo {
    ref_id: String,
    title: String,
    // ... existing fields ...

    // O-Cap fields:
    required_capability_id: Option<[u8; 32]>,  // None = any worker
    verification_mode: VerificationMode,        // Who pays decides
    assigned_capability: Option<[u8; 32]>,     // Which cap claimed it
}
```

### Capability Requirements Flow

```
Project Manager (PM) creates capability:
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  RegisterCapability("can_work_on_project_X")                    │
│    - issuer: PM_pubkey                                         │
│    - credential_requirement: { min_threshold: 1 }             │
│                                                                 │
│  IssueCapability(worker_A, "can_work_on_project_X")           │
│  IssueCapability(worker_B, "can_work_on_project_X")           │
│                                                                 │
│  CreateTask(                                                  │
│    title: "Implement feature Y",                               │
│    required_capability_id: can_work_on_project_X,             │
│    verification_mode: OffChain,  // PM trusts these workers    │
│  )                                                             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Delegation Privacy

When a worker claims a task:

```
┌─────────────────────────────────────────────────────────────────┐
│              What the PM Learns (vs what they don't)                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  LEARNS:                                                         │
│  ✓ Worker has can_work_on_project_X capability                  │
│  ✓ predicate_result = 1 (requirements met)                      │
│  ✓ nullifier (exists, not who)                                  │
│  ✓ Task is claimed                                              │
│                                                                 │
│  DOES NOT LEARN:                                                │
│  ✗ Worker's identity                                           │
│  ✗ Worker's real name, employer, history                        │
│  ✗ How many other tasks worker has claimed                      │
│  ✗ Worker's exact qualifications                               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Integration with Tender/Labor Market

Tau sits at the end of the tender pipeline:

```
Tender → Winner Selected → Labor Market Job Created → Tau Task Delegation
                                              │
                                    Winner (PM) delegates subtasks
                                              │
                              Workers prove capabilities to claim work
                                              │
                              PM verifies (OffChain or OnChain) and assigns
                                              │
                              Worker completes, receives payment via Labor Market
```

### Capability Linking

The same capability used to win the tender can be used for subtask delegation:

```rust
// Tender winner registers capability
RegisterCapability("can_build_defi_protocol")
    → issues to themselves and qualified workers

// Tender winner creates labor market job
CreateJob(required_capability: can_build_defi_protocol)

// Tender winner creates tau subtasks
CreateTask(
    title: "Smart contract audit",
    required_capability_id: can_audit_smart_contracts,
    // capability derived from: can_build_defi_protocol + attestation
)
```

### Labor Market Integration

Phase 3 adds fields to TaskInfo for linking to the labor market payment pipeline:

```rust
pub struct TaskInfo {
    // ... existing fields ...

    // Phase 3: Labor Market integration
    pub labor_job_id: Option<[u8; 32]>,        // Link to labor market job
    pub labor_attestation_id: Option<[u8; 32]>, // Link to attestation for deliverable
    pub payment_token: Option<[u8; 32]>,        // Token for payment
    pub payment_amount: Option<u64>,             // Payment amount
}
```

**Payment Flow:**
```
1. PM links tau task to labor market job:
   set_task_labor_link(task_id, labor_job_id, attestation_id, payment_token, amount)

2. Worker completes task in tau

3. PM triggers deliverable submission:
   submit_task_deliverable(task_id, work_proof)

4. System submits to labor market:
   attestation.create_claim(work_proof)
   labor_market.submit_deliverable(job_id, claim_id)

5. PM confirms delivery:
   labor_market.confirm_delivery(job_id)

6. Payment released to worker
```

**Note**: Full automation requires wallet integration for signing transactions.

## Verification Modes

### Off-Chain (Hot Path)

For trusted workers - fast, cheap, local verification:

```rust
fn verify_offchain(proof: &CapabilityProof, required: &[u8; 32]) -> bool {
    // 1. Check capability_id matches
    proof.capability_id == required

    // 2. Verify issuer signature
    verify_signature(proof.issuer_pub, proof)

    // 3. Check predicate_result == 1
    proof.predicate_result == 1

    // 4. Check not expired
    proof.created_at + EXPIRY > now()
}
```

**Use when**: Worker has established trust, task is low-value, speed is critical

### On-Chain (Cold Path)

For unproven workers - full ZK verification via Identity contract:

```rust
fn verify_onchain(
    proof: &CapabilityProof,
    required: &[u8; 32],
    payer_pubkey: &PublicKey,
) -> ContractResult<bool> {
    // Call identity contract's VerifyCapabilityV1 (0x0b)
    let params = VerifyCapabilityParams {
        capability_proof: proof.clone(),
        verifier_pub: payer_pubkey.into(),
        fee: 0,
    };
    identity::verify_capability(params)
}
```

**Use when**: Worker is new, task is high-value, maximum security required

## TODOs

### Phase 1: Foundation (Naive Integration) ✅ COMPLETE
- [x] Add `required_capability_id` field to TaskInfo
- [x] Add `verification_mode` enum (OnChain, OffChain)
- [x] Add `assigned_capability` field
- [x] Create `bin/tau/taud/src/capability.rs` module
- [x] Add off-chain verification implementation
- [x] Add `claim_task` JSON-RPC method
- [x] Add `set_task_capability` JSON-RPC method
- [x] Build and test

### Phase 2: On-Chain Verification (BLOCKED - Requires Wallet Integration)
- [x] Implement `verify_capability_onchain` with Identity contract interface
- [ ] **BLOCKED**: Add wallet integration for transaction signing
- [ ] **BLOCKED**: Connect to darkfid RPC for `tx_broadcast`
- [ ] **BLOCKED**: Parse CapabilityVerified events

**Blocker**: Tau is a standalone daemon without wallet functionality. On-chain verification requires:
- Wallet to sign transactions (PM's secret key)
- Connection to darkfid RPC for `tx_broadcast`
- Event subscription to parse `CapabilityVerified` events

**Current workaround**: Off-chain verification works and is used as fallback.

### Architectural Decision: Wallet Integration Belongs in App Layer

On-chain verification with the Identity contract requires signing transactions with a Pallas scalar SecretKey, but tau uses NaCl/X25519 keys for task signing. These are incompatible cryptographic primitives:

| Component | Key System | Purpose |
|-----------|------------|---------|
| Tau (task signing) | NaCl/X25519 | Task authentication |
| DarkFi (tx signing) | Pallas scalar | Transaction authorization |
| Identity contract | Pallas scalar | ZK verification |

**Decision**: Wallet integration for on-chain verification belongs in the **app layer (UI/CLI)**, not in tau (base layer). The app layer provides:
- Wallet key management (Pallas scalars)
- Transaction construction and signing
- RPC connection to darkfid for broadcasting

Tau remains a standalone daemon focused on task management. App-layer tools (CLI, web UI) handle the bridge to DarkFi's on-chain components. This separation keeps tau simple and allows users to interact with tau through familiar interfaces while leveraging DarkFi's ZK capabilities when needed.

**Implication**: Phase 2 on-chain verification is blocked at the tau layer, but can be unblocked by building app-layer integration that connects tau task events to DarkFi wallet operations.

### Phase 3: Full Pipeline Integration (IN PROGRESS)
- [x] Add `labor_job_id` field to TaskInfo (link to labor market job)
- [x] Add `labor_attestation_id` field to TaskInfo (link to attestation)
- [x] Add `payment_token` field to TaskInfo
- [x] Add `payment_amount` field to TaskInfo
- [ ] Add `link_task_to_job` RPC method to link tau task to labor market job
- [ ] Add `submit_task_deliverable` RPC method to trigger labor market deliverable submission
- [ ] Add `register_capability` RPC method for tender winner to register capabilities

**Blocker**: Phase 3 full implementation requires wallet integration (Phase 2 blocker).
- [ ] Link tau tasks to labor market jobs
- [ ] Capability registration from tender winner
- [ ] Payment linking via attestation

### Phase 4: Privacy Hardening
- [ ] Hide task-assignment correlation
- [ ] Prevent timing analysis
- [ ] Add rate limiting for claim operations

## See Also

- [Identity Contract](./identity.md) - O-Cap implementation
- [O-Cap & Composable Privacy](./ocap.md) - Authorization inversion theory
- [Labor Market Contract](./labor_market.md) - Job creation and payment
- [Tender Contract](./tender.md) - Sealed bid procurement
- [Zero-Knowledge Authorization](https://technologytruth.substack.com/p/the-zero-knowledge-authorization) - Mathematical foundation