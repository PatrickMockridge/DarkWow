# Privacy Tradeoffs: subscription_plain

## What This Contract Gives Up

This is a **"partial transparency"** alternative to the ZK `subscription` contract.
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
- `less_than_or_equal` bug allows prover to choose `out=0` bypassing intended logic
- Result: **undetectable theft** while the system thinks verification passed

### Our Approach

This plain contract:
- Uses **native Rust** for operations where ZK opcodes are unsound
- Uses **ZK (Schnorr signatures)** where opcodes are sound
- Documents every place where we chose plain over broken-ZK

### Opcode Soundness Status

| Opcode | Status | Can Use in ZK? |
|--------|--------|-----------------|
| `EcAdd` | ✅ Sound | Yes |
| `EcMul` | ✅ Sound | Yes |
| `PoseidonHash` | ✅ Sound | Yes |
| `SchnorrVerify` | ✅ Sound | Yes |
| `base_div` | ✅ **Implemented** | N/A | Available in ZKVM (0x58) |
| `less_than_or_equal` | ✅ **Verified Sound** | N/A | Available in ZKVM (0x55) |
| `is_equal_base` | ❌ Bug | **NO - delta_invert unconstrained** | Do not use |

## Opcode Dependencies

| Opcode | Status | Fallback | Impact |
|--------|--------|----------|--------|
| `base_div` | **IMPLEMENTED** (0x58) | Native Rust division | Results visible on-chain |

## Data Visibility

**Visible on-chain:**
- All subscription tier bitmasks (full ACCESS_READ, ACCESS_WRITE, ACCESS_ADMIN values)
- Rate limiting ratios (uses_allowed / period)
- Subscription durations and expiry blocks
- All subscription/unsubscription events
- Payment amounts

**NOT visible:**
- Actual content accessed (if routed through encrypted channels outside this contract)
- Specific service being subscribed to (if referenced by ID only)

## Why This Tradeoff

The ZK version of subscription is blocked by missing `base_div` opcode for true bitmask checking.
Currently只能 use a tiered linear approach (tier 1 < tier 2 < tier 3).

This plain version enables:
- Arbitrary permission combinations (READ+WRITE without ADMIN)
- Proper ratio-based rate limiting
- More expressive subscription tiers

## Future ZK Enhancement Path

When `base_div` is implemented in the ZKVM, this contract's logic could be ported back to ZK:

1. Replace native `&` bitmask with ZK constraint
2. Replace division with ZK-verified division
3. Keep subscription commitments private

## OPCODE PLACEHOLDERS

### base_div (IMPLEMENTED - 0x58)
```rust
// PRIVACY: In ZK version, division result would be constrained, not revealed.
// Currently: Native Rust division reveals result on-chain.
pub fn calculate_rate_limit(uses: u64, period: u64) -> u64 {
    uses / period  // DIVISION - visible on-chain
}
// ZK VERSION: Use base_div(a, b) opcode to compute ratio privately
```

### less_than_or_equal (VERIFIED SOUND)
```rust
// PRIVACY: In ZK version with sound less_than_or_equal, comparison result
// could be constrained without revealing the actual values.
// Currently: Cross-multiplication workaround visible.
```
