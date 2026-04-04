# Privacy Tradeoffs: oracle_plain

## What This Contract Gives Up

This is a **"partial transparency"** alternative to a hypothetical ZK oracle contract.
It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.

## Security Comparison: Why Plain Can Be Safer Than Unsound ZK

### The Danger of Unsound ZK Opcodes

**ZK proofs are more dangerous when broken than plain code.**

With a plain contract:
- Incorrect behavior is **visible** on-chain
- Anyone can see if funds are being stolen
- Attackers must act in plain sight

With a ZK contract using unsound opcodes:
- A **malicious proof** can appear valid while stealing funds
- The verifier believes the proof is sound when it's not
- Result: **undetectable theft** while the system thinks verification passed

### Our Approach

This plain contract:
- Uses **native Rust** for operations where ZK opcodes are unsound
- Uses **ZK (Schnorr signatures)** where opcodes are sound
- Documents every place where we chose plain over broken-ZK

## Opcode Soundness Status

| Opcode | Status | Can Use in ZK? |
|--------|--------|----------------|
| `EcAdd` | ✅ Sound | Yes |
| `EcMul` | ✅ Sound | Yes |
| `PoseidonHash` | ✅ Sound | Yes |
| `SchnorrVerify` | ✅ Sound | Yes |
| `base_div` | ✅ **Implemented** | N/A - available in ZKVM (0x58) |
| `set_membership` | ❌ Not implemented | N/A - needed for data point proofs |

**We prefer plain over ZK-with-unsound-opcodes.** A visible bug is fixable; an invisible theft is catastrophic.

## What oracle_plain Enables

A hypothetical ZK oracle contract would be limited by:
- Cannot do weighted average calculations (need division)
- Cannot prove data point inclusion without revealing it
- Limited aggregation logic due to circuit expressiveness

The plain version enables:
- **Weighted average aggregation**: Multiple data sources with weights
- **Slashable staking**: Stakers put up collateral that can be slashed for incorrect data
- **Confidence scoring**: Data sources rated by accuracy
- **Arbitrary aggregation**: Any aggregation function (median, mean, mode, etc.)

## Data Visibility

**Visible on-chain:**
- All data points submitted by stakers
- Staking amounts and weights
- Aggregated results
- Slash events and penalties

**NOT visible:**
- Specific staker identities (if using pseudonyms)
- Internal data source methodologies
- Detailed accuracy metrics

## Opcode Dependencies

| Opcode | Status | Fallback | Impact |
|--------|--------|----------|--------|
| `base_div` | NOT IMPLEMENTED | Native division | Weighted averages visible on-chain |
| `set_membership` | NOT IMPLEMENTED | Direct inclusion check | Data point inclusion visible |

## Weighted Average Example

```rust
// PRIVACY: All data points and weights are visible on-chain.
// OPCODE PLACEHOLDER: When set_membership is available, could prove
// data point inclusion without revealing the point itself.

pub fn weighted_average(
    data_points: Vec<(u64, u64)>,  // (value, weight)
) -> u64 {
    let total_weight: u64 = data_points.iter().map(|(_, w)| w).sum();
    let weighted_sum: u64 = data_points.iter().map(|(v, w)| v * w).sum();

    // DIVISION: Could reveal median instead of weighted average for privacy
    // Privacy tradeoff: weighted average is more useful but less private
    weighted_sum / total_weight
}
```

## Future ZK Enhancement Path

When `base_div` and `set_membership` are implemented in the ZKVM:

1. Keep individual data points private through commitments
2. Prove data point inclusion without revealing values
3. Use ZK for aggregation verification
4. Maintain slashing accountability through ZK proofs