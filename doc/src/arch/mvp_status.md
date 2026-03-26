# Contract MVP Status

This document tracks the blockers to reaching MVP for each contract in `src/contract/`. An MVP means: a functional, end-to-end testable version where the core use case works on-chain with real ZK proof verification.

---

## Summary Table

| Contract | Opcode Blockers | Architecture Blockers | MVP Ready |
|----------|----------------|-----------------------|-----------|
| `money` | None | Integration testing | **Yes** |
| `dao` | None | Governance token integration | **Yes** |
| `dex` | None | Order matching logic | Partial |
| `bridge` | None (placeholder noted) | Merkle verification, light client | Partial |
| `identity` | `LessThanOrEqual`, `IsEqualBase` integrated (experimental) | Integration testing | Partial |
| `stablecoin` | None (ratio checks use cross-multiplication — see `dao/exec.zk`) | P2P oracle, CDP notes | Partial |

**Key insight**: `LessThanOrEqual` (0x55), `IsEqualBase` (0x54), `NotBase` (0x56), and `BaseLtStrict` (0x57) are implemented in the zkVM (commit `41b0629e0`). `identity` and `stablecoin` have been updated to use `LessThanOrEqual`. Ratio checks (e.g., `collateral / debt < threshold`) use cross-multiplication via `base_mul + less_than_strict` — no `BaseDiv` needed. All experimental opcodes are grey-market goods — see [zkVM Primitive Layer](zkvm_primitives.md) for production readiness requirements.

---

## `money` — MVP Ready

**Location**: `src/contract/money/`

### Functions
FeeV1 (0x00), GenesisMintV1 (0x01), PoWRewardV1 (0x02), TransferV1 (0x03), OtcSwapV1 (0x04), AuthTokenMintV1 (0x05), AuthTokenFreezeV1 (0x06), TokenMintV1 (0x07), BurnV1 (0x08)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `mint_v1.zk` | Verified | Uses `ec_mul_short`, `ec_add`, `poseidon_hash` — all existing opcodes |
| `burn_v1.zk` | Verified | Full Merkle membership proof, nullifier derivation, signature verification |
| `auth_token_mint_v1.zk` | Verified | Token authority derivation via `ec_mul_base` + `poseidon_hash` |
| `fee_v1.zk` | Unread | — |
| `token_mint_v1.zk` | Unread | — |

### Blockers

**Opcode layer**: None. All circuits use only existing opcodes.

**Architecture**: End-to-end integration testing with the money contract's `spend_hook` and `auth_token` system. The OTC swap settlement (`OtcSwapV1`) is real plumbing that connects to the DEX.

**What it needs**: A full integration test demonstrating mint → transfer → burn lifecycle with real proofs and on-chain state verification.

---

## `dao` — MVP Ready

**Location**: `src/contract/dao/`

### Functions
Mint (0x00), Propose (0x01), Vote (0x02), Exec (0x03), AuthMoneyTransfer (0x04)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `propose-main.zk` | Verified | Merkle root of DAO bulla, proposer token balance check via `less_than_strict` |
| `exec.zk` | Verified | Timing enforcement, quorum check, approval ratio check using only existing opcodes |
| `vote-main.zk` | Unread | — |
| `mint.zk` | Unread | — |
| `propose-input.zk` | Unread | — |
| `vote-input.zk` | Unread | — |
| `early-exec.zk` | Unread | — |
| `auth-money-transfer.zk` | Unread | — |

### Blockers

**Opcode layer**: None. The `less_than_strict` usage in `exec.zk` (lines 97, 116) is correct — it constrains the circuit to fail if quorum is not met or if the proposal has not expired. This is exactly the right use of a constrain-only opcode.

**Architecture**: Governance token integration with the money contract. The DAO needs a funded treasury (money contract integration) and the `auth-money-transfer` hook to authorize treasury movements.

**What it needs**: End-to-end test of: governance token minting → proposal creation → voting → early execution (if quorum met) → treasury disbursement via `AuthMoneyTransfer`.

---

## `dex` — Partial MVP

**Location**: `src/contract/dex/`

### Functions
InitializeV1 (0x00), CreateSwapV1 (0x01), AcceptSwapV1 (0x02), ExecuteSwapV1 (0x03), CancelSwapV1 (0x04), UpdateConfigV1 (0x05)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `create_swap_v1.zk` | Corrected | Uses `constrain_equal_base`, not `assert_equal` |
| `accept_swap_v1.zk` | Corrected | Uses `constrain_equal_base` |
| `execute_swap_v1.zk` | Corrected | Uses `constrain_equal_base`, `range_check`, `bool_check` |
| `cancel_swap_v1.zk` | Corrected | Uses `constrain_equal_base` |

### Blockers

**Opcode layer**: None.

**Architecture blockers**:

1. **Manual matching required** — The current flow is `CreateSwap → AcceptSwap → ExecuteSwap`. There is no automatic order matching. After both parties post their locks, a third party (or one of the participants) must call `ExecuteSwap` to settle. This is an atomic swap, not an AMM or order book.

2. **No price or amount comparison** — `execute_swap_v1.zk` verifies amounts are valid via `range_check(64, amount)` but does not compare Alice's offered amount against Bob's requested amount. For a swap "Alice offers 100 token A for 50 token B", the circuit does not enforce that these amounts satisfy each other's requirements.

3. **No fill logic** — partial fills are not supported. If a swap is posted for 100 tokens but someone wants to fill only 50, there is no mechanism for that.

**What it needs for full MVP**: Either document the atomic swap flow explicitly (manual matching is acceptable for a basic MVP), or implement `LessThanOrEqual` to enable amount comparison and partial fills.

---

## `bridge` — Partial MVP

**Location**: `src/contract/bridge/`

### Functions
InitializeV1 (0x00), DepositV1 (0x01), WithdrawV1 (0x02), UpdateConfigV1 (0x03)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `deposit_v1.zk` | Verified | Commitment derivation, range check. **Merkle proof is a placeholder** (see below) |
| `withdraw_v1.zk` | Corrected in prior session | Uses `constrain_equal_base` |

### Blockers

**Opcode layer**: None.

**Architecture blockers**:

1. **Merkle proof is a placeholder** — `deposit_v1.zk` line 54 computes:
   ```zk
   merkle_check = poseidon_hash(deposit_leaf, merkle_path_0, merkle_path_1);
   ```
   This is **not** real Merkle verification. It should use the `merkle_root` opcode with the `MerklePath` type. The current implementation accepts any `merkle_path_*` values without cryptographic verification.

2. **No external block header verification** — The `external_block_hash` public input is accepted but not verified against an actual source chain. A real bridge needs a light client proof that commits to the external chain's block header.

3. **No withdrawal finality** — After a withdrawal is processed, there is no mechanism to prove the external chain transaction was finalized.

**What it needs for full MVP**: Replace the placeholder Merkle check with real `merkle_root` opcode verification, and integrate a light client proof system for the external chain (e.g., a simple header relay).

---

## `identity` — Partial MVP (Experimental Opcode Integrated)

**Location**: `src/contract/identity/`

### Functions
InitializeV1 (0x00), IssueCredentialV1 (0x01), RevokeCredentialV1 (0x02), CreateClaimV1 (0x03), VerifyClaimV1 (0x04)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `issue_credential_v1.zk` | Unread | Likely needs review |
| `create_claim_v1.zk` | Verified (experimental) | Uses `less_than_or_equal` for threshold checks. **Grey-market opcode.** |

### Blockers

**Opcode layer**: `LessThanOrEqual` (0x55) and `IsEqualBase` (0x54) are implemented and integrated in `create_claim_v1.zk`. Both are **experimental grey-market goods** — see [zkVM Primitive Layer](zkvm_primitives.md) for the delta-invert soundness concern and what production readiness requires.

**Architecture**: Integration testing — no end-to-end test of issue → claim → verify flow exists yet.

**What it needs**: Integration test for the full credential lifecycle. `IsEqualBase` delta-invert soundness fix. Review of `issue_credential_v1.zk`.

**See also**: [zkVM Primitive Layer](zkvm_primitives.md) for the full opcode dependency analysis.

---

## `stablecoin` — Blocked on Opcodes and Architecture

**Location**: `src/contract/stablecoin/`

### Functions
InitializeV1 (0x00), OpenPositionV1 (0x01), AddCollateralV1 (0x02), RemoveCollateralV1 (0x03), MintStableV1 (0x04), RepayStableV1 (0x05), LiquidateV1 (0x06), UpdateConfigV1 (0x07)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `open_position_v1.zk` | Verified (experimental) | Uses `less_than_or_equal` for 200% collateralization check. **Grey-market opcode.** |
| `mint_stable_v1.zk` | Corrected | Base arithmetic uses existing `base_add` opcode |
| `liquidate_v1.zk` | Partial (experimental) | Uses `less_than_or_equal` for reward bounds check. Ratio check uses cross-multiplication (`base_mul + less_than_strict`). **Grey-market opcode.** |

### Blockers

**Opcode layer**:

1. **`LessThanOrEqual` (0x55) integrated but experimental** — `open_position_v1.zk` and `liquidate_v1.zk` now use it. Grey-market goods — see [zkVM Primitive Layer](zkvm_primitives.md). No other opcode blockers.

**Architecture blockers**:

2. **No P2P oracle** — TWAP price is expected as an external oracle input. The NETHER/DRK AMM pool does not yet exist on-chain.

3. **CDP Note integration stubbed** — The money contract's `spend_hook` pointing to the CDP engine is designed but not implemented.

**What it needs**: First: P2P oracle / AMM integration to supply TWAP price. Second: CDP Note integration with money contract. Third: integration testing of the full lifecycle.

> **Note on division**: Ratio checks use cross-multiplication (see `dao/exec.zk` lines 118-126 for the exact pattern). `BaseDiv` is not needed and is not a blocker.

**See also**: [zkVM Primitive Layer](zkvm_primitives.md) for the full opcode dependency analysis.

---

## Cross-Contract Dependency Chain

```
money ──────────────────────────► dao
  │                                  │
  │                                  ▼
  │                            governance token
  │                                  │
  ▼                                  │
dex ◄────────────────────────────────┘
  │
  │ (needs order matching resolved)
  ▼
bridge
  │
  │ (needs Merkle verification fixed)
  ▼
identity ◄──────────── (LessThanOrEqual, ──► stablecoin
  │                   IsEqualBase: experimental)  │
  │                                               │
  └────────────────── (P2P oracle) ──────────────┘
```

---

## The Single Highest-Leverage Primitive

**`LessThanOrEqual`** (0x55), **`IsEqualBase`** (0x54), **`NotBase`** (0x56), and **`BaseLtStrict`** (0x57) are now implemented in the zkVM (commit `41b0629e0`). There are **no remaining opcode blockers** for any planned contract feature.

**Key pattern**: Ratio checks in ZK circuits use cross-multiplication, not division. To prove `a/b < c/d`, assert `a*d < b*c` using `base_mul + less_than_strict`. This is demonstrated in `dao/exec.zk` lines 118-126. TWAP prices are oracle inputs, not computed in-circuit.

```zk
# Ratio check (no division needed):
lhs = base_mul(collateral_value, 1);
rhs = base_mul(liquidation_threshold, debt_value);
less_than_strict(lhs, rhs);  # Proves collateral/debt < threshold
```

**See**: [zkVM Primitive Layer](zkvm_primitives.md) for implementation guidance.

---

## References

- [Private Authorization Layer](privauth.md)
- [Composability & General Primitives](composability.md)
- [zkVM Primitive Layer](zkvm_primitives.md)
- [DarkFi Identity Contract](../../src/contract/identity/)
- [DarkFi DEX Contract](../../src/contract/dex/)
- [DarkFi Bridge Contract](../../src/contract/bridge/)
- [DarkFi Stablecoin Contract](../../src/contract/stablecoin/)
- [DarkFi Money Contract](../../src/contract/money/)
- [DarkFi DAO Contract](../../src/contract/dao/)
