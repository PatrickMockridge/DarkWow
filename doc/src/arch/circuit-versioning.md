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
| `DRK_POSEIDON_DOMAIN_CAP_COMMIT` | 4 | Capability commitment (coin commitment) |
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

Circuit declarations use a V2 suffix to distinguish them from the deleted V1
originals. Two capitalization conventions coexist depending on the contract:

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

The manifest's `[[circuits]].name` must match the `.zk` circuit name exactly:

```toml
# Matches circuit "Mint_V2" in mint.zk
[[circuits]]
name = "Mint_V2"
namespace = "native_token"
```

The manifest's `[[functions]].proof_circuit` references a circuit by its
`[[circuits]].name`:

```toml
[[functions]]
name = "pow_reward"
code = 5
requires_proof = true
proof_circuit = "Mint_V2"   # Must match a [[circuits]].name
```

### Rust Namespace Constants

Contract `lib.rs` files declare namespace constants matching the `.zk` circuit
name exactly. Only V2 constants exist — V1 constants have been removed:

```rust
pub const NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V2: &str = "Mint_V2";
```

### Enum Variants and Model Types

Enum variants and model types use V1 suffix (e.g., `FeeV1`, `FeeParamsV1`).
These are the **contract API version**, NOT the circuit version:

```rust
pub enum NativeTokenFunction {
    FeeV1 = 0x00,          // Contract function API version
    PoWRewardV1 = 0x05,   // Contract function API version
}
```

The function `FeeV1` uses circuit `Fee_V2` per the manifest's `proof_circuit`
declaration. The API version and circuit version are independent:

| Layer | Version | Meaning |
|-------|---------|---------|
| Enum variant | `FeeV1` | Contract function interface (on-chain opcode) |
| Model type | `FeeParamsV1` | Wire format for function parameters |
| Manifest proof_circuit | `Fee_V2` | Which circuit proves this function |
| .zk circuit name | `Fee_V2` | The compiled circuit artifact |

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

Do NOT add version suffixes to `.zk` filenames. The filename describes the
function. The circuit inside describes which version it is.

## See Also

- [Contract Manifest](manifest.md) — the versioning mechanism
- [Contract Catalog](../contracts.md) — all 32 contracts
- [Formal Specification](formal-specification.md) — architectural commitments
- [AI Documentation Index](ai-index.md) — full document map
- [Contract Safety — Naming Conventions](../../dev/contracts/safety.md#naming-conventions--circuits-manifests-and-entrypoints) — naming rules with failure-prevention rationale
