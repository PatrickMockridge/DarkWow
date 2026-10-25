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

Residual risks and dispositions are approved as one batch (a decision of the project lead). The
approval commit contains:

- `scope_hash`: SHA-256 over the tracker, `residual-risk.md` and this document;
- the explicit list of every ID in the batch with old → new disposition and band;
- the ALARP-record hash;
- five IDs re-derived from the tree by the approver, named in the report.

No wildcard and no "all rows". A row whose evidence is `UNVERIFIED` cannot be in the approved set.
Any later edit to the three hashed files invalidates the approval and returns the batch to approval.
`scripts/check-defect-register.sh` recomputes the hash and refuses a stale approval.

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
3. **Exchange, if used.** A measured `P` gain may justify a residual-risk increase only for
   Broadly acceptable Minor/Moderate residuals, one trade per residual, with a cumulative ledger and
   `ΔR_allowed = min(2, P/10)`. Catastrophic and Major risks are never tradeable.
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
