# Contract MVP Status

> **Prerequisite reading**: Understanding why opcodes are blockers requires understanding [Field Arithmetic Constraints](field_arithmetic.md). Most "missing opcodes" are not missing because no one thought to implement them — they are missing because implementing them correctly in a ZK circuit is genuinely hard. This document explains which opcodes are missing, which are experimental, and what the path to production looks like.

This document tracks the blockers to reaching MVP for each contract in `src/contract/`. An MVP means: a functional, end-to-end testable version where the core use case works on-chain with real ZK proof verification.

---

## Summary Table

| Contract | Opcode Blockers | Architecture Blockers | MVP Ready |
|----------|----------------|-----------------------|-----------|
| `money` | None | Integration testing | **Yes** |
| `dao` | None | Governance token integration | **Yes** |
| `dex` | None | Order matching logic | Partial |
| `bridge` | None | Light client for external chain verification | Partial |
| `identity` | None — uses safemath (Level 0 zk_only refactor) | Integration testing | Partial |
| `stablecoin` | None — uses safemath `assert_lte_u64_v1.zk` | P2P oracle, CDP notes | Partial |
| `escrow` | None | ZK circuit compilation, Money integration | Partial |
| `dao_escrow` | None | Entry point wiring, spend hook, Money integration | Partial |

**Key insight**: `LessThanOrEqual` (0x55), `IsEqualBase` (0x54), `NotBase` (0x56), and `BaseLtStrict` (0x57) are implemented in the zkVM (commit `41b0629e0`). `stablecoin` and `identity` now use [safemath](../safemath.md) assertion gadgets instead of experimental `LessThanOrEqual`. Ratio checks use cross-multiplication via `base_mul + less_than_strict` — no `BaseDiv` needed. The experimental opcodes remain grey-market goods — see [zkVM Primitive Layer](zkvm_primitives.md) for production readiness requirements.

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
| `deposit_v1.zk` | Verified | Uses real `merkle_root` opcode with `MerklePath` type |
| `withdraw_v1.zk` | Corrected in prior session | Uses `constrain_equal_base` |

### Blockers

**Opcode layer**: None.

**Architecture blockers**:

1. **No external block header verification** — The `external_block_hash` public input is accepted but not verified against an actual source chain. A real bridge needs a light client proof that commits to the external chain's block header.

2. **No withdrawal finality** — After a withdrawal is processed, there is no mechanism to prove the external chain transaction was finalized.

**What it needs for full MVP**: Integrate a light client proof system for the external chain (e.g., a simple header relay) to verify `external_block_hash`.

---

## `identity` — Partial MVP (Architecture Blockers Only)

**Location**: `src/contract/identity/`

**Architecture**: Level 0 (zk_only) — verifier learns only "proof valid/invalid", not predicate result.

### Functions
InitializeV1 (0x00), IssueCredentialV1 (0x01), RevokeCredentialV1 (0x02), CreateClaimV1 (0x03), VerifyClaimV1 (0x04)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `issue_credential_v1.zk` | Unread | Likely needs review |
| `create_claim_v1.zk` | Verified | Uses safemath `assert_lte_u64_v1.zk` pattern |

**Opcode layer**: None. All LTE checks use safemath assertion gadgets. No experimental opcodes.

### Blockers

**Architecture**: Integration testing — no end-to-end test of issue → claim → verify flow exists yet.

**What it needs**: Integration test for the full credential lifecycle. Review of `issue_credential_v1.zk`. `verify_claim_v1.zk` is stubbed.

**See also**:
- [Safemath](../safemath.md) for the safemath integration guide.
- [identity contract README](../../src/contract/identity/README.md) for the privacy gradient design.

---

## `stablecoin` — Partial MVP (Architecture Blockers Only)

**Location**: `src/contract/stablecoin/`

**Architecture**: Synthetix-style pooled debt model. All collateral backs all debt — no individual position tracking.

### Functions
InitializeV1 (0x00), DepositCollateralV1 (0x01), WithdrawCollateralV1 (0x02), MintStableV1 (0x03), RepayStableV1 (0x04), LiquidateV1 (0x05), UpdateConfigV1 (0x06)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `open_position_v1.zk` | Verified | Uses safemath `assert_lte_u64_v1.zk` pattern for collateralization |
| `mint_stable_v1.zk` | Unread | Base arithmetic uses existing opcodes |
| `liquidate_v1.zk` | Verified | Uses safemath `assert_lte_u64_v1.zk` pattern for reward bounds |

**Opcode layer**: None. All LTE checks use safemath assertion gadgets instead of experimental `LessThanOrEqual` opcode.

### Blockers

**Architecture blockers**:

1. **No P2P oracle** — TWAP price is expected as an external oracle input. The XMR/DRK AMM pool does not yet exist on-chain.

2. **CDP Note integration stubbed** — The money contract's `spend_hook` pointing to the CDP engine is designed but not implemented.

**What it needs**: First: P2P oracle / AMM integration to supply TWAP price. Second: CDP Note integration with money contract. Third: integration testing of the full lifecycle.

> **Note on division**: Ratio checks use cross-multiplication (see `dao/exec.zk` lines 118-126 for the exact pattern). `BaseDiv` is not needed and is not a blocker.

> **Note on safemath**: The stablecoin uses [darkfi-safemath](https://codeberg.org/rusticml/darkfi-safemath) assertion gadgets instead of `LessThanOrEqual`. See [Safemath](../safemath.md) for details.

**See also**:
- [zkVM Primitive Layer](zkvm_primitives.md) for the full opcode dependency analysis.
- [Safemath](../safemath.md) for the safemath integration guide.

---

## `escrow` — Partial MVP

**Location**: `src/contract/escrow/`

### Functions
InitializeV1 (0x00), CreateEscrowV1 (0x01), FundV1 (0x02), ClaimV1 (0x03), RefundV1 (0x04), CancelV1 (0x05)

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `create_escrow_v1.zk` | Placeholder | Uses `poseidon_hash`, `ec_mul_base` — all existing |
| `fund_v1.zk` | Placeholder | Pedersen commitment, needs merkle integration |
| `claim_v1.zk` | Placeholder | Key derivation + nullifier, needs ZK wiring |
| `refund_v1.zk` | Placeholder | Uses `less_than_strict` for timeout check (constrain-only) |

### Blockers

**Opcode layer**: None. All required opcodes exist:
- `poseidon_hash` for commitment
- `ec_mul_base` for key derivation
- `less_than_strict` for timeout verification (constrain-only, no output)

**Architecture blockers**:

1. **ZK circuit compilation** — `.zk` files exist but need to be compiled to `.zk.bin`

2. **Entry point wiring** — `get_metadata()` and `process_instruction()` are stubs

3. **Money integration** — Phase 2 uses money contract's `spend_hook` for fund release. Phase 1 (standalone) doesn't need this.

**What it needs**: ZK circuit compilation, entry point implementation, state management for the escrow state machine (Created → Funded → Claimed/Refunded).

**See also**: [Escrow Contract MVP](./escrow.md) for the full analysis.

---

## `dao_escrow` — Partial MVP (Simplified)

**Location**: `src/contract/dao_escrow/`

### Functions
InitializeV1 (0x00), UpdateV1 (0x01), PayPremiumV1 (0x02), WithdrawV1 (0x03)

**Note**: Claims are handled by DAO treasury — no parallel voting in DAO-Escrow.

### Circuit Status

| Circuit | Status | Notes |
|---------|--------|-------|
| `init_v1.zk` | Complete | Links endowment to DAO bulla |
| `pay_premium_v1.zk` | Complete | Creates time-limited membership note |

### Blockers

**Opcode layer**: None. MVP uses only proven opcodes.

**Architecture blockers**:

1. **Entry point wiring** — `process_instruction()` and `process_update()` are stubs

2. **Membership note spend hook** — Money contract integration to check expiry at spend time

3. **Money integration** — Endowment pool needs to connect to actual token holdings

**What it needs**: Entry point wiring, spend hook integration, money integration.

**Extended features** (see roadmap in dao_escrow.md): Claims DAO voting, multi-tier governance, mutual insurance — all possible with existing opcodes (no opcode barriers).

**See also**: [DAO-Escrow Contract](./dao_escrow.md) for the full analysis including roadmap.

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
  │ (needs light client for external chain)
  ▼
identity ◄──────────── (LessThanOrEqual, ──► stablecoin
  │                   IsEqualBase: experimental)  │
  │                                               │
  └────────────────── (P2P oracle) ──────────────┘

escrow ──────► money (spend_hook integration)
  │
  │ (Phase 1: standalone, no money integration needed)
  ▼
dao_escrow ──────► dao (treasury via Propose/Vote/Exec)
  │                │
  │                └── Claims handled by DAO treasury
  │
  │ (membership note spend hook needs money integration)
  ▼
money (token holdings)
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
- [DarkFi Escrow Contract](../../src/contract/escrow/)
- [DarkFi DAO-Escrow Contract](../../src/contract/dao_escrow/)
