# Identity Contract

Minimal credential proofs for selective disclosure of attributes.

## Overview

The identity contract enables **Minimal Viable Information (MVI)** - proving you meet certain criteria without revealing more than necessary.

**Key Innovation**: Instead of revealing everything (traditional KYC), prove only "I meet criteria" with ZK proofs.

```
Traditional KYC:              DarkWow Identity (MVI):
┌─────────────────────┐      ┌─────────────────────────┐
│ Name: Alice          │      │ Age: ✓ (over 18)         │
│ DOB: 1990-01-01      │ →    │ Residency: ✓            │
│ Address: 123 Main St │      │ Not OFAC: ✓              │
│ SSN: ***-**-1234     │      │ Credential: DAO Member ✓ │
└─────────────────────┘      └─────────────────────────┘
    ALL THE DATA                    JUST A PROOF
```

## The Privacy Gradient

Graduated disclosure levels:

| Level | Name | What Verifier Sees | Use Case |
|-------|------|-------------------|----------|
| 0 | zk_only | Nothing | Maximum privacy |
| 1 | selective | Predicate result only | Basic verification |
| 2 | attested | Issuer confirms | Trusted issuers |
| 3 | public | Full disclosure | Regulatory compliance |

## Issuer-Holder-Verifier Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                  Issuer-Holder-Verifier Flow                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ISSUER: Issues credential (signed, encrypted)                   │
│     │                                                             │
│     │─── credential ───→ HOLDER (knows secret)                   │
│     │                                                             │
│  HOLDER: Creates ZK claim                                        │
│     │                                                             │
│     │─── claim (ZK proof) ───→ VERIFIER                          │
│     │                                                             │
│  VERIFIER: Checks proof → "✓ meets criteria"                     │
│                                                                   │
│  WHAT VERIFIER LEARNS: Only whether conditions met               │
│  WHAT STAYS HIDDEN: Identity, actual attribute values             │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Use Cases

| Instead of revealing... | We prove... |
|------------------------|-------------|
| Full identity (KYC) | "Over 18" or "Accredited investor" |
| Wallet address + balance | "Holds ≥1 DAO token" |
| Exact income | "Income exceeds threshold" |
| Real name | "Unique human (Sybil resistant)" |

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize identity registry |
| IssueCredentialV1 | 0x01 | Issuer creates credential |
| RevokeCredentialV1 | 0x02 | Issuer revokes credential |
| CreateClaimV1 | 0x03 | Holder creates ZK claim |
| VerifyClaimV1 | 0x04 | Verify claim on-chain |

## ZK Circuits

- `issue_credential_v1.zk`: Prove issuer legitimately issued credential
- `create_claim_v1.zk`: Prove predicate satisfied without revealing attributes

## Roadmap: ZK-Verified Competency DAGs

```
Level 0 (MVP - NOW)     Level 1 (Future)        Level 2 (Future)        Level 3 (Future)
─────────────────────────────────────────────────────────────────────────────────────────
Issuer-Holder-Verifier    Competency DAG          Trust Networks           K-Assets
Basic predicates         Prerequisite chains     Graduated disclosure     Knowledge markets
On-chain verify          Off-chain proofs       Anonymous reputation     Economic activation
```

## Structure

```
src/contract/identity/
├── proof/
│   ├── issue_credential_v1.zk
│   └── create_claim_v1.zk
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

## Building

```bash
cd src/contract/identity
cargo build
cargo test
```

## References

- [Identity Architecture](../../arch/identity.md)
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)
- [Anonymous Credentials](https://en.wikipedia.org/wiki/Anonymous_credentials)
