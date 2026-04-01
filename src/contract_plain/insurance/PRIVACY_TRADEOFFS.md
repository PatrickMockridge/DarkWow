# Privacy Tradeoffs: insurance_plain

## What This Contract Gives Up

This is a **"partial transparency"** alternative to a hypothetical ZK insurance contract.
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
| `base_div` | ❌ Not implemented | N/A - needed for premium/claim ratios |
| `less_than_or_equal` | ❌ Unsound bug | **NO - would allow false proofs** |

**We prefer plain over ZK-with-unsound-opcodes.** A visible bug is fixable; an invisible theft is catastrophic.

## What insurance_plain Enables

A hypothetical ZK insurance contract would be limited by:
- Cannot do actuarial premium calculations (need division)
- Cannot express coverage ratios properly
- Claims verification constrained by circuit expressiveness

The plain version enables:
- **Actuarial premium calculation**: Risk-based pricing using historical data
- **Coverage ratio verification**: Actual `verified_loss / claimed_loss` ratios
- **Pool capital tracking**: Real-time pool solvency monitoring
- **Claims processing**: Arbitrary verification logic with ZK soundness for key parts

## Data Visibility

**Visible on-chain:**
- All policy details (policyholder, coverage amounts, premiums paid)
- Claims filed and their amounts
- Pool capital and reserves
- Risk assessments and actuarial data

**NOT visible:**
- Personal health/property details (stored off-chain, only hash on-chain)
- Specific insurance company operations
- Internal risk models

## Opcode Dependencies

| Opcode | Status | Fallback | Impact |
|--------|--------|----------|--------|
| `base_div` | NOT IMPLEMENTED | Cross-multiplication workaround | Premium ratios visible on-chain |
| `less_than_or_equal` | Unsound bug | Cross-multiplication | Claim verification visible |

## Claims Ratio Check Example

```rust
// PRIVACY: Claim ratios are visible on-chain.
// OPCODE PLACEHOLDER: When less_than_or_equal is sound, could add ZK proof
// that verified_loss >= claimed_loss * coverage_ratio without revealing exact values.

pub fn verify_claim_ratio(
    verified_loss: u64,
    claimed_loss: u64,
    coverage_ratio: u64,  // e.g., 8000 = 80%
) -> bool {
    // Cross-multiplication to avoid division: a/b < c  <=>  a < b*c
    verified_loss * 10000 >= claimed_loss * coverage_ratio
}
```

## Future ZK Enhancement Path

When `base_div` is implemented in the ZKVM, this contract's logic could be ported back to ZK:

1. Replace premium division with ZK-verified division
2. Keep individual risk factors private where possible
3. Use ZK for claims verification where sound
4. Maintain pool transparency through commitments