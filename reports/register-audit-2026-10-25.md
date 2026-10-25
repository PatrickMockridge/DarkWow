# Register retrospective — 2026-10-25

Subject: `doc/src/arch/verification-hazop.md` at `9af26fb23a`, its gate, and the process that
produced it. Three read-only audits (register inventory, doctrine/fix history, production baseline)
were re-read against the files before this report was written.

**This report proposes a batch. Nothing in the tracker, `decisions.md` or `residual-risk.md` changes
until the batch is approved; the approval manifest and its `scope_hash` are produced in the approval
commit.**

## 1. What the evidence says

- 41 tracker rows: 37 OPEN, 4 CLOSED. **16 rows have no control** (`—`), and therefore no
  falsifiable exit criterion. Three rows (`OBL-Z18`, `OBL-Z23`, `OBL-Z3`) use the checker as its own
  control.
- `decisions.md` holds **46** entries, not the 44 the tracker claims; `OBL-C250` and `OBL-C261` are
  simultaneously decisions and live tracker rows.
- 503 commits touch the tracker, **202 of them tracker-only**; **42 commits touch the genesis pin**.
  The tracker-era closed rows took ~58 minutes (`C267`) and ~27 hours (`C268`, `C270`); one row was
  reopened (`C240` → `C259`).
- The gate enforces schema, locus, control resolution and "closure is code", but cannot see
  severity, age, failure-mode falsifiability, whether an OPEN row's control can fail, "no trade",
  quoted evidence, the negative twin, or dual tracking.
- Fixes have repeatedly become the next defect: `e42f2c1c1a`'s `try_reserve` claim became `C265`
  (28 days); the memory-max fix (`ec91497001`) made the container recipe's missing flag a genesis
  failure (`C263`); the manifest check (`bdcf30f4e3`) was broken one day later by the deployooor
  revert (`C262`); the `C240` fix introduced `C259`; the stale registry entry (`C214`) was created
  by an earlier fix.
- The production gaps outside this cycle: no disclosure policy, incident runbook, or compatibility
  policy; the release checklist survives only inside the retired launch register.

## 2. Calibration

Severity `C=5, H=3, M=2, L=1`; likelihood `D=5, P=3, T=1`; `R = S × L`; bands
Intolerable ≥ 15, Tolerable 6–14, Broadly ≤ 5; C and H are never Broadly acceptable. The anchors
are the pre-migration legend (`C` mints/destroys/steals value; `H` breaks a stated property; `M`
defence-in-depth/liveness; `L` wrong but unreachable).

Back-test against recorded decisions:

| Case | S/L | R | Band | Reproduces the decision |
|---|---|---|---|---|
| C265 (wire VarInt OOM) | H/D | 15 | Intolerable | Fixed, not accepted — correct |
| C267 (fee-window divergence) | C/D | 25 | Intolerable | Fixed same day — correct |
| C268 / C270 (port, gate) | M/D | 10 | Tolerable | Fixed with pipeline evidence |
| C259 (introduced by the C240 fix) | H/P | 9 | Tolerable | Open; the residual link is explicit |
| C262 (broken by the port) | M/P | 6 | Tolerable | Proposed residual, not a green |
| C263 (container recipe) | H/D | 15 | Intolerable | Fixed before release |
| C266 (RandomX cache mode vs docs) | H/P | 9 | Tolerable | A design decision, not a fund risk |

The proposed open-row distribution: **5 Intolerable (19%), 20 Tolerable, 1 Broadly acceptable**
(OBL-C136, hygiene). No C/H row falls in Broadly. The Intolerable set is C102, C248, C253, C92
(with C251 merged), C97 — all fund-loss or unauthorised-payout propositions.

## 3. Proposed row dispositions (41 rows)

Format: ID — disposition — S/L → R/band — reason. "Plan:" is a control plan recorded in the Control
cell; it is not green.

| ID | Disposition | S/L | R | Reason |
|---|---|---|---|---|
| C102 | OPEN | C/P | 15 Int. | Fund holdings inert; fixable, control exists |
| C109 | OPEN | H/P | 9 Tol. | Model/code maturity divergence; plan: mirror test |
| C130 | OPEN | H/P | 9 Tol. | Capability selected without ContractId/barbs; control exists |
| C136 | OPEN | L/D | 5 Broad | Test flakiness; plan: bounded condition poll |
| C160 | OPEN | H/P | 9 Tol. | Vote secret from public data; control exists |
| C202 | OPEN | M/D | 10 Tol. | Round-trip covers 2 of 5 arms; control exists |
| C205 | OPEN | M/D | 10 Tol. | No prove+verify burn control; plan: burn vector test |
| C206 | OPEN | M/P | 6 Tol. | PI-7 mismatch not reproducible; control exists |
| C214 | OPEN | H/P | 9 Tol. | Registry shadows manifest; plan: comparison gate |
| C239 | OPEN | M/P | 6 Tol. | Unknown type accepted then rejected; plan: rejection test |
| C240 | **MERGED** into C259 | — | — | Exclusivity is type-carried; the remaining defect is C259; tombstone alias |
| C241 | OPEN | H/P | 9 Tol. | Different preimage than the circuit; plan: agreement test |
| C246 | OPEN | M/P | 6 Tol. | Relayer arrangement has no endpoint-named control |
| C248 | OPEN | C/P | 15 Int. | Unauthenticated coverage proof + stale total; control exists |
| C250 | **DECISION** | — | — | Feature deleted with the upstream port; non-defect (kept in decisions.md) |
| C251 | **MERGED** into C92 | — | — | Same proposition as C92 (older ID); tombstone alias |
| C253 | OPEN | C/P | 15 Int. | Exit trusts caller for identity/weight; control exists |
| C259 | OPEN | H/P | 9 Tol. | Blocking mutex on smol thread; plan: async-aware lock + liveness test |
| C260 | OPEN | M/D | 10 Tol. | Multi-proof arms fail closed; plan: multi-proof plan + refusal test |
| C261 | **CLOSED** | H/D | 15 Int. | Port deleted the content commitment; failing-before = height-2 gas failure; control `test_heavyweight_deployooor` |
| C262 | **RESIDUAL** | M/P | 6 Tol. | Manifest check unavailable for user deployments; ALARP record R-01 |
| C263 | **CLOSED** | H/D | 15 Int. | Shared recipe + in-image gates; control `contrib/wasm_artifact_check.sh` (self-tested) |
| C264 | **CLOSED** | M/D | 10 Tol. | Phase 9 fails fast with named artifact logs; control = the wired pipeline |
| C265 | CLOSED (existing) | H/D | 15 Int. | Landed with control and planted-defect run |
| C266 | OPEN | H/P | 9 Tol. | Cache-mode PoW vs documented dataset; plan: decide enforcement vs docs |
| C267 | CLOSED (existing) | C/D | 25 Int. | Typed chain-derived state; control + planted defect |
| C268 | CLOSED (existing) | M/D | 10 Tol. | Port map + listener guard; pipeline evidence |
| C269 | OPEN | H/P | 9 Tol. | Non-atomic reorg writes; plan: fault-injection control |
| C270 | CLOSED (existing) | M/D | 10 Tol. | Same-height gate; pipeline evidence |
| C45 | OPEN | H/P | 9 Tol. | Two supply trees unreconciled; control exists |
| C54 | OPEN | M/P | 6 Tol. | Stratum template unversioned; plan: generation id at submit |
| C56 | OPEN | M/D | 10 Tol. | Mining gate not atomic; plan: atomic gate + test |
| C74 | **RESIDUAL** | M/D | 10 Tol. | 24/32 manifests lack barbs/schemas; the fix rebuilds 32 artifacts and moves the pin; ALARP record R-02 |
| C92 | OPEN | C/D | 25 Int. | Redemption pays no collateral; C251 merged here |
| C97 | OPEN | C/P | 15 Int. | Exit ignores stored authority; control exists |
| T22 | **RESIDUAL** | M/P | 6 Tol. | Write-path encode unmodelled; compensating Rust encode tests; ALARP R-03 |
| T24 | **RESIDUAL** | M/P | 6 Tol. | Mempool admission/expiry unmodelled; compensating Rust mempool tests; ALARP R-04 |
| Z12 | OPEN | H/P | 9 Tol. | Quotient bound not on chain; control exists |
| Z18 | OPEN | M/D | 10 Tol. | 51 equal-site adjudications; plan: external corpus (the checker is its own control today) |
| Z23 | **RESIDUAL** | M/D | 10 Tol. | 117 adjudicated dead values, none in genesis; report-only gate as compensating control; reopen on circuit change; ALARP R-05 |
| Z3 | OPEN | H/P | 9 Tol. | Domain gate does not check purpose; plan: purpose-specific token check |

Result: 26 OPEN, 5 RESIDUAL, 3 newly CLOSED, 2 merged, 1 decision, 4 pre-existing CLOSED.

## 4. Decisions triage (46 entries)

- **True accepted risks → `residual-risk.md`**: C42 (M/P 6, R-06), C66 (H/P 9, R-07). C80 is
  reclassified as a *decision*, not a risk: merge mining is off by default and the container refuses
  to enable it without a verifier (C95); clarified in `decisions.md`.
- **Requirements or claims, not risks** (stay decisions, no risk record): C19, C21, C32, C85, C134,
  T17, T9, Z8.
- **C250** stays a decision (feature removed). **C261**'s duplicate decision entry is removed; the
  CLOSED row carries the resolution and the failing-before evidence. The tracker's "44 decisions"
  line is replaced by a count the gate computes (45 after these changes).
- All other entries (restated/decision/dissolved) stay unchanged.

## 5. Residual proposals

| ID | Source | Harm scenario | Cheapest control / why not now | Compensating control | Reopen on |
|---|---|---|---|---|---|
| R-01 | C262 | A user deploys with a wallet-side manifest that disagrees with its WASM; the scan cannot compare them | Extend DeployV1 to carry a manifest — an upstream contract change; it removes the wallet-side design the port adopted | Wallet marks user-deployed contract metadata unverified | DeployV1 gains a manifest field |
| R-02 | C74 | Wallet capability discovery misses barbs/schemas for 24 manifests | Add the fields — every manifest is a `SOURCE_MANIFEST` input: 32 artifacts rebuilt and the genesis pin re-rolled | Wallet's native-token path and manifest fallback | A genuine artifact rebuild batch is scheduled |
| R-03 | T22 | The Lean wallet proof covers decode only; an encode divergence is unproved | Model the write path in Lean — a proof campaign | The Rust encode round-trip tests | Next Lean campaign |
| R-04 | T24 | Mempool admission/expiry rules are unmodelled; a divergence is invisible to Lean | Model admission/expiry — a proof campaign | The Rust mempool tests | Next Lean campaign |
| R-05 | Z23 | A new circuit introduces an undeclared dead value | Make the dead-value gate blocking — it would block every circuit change on a known count | The report-only gate names the count each run | A circuit change lands |
| R-06 | C42 | A competing block is destroyed before the fallible work, losing an uncle candidate | Reorder the four call sites — a contained fix | The competing-block cap and logging | Next sync change |
| R-07 | C66 | Caribina anchors to an unsettled Arweave block | Settlement check — impossible for consensus to observe | Anchor verification bounds the assumption | Arweave settlement becomes local data |

Every record follows the same rule: a residual is permitted only when no reasonably practicable
control exists at acceptable cost and a compensating control is named. The two that are cheap
(R-06, R-05) are flagged for the plan backlog rather than treated as permanent.

## 6. Fix-induced defects

| Introduced by | New defect | Mechanism | Cost |
|---|---|---|---|
| `e42f2c1c1a` | C265 | `try_reserve` claim false under overcommit; reserve+memset before read | 28 days |
| `ec91497001` | C263 | memory max became a hard requirement; container recipe lacked the flag | ~2 days |
| `bdcf30f4e3` | C249, then C262 after `f1c551ee9b` | a check was added, then the local envelope it read was deleted | 1 day |
| `572995a2f2` | C259 | type-carried exclusivity uses a blocking mutex | 1 day |
| `760b4fd64e` | C260 | fail-closed refusal became a capability regression | 3 days |
| `cc8ef7f71b` | C214 | a stale registry entry was itself the product of an earlier fix | 14 days |

The pattern is one of: compensating guard beside a root cause (R2), a fix that changes a shared input
without checking its consumers (R4), and a removal that takes a capability with it. The proposed rule
added to `hazop-rules.md`: a fix that adds a guard must name the failure mode the guard could
introduce and the control for it, or remove the guard.

## 7. Model authority

The tree asserts "the Python model is the specification" in `ai-index.md:305`, `consensus/safety.md`
and `consensus-core-map.md`, while `wallet_model.py` and `fee_model.py` — both wired as gates —
explicitly deny that authority. Proposed classification, for the batch:

- **Specification**: `chain_model`, `chain_validation_model`, `supply_chain_model`,
  `fee_window_model`, `sync_model`, `dex_lock_model`, `nullifier_lifecycle`, `halo2_math` (vectors).
- **Mirror** (a check of the code; divergence is a finding, not automatic authority):
  `wallet_model`, `fee_model`. Their disclaiming comments stay and the doctrine is corrected to name
  the exception; the gate comments say so.
- **Tool**: `capability_discovery`, `generate_wallet_fixture`, `pipeline_model`, `dockernet_model`.

The contradiction is corrected at the three doctrine sites and in the two model headers; no gate
changes are needed for the mirror pair.

## 8. Gate portfolio (provisional — measurement first)

The 112 `run_gate` sites are not reviewed wholesale; that would be the bureaucracy being removed.
`run_gate` will first record `gate,seconds,rc` to `reports/gate-times.tsv`. The review then covers
only Tier 1, report-only and red gates:

- The two report-only gates (dead values, host domain separation) are candidates to stay report-only
  with Z23 as their residual, or to become blocking with a bounded corpus.
- The heavyweight anti-pattern scanner is **expected red by design** and wired blocking; that is a
  candidate for an explicit declared-red gate rather than a permanently red umbrella.
- The four doc-table/census gates overlap heavily; a single markdown-table checker is a candidate
  merge.
- The retired launch register is the precedent: a second ledger policing document drift was removed.

No gate retires without a tombstone (invariant, replacement, planted-defect run, residual ID if
none), and no retirement counts as a `P` gain.

## 9. Worked example

RAM campaign (`fa76d8c2ff`, `ae1a8e4e5e`): node1 4.18 GiB OOM evidence → 2729 MiB steady state =
36% runtime improvement. `P = 40% × 36% = 14.5`. At 10 `P` per `R` and a 2 `R` cap, it buys ≤1.45
`R`; the change was risk-reducing, so no trade. `OBL-C267` at `C/D = 25` would need 250 `P`, above
the 100 maximum: the risk-first rule is absolute.

## 10. Batch manifest (for approval)

The batch consists of: the 41 row dispositions in §3; the decisions changes in §4; residuals R-01…R-07;
the model-authority classification in §7; the policy in `doc/src/arch/risk-management.md`; and the
schema/tool changes in the implementation plan.

- `scope_sha256 = 952fc1d5c8892538ced56e15d931042820bb31868f7d4e330077a1c5ca4cae3e`
  (SHA-256 over `verification-hazop.md`, `residual-risk.md`, `risk-management.md`).
- The explicit batch ID list is in `reports/register-approval.txt` (`batch=`); the gate resolves every
  ID against the tracker or its appendix.
- Five IDs re-derived from the tree for the approver: **C102** `C/P` → 15 Intolerable; **C109** `H/P`
  → 9 Tolerable; **C136** `L/D` → 5 Broadly acceptable; **C92** `C/D` → 25 Intolerable; **Z18** `M/D`
  → 10 Tolerable. Each is read from the row's Risk cell and `risk-management.md` §2–4, not from this
  report's prose.

Any edit to those three files after approval invalidates the hash and returns the batch to approval;
the gate fails on a stale hash.

## 11. Not established

- Per-commit closure attribution for the 233 pre-migration "settled" rows.
- Whether any pre-migration row was reopened other than the documented C240 case.
- The exact wall time of each gate (instrumentation lands with this batch; the portfolio conclusions
  above are provisional until it does).
- Whether C266 is a documentation defect or an intended cache-mode design; the batch records it as
  OPEN with a decision required, not as a residual.
