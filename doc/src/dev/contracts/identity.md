# Identity Contract

The Identity contract is the **Object Capability (O-Cap) authorization layer** for the DarkWow ecosystem. It enables holders to prove capabilities ("can_vote", "can_spend_treasury") without revealing identity — a paradigm shift from ACL-based access control to capability-based authorization.

## Architecture

```
src/contract/identity/
├── proof/
│   ├── issue_credential_v1.zk
│   ├── create_claim_v1.zk
│   ├── create_claim_v1_l1.zk
│   ├── create_claim_v1_l1_v2.zk
│   ├── create_claim_v1_multi.zk
│   ├── create_claim_v1_ratio.zk
│   ├── create_claim_v1_dag.zk
│   └── verify_capability_v1.zk
├── src/
│   ├── client/mod.rs
│   ├── entrypoint.rs
│   ├── error.rs
│   ├── lib.rs
│   └── model/mod.rs
├── tests/
├── Cargo.toml
└── README.md
```

## Implementation Status (May 2026)

All 14 entrypoints (0x00-0x0d) are fully implemented and compiled.

### Credential Functions (0x00-0x08)

| Opcode | Function | Description | Status |
|--------|----------|-------------|--------|
| 0x00 | `InitializeV1` | Initialize identity registry | Complete |
| 0x01 | `IssueCredentialV1` | Issuer issues credential to holder | Complete |
| 0x02 | `RevokeCredentialV1` | Issuer revokes a credential | Complete |
| 0x03 | `CreateClaimV1` | Holder creates claim (Level 0 zk_only) | Complete |
| 0x04 | `VerifyClaimV1` | Verifier checks claim on-chain | Complete |
| 0x05 | `CreateClaimV1L1` | Holder creates claim (Level 1 selective) | Complete |
| 0x06 | `CreateClaimV1L1V2` | Level 1 with LessThanOrEqual | Complete |
| 0x07 | `CreateClaimV1Multi` | Multi-credential AND claim | Complete |
| 0x08 | `CreateClaimV1Ratio` | Ratio-based predicate claim | Complete |

### O-Cap Capability Functions (0x09-0x0d)

| Opcode | Function | Description | Status |
|--------|----------|-------------|--------|
| 0x09 | `RegisterCapabilityV1` | Register a new capability type | Complete |
| 0x0a | `IssueCapabilityV1` | Issue a capability to a holder | Complete |
| 0x0b | `VerifyCapabilityV1` | Verify a capability proof (cross-contract) | Complete |
| 0x0c | `RevokeCapabilityV1` | Revoke a capability | Complete |
| 0x0d | `CreateClaimDAGV1` | DAG-based claim (multiple credential paths) | Complete |

## ZK Circuits (all compiled to .zk.bin)

| Circuit | Namespace | Purpose |
|---------|-----------|---------|
| `issue_credential_v1.zk` | `IssueCredential_V1` | Prove credential valid |
| `create_claim_v1.zk` | `CreateClaim_V1` | Level 0 zk_only claim |
| `create_claim_v1_l1.zk` | `CreateClaim_V1_L1` | Level 1 bounded equation |
| `create_claim_v1_l1_v2.zk` | `CreateClaim_V1L1V2` | Level 1 LessThanOrEqual |
| `create_claim_v1_multi.zk` | `CreateClaim_V1Multi` | Multi-credential AND |
| `create_claim_v1_ratio.zk` | `CreateClaim_V1Ratio` | Ratio-based predicate |
| `create_claim_v1_dag.zk` | `CreateClaim_V1DAG` | Multi-path DAG claim |
| `verify_capability_v1.zk` | `VerifyCapability_V1` | Capability verification |

## Database Trees

| Tree | Purpose |
|------|---------|
| `credentials` | Issued credentials |
| `nullifiers` | Revocation tracking |
| `issuers` | Trusted issuers |
| `config` | Configuration |
| `capabilities` | Capability definitions |
| `capability_issuances` | Holder → capability mapping |

## O-Cap Composability

The Identity contract serves as the authorization backbone for the entire DarkWow contract ecosystem. Any contract can call `VerifyCapabilityV1` to check if a caller holds a required capability:

| Contract | Capabilities Used |
|----------|-------------------|
| **dao_escrow** | `member_vote`, `board_treasury`, `board_endowment`, `dispute_arbitrator` |
| **tender** | `qualified_provider` (SubmitBidWithCapabilityV1) |
| **labor_market** | `verified_contractor` |
| **insurance_market** | `auditor_bond`, `institutional_inv`, `oracle_resolution` |

### Cross-Contract Call Patterns

`VerifyCapabilityV1 (0x0b)` is the primary entrypoint called by other contracts via cross-contract child calls. Each calling contract validates the child call's function code in its instruction phase (before any state mutation):

| Calling Contract | Calling Function | Validates |
|---|---|---|
| **labor_market** | `AcceptJobWithCapabilityV1` (0x0d) | `child.data[0] != 0x0b` |
| **dao_escrow** | `VerifyMemberCapabilityV1` (0x0b) | `child.data[0] != 0x0b` |

Both follow the same pattern: a ZK proof in the calling contract's params proves the capability predicate, and the child call to Identity provides on-chain verification that the capability exists and has not been revoked.

Note: `tender` and `insurance_market` are listed in the composability table above but their child call wiring to Identity is not yet implemented (future work).

For the complete cross-contract call mechanism, see [Composability](../../contract/composability.md).

## The Privacy Gradient

| Level | Name | What Verifier Sees |
|-------|------|-------------------|
| 0 | `zk_only` | Nothing (proof valid/invalid only) |
| 1 | `selective` | Predicate result (1/0) |
| 2 | `attested` | Issuer confirms |
| 3 | `public` | Full disclosure |

## Roadmap

**Complete (Levels 0-1):**
- O-Cap authorization (Register/Issue/Verify/Revoke)
- Competency DAGs with multi-path credential chains
- Multi-credential AND logic, ratio-based predicates
- Cross-contract capability verification

**Future (Levels 2-3):**
- Trust networks (Web of Trust + ZK)
- Anonymous reputation
- K-Assets (knowledge assets with economic activation)

## Building

```bash
cd src/contract/identity
RAYON_NUM_THREADS=10 cargo build
RAYON_NUM_THREADS=10 cargo test
```

## References

- [Identity Architecture](../../arch/identity.md)
- [O-Cap Architecture](../../arch/ocap.md)
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)
- [DAO-Escrow Contract](../../contract/dao_escrow.md) — primary consumer of O-Cap verification
