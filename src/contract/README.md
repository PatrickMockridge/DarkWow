This directory contains the native WASM contracts on DarkWow.

## What is here

`src/contract/` holds **34 directories**: 32 contracts, the `entropy` library (rlib-only, not a
deployable contract) and `test-harness` (test infrastructure). Nine contracts are genesis:
`native_token`, `deployooor`, `promissory_note`, `box`, `purse`, `identity`, `oracle`,
`attestation` and `multisig`. Thirty-one carry a `manifest.toml`; `deployooor` is deliberate
plaintext infrastructure with none.

The canonical catalog, maturity labels and per-contract documentation links are in
[doc/src/contracts.md](../../doc/src/contracts.md). Every contract directory has its own
`README.md`; where a README and a contract page disagree, the code and the page under
`doc/src/contract/` are authoritative.

**Status is not uniform.** Some arms are deliberately refused while their authority or custody
model is absent: the stablecoin's custody arms (`CustodyModelUnavailable`), bearer_bond's
issuer-authority arms, drain_protection's member exit, and bridge's deposit in the default build.
Check the contract page and the manifest before assuming an endpoint is live.

**Randomness**: see [provable_randomness.md](../../doc/src/contract/provable_randomness.md).

## Promissory Note (the standard)

`promissory_note` is the standard DeFi token contract — Poseidon-only, no EC operations, fungible
with hidden token ids. `native_token` is the consensus-native token for PoW rewards and fees. The
upstream `money` and `money_v2` contracts were removed; [MIGRATION.md](MIGRATION.md) records the
migration. See [promissory_note/README.md](promissory_note/README.md).

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
| `LessThanOrEqual` (0x55) | Gate-level implementation under investigation; opcode spec verified sound in Lean 4 |
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

**Why this matters**: Safemath (`assert_lte_u64.zk`) is a **workaround** that works for stablecoin/identity because they only need to assert constraints (not return Booleans). If you need the comparison result as a value for further constraints, LessThanOrEqual is required.

---

### Cross-Multiplication Pattern (Alternative to BaseDiv)

Ratio checks can use cross-multiplication instead of division:

```zk
# Prove: approval_ratio <= yes_vote / all_vote
# Without division - use cross-multiplication:
lhs = base_mul(all_vote_value, approval_ratio_quot);
rhs = base_mul(yes_vote_value, approval_ratio_base);
rhs_1 = base_add(rhs, 1);  # +1 converts strict < to <=
less_than_strict(lhs, rhs_1);
```

**Note**: `BaseDiv` is now implemented (opcode 0x58). This cross-multiplication pattern is still useful for simple ratio assertions where BaseDiv would be expensive.

---

### Merkle Verification Patterns

| Circuit | Pattern | Status |
|---------|---------|--------|
| `burn.zk` | `merkle_root(leaf_pos, path, coin_incl)` with `zero_cond` | ✅ Real |
| `propose-main.zk` | `merkle_root(dao_leaf_pos, dao_path, dao_bulla)` | ✅ Real |
| `propose-input.zk` | `sparse_merkle_root` for nullifiers | ✅ Real |
| `deposit.zk` | `poseidon_hash(deposit_leaf, merkle_path_0, merkle_path_1)` | ❌ **Fake** |

---

### Circuit Safety Summary

| Contract | Circuit | Experimental? | Issues |
|----------|---------|---------------|--------|
| dao | `exec.zk` | No | ✅ Safe |
| dao | `propose-main.zk` | No | ✅ Safe |
| money (v1) | `burn.zk` | No | ✅ Safe — historical, contract removed |
| escrow | `refund.zk` | No | ✅ Safe |
| dao_escrow | `init.zk` | No | ✅ Safe |
| dao_escrow | `pay_premium.zk` | No | ✅ Safe |
| identity | `create_claim.zk` | No | ✅ Uses safemath (Level 0 zk_only) |
| stablecoin | `open_position.zk` | No | ✅ Uses safemath `assert_lte_u64.zk` |
| stablecoin | `liquidate.zk` | No | ✅ Uses safemath `assert_lte_u64.zk` |
| bridge | `deposit.zk` | No | ✅ Fixed — real `merkle_root` |

---

### Key Takeaways

1. **`BaseDiv` is implemented** - Opcode 0x58 using binary exponentiation
2. **`LessThanOrEqual` opcode spec is verified sound** - Formally verified via Lean 4 at the specification level; gate-level implementation under active investigation
3. **`less_than_strict` is safe** - It's constrain-only (no return value manipulation)
4. **Cross-multiplication is still useful** - For simple ratio assertions without BaseDiv overhead
5. **stablecoin and identity use safemath** - Assertion gadgets (`assert_lte_u64.zk`) as workaround for LessThanOrEqual
6. **Bridge Merkle is fixed** - `deposit.zk` now uses real `merkle_root` opcode

**See also**:
- [Opcodes Reference](../../doc/src/arch/zk/opcodes.md) — Concise reference for contract authors
- [zkVM Primitive Layer](../../doc/src/arch/zk/zkvm_primitives.md) — Deep dive into opcode implementation

## Token Design Philosophy

DarkWow's token contracts (PromissoryNote, NativeToken) follow a **minimal infrastructure** philosophy:

> **Tokens are pipework, not reactors.**
>
> - Tokens move value. That's their job.
> - Business logic lives in smart contracts (DEX, stablecoin, etc.)
> - Tokens are the most frequently called contracts - simplicity minimizes attack surface
> - A bug in a token cascades to every operation; a bug in a smart contract is isolated

**Practical implications:**
- NativeToken: Minimal viable circuits for consensus (fees, rewards). One job, do it well.
- PromissoryNote: Poseidon-only design for DeFi tokens. Zero EC operations in ZK = zero heap bugs.

### PromissoryNote Design Principles

PromissoryNote is the cornerstone of DarkWow's DeFi ecosystem:

1. **Poseidon-only ZK circuits**: All cryptographic operations in ZK use Poseidon hash. No EC operations in ZK circuits.

2. **EC operations pushed to smart contracts**: Pedersen commitments and other EC operations happen in the contract verification layer, not in ZK circuits. This keeps ZK circuits simple and auditable.

3. **Privacy-first token model**:
   - Commitment = `poseidon_hash(pub, value, asset_id, spend_hook, user_data, blind)`
   - Nullifier = `poseidon_hash(secret, commitment)` or `poseidon_hash(secret, asset_id)` for auth
   - Value commitment = `poseidon_hash(value, blind)`

4. **Token authorization via backing proof**: MintV1 proves knowledge of the backing secret directly against the stored token_auth_parent commitment in a single step.

5. **Function IDs**:
   - `0x00` - TokenMintV1: Create a new token type
   - `0x01` - MintV1: Mint tokens of existing token type
   - `0x02` - BurnV1: Burn tokens
   - `0x03` - TransferV1: Private token transfer
   - `0x04` - OtcSwapV1: Atomic OTC token swap

This separation ensures ZK circuits remain minimal while complex business logic lives in audited smart contracts.

## Testing Infrastructure

Two pipelines for contract testing at different levels:

### Lightweight Pipeline (`bin/dwowd/src/tests/pipeline.rs`)

For deployment verification without ZK proof generation. Fast CI/CD checks.

```bash
# Test any contract by name
cargo test --package dwowd test_pipeline
CONTRACT_NAME=promissory_note cargo test --package dwowd test_pipeline
CONTRACT_NAME=stablecoin cargo test --package dwowd test_pipeline
```

### Heavyweight Pipeline (`bin/dwowd/src/tests/heavyweight_pipeline.rs`)

For full contract execution testing with real ZK proofs. Uses `ContractHarness` trait for generic ZK circuit access.

```bash
# Requires release mode or increased stack size due to halo2's computational intensity
cargo test --package dwowd --release test_dex_heavyweight
cargo test --package dwowd --release test_promissory_note_heavyweight

# Alternative: increase stack size
export RUST_MIN_STACK=16777216
cargo test --package dwowd test_dex_heavyweight
```

**Why the stack limit?** halo2's polynomial arithmetic uses deep recursion. Building multiple proving keys (4 circuits for DEX, 4 for PromissoryNote) exceeds the default ~8MB stack.

See [Contract Deployment Pipeline](../../doc/src/arch/dwowd_contract_pipeline.md) for full details.

### ContractHarness Trait

All test harnesses implement `ContractHarness` for generic ZK circuit access:

```rust
pub trait ContractHarness {
    fn name(&self) -> &str;
    fn circuits(&self) -> Vec<&'static str>;
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary>;
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey>;
}
```

Implemented by: `DexHarness`, `PromissoryNoteHarness`, `DarkbetExchangeHarness`, `DaoEscrowHarness`, `StablecoinHarness`, `NativeTokenHarness`

## Heavyweight Test Status

Heavyweight tests drive real contract endpoints through the accept path. The suite lives in
`bin/dwowd/src/tests/heavyweight_pipeline.rs` as `test_heavyweight_<contract>` (45 tests).

Run one:
```bash
cargo test --release -p dwowd test_heavyweight_stablecoin
```

Run the suite through the harness:
```bash
./bin/dwowd/src/tests/heavyweight.sh --all
```

Set `DWOW_TEST_LOGS=1` or a contract's own `msg!` output is discarded. Test status is the run's own
output, not a table here — a hand-maintained pass list goes stale the moment an arm changes.

## Official Contracts

All 32 contracts are official; there is no separate "unofficial" set. The canonical list with
maturity labels and crate names is [doc/src/contracts.md](../../doc/src/contracts.md). Nine are
genesis (listed under "What is here"); every contract directory has a `README.md` beside its
`manifest.toml`.

Architecture and implementation pages live under `doc/src/contract/` and are indexed by
[SUMMARY.md](../../doc/src/SUMMARY.md). The hand-written link list that used to live here pointed at
`doc/src/arch/` pages that do not exist; do not restore it — the catalog and `SUMMARY.md` are the
two indices.
