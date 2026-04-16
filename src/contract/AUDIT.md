# Contract Token Integration Audit

> **STATUS**: DEPRECATED (2026-04-16)
> **Superseded by**: `bin/darkfid/src/tests/heavyweight_pipeline.rs` and per-contract test harnesses

---

## DEPRECATED

This audit document served its purpose during the MoneyV2 → MoneyV3 migration (completed).

**Current Verification Infrastructure:**
- `bin/darkfid/src/tests/heavyweight_pipeline.rs` - Full ZK proof + endpoint tests for all 22 contracts
- `scripts/validate_zk_bins.sh` - ZK binary validation tool
- `src/contract/*/tests/` - Per-contract test harnesses

**Migration Results (Historical):**
| Contract | Old | New | Status |
|----------|-----|-----|--------|
| dao_escrow | money::TransferV2 | money_v3::transfer_v1 | ✅ Migrated |
| game_room | money::TransferV2 | money_v3::transfer_v1 | ✅ Migrated |
| subscription | money::TransferV2 | money_v3::transfer_v1 | ✅ Migrated |
| dex | money::OtcSwapV2 | money_v3::otc_swap_v1 | ✅ Migrated |
| stablecoin | N/A | money_v3::token_mint_v1 | ✅ Correct |

All 18 remaining contracts audited - no money_v2 usage found.
