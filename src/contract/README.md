This directory contains native WASM contracts on DarkFi.

## Unofficial/Experimental Contracts on Dev Branch

The `dev` branch contains additional contracts not yet in official DarkFi master. These are **EXPERIMENTAL** and **NOT AUDITED**.

| Contract | Description | Status |
|----------|-------------|--------|
| **attestation** | Generalized attestation and claims system | ✅ Complete |
| **atomic_swap** | Cross-chain atomic swaps via HTLC | ✅ Complete |
| **bridge** | Cross-chain asset transfers with Ocaps | ✅ Complete |
| **dao_escrow** | DAO with three modes (Escrow/Treasury/Endowment) | ✅ Complete |
| **darktoshi_dice** | Privacy-preserving Satoshi Dice clone | ✅ Complete |
| **dex** | Atomic swap DAO with partial fills + open execution | ✅ Complete |
| **drain_protection** | Endowment/treasury drain protections with 8 best practices | ✅ Complete |
| **escrow** | Hashed Timelock Contract variant | ✅ Complete |
| **identity** | ZK credential proofs using competency DAGs | ✅ Uses safemath (Level 0 zk_only) |
| **insurance_market** | Decentralized insurance marketplace | ✅ Complete |
| **labor_market** | Job/labor market with escrow and DAO governance | ✅ Complete |
| **auction** | Privacy-preserving auction using escrow for bids | ✅ Complete |
| **oracle** | Push-model oracle with attestation integration | ✅ Complete |
| **prediction_market** | Prediction market with AMM pricing | ✅ Complete |
| **tender** | Sealed-bid tendering with competency verification | ✅ Complete |
| **stablecoin** | Monero-collateralized stablecoin | ✅ Uses safemath |
| **subscription** | Member subscription with DAO treasury | ✅ Complete |

---

## Money V2 (Standard for This Fork)

**money_v2** is our standard money contract featuring improved circuit design with self-contained ZK proofs.

| Contract | Description | Standard |
|----------|-------------|----------|
| **money** | Original DarkFi money (v1) - upstream legacy | No |
| **money_v2** | Our secure version with constrain_equal_base | **Yes** |

See [money_v2/README.md](money_v2/README.md) for full details.

---

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

### Ideal vs Workaround: LessThanOrEqual vs Safemath

| Aspect | `LessThanOrEqual` (IDEAL) | Safemath (WORKAROUND) |
|--------|---------------------------|----------------------|
| Returns Boolean | ✅ Yes | ❌ No (constrain-only) |
| Composability | ✅ Full | ❌ Limited to assertions |
| Circuit bloat | ✅ None (single VM impl) | ❌ Each circuit copies gadget |
| Soundness | ⚠️ Unverified (gate soundness) | ✅ Production-ready |
| Use when | Need Boolean for downstream logic | Only need to assert `a <= b` |

**Why this matters**: Safemath (`assert_lte_u64_v1.zk`) is a **workaround** that works for stablecoin/identity because they only need to assert constraints (not return Booleans). If you need the comparison result as a value for further constraints, LessThanOrEqual is required.

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
| money (v1) | `burn_v1.zk` | No | ✅ Safe |
| money_v2 | `burn_v1.zk` | No | ✅ Safe - constrain_equal_base |
| escrow | `refund_v1.zk` | No | ✅ Safe |
| dao_escrow | `init_v1.zk` | No | ✅ Safe |
| dao_escrow | `pay_premium_v1.zk` | No | ✅ Safe |
| identity | `create_claim_v1.zk` | No | ✅ Uses safemath (Level 0 zk_only) |
| stablecoin | `open_position_v1.zk` | No | ✅ Uses safemath `assert_lte_u64_v1.zk` |
| stablecoin | `liquidate_v1.zk` | No | ✅ Uses safemath `assert_lte_u64_v1.zk` |
| bridge | `deposit_v1.zk` | No | ✅ Fixed — real `merkle_root` |

---

### Key Takeaways

1. **No `BaseDiv` needed** - Cross-multiplication with `less_than_strict` handles all ratio checks
2. **`less_than_strict` is safe** - It's constrain-only (no return value manipulation)
3. **stablecoin and identity use safemath** - Assertion gadgets (`assert_lte_u64_v1.zk`) as workaround for LessThanOrEqual
4. **LessThanOrEqual is IDEAL but has technical debt** - Returns Boolean for composability, but gate soundness unverified
5. **Safemath is WORKAROUND** - Production-ready for assertion-only, but cannot return Boolean for downstream logic
6. **Bridge Merkle is fixed** - `deposit_v1.zk` now uses real `merkle_root` opcode

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

## Unofficial Contract Documentation

Local READMEs exist for each contract in this folder:

- [attestation/README.md](attestation/README.md) - Generalized attestation and claims
- [atomic_swap/README.md](atomic_swap/README.md) - Cross-chain atomic swaps
- [bridge/README.md](bridge/README.md) - Cross-chain asset transfers
- [dao_escrow/README.md](dao_escrow/README.md) - Three-mode DAO
- [darktoshi_dice/README.md](darktoshi_dice/README.md) - Satoshi Dice clone
- [dex/README.md](dex/README.md) - Atomic swap DAO
- [drain_protection/README.md](drain_protection/README.md) - Endowment protection
- [escrow/README.md](escrow/README.md) - Hashed Timelock Contract
- [identity/README.md](identity/README.md) - ZK credential proofs
- [insurance_market/README.md](insurance_market/README.md) - Decentralized insurance
- [labor_market/README.md](labor_market/README.md) - Job/labor market
- [money_v2/README.md](money_v2/README.md) - Secure money contract (STANDARD)
- [auction/README.md](auction/README.md) - Privacy-preserving auction
- [oracle/README.md](oracle/README.md) - Push-model oracle
- [prediction_market/README.md](prediction_market/README.md) - Prediction market
- [stablecoin/README.md](stablecoin/README.md) - Collateral stablecoin
- [subscription/README.md](subscription/README.md) - Member subscription
- [tender/README.md](tender/README.md) - Sealed-bid tendering

Architecture docs:

- [dao_escrow.md](../../doc/src/arch/dao_escrow.md)
- [subscription.md](../../doc/src/arch/subscription.md)
- [atomic_swap.md](../../doc/src/arch/atomic_swap.md)
- [oracle.md](../../doc/src/arch/oracle.md)
- [security-analysis.md](../../doc/src/arch/security-analysis.md)
- [composability.md](../../doc/src/arch/composability.md)
