# Privacy Tradeoffs: labor_market_plain

## What This Contract Gives Up

This is a **"partial transparency"** alternative to the ZK `labor_market` contract.
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
|--------|--------|-----------------|
| `EcAdd` | ✅ Sound | Yes |
| `EcMul` | ✅ Sound | Yes |
| `PoseidonHash` | ✅ Sound | Yes |
| `SchnorrVerify` | ✅ Sound | Yes |
| `base_div` | ✅ **Implemented** | N/A - available in ZKVM (0x58) |
| `less_than_or_equal` | ✅ **Verified Sound** | N/A - available in ZKVM (0x55) |

**We prefer plain over ZK-with-unsound-opcodes.** A visible bug is fixable; an invisible theft is catastrophic.

## What labor_market_plain Enables

The ZK labor_market contract has limitations:
- Simple escrow (payment released on milestone OR timeout)
- No time-weighted release (partial payment for partial work)
- No milestone chains
- Complex delivery verification constrained by circuit expressiveness

The plain version enables:
- **Time-weighted release**: Partial payment for partial work based on elapsed time
- **Milestone chains**: Multi-stage deliverables with sequential release
- **Flexible verification**: Arbitrary verification logic (ZK circuits for sound parts)
- **Ratio-based calculations**: Actual `uses / total` ratios visible on-chain

## Data Visibility

**Visible on-chain:**
- All job details (employer, worker, payment amounts)
- Milestone progress and completion
- Time elapsed and release calculations
- Dispute filings and resolutions

**NOT visible:**
- Actual work content (stored off-chain, only hash on-chain)
- Communication between parties
- Specific deliverable content

## Opcode Dependencies

| Opcode | Status | Fallback | Impact |
|--------|--------|----------|--------|
| `base_div` | NOT IMPLEMENTED | Native Rust division | Time ratios visible on-chain |
| `less_than_or_equal` | Unsound bug | Cross-multiplication workaround | Block comparisons visible |

## Time-Weighted Release Example

```rust
// PRIVACY: In ZK version with sound base_div, time ratios would be constrained.
// Currently: Native Rust division reveals result on-chain.

pub fn calculate_partial_release(
    total_payment: u64,
    elapsed_blocks: u64,
    total_blocks: u64,
) -> u64 {
    // Proportional release based on time elapsed
    // Privacy tradeoff: Both elapsed and total blocks are visible
    total_payment * elapsed_blocks / total_blocks
}
```

## Future ZK Enhancement Path

When `base_div` is implemented in the ZKVM, this contract's logic could be ported back to ZK:

1. Replace time-weighted division with ZK-verified division
2. Keep payment amounts and milestone data private where possible
3. Use ZK for Schnorr signature verification
4. Maintain milestone chains with ZK constraints