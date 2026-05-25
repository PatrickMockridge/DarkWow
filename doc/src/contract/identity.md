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

## Contract Functions

16 function variants (0x00-0x0f), all implemented.

### Credential Functions (0x00-0x08)

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | `InitializeV1` | Initialize identity registry |
| 0x01 | `IssueCredentialV1` | Issuer issues credential to holder |
| 0x02 | `RevokeCredentialV1` | Issuer revokes a credential |
| 0x03 | `CreateClaimV1` | Holder creates claim (Level 0 zk_only) |
| 0x04 | `VerifyClaimV1` | Verifier checks claim on-chain |
| 0x05 | `CreateClaimV1L1` | Holder creates claim (Level 1 selective) |
| 0x06 | `CreateClaimV1L1V2` | Level 1 with LessThanOrEqual |
| 0x07 | `CreateClaimV1Multi` | Multi-credential AND claim |
| 0x08 | `CreateClaimV1Ratio` | Ratio-based predicate claim |

### O-Cap Capability Functions (0x09-0x0f)

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x09 | `RegisterCapabilityV1` | Register a new capability type |
| 0x0a | `IssueCapabilityV1` | Issue a capability to a holder |
| 0x0b | `VerifyCapabilityV1` | Verify a capability proof (cross-contract) |
| 0x0c | `RevokeCapabilityV1` | Revoke a capability |
| 0x0d | `CreateClaimDAGV1` | DAG-based claim (multi-path credentials) |
| 0x0e | `RegisterIssuerV1` | Register a trusted credential issuer |
| 0x0f | `UpdateReputationV1` | Update a relayer's reputation score |

## ZK Circuits

All 8 circuits compiled to `.zk.bin`:

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
| `reputations` | Relayer reputation records |

## The Privacy Gradient

| Level | Name | What Verifier Sees |
|-------|------|-------------------|
| 0 | `zk_only` | Nothing (proof valid/invalid only) |
| 1 | `selective` | Predicate result (1/0) |
| 2 | `attested` | Issuer confirms |
| 3 | `public` | Full disclosure |

## O-Cap Composability

`VerifyCapabilityV1 (0x0b)` is the primary cross-contract entrypoint:

| Contract | Capabilities Used |
|----------|-------------------|
| **dao_escrow** | `member_vote`, `board_treasury`, `board_endowment`, `dispute_arbitrator` |
| **tender** | `qualified_provider` (SubmitBidWithCapabilityV1) |
| **labor_market** | `verified_contractor` |
| **insurance_market** | `auditor_bond`, `institutional_inv`, `oracle_resolution` |

## See Also

- [O-Cap Architecture](../arch/ocap.md)
- [DAO-Escrow Contract](dao_escrow.md) — primary consumer of O-Cap verification
- [Tau Task Delegation](tau.md) — Authorization Inversion with O-Cap
- [Composability](composability.md) — Cross-contract call mechanism
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)
