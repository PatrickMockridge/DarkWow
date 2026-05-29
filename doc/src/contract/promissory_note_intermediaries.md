# Promissory Note Intermediary Contract Audit

## Scope

This audit covers all 29 contracts under `src/contract/`, with focus on the
22 that interact with the Promissory Note (PN) contract. The audit assesses:

- Which PN opcodes each contract calls and whether they are correctly validated
- spend_hook policy and enforcement
- Redemption readiness and balance sheet tracking
- Gaps between documentation and implementation

## Ecosystem Map

```
                    ┌──────────────────────┐
                    │  Promissory Note (PN) │
                    │  Opcodes: 0x00-0x05   │
                    └──────┬───────────────┘
                           │
          ┌────────────────┼────────────────┐
          │                │                 │
     ┌────▼─────┐   ┌─────▼──────┐   ┌─────▼──────────┐
     │  Issuer   │   │    DEX     │   │  20× Token     │
     │(stablecoin)│   │(OTC swaps) │   │    Movers      │
     └──────────┘   └────────────┘   └────────────────┘
```

### By Role

| Role | Contracts | PN Opcodes Used |
|------|-----------|-----------------|
| **Issuer** | stablecoin | TransferV1 (0x04) only |
| **OTC** | dex | TransferV1 (0x04), otc_swap_v1 (0x05) |
| **Token movers** | 20 contracts | TransferV1 (0x04) only |
| **No PN interaction** | attestation, deployooor, identity, native_token, oracle, tender, tau (7) | — |

### Complete Contract Inventory

| # | Contract | PN Role | Child Calls | PN Opcodes | Validates Contract ID | Validates Value Commit | spend_hook Policy | Redemption Ready | Balance Sheet |
|---|----------|---------|-------------|------------|----------------------|------------------------|-------------------|------------------|---------------|
| 1 | **stablecoin** | Issuer | 6× TransferV1 | 0x04 only | Yes | Yes | Client-documented, no exec() | No | CDP_TOTAL_DEBT, CDP_TOTAL_COLLATERAL — no TOTAL_REDEEMED |
| 2 | **bridge** | Token mover | 4× TransferV1 | 0x04 only | Yes (gated) | Yes | None | No | Deposit/withdrawal trees |
| 3 | **dex** | OTC | 5× TransferV1 + otc_swap_v1 | 0x04, 0x05 | Indirect | No | None | No | Swap state only |
| 4 | **darkbet_exchange** | Token mover | 8× TransferV1 | 0x04 only | Yes | No | None | No | Market/order state |
| 5 | **labor_market** | Token mover | 9× TransferV1 + other | 0x04, 0x07, 0x0b | Yes | No | None | No | Job/dispute state |
| 6 | **lottery** | Token mover | 3× TransferV1 | 0x04 only | Yes | No | None | No | Ticket/pool state |
| 7 | **baccarat** | Token mover | 3× TransferV1 | 0x04 only | Yes | No | None | No | Bet/deck state |
| 8 | **darktoshi_dice** | Token mover | 4× TransferV1 (all as `money`) | 0x04 only | Yes | No | None | No | Dice bet state |
| 9 | **roulette** | Token mover | 3× TransferV1 | 0x04 only | Yes | No | None | No | Table/bet state |
| 10 | **slot** | Token mover | 3× TransferV1 | 0x04 only | Yes | No | None | No | Slot bet state |
| 11 | **betting_stake** | Token mover | 3× TransferV1 | 0x04 only | Yes | No | None | No | Stake/pool state |
| 12 | **pool_stake** | Token mover | 3× TransferV1 | 0x04 only | Yes | No | None | No | Pool/stake state |
| 13 | **game_room** | Token mover | 6× TransferV1 | 0x04 only | Yes | No | None | No | Game/player state |
| 14 | **escrow** | Token mover | 3× TransferV1 | 0x04 only | Yes | No | None | No | Escrow state |
| 15 | **auction** | Token mover | 1× TransferV1 | 0x04 only | Yes | No | None | No | Auction/bid state |
| 16 | **dao_escrow** | Token mover | 5× TransferV1 + other | 0x04, 0x0b | Yes | No | None | No | DAO/escrow state |
| 17 | **drain_protection** | Token mover | 2× TransferV1 | 0x04 only | Yes | No | None | No | Fund/guardian state |
| 18 | **insurance_market** | Token mover | 4× TransferV1 | 0x04 only | Yes | No | None | No | Policy/claim state |
| 19 | **otc_swap** | Token mover | 2× TransferV1 | 0x04 only | Yes | No | None | No | Swap state |
| 20 | **relayer_endowment** | Token mover | 2× TransferV1 | 0x04 only | Yes | No | None | No | Endowment state |
| 21 | **subscription** | Token mover | 3× TransferV1 | 0x04 only | Yes | No | None | No | Subscription state |
| 22 | **dex** (listed twice) | OTC | — | — | — | — | — | — | — |

## Critical Gaps

### Gap 1: RedeemV1 Has Zero Consumers

RedeemV1 (opcode 0x01) is fully implemented at the protocol layer — circuit, entrypoint,
client builder, wallet scanner. But **no contract in the ecosystem** calls it.

- Zero child call validations for `0x01`
- Zero RedeemParamsV1 in any contract's instruction processing
- Zero receipt coin handling in any contract

The redemption half of the bearer-instrument lifecycle is implemented but dormant.

### Gap 2: Stablecoin Has No Redemption

The only issuer contract has:

| Operation | PN Opcode | Exists? |
|-----------|-----------|---------|
| MintStableV1 | TransferV1 (0x04) | Yes |
| RepayStableV1 | TransferV1 (0x04) | Yes |
| RedeemStableV1 | — | **No** |

- MintStable uses TransferV1 to move stablecoins out of the contract
- RepayStable uses TransferV1 to move stablecoins back
- No RedeemV1 for stablecoin → underlying collateral conversion
- Liquidation uses forced TransferV1 (collateral payout), not RedeemV1
- The `exec()` spend_hook callback is **not implemented** anywhere in the entrypoint

### Gap 3: Bridge Uses TransferV1 for "Burns"

Bridge withdrawals use TransferV1 (0x04) to move wrapped tokens back to the contract,
not BurnV1 (0x03). The "burn" is simulated by transferring to the bridge contract.
The actual release happens on the external chain (BTC/XMR/etc. tx), not through
PN redemption.

This is architecturally correct for a bridge — the external chain is the source of
truth — but it means the bridge is not using PN's lifecycle functions as designed.

### Gap 4: spend_hook WASM-Validated and Actioned — RESOLVED (May 2026)

**Status**: Resolved. The PN entrypoint now validates spend_hook at the WASM level
and dispatches callbacks via `emit_spend_hook`.

**What changed**: `burn_v1()` checks that all inputs share the same `spend_hook`
(`SpendHookMismatch` if not). When `spend_hook != 0`, PN builds a
`BurnSpendHookPayload` and calls `emit_spend_hook(target_cid, payload)`. The
host writes the request to `Env.spend_hook_request`, and the blockchain pipeline
dispatches `__spend_hook` → `apply()` on the target contract in the same overlay
for atomicity.

See [Spend Hooks](../arch/zk/spend_hook.md) for the full callback mechanism.

### Gap 5: Receipt Coin spend_hook Enforced — RESOLVED (May 2026)

**Status**: Resolved. RedeemV1's ZK circuit now exposes `coin_spend_hook` as a
public input, and the entrypoint metadata function includes `params.output.spend_hook`
in the public input vector. Parent contracts can verify the receipt coin's spend_hook
is set to the issuer contract, preventing transfer of receipt coins.

### Gap 6: Output spend_hook Visible to Parent Contracts — RESOLVED (May 2026)

**Status**: Resolved. All 4 output-creating ZK circuits (Mint_V1, TokenMint_V1,
BlindOutput_V1, Redeem_V1) now expose `coin_spend_hook` as a public input.
Parent contracts can inspect the spend_hook of any output or receipt coin by
reading the proof's public inputs.

### Gap 7: Validation Helpers Are TransferV1-Only

`src/contract/promissory_note/src/validation.rs` provides:
- `validate_child_contract_id` — generic, works for any child call
- `validate_child_value_commit` — parses TransferParamsV1, **only** works for 0x04

Missing helpers:
- `validate_child_redeem_v1` — no equivalent for RedeemV1
- `validate_child_mint_v1` — no equivalent for MintV1
- `validate_child_burn_v1` — no equivalent for BurnV1
- `validate_child_spend_hook` — no helper to verify a child output's spend_hook

### Gap 8: No Balance Sheet Tracking for Redemption

No contract tracks:
- `TOTAL_REDEEMED` — how much has been redeemed
- `Outstanding = Minted - Redeemed` — current supply in circulation
- Per-token redemption tracking

The PN contract has `TOTAL_SUPPLY` but the stablecoin tracks `CDP_TOTAL_DEBT`
only for minted amounts. Redemption would reduce debt but there's no mechanism.

### Gap 9: Universal TransferV1 — Full PN Lifecycle Unused

Every single child call from every contract to PN is `TransferV1 (0x04)`:

```
TokenMintV1 (0x00) — 0 uses
RedeemV1     (0x01) — 0 uses
MintV1       (0x02) — 0 uses
BurnV1       (0x03) — 0 uses
TransferV1   (0x04) — 100% of PN child calls
OtcSwapV1    (0x05) — 0 PN uses (used only by dex→otc_swap contract)
```

All value creation, destruction, and movement is simulated through transfers.
The PN contract's issuer lifecycle (mint → transfer → burn/redeem) is fully
implemented but completely unused by the ecosystem.

### Gap 10: spend_hook Callback Implemented in Stablecoin — RESOLVED (May 2026)

**Status**: Resolved. The stablecoin now uses `define_contract_with_spend_hook!`
to export a `__spend_hook` WASM function. `process_spend_hook()` validates the
caller PN contract ID, checks nullifiers for replay, and builds update data for
the `apply` phase. `apply_spend_hook_callback()` records nullifiers in the
callback nullifier tree. The callback runs in the same overlay as the burn
for atomicity.

`SpendHookCallback (0x0B)` is an internal opcode reachable only via `__spend_hook`,
never via `exec()`. Calling it through `process_instruction` returns an error.

## Contract-by-Contract Analysis

### stablecoin — Issuer (CRITICAL)

**Child calls**: OpenPosition, AddCollateral, RemoveCollateral, MintStable,
RepayStable, Liquidate — all TransferV1 (0x04).

**Validates**: contract_id, value_commit.

**spend_hook policy**: Implemented (May 2026). Uses `define_contract_with_spend_hook!`
to export `__spend_hook`. `process_spend_hook()` validates caller PN contract ID,
checks nullifier replay, and builds `SpendHookCallbackUpdateV1` for the apply phase.
`SpendHookCallback (0x0B)` is an internal opcode reachable only via `__spend_hook`.

**Redemption readiness**: Partial. Spend_hook callback provides the infrastructure
for tracking burns/redemptions, but `RedeemStableV1` (direct PN::RedeemV1 child
call for collateral release) is not yet implemented.

**Balance sheet**: Tracks `CDP_TOTAL_DEBT`, `CDP_TOTAL_COLLATERAL`,
`CDP_ACCUMULATED_FEES`. Missing: `CDP_TOTAL_REDEEMED`, `CDP_OUTSTANDING`.

**Priority**: Highest. This is the sole issuer contract.

### bridge — Token Mover

**Child calls**: Deposit, Withdraw, CancelWithdraw, ExecuteGuaranteedWithdraw —
all TransferV1 (0x04).

**Validates**: contract_id (gated on non-zero config), value_commit (on
Withdraw, CancelWithdraw, ExecuteGuaranteedWithdraw).

**spend_hook policy**: None. Bridge manages its own HTLC/relayer infrastructure.

**Redemption readiness**: N/A. Bridge withdrawals are external-chain releases
(BTC/XMR/etc. tx), not PN redemptions. Architecturally correct.

**Balance sheet**: Deposit/withdrawal Merkle trees, guaranteed pending counter.
Adequate for bridge operations.

**Priority**: Low. Bridge is architecturally sound for its purpose.

### dex — OTC

**Child calls**: TransferV1 for swap execution and refunds, otc_swap_v1 (0x05)
for the actual swap and cancel operations.

**Validates**: contract_id indirectly (through otc_swap). No value_commit
validation for transfers.

**spend_hook policy**: None.

**Redemption readiness**: N/A. DEX is a pure exchange mechanism.

**Priority**: Low.

### Token Movers (auction, baccarat, betting_stake, darkbet_exchange, darktoshi_dice, dao_escrow, drain_protection, escrow, game_room, insurance_market, labor_market, lottery, otc_swap, pool_stake, relayer_endowment, roulette, slot, subscription)

**Universal pattern**: All 18 contracts use TransferV1 (0x04) exclusively for
PN interaction. All validate `validate_child_contract_id` to prevent routing
attacks. Some validate value_commit; none validate spend_hook.

**Architectural note**: None of these contracts are issuers — they don't mint,
burn, or redeem tokens. They only move existing tokens between participants.
TransferV1 is the correct (and only needed) opcode for this role.

**spend_hook policy**: None set spend_hook on outputs. None need to — they
don't issue restricted-capability tokens.

**Redemption readiness**: Not applicable. Token movers don't participate in
the mint/redeem lifecycle.

**Priority**: Low. All properly use TransferV1 + validate_child_contract_id.

## Independent Contracts (No PN Interaction)

attestation, deployooor, identity, native_token, oracle, tender, tau (7)

These contracts don't interact with PN and don't need to. No findings.

## Recommendations by Priority

### Immediate (Phase 3 Implementation)

1. **Wire spend_hook callback in stablecoin** — DONE (May 2026)
   - `__spend_hook` export via `define_contract_with_spend_hook!`
   - `process_spend_hook()` validates caller, checks replay
   - `apply_spend_hook_callback()` records nullifiers
   - `SpendHookCallback (0x0B)` internal opcode

2. **Add RedeemStableV1 to stablecoin** — NOT YET IMPLEMENTED
   - New opcode that calls PN::RedeemV1 as child call
   - Releases collateral proportionally to redeemed stablecoins
   - Updates CDP_TOTAL_DEBT and new CDP_TOTAL_REDEEMED
   - Separate feature from spend_hook callback mechanism

3. **Add validation helpers to PN**
   - `validate_child_redeem_v1` — parses RedeemParamsV1
   - `validate_child_spend_hook` — verifies output spend_hook matches expected
   - Update `validate_child_value_commit` to dispatch on opcode

4. **Expose output spend_hook as circuit public input** — DONE (May 2026)
   - All 4 output-creating circuits (Mint_V1, TokenMint_V1, BlindOutput_V1,
     Redeem_V1) now expose `coin_spend_hook` as a public input

### Short-term (Phase 2 Documentation)

5. **Update stablecoin.md** — DONE (May 2026)
   - SpendHookCallback function, callback architecture, nullifier tracking

6. **Update bridge.md** — clarify withdrawal vs redemption distinction

7. **Update promissory_note.md** — DONE (May 2026)
   - Circuit tables updated, spend_hook callback mechanism, best practices

### Medium-term

8. **Enforce receipt coin spend_hook = caller** — DONE (May 2026)
   - RedeemV1 now exposes receipt spend_hook as public input
   - Parent contracts can verify receipt coin spend_hook matches issuer

9. **Add CDP_TOTAL_REDEEMED** tracking to stablecoin
   - Spend_hook nullifier tracking provides the data; formal aggregation TBD

10. **Entrypoint hardening** — DONE (May 2026)
    - PN burn_v1 validates spend_hook consistency at WASM level
    - Dispatches callbacks via emit_spend_hook host function
    - Blockchain pipeline handles dispatch with overlay atomicity

## Summary

| Metric | Count |
|--------|-------|
| Total contracts | 29 |
| PN-interacting | 22 |
| Issuers | 1 (stablecoin) |
| Token movers | 20 (+ dex as OTC) |
| Independent | 7 |
| Contracts using 0x00 (TokenMintV1) | **0** |
| Contracts using 0x01 (RedeemV1) | **0** |
| Contracts using 0x02 (MintV1) | **0** |
| Contracts using 0x03 (BurnV1) | **0** |
| Contracts using 0x04 (TransferV1) | **22** |
| Contracts with exec() callback | **0** |
| Contracts with redemption support | **0** |
| Contracts with balance sheet tracking | 1 (stablecoin, partial) |

**Bottom line (updated May 2026)**: The PN contract implements a complete
bearer-instrument lifecycle — mint, transfer, burn, redeem, OTC swap. The
ecosystem primarily uses transfer, but the spend_hook callback mechanism is
now fully wired: PN burn_v1 dispatches callbacks via `emit_spend_hook`, the
blockchain pipeline runs `__spend_hook` → `apply()` in the same overlay for
atomicity, and the stablecoin implements a reference spend_hook receiver with
nullifier tracking. All 5 ZK circuits expose `coin_spend_hook` as a public
input, enabling parent contracts to verify spend_hook on any coin. RedeemV1
and direct MintV1/BurnV1 adoption remain as next steps.
