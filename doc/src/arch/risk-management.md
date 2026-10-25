# Risk management: ALARP, dispositions, and the performance/risk trade-off

This document is normative for how this repository decides what is a defect, what is an accepted
residual, and when optimisation work may proceed. It replaces the informal practice in which the
defect tracker accumulated rows faster than they closed, with no stated point at which a residual
risk was accepted.

It borrows the structure of established practice, and deliberately not the parts that do not fit a
project of this size:

- **Impact vocabulary**: the four-class severity scheme used by production blockchain projects
  (Critical/High/Medium/Low, defined by network integrity, stated properties, degradation and
  local impact) is the reference for the C/H/M/L anchors below.
- **Tolerability**: HSE's ALARP structure — an intolerable region, a tolerable region where risk
  must be reduced as low as reasonably practicable, and a broadly acceptable region — is the
  reference for the bands below.
- **Not adopted**: the numeric gross-disproportion factor is a legal cost-benefit test that requires
  monetised risk and a named duty holder. This project records the ALARP reasoning (cheapest
  control, why not now, compensating control, reopening event) but does not pretend to a legal
  standard it cannot satisfy.

## 1. Harm domains

1. Loss, forgery or theft of funds or keys.
2. Hidden supply inflation or invalid value creation.
3. Consensus divergence: invalid blocks accepted, or a chain split.
4. Privacy loss or deanonymisation.
5. Liveness and availability (including denial of service and mining starvation).
6. Governance capture or unauthorised authority.
7. Upgrade and data loss.

A row or residual SHALL name the harm domain it belongs to; a finding that fits none is not a risk
record (it is a documentation correction or a decision).

## 2. Severity

The anchors are the ones this tree already used before the tracker migration, restored here as the
single vocabulary. `security-analysis.md`'s Critical/High/Medium map onto C/H/M.

| Code | Weight | Anchor |
|---|---|---|
| **C** | 5 | A violation mints, destroys or steals consensus value, or breaks network integrity or privacy |
| **H** | 3 | It breaks a stated safety or security property, or causes widespread disruption or local fund loss |
| **M** | 2 | Defence-in-depth, liveness, degraded function, or a limited-scope verification gap |
| **L** | 1 | Hygiene: a statement that is wrong without being reachable, or local/non-default impact |

## 3. Likelihood

Likelihood is the likelihood of the **harm scenario**, not the existence of the code path.

| Code | Weight | Evidence rule |
|---|---|---|
| **D** (Demonstrated) | 5 | Observed in a run or test on the current tree; a failing-before control or reproduction exists |
| **P** (Plausible) | 3 | A reachable path with a named actor and trigger, feasible with effort; no run yet |
| **T** (Theoretical) | 1 | No known trigger; requires unstated preconditions or an external compromise |

A row whose likelihood is `D` SHOULD carry the run line that demonstrates it. A row whose likelihood
is `P` SHALL name the actor, the trigger and the consequence.

## 4. Tolerability

`R = severity weight × likelihood weight`. The band is fixed by the matrix; no free-text judgement
can move a row between bands.

| R | Band | Disposition |
|---|---|---|
| ≥ 15 | **Intolerable** | `BLOCK-RELEASE`: fix, or disable the path, before release, regardless of cost |
| 6–14 | **Tolerable** | `ACCEPT-WITH-RECORD`: the ALARP record below is required |
| ≤ 5 | **Broadly acceptable** | `MONITOR`: tracked, reviewed on the triggers below |

Hard overrides: **C and H are never Broadly acceptable**, whatever the likelihood. A C with `D` or
`P` is Intolerable. `L` severity may be Broadly acceptable at any likelihood.

The band is a property of the risk, not of the effort to fix it. A cheap Intolerable row is still
Intolerable; a Broadly acceptable row does not become an obligation because it is easy.

## 5. Dispositions

The tracker's status vocabulary (see `hazop-rules.md`):

- `OPEN` — a falsifiable defect with a resolving control or a control plan. `—` is forbidden.
- `BLOCKED` — a real defect whose control cannot be written yet: named blocker, owner, review
  trigger, plan-backlog ID. Counts as an open defect.
- `DEFERRED` — a fix deferred by design, with `deferred:<release/commit>` or `deferred_by:<ID>`.
  The gate fails if the named tag has landed or the blocking row is CLOSED.
- `RESIDUAL` — an accepted risk. The ID is tombstoned in the tracker's Settled-IDs appendix and
  carries the ALARP record in [`residual-risk.md`](residual-risk.md). A residual is **not** green;
  it is an unresolved risk with a signed record.
- `CLOSED` — impossible in current code, with a failing-before control, quoted evidence and a code
  change (the nine-condition rule in `hazop-rules.md`).
- `NON-DEFECT` / `DECISION` — lives only in [`decisions.md`](decisions.md).

A residual is permitted **only** when no reasonably practicable control exists at acceptable cost
and a compensating control is named. "Not fixing it now" is not a residual criterion. An ID cannot
be live in the tracker and in `decisions.md` or `residual-risk.md` at the same time.

**The lifecycle is binary.** `OPEN`, `BLOCKED` and `DEFERRED` are **undecided**; `CLOSED` and
`RESIDUAL` are **decided**. While any undecided row exists, no verification surface — a Docker
run, a pipeline pass, an umbrella sweep, a release verdict — may run (§10.6). A targeted control run
directly by the fix in progress is not a verdict. There is no override. The 2026-10-25 process
round found that every earlier control permitted "open but verified"; that state is now refused by
`scripts/check-tracker-settled.sh` at the pipeline and the umbrella.

## 6. The ALARP record

Every `ACCEPT-WITH-RECORD` residual carries:

1. **Harm scenario**: actor, trigger, consequence, harm domain.
2. **Cheapest control found**: what it is, what it costs in engineer-days (measured or bounded), and
   the risk it removes.
3. **Why it is not being done now**: cost, the new failure mode the control could introduce (R2),
   or a missing prerequisite. "Not worth it" is not a reason.
4. **Compensating control in place**: the gate, test, operational limit or monitoring that bounds
   the risk today.
5. **Reopening trigger**: `locus-changed`, `release`, `severity-changed`, `fix-induced`, or
   `by:<date>`.

The record is reviewed when its trigger fires; a `reviewed_at` update requires new evidence (a run
line or a commit) or a no-change statement naming the current locus hash. A bare date bump fails.

## 7. Approval

Residual risks and the policy that governs them are approved as one batch (a decision of the project
lead). `reports/register-approval.txt` contains:

- `scope_sha256`: SHA-256 over the **acceptance boundary** — `residual-risk.md` and this document;
- `appendix_baseline_sha256`: SHA-256 over the frozen pre-migration tombstones (the historical
  `settled`/`restated`/`accepted`/`dissolved` rows);
- `batch=`: the explicit list of every ID in the batch;
- `approved=`: the date and the holder.

The tracker is deliberately **not** hashed. A tracker edit is a disposition earned by evidence and
checked by `check-defect-register.sh`; hashing the live tracker made every legitimate closure
invalidate the approval and forced a re-approval treadmill — the adversarial verifier confirmed a
single closure edit broke the hash. What a lead signs is the acceptance of risk (the residual
records) and the policy that governs it. Any edit to those two files changes `scope_sha256` and
returns the batch to approval. The hash is computed from the mode's own view — HEAD's blobs in HEAD
mode, the index in `--staged`, the working tree otherwise — so an uncommitted edit cannot invalidate
a check of the committed register.

No wildcard and no "all rows". A row whose evidence is `UNVERIFIED` cannot be in the approved set.
The approval manifest is itself an ordinary tracked file; its integrity rests on the commit history,
not on a signature. That limitation is recorded in §10.9.

## 8. Weights: performance against risk

### 8.1 Risk index

`R` as above, recorded on every row and residual. The report publishes the distribution, the median
age by severity, the disposition mix, the reopen rate, and fix-induced defects per released change.

### 8.2 Performance index

`P = 40%·Δruntime + 40%·Δprocess + 20%·Δmaintainability + 0%·Δdeveloper-experience`.

Each `Δ` is a percentage against a committed baseline file that names the commit, command, hardware,
sample size and date; each is clipped to ±25%. An unmeasured term makes `P` **INCOMPLETE**, and an
incomplete `P` cannot justify a risk trade.

- **Runtime**: the pipeline's cgroup `memory.peak` per role is the only recorded runtime measurement.
  Latency and throughput are `UNMEASURED` until a harness exists; they are not counted.
- **Process**: fresh/cached build time, the recorded heavyweight durations, the Lean suite, the
  pipeline run. Gate-suite wall time is measured by `run_gate` (`reports/gate-times.tsv`); retiring
  a gate is a risk decision, never a `P` gain.
- **Maintainability**, from git: median changed lines/files per closure, reopen rate, fix-induced
  defects per 100 closures.
- **Developer experience** is 0 until a clean-checkout-to-green timer exists; it is reported as a
  diagnostic only.

### 8.3 Decision rules

1. **Risk-neutral by default.** Optimisation work may not increase any `R`. A change that adds risk
   is not an optimisation.
2. **Risk first.** While any Intolerable row is open, risk work preempts optimisation. While
   Tolerable rows lack a signed ALARP record, optimisation is capped and must be risk-neutral.
3. **No exchange.** `P` is a diagnostic of process cost, not a currency for risk. A measured
   performance gain never justifies accepting a new risk. The earlier exchange clause (a ledger with
   `ΔR_allowed = min(2, P/10)`) is retired: it removed no risk, and a solo project cannot monetise
   the terms a gross-disproportion test would need. Risk-reducing work may proceed when it is also
   performance-positive; performance alone is not a reason to take risk.
4. **Ordering.** Risk-reduction work is ordered by `R × exposure ÷ cost`: a cheap control on a
   common failure outranks an expensive control on a rare one.

### 8.4 Worked example

The RAM campaign (`fa76d8c2ff`, `ae1a8e4e5e`) reduced node1's peak from the 4.18 GiB OOM evidence to
2729 MiB steady state — a 36% runtime improvement, `40% × 36% = 14.5 P`. At 10 `P` per `R` and a 2
`R` cap it would buy at most 1.45 `R`; the change was risk-reducing, so no trade was used.
`OBL-C267` at `C5/D5 = 25` would need 250 `P`, above the 100-point maximum: the risk-first rule is
absolute. The audit report reproduces this arithmetic and the calibration.

## 9. Register health

Published with every status report: the share of rows with a falsifiable failure mode and a control;
median age by severity; disposition mix; reopen rate; fix-induced defects per release; gate cost per
verdict; ALARP reviews on time. These are measures, not gates — a cap that rewards re-dating is worse
than no cap.

## 10. The assurance process: a HAZOP and ALARP round

The register grew a policy, bands and a gate, and still permitted the state the project lead caught:
a finding stayed `OPEN` while a Docker run proceeded, and the run's green was read as a verdict.
This round analyses **the process**, not the findings. It was required to be adversarial: three
independent auditors, one verifier whose job was to falsify their load-bearing claims against the
process files (R6).

### 10.1 Method and corrections

Each process parameter was walked with the guide words **NO/NOT, AS WELL AS, PART OF, REVERSE,
OTHER THAN, MORE/LESS**. A cell is filled or marked N/A with a reason; a missing cell is itself a
finding. Each control was assessed for its failure mode, whether it can fail, its cost, and an ALARP
decision: eliminate, reduce, or accept with a compensating control. A control that removes no risk is
deleted, not kept for completeness.

The verifier **confirmed** six load-bearing findings: the Tier-2 dispatch bug
(`run-all-tests.sh:71` concatenates `--tier` and `2` to `--tier2`, so the documented command never
reaches `:869`); the pipeline has no tracker read at all and the wallet-only exit at `:164-168`
precedes every remaining gate; `report()` consults only the pipeline's `FAIL` counter, so a green
run coexists with 25 open rows; an unknown status is silently dropped by the parser; `negative:`
prose waived the self-test for exactly C264/C268/C270; and the approval hash covers the live tracker,
so a legitimate closure invalidates it.

The verifier **falsified** two claims, which are recorded here rather than dropped:

- The approval batch is **complete** (61 IDs, every one resolving); there is no missing ID, and the
  doctrine already forbids an "all rows" approval. The treadmill finding stands on the hash
  boundary, not on an incomplete list.
- `plan:` is **not** accepted as a closure. The gate refuses a `CLOSED` row whose control starts
  with `plan:` (`check-defect-register.sh:221-224`); 13 OPEN rows use it as an annotation, which the
  status rule permits. The defect was not that `plan:` closed a row but that an OPEN row could be
  verified at all.

The verifier also found two defects the auditors missed: `scope_hash()` read the working tree even
in `HEAD` mode, so an uncommitted edit invalidated a check of the committed register; and
`test_pipeline.sh` passed the control-wiring check only through the dead Tier-2 block.

### 10.2 Guide-word table

| Parameter | NO/NOT | AS WELL AS | PART OF | REVERSE | OTHER THAN | MORE/LESS |
|---|---|---|---|---|---|---|
| Finding intake → status | A finding with no recognized status was silently dropped (`OPENISH`); fixed — unknown status is a schema error. | An OPEN row **and** a green verdict coexisted; fixed — the three undecided statuses block verification. | `plan:` was read as a disposition; it is an annotation on an undecided row, never an exit. | The verification gate was optional (dead Tier-2 dispatch); fixed — preflight and umbrella refusal, no override. | A residual was used as a green in prose; the gate now requires the ALARP record, the tombstone and the boundary approval. | Undecided rows had no owner or bound; every OPEN row is now in a dated cycle. |
| Risk scoring | A row with no risk failed schema; correct. | Bands plus the C/H override; correct. | Scoring by fix cost is forbidden: the band is a property of the risk (§4). | Re-scoring into acceptability is guarded by `basis:` on any decrease; the live register has none. | A score for a different harm: the harm domain is named on residuals, not on every row; gap recorded. | Bands that never bind: Intolerable rows blocked release only by policy; the preflight now refuses the verification surface itself. |
| Disposition | No owner/cycle: fixed. | OPEN plus verification: fixed. | Guard beside root cause: the C269/C259 pair is the recorded example; a guard must name its own new failure mode (R2/R4). | RESIDUAL hiding an unfixed defect: R-05/R-06 were exactly that; withdrawn 2026-10-25. | A control for another property: `negative:` prose stood in for a self-test; removed, and the three rows now carry real pipeline self-tests. | Ceremony without risk removed: the approval hash over the live tracker was one; retired. |
| Evidence/control | A control that cannot fail: the standing self-test gates are being measured before any retirement. | A control plus a second ledger: the retired launch register is the precedent; there is no second ledger. | A test shaped by the defect: `negative:` prose; removed. | A checker controlling itself (Z18/Z3): now OPEN with external-corpus plans, not residuals. | Checker-as-control: same, and no closure by extending an exception list. | Gate portfolio vs risk removed: cost is recorded per gate; no retirement without a tombstone, a replacement and a planted-defect run; a retirement is never a `P` gain. |
| Approval | No sign-off: every residual needs the manifest. | Approval plus later edits: the live-tracker hash made every closure a re-approval; moved to the acceptance boundary. | Batch hash over stale files: fixed — HEAD mode hashes HEAD's blobs, `--staged` hashes the index. | Blanket approval: forbidden by the explicit `batch=` list. | Approval of unverified rows: `UNVERIFIED` evidence cannot be in the set. | Approval effort vs risk: the batch is 61 IDs and complete; effort now binds only acceptances. |
| Verification gate | No gate: fixed — pipeline and umbrella preflight. | Run while OPEN: refused. | Tier-1 sweep as verdict: the umbrella is refused; targeted controls run directly. | Gate skipped silently: a refused Tier 2 is a red umbrella with a named line, never a skip. | Gate proving the wrong invariant: the tracker is read at the pipeline's own ref with HEAD equality and a clean tree. | Gate cost vs risk: timing is recorded; portfolio decisions await it. |
| Release | No criterion: the preflight is the release precondition. | Release with Intolerable open: impossible while undecided; an Intolerable row cannot become a residual without an ALARP record. | Checklist as opinion: no release checklist survives outside this policy. | Intolerable accepted as exception: no exception path exists. | Criterion not tied to the tracker: the preflight reads the tracker directly. | Freezes that block forever: a lapsed cycle bound fails the gate, so the register cannot park rows indefinitely. |

**Why the safeguard did not fire.** A gate placed inside the umbrella but absent from the pipeline
left the out-of-band path open. The approval hash conflated two different decisions — a closure
earned by evidence and an acceptance signed by a lead — so the only way to keep it valid was to
re-approve constantly, and the natural response was to stop using it. The `negative:` escape was a
convenience for three rows whose control is a whole pipeline; instead of making the pipeline
self-testable, the gate accepted prose. The status parser's membership test silently ignored
everything it did not recognize; a parser that drops what it does not understand cannot report drift.

### 10.3 Controls and their ALARP decisions

| Control | Failure mode | Can it fail? | Cost | ALARP decision |
|---|---|---|---|---|
| Schema + status vocabulary | unknown status dropped | yes (self-test) | seconds | **Reduce**: unknown status is an error; one shared parser. |
| Risk bands + C/H override | score moved by hand | yes (gate) | seconds | **Keep**: fixed matrix; `basis:` on decrease. |
| Residual record | convenience acceptance | yes (withdrawal rule, gate) | minutes per record | **Keep and enforce**: a cheap control is an OPEN cycle; R-05/R-06 withdrawn. |
| Approval over acceptance boundary | treadmill; stale hash in HEAD mode | yes (self-test) | seconds | **Replace**: residual + policy, mode-aware; freeze the historical appendix. |
| Verification preflight | run while undecided | yes (self-test and live refusal) | under a second | **Add**; no override. |
| Umbrella refusal | a sweep read as a verdict | yes (live refusal) | under a second | **Add**. |
| Tier split and dispatch | documented Tier 2 unreachable | yes (the command now reaches the gate) | none | **Repair** (`TIER` concatenation), not accept. |
| Fix-cycle bound | undecided rows in limbo | yes (coverage and lapse checks) | minutes per cycle | **Add**. |
| Shared parser | gate/report/preflight drift | yes (one module, self-tests) | one-time | **Add**. |
| Appendix freeze | `settled` as a new exit | yes (baseline hash) | seconds | **Add**. |
| Pipeline self-tests (C264/C268/C270) | prose as control | yes (fixtures plus planted pre-fix parse) | seconds | **Add**; remove the `negative:` escape. |
| Performance exchange ledger | risk traded for speed | n/a | ledger maintenance | **Delete**: it removed no risk; `P` is a diagnostic, not a currency. |
| Standing self-test gates | ceremony | measured per gate | gate time | **Measure first** (`reports/gate-times.tsv`); retire only with a tombstone, a replacement and a planted-defect run. |
| Expected-red anti-pattern scanner | a permanent red masks a new failure | yes, once baselined | none | **Schedule**: make the expected red explicit with a baseline; until then it is not a pass/fail verdict. |
| Overlapping doc-table/census gates | duplicated cost | yes | gate time | **Measure, then merge candidate**; no retirement without replacement. |

The production baselines are used as references, not as models to copy. Bitcoin Core's four severity
classes and coordinated disclosure/release handling are the reference for the C/H/M/L anchors and
the release discipline; HSE's three ALARP regions are the reference for the bands. Where the
baseline does not fit: a solo project has no duty holder and cannot monetise risk, so the residual
record substitutes a named owner, the cheapest control, a compensating control and a reopening
trigger. That is the maximum honest claim, and §7's approval is the instrument that makes it.

### 10.4 The verification rule

`scripts/check-tracker-settled.sh` runs before any phase in `test_pipeline.sh` — every mode,
wallet-only included — and at the top of `scripts/run-all-tests.sh`. It reads the tracker at the
pipeline's own ref (`origin/linear-master`, the ref the Dockerfile clones), requires `HEAD` to equal
that ref and the tracker to be clean, and refuses on any `OPEN`, `BLOCKED` or `DEFERRED` row or any
schema error. It prints every blocking row and the two legal exits. There is no override. A targeted
control run directly by the fix in progress is not a verdict. A refused Tier 2 is a red umbrella
with a named line, never a silent skip.

### 10.5 The disposition rule

`CLOSED` = code change + failing-before control + quoted run + wired + no trade (the nine conditions
in `hazop-rules.md`). `RESIDUAL` = ALARP record + acceptance-boundary approval + tombstone.
Everything else is `OPEN`, in a dated cycle, and blocks verification. A cycle bound that lapses fails
the gate; a lapsed bound is re-dispositioned, never re-dated silently.

### 10.6 What the round changed

- The pipeline preflight and the umbrella refusal; the Tier-2 dispatch repair; one shared tracker
  parser for the gate, the report and the preflight.
- Unknown status is a schema error; every OPEN row is in a dated cycle; the historical appendix is
  frozen by hash; the approval boundary is residual + policy.
- The `negative:` escape is removed; `test_pipeline.sh` gained a fixture-based `--self-test` covering
  the C264/C268/C270 mechanisms, and `entrypoint.sh` gained the C268 port self-test.
- R-05 and R-06 were withdrawn because their cheapest controls are reasonably practicable; C42 and
  Z23 are OPEN rows in the chain and models cycles.
- The performance exchange ledger is retired; `P` remains a diagnostic.

### 10.7 What this round did not establish

- Per-commit attribution for the 233 pre-migration `settled` rows; whether any pre-migration row
  reopened other than the documented C240 case.
- Gate wall times. The instrumentation (`reports/gate-times.tsv`) lands with this batch; the
  portfolio conclusions above are provisional until it has data.
- Whether C266 is a documentation defect or an intended cache-mode design; it stays OPEN with a
  decision required.
- That each standing self-test gate removes risk; only its cost is being measured.
- The approval manifest's cryptographic provenance. It is an ordinary tracked file; its integrity
  rests on the commit history and review, not on a signature.
- A disclosure policy, incident runbook and compatibility policy. They are the next bounded
  workstream; their absence is a production gap, not a register row.
- Whether the strict binary rule survives a large register. If row count makes it unworkable, the
  thing to increase is fix throughput and cycle honesty, not the threshold for a verdict.
