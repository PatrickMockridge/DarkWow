# Residual risk register

Accepted risks. A residual is **not** a closed defect: it is an unresolved risk with a signed ALARP
record, a compensating control and a review trigger, per
[`risk-management.md`](risk-management.md). The source ID stays in the tracker's Settled-IDs appendix
as a tombstone so citations resolve; it is never green.

The records below are **PROPOSED (2026-10-25)** and take effect when the register-audit batch is
approved; the approval manifest in the approval commit names the `scope_hash`, the ID list and the
five re-derived samples.

| ID | Source | Harm domain | S/L → R | Harm scenario | Cheapest control / why not now | Compensating control | Owner | Reopen on |
|---|---|---|---|---|---|---|---|---|
| R-01 | OBL-C262 | Privacy/metadata | M/P → 6 Tol. | A user deploys a contract with a wallet-side manifest that disagrees with its WASM; scan-time comparison cannot read a manifest from the upstream `DeployV1` payload, so the wallet shows metadata it has not verified | Extend `DeployV1` to carry a manifest — an upstream contract change that would remove the wallet-side design the port adopted | The wallet marks user-deployed contract metadata unverified | project lead | DeployV1 gains a manifest field |
| R-02 | OBL-C74 | Governance/availability | M/D → 10 Tol. | 24 of 32 manifests declare no barbs or note schema, so wallet capability discovery for those contracts is incomplete | Add the fields — every `manifest.toml` is a `SOURCE_MANIFEST` input: 32 artifacts rebuild and the genesis pin re-rolls | The wallet's native-token path and its manifest fallback | project lead | A genuine artifact rebuild batch is scheduled |
| R-03 | OBL-T22 | Privacy/verification | M/P → 6 Tol. | The Lean wallet proof covers decode only; an encode divergence between the model and Rust is unproved | Model the write path in Lean — a proof campaign, not a patch | The Rust encode round-trip tests | project lead | Next Lean campaign |
| R-04 | OBL-T24 | Availability | M/P → 6 Tol. | Mempool admission and expiry rules are modelled nowhere in the Lean wallet model; a divergence is invisible to the proof | Model admission and expiry — a proof campaign | The Rust mempool tests (`test_gas_limit_respected`, expiry, duplicate/nullifier cases) | project lead | Next Lean campaign |
| R-05 | OBL-Z23 | Consensus/verification | M/D → 10 Tol. | A new circuit introduces an undeclared dead value (a witness nothing derives from) | Make the dead-value gate blocking — it would block every circuit change on the known count | The report-only gate names the count on every run; no undeclared dead value is in a genesis circuit | project lead | A circuit change lands |
| R-06 | OBL-C42 | Availability | M/P → 6 Tol. | `take_competing_blocks()` destroys competing blocks before the fallible work in three of four call sites; an uncle candidate can be lost | Reorder the four call sites — a contained fix; scheduled as ordinary work | The competing-block cap and the diagnostic logging | project lead | Next sync-path change |
| R-07 | OBL-C66 | Consensus/finality | H/P → 9 Tol. | Caribina anchors a block to a settled Arweave block; settlement is not checked, and consensus cannot observe another chain, so the finality assumption rests on the anchor proof alone | Check settlement — impossible for consensus to observe by design | Anchor verification; the assumption is recorded in the decision | project lead | Arweave settlement becomes local data |

## Rules

- A residual is permitted only when no reasonably practicable control exists at acceptable cost and
  a compensating control is named. "Not fixing it now" is not a criterion.
- A cheap control (R-05, R-06) belongs in the plan backlog, not in this register indefinitely.
- A review trigger is `locus-changed`, `release`, `severity-changed`, `fix-induced` or `by:<date>`.
  A `reviewed_at` update requires new evidence or a no-change statement naming the locus hash.
- `scripts/check-defect-register.sh` requires every tombstoned `RESIDUAL` to have a record here and
  every record here to name an appendix ID; a record without an ALARP rationale, owner or trigger
  fails.
