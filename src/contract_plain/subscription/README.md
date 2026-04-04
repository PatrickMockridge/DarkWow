# Subscription Plain Contract

A **partial transparency** alternative to the ZK [`subscription`](../contract/subscription.md) contract. It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.

## Why This Contract Exists

The existing ZK `subscription` contract is constrained by missing `base_div` opcode:
- Cannot do true bitmask access control (only tiered linear approach)
- Cannot express ratio-based rate limiting

See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation of privacy tradeoffs.

## Key Differences from ZK Version

| Feature | ZK Version | Plain Version | Privacy Impact |
|---------|-----------|---------------|----------------|
| Access control | Tiered linear (1<2<3) | True bitmask (READ+WRITE without ADMIN) | All permissions visible |
| Rate limiting | Simple counter | Ratio-based: `uses_allowed / period > threshold` | Rate ratios visible |
| Permissions | Hierarchical tiers | Arbitrary bitmask combinations | Bitmask values public |

## Architecture

This contract uses a **hybrid ZK/plain approach**:

| Operation | Method | Why |
|-----------|--------|-----|
| Signature verification | ZK (Schnorr) | Sound, constrainable |
| Subscription commitment | ZK (Poseidon) | Privacy-preserving for ID |
| Access bitmask check | Native Rust | Needs `base_div` (not in ZK) |
| Rate limit calculation | Native Rust | Needs `base_div` (not in ZK) |

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
- All subscription tier bitmasks (full ACCESS_READ, ACCESS_WRITE, ACCESS_ADMIN values)
- Rate limiting ratios (uses_allowed / period)
- Subscription durations and expiry blocks
- All subscription/unsubscription events
- Payment amounts

**NOT visible:**
- Actual content accessed (if routed through encrypted channels outside this contract)
- Specific service being subscribed to (if referenced by ID only)

See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation.

## Opcode Dependencies

| Opcode | Status | Plain Fallback |
|--------|--------|----------------|
| `base_div` | **IMPLEMENTED** (0x58) | Native Rust division (privacy tradeoff: results visible) |
| `less_than_or_equal` | **Verified Sound** | Cross-multiplication workaround |

## ZK Enhancement Path

`base_div` is now implemented and `less_than_or_equal` is verified sound. Migration to ZK is possible:

1. Replace native `&` bitmask with ZK constraint
2. Replace division with ZK-verified division
3. Keep subscription commitments private

## File Structure

```
src/contract_plain/subscription/
├── Cargo.toml
├── PRIVACY_TRADEOFFS.md
└── src/
    ├── lib.rs              # Function enum
    ├── error.rs            # Error types
    ├── model/mod.rs        # Subscription, AccessRight, etc.
    └── entrypoint.rs       # init, exec, update
```

## Compilation

```bash
cargo check -p darkfi_plain_subscription_contract
```

## Related Documentation

- [Plain Contracts Architecture](../../doc/src/arch/plain_contracts.md) - Dual-layer contract architecture
- [Composability](../../doc/src/arch/composability.md) - Cross-contract patterns
- [Parallel Societies](../../doc/src/arch/parallel_societies.md) - Privacy for social reproduction
- [ZK Subscription Contract](../../doc/src/arch/subscription.md) - ZK version (limited expressiveness)