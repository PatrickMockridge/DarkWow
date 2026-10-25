# Residual risk register

Accepted risks. A residual is **not** a closed defect: it is an unresolved risk with a signed ALARP
record, a compensating control and a review trigger, per
[`risk-management.md`](risk-management.md). The source ID stays in the tracker's Settled-IDs appendix
as a tombstone so citations resolve; it is never green.

The records below are approved in `reports/register-approval.txt` (2026-10-25); a later change to
this file changes the acceptance-boundary hash and requires a new approval. IDs are permanent, so
the withdrawn records leave gaps rather than being renumbered.

**Withdrawn 2026-10-25 (process ALARP round).** R-05 (OBL-Z23) and R-06 (OBL-C42) are not residuals:
their cheapest controls are reasonably practicable — a blocking dead-value gate frozen against the
adjudicated baseline, and a contained reordering of four call sites. Both are live `OPEN` rows in
the tracker's chain and models cycles. A cheap control belongs in a cycle, not in this register.

| ID | Source | Harm domain | S/L → R | Harm scenario | Cheapest control / why not now | Compensating control | Owner | Reopen on |
|---|---|---|---|---|---|---|---|---|
| R-01 | OBL-C262 | Privacy/metadata | M/P → 6 Tol. | A user deploys a contract with a wallet-side manifest that disagrees with its WASM; scan-time comparison cannot read a manifest from the upstream `DeployV1` payload, so the wallet shows metadata it has not verified | Extend `DeployV1` to carry a manifest — an upstream contract change that would remove the wallet-side design the port adopted | The wallet marks user-deployed contract metadata unverified | project lead | DeployV1 gains a manifest field |
| R-02 | OBL-C74 | Governance/availability | M/D → 10 Tol. | 24 of 32 manifests declare no barbs or note schema, so wallet capability discovery for those contracts is incomplete | Add the fields — every `manifest.toml` is a `SOURCE_MANIFEST` input: 32 artifacts rebuild and the genesis pin re-rolls | The wallet's native-token path and its manifest fallback | project lead | A genuine artifact rebuild batch is scheduled |
| R-03 | OBL-T22 | Privacy/verification | M/P → 6 Tol. | The Lean wallet proof covers decode only; an encode divergence between the model and Rust is unproved | Model the write path in Lean — a proof campaign, not a patch | The Rust encode round-trip tests | project lead | Next Lean campaign |
| R-04 | OBL-T24 | Availability | M/P → 6 Tol. | Mempool admission and expiry rules are modelled nowhere in the Lean wallet model; a divergence is invisible to the proof | Model admission and expiry — a proof campaign | The Rust mempool tests (`test_gas_limit_respected`, expiry, duplicate/nullifier cases) | project lead | Next Lean campaign |
| R-07 | OBL-C66 | Consensus/finality | H/P → 9 Tol. | Caribina anchors a block to a settled Arweave block; settlement is not checked, and consensus cannot observe another chain, so the finality assumption rests on the anchor proof alone | Check settlement — impossible for consensus to observe by design | Anchor verification; the assumption is recorded in the decision | project lead | Arweave settlement becomes local data |
| R-08 | OBL-C260 | Availability | M/D → 10 Tol. | A manifest-driven caller invokes a PromissoryNote multi-proof arm (`redeem`/`transfer`/`otc_swap`); the generic client refuses it by name, so the action is unavailable through manifest-only invocation | A manifest schema for per-circuit witness subsets plus a multi-proof engine — an SDK feature campaign; every `src/sdk` change rebuilds all 32 contract artifacts and re-rolls the genesis pin | The named refusal (`single_proof_circuit`) prevents a silently wrong single-proof call, and the three arms have bespoke clients | project lead | A manifest-only caller needs one of the arms, or a manifest-schema batch |
| R-09 | OBL-C206 | Verification/availability | M/P → 6 Tol. | A PI-7 block-hash mismatch in the heavyweight harness cannot be replayed from the failure report, so one failure cannot distinguish a deterministic pipeline defect from environmental noise and may be dismissed | A seeded, replayable harness: fix the chain-setup timestamp and nonce seeds and capture every RNG input in the report — a harness campaign | The runner executes chains A and B in one process and compares heights and block hashes (PI-7), localising a mismatch to height, transaction and byte and writing a report; a mismatch fails the run | project lead | A PI-7 mismatch is not localised, or the harness changes |
| R-10 | OBL-Z18 | Verification | M/D → 10 Tol. | The 51 adjudications of vacuous `constrain_equal_base` sites live in the checker's own exception list; a wrong adjudication lets a vacuous binding pass | An external adjudication corpus — a second independent parser or per-site proofs — a ZK verification campaign | The pre-commit hook blocks any new vacuous binding; each exception names its mechanism or the register row that schedules its repair; the domain, dead-value and instance gates cover adjacent classes | project lead | A circuit change at a listed site, or a disputed adjudication |

## Rules

- A residual is permitted only when no reasonably practicable control exists at acceptable cost and
  a compensating control is named. "Not fixing it now" is not a criterion.
- A cheap control belongs in a fix cycle, not in this register. R-05 and R-06 were withdrawn on
  2026-10-25 for exactly that reason.
- A review trigger is `locus-changed`, `release`, `severity-changed`, `fix-induced` or `by:<date>`.
  A `reviewed_at` update requires new evidence or a no-change statement naming the locus hash.
- `scripts/check-defect-register.sh` requires every tombstoned `RESIDUAL` to have a record here and
  every record here to name an appendix ID; a record without an ALARP rationale, owner or trigger
  fails.
