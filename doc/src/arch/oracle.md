# Oracle Contract

A demonstration of the "push model" for oracles in DarkFi, enabling trustless external data integration with on-chain contracts.

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
    pub oracle_id: OracleId,
    pub oracle_pub_x: pallas::Base,
    pub oracle_pub_y: pallas::Base,
    pub name: String,
    pub data_type: String,
}
```

### Value Updates

Oracle pushes new values:
```rust
pub struct PushValueParamsV1 {
    pub oracle_id: OracleId,
    pub value: pallas::Base,
}
```

### Attestation Creation

Oracle creates attestations for specific values:
```rust
pub struct AttestValueParamsV1 {
    pub oracle_id: OracleId,
    pub attestation_id: AttestationId,
    pub predicate: u8,      // 0=Matches, 1=GreaterOrEqual, 2=LessOrEqual
    pub threshold: pallas::Base,
}
```

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

## ZK Circuits

| Circuit | Purpose | Public Inputs |
|---------|---------|--------------|
| `register_oracle_v1.zk` | Prove oracle registration | oracle_pub_x, oracle_pub_y |
| `push_value_v1.zk` | Prove value push authorization | oracle_id, value |
| `attest_value_v1.zk` | Prove attestation creation | oracle_id, attestation_id, predicate, threshold |

All circuits use **proven opcodes only**.

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
| Oracle operator provides accurate data | Use multiple oracle sources, audit trails |
| Data is timely | Check `updated_at` timestamp |
| Oracle doesn't double-attest | Attestation contract prevents replay |
| Predicate logic is correct | Attestation contract audits predicate |

## File Structure

```
src/contract/oracle/
├── proof/
│   ├── register_oracle_v1.zk
│   ├── push_value_v1.zk
│   └── attest_value_v1.zk
├── src/
│   ├── lib.rs
│   ├── entrypoint.rs
│   ├── model/mod.rs
│   ├── error.rs
│   └── client/mod.rs
└── README.md
```

## See Also

- [Attestation Contract](./attestation.md) - Generalized attestation and claims
- [Stablecoin Contract](./stablecoin.md) - Uses oracle for liquidation
- [Labor Market Contract](./labor_market.md) - Uses attestation for deliverable verification
