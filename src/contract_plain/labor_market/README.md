# Labor Market Plain Contract

> **DEPRECATED**: Use `darkfi_labor_market_contract` in `../../contract/labor_market/` instead.
>
> ZK opcodes `base_div` and `less_than_or_equal` are now sound and implemented.

---

A **partial transparency** alternative to the ZK `labor_market` contract. It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.

## Why This Contract Exists (Historical)

The existing ZK `labor_market` contract is constrained by missing `base_div` opcode:
- Cannot do time-weighted partial payment release
- Cannot express milestone chains with complex dependencies
- Complex delivery verification limited by circuit expressiveness

See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation of privacy tradeoffs.

## Key Features

| Feature | ZK Version | Plain Version | Privacy Impact |
|---------|-----------|---------------|----------------|
| Time-weighted release | Not available | Native division | Timing ratios visible |
| Milestone chains | Limited | Full support | All milestones public |
| Payment tracking | Hidden in commitments | Public on-chain | All amounts visible |
| Delivery verification | Circuit-limited | Arbitrary logic | Verification public |

## Architecture

This contract uses a **hybrid ZK/plain approach**:

| Operation | Method | Why |
|-----------|--------|-----|
| Signature verification | ZK (Schnorr) | Sound, constrainable |
| Payment commitment | ZK (Pedersen) | Privacy-preserving |
| Time-weighted release | Native Rust | Needs `base_div` (not in ZK) |
| Milestone progress | Native Rust | Arbitrary logic |
| Delivery verification | Hybrid | ZK for sound parts, plain for complex |

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
- All job details (employer, worker, payment amounts)
- Milestone progress and completion
- Time elapsed and release calculations
- Dispute filings and resolutions

**NOT visible:**
- Actual work content (stored off-chain, only hash on-chain)
- Communication between parties
- Specific deliverable content

See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation.

## Job State Machine

```
Created → InProgress → Delivered → Confirmed
                    ↘ Dispute ↗
         Cancelled (before acceptance)
         Refunded (after timeout)
```

## Opcode Dependencies

| Opcode | Status | Plain Fallback |
|--------|--------|----------------|
| `base_div` | **IMPLEMENTED** (0x58) | Native Rust division (privacy tradeoff: results visible) |
| `less_than_or_equal` | **Verified Sound** | Cross-multiplication workaround |

## ZK Enhancement Path

`base_div` is now implemented and `less_than_or_equal` is verified sound. Migration to ZK is possible:

1. Replace time-weighted division with ZK-verified division
2. Keep payment amounts and milestone data private where possible
3. Use ZK for Schnorr signature verification
4. Maintain milestone chains with ZK constraints

## File Structure

```
src/contract_plain/labor_market/
├── Cargo.toml
├── PRIVACY_TRADEOFFS.md
└── src/
    ├── lib.rs              # Function enum
    ├── error.rs            # Error types
    ├── model/mod.rs        # Job, Milestone, state machine, params, updates
    └── entrypoint.rs       # init, exec, update
```

## Compilation

```bash
cargo check -p darkfi_plain_labor_market_contract
```

## Related Documentation

- [Plain Contracts Architecture](../../doc/src/arch/plain_contracts.md) - Dual-layer contract architecture
- [Composability](../../doc/src/arch/composability.md) - Cross-contract patterns
- [Parallel Societies](../../doc/src/arch/parallel_societies.md) - Privacy for social reproduction