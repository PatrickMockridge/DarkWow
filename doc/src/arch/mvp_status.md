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
| `identity` | Circuit code not updated | — | No (needs circuit update) |
| `stablecoin` | `BaseDiv` not implemented | P2P oracle, CDP notes | No |

**Key insight**: `LessThanOrEqual` (0x55), `IsEqualBase` (0x54), `NotBase` (0x56), and `BaseLtStrict` (0x57) are now implemented in the zkVM (commit `41b0629e0`). `identity` and `stablecoin` are no longer blocked at the opcode layer — their circuits must now be updated to use the new opcodes. `BaseDiv` remains the only unimplemented opcode blocking `stablecoin`.

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

## `identity` — Needs Circuit Update

**Location**: `src/contract/identity/`

### Functions
InitializeV1 (0x00), IssueCredentialV1 (0x01), RevokeCredentialV1 (0x02), CreateClaimV1 (0x03), VerifyClaimV1 (0x04)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `issue_credential_v1.zk` | Unread | Likely needs review |
| `create_claim_v1.zk` | Has placeholder | Predicate verification uses placeholder that always passes |

### Blockers

**Opcode layer**: `LessThanOrEqual` (0x55) and `IsEqualBase` (0x54) are now implemented in the zkVM. The circuit code must be updated to use them.

**Circuit update needed**: `create_claim_v1.zk` currently uses a placeholder predicate that always passes. It should be updated to use `LessThanOrEqual` for threshold comparisons (e.g., `age >= 18`) and `IsEqualBase` for type comparisons.

**What it needs**: Update `create_claim_v1.zk` to use `less_than_or_equal(a, b)` and `is_equal_base(a, b)` in place of the placeholder predicate.

**See also**: [zkVM Primitive Layer](zkvm_primitives.md) for the full opcode dependency analysis.

---

## `stablecoin` — Blocked on Opcodes

**Location**: `src/contract/stablecoin/`

### Functions
InitializeV1 (0x00), OpenPositionV1 (0x01), AddCollateralV1 (0x02), RemoveCollateralV1 (0x03), MintStableV1 (0x04), RepayStableV1 (0x05), LiquidateV1 (0x06), UpdateConfigV1 (0x07)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `open_position_v1.zk` | Circuit update needed | Uses `LessThanOrEqual` reasoning, needs actual opcode |
| `mint_stable_v1.zk` | Corrected | Base arithmetic uses existing `base_add` opcode |
| `liquidate_v1.zk` | Circuit update needed | Uses `LessThanOrEqual` reasoning, needs actual opcode |

### Blockers

**Opcode layer**:

1. **`LessThanOrEqual` (0x55) now implemented** — The zkVM now has `LessThanOrEqual`. The circuits need to be updated to use it for collateralization checks.

2. **`BaseDiv` not implemented** — Required for:
   - Computing `collateral_ratio = collateral / debt`
   - TWAP price computation: `exchange_rate = output_amount / input_amount`

**Architecture blockers**:

3. **No P2P oracle** — The design requires a NETHER/DRK AMM pool for TWAP price discovery, but this does not yet exist on-chain.

4. **CDP Note integration stubbed** — The money contract's `spend_hook` pointing to the CDP engine is designed but not implemented.

**What it needs**: First: update circuits to use `LessThanOrEqual`. Second: implement `BaseDiv`. Third: P2P oracle / AMM integration. This is a multi-step dependency chain.

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
identity ◄─── (LessThanOrEqual, ───► stablecoin
  │          IsEqualBase: experimental)   │
  │                                         │
  └──────── BaseDiv ────────────────────────┘
```

---

## The Single Highest-Leverage Primitive

**`LessThanOrEqual`** (0x55), **`IsEqualBase`** (0x54), **`NotBase`** (0x56), and **`BaseLtStrict`** (0x57) are now implemented in the zkVM (commit `41b0629e0`). The next highest-leverage primitive is **`BaseDiv`** — it unblocks stablecoin's TWAP price computation and collateral ratio checks.

```zk
# Once LessThanOrEqual exists, IsEqualBase is also trivially available:
less_than_or_equal(a, b) = is_equal_base(a, b) OR less_than_loose(a, b)
is_equal_base(a, b) = 1 - less_than_loose(a, b) - less_than_loose(b, a)
```

This means implementing `LessThanOrEqual` also gives you `IsEqualBase` for free.

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
