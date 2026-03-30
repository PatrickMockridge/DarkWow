# DarkFi Money V2 Contract

**money_v2** is our standard money contract for this fork, featuring improved circuit design with self-contained ZK proofs.

## Overview

money_v2 implements private token transfers, atomic swaps, token minting/freezing, and staking with a focus on **provably correct circuit design**.

## Key Differences from money (v1)

| Feature | money (v1) | money_v2 |
|---------|------------|----------|
| Namespace | `Fee_V1`, `Burn_V1`, etc. | `Fee_V2`, `Burn_V2`, etc. |
| Public key binding | Relies on external verification | `constrain_equal_base` |
| Self-contained proofs | No | Yes |
| Standard for this fork | No | **Yes** |

## Security Model

money_v2 circuits use `constrain_equal_base` to bind derived public keys to their witnesses, making the circuit **self-contained** without relying on external verification layers.

```zk
# Sound pattern in money_v2
signature_public = ec_mul_base(signature_secret, NULLIFIER_K);
derived_sig_pub_x = ec_get_x(signature_public);
derived_sig_pub_y = ec_get_y(signature_public);
constrain_equal_base(derived_sig_pub_x, signature_public_x);  # BIND
constrain_equal_base(derived_sig_pub_y, signature_public_y);  # BIND
constrain_instance(signature_public_x);
constrain_instance(signature_public_y);
```

## Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| FeeV2 | 0x00 | Fee calculation for transactions |
| GenesisMintV2 | 0x01 | Initial token minting |
| PoWRewardV2 | 0x02 | Proof-of-work mining rewards |
| TransferV2 | 0x03 | Private token transfers |
| OtcSwapV2 | 0x04 | Atomic OTC swaps |
| AuthTokenMintV2 | 0x05 | Authorized token minting |
| AuthTokenFreezeV2 | 0x06 | Token freezing |
| TokenMintV2 | 0x07 | Standard token minting |
| BurnV2 | 0x08 | Token burning |

## Circuit Namespaces

- `Fee_V2`
- `Mint_V2`
- `Burn_V2`
- `AuthTokenMint_V2`
- `TokenMint_V2`

## Building

```bash
# Compile circuits and WASM
make all

# Run integration tests
make test

# Run specific test
make test-integration
```

## Repository Structure

```
src/contract/
├── money/           # Original DarkFi money (v1) - upstream legacy
├── money_v2/        # Our secure version (v2) - STANDARD GOING FORWARD
└── ...
```

## Documentation

- [Money Vulnerability Analysis](../../doc/src/arch/money-vulnerability-analysis.md) - Security reasoning
- [Money Version Bridge](../../doc/src/arch/money-version-bridge.md) - Fork decision details
- [Public Key Constraint Hook](../../doc/src/arch/pubkey-constraint-hook.md) - Prevention mechanism
- [Security Analysis](../../doc/src/arch/security-analysis.md) - Full audit details

## Status

**money_v2 is our standard** for this fork. Use it for all new development.

The original `money/` contract is maintained for network compatibility with upstream DarkFi.
