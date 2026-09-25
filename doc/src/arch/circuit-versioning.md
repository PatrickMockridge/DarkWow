# Circuit Versioning

This document is the single source of truth for DarkWow's ZK circuit versioning
conventions. It explains why V2 circuits exist, what the naming conventions are,
and how versioning is handled going forward.

## V1→V2 Migration (May 2026, HAZOP RC3)

### Background

During the May 2026 HAZOP (Hazard and Operability) security review, a domain
separation gap was identified in all ZK circuits: `poseidon_hash` calls lacked
unique domain constants. Without domain separation, a hash output from one
circuit context could be reused in a different context, creating a cross-circuit
hash collision attack surface.

### What Changed

V2 circuits add `DRK_POSEIDON_DOMAIN_*` constants (defined in
`src/sdk/src/crypto/constants.rs`) prepended to every `poseidon_hash` call:

| Constant | witness_base value | Purpose |
|----------|-------------------|---------|
| `DRK_POSEIDON_DOMAIN_NULLIFIER` | 1 | Nullifier derivation |
| `DRK_POSEIDON_DOMAIN_TOKEN_COMMIT` | 2 | Token commitment |
| `DRK_POSEIDON_DOMAIN_TX_BINDING` | 3 | Transaction binding |
| `DRK_POSEIDON_DOMAIN_CAP_COMMIT` | 4 | Capability commitment (commitment) |
| `DRK_POSEIDON_DOMAIN_MERKLE_LEAF` | 5 | Merkle leaf hashing |
| `DRK_POSEIDON_DOMAIN_USER_DATA_ENC` | 6 | User data encryption |
| `DRK_POSEIDON_DOMAIN_SIGNATURE_SECRET` | 7 | Signature secret derivation |
| `DRK_POSEIDON_DOMAIN_KEY_DERIVE` | 8 | Key derivation |
| `DRK_POSEIDON_DOMAIN_CAPABILITY_ID` | 9 | Capability identifier |

In `.zk` circuit source, these are inlined as `witness_base(N)` values. Each
`poseidon_hash` call in a V2 circuit begins with exactly one domain constant as
its first argument:

```zk
# V1 (pre-hardening):
nullifier = poseidon_hash(secret, commitment);

# V2 (post-hardening):
nullifier = poseidon_hash(DOMAIN_NULLIFIER, secret, commitment);
```

### What Did NOT Change

The V1→V2 migration was a focused patch — domain constants only. No circuit was
fundamentally redesigned. The constraint structure, witness layout, public
inputs, and proof semantics are identical between V1 and V2 circuits. The V2
suffix marks the domain separation hardening, not a replacement or redesign.

### Migration Scope

All circuits across all 32 contracts were migrated. V1 circuit source files
(`*.zk` with V1 circuit declarations) were deleted. Only V2 circuits exist on
disk. The migration is tracked in git history under the HAZOP RC3 hardening
commits.

## Naming Conventions

### `.zk` Source Filenames

No version suffix. Examples: `mint.zk`, `deposit.zk`, `create_swap.zk`.

The filename describes the function, not the circuit version. A `.zk` file
contains exactly one circuit, and that circuit is the current (latest) version.

### Circuit Names Inside `.zk` Files

Circuit declarations use a V2 suffix. Two capitalization conventions coexist
depending on the contract:

**CamelCaseV2** (no underscore before version):
```
circuit "IssueCredentialV2"    # identity
circuit "CreateSwapV2"          # dex
circuit "DepositV2"             # bridge
```

**Snake_Case_V2** (underscore before version):
```
circuit "Mint_V2"               # native_token
circuit "CommitBet_V2"          # baccarat
circuit "RegisterType_V2"       # promissory_note
```

Both conventions are valid. Use whichever the contract already uses — never mix
conventions within a single contract.

### Manifest `[[circuits]]` Entries

`[[functions]].proof_circuit` names a `[[circuits]]` entry `(name, namespace)`
— the wallet loads the compiled zkas binary for that pair from the
`zkas_binaries` store (`src/sdk/src/prover.rs` §Construction steps 2-3):

```toml
# native_token/manifest.toml
[[circuits]]
name = "Fee_V3"
namespace = "native_token"

[[functions]]
name = "fee"
code = 8
requires_proof = true
proof_circuit = "Fee_V3"
```

The manifest `name` matches the circuit name inside the `.zk` file
(`circuit "Fee_V3"` in `proof/fee.zk`), and the namespace constant matches it
too (`NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V3 = "Fee_V3"`,
`src/contract/native_token/src/lib.rs:178`). The build compiles `proof/*.zk`
→ `proof/*.zk.bin` by filename (`native_token/Makefile`).

### Rust Namespace Constants

Contract `lib.rs` files declare namespace constants matching the `.zk` circuit
name exactly. Only the current versions exist — there is no `_V1` namespace
constant anywhere (the deleted V1 circuits have no constants left):

```rust
pub const NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V2: &str = "Mint_V2";
```

### Enum Variants and Model Types

Enum variants and model types carry the **contract API version**, NOT the
circuit version. Most `native_token` variants keep their original `V1`
suffix; the fee entrypoint is `FeeV3` with model type `FeeParamsV3`
(`src/contract/native_token/src/model/fee.rs:78`):

```rust
pub enum NativeTokenFunction {
    MintV1 = 0x01,
    BurnV1 = 0x02,
    TransferV1 = 0x03,
    SpendV1 = 0x04,
    PoWRewardV1 = 0x05,
    FeeCollectV1 = 0x06,
    UncleMintV1 = 0x07,
    FeeV3 = 0x08,        // Contract function API version
}
```

The function `FeeV3` uses circuit `Fee_V3` per the manifest's `proof_circuit`
declaration. The API version and circuit version are independent:

| Layer | Version | Meaning |
|-------|---------|---------|
| Enum variant | `FeeV3` | Contract function code (the selector byte) |
| Model type | `FeeParamsV3` | Wire format for function parameters |
| Manifest proof_circuit | `FeeV3` | Which `[[circuits]]` entry names the proving circuit |
| .zk circuit name | `Fee_V3` | The compiled circuit artifact |

A function that **drops** its proof requirement keeps its API version unchanged and gains no circuit
version at all — see [When a Function Stops Requiring a Proof](#when-a-function-stops-requiring-a-proof).

### `include_bytes!` Paths Match the Makefile Output Exactly

The Makefile produces `proof/<stem>.zk.bin` from `proof/<stem>.zk` by direct stem substitution
(`$(ZK_SRC:.zk=.zk.bin)`). Every `include_bytes!` path references that exact filename, with no version
marker added:

```rust
include_bytes!("../proof/mint.zk.bin");   // NOT mint_v2.zk.bin
```

During the V1→V2 consolidation, 327 `include_bytes!` calls referenced `_v2.zk.bin` paths the Makefile
never produces. Any path that adds, removes, or alters part of the stem fails at compile time with
"No such file or directory" — a loud failure, but one that costs a full rebuild to discover, and one
that a stem-adding rename introduces wholesale. The circuit version lives inside the file
(`circuit "Mint_V2"`); it never appears in a path.

### Entrypoint Module Filenames Carry No Version Suffix

Module files under `entrypoint/` are named for the function without `_v1`: `commit_bet.rs`, not
`commit_bet_v1.rs`. The functions inside still carry the API version
(`baccarat_commit_bet_process_instruction_v1`).

During a prior refactoring, 39 module files were renamed to drop the `_v1` suffix but the
corresponding `mod` declarations and `use` statements were not updated, leaving six contracts unable
to compile for an unknown period. Keeping module filenames suffix-free removes the class of bug where
a file rename is not propagated to its `mod` declaration — the same failure shape as the manifest,
namespace-constant, and `include_bytes!` rules above.

## Future Versioning

Going forward, versioning is handled via manifests:

- **Contract version**: `[contract].version` field (e.g., `"2.0.0"`)
- **Circuit selection**: `[[functions]].proof_circuit` declares which circuit
  proves each function
- **Circuit catalog**: `[[circuits]]` declares all circuits the contract uses
- **Backward compatibility**: A contract can declare both old and new circuits
  in `[[circuits]]`, with `proof_circuit` pointing to the active one

When a circuit is hardened or replaced:
1. The new circuit is added as a new `.zk` file with a new circuit name
2. The manifest's `[[circuits]]` gains a new entry for the new circuit
3. The function's `proof_circuit` is updated to point to the new circuit
4. The old circuit entry can be kept in `[[circuits]]` for historical reference
   or removed if no longer needed
5. Enum variants and model types do NOT need version suffix changes — the
   manifest handles circuit versioning independently

### When a Function Stops Requiring a Proof

A function can also stop needing a circuit altogether — its values become public, so the host can verify
it in the clear. The manifest expresses this by **omitting** `requires_proof` and `proof_circuit` from the
function's entry:

```toml
# native_token/manifest.toml
[[functions]]
name = "pow_reward"
code = 5
description = "Block reward via PoWRewardV1 — … in plaintext (no ZK proof)."
# no requires_proof, no proof_circuit
```

This case is written down separately from the replacement procedure above because three things differ,
and each is a place a reader can go wrong:

1. **Nothing is renamed.** The enum variant keeps its contract-API version (`PoWRewardV1` stays `0x05`),
   because there is no new circuit name to match — so no namespace constant, manifest `proof_circuit` or
   `include_bytes!` path changes. A function's API version carrying a `V1` suffix is not a claim that a
   circuit version exists.
2. **The `.zk` file is not necessarily deleted.** When the circuit was shared — `Mint_V2` proved the
   coinbase *and* transfer/spend outputs — the file remains for the callers that still need it, and only
   the one function's use of it ends. Check for other callers before removing anything.
3. **The status must be stated where the function is tabulated**, because there is no circuit name left to
   signal it. A reader who sees a function code and no `proof_circuit` must be told the call is plaintext
   rather than left to infer it from an absence.

Worked example: `native_token` — `pow_reward` (0x05), `fee_collect` (0x06) and `uncle_mint` (0x07) are
plaintext calls with no circuit and no proof. `Mint_V2` was removed from the coinbase path and remains the
transfer/spend output mint; `FeeCollect_V2` was dropped entirely. See
[Consensus & Coinbase](consensus-coinbase.md).

Do NOT add version suffixes to `.zk` filenames. The filename describes the
function. The circuit inside describes which version it is.

## See Also

- [Contract Manifest](manifest.md) — the versioning mechanism
- [Contract Catalog](../contracts.md) — all 32 contracts
- [Formal Specification](formal-specification.md) — architectural commitments
- [AI Documentation Index](ai-index.md) — full document map
- [Naming Conventions](#naming-conventions) — naming rules with failure-prevention rationale
