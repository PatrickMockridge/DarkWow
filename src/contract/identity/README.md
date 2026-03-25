# DarkFi Identity Contract

Minimal credential proofs for selective disclosure of attributes.

## The Problem: Identity Verification = Surveillance

Traditional identity verification requires revealing everything:
- **KYC**: Name, DOB, address, SSN — all given to the verifier
- **OAuth/OIDC**: Your identity handed to third parties
- **Proof of Personhood**: Reveals who you are to verify humanness

**But sometimes you just need to prove you're over 18, or hold a token, or are a DAO member — without revealing WHO you are.**

## Our Solution: Minimal Viable Information (MVI)

Release only the **minimum information necessary**:

```
Traditional KYC:              DarkFi Identity (MVI):
┌─────────────────────┐      ┌─────────────────────────┐
│ Name: Alice          │      │ Age: ✓ (over 18)         │
│ DOB: 1990-01-01      │ →    │ Residency: ✓            │
│ Address: 123 Main St │      │ Not OFAC: ✓              │
│ SSN: ***-**-1234     │      │ Credential: DAO Member ✓ │
└─────────────────────┘      └─────────────────────────┘
    ALL THE DATA                    JUST A PROOF
```

## Level 0 MVP: Issuer-Holder-Verifier

```
┌─────────────────────────────────────────────────────────────────────┐
│                  Identity Contract Flow                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ISSUER                                  HOLDER                       │
│     │                                        │                        │
│     │  1. Issues credential                  │                        │
│     │     Credential = signed(               │                        │
│     │       attributes,                      │                        │
│     │       expiration                      │                        │
│     │─────────────────────────────→         │                        │
│     │                                        │                        │
│     │  2. Holder creates claim               │                        │
│     │     Claim = ZKProof{                   │                        │
│     │       "I hold valid credential"        │                        │
│     │       "age > 18"                       │                        │
│     │       "nullifier"                      │                        │
│     │     }                                  │                        │
│     │←──────────────────────────────────────│                        │
│     │                                        │                        │
│     │  3. Verifier checks claim             │                        │
│     │     ZK proof verifies:                 │                        │
│     │       - Credential valid               │                        │
│     │       - Conditions met               │                        │
│     │       - Not revoked                  │                        │
│     │───────────────────→───────────────────────→ VERIFIER            │
│                                                                       │
│  RESULT: ✓ or ✗ — NO additional information revealed                 │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

## Privacy Properties

| What You Reveal | What Stays Hidden |
|-----------------|-------------------|
| "I meet criteria" | Who you are |
| "Credential valid" | Your actual data |
| "Not revoked" | When credential expires |
| Issuer is trusted | Full credential contents |
| Predicate result (e.g., true/false) | The actual attribute values |

## Use Cases

### Age Verification
```rust
// Prove you're over 18 without revealing DOB
let claim = CreateClaimBuilder::new()
    .claim_type(b"age_over_18")
    .predicate(b">= 18")
    .build()?;
// Reveals: Only "age >= 18" — birthdate hidden
```

### DAO Membership
```rust
// Prove you're a DAO member without revealing wallet
let claim = CreateClaimBuilder::new()
    .claim_type(b"dao_member")
    .predicate(b">= 1")
    .build()?;
// Reveals: Only "holds >= 1 token" — balance and address hidden
```

### Accredited Investor
```rust
// Prove accredited status without revealing income
let claim = CreateClaimBuilder::new()
    .claim_type(b"accredited_investor")
    .predicate(b"== true")
    .build()?;
// Reveals: Only "is accredited" — income/net worth hidden
```

### Sybil Resistance
```rust
// Prove you're a unique human without deanonymizing
let claim = CreateClaimBuilder::new()
    .claim_type(b"unique_human")
    .predicate(b"== true")
    .build()?;
// Reveals: Only "is unique human" — identity hidden
```

## Contract Functions

### InitializeV1 (0x00)

Initializes the identity contract:
- Creates `credentials` tree for issued credentials
- Creates `nullifiers` tree for revocation tracking
- Creates `issuers` tree for trusted issuers
- Creates `config` tree for settings

### IssueCredentialV1 (0x01)

Issuer creates a credential for a holder:
- Verifies issuer is trusted
- Verifies nullifier uniqueness
- Stores credential record (commitment, not attributes)
- Emits `CredentialIssued` event (only nullifier + issuer revealed)

### RevokeCredentialV1 (0x02)

Issuer revokes a credential:
- Verifies issuer signature
- Marks credential as revoked
- Updates revocation list

### CreateClaimV1 (0x03)

Holder creates a claim from their credential:
- Verifies credential exists and is valid
- Verifies not expired or revoked
- Generates ZK proof of predicate satisfaction
- Emits `ClaimCreated` event

### VerifyClaimV1 (0x04)

Verifier checks a claim:
- Verifies credential exists and is valid
- Verifies ZK proof
- Verifies not expired or revoked
- Emits `ClaimVerified` event with result

## ZK Circuits

### issue_credential_v1.zk

Proves the issuer legitimately issued this credential:
- **Public inputs**: commitment, issuer_pub, holder_pub, schema_hash
- **Private inputs**: attributes, issuer_signature
- **Verification**: Signature valid, commitment matches

### create_claim_v1.zk

Proves the claim without revealing attributes:
- **Public inputs**: nullifier, claim_type, predicate_result
- **Private inputs**: credential_attributes, holder_secret
- **Verification**: Credential valid, predicate satisfied, not revoked

### verify_claim_v1.zk

Proves the claim can be verified:
- **Public inputs**: claim, verifier_pub
- **Private inputs**: issuer_pub, holder_secret
- **Verification**: Claim valid, issuer trusted, not double-spent

## Roadmap

```
Level 0 (MVP)           Level 1               Level 2               Level 3
─────────────────────────────────────────────────────────────────────────────
Issuer-Holder-Verifier   Credential chaining    Anonymous credentials  Self-sovereign
Single issuer            Multiple issuers       CL signatures          Full revocation
On-chain verification    Off-chain proof        Selective disclosure   Universal IDs
                          generation
```

### Why This Roadmap?

**Level 0 (NOW)**: Issuer-Holder-Verifier
- Minimal blast radius
- Clear trust model
- Foundation for everything else

**Level 1 (Future)**: Multiple issuers, chaining
- Credentials can reference other credentials
- Building blocks for reputation systems

**Level 2 (Future)**: Anonymous credentials (CL signatures)
- Issuer can't track credential usage
- Complete unlinkability

**Level 3 (Future)**: Self-sovereign identity
- User controls their own data
- Universal revocation capability

## Comparison

| Feature | Traditional KYC | ZK-Based Verifier | DarkFi Identity |
|---------|-----------------|-------------------|-----------------|
| Identity revealed | Everything | Depends on verifier | Nothing (MVI) |
| Data minimization | None | Partial | Full |
| Revocability | Full | Limited | Full |
| Issuer tracking | Full | Partial | None |
| Offline verification | No | Possible | Possible |

## References

- [DarkFi Identity Contract](../../src/contract/identity/)
- [DarkFi DEX Contract](../dex/)
- [DarkFi Money Contract](../money/)
- [DarkFi Bridge Contract](../bridge/)
- [Differential Privacy](https://en.wikipedia.org/wiki/Differential_privacy)
- [Anonymous Credentials](https://en.wikipedia.org/wiki/Anonymous_credentials)