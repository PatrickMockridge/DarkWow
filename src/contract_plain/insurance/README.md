# Insurance Plain Contract

> **DEPRECATED**: Use `darkfi_insurance_market_contract` in `../../contract/insurance_market/` instead.
>
> ZK opcodes `base_div` and `less_than_or_equal` are now sound and implemented.

---

A **partial transparency** alternative to a hypothetical ZK insurance contract. It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.

## Why This Contract Exists (Historical)

A hypothetical ZK insurance contract would be constrained by missing `base_div` opcode:
- Cannot do actuarial premium calculations (need division)
- Cannot express coverage ratios properly
- Claims verification constrained by circuit expressiveness

See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation of privacy tradeoffs.

## Key Features

| Feature | ZK Version | Plain Version | Privacy Impact |
|---------|-----------|---------------|----------------|
| Premium calculation | Limited | Full actuarial | Premium ratios visible |
| Coverage verification | Circuit-limited | Full expression | Verification visible |
| Claims processing | Simple | Arbitrary logic | Claims data visible |
| Pool tracking | Limited | Full transparency | Pool data public |

## Architecture

This contract uses a **hybrid ZK/plain approach**:

| Operation | Method | Why |
|-----------|--------|-----|
| Signature verification | ZK (Schnorr) | Sound, constrainable |
| Policy commitment | ZK (Pedersen) | Privacy-preserving |
| Premium calculation | Native Rust | Needs `base_div` (not in ZK) |
| Claims verification | Hybrid | ZK for sound parts, plain for complex |

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
- All policy details (policyholder, coverage amounts, premiums paid)
- Claims filed and their amounts
- Pool capital and reserves
- Risk assessments and actuarial data

**NOT visible:**
- Personal health/property details (stored off-chain, only hash on-chain)
- Specific insurance company operations
- Internal risk models

## Policy State Machine

```
Created → Active → Expired
               ↘ Claimed → Approved → Paid
                      ↘ Rejected (back to Active)
               ↘ Cancelled
```

## Opcode Dependencies

| Opcode | Status | Plain Fallback |
|--------|--------|----------------|
| `base_div` | **IMPLEMENTED** (0x58) | Cross-multiplication workaround (privacy tradeoff: results visible) |
| `less_than_or_equal` | **Verified Sound** | Cross-multiplication |

## ZK Enhancement Path

`base_div` is now implemented and `less_than_or_equal` is verified sound. Migration to ZK is possible:

1. Replace premium division with ZK-verified division
2. Keep individual risk factors private where possible
3. Use ZK for claims verification where sound
4. Maintain pool transparency through commitments

## File Structure

```
src/contract_plain/insurance/
├── Cargo.toml
├── PRIVACY_TRADEOFFS.md
└── src/
    ├── lib.rs              # Function enum
    ├── error.rs            # Error types
    ├── model/mod.rs        # Policy, Claim, state machine, params, updates
    └── entrypoint.rs       # init, exec, update
```

## Compilation

```bash
cargo check -p darkfi_plain_insurance_contract
```

## Related Documentation

- [Plain Contracts Architecture](../../doc/src/arch/plain_contracts.md) - Dual-layer contract architecture
- [Composability](../../doc/src/arch/composability.md) - Cross-contract patterns
- [Parallel Societies](../../doc/src/arch/parallel_societies.md) - Privacy for social reproduction