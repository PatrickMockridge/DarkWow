# Oracle Contract

A demonstration of the "push model" for oracles in DarkWow, enabling trustless external data integration with on-chain contracts.

## Overview

Oracles bridge the gap between external data sources and on-chain contract logic. The oracle contract implements a "push model" where oracle operators push data values that can be attested for consumption by other contracts.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Oracle Push Model                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Oracle Operator                                                             │
│     │                                                                       │
│     │  RegisterOracle(name, data_type)                                      │
│     ▼                                                                       │
│  Oracle(Active)                                                             │
│     │                                                                       │
│     │  PushValue(value)                                                    │
│     │  PushValue(value)                                                    │
│     │  PushValue(value)                                                    │
│     │                                                                       │
│     │  AttestValue(predicate, threshold)                                    │
│     ▼                                                                       │
│  Attestation(Active) ─────────────────────────────────────────────────────►│
│                                                                              │
│                                              Consumer Contract               │
│                                                 │                            │
│                                                 │ CreateClaim(evidence)      │
│                                                 ▼                            │
│                                              Claim(Verified)                 │
│                                                 │                            │
│                                                 │ ConsumeClaim()             │
│                                                 ▼                            │
│                                              Contract Logic                  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Key Concepts

### Push Model

- **Oracle pushes**: Oracle operator actively pushes data values to their feed
- **On-chain storage**: Values stored on-chain for verifiable access
- **Attestation**: Oracle creates attestations for specific values
- **Consumption**: Other contracts consume attestations via attestation contract

### Pull Model (Alternative)

- **Consumer pulls**: Data consumer queries off-chain oracle
- **Off-chain response**: Oracle provides data directly to consumer
- **On-chain proof**: Consumer proves data validity via ZK

The push model is preferred when:
- Data is time-sensitive (prices, scores)
- Multiple consumers may need the same data
- Audit trail of data values is important

## Architecture

### Oracle Registration

Oracle operators register their data feed:
```rust
pub struct RegisterOracleParamsV1 {
    pub proof: Vec<u8>,
    pub oracle_id: OracleId,
    pub oracle_commitment: pallas::Base,   // H(4, oracle_secret, oracle_id)
    pub name: String,
    pub data_type: String,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}
```

`oracle_commitment` is a **hiding commitment** to the operator's secret, not a public key. It was
`oracle_pubkey: PublicKey` before OBL-Z9/Z10; that disclosed a static operator identity, which is the
correlation anti-pattern `darkwow-address-model` exists to prevent, and it authorized nothing,
because each later circuit compared a point it derived itself against a witness it never exposed.
The commitment is re-derived in every operation and compared against this record.

### Value Updates

Oracle pushes new values:
```rust
pub struct PushValueParamsV1 {
    pub proof: Vec<u8>,
    pub oracle_id: OracleId,
    pub oracle_commitment: pallas::Base,   // must equal the registered record
    pub value: pallas::Base,
    pub nullifier: pallas::Base,           // H(1, oracle_secret, oracle_id, value)
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}
```

### Attestation Creation

Oracle creates attestations for specific values:
```rust
pub struct AttestValueParamsV1 {
    pub proof: Vec<u8>,
    pub oracle_id: OracleId,
    pub oracle_commitment: pallas::Base,
    pub attestation_id: AttestationId,
    pub predicate: u8,      // 0=Matches, 1=GreaterOrEqual, 2=LessOrEqual
    pub threshold: pallas::Base,
    pub nullifier: pallas::Base,           // H(1, oracle_secret, oracle_id, attestation_id)
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}
```

### Authorization and replay

Every operation that changes oracle state carries `oracle_commitment`, re-derived in-circuit from
the operator secret the prover holds. The host compares it against the stored record and returns
`NotAuthorized` on a mismatch — so a valid proof by a non-operator is refused. The four
push/attest operations additionally expose a **nullifier** derived from the same secret and bound to
the operation's own payload; the host checks it unspent (`DuplicateNullifier`) and marks it spent in
apply. `set_oracle_active` carries the commitment but no nullifier: a toggle has to stay repeatable.

Before this (OBL-Z9/Z10), `push_value_v1` looked the oracle up, checked `is_active`, and assigned
`params.value` — nothing else — and `set_oracle_active_v1` compared a caller-supplied copy of a
*public* key against the stored one, so anyone could set any feed's value and deactivate any feed.

## Integration with Attestation

The oracle contract integrates with the [Attestation Contract](./attestation.md) for verifiable data claims:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Oracle + Attestation Flow                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Oracle Operator                                                             │
│     │                                                                       │
│     │  PushValue(current_price)                                             │
│     ▼                                                                       │
│  Oracle(value=50000)                                                        │
│     │                                                                       │
│     │  AttestValue(GreaterOrEqual, 45000)                                   │
│     ▼                                                                       │
│  Attestation(claim_data=[50000])                                            │
│     │                                                                       │
│     │ attestation_id                                                        │
│     │                                                                       │
│     │◄────────────────────── Consumer Contract                             │
│     │                                                                       │
│     │                              CreateClaim(evidence)                    │
│     │                              (e.g., poseidon_hash(50000))            │
│     ▼                                                                       │
│  Claim(Verified)                                                            │
│     │                                                                       │
│     │ ConsumeClaim()                                                       │
│     ▼                                                                       │
│  Contract Executes                                                          │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `RegisterOracleV1` | 0x00 | Register a new oracle operator |
| `PushValueV1` | 0x01 | Push a data value on-chain |
| `AttestValueV1` | 0x02 | Create an attestation with predicate |
| `PushValueCommitmentV1` | 0x03 | Push a committed value (reveal later) |
| `AggregateV1` | 0x04 | Aggregate multiple oracle values |
| `SetOracleActiveV1` | 0x05 | Activate or deactivate an oracle operator (ZK since OBL-Z10) |

## ZK Circuits

All 6 circuits compiled to `.zk.bin`:

| Circuit | Purpose | Instances |
|---------|---------|-----------|
| `register_oracle.zk` | Prove registration, bind the record to the secret | oracle_id, oracle_commitment, tx_binding, tx_nonce |
| `push_value.zk` | Prove value push authorization + consume a nullifier | oracle_id, oracle_commitment, value, nullifier, tx_binding, tx_nonce |
| `attest_value.zk` | Prove attestation creation + consume a nullifier | oracle_id, oracle_commitment, attestation_id, predicate, threshold, nullifier, tx_binding, tx_nonce |
| `push_value_commitment.zk` | Prove commitment to value (reveal later) | oracle_id, oracle_commitment, commitment, nullifier, tx_binding, tx_nonce |
| `aggregate.zk` | Prove aggregated value + consume a nullifier | oracle_id, oracle_commitment, result, min_result, max_result, nullifier, tx_binding, tx_nonce |
| `set_oracle_active.zk` | Prove the toggler is the operator (added for OBL-Z10 — the function had no circuit at all before) | oracle_id, oracle_commitment, is_active, tx_binding, tx_nonce |

`DOMAIN_OPERATOR_COMMITMENT = witness_base(4)` (the capability-commitment slot) and
`DOMAIN_NULLIFIER = witness_base(1)`, both from the fixed domain table in
`doc/src/arch/circuit-versioning.md`.

## Use Cases

### 1. DeFi Price Feeds

```rust
// Oracle pushes BTC price
oracle.push_value(50000);  // BTC/USD

// Oracle creates attestation for liquidation threshold
oracle.attest_value(Predicate::LessOrEqual, 45000);

// Stablecoin contract consumes attestation
let claim = attestation.create_claim(attestation_id, Predicate::LessOrEqual, poseidon_hash(current_price));
if claim.verified {
    liquidate_position(borrower);
}
```

### 2. Prediction Markets

```rust
// Oracle pushes game outcome
oracle.push_value(team_a_wins);

// Oracle creates attestation for result
oracle.attest_value(Predicate::Matches, team_a_wins);

// Prediction market consumes
let claim = attestation.create_claim(attestation_id, Predicate::Matches, poseidon_hash(result));
if claim.verified {
    settle_bets(winning_positions);
}
```

### 3. Gaming Randomness

```rust
// Oracle commits to random value
oracle.push_value(commit_random(secret));

// Oracle reveals and creates attestation
oracle.attest_value(Predicate::Matches, committed_value);

// Game contract consumes for fair randomness
let claim = attestation.create_claim(attestation_id, Predicate::Matches, poseidon_hash(random));
if claim.verified {
    mint_nft(random_trait);
}
```

## Security Model

| Trust Assumption | Mitigation |
|-----------------|------------|
| Oracle operator provides accurate data | **Not enforced and not enforceable** — the contract binds *who* may write, not whether what they write is true. Use multiple oracle sources and audit trails |
| Only the registered operator writes | The operator commitment, re-derived in-circuit and compared against the stored record (`NotAuthorized`) |
| An operation is not replayed | Per-operation nullifier, checked unspent (`DuplicateNullifier`) |
| Data is timely | Check `updated_at` timestamp |
| Oracle doesn't double-attest | The operation's nullifier, bound to `attestation_id` |
| Predicate logic is correct | Attestation contract audits predicate |

### Signature Verification Limitations

The Oracle contract provides a framework for oracle operators to push values and create
attestations. However, **signature verification is BYPASSED in-circuit** in consuming contracts:

- [DarkBet Exchange](darkbet_exchange.md): AMM-based binary outcome markets accept oracle resolution for event settlement
- [Insurance Market](insurance_market.md): Claims accept oracle resolution but do not verify the oracle's signature

**Security limitation**: The `oracle_signature` field is stored but the public key is never used to
cryptographically validate the signature within the ZK circuit.

One claim that used to stand here is now false and is corrected rather than deleted, because the
difference matters to anyone reading the older text: it said *"any value can be pushed regardless of
whether the submitter holds the oracle's private key."* That was true of **this** contract and is no
longer. As of OBL-Z9/Z10 an operation is authorized by the registered operator's commitment —
re-derived in-circuit from a secret only the operator holds, and compared against the stored record —
so a value cannot be pushed, and a feed cannot be deactivated, by anyone else. What remains is the
*other* half: this contract proves **who** wrote a datum, and nothing anywhere proves the datum is
**true**, because the operator is the only party who can attest to an off-chain fact. Authority is
cryptographic; accuracy is a trust assumption about the operator, and no opcode can change that.

The consuming contracts' limitation is separate and still open: `darkbet_exchange` and
`insurance_market` accept an `oracle_signature` they never verify, so a consumer that reads a
signature field rather than this contract's own record is trusting its caller.

**Required**: A `SchnorrVerify` opcode is needed for proper on-chain signature verification inside
ZK circuits. This is tracked in the [Security Analysis](../arch/security-analysis.md).

**Future**: When `SchnorrVerify` is implemented, circuits will be able to:
```zk
# In ZK circuit:
is_valid = schnorr_verify(oracle_commitment, message, signature);
constrain_instance(is_valid);
```

## File Structure

```
src/contract/oracle/
├── proof/
│   ├── register_oracle.zk
│   ├── push_value.zk
│   ├── push_value_commitment.zk
│   ├── attest_value.zk
│   ├── aggregate.zk
│   └── set_oracle_active.zk
├── src/
│   ├── lib.rs
│   ├── entrypoint.rs
│   ├── model/mod.rs
│   ├── error.rs
│   └── client/mod.rs
└── README.md
```

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis


- [Attestation Contract](./attestation.md) - Generalized attestation and claims
- [Stablecoin Contract](./stablecoin.md) - Uses oracle for liquidation
- [Labor Market Contract](./labor_market.md) - Uses attestation for deliverable verification
