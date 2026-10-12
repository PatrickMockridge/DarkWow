# Security Audit

> **USE AT YOUR OWN RISK.** The smart contracts in this repository have undergone internal simulation-based security review but have NOT been audited by an independent third-party firm.

The full security audit is available at [src/contract/AUDIT.md](https://github.com/PatrickMockridge/DarkWow/blob/linear-master/src/contract/AUDIT.md) in the repository.

## Summary

In May 2026, the bridge and relayer_endowment contracts underwent a hardening pass driven by a Python discrete-event simulation. 10 adversarial scenarios were modeled block-by-block, identifying **17 failure modes** (1 CRITICAL, 6 HIGH, 8 MEDIUM, 2 LOW).

### Resolution Status

[`src/contract/AUDIT.md`](../../../src/contract/AUDIT.md) marks **all 17** findings `FIXED`. This
page said "**FIXED** 14 of 17 / **PLANNED** 3 of 17 (pool reputation, fee discovery, health
checks)" until 2026-10-12 — so the two records of the same audit disagreed before either was
checked against the tree, and the three the page called planned include finding #14, which
`AUDIT.md` records as fixed.

Neither count is defensible. The table below is what the artifacts actually carry, row by row:
some fixes exist, one was deliberately removed, and several were never written. A count cannot be
recovered from it without re-adjudicating all seventeen, which is the work that table represents —
so this page no longer gives one.

### Key Fixes Applied

**Restated 2026-10-11 (`F-519`).** This table listed nine protections as applied.
Measured against the tree, two exist, one was deliberately removed, and six were
never written — so it told a reader the bridge carries protections it does not. The
rows below are what the artifacts have, with the removed and the absent named rather
than deleted: a bridge that does not slash is something a reader needs to know, and
an omission would say the same thing less honestly.

| Feature | Contract | State |
|---------|----------|-------|
| Token-aware anti-dust floor | Bridge | **Not as this table said.** `token_minimum` was a *circuit public input*, and it was **removed 2026-09-24**: it was a witness the prover chose, so it bounded nothing (`proof/withdraw.zk` records the derivation). The floor is host-checked against the contract's own value. |
| Force settlement | Relayer Endowment | `ForceSettleV1` (0x06) — present. It could not pay until 2026-10-11, when the log it distributes acquired a writer (`RecordCollectedFeesV1`, `F-513`); before that its pro-rata share was always zero. |
| Fee logging | Relayer Endowment | `last_settlement_height` and `total_collected_fees_log` — present, with the same caveat: the log had no writer until `F-513`. |
| HTLC state machine atomicity | Bridge | **Absent.** `AUDIT.md` records "Added `claimed_at: Option<u64>` and `refunded_at: Option<u64>` to `HtlcSwapInfo`" — there is no `HtlcSwapInfo` in the bridge, and no `claimed_at`/`refunded_at` field in it. (`lottery` has an unrelated `claimed_at` on its `Claim` record.) |
| Circuit breaker | Bridge | **Absent.** No `GUARANTEED_PENDING` counter and no `MAX_GUARANTEED_TOTAL`. |
| Withdrawal reassignment | Bridge | **Absent.** There is no `ReassignWithdrawalV1`. |
| Proportional slashing | Bridge | **Absent.** No `MIN_SLASH` and no `SLASH_BP`. |
| Fee caps | Bridge | **Absent.** No `MAX_FEE_BP`, and no per-user `max_fee_bp`. |
| Merkle proof enforcement | Bridge ZK | **Absent.** No `SparseMerklePath` type and no `sparse_merkle_root` constraint. |

The six absent protections are tracked by no register row. The bridge is deferred;
what this section owes a reader is the absence itself, stated where the claim was
made. Recorded as `OBL-C228`.

### Residual Risks

The following risks remain:
- ZK circuit soundness is unproven (no formal verification)
- No external block header verification (Merkle proofs not anchored to real chain state)
- No deposit finality guarantee (chain reorgs could revert deposits)
- Pool reputation tracking (Phase 2d) not yet deployed
- No third-party audit has been performed

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis


- [Full Audit Report](https://github.com/PatrickMockridge/DarkWow/blob/linear-master/src/contract/AUDIT.md)
- [Bridge Contract](bridge.md)
- [Relayer Endowment Contract](relayer_endowment.md)
- [Simulation Report](https://github.com/PatrickMockridge/DarkWow/blob/linear-master/sim/report.md)
- [Relayer Economics](../relayer/relayer_economics.md)
