# Attestation Plain Contract

> **DEPRECATED**: Use `darkfi_attestation_contract` in `../../contract/attestation/` instead.
>
> ZK opcodes `base_div` and `less_than_or_equal` are now sound and implemented.

---

A **partial transparency** alternative to the ZK `attestation` contract. It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.

## Why This Contract Exists (Historical)

The existing ZK attestation contract is constrained by missing `base_div` opcode:
- Cannot do delegation ratio calculations (need division)
- Cannot express complex credential chains
- Limited depth for delegation

See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation of privacy tradeoffs.

## Key Features

| Feature | ZK Version | Plain Version | Privacy Impact |
|---------|-----------|---------------|----------------|
| Credential chains | Limited | Full delegation support | Chain data visible |
| Delegation ratios | Circuit-limited | Full expression | Ratios visible |
| Cross-references | Simple | Full graph support | Reference chains visible |
| Expiry verification | Basic | Time-bounded with ratios | Expiry visible |

## Architecture

This contract uses a **hybrid ZK/plain approach**:

| Operation | Method | Why |
|-----------|--------|-----|
| Signature verification | ZK (Schnorr) | Sound, constrainable |
| Attestation commitment | ZK (Pedersen) | Privacy-preserving |
| Delegation ratio | Native Rust | Needs `base_div` (not in ZK) |
| Credential chains | Native Rust | Complex graph traversal |

## Security Principle

> **We prefer plain over ZK-with-unsound-opcodes.**

### The Danger of Unsound ZK Opcodes

With a **plain contract**:
- Incorrect behavior is **visible** on-chain
- Anyone can see if credentials are being misused
- Attackers must act in plain sight

With a **ZK contract using unsound opcodes**:
- A **malicious proof** can appear valid while circumventing credentials
- Bug in `less_than_or_equal` allows prover to bypass intended logic
- Result: **undetectable credential fraud** while the system thinks verification passed

See [Opcode Soundness Status](PRIVACY_TRADEOFFS.md#opcode-soundness-status) for details.

## Privacy Tradeoffs

**Visible on-chain:**
- All attestation schemas
- Credential chains and delegation paths
- Delegation ratios and depth limits
- Revocation status

**NOT visible:**
- Specific credential content (stored off-chain, only hash)
- Personal details in credentials
- Internal attestation policies

## Credential State Machine

```
Active → Revoked
      → Expired (time-based)
      → Delegated (creates new attestation)
```

## Opcode Dependencies

| Opcode | Status | Plain Fallback |
|--------|--------|----------------|
| `base_div` | **IMPLEMENTED** (0x58) | Native division (privacy tradeoff: ratios visible) |
| `less_than_or_equal` | **Verified Sound** | Cross-multiplication |

## ZK Enhancement Path

`base_div` is now implemented and `less_than_or_equal` is verified sound. Migration to ZK is possible:

1. Replace delegation ratio checks with ZK constraints
2. Keep credential chains private through commitments
3. Use ZK for attestation verification where sound
4. Maintain revocation transparency through ZK proofs

## File Structure

```
src/contract_plain/attestation/
├── Cargo.toml
├── PRIVACY_TRADEOFFS.md
└── src/
    ├── lib.rs              # Function enum
    ├── error.rs            # Error types
    ├── model/mod.rs        # Attestation, Attestor, params, updates
    └── entrypoint.rs       # init, exec, update
```

## Compilation

```bash
cargo check -p darkfi_plain_attestation_contract
```

## Related Documentation

- [Plain Contracts Architecture](../../doc/src/arch/plain_contracts.md) - Dual-layer contract architecture
- [Composability](../../doc/src/arch/composability.md) - Cross-contract patterns
- [Parallel Societies](../../doc/src/arch/parallel_societies.md) - Privacy for social reproduction