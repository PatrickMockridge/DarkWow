# Process ALARP round — 2026-10-25

Subject: the assurance process around `doc/src/arch/verification-hazop.md` at `b5de5acc60`, its
gate, the umbrella and the Docker pipeline. This report is the evidence record for the normative
process rules in [`risk-management.md`](../doc/src/arch/risk-management.md) §10 and
[`hazop-rules.md`](../doc/src/dev/hazop-rules.md) "The binary finding lifecycle".

**What the round is for.** The project lead caught the process in the state it was meant to prevent:
a Docker run was started while 25 findings were `OPEN`, five of them Intolerable, and a green run was
being read as a verdict. The demand was not another row edit; it was certainty that the process
cannot produce that state again. The round therefore analyses the **process**, not the findings.

## 1. Method

Three independent auditors covered (a) production baseline and control portfolio, (b) the
verification gates and their reachability, and (c) the register, dispositions and approval. One
adversarial verifier then tried to falsify every load-bearing claim by reading the process files
(R6). The verifier's corrections are part of the record; a claim that did not survive is not used to
justify a change.

Guide words applied to seven parameters: `NO/NOT`, `AS WELL AS`, `PART OF`, `REVERSE`, `OTHER THAN`,
`MORE/LESS`. The filled table is in `risk-management.md` §10.2; each control's failure mode, cost and
ALARP decision is in §10.3.

## 2. Verifier verdicts

| # | Claim | Verdict |
|---|---|---|
| 1 | The documented `--tier 2` cannot reach the Docker block | **CONFIRMED** — `TIER="${1:-}${2:-}"` yields `--tier2`; the guard compared `"--tier 2"` |
| 2 | The pipeline never reads the tracker; wallet-only exits before every remaining gate | **CONFIRMED** |
| 3 | A green verdict coexists with 25 OPEN rows | **CONFIRMED** — `report()` reads only `FAIL`; the tracker gate is in the umbrella and passes on open rows |
| 4 | An unknown status is silently dropped | **CONFIRMED** — a six-cell `OPENISH` row matched no branch |
| 5 | `plan:` is accepted as a closure | **PARTLY TRUE** — 13 rows use it, all OPEN; a `CLOSED` row with `plan:` is explicitly refused |
| 6 | `negative:` prose waives the self-test | **CONFIRMED** — exactly C264/C268/C270 rely on it |
| 7 | The approval hash covers the live tracker | **CONFIRMED** — and `HEAD` mode read the working tree, a second defect |
| 8 | The approval batch lists 60 IDs with one missing | **FALSE** — 61 IDs, every one resolves |
| 9 | 233 `settled` rows are non-binary exits | **PARTLY TRUE** — 233 is exact; 7 appendix residuals are proper ALARP records; 19 `decision` and 2 `merged` are legitimate tombstones |
| 10 | Tier 1 is the control harness; a Tier-1 sweep is a release verdict | **PARTLY TRUE** — Tier 1 is a fast/hermetic gate set; the conclusion (a sweep is a verdict) is adopted, the label was wrong |
| 11 | `--mode wallet` exits before any verdict gate | **CONFIRMED** |
| 12 | Live counts | **CONFIRMED at the time** — 25 OPEN / 8 CLOSED / 7 RESIDUAL |

The verifier also found that `test_pipeline.sh` passed the control-wiring check only through the
dead Tier-2 block, and that the approval manifest is not itself hashed or signed.

## 3. What was unsafe, and why the safeguard did not fire

- **The gate was inside the thing it judged.** The tracker gate lived only in the umbrella; the
  pipeline read nothing. A control placed on one path does not cover the out-of-band path.
- **The Tier-2 route was dead code.** The documented command could never reach the gate that would
  have run the pipeline, so the one place a tracker check *was* wired could not be reached.
- **The approval conflated two decisions.** A closure is earned by evidence; an acceptance is signed
  by a lead. Hashing the live tracker made every legitimate closure invalidate the approval, so
  re-approval became the rational response — and then the approval stopped meaning anything.
- **Prose stood in for a control.** The `negative:` escape let three rows claim an observed failure
  without a control that can fail; the pipeline was never made self-testable.
- **The parser dropped what it did not understand.** An unknown status matched no branch and was
  silently ignored; a parser with a silent default cannot report drift.
- **Disposition had a third exit.** `OPEN` plus a green run was accepted in practice. There is no
  third exit now: the undecided statuses block every verification surface.

## 4. Guide-word findings (summary)

The full table is `risk-management.md` §10.2. The load-bearing cells:

- **Finding intake, `NO/NOT`:** a finding could carry no recognized status and vanish. Fixed.
- **Finding intake, `AS WELL AS`:** an OPEN row and a green verdict coexisted. Fixed by the binary
  rule.
- **Disposition, `REVERSE`:** R-05 and R-06 were residuals hiding contained fixes. Withdrawn.
- **Evidence, `OTHER THAN`:** a control for another property (`negative:` prose) waived the control
  for this one. Removed.
- **Approval, `AS WELL AS`:** approval plus later edits was a treadmill. Moved to the acceptance
  boundary.
- **Verification, `PART OF`:** a Tier-1 sweep was being read as the verdict. The umbrella refuses in
  both tiers; a targeted control is not a verdict.
- **Release, `MORE/LESS`:** a lapsed cycle bound now fails the gate, so the register cannot park
  rows indefinitely.

## 5. Controls and ALARP decisions

| Control | Decision |
|---|---|
| Schema and status vocabulary | **Reduce** — unknown status is an error; one shared parser. |
| Risk bands and C/H override | **Keep** — fixed matrix; `basis:` on a decrease. |
| Residual record | **Keep and enforce** — a cheap control is an OPEN cycle; R-05/R-06 withdrawn. |
| Approval hash | **Replace** — over the acceptance boundary, mode-aware; freeze the appendix. |
| Verification preflight | **Add** — no override. |
| Umbrella refusal | **Add** — both tiers. |
| Tier dispatch | **Repair** — and self-tested. |
| Fix-cycle bound | **Add** — coverage and lapse checks. |
| Shared parser | **Add** — gate, report and preflight. |
| Appendix freeze | **Add** — baseline hash. |
| Pipeline self-tests | **Add** — C264/C268/C270 on fixtures; `negative:` removed. |
| Performance exchange ledger | **Delete** — removed no risk; `P` is a diagnostic. |
| Standing self-test gates | **Measure first** — `reports/gate-times.tsv`; retire only with a tombstone, a replacement and a planted defect. |
| Expected-red anti-pattern scanner | **Schedule** — make the expected red explicit with a baseline. |
| Overlapping doc/census gates | **Measure, then a merge candidate** — no retirement without replacement. |

## 6. What changed

- `scripts/check-tracker-settled.sh`: the preflight, with `--self-test` and fixtures for OPEN,
  BLOCKED, DEFERRED, CLOSED, residual-only and an unknown status.
- `test_pipeline.sh`: the preflight before any phase (wallet-only included), reading
  `origin/linear-master` and requiring `HEAD` equality and a clean tracker; a fixture-based
  `--self-test` for the C264/C268/C270 mechanisms.
- `entrypoint.sh`: the duplicate-listener self-test (C268).
- `scripts/run-all-tests.sh`: correct `--tier` parsing with a self-test; refusal in both tiers;
  the Docker guard now compares `"2"`.
- `scripts/register_parse.py`: one parser for the gate, the report and the preflight.
- `scripts/check-defect-register.sh`: unknown status, fix-cycle coverage, appendix vocabulary and
  freeze, acceptance-boundary approval, `HEAD`/index hashing; the `negative:` escape is removed.
- Tracker: a dated `## Fix cycles` table covering every OPEN row; the appendix declared frozen
  history (257 rows); C42 and Z23 reopened; 44 decisions.
- `residual-risk.md`: R-05 and R-06 withdrawn with the reason; 5 residuals remain.
- `risk-management.md`: §5 binary lifecycle, §7 acceptance-boundary approval, §8.3 no exchange,
  §10 this round's normative record.
- `hazop-rules.md`: the binary lifecycle and the corrected condition 9.

## 7. Verification of the controls (no Docker, no cargo)

- `check-defect-register.sh --self-test`: PASS, including the new cycle, status, history,
  treadmill and `negative:` cases.
- `check-tracker-settled.sh --self-test`: PASS; the live tracker is refused with all 27 undecided
  rows named.
- `test_pipeline.sh --self-test`: PASS; each fixture exercises the pre-fix parse and the current
  parse.
- `entrypoint.sh --self-test`: PASS.
- `run-all-tests.sh --self-test`: PASS; `--tier 2` now parses to Tier 2.
- `run-all-tests.sh --tier 2` on the live tree: refused at the preflight (uncommitted tracker
  edits); after commit it refuses on the 27 undecided rows. It does not reach Docker.

## 8. What this round did not establish

- Per-commit attribution for the 233 pre-migration `settled` rows.
- Whether any pre-migration row reopened other than C240.
- Gate wall times; the portfolio conclusions are provisional until `gate-times.tsv` has data.
- Whether C266 is a documentation defect or an intended cache-mode design.
- That each standing self-test gate removes risk; only its cost is being measured.
- The approval manifest's cryptographic provenance; it is a tracked file, not a signed artefact.
- A disclosure policy, incident runbook and compatibility policy; these are the next bounded
  workstream.
- Whether the strict binary rule survives a large register. If it does not, the answer is fix
  throughput and cycle honesty, not a lower threshold for a verdict.

## 9. Batch record

The acceptance boundary is `residual-risk.md` + `risk-management.md`;
`reports/register-approval.txt` records its `scope_sha256`, the frozen appendix
`appendix_baseline_sha256`, the explicit 61-ID batch and the approval. A tracker edit no longer
invalidates that approval; an edit to the boundary does.
