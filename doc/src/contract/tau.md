# Tau Task Delegation with O-Cap Authorization

*Tau is a privacy-preserving task delegation system that integrates with the tender/labor market pipeline using Object Capabilities (O-Cap) for authorization.*

**Note**: Tau exists in two variants:
- **taud** (`bin/tau/taud/`) - Off-chain task management using NaCl/X25519
- **tau_pallas** (`bin/tau/tau_pallas/`) - Pallas-native variant with on-chain DarkWow integration

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Tau + O-Cap Delegation System                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   TENDER              LABOR MARKET         TAUD/TAU_PALLAS                    │
│   Contract              Contract         Task Delegation                        │
│       │                   │                    │                               │
│       ▼                   ▼                    ▼                               │
│  ┌─────────┐        ┌─────────┐         ┌─────────┐                          │
│  │ Award   │        │ Create  │         │ Create  │                          │
│  │ winner  │──────▶│ job     │────────▶│ task    │                          │
│  │         │        │         │         │ + cap   │                          │
│  └─────────┘        └─────────┘         │ req     │                          │
│                                          │ + mode  │                          │
│                                          └────┬────┘                          │
│                                               │                               │
│                                    ┌──────────┴──────────┐                    │
│                                    │                     │                    │
│                              ┌─────▼─────┐       ┌─────▼─────┐             │
│                              │   TAUD    │       │TAU_PALLAS │             │
│                              │(Off-chain)│       │(On-chain) │             │
│                              └───────────┘       └─────┬─────┘             │
│                                                         │                    │
│                                                         ▼                    │
│                                          ┌────────────────────────┐          │
│                                          │      dwowd           │          │
│                                          │  (tx.broadcast RPC)   │          │
│                                          └───────────┬────────────┘          │
│                                                      │                      │
│                                          ┌───────────▼───────────┐           │
│                                          │  Identity Contract   │           │
│                                          │ (VerifyCapabilityV1)│           │
│                                          └───────────────────────┘           │
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

Tau applies the **Authorization Inversion** principle from [O-Cap Authorization](../arch/ocap.md):

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
    // Call identity contract's VerifyCapabilityV1 (0x06)
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

### Phase 2: On-Chain Verification (✅ UNBLOCKED via Tau_Pallas)
- [x] Implement `verify_capability_onchain` with Identity contract interface
- [x] **DONE**: Add wallet integration via Pallas-native dwow_sdk crypto
- [x] **DONE**: Connect to dwowd RPC for `tx.broadcast` via DarkfidClient
- [x] **DONE**: Build and sign transactions with Pallas keys

**Solution**: Created **Tau_Pallas** (`bin/tau/tau_pallas/`) - a Pallas-native variant of tau that:
- Uses `dwow_sdk::crypto::Keypair` for Pallas curve key management
- Has `DarkfidClient::broadcast_tx()` for dwowd RPC integration
- Can construct `Transaction` objects and sign with `tx.create_sigs()`
- Implements working `verify_capability_onchain()` that broadcasts verification txs

**Note**: `taud` (NaCl/X25519) remains for pure off-chain task management. `tau_pallas` is for on-chain integration.

### Tau_Pallas: Pallas-Native Variant

Created as a separate binary alongside `taud` for direct DarkWow integration:

| Component | taud | tau_pallas |
|-----------|------|------------|
| Keypair | NaCl/X25519 | Pallas (dwow_sdk) |
| Signing | crypto_box | Schnorr (tx.create_sigs) |
| dwowd RPC | N/A | DarkfidClient |
| On-chain verification | Fallback only | Working |

#### Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        TAU ECOSYSTEM                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌─────────────────┐              ┌─────────────────┐           │
│   │      TAUD       │              │   TAU_PALLAS    │           │
│   │  (Off-chain)    │              │  (On-chain)     │           │
│   └────────┬─────────┘              └────────┬─────────┘           │
│            │                                 │                      │
│            │     ┌─────────────────┐       │                      │
│            └────▶│  Task Storage   │◀──────┘                      │
│                  │   (JSON files)   │                               │
│                  └────────┬────────┘                               │
│                           │                                          │
│                           ▼                                          │
│                  ┌─────────────────┐                               │
│                  │    dwowd       │                               │
│                  │  (blockchain)    │                               │
│                  └────────┬────────┘                               │
│                           │                                          │
│                           ▼                                          │
│                  ┌─────────────────┐                               │
│                  │ Identity Contract│                               │
│                  │ (capabilities)   │                               │
│                  └─────────────────┘                               │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

#### DarkfidClient RPC

The core of tau_pallas's on-chain capability is `DarkfidClient`:

```rust
// Create client connected to dwowd
let client = DarkfidClient::new("http://localhost:18332", executor).await?;

// Broadcast a transaction
let tx_hash = client.broadcast_tx(&signed_tx).await?;
```

#### On-Chain Verification Flow

When a worker claims a task with `verification_mode: OnChain`:

```
1. Worker submits CapabilityProof to tau_pallas
2. tau_pallas constructs Identity contract call:
   - Function: VerifyCapabilityV1 (0x06)
   - Params: capability_proof, verifier_pub, fee
3. tau_pallas signs transaction with PM's Pallas secret key
4. tau_pallas broadcasts via DarkfidClient.broadcast_tx()
5. dwowd validates and includes tx in block
6. Identity contract verifies ZK proof on-chain
7. Identity contract emits CapabilityVerified event
```

#### Key Files

- `bin/tau/tau_pallas/src/rpc_client.rs` - DarkfidClient for tx.broadcast
- `bin/tau/tau_pallas/src/identity_client.rs` - VerifyCapability calldata builder
- `bin/tau/tau_pallas/src/capability.rs` - Working on-chain verification

#### When to Use Each

| Use Case | Binary | Why |
|----------|--------|-----|
| Pure task management, no blockchain | taud | Lightweight, no dependencies |
| On-chain capability verification | tau_pallas | Full Pallas/DarkWow integration |
| Testing off-chain claims | taud | Fast local verification |
| Production on-chain verification | tau_pallas | ZK proofs verified on-chain |
| Low-value tasks, trusted workers | taud (OffChain mode) | Fast, cheap |
| High-value tasks, new workers | tau_pallas (OnChain mode) | Maximum security |

### Phase 3: Full Pipeline Integration ✅ COMPLETE

- [x] Add `labor_job_id` field to TaskInfo (link to labor market job)
- [x] Add `labor_attestation_id` field to TaskInfo (link to attestation)
- [x] Add `payment_token` field to TaskInfo
- [x] Add `payment_amount` field to TaskInfo
- [x] Add `link_task_to_job` RPC method to link tau task to labor market job
- [x] Add `submit_task_deliverable` RPC method to trigger labor market deliverable submission
- [x] Add `register_capability` RPC method for tender winner to register capabilities

**Implementation**: All Phase 3 RPC methods implemented in `bin/tau/tau_pallas/src/jsonrpc.rs`:
- `link_task_to_job` - Links tau task to labor market job with payment details
- `submit_task_deliverable` - Submits deliverable to labor market; on-chain tx submission pending dwowd wallet integration
- `register_capability` - Registers capabilities for tender winner; on-chain registration pending identity contract wallet integration

**Note**: Full on-chain transaction submission for `submit_task_deliverable` requires dwowd wallet integration (app-layer responsibility).

### Phase 4: Privacy Hardening
- [ ] Hide task-assignment correlation
- [ ] Prevent timing analysis
- [ ] Add rate limiting for claim operations

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [Identity Contract](./identity.md) - O-Cap implementation
- [O-Cap & Composable Privacy](../arch/ocap.md) - Authorization inversion theory
- [Labor Market Contract](./labor_market.md) - Job creation and payment
- [Tender Contract](./tender.md) - Sealed bid procurement
- [Zero-Knowledge Authorization](https://technologytruth.substack.com/p/the-zero-knowledge-authorization) - Mathematical foundation