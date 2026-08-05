# Identity Contract

The Identity contract is the **Object Capability (O-Cap) authorization layer** for the DarkWow ecosystem. It enables holders to prove capabilities ("can_vote", "can_spend_treasury") without revealing identity — a paradigm shift from ACL-based access control to capability-based authorization.

---

## "Anonymous Identity" — The Oxymoron That Works

"How can identity be anonymous?" The phrase sounds like a contradiction — and in
the ACL world it IS one. If authorization means checking a name against a list,
then anonymity and authorization are fundamentally incompatible. You can't be
both unknown and authorized.

But that framing IS the bug. Identity doesn't mean "my name is Alice." Identity
means "I possess a specific attribute or authorization, and I can prove it."
The name is cargo-culted from the database — a unique key for looking up
permissions. The authorization is what actually matters.

DarkWow inverts this. Instead of asking "WHO has access?", the system asks "Can
you PROVE you have access?" The proof IS the identity. The verifier learns the
predicate result — yes you can vote, yes you can spend the treasury, yes you
are a qualified underwriter — without learning who you are, what specific
credential you hold, or what attribute value satisfied the check.

### Why the Inversion Works — The Authorization Inversion Theorem

This is not hand-waving. The inversion from identity-based to proof-based
authorization has a formal mathematical foundation. In any ACL system,
authorization is modelled as:

```
A(p, r, s) = 1  iff  (p, r, s) ∈ L
```

A principal `p` is authorised for resource `r` and action `s` if and only if
the tuple appears in a pre-authorised list `L`. In this model, authorisation
**structurally leaks identity information.** An observer who sees "ACCESS
GRANTED" learns that the principal belongs to the authorised set for that
resource — quantified by the information leakage formula:

```
I_min(p; grant) = log₂ |{ p' ∈ P : (p', r, s) ∈ L }|
```

In plain English: the minimum information an observer learns about you is how
many people are in your authorization group. If only 3 people can access the
treasury, an observer learns you are one of 3. This is not a bug in a
particular ACL implementation — it is a **structural property** of any system
that conditions access on identity.

The inversion replaces the identity-dependent check with a witness-dependent
one:

```
A'(π, r, s) = ∃ w : P_{r,s}(w) = 1
```

Where:
- **A'** is the inverted, privacy-preserving authorization function
- **π** is a zero-knowledge proof — not a principal identity
- **w** is a secret witness known only to the prover
- **P_{r,s}** is a predicate depending only on resource `r` and action `s` —
  **never on identity p**

**Theorem (Authorization Inversion).** An ACL-based authorization system
A(p, r, s) can be inverted to a privacy-preserving capability scheme A'(π, r, s)
if and only if there exists a zero-knowledge proof system for the language
L_{r,s} = { w : P_{r,s}(w) = 1 }, with proofs simulatable without knowledge of w.

In plain English: **authorization is granted if and only if there exists a
secret witness w that satisfies the predicate — and the ZK proof π demonstrates
this without revealing w.** The verifier learns the predicate result (1 or 0).
The witness — and everything it encodes about its holder — stays private. The
equation formally proves that capability-based authorization is mathematically
equivalent to having a ZK proof system for the predicate defined by the
capability.

Applied to the voting booth example: the predicate P is "holder is registered
in District 7," the witness w is your voter registration credential, and the
ZK proof π is the cryptographic equivalent of handing over your voter card. The
official verifies the proof without seeing w — just like they check the card
without asking your name. And because the proof reveals nothing about w beyond
what the predicate exposes, different votes cast with different credentials for
the same district are cryptographically unlinkable.

For the full formal treatment including the soundness lemma for the
`LessThanOrEqual` gate that makes practical return-value comparison possible
within ZK circuits, see [The Zero-Knowledge Authorization Inversion
Theorem](https://technologytruth.substack.com/p/the-zero-knowledge-authorization)
and [O-Cap Architecture](../arch/ocap.md).

### The Voting Booth

Most people have already experienced anonymous identity. When you walk into a
polling station, you hand the official a voter card — a slip of paper that says
"this person can vote in this election." The official checks the card, tears
off the stub, and gives you a ballot. They never ask your name. They don't
check your ID. The card IS the authorization, and once consumed (torn), it
cannot be reused.

Now imagine:

- The voter card is a **ZK credential** issued off-chain by the electoral
  authority. The card says "holder is registered in District 7" — but never
  reveals the holder's name, address, or ID number.
- Walking into the polling station is calling **`VerifyCapabilityV1 (0x0b)`**
  on the Identity contract. The ZK proof cryptographically demonstrates "I hold
  a valid District 7 credential" without revealing which credential, which
  district, or anything else.
- The official tearing off the stub is the **nullifier** — the credential is
  consumed exactly once, preventing double-voting. The nullifier is a
  cryptographic hash that proves consumption without revealing what was
  consumed.
- The ballot you cast is **unlinkable** to the credential. Nobody —
  not the electoral authority, not the DAO, not an observer — can trace your
  vote back to your registration.

This is not speculative. It is what `VerifyCapabilityV1` does today. The DAO
governs a treasury, and members vote on proposals. Each member proves "I hold
a `member_vote` capability." That's it. No name. No address. No link between
votes. Nobody knows who voted — only that the threshold was reached by valid
members.

### It's Not KYC. It's the Opposite. And It's Safer.

KYC says: "Reveal everything about yourself, to everyone, every time you
transact. Trust us to store it securely. Trust us not to leak it. Trust every
intermediary in the chain."

Anonymous identity says: "Prove exactly what you need to prove, for exactly
this interaction, and nothing more. The data is firewalled off at the source."

| | KYC/AML | DarkWow Identity |
|---|---|---|
| What you reveal | Everything (name, address, ID scan, income, biometrics) | One predicate result (0 or 1) |
| Data storage | Centralised honeypot at every service provider | Zero — proofs are transient, nullifiers are unlinkable hashes |
| Reuse risk | Every provider stores a copy; one breach leaks everything | Capabilities are per-contract-instance, cryptographically unlinkable |
| Revocation | Ask the database admin to delete your record | Issuer revokes the credential; all claims become unprovable |
| Compartmentalisation | None — your passport number is the same everywhere | `SecretKey::derive_instance` gives every contract a unique, unlinkable key |

But here is the critical point: **these systems can compose.** A jurisdiction
that legally requires KYC — say, for regulated securities trading — doesn't
need to break the anonymous identity model. The KYC check happens ONCE, at the
point of credential issuance. A regulated issuer verifies your real-world
documents, then issues an anonymous capability credential: "holder has passed
KYC Level 2." From that point forward, you prove the capability — never the
underlying documents. The KYC data is firewalled at the issuer, not replicated
to every contract, exchange, and DeFi protocol you interact with. One gate.
One check. Zero data sprawl.

This is structurally impossible in the ACL model, where every service must
independently verify your identity by requesting copies of your documents.
In DarkWow's model, the credential IS the verification. Proving it doesn't
leak who verified it, when, or what documents were checked.

---

## The O-Cap Universe — What Identity Unlocks

Identity is not a standalone contract. It is the **authorization primitive**
that every other contract composes with. Without Identity, DarkWow has tokens
and privacy. With Identity, DarkWow has governance, labour markets, insurance,
qualified tendering, subscription access, and reputation — all without
surrendering privacy.

Here is every contract that uses `VerifyCapabilityV1 (0x06)` as a child call,
and what it proves:

| Contract | Capability | What It Enables |
|----------|-----------|-----------------|
| **dao_escrow** | `member_vote` | DAO member votes on treasury proposals. One member, one vote, zero identity leakage. |
| **dao_escrow** | `board_treasury` | Board member authorises treasury spends. Authority is bounded to exactly the spend being approved. |
| **dao_escrow** | `board_endowment` | Endowment committee manages long-term capital. A different capability from treasury — role separation with no shared identity. |
| **dao_escrow** | `dispute_arbitrator` | Arbitrator resolves disputes. The arbitrator proves their role; the disputants never learn who they are. |
| **labor_market** | `verified_contractor` | Worker proves qualification without revealing their work history, portfolio, or identity. The employer sees only: "qualified." |
| **tender** | `qualified_provider` | Bidder proves they meet the tender's competency requirements. No bidder list. No favoritism. No leak of who bid. |
| **insurance_market** | `auditor_bond` | Underwriter proves they hold sufficient audit bond to back coverage. Financial strength proven without balance sheet disclosure. |
| **insurance_market** | `institutional_inv` | Institutional investor proves accredited status. One proof, not a 50-page KYC packet sent to every protocol. |
| **subscription** | `access_tier` | Subscriber proves they hold a valid subscription tier. Access granted without the service knowing who you are. |
| **tau** | `task_role` | Worker accepts a delegated task by proving role capability. The task assigner never learns the worker's identity. |

Every one of these uses the same mechanism: a ZK proof that a capability exists
and has not been revoked. The verifier — the DAO, the employer, the insurance
market, the tender issuer — learns exactly one bit: "authorised" or "not
authorised." The capability model means authority is **bounded.** A
`member_vote` capability cannot authorise a treasury spend. A `verified_contractor`
cannot vote in the DAO. Each capability is a key that opens exactly one door.

---

## The Claim Gradient — How Much You Reveal

Not every interaction needs zero disclosure. Identity supports a **privacy gradient**
through a consolidated `CreateClaimV1 (0x03)` entrypoint with five claim modes:

| Mode | Name | What Verifier Sees | Use Case |
|------|------|-------------------|----------|
| 0 | `basic` | Nothing — proof valid/invalid only | Simple membership: "I am in the DAO" |
| 1 | `threshold` | Predicate result (1/0) | "I earn ≥ $50K" — amount not revealed |
| 2 | `ratio` | Ratio predicate result | "My value meets the threshold ratio" |
| 3 | `multi` | AND of up to 3 credentials | "I hold a law degree AND a bar license" |
| 4 | `dag` | Multi-path credential DAG | "I qualify via education OR work experience OR certification" |

All five modes use the same unified `CreateClaimV2` ZK circuit. The `claim_mode`
field in `CreateClaimParams` selects the mode. The DAG variant enables **multi-path
qualification** where a competency can be satisfied through different credential
routes — the verifier learns only that at least one path is satisfied,
without learning which one.

---

## Architecture

```
src/contract/identity/
├── proof/
│   ├── create_claim.zk          (unified, 5 claim modes)
│   ├── issue_credential.zk
│   └── verify_capability.zk
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

9 function variants (0x00-0x08).

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | `InitializeV1` | Initialize identity registry |
| 0x01 | `IssueCredentialV1` | Issuer issues credential to holder |
| 0x02 | `RevokeCredentialV1` | Issuer revokes a credential |
| 0x03 | `CreateClaimV1` | Unified claim creation (modes 0-4: basic/threshold/ratio/multi/dag) |
| 0x04 | `RegisterCapabilityV1` | Register a new capability type |
| 0x05 | `IssueCapabilityV1` | Issue a capability to a holder |
| 0x06 | `VerifyCapabilityV1` | Verify a capability proof (cross-contract) |
| 0x07 | `RevokeCapabilityV1` | Revoke a capability |
| 0x08 | `RegisterIssuerV1` | Register a trusted credential issuer |

## ZK Circuits

| Circuit | Namespace | Purpose |
|---------|-----------|---------|
| `create_claim.zk` | `CreateClaimV2` | Unified claim creation (5 modes via `cond_select`) |
| `issue_credential.zk` | `IssueCredentialV2` | Prove credential issuance |
| `verify_capability.zk` | `VerifyCapabilityV2` | Capability proof verification |

## Database Trees

| Tree | Purpose |
|------|---------|
| `credentials` | Issued credentials |
| `nullifiers` | Revocation tracking |
| `issuers` | Trusted issuers |
| `config` | Configuration |
| `capabilities` | Capability definitions |

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [O-Cap Architecture](../arch/ocap.md)
- [DAO-Escrow Contract](dao_escrow.md) — primary consumer of O-Cap verification
- [Tau Task Delegation](tau.md) — Authorization Inversion with O-Cap
- [Composability](composability.md) — Cross-contract call mechanism
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)
