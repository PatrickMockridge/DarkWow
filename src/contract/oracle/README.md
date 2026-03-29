# Oracle Contract

Demonstrates the "push model" for oracles in DarkFi. Oracles push data values (prices, scores, weather, etc.) that can be attested for consumption by other contracts.

## Overview

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

## How It Works

### 1. Oracle Registration

Oracle operator registers a data feed:
```rust
let params = RegisterOracleParamsV1 {
    oracle_id: poseidon_hash(operator_pubkey, name),
    oracle_pub_x,
    oracle_pub_y,
    name: "BTC/USD".to_string(),
    data_type: "price".to_string(),
};
```

### 2. Value Updates

Oracle pushes values to their feed:
```rust
let params = PushValueParamsV1 {
    oracle_id,
    value: current_btc_price,
};
```

### 3. Attestation Creation

Oracle creates an attestation for a specific value:
```rust
let params = AttestValueParamsV1 {
    oracle_id,
    attestation_id: poseidon_hash(oracle_id, predicate, threshold),
    predicate: 1,  // GreaterOrEqual
    threshold: pallas::Base::from(50000),
};
```

### 4. Contract Consumption

Other contracts consume the attestation via attestation contract:
```rust
// Stablecoin liquidation using oracle attestation
let claim_id = attestation::create_claim(
    attestation_id,
    claimant: liquidator_pubkey,
    predicate: Predicate::GreaterOrEqual,
    evidence_commitment: poseidon_hash(current_price),
)?;

// If price < collateral_value, liquidate
if verified && price < threshold {
    liquidate_position(market, borrower)?;
}
```

## Data Structures

### Oracle

```rust
pub struct Oracle {
    pub id: OracleId,
    pub oracle_pubkey: PublicKey,
    pub name: String,
    pub data_type: String,
    pub value: pallas::Base,
    pub updated_at: u64,
    pub is_active: bool,
}
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `RegisterOracleV1` | 0x00 | Register a new oracle data feed |
| `PushValueV1` | 0x01 | Push a new value to the feed |
| `AttestValueV1` | 0x02 | Create attestation for current value |

## ZK Circuits

| Circuit | Purpose | Key Opcodes |
|---------|---------|-------------|
| `register_oracle_v1.zk` | Prove oracle registration | `ec_mul_base`, `constrain_instance` |
| `push_value_v1.zk` | Prove value push authorization | `ec_mul_base`, `constrain_equal_base` |
| `attest_value_v1.zk` | Prove attestation creation | `ec_mul_base`, `constrain_equal_base` |

All circuits use **proven opcodes only**.

## Use Cases

### Price Feeds (DeFi)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Price Feed Oracle Flow                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Price Oracle                                                               │
│     │                                                                       │
│     │  PushValue(50000)  // BTC/USD price                                   │
│     │  PushValue(50000)                                                     │
│     │  PushValue(49500)  // Price drops                                     │
│     │                                                                       │
│     │  AttestValue(GreaterOrEqual, 45000)  // Liquidation threshold        │
│     ▼                                                                       │
│  Attestation                                                                │
│                                                                              │
│                                              Stablecoin Contract             │
│                                                 │                            │
│                                                 │ CreateClaim(evidence)      │
│                                                 ▼                            │
│                                              Claim(Verified)                 │
│                                                 │                            │
│                                                 │ consume_claim()            │
│                                                 ▼                            │
│                                              If collateral_ratio < 1.5:     │
│                                                 liquidate()                  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Sports Betting

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Sports Betting Oracle Flow                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Sports Oracle                                                              │
│     │                                                                       │
│     │  PushValue(team_a_wins)                                               │
│     │                                                                       │
│     │  AttestValue(Matches, team_a_wins)                                   │
│     ▼                                                                       │
│  Attestation                                                                │
│                                                                              │
│                                              Prediction Market               │
│                                                 │                            │
│                                                 │ CreateClaim(winner)        │
│                                                 ▼                            │
│                                              Claim(Verified)                 │
│                                                 │                            │
│                                                 │ settle_bet()               │
│                                                 ▼                            │
│                                              Payout to winners               │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Randomness Beacon

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Randomness Oracle Flow                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Randomness Oracle                                                          │
│     │                                                                       │
│     │  PushValue(generate_random())                                         │
│     │                                                                       │
│     │  AttestValue(Matches, committed_value)                                │
│     ▼                                                                       │
│  Attestation                                                                │
│                                                                              │
│                                              NFT Game                        │
│                                                 │                            │
│                                                 │ CreateClaim(randomness)   │
│                                                 ▼                            │
│                                              Claim(Verified)                 │
│                                                 │                            │
│                                                 │ mint_nft()                 │
│                                                 ▼                            │
│                                              Fair NFT minting               │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Composability

```
Oracle Contract
├── Uses Attestation for verifiable data claims
├── Integrates with any contract needing external data
└── Examples:
    ├── Stablecoin (price liquidation)
    ├── Prediction markets (outcome attestation)
    ├── Gaming (randomness beacon)
    ├── Insurance (event attestation)
```

## Security Considerations

1. **Oracle operator trust**: Users must trust oracle operators to provide accurate data
2. **Attestation verification**: Consumers should verify attestations before use
3. **Multiple oracles**: For critical data, use multiple oracle sources
4. **Timeliness**: Consumers should check `updated_at` to ensure data freshness

## See Also

- [Attestation Contract](../attestation/README.md) - Generalized attestation and claims
- [Stablecoin Contract](../stablecoin/README.md) - Uses oracle for liquidation
