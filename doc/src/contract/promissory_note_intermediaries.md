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

### Gap 4: spend_hook ZK-Constrained, WASM-Unvalidated

The PN entrypoint passes `input.spend_hook` as a ZK public input in BurnV1 and
TransferV1 proofs (lines 337, 375, 839, 964 of `entrypoint/mod.rs`). This means
the spend_hook value is cryptographically bound to the proof — a prover cannot
change it without breaking the proof.

However, **the WASM layer never reads or acts on spend_hook**:

```rust
// PN entrypoint passes spend_hook to ZK but never validates it:
zk_public_inputs.push((
    PROMISSORY_NOTE_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
    vec![
        input.nullifier.inner(),
        // ...
        input.spend_hook,  // ← ZK-constrained, WASM-ignored
        input.signature_public,
    ],
));
```

There is **no callback mechanism**: the PN contract never calls into the contract
identified by spend_hook. The field is a ghost — cryptographically real but
operationally dead.

### Gap 5: Receipt Coin spend_hook Unenforced

RedeemV1's ZK circuit exposes `coin_value` as a public input (to prove value=0),
but does **not** expose the receipt coin's `spend_hook` as a public input. Any
caller can set the receipt coin's spend_hook to anything. There is no protocol-level
guarantee that receipt coins are non-transferable (spend_hook = issuer contract).

### Gap 6: Output spend_hook Invisible to Parent Contracts

BlindOutput_V1 circuit includes spend_hook in the coin hash (so it affects the
coin commitment) but never makes it a **public input**. Parent contracts cannot
inspect what spend_hook their output coins carry without trusting the caller's
word.

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

### Gap 10: spend_hook exec() Callback Missing in Stablecoin

The stablecoin client code (`mint_stable_v1.rs`, `open_position_v1.rs`,
`liquidate_v1.rs`) extensively documents a spend_hook callback architecture:

```
1. User calls PromissoryNote::BurnV1 with spend_hook = stablecoin contract
2. The spend_hook triggers stablecoin's exec() callback
3. Stablecoin mints stablecoins atomically
```

But the stablecoin entrypoint has **no `exec()` function**. The callback
mechanism described in client documentation doesn't exist in the runtime.

## Contract-by-Contract Analysis

### stablecoin — Issuer (CRITICAL)

**Child calls**: OpenPosition, AddCollateral, RemoveCollateral, MintStable,
RepayStable, Liquidate — all TransferV1 (0x04).

**Validates**: contract_id, value_commit.

**spend_hook policy**: Client docs describe a rich spend_hook-based architecture
(mint via spend_hook callback, liquidation via spend_hook trigger), but the
entrypoint implements none of it.

**Redemption readiness**: No. Needs:
1. `RedeemStableV1` opcode dispatching `RedeemV1`
2. `exec()` callback to handle spend_hook triggers
3. `TOTAL_REDEEMED` tracking in config DB
4. Redemption-rate-aware collateral release logic

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

1. **Wire spend_hook exec() in stablecoin**
   - Implement `exec()` callback handling BurnV1 spend_hook triggers
   - Dispatch based on user_data to handle mint/liquidate/redeem
   - Required for atomic cross-contract operations

2. **Add RedeemStableV1 to stablecoin**
   - New opcode that calls PN::RedeemV1 as child call
   - Releases collateral proportionally to redeemed stablecoins
   - Updates CDP_TOTAL_DEBT and new CDP_TOTAL_REDEEMED

3. **Add validation helpers to PN**
   - `validate_child_redeem_v1` — parses RedeemParamsV1
   - `validate_child_spend_hook` — verifies output spend_hook matches expected
   - Update `validate_child_value_commit` to dispatch on opcode

4. **Expose output spend_hook as BlindOutput_V1 public input**
   - Required for parent contracts to verify output coin spend_hook
   - Enables spend_hook validation in cross-contract calls

### Short-term (Phase 2 Documentation)

5. **Update stablecoin.md** — document new RedeemStableV1, spend_hook
   architecture, balance sheet tracking

6. **Update bridge.md** — clarify withdrawal vs redemption distinction

7. **Update promissory_note.md** — add cross-contract spend_hook section

### Medium-term

8. **Enforce receipt coin spend_hook = caller** — expose receipt spend_hook
   as RedeemV1 public input; validate in entrypoint that receipt coin
   spend_hook equals the calling contract

9. **Add CDP_TOTAL_REDEEMED** tracking to stablecoin

10. **Consider entrypoint hardening** — validate spend_hook at WASM level in
    PN entrypoint for BurnV1 and TransferV1 (not just ZK)

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

**Bottom line**: The PN contract implements a complete bearer-instrument
lifecycle — mint, transfer, burn, redeem, OTC swap. But the ecosystem uses
only transfer. The issuer contract (stablecoin) simulates minting and burning
via transfers. Redemption is implemented at the protocol layer but has no
application-layer consumer. The spend_hook mechanism, which would enable
atomic cross-contract operations, is ZK-constrained but not enforced or
actioned at the WASM level.
