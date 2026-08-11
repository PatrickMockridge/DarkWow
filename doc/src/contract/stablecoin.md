# DarkWow Stablecoin Architecture

> **Contract specification.** For developer integration details, see [Stablecoin Dev Guide](../dev/contracts/stablecoin.md).

*Privacy-preserving collateralized stablecoin with configurable models and multi-collateral support.*

## Purse Composition

Stablecoin composes with the genesis [Purse](purse.md) primitive. Total debt, total
collateral, and accumulated fees are tracked in Purses rather than via manual config
DB key arithmetic. MintStable calls `Purse::DepositV1` to increase the debt counter.
RepayStable and Liquidate call `Purse::WithdrawV1`. The Purse contract handles
balance integrity via Pedersen commitments — the stablecoin contract reads Purse
balances for collateralization ratio checks and governance reports.

## Overview

The DarkWow stablecoin is a privacy-preserving collateralized stablecoin that supports:

- **Multi-collateral**: XMR, DRKW, and ETH (via bridge) as collateral
- **Configurable models**: PooledDebt, Liquity, Fractional, or IndividualCDP
- **Hot/Cold separation**: Cheap user operations, precise governance
- **Dead man switch**: Emergency shutdown if executive authority unresponsive
- **Full ZK privacy**: All positions, amounts, and identities hidden

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Stablecoin Contract                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │   Collateral │    │   Debt Pool  │    │    PI        │       │
│  │    Pools    │◄──▶│  (global)   │◄──▶│  Controller │       │
│  └──────────────┘    └──────────────┘    └──────────────┘       │
│                                                                   │
│  Collateral Types: XMR, DRKW, ETH                                  │
│  Models: PooledDebt | Liquity | Fractional | IndividualCDP        │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Configurable Models

The stablecoin deployer selects the model at initialization:

| Model | Min Collateral | Liquidation | Governance |
|-------|---------------|-------------|------------|
| **PooledDebt** | 150% | Global pool | PI Controller |
| **Liquity** | 110% | Stability pool | None |
| **Fractional** | 80% | Mixed | Partial algorithmic |
| **IndividualCDP** | 150% | Per-position | Per-asset |

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Initialize stablecoin with model and parameters |
| `OpenPositionV1` | `0x01` | Open a new CDP position |
| `AddCollateralV1` | `0x02` | Add collateral to existing position |
| `RemoveCollateralV1` | `0x03` | Remove collateral (subject to ratio check) |
| `MintStableV1` | `0x04` | Mint stablecoin against collateral |
| `RepayStableV1` | `0x05` | Repay stablecoin debt |
| `LiquidateV1` | `0x06` | Liquidate undercollateralized position |
| `UpdateConfigV1` | `0x07` | Update configuration parameters |
| `GovernanceReportV1` | `0x08` | Precise collateral/debt ratio — verifies on-chain state, enforces no fractional reserve (BaseDiv, cold) |
| `AccrueInterestV1` | `0x09` | Precise interest accrual (BaseDiv, cold) |
| `RedeemStableV1` | `0x0A` | Redeem stablecoins for underlying collateral via PN::RedeemV1 |
| `SpendHookCallback` | `0x0B` | Process spend_hook callback from PN burn (internal) |

## Promissory Note Lifecycle Integration

The stablecoin is the **sole issuer** in the Promissory Note ecosystem. It creates and destroys tokens through the PN contract, making it the gateway for stablecoin supply management.

### Role in the PN Lifecycle

| Phase | PN Opcode | Stablecoin Operation | Status |
|-------|-----------|---------------------|--------|
| **Mint** | TransferV1 (0x04) | `MintStableV1` — issues stablecoins to borrower | Implemented |
| **Transfer** | TransferV1 (0x04) | All collateral movements, repayments, liquidations | Implemented |
| **Burn** | BurnV1 (0x03) | `PN::BurnV1` — direct burn with spend_hook callback | Implemented |
| **Redeem** | RedeemV1 (0x01) | `RedeemStableV1` — redeem stablecoins for collateral | Implemented |

### Architecture Note: PN Lifecycle Usage

MintStableV1 and RepayStableV1 use TransferV1 (0x04) — stablecoins are pre-minted
to the contract during initialization and transferred out/in. However, the full PN
lifecycle is increasingly used:

- **RedeemV1 (0x01)**: `RedeemStableV1` calls PN::RedeemV1 to release collateral
  proportionally when stablecoins are redeemed. `total_redeemed` is atomically
  incremented.
- **BurnV1 (0x03)**: Direct burns via PN::BurnV1 with `spend_hook = stablecoin_cid`
  trigger the `__spend_hook` callback. The callback records nullifiers and
  increments `total_redeemed` in the same overlay for atomicity.
- **TokenMintV1 (0x00) / MintV1 (0x02)**: Not yet used directly; minting remains
  via TransferV1.

### spend_hook Callback Architecture

The stablecoin uses `define_contract_with_spend_hook!` to export a `__spend_hook`
WASM function alongside the standard 4 exports. When a user burns stablecoins via
`PromissoryNote::BurnV1` with `spend_hook = stablecoin_contract_id`, the PN
contract dispatches a callback to the stablecoin:

```
User calls PN::BurnV1 (spend_hook = stablecoin_cid)
  → PN verifies nullifiers, builds BurnSpendHookPayload
  → PN calls emit_spend_hook(stablecoin_cid, payload)
    → Host writes to Env.spend_hook_request
      → Blockchain pipeline loads stablecoin WASM
        → stablecoin.__spend_hook(payload)
          → process_spend_hook():
              1. Deserializes BurnSpendHookPayload
              2. Verifies caller_contract_id == expected PN contract
              3. Checks nullifiers for replay (DB lookup)
              4. Builds SpendHookCallbackUpdateV1
              5. Returns update via set_return_data
        → stablecoin.apply(update_data)
          → apply_spend_hook_callback():
              1. Records nullifiers in nullifier tree (replay protection)
              2. Increments total_redeemed counter in config DB
              3. Enables computation: Outstanding = Minted - Redeemed
```

**Atomicity**: The callback runs in the same overlay as the parent burn. If
verification fails or the nullifier is a duplicate, the entire overlay is
reverted — the burn does not take effect.

**SpendHookCallback (0x0B)**: An internal opcode that can only be reached via
`__spend_hook`, never via `exec()`. Calling it through `process_instruction`
returns an error. This separation prevents reentrancy: the callback goes through
`__spend_hook`, not the contract's main `__entrypoint`.

**Nullifier tracking**: Processed nullifiers are stored in the callback nullifier
tree, enabling the contract to compute redemption totals and detect replay
attempts.

### Balance Sheet Tracking

The contract tracks:

| Key | Purpose |
|-----|---------|
| `CDP_TOTAL_DEBT` | Total stablecoins minted (outstanding debt) |
| `CDP_TOTAL_COLLATERAL` | Total collateral locked |
| `CDP_ACCUMULATED_FEES` | Interest/fees accrued |
| `CDP_TOTAL_REDEEMED` | Total redeemed via RedeemStableV1 + spend_hook callbacks |
| `SPEND_HOOK_NULLIFIERS` | Processed spend_hook callback nullifiers (replay protection) |
| `GOVERNANCE_REPORTS` | Historical governance reports for public audit |

`CDP_TOTAL_REDEEMED` is incremented by both `RedeemStableV1` (when stablecoins
are redeemed for collateral via PN::RedeemV1) and `apply_spend_hook_callback`
(when stablecoins are burned via PN::BurnV1 with spend_hook). Outstanding
circulation is computed as `Outstanding = CDP_TOTAL_DEBT - CDP_TOTAL_REDEEMED`.

### Governance Report: No Fractional Reserve Proof

`GovernanceReportV1 (0x08)` provides cryptographically-enforced proof of full
collateralization:

1. **On-chain verification**: Reads `total_debt`, `total_collateral`, and
   `total_redeemed` from the config DB. Rejects the report if the reporter's
   params don't match on-chain state.
2. **Outstanding computation**: `outstanding = total_debt - total_redeemed`
3. **No fractional reserve**: Enforces `total_collateral >= outstanding`. Returns
   `InsufficientCollateral` if violated.
4. **Persistence**: The verified report is stored in the `governance_reports`
   tree keyed by `poseidon_hash(token_id, outstanding, total_collateral, ratio)`,
   providing an on-chain audit trail.

The ZK circuit (`governance_report_v1.zk`) computes
`collateral_ratio_bps = base_div(total_collateral, outstanding)` using the
BaseDiv opcode. The entrypoint verifies the circuit's inputs match on-chain
state before accepting the proof.

### Cross-Contract Validation

All child calls to PN use `validate_child_contract_id` to prevent routing attacks
and `validate_child_value_commit` to verify transfer amounts. `RedeemStableV1`
uses `validate_child_redeem_v1` for RedeemV1 child call validation.

## ZK Circuits

All 9 circuits compiled to `.zk.bin`:

| Circuit | Purpose |
|---------|---------|
| `init_v1.zk` | Prove initialization parameters |
| `open_position_v1.zk` | Prove CDP position creation |
| `add_collateral_v1.zk` | Prove collateral addition |
| `remove_collateral_v1.zk` | Prove collateral removal with ratio check |
| `mint_stable_v1.zk` | Prove stablecoin minting within limits |
| `repay_stable_v1.zk` | Prove debt repayment |
| `liquidate_v1.zk` | Prove liquidation conditions met |
| `governance_report_v1.zk` | Prove precise ratio report (BaseDiv) |
| `accrue_interest_v1.zk` | Prove precise interest calculation (BaseDiv) |

## Multi-Collateral Support

Collateral types and risk parameters:

| Asset | Haircut | Liquidation Threshold | Max Debt Share |
|-------|---------|---------------------|----------------|
| ETH | 2% | 125% | 50% |
| XMR | 1% | 130% | 30% |
| DRKW | 0% | 150% | 100% |

**Haircut**: Value discount applied before collateral calculation
**Max debt share**: Maximum % of total debt this collateral can back

## Hot/Cold Circuit Separation

Operations are split by computational cost:

### Hot (Cheap, Frequent)

| Operation | Method | Cost |
|-----------|--------|------|
| Deposit | LTE + cross-mul | ~100 constraints |
| Mint | LTE + cross-mul | ~100 constraints |
| Withdraw | LTE + cross-mul | ~100 constraints |
| Repay | LTE + cross-mul | ~100 constraints |

### Cold (Expensive, Rare)

| Operation | Method | Cost |
|-----------|--------|------|
| `GovernanceReportV1` | BaseDiv | ~500 field muls |
| `AccrueInterestV1` | BaseDiv | ~500 field muls |

Cold operations are for monthly governance reporting and precise interest calculations. Hot operations handle user actions.

## Dead Man Switch

Emergency shutdown mechanism if executive authority becomes unresponsive:

| Setting | Default | Description |
|---------|---------|-------------|
| `enabled` | false | Opt-in safety feature |
| `timeout_blocks` | 43200 | ~30 days at 1 block/min |

**Trigger actions:**
- `LiquidateAll`: Emergency settlement at current prices
- `DisableMinting`: No new debt, positions remain
- `EnableFreeWithdrawals`: Users can exit without ratio checks

## Price Feed

AMM-based TWAP price discovery (P2P Oracle):

```
External Pool → TWAP → PI Controller → Redemption Rate
```

No centralized oracles - the AMM pool itself provides price discovery.

## Opcode Status

| Opcode | Status | Use |
|--------|--------|-----|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Collateralization checks |
| `BaseDiv` (0x58) | ✅ Implemented | Interest/ratio calculations |
| `less_than_strict` | ✅ Sound | Bounded comparisons |

## Relationship to Bridge

The bridge provides multi-collateral support:

| Chain | Token | Integration |
|-------|-------|-------------|
| Ethereum | ETH | Native via bridge |
| Monero | XMR | Privacy-native |
| Zcash | ZEC | Shielded |
| Litecoin | LTC | Trade pair |

## Governance: Compositional Concern

**Governance integration is a compositional concern for deployers**, not the contract itself. The contract provides financial primitives; governance organization is your responsibility.

### Pre-Deployment Checklist

1. **DAO should pre-exist deployment**
   - Create DAO and operational BEFORE stablecoin deployment
   - Define governance token and initial supply
   - Set up voting mechanisms

2. **Deployment wallet = DAO multisig**
   - Deployer wallet should be a DAO multisig, not an individual
   - Dead man switch is backup — primary governance is the DAO
   - All executive actions via DAO voting

3. **Initial parameters via governance**
   - Minimum collateralization ratio
   - Liquidation thresholds
   - PI controller settings
   - Dead man switch configuration

### Staking Integration (External)

Staking tokens to the stablecoin for governance weight is configured at the **DAO level**, not the contract level. The contract provides financial primitives; how staking integrates with your DAO's governance is your design decision.

### DrainProtection (Optional)

- **Dead man switch is the minimum** (already in contract)
- Deployers can add [DrainProtection](./drain_protection.md) as an additional layer
- 8 best practices available but not required
- Your governance structure determines which practices make sense

### Summary: Where Decisions Are Made

| Concern | Where Decided |
|---------|----------------|
| Collateral types | Contract deployment |
| Model selection | Contract deployment |
| Interest rates | DAO governance |
| Emergency shutdown | Dead man switch (contract) + DAO |
| Staking for governance | DAO organization |
| Executive actions | DAO multisig |

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [Stablecoin Contract](../../../src/contract/stablecoin/)
- [Bridge](./bridge.md)
- [Opcodes](../arch/zk/opcodes.md)
- [DrainProtection](./drain_protection.md)
