# Security Audit

> **USE AT YOUR OWN RISK.** The smart contracts in this repository have undergone internal simulation-based security review but have NOT been audited by an independent third-party firm.

The full security audit is available at [src/contract/AUDIT.md](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/AUDIT.md) in the repository.

## Summary

In May 2026, the bridge and relayer_endowment contracts underwent a hardening pass driven by a Python discrete-event simulation. 10 adversarial scenarios were modeled block-by-block, identifying **17 failure modes** (1 CRITICAL, 6 HIGH, 8 MEDIUM, 2 LOW).

### Resolution Status

| Status | Count |
|--------|-------|
| **FIXED** | 14 of 17 |
| **PLANNED** | 3 of 17 (pool reputation, fee discovery, health checks) |

### Key Fixes Applied

| Feature | Contract | Description |
|---------|----------|-------------|
| HTLC state machine atomicity | Bridge | `claimed_at`/`refunded_at` timestamps with mutual exclusion |
| Circuit breaker | Bridge | `GUARANTEED_PENDING` counter capped at `MAX_GUARANTEED_TOTAL` |
| Withdrawal reassignment | Bridge | `ReassignWithdrawalV1` (0x09) — any relayer can claim stuck withdrawals |
| Proportional slashing | Bridge | `max(MIN_SLASH, amount * SLASH_BP / BP_PRECISION)` — 10% of amount |
| Fee caps | Bridge | `MAX_FEE_BP = 1000` (10%) + per-user `max_fee_bp` option |
| Token-aware dust minimum | Bridge ZK | `token_minimum` public input instead of hardcoded threshold |
| Merkle proof enforcement | Bridge ZK | `sparse_merkle_root` constraint with `SparseMerklePath` type |
| Force settlement | Relayer Endowment | `ForceSettleV1` (0x06) — backer-initiated pro-rata fee distribution |
| Fee logging | Relayer Endowment | `last_settlement_height` + `total_collected_fees_log` for auditability |

### Residual Risks

The following risks remain:
- ZK circuit soundness is unproven (no formal verification)
- No external block header verification (Merkle proofs not anchored to real chain state)
- No deposit finality guarantee (chain reorgs could revert deposits)
- Pool reputation tracking (Phase 2d) not yet deployed
- No third-party audit has been performed

## See Also

- [Full Audit Report](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/AUDIT.md)
- [Bridge Contract](bridge.md)
- [Relayer Endowment Contract](relayer_endowment.md)
- [Simulation Report](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/sim/report.md)
- [Relayer Economics](../relayer/relayer_economics.md)
