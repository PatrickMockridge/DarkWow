# Oracle Plain Contract

A **partial transparency** alternative to a hypothetical ZK oracle contract. It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.

## Why This Contract Exists

A hypothetical ZK oracle contract would be constrained by missing `base_div` and `set_membership` opcodes:
- Cannot do weighted average calculations (need division)
- Cannot prove data point inclusion without revealing it
- Limited aggregation logic due to circuit expressiveness

See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation of privacy tradeoffs.

## Key Features

| Feature | ZK Version | Plain Version | Privacy Impact |
|---------|-----------|---------------|----------------|
| Data aggregation | Limited | Full expression | All data visible |
| Weighted averages | Circuit-limited | Full support | Weights visible |
| Slash verification | Simple | Arbitrary logic | Slash data visible |
| Confidence scoring | Limited | Full support | Scores visible |

## Architecture

This contract uses a **hybrid ZK/plain approach**:

| Operation | Method | Why |
|-----------|--------|-----|
| Signature verification | ZK (Schnorr) | Sound, constrainable |
| Data commitment | ZK (Pedersen) | Privacy-preserving |
| Weighted average | Native Rust | Needs `base_div` (not in ZK) |
| Aggregation logic | Native Rust | Arbitrary complexity |

## Security Principle

> **We prefer plain over ZK-with-unsound-opcodes.**

### The Danger of Unsound ZK Opcodes

With a **plain contract**:
- Incorrect behavior is **visible** on-chain
- Anyone can see if funds are being stolen
- Attackers must act in plain sight

With a **ZK contract using unsound opcodes**:
- A **malicious proof** can appear valid while stealing funds
- Bug in `less_than_or_equal` allows prover to bypass intended logic
- Result: **undetectable theft** while the system thinks verification passed

See [Opcode Soundness Status](PRIVACY_TRADEOFFS.md#opcode-soundness-status) for details.

## Privacy Tradeoffs

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

| Opcode | Status | Plain Fallback |
|--------|--------|----------------|
| `base_div` | NOT IMPLEMENTED | Native division (privacy tradeoff: results visible) |
| `set_membership` | NOT IMPLEMENTED | Direct inclusion check |

## Future ZK Enhancement Path

When `base_div` and `set_membership` are implemented in the ZKVM:

1. Keep individual data points private through commitments
2. Prove data point inclusion without revealing values
3. Use ZK for aggregation verification
4. Maintain slashing accountability through ZK proofs

## File Structure

```
src/contract_plain/oracle/
├── Cargo.toml
├── PRIVACY_TRADEOFFS.md
└── src/
    ├── lib.rs              # Function enum
    ├── error.rs            # Error types
    ├── model/mod.rs        # DataPoint, Staker, params, updates
    └── entrypoint.rs       # init, exec, update
```

## Compilation

```bash
cargo check -p darkfi_plain_oracle_contract
```

## Related Documentation

- [Plain Contracts Architecture](../../doc/src/arch/plain_contracts.md) - Dual-layer contract architecture
- [Composability](../../doc/src/arch/composability.md) - Cross-contract patterns
- [Parallel Societies](../../doc/src/arch/parallel_societies.md) - Privacy for social reproduction