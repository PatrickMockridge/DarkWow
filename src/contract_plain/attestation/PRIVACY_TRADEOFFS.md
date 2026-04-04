# Privacy Tradeoffs: attestation_plain

## What This Contract Gives Up

This is a **"partial transparency"** alternative to the ZK `attestation` contract.
It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.

## Security Comparison: Why Plain Can Be Safer Than Unsound ZK

### The Danger of Unsound ZK Opcodes

**ZK proofs are more dangerous when broken than plain code.**

With a plain contract:
- Incorrect behavior is **visible** on-chain
- Anyone can see if credentials are being misused
- Attackers must act in plain sight

With a ZK contract using unsound opcodes:
- A **malicious proof** can appear valid while circumventing credentials
- The verifier believes the proof is sound when it's not
- Result: **undetectable credential fraud** while the system thinks verification passed

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
| `less_than_or_equal` | ✅ **Verified Sound** | N/A - available in ZKVM (0x55) |

**We prefer plain over ZK-with-unsound-opcodes.** A visible bug is fixable; an invisible credential fraud is catastrophic.

## What attestation_plain Enables

The existing ZK attestation contract has limitations:
- Simple attestation verification
- No delegation chains
- Limited credential depth
- Cannot express complex credential graphs

The plain version enables:
- **Hierarchical credential verification**: Multi-level delegations
- **Cross-reference checking**: Attestors can reference each other
- **Delegation chains with depth limits**: Controlled credential propagation
- **Time-bounded credentials**: Expiry ratios and validity periods

## Data Visibility

**Visible on-chain:**
- All attestation schemas
- Credential chains and delegation paths
- Delegation ratios and depth limits
- Revocation status

**NOT visible:**
- Specific credential content (stored off-chain, only hash)
- Personal details in credentials
- Internal attestation policies

## Opcode Dependencies

| Opcode | Status | Fallback | Impact |
|--------|--------|----------|--------|
| `base_div` | NOT IMPLEMENTED | Native division | Delegation ratios visible |
| `less_than_or_equal` | Unsound bug | Cross-multiplication | Credential depth checks visible |

## Delegation Ratio Example

```rust
// PRIVACY: Delegation ratios are visible on-chain.
// OPCODE PLACEHOLDER: When base_div is in ZK, could use ZK proofs
// to verify delegation ratios without revealing exact values.

pub fn verify_delegation_ratio(
    delegator_stake: u64,
    delegatee_stake: u64,
    max_ratio: u64,  // e.g., 10000 = 100%
) -> bool {
    // Cross-multiplication to avoid division: a/b < c  <=>  a < b*c
    delegator_stake * 10000 <= delegatee_stake * max_ratio
}
```

## Future ZK Enhancement Path

When `base_div` is implemented in the ZKVM:

1. Replace delegation ratio checks with ZK constraints
2. Keep credential chains private through commitments
3. Use ZK for attestation verification where sound
4. Maintain revocation transparency through ZK proofs