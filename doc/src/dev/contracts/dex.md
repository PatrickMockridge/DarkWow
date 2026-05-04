# DEX Contract

Level 0 MVP: Atomic Swap DAO for privacy-preserving token swaps.

## Overview

The DEX implements **atomic swaps** between two parties without revealing amounts, identities, or trade data. No order book - parties bilaterally agree on swaps.

**Key Innovation**: Starts completely dark (Level 0) and can expand toward transparency incrementally.

## The Problem: SPV De-anonymization

Bitcoin's SPV bloom filters leak what addresses you're interested in. The same problem affects order book DEXs - revealing what swaps you're looking for = privacy failure.

## Our Solution: Incremental Transparency

```
Level 0: Complete darkness (MVP - atomic swaps via DAO)
Level 1: Aggregate market data only (price ranges, volume bands)
Level 2: Anonymized trades (unlinkable)
Level 3: Full transparency (opt-in)
```

## Level 0 MVP Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    Atomic Swap DAO                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. Alice creates swap: lock(DRK, 100), request(ETH, 1)        │
│     → lock_commitment = H(secret, token, amount)                 │
│     → swap_id = H(lock_commitment, request_token, request_amount) │
│                                                                   │
│  2. Bob accepts: lock(ETH, 1)                                   │
│     → Contract verifies Bob's lock matches Alice's request      │
│                                                                   │
│  3. Execute: ZK proof verifies both secrets known               │
│     → Atomic: Alice gets ETH, Bob gets DRK                         │
│                                                                   │
│  4. OR Cancel after timeout: refund both                         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Privacy Properties

| What Level 0 Hides | What Level 0 Reveals |
|--------------------|-----------------------|
| Who is trading | That a swap occurred |
| Amounts swapped | Aggregate volume (after batch) |
| Tokens involved (until match) | |

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize swap contract |
| CreateSwapV1 | 0x01 | Create atomic swap proposal |
| AcceptSwapV1 | 0x02 | Accept swap (provide liquidity) |
| ExecuteSwapV1 | 0x03 | Execute atomic swap |
| CancelSwapV1 | 0x04 | Cancel and refund |
| UpdateConfigV1 | 0x05 | Update timeout/fee |

## ZK Circuits

- `create_swap_v1.zk`: Prove proposer has locked valid funds
- `accept_swap_v1.zk`: Prove acceptor has locked matching funds
- `execute_swap_v1.zk`: Prove both secrets known, locks valid
- `cancel_swap_v1.zk`: Prove ownership for cancellation

### Signature Verification

The DEX uses a **split verification model** for signatures:

1. **Host verifies signature** before contract execution using `SchnorrPublic::verify()`
2. **ZK circuit constrains** the signature public key coordinates

This is necessary because there is no `schnorr_verify` opcode in the zkVM.
The circuit proves knowledge of the secret key by deriving the public key and
constraining its coordinates to match the provided values.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Signature Verification Flow                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. Client creates swap, signs with secret key                   │
│  2. Host verifies signature (Rust level)                         │
│  3. If valid, contract executes                                 │
│  4. ZK circuit constrains signature_public.x/y                   │
│     (proves prover knows corresponding secret)                   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Opcode Dependencies

The DEX design is constrained by missing opcodes:

| Opcode | Status | Impact |
|--------|--------|-------|
| `BaseDiv` | Implemented (0x58) | Enables price ratio calculations in circuit |
| `LessThanOrEqual` | Experimental | Uses safemath pattern for assertions |
| Cross-contract ZK | Not implemented | Requires trusted setup for Money contract |
| `schnorr_verify` | Not implemented | Signature verification split between host and circuit |

See [DEX Architecture](../../contract/dex.md) for detailed analysis of opcode limitations.

## Roadmap

```
Level 0 (MVP - NOW)     Level 1 (Future)        Level 2 (Future)        Level 3 (Future)
─────────────────────────────────────────────────────────────────────────────────────────
Atomic swaps via DAO       SMT order book           ZK matching              Differential privacy
Bilateral agreement       Hidden commitments       Anonymous trades         Aggregate noise
No price discovery        ZK proof of match       Unlinkable              Opt-in transparency
```

## Structure

```
src/contract/dex/
├── proof/
│   ├── create_swap_v1.zk
│   ├── accept_swap_v1.zk
│   ├── execute_swap_v1.zk
│   └── cancel_swap_v1.zk
├── src/
│   ├── client/mod.rs
│   ├── entrypoint.rs
│   ├── error.rs
│   ├── lib.rs
│   └── model/mod.rs
├── tests/
├── Cargo.toml
└── Makefile
```

## Building

```bash
cd src/contract/dex
make
make proof
cargo test
```

## References

- [DEX Architecture](../../contract/dex.md)
- [SPV Privacy Problem](https://en.bitcoin.it/wiki/Thin_Client_Security)
