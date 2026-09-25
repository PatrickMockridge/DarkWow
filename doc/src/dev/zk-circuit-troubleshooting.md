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
   - Example: `PROMISSORY_NOTE_CONTRACT_ZKAS_ISSUE_NS_V1 = "Issue_V1"` while the only issue circuit
     on disk is `Issue_V2` (`proof/issue.zk`). This is not a migration in progress: the `_V1`
     constants are declared and re-exported but never read, so they name a circuit that does not
     exist — the class `scripts/check-artifact-freshness.sh` reports as dead legacy constants that
     mislead about which circuit is in use

3. **Missing or corrupted binary**
   - Binary file doesn't exist or is corrupted
   - CI/CD didn't regenerate binaries after code changes

## How to Regenerate Circuit Binaries

Each contract with ZK circuits has a `Makefile` that handles binary generation:

```bash
# Navigate to the contract directory
cd src/contract/promissory_note

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

Example output (promissory_note):
```
Issue_V2.constant
Redeem_V2.constant
RegisterType_V2.constant
Transfer_V2.constant
Revoke_V2.constant
```

Then verify the constants in `src/lib.rs` match what the binary contains. A constant naming a
circuit that is not on disk is the mismatch this guide is about:

```rust
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_ISSUE_NS_V1: &str = "Issue_V1";  // no Issue_V1 circuit on disk
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_ISSUE_NS_V2: &str = "Issue_V2";  // this one resolves
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

## PromissoryNote Contract Specific Notes

The promissory_note contract's circuits, as they are on disk (`proof/*.zk`), with the namespace
constant that must match each:

| Binary | Namespace | Used By |
|--------|-----------|---------|
| `issue.zk.bin` | `Issue_V2` | Issue a note |
| `redeem.zk.bin` | `Redeem_V2` | Redeem a note |
| `register_type.zk.bin` | `RegisterType_V2` | Register a note type |
| `revoke.zk.bin` | `Revoke_V2` | Revoke a note |
| `transfer.zk.bin` | `Transfer_V2` | Transfer a note |

Its `lib.rs` also declares five `_V1` namespace constants (`PROMISSORY_NOTE_CONTRACT_ZKAS_ISSUE_NS_V1
= "Issue_V1"` and its four siblings), which the SDK re-exports at
`src/sdk/src/contracts/promissory_note.rs:56-59`. Nothing reads them and no `_V1` circuit is on disk:
they are dead lookups, the same class the artifact-freshness gate reports, not a pending migration.

Note: The filename does NOT indicate the circuit's version — the namespace inside the file does. No
native contract filename carries a version suffix; see
[Circuit Versioning](../arch/circuit-versioning.md).
