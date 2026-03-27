This directory contains native WASM contracts on DarkFi.

## ZKas Circuit Analysis

### Opcode Classification

**Proven Opcodes (Production Safe)**

| Opcode | Purpose |
|--------|---------|
| `poseidon_hash` | Commitment hashing, nullifiers, public keys |
| `ec_mul_base` | Public key derivation from secrets |
| `ec_get_x`, `ec_get_y` | Extract EC point coordinates |
| `ec_add`, `ec_mul`, `ec_mul_short` | Pedersen commitments |
| `merkle_root`, `sparse_merkle_root` | Merkle membership proofs |
| `less_than_strict` | Constrain-only comparisons (no return value) |
| `base_add`, `base_mul`, `base_sub` | Field arithmetic |
| `range_check`, `bool_check` | Value validation |
| `constrain_equal_base` | Equality constraints |

**Experimental Opcodes (Grey-Market - Not Production Ready)**

| Opcode | Issue |
|--------|-------|
| `LessThanOrEqual` (0x55) | Gate soundness vulnerability |
| `IsEqualBase` (0x54) | Delta-invert issue when `a == b` |
| `NotBase` (0x56) | Unused, experimental |
| `BaseLtStrict` (0x57) | Unused, experimental |

---

### Soundness Issues

**1. LessThanOrEqual Gate Soundness (CRITICAL)**

The constraint `out * (b - a) + (1 - out) * (a - b - 1)` can be satisfied with malicious assignments when `out = 0` and `a > b`. The range check limits but doesn't fully close the attack vector.

**2. IsEqualBase Delta-Invert (CRITICAL)**

When `a == b`, the constraint `delta * delta_invert == 1` becomes `0 * anything == 1`, which is unsatisfiable. A selector gate disables this, but the prover can assign arbitrary `delta_invert` when inputs are equal.

---

### Cross-Multiplication Pattern (Avoids BaseDiv)

Ratio checks use cross-multiplication instead of division:

```zk
# Prove: approval_ratio <= yes_vote / all_vote
# Without division - use cross-multiplication:
lhs = base_mul(all_vote_value, approval_ratio_quot);
rhs = base_mul(yes_vote_value, approval_ratio_base);
rhs_1 = base_add(rhs, 1);  # +1 converts strict < to <=
less_than_strict(lhs, rhs_1);
```

This pattern (in `dao/exec.zk`) handles ratio checks without `BaseDiv`.

---

### Merkle Verification Patterns

| Circuit | Pattern | Status |
|---------|---------|--------|
| `burn_v1.zk` | `merkle_root(leaf_pos, path, coin_incl)` with `zero_cond` | ✅ Real |
| `propose-main.zk` | `merkle_root(dao_leaf_pos, dao_path, dao_bulla)` | ✅ Real |
| `propose-input.zk` | `sparse_merkle_root` for nullifiers | ✅ Real |
| `deposit_v1.zk` | `poseidon_hash(deposit_leaf, merkle_path_0, merkle_path_1)` | ❌ **Fake** |

---

### Circuit Safety Summary

| Contract | Circuit | Experimental? | Issues |
|----------|---------|---------------|--------|
| dao | `exec.zk` | No | ✅ Safe |
| dao | `propose-main.zk` | No | ✅ Safe |
| money | `burn_v1.zk` | No | ✅ Safe |
| escrow | `refund_v1.zk` | No | ✅ Safe |
| dao_escrow | `init_v1.zk` | No | ✅ Safe |
| dao_escrow | `pay_premium_v1.zk` | No | ✅ Safe |
| identity | `create_claim_v1.zk` | **Yes** | LessThanOrEqual + IsEqualBase |
| stablecoin | `open_position_v1.zk` | **Yes** | LessThanOrEqual |
| stablecoin | `liquidate_v1.zk` | **Yes** | LessThanOrEqual |
| bridge | `deposit_v1.zk` | No | ✅ Fixed — real `merkle_root` |

---

### Key Takeaways

1. **No `BaseDiv` needed** - Cross-multiplication with `less_than_strict` handles all ratio checks
2. **`less_than_strict` is safe** - It's constrain-only (no return value manipulation)
3. **Experimental opcodes block production** - `identity` and `stablecoin` cannot ship until `LessThanOrEqual` is formally verified or replaced
4. **Bridge Merkle is fixed** - `deposit_v1.zk` now uses real `merkle_root` opcode

**See also**:
- [Experimental Opcodes](../../doc/src/arch/experimental-opcodes.md) — Concise reference for contract authors
- [zkVM Primitive Layer](../../doc/src/arch/zkvm_primitives.md) — Deep dive into opcode implementation

## Official Contracts

- **Money**: Private token transfers and basic operations
  - [Documentation](https://dark.fi/book/dev/darkfi_money_contract/)
- **DAO**: Decentralized autonomous organization with governance
  - [Documentation](https://dark.fi/book/dev/darkfi_dao_contract/)
- **Deployooor**: Contract deployment and management
  - [Documentation](https://dark.fi/book/dev/darkfi_deployooor_contract/)

## Development Fork Contracts

The `dev` branch contains additional contracts not yet in official DarkFi:

- **Bridge**: Cross-chain asset transfers with Object Capability Security (no VSS)
  - [README](https://codeberg.org/PatrickM123/darkfi/src/branch/dev/src/contract/bridge/README.md)
  - [Contract Doc](https://codeberg.org/PatrickM123/darkfi/src/branch/dev/doc/src/dev/contracts/bridge.md)
- **DEX**: Atomic swap DAO with incremental transparency roadmap
  - [README](https://codeberg.org/PatrickM123/darkfi/src/branch/dev/src/contract/dex/README.md)
  - [Contract Doc](https://codeberg.org/PatrickM123/darkfi/src/branch/dev/doc/src/dev/contracts/dex.md)
- **Identity**: Minimal credential proofs using ZK-verified competency DAGs
  - [README](https://codeberg.org/PatrickM123/darkfi/src/branch/dev/src/contract/identity/README.md)
  - [Contract Doc](https://codeberg.org/PatrickM123/darkfi/src/branch/dev/doc/src/dev/contracts/identity.md)
- **Stablecoin**: Monero-collateralized stablecoin with P2P Oracle design
  - [README](https://codeberg.org/PatrickM123/darkfi/src/branch/dev/src/contract/stablecoin/README.md)
  - [Contract Doc](https://codeberg.org/PatrickM123/darkfi/src/branch/dev/doc/src/dev/contracts/stablecoin.md)
