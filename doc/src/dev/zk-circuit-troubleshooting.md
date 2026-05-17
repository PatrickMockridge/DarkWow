# ZK Circuit Troubleshooting Guide

## Overview

This document covers common issues that occur during the test lifecycle when working with DarkWow's ZK circuits (`.zk.bin` files).

## What are `.zk.bin` Files?

`.zk.bin` files are compiled Zero-Knowledge circuits for the Halo2 proof system. They contain:

- Circuit constraints (PLONKish arithmetic circuits)
- Proving keys (PK) and Verification keys (VK)
- Namespace identifiers used to lookup circuits at runtime

These binaries are generated from `.zk` source files using the `zkas` tool.

## Common Error: EcGetX

```
EcGetX: heap index 6 >= heap.len() 5
Error: PlonkError("General synthesis error")
```

**What it means**: The zkVM couldn't find the expected constraint or heap variable at the specified index. This is a circuit synthesis failure.

**Common causes**:

1. **Circuit binary is out of sync with code**
   - The circuit source (`.zk` file) was modified but the binary wasn't regenerated
   - Binary was compiled with different parameters than the code expects

2. **Namespace constant mismatch**
   - The constant in `src/lib.rs` doesn't match the actual namespace in the binary
   - Example: `MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V2 = "TokenMint_V2"` but binary contains `"TokenMint_V1"`

3. **Missing or corrupted binary**
   - Binary file doesn't exist or is corrupted
   - CI/CD didn't regenerate binaries after code changes

## How to Regenerate Circuit Binaries

Each contract with ZK circuits has a `Makefile` that handles binary generation:

```bash
# Navigate to the contract directory
cd src/contract/money_v3

# Clean existing binaries
make clean

# Regenerate all .zk.bin files
make all
```

The Makefile typically uses:
```
ZKAS = ../../../zkas
$(ZKAS) proof/<circuit>.zk -o proof/<circuit>.zk.bin
```

## Verifying Binary Contents

To check what namespace a binary actually contains:

```bash
strings proof/*.zk.bin | grep -E "^[A-Z].*_" | head
```

Example output:
```
Mint_V2.constant
Fee_V2.constant
Burn_V2.constant
TokenMint_V1.constant
AuthTokenMint_V1.constant
```

Then verify the constants in `src/lib.rs` match:
```rust
pub const MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V2: &str = "TokenMint_V1";
```

## Prevention

1. **Regenerate binaries after circuit code changes**
   - Any modification to `.zk` files should be followed by `make clean && make all`
   - Commit both the `.zk` source files AND the regenerated `.zk.bin` files

2. **Track binary versions**
   - The git history should show when binaries were last regenerated
   - A mismatch between source modification date and binary modification date indicates staleness

3. **CI/CD integration** (recommended)
   - Add circuit binary regeneration to CI/CD pipeline
   - Fail builds if source files changed but binaries weren't regenerated

## Test Lifecycle Issues

This issue tends to recur during the test lifecycle because:

1. **Circuit code evolves** - When ZK circuit logic changes, binaries become stale
2. **Migration scenarios** - During v1→v2 migrations, namespace constants change but binaries weren't regenerated
3. **Cross-branch work** - Binaries from one branch may not match another branch's code

### Warning Signs

- Tests pass on `master` but fail on a feature branch
- A specific test fails while others pass (test that uses a particular circuit)
- `EcGetX` errors appearing after merging or rebasing

### Resolution Checklist

1. Verify all circuit binaries are present: `ls proof/*.zk.bin`
2. Regenerate binaries: `make clean && make all`
3. Verify namespace constants match binary contents
4. Run the failing test again
5. If still failing, check if circuit source changed and binary regeneration is truly needed

## Money Contract Specific Notes

The money_v3 contract has these circuits (Poseidon-only, no EC operations):

| Binary | Namespace | Used By |
|--------|-----------|---------|
| `token_mint_v1.zk.bin` | `TokenMint_V1` | Create new token types |
| `auth_token_mint_v1.zk.bin` | `AuthTokenMint_V1` | Authorize token minting |
| `mint_v1.zk.bin` | `Mint_V1` | Mint tokens |
| `burn_v1.zk.bin` | `Burn_V1` | Burn tokens (nullifier) |

TransferV1 and OtcSwapV1 reuse Burn_V1 + Mint_V1 circuits.

Note: The filename pattern (`_v1.zk.bin`) does NOT necessarily mean it's a "v1" circuit. The namespace inside the file determines the actual version.
