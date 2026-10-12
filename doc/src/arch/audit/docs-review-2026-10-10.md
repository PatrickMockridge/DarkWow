# Documentation-vs-Code Review — Findings Ledger

**Started 2026-10-10; the review is complete and fully adjudicated** — read the code before acting on any
row (`R6`). Four phases (the registers; the formal spine + Lean/Python; the contract descriptions;
arch/dev + the narrative tail) reviewed the corpus against the code, and a refuter pass adjudicated
every row: **486 findings — 239 CONFIRMED, 29 PARTIAL, 3 REFUTED**. This is the intake ledger for that
review. It is **not a home**: remediation moves each confirmed finding to where it belongs (a doc edit, a
new `OBL-*` row in [the register](../verification-hazop.md), or a new gate) and records the stamp in the
`<!-- promotion-log -->` block at this file's foot, which `scripts/check-ledger-promotion.sh` reads.

## Why this exists

The repository instruments its **code** heavily (≈30 gates pin circuit ↔ entrypoint ↔ client ↔
manifest) and its **docs** *structurally* (`scripts/check-doc-index.sh`). Nothing read a doc's **prose
claim about a code fact** and compared it to the code. Phase 1 found that this gap has, over months,
let **every register and audit document drift from the code in the same direction: the code moved and
the record did not.**

## Method, and the shape of the result

Ten read-only agents partitioned the register corpus; each finding carries both sides quoted with
`path:line`. Instruments are read by scripts that ship a negative control (`R8`). The dominant result
is not new defects in the *code* — it is that the **record overstates its own currency**: instruments
described as they were, counts frozen at an old commit, and findings presented as open that the code
has closed.

**Classes** (remediation home): DOC-STALE → edit the doc where load-bearing (`R11`);
CODE-VIOLATES-ITS-OWN-SPEC → new `OBL-*` row; AMBIGUOUS-SPEC → restate the clause;
REGISTER-ROW-UNSOUND → amend the row in place; INSTRUMENT-GAP → build the detector (+ control).
**Severity** uses the register's legend: C mints/destroys/steals value; H breaks a stated property;
M defence-in-depth or liveness.

## Summary

| Cluster | # | The shared cause |
|---|---|---|
| 1 — Register prose, counts and instrument descriptions | 12 | the register's prose is not gated; it drifted from its own rows and scripts |
| 2 — Status cell vs body (`DECISION` token) | 3 | the 2026-10-08 relabel changed cells, append-only bodies kept the old word |
| 3 — Stale register rows (code moved, row not reconciled) | 18 | the row's proposition or figures were true at some commit and are not now |
| 4 — Lean HAZOP matrix | 22 | the riskMatrix presents 2026 findings as open; the circuits fixed them |
| 5 — Lean assumption boundary (`check_lean_axioms.py`) | 5 | the intake checker has blind spots and no control for the inventory |
| 6 — `security-analysis.md` | 14 | the audit quotes code that has since changed, as live "FIXED" |
| 7 — sync red-team audit + audit README | 7 | resolutions and a "lesson" the code and spec contradict |
| 8 — `safety.md` root-cause ledger | 16 | the canonical ledger drifted from the Lean work and the register |
| 9 — Cross-cutting instrument gaps (`R8`) | 4 | gates that name a control they do not ship |
| **Total** | **101** | plus the 7 from Phase-0 detectors (below) |

By class: DOC-STALE 68 · REGISTER-ROW-UNSOUND 12 · INSTRUMENT-GAP 9 · AMBIGUOUS-SPEC 5 ·
CODE-VIOLATES-ITS-OWN-SPEC 3 · (plus 7 Phase-0, all DOC-STALE/INSTRUMENT-GAP).

**Adjudication (Phase 5): 95 CONFIRMED · 11 PARTIAL · 2 REFUTED** — see the section below.

**Phase 2 (the formal spine): +109 findings** (`F-301`–`F-375`; some rows aggregate related findings).
**Adjudicated: 66 CONFIRMED · 9 PARTIAL · 0 REFUTED** — of which **11 are confirmed
`CODE-VIOLATES-ITS-OWN-SPEC`** (a normative clause the code does not enforce) owned by no existing row.
See the Phase 2 section below.

**Phase 3 (the contract descriptions): +136 findings** (`F-501`–`F-551`; some rows aggregate).
**Adjudicated: 45 CONFIRMED · 5 PARTIAL · 1 REFUTED** — of which **9 are confirmed `CODE-VIOLATES`**
owned by no existing row (deployooor's `LockV1` no-signature lock, dex's transfer-not-swap,
relayer_endowment's unpayable `ForceSettle`, `audit.md`'s all-false fix table, …). The dominant family:
a doc's "composes with X" section names a cross-contract child call the entrypoint never makes. See the
Phase 3 section below.

**Phase 4 (arch/dev + narrative tail): +133 findings** (`F-601`–`F-637`; some rows aggregate).
**Adjudicated: 33 CONFIRMED · 4 PARTIAL · 0 REFUTED.** The dominant family: `dev/` and `testnet/`
operational docs running commands that do not exist, plus ~25 stale counts/constants, ~15 phantom
modules/symbols, and CODE-VIOLATES where a spec the code does not meet (or two specs that contradict).
See the Phase 4 section below.

**The review is complete and fully adjudicated:** all four phases recorded, every row refuted-or-not.
Totals — **486 findings; 239 confirmed rows**, of which **~25 are `CODE-VIOLATES-ITS-OWN-SPEC` owned
by no existing row** (the Phase-2 `F-3xx` and Phase-3 `F-5xx` sets), the rest the record drifting from
the code. The next exercise is remediation.

---

## Phase 5 — adjudication (refuter pass)

Seven independent refuters re-opened every finding, verified both quotes verbatim, reproduced any
cited command, and dedup-checked against the existing rows. Verdicts over the 108 findings:

| Verdict | Count |
|---|---|
| CONFIRMED | 95 |
| PARTIAL (a weaker/different claim; correction below) | 11 |
| REFUTED (struck) | 2 |

The refutation rate is **2 of 108 (~2%)** — far below the register's own 2026-09-23 precedent (~10 of
14). Read that as the producers having quoted both sides and verified their paths, **not** as the
refuters being idle: the same pass corrected a figure or a framing in 11 further findings.

**Refuted — do not action:**
- **F-154** (HAZOP MOD-7, `NULLIFIER_K` for signature derivation): the pattern is present *verbatim* in
  the circuit; the producer's own text conceded "unclear it is a defect". Not a stale finding.
- **F-190** (register `RC1`–`RC6` collision): a duplicate of F-111. One finding, not two.

**Partial — action with the correction:**
- **F-004** (`slot.md`): the omission is real (`reveal_spin` appears nowhere), but `slot.md` has no
  "All N" table, so the "circuit table lists 2" framing is loose.
- **F-112** (`OBL-Z6`): drop the sub-claim "the Lean module calls it partly" — `HashOps.lean` contains
  no such word; the cell/body divergence (`DECISION` vs "stays `PARTLY`") stands.
- **F-129** (`OBL-Z19`): the cell is **not empty** — it holds "**M**" misplaced in the "Checked today
  by" column. A column-placement defect, not an absent cell.
- **F-136** (HAZOP paths): it is **15 of 15** fully-qualified paths that fail to resolve, not 14.
- **F-152** (HAZOP MOD-3): the witnesses gained `range_check`s but remain unbound; "partly superseded"
  is the exact degree.
- **F-158** (`check_lean_axioms.py`): `run-all-tests.sh` *does* run `check_inventory`
  (`--require-collector`), so "cannot fail in CI" is wrong; the residue is that its failure path has no
  *shipped* automated control (`R8`).
- **F-186** (`safety.md` `DrainConfig`): the bullet sits under the "How it has manifested" section,
  which documents *resolved* findings by design; the defect is only that it lacks the fix marker its
  siblings carry.
- **F-192** (`safety.md`): "a finding is never recorded in both places" — the register rows are recast
  *obligations* cross-referenced by RC7, not duplicate write-ups; the absolute claim is arguable, not
  false.
- **F-194** (`safety.md`): "seven deactivation functions across four contracts" is definition-dependent
  (a narrow reading yields 6 across 4); the number cannot be crisply adjudicated.
- **F-198** (`check-wallet-kernel.sh`): a planted-defect control *was* performed (recorded in
  `run-all-tests.sh`), so "no control" is wrong; the residue is that it is not *shipped* in the script.
- **F-200** (severity legend): the facts hold (the legend is C/H/M; 17 rows carry `L`); the
  "severity-partitioning instrument" it warns about is named nowhere.

Two CONFIRMED findings carry a minor caveat: **F-110**'s "72/78/84" aside is not reproducible (the
"every one fail-open" vs "zero fail-open" contradiction holds); **F-121**'s cited "49" is a historical
paragraph (the row's "57" vs the run's "52" stands).

---

## Phase 2 — the formal spine (109 findings, unadjudicated)

Ten review agents over the normative specs (`type-system`, `contract-wasm-type-system`, `ocap`,
`composition`, `privacy`, `privacy-model`, `consensus/*`, `consensus-coinbase`, `uncle_merkle`,
`fee-spec` + `fee_model.py`, `genesis`, `manifest`, `sync-protocol`, `sync-conformance`,
`net-node-boundary`, `mempool`, `wallet`, `key-management`, and the Python models). Read-at commit
`1bf6faa25f31ad50f2d2e49f9914eb9a70547b51`. **Not refuted** — the Phase-5 refuter pass has not run on
these; treat each as a producer hypothesis.

The dominant new class is **`CODE-VIOLATES-ITS-OWN-SPEC`** — a normative clause the code does not
enforce — which the count-and-table drift of Phase 1 could not reach.

### 2A — A normative clause the code does not enforce (`CODE-VIOLATES`)

| ID | sev | doc | code | claim |
|---|---|---|---|---|
| F-301 | H | `fee-spec.md:1826` (FI-GEN-1) | `fee_window.rs:298` | fee economic parameters SHALL be genesis sled state; `BASELINE_STORAGE` is a `pub const` and the alphas/caps are hardcoded |
| F-302 | H | `fee-spec.md:2026` (FI-PLAIN-2) | `crates/dwow-mempool/src/lib.rs:443` | a fee not matching the re-derived value SHALL be rejected; the mempool never re-derives (`extract_tier` unused) — an arbitrary declared fee is admitted |
| F-303 | H | `fee-spec.md:2064` (FI-RISK-1) | `bin/dww/src/lib.rs:276` | the per-contract risk multiplier (1.0×→2.0×) SHALL apply; it is always `BASELINE` (inert) |
| F-304 | M | `fee-spec.md:2069` (FI-RISK-2) | `bin/dww/src/lib.rs:276` | the wallet SHALL compute a trust metric; no such metric exists |
| F-305 | M | `type-system.md:1118` | `src/net/channel.rs:76` | `ChannelId(u32)` SHALL be the nominal type; the type does not exist, the field is bare `u32` |
| F-306 | M | `type-system.md:1141` | `src/net/session/mod.rs:60` | `SessionSelector` SHALL replace the `SessionBitFlag` alias; neither exists in the tree |
| F-307 | M | `net-node-boundary.md:215` | `src/net/message_publisher.rs:246` | barb enforcement at dispatch is named as active; both `barbs()` impls are **commented out** |
| F-308 | M | `mempool.md:156` | `crates/dwow-mempool/src/lib.rs:58` | §5.2's `extract_fee → None → REJECT`; the trait returns non-optional `FeeAmount`, `ZERO` when absent — non-FeeV3 txs are never rejected for this |
| F-309 | M | `contract-wasm-type-system.md:1204` | `src/runtime/` | re-entrancy detection SHALL reject with `InvalidFunction`; no detection exists |
| F-310 | M | `contract-wasm-type-system.md:178` | `src/contract/` | `#[deny(unreachable_patterns)]` SHALL be on the dispatch match; absent from the whole contract tree |
| F-311 | M | `contract-wasm-type-system.md:870` | `src/runtime/import/util.rs:112` | every host fn SHALL call `acl_allow` and return `i64`; `drk_log` does neither |
| F-312 | H | `privacy.md:39` | `src/contract/box/manifest.toml:108` | an L1 observer sees "only a nullifier and a Merkle root"; `box::Take` still publishes `contents_commit` (the gate reports 1 live leak) |
| F-313 | M | `key-management.md:112` | `crates/dwow-accounts/src/lib.rs:1032` | PBKDF2 password is the mnemonic **phrase**; the code hashes the decoded **entropy** — a silent BIP39 divergence |
| F-314 | M | `consensus/consensus.md:19` | `src/linear/src/block.rs:142` | "nullifier_root header verification `[IMPLEMENTED]`" and Phase 6 as a rule; the field is `RESERVED`, always zero, compared by nothing |
| F-315 | M | `consensus/consensus.md:548` | `src/linear/src/chain_state.rs:1581` | the finality guard covers "Caribina **or Monero**"; only `caribina::verify_anchor_proof` participates |

### 2B — The spec contradicts itself or the code (`AMBIGUOUS-SPEC`)

| ID | sev | doc | code | claim |
|---|---|---|---|---|
| F-316 | M | `consensus-coinbase.md:111` vs `:1006` | `src/linear/src/block.rs:734` | §1.3 says `total_reward == expected_reward(h)`; §6 and the code say `base − Σ pin`; the two sections disagree |
| F-317 | M | `consensus-coinbase.md:279` | `native_token/.../mod.rs:1047` | §2.6 check #7 says reward "meets or exceeds" expected; the handler enforces **exact equality** (the lower-bound was the pre-HAZOP-F1 over-mint bug) |
| F-318 | M | `uncle_merkle.md:735` | `src/linear/src/validation.rs:287` | says the uncle PoW is judged against the canonical block's target; the code uses the target at the uncle's **own** height (the doc's own §2 says this) |
| F-319 | M | `consensus/consensus.md:678` | `src/linear/src/block.rs:313` | `finality_flags` "at offset 145"; no serializer produces that offset (it is excluded from the mining blob; ~329 in the canonical codec) |
| F-320 | M | `privacy.md:701` (wallet) | `bin/dww/src/capability.rs:29` | a capability in the `Reserved` state SHALL NOT be selected; `CapStatus` has no `Reserved` (nor `Unspent`) — the SHALL is unsatisfiable |
| F-321 | M | `type-system.md:1728` | `Capability/Inversion.lean:259` | `capabilityPredicateBypass_prevention` is said to "close" the bypass; the Lean explicitly disclaims the provenance half (`OBL-T7`) |
| F-322 | M | `ocap.md:132` | `Capability/Composition.lean:179` | §2.3's composed type disagrees with the Lean `tenderBidType` it is said to `PROVE` |

### 2C — Register-row unsound

| ID | sev | doc | code | claim |
|---|---|---|---|---|
| F-323 | M | `verification-hazop.md:1452` (OBL-C145) | `check-l1-wire-conformance.sh:264` | the row is `CLOSED` with an evidence cell quoting "PASS: **0** declared wire leak(s)"; the gate now prints **1** (the leak was added after the close, commit `da06073001`) |

### 2D — Phantom symbols, modules and paths in the docs

| ID | doc | claim |
|---|---|---|
| F-324 | `wallet.md:283` | `ProverContext::build_proof` — no such method anywhere |
| F-325 | `wallet.md:893` | `build_fee_and_finalize_tx` — phantom; the API is `FeeHandoff`/`prepare_fee_call` |
| F-326 | `wallet.md:331`, `wallet-vs-daemon.md:190` (H) | `manifest_verify.rs` — the trust-layer-1 "manifest-vs-WASM verification" module does not exist |
| F-327 | `manifest.md:555` (×3) | `bin/dww/src/rpc.rs` — no such file (`scan.rs` holds `scan_block_linear`) |
| F-328 | `genesis.md:118` | the "new genesis contract" checklist names `set_contract_data` in `init_linear()` — that call exists nowhere |
| F-329 | `sync-conformance.md:42` (×many) | cites `§14.2`/`§15` — clauses that do not exist in `sync-protocol.md`; `p2p_wallet.rs` cites `§15` and the gate PASSes |
| F-330 | `sync-protocol.md:429` code cites | `chain_state.rs`/`sync_types.rs` cite `§14.3`, `§19.3`, `§19.4`, `§6` — all dangling |
| F-331 | `wallet_model.py:2646` | cites `bin/dww/src/cache.rs` — no such file |
| F-332 | `type-system.md:1802`, `:1730` | `window_boundary_emission`, `verifierMinimumDisclosure` — dangling Lean decl names |

### 2E — Stale formulas, constants and counts

| ID | doc | claim |
|---|---|---|
| F-333 | `contract-wasm-type-system.md:2351` (H) | the block-anchor leaf formula omits the nullifier **and** `MERKLE_LEAF`; a reader computes a mismatching leaf |
| F-334 | `consensus-coinbase.md:191` | `derive_instance` given as a 3-element hash; the SDK is 4-element with a domain separator — a re-implementer cannot decrypt the coinbase |
| F-335 | `uncle_merkle.md:413`, `consensus-coinbase.md:1258` | mining blob stated as 260 bytes; it is **292** (OBL-C64) — a client built from the spec never meets target |
| F-336 | `fee-spec.md:1206` | §12.4.4's CF formula places the floor inside; code and `fee_window_model.py` floor once |
| F-337 | `fee-spec.md:1971` (FI-WINDOW-2) | "float SHALL NOT be used"; the CF path uses `f64` throughout |
| F-338 | `type-system.md:1456` | "all 32" barbs; the alphabet is **33** (drops `Shard`) |
| F-339 | `type-system.md:1505` | the barb space is 2²²; the alphabet is 33 → 2³³ |
| F-340 | `type-system.md:1718` | "180 `.zk` sources"; the instrument roots yield **178** |
| F-341 | `type-system.md:1684` | "all 10 pairs in §8.4"; the table has **13** rows |
| F-342 | `contract-wasm-type-system.md:758` | "29 host functions"; the code registers **30** (8 omitted from every table) |
| F-343 | `contract-wasm-type-system.md:823` | error codes "(1-24)" and a stale `error.rs` cite; codes run **1-37** |
| F-344 | `contract-wasm-type-system.md:133` | `__metadata` signature `-> Vec<u8>`; the macro requires `ContractResult` |
| F-345 | `contract-wasm-type-system.md:782` | §A.4.2 tables allow `Update` for `get_object_bytes_`/`get_object_size_`; the ACL excludes it |
| F-346 | `contract-wasm-type-system.md:2690` | `O_CEILING = 3` "from Merkle proof cost (34 witnesses)"; the Lean derives it from operation count |
| F-347 | `privacy.md:104` | "10 declared slots across Box and Purse"; the gate has **1** |
| F-348 | `privacy.md:215` | the domain vocabulary is "7 values"; the code defines **10** (`privacy-model.md §3` itself uses domain 8) |
| F-349 | `privacy-model.md:31` | names `fee_v3_tx_binding`, removed by `OBL-C198` |
| F-350 | `ocap.md:473` | `Types.lean` "14 barbs, 13 types"; it has **33** barbs, 14 types (14 is the SDK subset) |
| F-351 | `ocap.md:96,98,345,351` | the commitment/nullifier preimages omit the V2 domain tag (7-arg vs 8, 2-arg vs 3) |
| F-352 | `consensus/consensus.md:634` | documents the "instant blocks" ease branch `OBL-C37` **removed** — following it reintroduces the bug |
| F-353 | `consensus/consensus.md:657` | "`0x00FFFFFF`" (and ~1/256); the default is `0x0FFFFFFF` (~1/16); the doc's own table says so |
| F-354 | `consensus/consensus.md:794`, `chain_architecture.md:72` | `BlockConnectOutcome` has "five variants"; it has **four** |
| F-355 | `consensus/consensus.md:110` | the "boundary codec" rail is presented as inert-but-present; the trait/macro were **removed** |
| F-356 | `consensus/linear_blockchain.md:66` | the `UncleProof` struct has `header`+`pow_hash`; the real one has neither |
| F-357 | `consensus/linear_blockchain.md:245` | "25 of 28 contracts"; the tree has 32/33 (`check-doc-canonical-counts` P1 does not match this phrasing) |
| F-358 | `consensus-core-map.md:46` | mechanism 4's Python count 3871; the file is **3918** |
| F-359 | `consensus/consensus.md:17,18` | two IMPLEMENTED citations (`:736-760`, `blockchain.rs:106-137`) point at unrelated code |
| F-360 | `consensus-coinbase.md:1436` | 100-year supply "~37.8M DRKW"; §4.5 gives 37,326,004 (the doc's own table disagrees) |
| F-361 | `consensus-coinbase.md:279`, `:1603`, `:1624` | §17.4.1 check #6 (nullifier non-zero) and §17.4.3's uncle-mint algorithm describe a design the code no longer has |
| F-362 | `sync-protocol.md:314` (§12) | calls `sync_model.py` the "1:1 executable specification"; the model has a **five**-variant `SyncState`, the Rust **two** |
| F-363 | `sync-protocol.md` §8.1 | the constants table omits `HANDSHAKE_TIMEOUT` and `MAX_SYNC_CONNECTIONS` |
| F-364 | `mempool.md:51` | `max_age_secs = 12,000`; the code default is **3600** (`wallet.md` agrees with the code) |
| F-365 | `genesis.md:319` (H) | says `init_genesis` hard-errors on a genesis mismatch; enforcement is deliberately elsewhere (`check_genesis_pin` from `init_linear`) |
| F-366 | `genesis.md:242` | the genesis header-field table lost its header row (renders as a paragraph) and omits `anchor_owner`/`caribina_anchor` |
| F-367 | `genesis.md:92` | `genesis_contracts()` cited at `:915-927`; it is at `:949-961` |
| F-368 | `manifest.md:263` | advertises a `string` param type no code arm implements |
| F-369 | `manifest.md:9,8` | "6/6 unit tests" (now 27) and Python "13 tests" (file has 114) |
| F-370 | `wallet.md:281,284,295`, `key-management.md:193` | four spec'd boundary signatures don't match the code (`wallet_construct`, `CircuitWitnessMap::from_manifest`, `generate_proof`, `compute_balance`); a fifth §6.4.1 "closed" rule table omits two accepted rules |
| F-371 | `wallet_model.py:2532` | the model omits the `TokenId` alias the Rust accepts — a manifest parses in Rust but fails-closed in the model |
| F-372 | `type-system.md:141`, `ocap.md:441`, `:457` | misc: normative barb count vs §10's "22"; `requiredBarbs` on `Action` vs `Resource`; a `#print axioms` sentence the Lean flags as previously false |
| F-373 | `consensus/chain_architecture.md:59` | lists `uncle_commitment_set` as a live field; deleted (`OBL-C26`) |
| F-374 | `consensus/scaling.md` (6 notes) | every "Scaffolding" note cites `sharding` files and a feature that no longer exist |
| F-375 | `sync-conformance.md:60` | claims the gate fails on a header that "drifts"; it only greps the substring and does not resolve the clause number |

### 2F — Line-citation drift (class note)

Pervasive across `type-system.md`, `contract-wasm-type-system.md`, `consensus.md`,
`consensus-coinbase.md`, `wallet.md`, `net-node-boundary.md` — cited `file:line` ranges that have moved
while the named symbol still holds the claim (the `OBL-Z5` class). Not filed row-by-row; representative
instances are folded into the rows above. No gate reads a doc's line citations.

### Phase 2 adjudication (refuter pass)

Five refuters over the 75 rows. **66 CONFIRMED · 9 PARTIAL · 0 REFUTED.** The rate is 0%, but the pass
did real work — the partials corrected overstatements and mis-citations.

**The 11 confirmed `CODE-VIOLATES` that no existing row owns** — the campaign's highest-value output,
genuine code-vs-spec gaps invisible until a reader compares the clause to the code:
`F-301` (FI-GEN-1), `F-302` (FI-PLAIN-2), `F-303` (FI-RISK-1), `F-305` (`ChannelId`), `F-306`
(`SessionSelector`), `F-308` (mempool non-FeeV3 rejection), `F-309` (re-entrancy), `F-310`
(`deny(unreachable_patterns)`), `F-311` (`drk_log` ACL), `F-313` (PBKDF2 entropy), `F-314`
(`nullifier_root` verification).

**The 4 partial `CODE-VIOLATES`** — three are **already tracked** (not new): F-307 → `OBL-C32`,
F-312 → `OBL-C145`, F-315 → `OBL-C65/66/67/80`; and F-304 is **overstated** — the wallet *does* display
a per-contract trust tier (`resolve_show_trust`), just not the four-input metric the spec demands, so
the residual is narrower than "no such metric exists".

**Corrections the partials force:**
- **F-320** — the sentence is in `wallet.md:701`, not `privacy.md:701` (a mis-citation in the producer's
  row); and it duplicates the already-open `OBL-C126`.
- **F-350** — the doc's "14 barbs, 13 types" is stale, but the finding's own replacement count ("14
  types") is **also wrong**: the recount is 33 barbs and **17** primitive types (and register `OBL-T9`
  says 16 — the primitive-type count has drifted across at least three records).
- **F-361** — two of its three sub-claims hold; the third (the §2.6 `pow_reward_v1` "nullifier non-zero"
  cite) is refuted — that check is still enforced.
- **F-323** — the stale count is superseded in substance by `OBL-C184`.
- Several doc line-cites are off by one (`F-355`, `F-356`, `F-357`, `F-362`) — the line-drift class;
  content unaffected.
- **F-332** — overreaches: the `:1730` identifier it calls dangling is a real Lean theorem.

---

## Phase 3 — the contract descriptions (136 findings, unadjudicated)

Nine agents over the 42 `doc/src/contract/*.md` (eight batches of paired docs + the analysis/app-layer
docs), each against its manifest, circuits, entrypoint, model and client. Read-at `cb0c85ce8f`.
**Not refuted.** The Phase-0 detectors already own the circuit-table and function-table classes, so
these are what they cannot see — the same families recur across nearly every contract.

### 3A — A cross-contract call the doc names and the code does not make

The dominant family: a doc's opening "composes with X" section names a child call the entrypoint never
makes. `Purse::DepositV1`/`WithdrawV1` is claimed by `drain_protection`, `escrow`, `betting_stake`,
`labor_market`, `relayer_endowment` — none call Purse (all use `promissory_note::transfer_v1 0x04`),
and each crate carries an unused `*_PURSE_CONTRACT_ID` constant.

| ID | sev | doc | claim |
|---|---|---|---|
| F-501 | H | `drain_protection.md:10`, `escrow.md:9`, `betting_stake.md:9`, `labor_market.md:8`, `relayer_endowment.md:8` | the Purse/Box composition family — no such child call exists |
| F-502 | H | `auction.md:12` | "composes with escrow"; it composes with `promissory_note` (the stale claim is in the crate's own comments too) |
| F-503 | H | `box.md:139` | lists `drain_protection`/`subscription`/`dao_escrow` as composing Box — only `escrow` does |
| F-504 | H | `identity.md:195` | the `VerifyCapabilityV1` caller table names `subscription` (own circuit), `tau` (**no such contract**), `dao_escrow` (removed) |
| F-505 | H | `tender.md:315` | "`claim_id` is verified by attestation" — tender never calls attestation |
| F-506 | H | `subscription.md:229` | claims a DAO-Escrow membership Merkle proof in `subscribe.zk`; the circuit instances only `derived_id` + the tx pair |
| F-507 | H | `insurance_market.md:350` | claims PN `RevokeV1`/`IssueV1`; every child call is `TransferV1` |
| F-508 | H | `promissory_note_intermediaries.md:190,204,198` | the bridge "uses TransferV1 exclusively" / "does not use RedeemV1" — it uses `IssueV1`/`RedeemV1`, and never `validate_child_value_commit` |
| F-509 | M | `game_room.md:168,183` | Deposit/Withdraw use `RevokeV1`/`IssueV1`; the code uses `TransferV1` |
| F-510 | M | `pool_stake.md:8` | Slash uses a `transfer_v1` child; `process_slash_coverage` reads no child |

### 3B — A mechanism the doc advertises that the code does not implement (`CODE-VIOLATES`)

| ID | sev | doc | code | claim |
|---|---|---|---|---|
| F-511 | H | `deployooor.md:63` | `deployooor/src/entrypoint/lock.rs:58` | `LockV1` verifies "a signature from the deployer"; it publishes an **empty** signature-pubkey vector and checks no signature — any tx can lock any deployed contract |
| F-512 | H | `dex.md:109` | `dex/src/entrypoint/execute_swap.rs:109` | `ExecuteSwap*` drive `otc_swap_v1 (0x05)`; the enforced child byte is `0x04` (`transfer_v1`) — the DEX does plain transfers (the code comments say 0x05 too) |
| F-513 | H | `relayer_endowment.md:92` | `relayer_endowment/src/entrypoint.rs:980` | `ForceSettle`'s pro-rata share reads `total_collected_fees_log`, **never incremented** — the guard is always false and it can never pay |
| F-514 | M | `insurance_market.md:179` | `resolve_claim_with_capability.rs:56` | `ResolveClaimWithCapabilityV1` carries a `capability_proof` the handler never reads |
| F-515 | M | `dex.md:158` | `create_swap.rs:100` | "Host verifies `schnorr_verify`"; `get_metadata` publishes no signature pubkeys and no host check runs |
| F-516 | M | `stablecoin.md:45` | `stablecoin/src/entrypoint.rs:124` | four "selectable models"; `model` is decoded but never stored or branched on |
| F-517 | M | `stablecoin.md:178` | `entrypoint.rs:1755` | "verifies every exposed input"; `interest_accrued` is not (`OBL-C204`) |
| F-518 | M | `otc_swap.md:170` | `otc_swap/proof/cancel_swap.zk:66` | the cancel circuit enforces the timeout via `less_than_strict`; it was **removed** (host-enforced) |
| F-519 | H | `audit.md:24,27,28,25` | `bridge/src/lib.rs:69` | the "Key Fixes Applied" table advertises four bridge fixes (`ReassignWithdrawalV1`, `token_minimum`, sparse-Merkle, slashing) — none is in the tree |
| F-520 | M | `dex.md:40` | `dex/src/model/mod.rs:64` | `InitializeV1` registers a governance key; none is stored/written |

### 3C — A module, function or field the doc teaches that does not exist

| ID | doc | claim |
|---|---|---|
| F-521 | `wallet.md` (via contract docs) | `manifest_verify.rs`, `build_proof`, `build_fee_and_finalize_tx` — see Phase 2 |
| F-522 | `composability.md:74` | `validate_child_transfer_value` — phantom; the helper is `validate_child_value_commit` |
| F-523 | `composability.md:57` | `Output.public_value`/`public_asset_id` and a `TransferOutput_V1` circuit — neither exists |
| F-524 | `game_room_app_layer.md:122,81` | `entropy_commit`/`entropy_reveal`/`get_account` — no such `GameRoomClient` methods |
| F-525 | `drain_protection.md:70` | `GraduatedTiers` / `DrainConfig::full/conservative/minimal` — none exist |
| F-526 | `promissory_note.md:292` | an `is_notequal(value, 0)` gate and a whole "Breakthrough" section on it; the opcode is `is_not_equal` and `Redeem` uses `constrain_equal_base(value, ZERO)` |
| F-527 | `multi_contract` field tables | `game_room` `PlayerAccount.balance/locked`; `relayer_endowment`/`betting_stake`/`pool_stake`/`roulette`/`darkbet_exchange`/`bearer_bond`/`oracle`/`auction`/`attestation` struct tables each omit `version`/`instance_seed`/… — a serialization-layout mismatch (aggregated; ~15 rows) |

### 3D — A state machine the doc draws that the code cannot reach

| ID | doc | claim |
|---|---|---|
| F-528 | `game_room.md:57` | `RoomState::Concluded` unreachable |
| F-529 | `attestation.md:124` | `ClaimState::Rejected` never set (reverts instead) |
| F-530 | `auction.md:63` | a `Claimed` bid state; `BidState` ends at `Refunded` |
| F-531 | `tender.md:105` | `BidState::Expired` unreachable |
| F-532 | `darktoshi_dice.md:16` | a single `SETTLED` state and a SettleBet→CANCELLED edge; five states, none named SETTLED |
| F-533 | `darkbet_exchange.md:249,283` | `MarketState::Closed/Cancelled` and `PositionState::Refunded` never set; no `None` variant |
| F-534 | `lottery.md:77` | a four-state machine with `TICKETS_SOLD`; only three states exist |
| F-535 | `labor_market.md:118` | Cancel-from-Delivered edge; Cancel is Created-only; Dispute/Refund edges omit InProgress |

### 3E — Stale opcodes, layouts, trees, counts

| ID | doc | claim |
|---|---|---|
| F-536 | `composability.md:102,109`, `recruitment_pipeline.md:104` (H) | `Identity::VerifyCapabilityV1` written `0x0b`; it is `0x06`; and `VerifyMemberCapabilityV1`/`ResolveDisputeV1 (0x0b/0x0c)` are retired dao_escrow selectors |
| F-537 | `oracle.md:188`, `:70` (H) | `DOMAIN_OPERATOR_COMMITMENT = witness_base(4)`; all six circuits use `witness_base(8)` — a client's proofs fail |
| F-538 | `promissory_note.md:537` (H) | the five Public-Input-Layout sections omit the trailing `tx_binding`/`tx_nonce` pair (the wire spec 21 contracts zip against) |
| F-539 | `promissory_note.md:379`, `otc_swap.md:143`, `escrow.md:119` | params/pubinput snippets omit `tx_nonce`/the trailing pair; `escrow`'s commitment omits the domain tags |
| F-540 | `dao_escrow.md:252` (H) | the `constrain_instance` order for 3 of 5 circuits is the pre-`OBL-C198` order — a client publishes in an order the circuit rejects |
| F-541 | `multi_contract` V1-vs-V2 | `bearer_bond.md:393` (`Burn_V1`…), `promissory_note.md:876` (`*_v1.rs` files), `slot.md`, `roulette.md:26` name `_V1` circuits/files that are `_V2` (aggregated) |
| F-542 | `bearer_bond.md:210,228,242`, `:76` | `CoinAttributes`→`CommitmentAttributes`; missing `series_asset_id`; tree keyed by `commitment` not `token_commit`; missing `receipt_*` fields |
| F-543 | `multi_contract` trees | `multisig.md:208` fabricates an `info` tree; `identity.md:281` omits `identity_info`; `bearer_bond.md:451` wrong tree names; `bridge.md:80`+`deployooor.md:92` — the **manifest** omits `chain_events`/`singleton` while the doc and constants list them |
| F-544 | `bearer_bond.md:439`, `otc_swap.md:118`, `box.md` | capability tables list 6/4/… where the manifest declares 1/2 — doc and manifest disagree on the capability ABI |
| F-545 | `multi_contract` counts | `bearer_bond.md:407` "30 variants" (35); `otc_swap.md:239` "274 lines" (708); `promissory_note_intermediaries.md:432` opcode-consumer counts; `roulette.md:167` "circuits not yet built" (built) |
| F-546 | `recruitment_pipeline.md:201`, `provable_randomness.md:427` | `RegisterCapabilityV1` `0x09` (is `0x04`); `ContributeEntropyV1` `0x04` (is `0x09`) |
| F-547 | `dex.md:47,148`, `baccarat.md:73`, `slot.md:139` | `(BaseDiv)` annotations on circuits that use quotient-remainder; `baccarat`'s payout formula; `slot`'s "paytable lookup in ZK" (it instances `payout`) |
| F-548 | `audit.md:15`, `:22` | "14 FIXED / 3 PLANNED" contradicts `AUDIT.md` (all 17 FIXED); HTLC `claimed_at`/`refunded_at` absent |
| F-549 | `risk_market_ecosystem.md:115,263` | a proposed insurance selector table that doesn't match the shipped ABI; a `prediction_market` contract that doesn't exist |
| F-550 | `bridge.md`, `betting_stake.md` | `betting_stake`'s `Stake`/`TableStakeRegistry` omit fields; `bridge` value-commit validation is inline not the named helper |

### 3F — The `security-analysis.md`-style status pages inside contract docs

`audit.md` (`audit.md`) and the "MVP Status"/"Future Improvements" tables in `subscription.md`,
`lottery.md`, `roulette.md` mark as **Complete/Done** features that are removed or absent
(`subscription.md:345` DAO-treasury + bridge "✅ Complete"; `lottery.md:283` "Merkle tree" +
"tests" unchecked but present; `roulette.md:167` circuits "not yet built" but shipped). Aggregated as
**F-551** (H, ~6 sites).

### Phase 3 adjudication (refuter pass)

Five refuters over the 51 rows. **45 CONFIRMED · 5 PARTIAL · 1 REFUTED.**

**Refuted — do not action:**
- **F-517** — a duplicate of the existing `OBL-C204` row (which already owns the stablecoin
  `interest_accrued` finding).

**Partial — action with the correction:**
- **F-501** (aggregated) — overreaches on **escrow**, which *does* call `Purse::DepositV1` (in
  `FundV1`) and `Box::TakeV1` (in `ClaimV1`); only its Refund-Box claim is false. The other four
  (drain_protection, betting_stake, labor_market, relayer_endowment) hold.
- **F-527** (aggregated struct tables) — holds for 8 of the 9 contracts it names; overreaches on
  `oracle` and `bearer_bond`, whose documented structs carry no `version`/`instance_seed` in code.
- **F-541** (V1-vs-V2) — the `bearer_bond` and `promissory_note` sites hold; the `slot`/`roulette`
  sub-cites do not (those `V1` names are real enum variants, not stale circuit names).
- **F-544** (capability tables) — `bearer_bond` (6 vs 1) and `otc_swap` (4 vs 2) hold; the `box.md`
  sub-cite does not (box has no capability-discriminant table).
- **F-550** — the `betting_stake` half holds; the `bridge` half does not (bridge.md makes no
  value-commit claim).

**The 9 confirmed `CODE-VIOLATES` owned by no existing row** — the Phase-3 equivalent of the Phase-2
`FN-3xx` set: `F-511` (`LockV1` no signature — the refuter traced the whole tx-validity path),
`F-512` (`dex` 0x04 vs 0x05), `F-513` (unpayable `ForceSettle`), `F-514` (unread `capability_proof`),
`F-515` (no host `schnorr_verify`), `F-516` (models never stored), `F-518` (cancel timeout removed),
`F-519` (`audit.md`'s fix table), `F-520` (no governance key).

---

## Phase 4 — arch/dev + narrative tail (133 findings, unadjudicated)

Nine agents over the remaining arch docs, `dev/*`, `zkas/*`, `spec/*`, `crypto/*`, `dep/*`, and the
sampled narrative tail (`learn/`, `misc/`, `testnet/`, `relayer/`, `philosophy/`, `about/`, root docs).
Read-at `e32e4d1eb4`. **Not refuted.**

### 4A — A command, binary or module the doc tells the reader to use, that is not there

The largest family: `dev/` and `testnet/` operational docs, plus two whole-binary cases.

| ID | sev | doc | claim |
|---|---|---|---|
| F-601 | H | `localnet-dev.md:19,65,99` | `contrib/localnet/` config absent (first command can't run); the CLI reference lists ~15 wallet/contract subcommands the parser rejects; block reward "20 DRKW" (curve gives ~13.8) |
| F-602 | H | `dev/native-workflow.md:58,91` | `wallet import-secret` absent, and dwowd does not auto-generate a mining keypair ("no auto-generation") |
| F-603 | H | `testnet/token.md` | a whole `token`/`alias` interactive CLI that does not exist |
| F-604 | H | `testnet/node.md:338`, `bootstrapping.md:118` | `dwow_wallet interactive` (disabled) and `wallet keygen` (removed) |
| F-605 | H | `relayer/relayer.md`, `dev/bridge-node.md` | `universal_relayer` — commented out of the workspace (all stub code); the image cannot run its relayer |
| F-606 | H | `dev/bridge-node.md:170,200` | `contract invoke <bridge_id> get_config` (no such function); `/app/universal_relayer` |
| F-607 | H | `dev/quickstart.md:137` | a harness example using `create_token`/`mint`/`verify` — the API is `spawn`/`issue`/`transfer` |
| F-608 | M | `misc/vanityaddr.md`, `misc/network-troubleshooting.md` | `vanityaddr` (on hold, not a member); `node_get-info.py` (is `node_get_info.py`) |
| F-609 | M | `learn/…/writing-a-daemon.md:17` | `use dwow_util::…` — the crate is `dwow_core` (the tutorial's own example says so) |
| F-610 | M | `learn/…/using-dnet.md:40` | `python main.py` — no such file (the script is `dnet`) |
| F-611 | M | `misc/nodes/node-configurations.md:28`, `public-guide.md` | darkirc/taud default ports stale (9600/9800 vs 25551/23330) |

### 4B — A spec the code does not meet, or two specs that contradict (`CODE-VIOLATES`)

| ID | sev | doc | claim |
|---|---|---|---|
| F-612 | H | `dev/testing/heavyweight-spec.md:207` | §3.5 mandates an unconditional `FeeCollectV1`; the harness omits it — and the sibling normative `production-test-standard.md` says it *shall* omit it. Two normative docs, opposite rules |
| F-613 | H | `dev/testing/heavyweight-spec.md:454` | §5.4 requires 9 identity functions / `CreateClaimV1 (0x03)`, removed; the enum has 8 |
| F-614 | H | `zk-engineering-posture.md:69` | "no asterisks, no exceptions — every call path carries a ZK proof"; native_token's coinbase/fee/uncle-mint are plaintext, circuits deleted |
| F-615 | M | `arch/legacy/event_graph.md:114` | asserts a binary-level quarantine §10.4 says dwowd must never enable; `src/lib.rs:46` says dwowd intentionally enables it (doc/comment/manifest disagree) |
| F-616 | M | `dev/testing/heavyweight-spec.md:59,117` | oracle circuit count (5 vs §5.8's own 6); a gate cites `src/linear/src/block_acceptor.rs` (it is `bin/dwowd/…`) |

### 4C — A phantom module, type or symbol

| ID | doc | claim |
|---|---|---|
| F-617 | `arch/consensus/merge-mining-ffi.md` | the whole `dwow_mm_*` FFI layer — 8+ symbols nowhere in the tree |
| F-618 | `arch/monero-merge-mining.md:192`, `arch/services.md:44` | `CChainState::apply_block_with_uncles` and a `ManualSession.connect` — neither exists |
| F-619 | `arch/zk/spend_hook.md:29` | five PN circuits (`burn_v1.zk`, `mint_v1.zk`, …) — none exists |
| F-620 | `arch/economics-caveat-emptor.md:179` | `DeclareProfitsV1`/`ClaimProfitsV1` bearer_bond entrypoints — neither exists; whole scenarios built on them |
| F-621 | `arch/contract-metadata.md`, `contract-wasm-standards-best-practices.md` | `contract_interactions`/`transactions_history` tables (removed); `BoxRecord`, `Purse`, `PurseFunction` — removed/renamed |
| F-622 | `for-researchers.md:30` | the type `CoinCommitment` — the type is `Commitment` (a no-coins-rename survivor) |
| F-623 | `arch/contract-trust-model.md`, `dev/contracts/identity.md` | `manifest_verify.rs` / Layer-2 WASM verification "not implemented"; identity dev guide repeats `0x0b`/`CreateClaimV1`/3 circuits |

### 4D — Stale counts, constants and values

| ID | doc | claim |
|---|---|---|
| F-624 | `monero-merge-mining.md:32`, `consensus/stratum.md:179`, `merge-mining.md:22`, `localnet-dev.md:100` | the mining blob given as **227 / 260 / 292 / 43** across four docs; it is 292 (`OBL-C64`) |
| F-625 | `intro.md:72`, `README.md:13`, `for-researchers.md:13,48` | "32 zkVM opcodes" (31 usable); "26 Lean proof files" (81); "228 circuits" (dedup F-007); "5-level test pipeline" (4) |
| F-626 | `quantum-threat.md:104,171` | "120 contract circuits" (166) and "X25519 DH" note encryption (Pallas ECDH) |
| F-627 | `arch/network-types.md:15,72,12` | devnet difficulty `20000` / key `fixed-difficulty` (config: `pow_fixed_difficulty = 1`); default network devnet (is testnet); magic bytes "(none)" |
| F-628 | `dev/testing/*` | `.zk.bin` 175 (166); pipeline modules 18 (19); harness circuit counts (oracle 1→6, attestation 5→10 …); model test counts (34/34→41/41) |
| F-629 | `dev/intro.md`, `spec/crypto-schemes.md:192`, `crypto/*` | `⊕_b` formula has identical cases; `rln.md` describes v1 while embedding v2; `zk_explainer.md` written against **bellman** (the project uses halo2) |
| F-630 | `arch/circuit-versioning.md:56` | "Only V2 circuits exist on disk" — five unsuffixed circuits (`Put`/`Take`/`Deposit`/`Withdraw`/`Balance`) |

### 4E — Instrument gaps

| ID | sev | doc | claim |
|---|---|---|---|
| F-631 | H | `dev/contracts/checklist.md:3` | presents ~30 items as "the operative pre-commit gates"; only two classes are mechanically enforced (Exec/Apply phasing, clippy-unwrap). Nullifier-zero, temporal bounds, fail-closed, consensus-path rules have no gate |
| F-632 | M | 12 files | malformed markdown links — the closing bracket is not followed by an opening parenthesis, so they render as literal text; the targets exist. `check-doc-index.sh` does not flag link syntax |

### 4F — Registers and indexes pointing at the wrong state

| ID | doc | claim |
|---|---|---|
| F-633 | `arch/monero.md:232`, `merge-mining.md:144` | cite `OBL-C67` as "open"; the register records it CLOSED (residual split to `OBL-C80`) |
| F-634 | `arch/sync.md:3` vs `arch/README.md:51`, `SUMMARY.md:85` | sync.md self-declares SUPERSEDED; the indexes still list it as current with no redirect |
| F-635 | `arch/consensus/stratum.md:146` | presents `OBL-C68` as an active self-inflicted lock serialization; the code drops the lock before the network call (`OBL-C68` FIXED) |
| F-636 | `arch/consensus/transfer-spec.md:38,70` | cites a renamed Lean theorem; a caveat about a "legacy identifier" the model no longer has |
| F-637 | `arch/merge-mining.md:143` | a paragraph duplicated verbatim (lines 143-144 = 146-147) |

### Phase 4 adjudication (refuter pass)

Five refuters over the 37 rows. **33 CONFIRMED · 4 PARTIAL · 0 REFUTED.**

**Partial — action with the correction:**
- **F-631** (the checklist instrument-gap) — the "only two classes are enforced" universal is **false**: `run-all-tests.sh` wires ~90 gates and `hooks/pre-commit` blocks on two more, so several checklist items map to shipped gates. The valid residue is narrower — the four named rule classes (nullifier-zero, temporal bounds, fail-closed, consensus-path) have **no** gate — and the file has 48 checkboxes, not ~30.
- **F-633** (the `OBL-C67` citation) — `monero.md:232` does call it "open" (the register Status column says `CLOSED`, residual split to `OBL-C80`), but `merge-mining.md:144` merely cites it without the word "open"; and the register's **own body** says "OPEN." while its Status column says CLOSED — an internal conflict the row did not name.
- **F-614** (`zk-engineering-posture`) — the underlying code fact is already owned by `OBL-C134`; only the residual (this posture doc still makes the absolute claim, and it is not in that row's swept-file list) is new.
- **F-625** (`for-researchers` counts) — the "32 opcodes" sub-claim is a doc-vs-doc inconsistency (32 enum variants incl. the internal `Noop`; 31 usable), not a clean doc-vs-code error; the other three sub-claims hold.

**Corrections the pass forces anywhere (confirmed, but the row's own wording is off):** F-624 listed "292" among the stale blob values — 292 is the *correct* value (the docs say 260/260/227/43); F-632's count is **13 links in 11 files**, not "12 files", and the closing bracket is absent entirely; F-627's "default is testnet" holds for the config file though the CLI default is `darkwow-devnet`.

**The confirmed CODE-VIOLATES owned by no existing row:** `F-612` (two normative testing docs give opposite rules for `with_fee_collect`), `F-613` (heavyweight-spec §5.4 requires a removed identity function), `F-615` (the four-way event-graph quarantine disagreement).

---

## Cluster 1 — The register's prose, counts and instrument descriptions are stale

| ID | class | sev | doc | code | claim |
|---|---|---|---|---|---|
| F-101 | DOC-STALE | H | `verification-hazop.md:47` | `scripts/check-register-worklist.sh` | census stale in 5 of 6 figures: prose 57/26/10/12/4/31 vs derived 56/27/12/11/4/29; the same section says the counts "are not restated here" |
| F-102 | DOC-STALE | M | `verification-hazop.md:85` | `check-register-worklist.sh:122` | "thirty-one `DECISION`" vs the gate's 29 |
| F-103 | DOC-STALE | M | `verification-hazop.md:91` | `insurance_market/src`, `dao_escrow/src/model/mod.rs:659` | D1 lists `C166`/`C203` as pending deletions; both rows are FIXED and the code confirms the deletions |
| F-104 | DOC-STALE | M | `verification-hazop.md:98` | `verification-hazop.md:1445,1454` | D3 gives `C140`→ACCEPTED-WITH-REASON and `C147`→RESTATED; both rows carry DECISION |
| F-105 | DOC-STALE | H | `verification-hazop.md:1054` | `scripts/register-status.sh:160` | prose says the status check reads a row's **last** cell; the script was changed 2026-09-23 to read the **Status** column |
| F-106 | INSTRUMENT-GAP | H | `verification-hazop.md:1056` | `register-status.sh:157` | the *documented* negative control no longer fails — a coined marker in the last cell exits 0; the guard's real control (a marker in the status cell) is undocumented |
| F-107 | DOC-STALE | M | `verification-hazop.md:1059` | `register-status.sh:183` | "guards the form, not the existence" — `--check` now also fails a missing status; "the ten rows that still carry none" — report mode prints **0** |
| F-108 | DOC-STALE | M | `verification-hazop.md:1024` | `check-register-worklist.sh:11` | "runs it `--strict` against a declared budget" — there is no budget since 2026-10-08; the same doc contradicts this at :72 |
| F-109 | DOC-STALE | M | `verification-hazop.md:1042` | `verification-hazop.md:1126` | worked example cites `OBL-C44` as an open row at severity **H**; the row is CLOSED at severity **M** |
| F-110 | DOC-STALE | M | `verification-hazop.md:1919` | `verification-hazop.md:1068` | "item 5: every one of `C16`'s 72 guards fail-open"; the row says zero fail-open and is CLOSED (72/78/84 all disagree) |
| F-111 | AMBIGUOUS-SPEC | M | `verification-hazop.md:61` | `dev/contracts/safety.md:131` | the register reuses `RC1`–`RC6` with meanings that collide with safety.md's canonical `RC1`–`RC12`; not in the alias table; `check-doc-index.sh` check 6 cannot see bare `RC<n>` |
| F-134 | DOC-STALE | L | `verification-hazop.md:469` (Z1) | `script/circuit_instance_derivation.py` | stale figures: 880→899 instances, 200→218 bound, 223→224 redundant |

## Cluster 2 — Status cell disagrees with the row's own body

The `DECISION` token was added 2026-10-08; it re-labelled the status cells while the append-only body
text kept the old word. A reader trusting the body counts a different status than the gate does.

| ID | class | sev | doc | code | claim |
|---|---|---|---|---|---|
| F-112 | REGISTER-ROW-UNSOUND | M | `verification-hazop.md:474` (Z6) | `proofs/lean/.../HashOps.lean:296` | cell **DECISION**, body "The status stays `PARTLY`"; the Lean module calls it "partly" |
| F-113 | REGISTER-ROW-UNSOUND | M | `verification-hazop.md:1491` (C184) | same row's closing text | cell **DECISION**, closing sentence "Status unchanged at `OPEN`" |
| F-114 | REGISTER-ROW-UNSOUND | M | `verification-hazop.md` Z5 :475, Z13 :478, T20, T23 | — | the same cell/body divergence recurs; T23's body even says "The row is `OPEN` rather than `PARTLY`" |

## Cluster 3 — Register rows the code has moved past

| ID | class | sev | doc | code | claim |
|---|---|---|---|---|---|
| F-115 | DOC-STALE | M | `verification-hazop.md:1461` (C154) | `dao_escrow/src/model/mod.rs:359`, `entrypoint.rs:1612` | DECISION row asserts `mode` unsettable and `Approved` written by nobody; both are fixed, the fix sites cite `C154`/`C159` by name |
| F-116 | DOC-STALE | M | `verification-hazop.md:1416` (C97) | `drain_protection/src/entrypoint.rs:613,701` | residual names `propose`/`vote`/`execute` as unchecked; `propose` and `execute` now call `require_fund_authority` |
| F-117 | DOC-STALE | M | `verification-hazop.md:1438` (C136) | `src/net/tests.rs:306` | attributes the message-swallowing hook to the event-graph harness; it is `net`'s, and the payload loss it calls unfixable is fixed |
| F-118 | REGISTER-ROW-UNSOUND | M | `verification-hazop.md:1368` (C74) | `stablecoin/manifest.toml:1` | `PARTLY` over a 3-of-4-false compound (8/32 declare `required_barbs`, 8/32 `note_schema`), with no discussion cell |
| F-119 | DOC-STALE | M | `verification-hazop.md:1507` (C200) | `bearer_bond/src/error.rs:56,110,168` | gives `InterestOverflow` (code 9) the text "must be non-zero"; that text belongs to `InvalidInterestRate` (code 28) |
| F-120 | REGISTER-ROW-UNSOUND | M | `verification-hazop.md:497` (Z19) | `betting_stake/proof/claim.zk:42` | `CLOSED` on a **universal** still false: the nullifier and the tx binding share `witness_base(3)`; the gate reports it live |
| F-121 | DOC-STALE | M | `verification-hazop.md:495` (Z18) | `scripts/check-pubkey-binding.sh` | "carries all 57 findings" / control "exits 1 with all 57"; reality is **52** (57, 49 and 52 disagree across the row, the file header, and the run) |
| F-122 | DOC-STALE | M | `verification-hazop.md:503` (Z23) | `check-circuit-dead-values.sh` | "177 `.zk` files" and quoted coverage "177 of 177"; now **178** |
| F-123 | DOC-STALE | M | `verification-hazop.md:477` (Z12) | `check-circuit-instance-derivation.sh` | "880 `constrain_instance` sites"; now **899** (the same figure is stale in Z1) |
| F-124 | DOC-STALE | M | `verification-hazop.md:474` (Z6) | `sdk/src/crypto/constants/sinsemilla.rs:89` | cites `:89` for `ALTITUDE_BITS=10`; `:89` is a function, `K = 10` is at `:31` |
| F-125 | DOC-STALE | M | `verification-hazop.md:478` (Z13) | `stablecoin/src/entrypoint.rs:1737` | cited `:1363-1368` has moved ~370 lines (the proposition still holds) |
| F-126 | DOC-STALE | M | `verification-hazop.md:938` (T2), T21 | `Capability/Types.lean:278`, `fee_builder.rs:189` | stale line citations on `PROVED`/`DEFINITIONAL` rows |
| F-127 | DOC-STALE | H | `verification-hazop.md:470` (Z2) | `check-circuit-metadata-alignment.sh:37` | credits the gate with "11 contracts / 68 sites" — the very allowlist the gate records as its worst defect; it now covers 31 contracts / 162 |
| F-128 | INSTRUMENT-GAP | H | `verification-hazop.md:501` (Z22) | `scripts/check-circuit-fidelity.py:125` | claims a negative control; the checker ships no `--self-test` |
| F-129 | REGISTER-ROW-UNSOUND | M | `verification-hazop.md:497` (Z19) | — | the row's "Checked today by" cell is **empty** on a CLOSED row |
| F-130 | DOC-STALE | M | `verification-hazop.md:400` (C13) | `hooks/pre-commit:24` | credits `hooks/pre-commit`; the row's own text says the hook "never enforced this" |
| F-131 | DOC-STALE | M | `verification-hazop.md:471` (Z3) | `check-circuit-domain-separation.sh:9` | cell says the script "never [checks] the correct one"; it has a ROLE rule that does |
| F-132 | DOC-STALE | M | `verification-hazop.md:472` (Z4) | `hooks/pre-commit:20` | cell describes the removed line-anchored awk |
| F-133 | REGISTER-ROW-UNSOUND | M | `verification-hazop.md:393` (C6/C10/C11) | — | SATISFIED/CLOSED rows whose cell names a *category* ("Rust only") while their own text says no test and no control exist |

## Cluster 4 — The Lean HAZOP matrix presents fixed findings as open

`proofs/lean/src/DarkFi/HAZOP.lean` (the `riskMatrix`) and `HAZOP/{Critical,High,Elevated}.lean`. The
structural findings are F-135/F-136; the 19 stale findings are F-137–F-155, each of the form "the
matrix holds this at risk *n*; the named circuit now carries an explicit `HAZOP … FIX` guard."

| ID | class | sev | doc | code | claim |
|---|---|---|---|---|---|
| F-135 | INSTRUMENT-GAP | H | `HAZOP.lean:55`, `src/Main.lean:295` | `src/Main.lean:295` | `Main.lean` asserts the stale counts (4/5/6/7); correcting the matrix to drop fixed findings **fails the suite** — the guard pins the stale data |
| F-136 | DOC-STALE | M | `HAZOP.lean:7` | `src/contract/*/proof` | 14 of 14 distinctive `riskMatrix` paths fail to resolve (`stablecommitment/`, `*_v2.zk` — no such files); no gate checks the paths |
| F-137 | DOC-STALE | H | `HAZOP.lean:57` (CRIT-1) | `stablecoin/proof/governance_report.zk:82` | the free witnesses are now instanced; superseded (register `OBL-Z14`) |
| F-138 | DOC-STALE | H | `HAZOP/Critical.lean:152` (CRIT-2) | `stablecoin/proof/liquidate.zk:79` | "does NOT enforce undercollateralization" — the circuit carries `HAZOP CRIT-2 FIX` + the constraint |
| F-139 | DOC-STALE | H | `HAZOP.lean:61` (CRIT-3) | `bridge/proof/withdraw.zk:31` | recipient front-running — the nullifier is now bound to `recipient_hash` |
| F-140 | DOC-STALE | H | `HAZOP/Critical.lean:298` (CRIT-4) | `oracle/proof/aggregate.zk:163` | boundary checks "are NO-OP" — now `range_check`ed under `HAZOP CRIT-4 FIX` |
| F-141 | DOC-STALE | M | `HAZOP/High.lean:105` (HIGH-2) | `native_token/proof/burn.zk:83` | `zero_cond` unguarded — now `less_than_strict(ZERO, value)` |
| F-142 | DOC-STALE | M | `HAZOP/High.lean:194` (HIGH-4) | `labor_market/proof/milestone_payment.zk:21` | shared nullifier derivation — now distinct action tags (CONFIRM 4 / MILESTONE 2 / REFUND 7) |
| F-143 | DOC-STALE | M | `HAZOP/High.lean:125` (HIGH-3) | `labor_market/proof/refund.zk:1` | the described `cond_select` branch no longer exists (residual is `OBL-C96`) |
| F-144 | DOC-STALE | M | `HAZOP/High.lean:243` (HIGH-5) | `stablecoin/proof/governance_report.zk:156` | premise (free `total_debt`, 15000 floor) both gone (`OBL-C204`) |
| F-145 | DOC-STALE | M | `HAZOP.lean:67` (HIGH-1) | `promissory_note/proof/` | cites `burn_v2.zk`; PN has no burn circuit — the path never resolves |
| F-146 | DOC-STALE | M | `HAZOP/Elevated.lean:32` (ELEV-1) | `bridge/proof/deposit.zk:41` | describes `zero_cond` + a Merkle proof that are not there |
| F-147 | DOC-STALE | M | `HAZOP/Elevated.lean:77` (ELEV-2) | `dex/proof/cancel_swap.zk:51` | incompatible `swap_id` derivations — now identical |
| F-148 | DOC-STALE | M | `HAZOP/Elevated.lean:130` (ELEV-4) | `promissory_note/proof/redeem.zk:57` | asks for `constrain_equal_base(value, ZERO)` — the circuit carries it under `HAZOP ELEV-4 FIX` |
| F-149 | DOC-STALE | M | `HAZOP/Elevated.lean:167` (ELEV-5) | `dex/proof/execute_swap_slippage.zk:4` | "uses `base_div`" — no `base_div` remains; cross-multiplication |
| F-150 | DOC-STALE | M | `HAZOP/Elevated.lean:212` (ELEV-6) | `dex/proof/execute_swap.zk:78` | `bool_check` — none remains; the file is a rewritten circuit |
| F-151 | DOC-STALE | M | `HAZOP.lean:93` (MOD-1) | `dex/proof/execute_swap_fee.zk:6` | "field division for fees" — exact integer quotient-remainder (`OBL-Z13`) |
| F-152 | DOC-STALE | M | `HAZOP.lean:97` (MOD-3) | `stablecoin/proof/accrue_interest.zk:53` | free witnesses now range-checked (partly superseded) |
| F-153 | DOC-STALE | M | `HAZOP.lean:101` (MOD-5) | `tender/proof/reveal_bid.zk:73` | recorded twice, unlinked: matrix MOD-5 (risk 22) and a register ZK row (severity H) |
| F-154 | DOC-STALE | L | `HAZOP.lean:105` (MOD-7) | `dex/proof/create_swap.zk:64` | `NULLIFIER_K` for signature derivation — but that is the tree-wide generator; unclear it is a defect |
| F-155 | DOC-STALE | M | `HAZOP/Critical.lean:8`, `High.lean:8` | `HAZOP/Critical.lean:2` | banners say "NOT FORMAL PROOFS" while subtitles say "Formal Vulnerability Proofs" and entries are labelled `THEOREM` |

## Cluster 5 — The Lean assumption boundary (`check_lean_axioms.py`)

| ID | class | sev | doc | code | claim |
|---|---|---|---|---|---|
| F-156 | INSTRUMENT-GAP | H | `verification-hazop.md:110` | `script/check_lean_axioms.py:209` | `assumptions()` sees only an `axiom` carrying a `/-- … -/` doc block; an **undocumented** axiom in `Axioms.lean` is invisible to the inventory — so "a new assumption cannot be sneaked in" overstates the instrument |
| F-157 | INSTRUMENT-GAP | M | `verification-hazop.md:112` | `check_lean_axioms.py:207` | `assumptions()` reads raw text (no `strip_comments()`), so a comment-out deletion is not caught — the "absence is a failure" direction misses it |
| F-158 | INSTRUMENT-GAP | M | `verification-hazop.md:165` | `check_lean_axioms.py:512` | the only control is `--self-test`, which exercises `reconcile()` only; nothing can make `check_inventory` fail in CI (`R8`) |
| F-159 | INSTRUMENT-GAP | M | `verification-hazop.md:106` | `check_lean_axioms.py:169` | the location regex misses attributed declarations (`@[simp] axiom foo : Nat`), which also evade the inventory |
| F-160 | DOC-STALE | L | `verification-hazop.md:106` | `check_lean_axioms.py:46` | "the only file in `proofs/lean/`" — scope is `proofs/lean/src`; `.lake/packages/**` hold many axioms (the intended reading is probably fine, the wording is not) |

## Cluster 6 — `security-analysis.md`

| ID | class | sev | doc | code | claim |
|---|---|---|---|---|---|
| F-161 | DOC-STALE | H | `security-analysis.md:409,584` | `check-circuit-instance-derivation.sh` | "reports 4 unclassified, exiting 1"; today 0 unclassified, exit 0 |
| F-162 | DOC-STALE | H | `security-analysis.md:401,456` | `check-pubkey-binding.sh:229` | carries **two** contradictory figures (49 and 51) for the same checker; it prints 52 |
| F-163 | DOC-STALE | H | `security-analysis.md:206` | `dao_escrow/proof/pay_premium.zk:45` | a `max_membership_blocks = 52560` cap "FIXED" — the constant exists nowhere under `src/` |
| F-164 | DOC-STALE | H | `security-analysis.md:236` | `dex/src/` | quotes a `params.lock_proof` guard as a live partial fix; the path was removed (`d71447b096`) |
| F-165 | DOC-STALE | H | `security-analysis.md:219` | `dex/src/entrypoint/create_swap.rs:139` | "zeroed pubkeys for proposer and acceptor" — the proposer key is derived; only the acceptor stays zeroed |
| F-166 | DOC-STALE | H | `security-analysis.md:356` | `src/contract/*/proof` | a whole "Circuits Fixed" table names `_v1.zk` files that no longer exist (only 2 of 30 resolve) |
| F-167 | DOC-STALE | H | `security-analysis.md:450` | `bridge/src/lib.rs:106` | bridge anti-dust floor stated as `100_000_000`; the code is **100** |
| F-168 | AMBIGUOUS-SPEC | M | `security-analysis.md:283` vs `:450` | `bridge/src/entrypoint.rs:633` | the body presents Issue 12 as open with a Recommendation; the summary row says "FIXED" |
| F-169 | DOC-STALE | M | `security-analysis.md:163` | `escrow/proof/claim.zk:14` | quotes a comment "`<-- NOT VERIFIED`" as live; it now reads "CONTRACT ENFORCED" and the host enforces it |
| F-170 | DOC-STALE | H | `security-analysis.md:59` | `subscription/proof/verify_access.zk:36` | quotes tiered-permission circuit code that is absent (the circuit is a capability-hash scheme) |
| F-171 | DOC-STALE | H | `security-analysis.md:99` | `verify_access.zk` | cites `verify_access.zk:150-151` and quoted Merkle code in a 66-line file that has neither |
| F-172 | DOC-STALE | M | `security-analysis.md:256` | `dex/src/entrypoint/mod.rs:239` | "ZK verification returns empty / circuits never called" — the DEX is now a full contract |
| F-173 | DOC-STALE | M | `security-analysis.md:401` | `run-all-tests.sh:316` | a correction's own replacement line reference (`:200`) is itself stale (now `:316`) |
| F-174 | DOC-STALE | M | `security-analysis.md:456` | `verification-hazop.md:1393` | "fifteen genuine defects … scheduling its repair" — four of the five named rows are CLOSED |

## Cluster 7 — sync red-team audit + audit README

| ID | class | sev | doc | code | claim |
|---|---|---|---|---|---|
| F-175 | DOC-STALE | M | `arch/audit/README.md:19` | `src/linear/src/chain_state.rs:268` | the headline "two audits disagreed, the specific one won" is unsupported — the "refuted" items were real HAZOP findings a same-day commit fixed |
| F-176 | DOC-STALE | M | `sync-red-team-audit.md:70` | `sync-protocol.md:90` | attributes to §3 a requirement the clause does not contain ("a zero hash at height > 0 SHALL NOT round-trip") |
| F-177 | DOC-STALE | M | `sync-red-team-audit.md:139` | `bin/dww/src/sync_task.rs:59` | R3 "RESOLVED" — `HighestPeerTip` is still monotonic and never decays; the defect survives and gates `is_synced()` |
| F-178 | REGISTER-ROW-UNSOUND | L | `arch/audit/README.md:10` | `safety.md:1084` | "every legacy ID … mapped in the alias table" — the red-team `C-/H-/M-/L-/SC-` forms and the sync-HAZOP `R1–R5` are not mapped |
| F-179 | DOC-STALE | L | `sync-red-team-audit.md:142` | `sync-protocol.md:363` | R6 "RESOLVED (round-robin)" — §13.3 explicitly says there is no round-robin, and the code does not implement one |
| F-180 | DOC-STALE | L | `sync-red-team-audit.md:81` | `sync-protocol.md:97` | cites §5 for `MAX_BYTES`; it is §4 |
| F-181 | DOC-STALE | L | `sync-red-team-audit.md:61` | `sync-protocol.md:361` | cites §13.3 for "3 failures deprioritised"; the clause says a timeout peer "is simply skipped" |

## Cluster 8 — `safety.md` root-cause ledger

| ID | class | sev | doc | code | claim |
|---|---|---|---|---|---|
| F-182 | DOC-STALE | H | `safety.md:242` | `proofs/lean/README.md:206`, `CircuitIndex.lean:37` | RC2 says `Axioms.NoFreeInstances` still exists; it was deleted 2026-09-24 |
| F-183 | DOC-STALE | H | `safety.md:244` | `proofs/lean/src/DarkFi/Circuits/InstanceDerivation.lean:458` | "`Circuits/` contains no Lean declarations" — it holds theorems (`soundness`, …), plus Token/Bridge/Exchange structures |
| F-184 | DOC-STALE | M | `safety.md:449` | `src/linear/src/chain_state.rs:1167` | RC6's cited lines are both stale (1117→1167, 1340-1349→1380-1389); the structural claim holds |
| F-185 | DOC-STALE | M | `safety.md:306` | `auction/proof/create_auction.zk:6` | `4 = COIN_COMMIT` — no such constant; value 4 is `DOMAIN_COMMITMENT` |
| F-186 | DOC-STALE | M | `safety.md:598` | `drain_protection/src/model/mod.rs:253` | the "insecure `Default`" no longer holds; both fields are `Some(...)` |
| F-187 | DOC-STALE | M | `safety.md:638` | `src/contract/identity/src/lib.rs` | `IDENTITY_CONTRACT_MAX_DAG_CREDENTIALS` is defined nowhere in the repo |
| F-188 | DOC-STALE | M | `safety.md:744` | `src/error.rs:165` | a "naming convention" (`ComponentOperationFailed`) with zero instances |
| F-189 | CODE-VIOLATES-ITS-OWN-SPEC | M | `safety.md:748` | `bin/dwowd/src/rpc/miner.rs:139` | the doc resolves `safety.md C1` to a *tier*; code calling the token calls it a *finding* |
| F-190 | REGISTER-ROW-UNSOUND | H | `safety.md:1060` | `verification-hazop.md:61` | the register's `RC1`–`RC6` collide with safety.md's canonical `RC1`–`RC12`, with no alias or disambiguation (mirrors F-111) |
| F-191 | AMBIGUOUS-SPEC | M | `safety.md:1067` | `safety.md:507` | the alias table maps Lesson 3→RC2 and Lesson 21→RC12, but RC7's "Taught by" cites Lesson 3 and RC5's cites Lesson 21 |
| F-192 | DOC-STALE | M | `safety.md:14` | `safety.md:507` | "A finding is never recorded in both places" — RC7 records its findings as `OBL-C1/C3/C4/C5` (all SATISFIED) |
| F-193 | INSTRUMENT-GAP | M | `safety.md:302` | `src/contract/*/proof/*.zk` | the "grep returns nothing" recipe returns 42 lines (DOMAIN_* on the next line of multi-line calls) |
| F-194 | DOC-STALE | M | `safety.md:633` | `insurance_market/src/lib.rs:66` | "seven deactivation functions across four contracts" does not reconcile with the tree |
| F-195 | DOC-STALE | L | `safety.md:496` | `native_token/proof/mint.zk:46` | names `Mint_V1`; the circuit is `Mint_V2` |
| F-196 | CODE-VIOLATES-ITS-OWN-SPEC | M | `safety.md:160` | `drain_protection/src/client/mod.rs:327` | a `signature: pallas::Base::zero()` of the exact RC1 anti-pattern survives in production client code (also `lottery`) |
| F-197 | AMBIGUOUS-SPEC | M | `safety.md:1084` | `verification-hazop.md:9` | the alias table does not map the `V1`–`V7` or `C1` finding-ID schemes the register names as existing, while the register calls it "every legacy scheme" |

## Cluster 9 — Cross-cutting instrument gaps (`R8`)

| ID | class | sev | doc | code | claim |
|---|---|---|---|---|---|
| F-198 | INSTRUMENT-GAP | M | `verification-hazop.md:1431` (C130) | `scripts/check-wallet-kernel.sh` | the row names it as the falsifying instrument; it ships no `--self-test` and no planted-defect control |
| F-199 | INSTRUMENT-GAP | M | `check-register-artifacts.sh` header | same file | its header still says "line suffixes are stripped, and no symbol is checked", which its new `line_gone()` contradicts |
| F-200 | DOC-STALE | M | `verification-hazop.md:39` | — | the severity legend defines only C/H/M; 17 rows carry `L`, so a severity-partitioning instrument cannot bucket them (overlaps `OBL-C149`) |
| F-201 | DOC-STALE | M | `check-register-worklist.sh:7` | same file | the worklist script's own docstring repeats the stale census ("258 rows; 57 not settled") |

---

## Phase 0 — instrument findings (detectors)

| ID | class | sev | doc (claim) | code (reality) | status |
|---|---|---|---|---|---|
| F-001 | DOC-STALE | M | `contract/attestation.md:261` — "All 10 circuits" | `src/contract/attestation/proof/` holds **12** | proposed |
| F-002 | DOC-STALE | M | `contract/escrow.md:320` — "All 4 circuits"; table omits `cancel` | `src/contract/escrow/proof/` holds **5** | proposed |
| F-003 | DOC-STALE | M | `contract/game_room.md:214` — "All 5 circuits"; table lists 5 | `src/contract/game_room/proof/` holds **12**; omits 7 | proposed |
| F-004 | DOC-STALE | M | `contract/slot.md` — circuit table lists 2 | `src/contract/slot/proof/` holds **3**; `reveal_spin` omitted | proposed |
| F-005 | DOC-STALE | M | `contract/stablecoin.md:193` — "All 9 circuits"; table lists 9 | `src/contract/stablecoin/proof/` holds **10**; `update_config` omitted | proposed |
| F-006 | DOC-STALE | M | `arch/verification-hazop.md` — Surface 2's "Inventory: *N* circuits" | the paragraph carried **four** counts (the total, circuits under `src/contract/*/proof/`, the contract-directory pair, and the `proofs/core/` count) and every one had drifted by one; the corrected text no longer states any of them, because `scripts/check-doc-canonical-counts.sh` derives all four from the tree (independently confirmed by F-110/F-127/F-136) | proposed |
| F-007 | DOC-STALE | M | `for-researchers.md` — the "*N* ZK circuits" bullet | the bullet's figure was nearly 40% high against `src/contract/*/proof/*.zk`; the line now gives the contract-only count and the total separately | proposed |

## Coverage observations (not findings)

- **No circuit table.** Fourteen contract docs have a `proof/` directory but no circuit table:
  `baccarat`, `bearer_bond`, `betting_stake`, `darkbet_exchange`, `darktoshi_dice`, `drain_protection`,
  `insurance_market`, `lottery`, `native_token`, `pool_stake`, `relayer_endowment`, `roulette`,
  `subscription`, `tender`.
- **Incomplete function tables.** Six docs list fewer functions than the manifest declares:
  `bearer_bond` (9/10), `bridge` (1/3), `game_room` (11/12, missing `create_pot`), `labor_market` (3/15,
  diagram-documented), `native_token` (6/8), `stablecoin` (10/12).

## Instruments built (Phase 0)

| Detector | Pins | Commit |
|---|---|---|
| `scripts/check-contract-doc-circuits.sh` | a contract doc's circuit table + "All N circuits" ↔ `proof/*.zk` | `6ee5fbc447` |
| `scripts/check-contract-doc-functions.sh` | a doc's function/opcode table ↔ `manifest.toml [[functions]]` | `ae6e869ccb` |
| `scripts/check-doc-canonical-counts.sh` (+ `script/doc_count_exceptions.txt`) | the tree-population counts ↔ the tree | `7b2b2d6dbd` |

## What is not established (R7)

- **Symbol and mechanism claims have no sound gate.** A doc claim that a *symbol* or a *mechanism* is X
  cannot be mechanically checked (names lie — `R6`); those rows are verified by a two-sided read and are
  recorded as this exercise's named residue rather than a silent gap.
- **Line-number drift** is pervasive in the register and was not filed row-by-row (rows whose cited line
  moved but whose code still supports the proposition) — the `OBL-Z5` class.
- A blanket "every *N* circuits" scanner was **built and rejected as unsound** (Phase 0): the count gate
  fires only on global phrasings; the residual class is left to the agent pass.
- Three rows were **refuted** in adjudication (`struck`) and 29 are PARTIAL (each with its correction
  recorded in the phase's adjudication subsection).
- No gate, test, `cargo` command or Lean build was executed by any review agent (`R5`); every review
  verdict is from reading code, manifests, scripts and git history. A remediation fix is the first place
  a run is quoted.

## Promotion log (remediation)

Remediation moves each confirmed finding to its home and records the stamp in the block below;
`scripts/check-ledger-promotion.sh` reads it. Grammar — a comma-separated list per row:
`doc:<path>#<anchor>@<commit>` · `row:OBL-<id>` · `gate:<script>` · `struck:REFUTED:<id>` ·
`partial:<the correction applied>`.

<!-- promotion-log -->
refuted: F-154 F-190 F-517
F-511: row:OBL-C213@6820bb91fe, doc:doc/src/contract/deployooor.md#lockv1-0x01@298f47436c
<!-- F-511 is promoted whole. The `doc:` half landed with the `row:` rather than waiting for E5,
     because the session that found the code defect had the section open: it claimed a signature
     check, a single-field `LockParamsV1` and a `LockCallBuilder` that does not exist, and it carried
     a `## No ZK Circuits` heading whose text named a signature as the mechanism. Leaving that
     standing while the code changed underneath it would have been the same drift this exercise is
     about. -->
F-301: doc:doc/src/arch/consensus/fee-spec.md#141-genesis-initialization@52686077c5, row:OBL-C216
<!-- F-301's clause is not where the ledger's row points. The row cites `fee-spec.md:1826`, which is
     §13.2's bullet list, while the finding ID it carries is `FI-GEN-1` — a clause in §14.1. Both
     statements existed, and had drifted from the code and from each other. The promotion therefore
     names the home (§14.1) and the drifted copy (§13.2) is collapsed into a pointer at it. The
     obligation the old wording carried — that the economic coefficients be adjustable at a window
     boundary — is covered by nothing, and `OBL-C216` says so in the open rather than dropping it. -->
F-302: row:OBL-C218@264894db96, doc:doc/src/arch/consensus/fee-spec.md#1281-admission-gate-per-transaction@264894db96
<!-- F-302's clause contradicted itself. §12.8.1 carried "no gas re-derivation" while FI-PLAIN-2, which
     this finding is about, requires a non-matching fee to be rejected; the row quotes both. Both are
     corrected in the commit the row names: the pseudocode now shows the re-derivation, and the rule is
     stated as a floor rather than an equality test. -->
F-303: row:OBL-C217@706a3220ef, row:OBL-C219@39be345363
<!-- F-303 is promoted into two rows rather than one, because it turned out to be three defects with two
     different homes. `OBL-C219` owns the tracker's inputs being two kinds of quantity — it escalated
     every FeeV3-carrying contract to MAX from a comparison that meant nothing — and is FIXED.
     `OBL-C217` owns where the factor comes from and who may read it, and is PARTLY: the chain now
     derives it, and the wallet can never be a party to it, which `FI-RISK-1` was restated to say. -->
F-512: doc:doc/src/contract/dex.md#pn-opcodes-used@8cfb6f6b78, row:OBL-C220@8cfb6f6b78
<!-- F-512's clause is one the code was right about. The finding is a doc-vs-code disagreement, and the
     resolution turned on a detail neither side had stated: the entrypoints require two children, and
     `OtcSwapV1` is itself bilateral, so the documented opcode could not have produced the shape the
     code enforces. The promotion therefore corrects three declarations — the doc's table and prose,
     three entrypoints' comments, and the client's field comments — and leaves the code untouched.
     One residual is recorded rather than renamed: the client's `*_otc_func_id` fields. -->
F-513: row:OBL-C221@3ce8fde500, doc:doc/src/contract/relayer_endowment.md#the-logs-writer@3ce8fde500
<!-- F-513 is promoted as a repair, because the endpoint's subject was reachable and payable in
     principle — only its input was unwritable. The promotion therefore adds the writer rather than
     deleting the endpoint, and both the row and the document state what the writer does not fix:
     it is the relayer's own claim, so it protects a relayer that is late and not one that evades.
     The writer also deliberately leaves `last_settlement_height` alone, or it would suppress the
     very timeout it exists to satisfy. -->
F-311: doc:doc/src/arch/contract-wasm-type-system.md#a47-acl-enforcement@1b79e768c9, row:OBL-C222@1b79e768c9
<!-- F-311 is promoted as a restatement, because the code's deviation was deliberate, justified at the
     function, and the only one of fifteen host functions in its module. The promotion does not weaken
     the rule: it names the exception inside the rule, which is where a reader of the specification
     will look. Applying it would have meant a full rebuild of all 32 contract artifacts (the guest
     declaration lives in `src/sdk`) for a return value the guest discards. -->
F-308: row:OBL-C223@f94e6c117e
<!-- F-308's clause is about a gate that rejects a transaction whose fee cannot be read, and the row it
     is promoted to is about the gate not being reached at all. The framing was narrow: the trait's
     non-optional return is real, but the defect that matters is that `is_fee_v3` reads only the first
     call, which is the main call in every transaction the wallet builds. Triaging this also forced a
     correction to `OBL-C218`, whose fix sits behind the same predicate and had been marked FIXED. -->
F-309: doc:doc/src/arch/contract-wasm-type-system.md#a74-re-entrancy@57715ed6da, row:OBL-C224@57715ed6da
<!-- F-309 is promoted as a restatement, and the promotion's work was to establish which of two things was
     wrong: a missing check, or a clause written for a different execution model. It is the second — the
     host registers no import that creates a call, so the graph is static and the recursion the clause
     fears cannot be built. The restatement keeps the rule, says what covers it, and disclaims a
     similarly-named constant so a later reader does not mistake it for the check. -->
F-314: doc:doc/src/arch/consensus/consensus.md#implementation-status@8f8c7069b8, row:OBL-C225@8f8c7069b8
<!-- F-314 is promoted as a restatement, and it is the clearest case in the ledger of a capability table
     asserting an instrument that does not exist: a row marked [IMPLEMENTED] whose cited site does not
     mention the field. The promotion therefore corrects the row itself, not only the prose, and states
     what the obligation's absence means — with the reason it is not repaired here (population moves the
     PoW preimage, so it is a deliberate consensus change and the plan's out-of-scope default applies). -->
F-305: row:OBL-C226@96ac17e28e
<!-- F-305 is promoted as a repair, and the clause required no amendment: `type-system.md` §8.6.4
     specified the type and its call sites accurately, and the code simply lacked them. Its sentence
     naming the `ProtocolGenericHandler` routing was, in the event, the work list. -->
F-306: doc:doc/src/arch/type-system.md#865-sessionselector--nominal-session-topology@7fe6390b09, row:OBL-C227@7fe6390b09
<!-- F-306 is the second clause in this campaign whose *type* was impossible rather than merely absent: an
     enum cannot hold a bitmask, and the values here are OR-sums. The promotion therefore restates the
     clause and implements the corrected one, and records the part of the section that is specified but
     has no consumer (the `ExhibitsBarb` impl) instead of dropping it. -->
F-519: doc:doc/src/contract/audit.md#key-fixes-applied@20d308c7c1, row:OBL-C228@20d308c7c1
<!-- F-519 is promoted as a correction to a *security* document, which is the class at its most
     dangerous: six of the nine protections it advertised have no identifier anywhere in the tree, and
     a seventh was deliberately removed on 2026-09-24. The promotion therefore does not merely correct
     a figure — it names each absence where the claim was made, and records that nothing covers the
     obligation the table created. The ledger rated this H for exactly that reason. -->
F-196: row:OBL-C229@e9e0d2567a
<!-- F-196 is promoted as a repair with no doc half: `safety.md`'s RC1 bullet states the anti-pattern
     class and its checklist asks the exact question this field answered wrongly, so the document was
     right and only the code was wrong. The promotion's cost is unusual for this ledger and is recorded
     in the row: removing an unread field from six params is an ABI change (32 bytes each), and because
     the Python SDK bindings mirror those params under `src/sdk`, all 32 artifacts rebuilt behind it. -->
F-518: doc:doc/src/contract/otc_swap.md#cancel_swapzk@4808b9b824, row:OBL-C230@4808b9b824
<!-- F-518 is promoted as a documentation correction with no code half: the circuit had already dropped
     the constraint, and dropped it for a good reason recorded at the site. The promotion's work was to
     check the enforcement it claimed to have moved actually exists — it does, in the host, against the
     stored swap and the chain height — so the correction names that site instead of leaving a reader to
     infer that the timelock is unenforced. -->
F-514: row:OBL-C232@3faf7a4e6a, doc:doc/src/contract/insurance_market.md#o-cap-enabled-functions-0x09-0x0f@3faf7a4e6a
<!-- F-514 is the ledger's most understated row and is promoted as a *security* repair. It was recorded
     as an unread field; measured, the field was the symptom of an endpoint that resolved claims from the
     caller's own `is_valid` and `payout_amount` with no authorization of any kind, which is why the
     register row carries C where the ledger carried M. The promotion names the removal and what the
     endpoint's return needs, so that the feature is not simply gone. -->
F-515: doc:doc/src/contract/dex.md#signature-verification@3faf7a4e6a, row:OBL-C234@3faf7a4e6a
<!-- F-515 is promoted as a restatement of a section that contradicted itself: it claimed a host
     `schnorr_verify` and, in the same breath, that no such opcode exists. The circuit half is real and is
     the whole binding, so the correction names the mechanism rather than the missing half. -->
F-516: doc:doc/src/contract/stablecoin.md#configurable-models@3faf7a4e6a, row:OBL-C231@3faf7a4e6a
<!-- F-516 is promoted as a restatement: the contract accepts a model and ignores it, so the document's
     "the deployer selects the model" was the falsifiable claim. The table now separates the one
     implemented model from the three intended ones, and the row says out loud that building them is a
     feature rather than a repair. -->
F-520: doc:doc/src/contract/dex.md#contract-functions@3faf7a4e6a, row:OBL-C233@3faf7a4e6a
<!-- F-520 is promoted with a code half as well as a doc half: the constant the row leaned on was dead in
     the tree — written and read nowhere — so it is removed rather than left as the explanation of a
     registration that does not happen. -->
F-189: row:OBL-C235@cf09d131a3, doc:doc/src/dev/contracts/safety.md#rc12--an-error-dropped-panicked-on-or-made-indistinguishable@cf09d131a3
<!-- F-189 is promoted as a repair plus a correction. The repair retires the last production `.expect()`
     the finding `C1` names; the correction is to the *citation*, which used a finding as a licence.
     Promoting it also forced a re-measurement of OBL-C194's census, which is RED — twelve survivors in
     three contracts, wired into the umbrella — and was red before this change: a gate I had not been
     running, and the reason the row now says so. -->
F-310: doc:doc/src/arch/contract-wasm-type-system.md#a12-function-dispatch@0df2e754b0, row:OBL-C236@0df2e754b0
<!-- F-310 is promoted as a gate that now exists where the clause asked for an attribute. It found three
     unreachable arms on its first run — and the third by a *different* tool than the first two, because
     `cargo check --workspace` had reported clean without ever compiling that crate. The promotion records
     that, and it also records a regression of mine the same run exposed: the F-514 removal broke
     `dwow-sdk-py`, which that row's own verification had missed. -->
F-315: doc:doc/src/arch/consensus/consensus.md#implications@d0ae4f9b1a
<!-- F-315 is a documentation correction with no code half: the guard is Caribina-only by construction,
     and OBL-C64 already records that the anchor fields "are no longer consulted for finality at all".
     The correction is to the *scope* the bullet claimed — "or Monero" — not to the guard. -->
F-316: doc:doc/src/arch/consensus-coinbase.md#13-block-header@d0ae4f9b1a
<!-- F-316 is an internal self-contradiction: section 1.3 documented `total_reward` as the emission
     schedule's value while section 6, 900 lines later, gave the rule the code enforces. The correction
     names section 6 as the home. -->
F-317: doc:doc/src/arch/consensus-coinbase.md#26-wasm-entrypoint-verification@d0ae4f9b1a
<!-- F-317 is the most consequential of the five and still a doc correction: the check was documented as
     a lower bound and the code enforces equality, because the lower bound was the over-minting bug. The
     direction of the error is what made it worth correcting rather than leaving — a reader implementing
     against the doc would reintroduce the bug. -->
F-318: doc:doc/src/arch/consensus/uncle_merkle.md#security-considerations@d0ae4f9b1a
<!-- F-318 is a self-contradiction again: the security list's difficulty rule named the referencing
     block's target while the code's contract forbids it and the same document's section 2 agrees with
     the code. -->
F-319: doc:doc/src/arch/consensus/consensus.md#flag-bits@d0ae4f9b1a
<!-- F-319 is an offset with no serializer behind it, contradicted by the paragraph directly beneath the
     claim. Correcting it also removes the invitation to look for a byte the mining blob does not
     contain. -->
F-321: doc:doc/src/arch/type-system.md#114-authorization-inversion-type-level@3075defa7f
<!-- F-321 is promoted as a doc-only correction to an overclaim: the theorem closes the carrier half of the
     pattern and the Lean names the provenance half as open (OBL-T7). No code and no Lean changed — the
     Lean was already the careful one. -->
F-322: doc:doc/src/arch/ocap.md#23-construction-example-tender-bid@3075defa7f
<!-- F-322 is promoted as a labelling rather than a rewrite. The example and the Lean's type genuinely
     differ, and deciding which is the tender bid's real shape is a modelling question, so the example
     keeps its content and gains the fact that it is a sketch. -->
F-320: doc:doc/src/arch/wallet.md#62-barb-cover-input-selection@595eb7b0eb, row:OBL-C126
<!-- F-320 is a duplicate of an already-open row, and the ledger said so itself: OBL-C126 was filed
     2026-09-24 against the same sentence. The promotion therefore names the existing row rather than
     creating a second one, and the row records the defect's one-row home. The ledger's own citation was
     also wrong — privacy.md:701 for a sentence in wallet.md — which its notes already correct. -->
F-324: doc:doc/src/arch/wallet.md#013-interface-contracts@595eb7b0eb
<!-- F-324 is a doc-only correction: the interface bullet named a method `ProverContext` does not have,
     and the two types its signature used are real and do different work. -->
F-325: doc:doc/src/arch/wallet.md#64-one-bespoke-citizen-write-side@595eb7b0eb
<!-- F-325 is a doc-only correction, and the phantom name is instructive: `build_fee_and_finalize_tx`
     survives in comments in `bin/dww`, so the document inherited it from the code rather than inventing
     it. Correcting the document leaves the comments; they are a smaller version of the same drift and
     are noted rather than swept here, since sweeping them is an src edit with artifact consequences. -->
F-326: doc:doc/src/arch/wallet.md#014-module-map-per-component@595eb7b0eb, doc:doc/src/arch/wallet-vs-daemon.md
<!-- F-326 is a doc correction plus a row: the module named in two listings does not exist *and* the check
     it describes is performed nowhere, so this is an absence to record (OBL-C237) rather than a rename. -->
F-327: doc:doc/src/arch/manifest.md#3-scanning@595eb7b0eb
<!-- F-327 is a citation correction: three references to a file that does not exist, repointed at the one
     that holds `scan_block_linear`. -->
F-328: doc:doc/src/arch/genesis.md#adding-a-new-genesis-contract@595eb7b0eb
<!-- F-328 is a checklist correction whose old instruction would have added a second writer of state the
     genesis block already carries. -->
F-335: doc:doc/src/arch/consensus/stratum.md#292-byte-mining-blob-layout@37c8565a05
<!-- F-335 is promoted at the layout table rather than at the two documents the row names, because the
     table is the home and the copies are downstream of it: the blob has been 292 bytes since
     `anchor_owner` was appended (bytes 260..292), and the table's total said 260 and omitted the row
     entirely. The row's own value — "a client built from the spec never meets target" — is the reason
     this is corrected rather than annotated. -->
F-624: doc:doc/src/arch/consensus/stratum.md#292-byte-mining-blob-layout@37c8565a05
<!-- F-624's four docs and F-335's two are one claim with seven homes; the sweep is the promotion. One
     of the four (localnet-dev.md's "43 bytes = [2 byte padding][40 byte header]") described neither the
     blob nor anything else and is replaced by a pointer at the layout table. -->
F-329: doc:doc/src/arch/sync-conformance.md#net-crate-ownership-141@37c8565a05, gate:contrib/ci/check_sync_conformance.sh@37c8565a05
<!-- F-329's clause half was the row's smaller half. The finding's real content is the parenthetical —
     the gate PASSes — and it passed because it greps for the header's presence and never resolves the
     number. The gate now resolves every `§N` (ranges expanded) against sync-protocol.md's own headings,
     and gained a second control that plants a dead clause; the first control only proved it could notice
     an absent header. -->
F-330: partial:the module headers of sync_types.rs, chain_state.rs and p2p_wallet.rs re-pointed at the clauses that exist, and sync_types.rs's invariant list renumbered to the document it cites@37c8565a05
<!-- F-330 is the same defect in code comments, and it forced the correction of a detail the row did not
     name: sync_types.rs's header numbered its own invariants §1..§4 against a document where those
     subjects are §2/§3/§4/§5, and the range it cited (`§2-§7`) carried the deleted §6 inside it. -->
F-626: doc:doc/src/arch/quantum-threat.md#retroactive-privacy-protection@37c8565a05
<!-- F-626's X25519 half was a live claim in four places, not two: the primitive table, the HNDL
     paragraph, the migration timeline and `for-researchers.md`. Note encryption is Pallas DH
     (`sapling_ka_agree`/`kdf_sapling`), which matters because the exposure is not a separate break —
     it is the same curve the commitments rest on, so it falls with them. The circuit count is 167. -->
F-627: doc:doc/src/arch/network-types.md#darkwow-devnet-formerly-linear-testnet@37c8565a05
<!-- F-627 is four wrong cells in one table plus a config-file claim. The ledger's own note is the
     instructive part: "default is testnet" holds for the config file while the CLI default is devnet,
     so both are now stated. Same shape as F-329 — a document quoting a file that moved. -->
F-637: doc:doc/src/arch/merge-mining.md#finality-configuration@37c8565a05
F-633: doc:doc/src/arch/monero.md#see-also@37c8565a05
<!-- F-633 and F-637 are promoted together because they are the same paragraph's two defects: the
     monero finality note was duplicated verbatim in merge-mining.md, and both copies plus monero.md
     cited `OBL-C67` as open. It was re-closed when its residual split out to `OBL-C80`, so the
     citations now name `OBL-C80` and its actual status — ACCEPTED-WITH-REASON, not open. -->
F-501: doc:doc/src/contract/drain_protection.md#composition@2d09ea7313, doc:doc/src/contract/betting_stake.md#composition@2d09ea7313, doc:doc/src/contract/labor_market.md#composition@2d09ea7313, doc:doc/src/contract/relayer_endowment.md#composition@2d09ea7313, doc:doc/src/contract/escrow.md#composition@2d09ea7313
<!-- F-501 is the ledger's own 3A header for the family, and measuring it is why the row is promoted per-document
     rather than as one claim: "none call Purse" is false for escrow, whose `FundV1` requires a Purse child and
     validates its contract id — the ledger's parenthetical "(all use promissory_note::transfer_v1 0x04)" holds
     for the four that do not, and not for escrow, drain_protection's multisig child or labor_market's two
     attestation children. The four false docs keep the composition they were written for, marked as not
     implemented, since the intent is real and the present tense was the defect. -->
F-502: doc:doc/src/contract/auction.md#composition-with-promissorynote@2d09ea7313
<!-- F-502's promotion is the one case where the doc's intended composition was a different contract rather than a
     missing feature: the auction re-implements no escrow because it calls no escrow, and the note that it
     "composes with escrow" was a design note that outlived the design. -->
F-503: doc:doc/src/contract/box.md#composing-contracts@2d09ea7313
F-505: doc:doc/src/contract/tender.md#breaking-changes-v2@2d09ea7313
F-507: doc:doc/src/contract/insurance_market.md#integration-with-other-contracts@2d09ea7313
F-509: doc:doc/src/contract/game_room.md#stake-lifecycle@2d09ea7313
<!-- F-503/F-505/F-507/F-509 are promoted together as one method applied four times — read the entrypoint's
     `children_indexes` checks, name the contract whose id the child is validated against, and write that. The
     gate that guards the table side of these docs (`check-contract-doc-functions.sh`) also needed a correction
     here: it was firing on `insurance_market`'s row for the endpoint F-514 removed, so a retirement row is now
     skipped — a row that says a function is gone makes no claim about the manifest. -->
F-504: doc:doc/src/contract/identity.md#the-o-cap-universe--what-identity-unlocks@a392209ec8
<!-- F-504 is the sharpest instance of the family, because two of its three bad rows were not stale claims about
     a contract but a contract that is not there: `tau` has no `src/contract/` directory at all, and the
     `task_role` capability it was listed for exists in no entrypoint. The four `dao_escrow` rows were a
     vocabulary rather than calls, which is why they are recorded here rather than deleted silently. -->
F-506: doc:doc/src/contract/subscription.md#circuits@a392209ec8
F-508: doc:doc/src/contract/promissory_note_intermediaries.md#bridge--cross-chain-token-mover@a392209ec8
F-510: doc:doc/src/contract/pool_stake.md#token-movement@a392209ec8
<!-- F-506/F-508/F-510 are one method again — read the circuit's `constrain_instance` lines or the entrypoint's
     `children_indexes` checks, then write those. F-508 is the one that reverses the row's direction: the bridge
     does not use TransferV1, it mints with `issue_v1` and burns with `redeem_v1`, so the doc's "does not use
     RedeemV1" was exactly backwards. -->
F-528: doc:doc/src/contract/game_room.md#room-state@2d7cf04507
F-529: doc:doc/src/contract/attestation.md#claim-state@2d7cf04507
F-530: doc:doc/src/contract/auction.md#bid-state@2d7cf04507
F-531: doc:doc/src/contract/tender.md#bid-state-machine@2d7cf04507
F-532: doc:doc/src/contract/darktoshi_dice.md#state-machine@2d7cf04507
F-533: doc:doc/src/contract/darkbet_exchange.md#state-machines@2d7cf04507
F-534: doc:doc/src/contract/lottery.md#state-machine@2d7cf04507
F-535: doc:doc/src/contract/labor_market.md#state-machine@2d7cf04507
<!-- The 3D family is one method eight times, and it splits cleanly into two kinds. F-528/F-529/F-531 (and part
     of F-533) are *declared and unreachable* — the enum has the variant, the diagram draws the box, and no
     handler writes it; a reader cannot tell from either side that it is dead. F-530/F-532/F-534 (and darkbet's
     three `None` start states) are states that do not exist at all. F-533 is narrower than the ledger's
     wording: `Resolved` and `Settled` *are* assigned, so the correction names which half of the market enum is
     live rather than marking all of it dead. F-535 is the one wrong in both directions — Cancel accepts less
     than drawn, Dispute and Refund accept more. -->
F-522: doc:doc/src/contract/composability.md#promissory-note-child-call-amount-validation@7665e7d754
F-523: doc:doc/src/contract/composability.md#promissory-note-child-call-amount-validation@7665e7d754
F-524: doc:doc/src/contract/game_room_app_layer.md#sdk-usage@7665e7d754
F-525: doc:doc/src/contract/drain_protection.md#deployer-control@7665e7d754
F-526: doc:doc/src/contract/promissory_note.md#the-breakthrough-capability-not-access-control@7665e7d754
<!-- F-522/F-523 are one section describing a mechanism that is the opposite of the real one: the doc taught
     public value fields and a commitment-comparison helper, and the helper IS a commitment comparison — so the
     correction is smaller than a rewrite and the reason the amount is private is now stated. F-526 is the
     family's phantom *name* rather than a phantom symbol: the opcode is real (`IsNotEqualBase = 0x62,
     "is_not_equal"`) and simply unused by `redeem.zk`, which enforces `value == 0` twice by other means. The
     two source-comment occurrences in `promissory_note` are recorded in this comment and not swept, because
     PromissoryNote is a genesis contract and a comment edit moves the pin. -->
F-536: doc:doc/src/contract/composability.md#current-cross-contract-call-map@5c53e27b1b, doc:doc/src/contract/recruitment_pipeline.md#how-four-contracts-compose-to-automate-hiring@5c53e27b1b
<!-- F-536 is the family's most consequential error and its smallest: one byte. `0x0b` is not
     `VerifyCapabilityV1`; it is a dao_escrow opcode that no longer exists, so the doc's wrong byte and the
     doc's retired-selector table are the *same* mistake — a client reading either would publish a call the
     contract refuses. Chasing it also found that the composite-transaction example was a fabricated
     two-child shape no entrypoint accepts. -->
F-537: doc:doc/src/contract/oracle.md#zk-circuits@5c53e27b1b
<!-- F-537 is the doc being wrong on both sides of a variable: the value (4 vs 8) and the reason for it (the
     capability-commitment slot, when the code deliberately gives the operator commitment its own domain,
     `OBL-Z19`). The client's own comment at the call already said "not 4", so this was a three-way
     disagreement the document lost. -->
F-538: doc:doc/src/contract/promissory_note.md#zk-circuits@6d94948c87
F-539: doc:doc/src/contract/promissory_note.md#transferv1-0x04@6d94948c87, doc:doc/src/contract/otc_swap.md#zk-circuits@6d94948c87, doc:doc/src/contract/escrow.md#zk-circuits@6d94948c87
F-540: doc:doc/src/contract/dao_escrow.md#zk-circuits@6d94948c87
<!-- F-538/F-539/F-540 are one claim — the trailing `tx_binding`/`tx_nonce` pair is the *position* the node
     reads — and the fix is the same edit in three documents. F-540 is the one with a gate beside it:
     `check-circuit-metadata-alignment.sh` promotes exactly this position to a hard FAIL, comparing the client
     against the circuit rather than against a convention, and it is green. Its scope is circuit↔client; the doc
     is outside it, which is where three pre-`OBL-C198` rows survived. Recorded here as residue rather than
     built as a gate: reading an order out of a prose table is the symbol-mapping class that cannot be checked
     soundly (see the R7 section). -->
F-541: doc:doc/src/contract/bearer_bond.md#zk-circuits@b354827990, doc:doc/src/contract/promissory_note.md#files@b354827990, doc:doc/src/dev/contracts/journey.md#3-write-the-manifest@b354827990
F-542: doc:doc/src/contract/bearer_bond.md#data-model@b354827990
F-544: doc:doc/src/contract/bearer_bond.md#capability-types@b354827990, doc:doc/src/contract/otc_swap.md#implementation-status@b354827990
<!-- F-541 is promoted wider than the row names, and the sweep is the promotion: the `_v1` suffix had one more
     home than the ledger listed (`contract-wasm-standards-best-practices.md` cited a `commit_bet_v1.rs` that
     `circuit-versioning.md` §7.1 says should not exist and does not), and the bearer_bond table's `_V1` names
     came with a second wrong column — "Reused from PN" — that reading the proof directory falsified. F-542 is
     the ledger's most valuable row in this family: the doc described the *broken* form, including the sentence
     the code's own comment quotes when explaining why it could not work. F-544 is the one where the correction
     is a distinction rather than a fix — the six (and four) discriminants are real, and what disagrees with the
     manifest is a different vocabulary from the one the doc implies. -->
F-545: doc:doc/src/contract/promissory_note_intermediaries.md#contract-inventory@29d879b87f, doc:doc/src/contract/bearer_bond.md#files@29d879b87f, doc:doc/src/contract/roulette.md#future-improvements@29d879b87f
F-546: doc:doc/src/contract/recruitment_pipeline.md#how-four-contracts-compose-to-automate-hiring@29d879b87f, doc:doc/src/contract/provable_randomness.md#contracts-using-block-hash-entropy@29d879b87f
<!-- F-545's most useful cell is the one that measured 0 and is 1: the ecosystem table said no contract uses
     `IssueV1`, in the same document whose bridge section F-508 had just corrected *to* `issue_v1`. Two sections
     of one file disagreeing, which is the case a reader cannot resolve without the code. F-546 is the one-byte
     class again, and its shape is worth naming: the two documents published each other's value — 0x09 where
     0x04 belongs and 0x04 where 0x09 does — so neither is a copy of a stale source, and the swap is only
     visible with both open. -->
F-527: doc:doc/src/contract/game_room.md#playeraccount@e7bf3821e1, doc:doc/src/contract/relayer_endowment.md#data-model@e7bf3821e1, doc:doc/src/contract/pool_stake.md#data-model@e7bf3821e1, doc:doc/src/contract/roulette.md#data-model@e7bf3821e1, doc:doc/src/contract/darkbet_exchange.md#data-model@e7bf3821e1, doc:doc/src/contract/auction.md#data-structures@e7bf3821e1, doc:doc/src/contract/attestation.md#core-concepts@e7bf3821e1
<!-- F-527 is the campaign's largest sweep and the one where the method mattered most: fifteen structs in seven
     documents were diffed *field by field against the code* rather than read, which is how the row's own two
     errors surfaced — bearer_bond and oracle are named by it and carry no `version`/`instance_seed` in the
     structs they document (oracle's are Params), so those two sub-cites are wrong. Sweeping into them anyway
     found *different* defects the row did not name: bearer_bond's five missing `receipt_*` fields (`OBL-C199`)
     and oracle's three params carrying a `tx_binding` that `OBL-C198` removed. The comparison is over field
     names only; it does not check types, and a code struct with no doc snippet is outside it. -->
F-543: doc:doc/src/contract/multisig.md#database-trees@9f698f3a9e, doc:doc/src/contract/identity.md#database-trees@9f698f3a9e, doc:doc/src/contract/bearer_bond.md#database-trees@9f698f3a9e, doc:doc/src/contract/bridge.md#4-trees@9f698f3a9e, doc:doc/src/contract/deployooor.md#database-trees@9f698f3a9e
<!-- F-543 is two findings wearing one number, and the split is the point. Three docs were simply wrong about
     their own trees (a fabricated `info`, an omitted `identity_info`, three prose names in place of keys) and are
     corrected. The other two are the *manifest* being incomplete about trees the code really has — `chain_events`
     and `singleton` — and those are recorded, not repaired: a contract's `manifest.toml` is in its `SOURCE_MANIFEST`,
     deployooor is genesis, so the repair costs an artifact rebuild and a pin re-roll for a list no gate and no code
     reads. Naming the gap is the honest disposition; spending a pin on it would not be. -->
F-547: doc:doc/src/contract/dex.md#contract-functions@9f698f3a9e, doc:doc/src/contract/baccarat.md#payout-formula@9f698f3a9e, doc:doc/src/contract/slot.md#zk-proof-structure@9f698f3a9e
<!-- F-547's `slot` half is the strongest of the three because the ledger's own wording ("it instances `payout`") is
     exactly the distinction the doc erased: instancing a value publishes it, and the doc claimed the circuit
     *verified* a paytable lookup, ranged the reels and applied the house edge. Reading the circuit shows all three
     are the host's work and none is in the proof. -->
F-549: doc:doc/src/contract/risk_market_ecosystem.md#existing-contracts-to-extend@d246f02eec
F-551: doc:doc/src/contract/subscription.md#mvp-status@d246f02eec, doc:doc/src/contract/lottery.md#status@d246f02eec, doc:doc/src/contract/roulette.md#future-improvements@d246f02eec
<!-- F-551 is promoted across three documents, two of which are already stamped for other rows: `roulette`'s
     unchecked "ZK circuits for place_bet and settle_bet" was corrected with F-545 and is the same defect, and
     `subscription`'s table is F-506's document seen from the status side rather than the circuit side. What the
     class shares is that none of it is a *description* defect — a status page asserts present state, so every
     wrong cell is a claim that something is done (or not) rather than a stale number. `lottery` is the one where
     the misreport costs work: two of three unchecked boxes describe shipped things, and one of them is the
     ticket Merkle tree the contract's model rests on. -->
F-548: doc:doc/src/contract/audit.md#resolution-status@3593851e33
F-550: doc:doc/src/contract/betting_stake.md#data-model@3593851e33
<!-- F-548 and F-550 are stamped late, in the commit after their corrections landed, because reading them as
     "handled" while working them is exactly the failure the promotion gate exists to catch. F-548 is two records
     of one audit disagreeing about their own counts; F-550 is the per-contract instance of F-527's class.
     F-521 is a **pointer**, not a finding — its own text says "see Phase 2" — so it carries no stamp; the three
     rows it points at (F-324/F-325/F-326) are promoted. -->
F-161: doc:doc/src/arch/security-analysis.md#issue-13-no-formal-verification@d4429a08fb
F-162: doc:doc/src/arch/security-analysis.md#issue-19-missing-public-key-constraint-major--fixed-in-circuits-below@d4429a08fb
F-163: doc:doc/src/arch/security-analysis.md#issue-11-membership-expiry-is-witness-not-verified-moderate--fixed@d4429a08fb
F-164: doc:doc/src/arch/security-analysis.md#issue-17-lock_proof-partially-verified-critical--the-path-is-gone@d4429a08fb
F-165: doc:doc/src/arch/security-analysis.md#issue-16-public-keys-hardcoded-to-zero-critical--corrected@d4429a08fb
F-166: doc:doc/src/arch/security-analysis.md#issue-19-missing-public-key-constraint-major--fixed-in-circuits-below@d4429a08fb
F-167: doc:doc/src/arch/security-analysis.md#summary-table@d4429a08fb
F-168: doc:doc/src/arch/security-analysis.md#issue-12-weak-range-check-on-amount-moderate--fixed-and-not-in-the-circuit@d4429a08fb, doc:doc/src/arch/security-analysis.md#summary-table@d4429a08fb
F-169: doc:doc/src/arch/security-analysis.md#issue-8-no-state-verification-on-claim-major--the-quoted-comment-is-a-correction-not-a-defect@d4429a08fb
F-170: doc:doc/src/arch/security-analysis.md#issue-2-permission-bitmask-checking-is-absent-major--fixed-capability-hash@d4429a08fb
F-171: doc:doc/src/arch/security-analysis.md#issue-3-no-cancellation-nullifier-verification-moderate--provisional-fix@d4429a08fb
F-172: doc:doc/src/arch/security-analysis.md#issue-18-zk-proof-verification-returns-empty-critical--no-longer-true@d4429a08fb
F-173: doc:doc/src/arch/security-analysis.md#issue-19-missing-public-key-constraint-major--fixed-in-circuits-below@d4429a08fb
F-174: doc:doc/src/arch/security-analysis.md#summary-table@d4429a08fb
<!-- The security-analysis rows are promoted per section rather than per document because the correction is the
     same edit fourteen times: this file is an audit written in May 2026 and corrected piecemeal since, and
     several of its *corrections* had gone stale — which is the failure a "Correction (date)" banner invites.
     F-174 is the clearest: a correction claiming "fifteen named defects open" whose own five citations are four
     CLOSED rows and one DECISION. F-161/F-162/F-173 are the checker-count class, now given as names because
     every number in them had drifted twice. F-163 is a cap presented as applied that is in fact a prover-chosen
     witness. F-164/F-165/F-172 describe a DEX that has since become a full contract. F-166 named thirty `_v1.zk`
     files, two of which resolved. -->
F-175: doc:doc/src/arch/audit/README.md#what-the-corpus-is-worth-remembering@26e6e83228
F-176: doc:doc/src/arch/sync-red-team-audit.md#r4-high--tip_hash-serves-a-zero-hash-with-a-real-height@26e6e83228
F-177: doc:doc/src/arch/sync-red-team-audit.md#r3-high--highestpeertip-is-monotonic-and-never-decays@26e6e83228, row:OBL-C238@26e6e83228
F-178: doc:doc/src/arch/audit/README.md#what-the-corpus-is-worth-remembering@26e6e83228
F-179: doc:doc/src/arch/sync-red-team-audit.md#resolution-status@26e6e83228
F-180: doc:doc/src/arch/sync-red-team-audit.md#r5-medium--two-server-implementations-with-divergent-byte-budgets@26e6e83228
F-181: doc:doc/src/arch/sync-red-team-audit.md#r3-high--highestpeertip-is-monotonic-and-never-decays@26e6e83228
<!-- F-177 is the one row in E6 that is promoted twice — to the document *and* to a new register row — and the
     reason is the E6 decision rule doing its work: this finding is live, the audit's own table recorded it as
     RESOLVED, and no row owned it. Deletion of the audit would have lost it, so the audit is corrected in place
     (the rule's other branch) and the defect is given `OBL-C238` with a non-empty check cell. F-175/F-178 are
     the README's two framing claims, both wider than the record: a "the more specific audit won" story that
     three same-day fixes falsify, and an alias-table scope sentence that does not match the table. -->
F-006: doc:doc/src/arch/verification-hazop.md#surface-2--zk-circuits-obl-z@fdd853deae, gate:scripts/check-doc-canonical-counts.sh@fdd853deae
F-007: doc:doc/src/for-researchers.md@fdd853deae, gate:scripts/check-doc-canonical-counts.sh@fdd853deae
<!-- F-006/F-007 are the two rows whose *claim cells in this ledger* quoted the stale figures, and each also
     carried a stale correction of its own ("the gate walks 178", "holds 166 (178 walked)"). Both cells are
     rewritten to describe the defect without quoting a number, which is the Phase F trap taken early: a row
     that restates a drifting figure becomes the next stale source the moment it is read as current. The
     instrument that now owns all four figures is `check-doc-canonical-counts.sh`, which is GREEN as of these
     stamps — the gate Phase F was going to flip. -->
F-625: doc:doc/src/intro.md@f5efdf4f52, doc:doc/src/for-researchers.md@f5efdf4f52
<!-- F-625 is four claims in three root documents: the opcode count (32 declared, 31 usable — `noop` computes
     nothing), "26 Lean proof files" against 81, the circuit count (deduplicated into F-007), and a "5-level
     test pipeline" where the tree has four testing levels. -->
F-628: doc:doc/src/dev/testing/level-2-heavyweight.md#contract-harness-list@670cc41948, doc:doc/src/dev/testing/python-simulations.md#python-consensus-models@670cc41948, doc:doc/src/dev/testing/overview.md@670cc41948
F-629: doc:doc/src/spec/crypto-schemes.md@f5efdf4f52, doc:doc/src/crypto/rln.md@f5efdf4f52, doc:doc/src/crypto/zk_explainer.md@f5efdf4f52
F-630: doc:doc/src/arch/circuit-versioning.md#migration-scope@f5efdf4f52
F-632: gate:scripts/check-doc-index.sh@f5efdf4f52
F-634: doc:doc/src/arch/README.md@f5efdf4f52, doc:doc/src/SUMMARY.md@f5efdf4f52
F-635: doc:doc/src/arch/consensus/stratum.md@f5efdf4f52
F-636: doc:doc/src/arch/consensus/transfer-spec.md@f5efdf4f52
<!-- F-632 is promoted against the *gate* rather than a document, and that is the outcome the plan's Phase B
     aimed at: the thirteen malformed links are gone, and `check-doc-index.sh` now carries the link-syntax
     check written for them — its malformed-link arm is what proves the class cannot return. F-628's sweep is
     far wider than its row: the row cites two harness counts and **every one of the twenty-two rows** in the
     table it points at was wrong. F-629's `⊕_b` is the sharpest single defect in E7: two identical cases, so
     the defining formula said the function ignores its parameter. -->
F-601: doc:doc/src/localnet-dev.md#mining-details@3bfed30427, doc:doc/src/localnet-dev.md#dwow_wallet-cli-wallet@3bfed30427
F-602: doc:doc/src/localnet-dev.md#dwow_wallet-cli-wallet@3bfed30427
<!-- F-601/F-602 are the 4A family's densest instance and its archetype: a guide whose first command cannot run
     (the config path does not exist), whose reward figure is wrong at both sites, and which lists ten CLI
     subcommands that are not variants of either enum. F-602's `wallet import-secret` is one of the ten, so the
     two rows are one document and one method — read the enum, run the command. -->
F-603: doc:doc/src/testnet/token.md@8b94a73747
F-604: doc:doc/src/testnet/node.md#wallet-sync@8b94a73747, doc:doc/src/testnet/bootstrapping.md#wallet-setup@8b94a73747, doc:doc/src/testnet/contract.md@8b94a73747, doc:doc/src/arch/formal-specification.md@8b94a73747
F-605: doc:doc/src/relayer/relayer.md@8b94a73747, doc:doc/src/intro.md@8b94a73747, doc:doc/src/dev/bridge-node.md@8b94a73747
F-606: doc:doc/src/dev/bridge-node.md@8b94a73747
F-607: doc:doc/src/dev/quickstart.md@8b94a73747
F-608: doc:doc/src/misc/vanityaddr.md@8b94a73747, doc:doc/src/misc/network-troubleshooting.md@8b94a73747
F-609: doc:doc/src/learn/dchat/deployment/writing-a-daemon.md@8b94a73747
F-610: doc:doc/src/learn/dchat/network-tools/using-dnet.md@8b94a73747
F-611: doc:doc/src/misc/nodes/node-configurations.md@8b94a73747
<!-- F-605 and F-608 are promoted as *banners rather than deletions*, and the distinction is the plan's E6 rule
     applied where it fits: neither document is an audit whose open items are already rows, both are the only
     written form of a design (the relayer's o-cap withdrawal model; vanityaddr's option list), and in each case
     the program is absent from the tree rather than superseded. F-603 is the one where the whole document was
     the defect — 216 lines of an interactive shell over a token model DarkWow does not have — and it is
     rewritten rather than bannered because its subject (DRKW: getting it, checking it, sending it) is real.
     F-604 sweeps two phantom commands across four documents; F-611's `fud` row is left marked **unverified**
     rather than guessed at, since `bin/fud/` holds no config to read. -->
F-612: doc:doc/src/dev/testing/heavyweight-spec.md#72-heavyweightpipeline@173ef382ab
F-613: doc:doc/src/dev/testing/heavyweight-spec.md#54-o-cap-authorization--identity@173ef382ab
F-614: doc:doc/src/arch/zk-engineering-posture.md@173ef382ab
F-615: doc:doc/src/arch/legacy/event_graph.md@173ef382ab
F-616: doc:doc/src/dev/testing/heavyweight-spec.md#heavyweight-block@173ef382ab
<!-- F-612's row was already half false when read: §3.5's convention paragraph had been corrected by an earlier
     remediation and cites `OBL-C117`, so the "two normative docs, opposite rules" framing no longer described the
     page. What still contradicted both the code and §3.5 was PR-2 fifty lines below — the same document, a
     different clause, which is why the row is promoted to the pipeline section rather than to §3.5.
     F-614 is E7's most consequential: a posture document asserting the absence of exceptions, in a tree whose
     manifest lists four. F-615 is the same shape from the other side — a prohibition the code's own comment
     records as having been wrong about what it forbade. -->
F-617: doc:doc/src/arch/consensus/merge-mining-ffi.md@8cb4f41e99
F-618: doc:doc/src/dwowd.md@8cb4f41e99, doc:doc/src/arch/monero-merge-mining.md@8cb4f41e99, doc:doc/src/arch/services.md@8cb4f41e99
F-619: doc:doc/src/arch/zk/spend_hook.md@8cb4f41e99
F-620: doc:doc/src/arch/economics-caveat-emptor.md@8cb4f41e99
F-621: doc:doc/src/arch/contract-metadata.md@8cb4f41e99, doc:doc/src/arch/contract-wasm-standards-best-practices.md@8cb4f41e99
F-622: doc:doc/src/for-researchers.md@8cb4f41e99
F-623: doc:doc/src/arch/contract-trust-model.md#layer-2-wasm-verification@8cb4f41e99, doc:doc/src/dev/contracts/identity.md@8cb4f41e99
<!-- F-619 is the family's cleanest instance: five circuit filenames that have never existed, in a table whose
     *claim* — all five instance `coin_spend_hook` — is true of the real five. That is the shape most of 4C
     shares: the assertion survives, the names in it do not. F-620 is the one where a whole scenario was built on
     absent endpoints, and it is rewritten onto the real path rather than struck, because the economic concern it
     raises is sharper against `ProveCoverageV1` than against the fiction. F-623's identity half repeats F-536's
     three defects in a second file — the same mistake copied, which is why the sweep rule exists. F-618's
     `ManualSession` example is the one a reader would copy and then fail to compile. -->
F-001: doc:doc/src/contract/attestation.md#zk-proof-structure@9a59b8c880
F-002: doc:doc/src/contract/escrow.md#zk-circuits@9a59b8c880
F-003: doc:doc/src/contract/game_room.md#zk-circuits@9a59b8c880
F-004: doc:doc/src/contract/slot.md@9a59b8c880
F-005: doc:doc/src/contract/stablecoin.md#zk-circuits@9a59b8c880
<!-- F-001 to F-005 are the contract docs' instance of F-628's class and were found only at the end of E7, by
     asking what the *same* comparison would say about a different family of tables: the harness table's counts
     were wrong against the tree, and so are the contracts' own circuit tables. Four of the five turned out to be
     omissions rather than totals — game_room was missing seven of its twelve — which is the more expensive
     direction, since a reader using those tables to decide what is proven would conclude that raise, call and
     fold have no circuit. Noted as a coverage lesson rather than a stray: the review grouped these under its own
     heading and I worked the tranches by document family, so a class spanning families is the kind that survives
     a tranche boundary. -->
F-103: doc:doc/src/arch/verification-hazop.md#the-root-causes--the-open-findings-reduce-to-six@160399609c
F-104: doc:doc/src/arch/verification-hazop.md#the-root-causes--the-open-findings-reduce-to-six@160399609c
F-105: doc:doc/src/arch/verification-hazop.md@160399609c, partial:scripts/register-status.sh usage header corrected to the Status cell@160399609c
F-106: partial:the documented control was verified against the script and holds; only the usage header was stale@160399609c
F-107: partial:the prose was already correct — `--check` does fail a missing status; only the usage header was stale@160399609c
F-109: doc:doc/src/arch/verification-hazop.md@160399609c
F-110: doc:doc/src/arch/verification-hazop.md@160399609c
<!-- Cluster 1's first slice, and the shape is instructive: F-106 and F-107 were *already fixed* — the prose named
     the Status column, documented the real control, and claimed both form and existence — and what still said
     otherwise was the script's own usage header, which no document gate reads and which the review's citer
     could not see. F-110 is the register's cleanest self-refuting figure: item 5 corrects the number 72 by
     giving its own 78, and 78 has since become 84, so the bullet now states the argument (a moving figure
     quoted without its date) and no figure. F-103/F-104 are decided policies read as pending work. -->
F-111: doc:doc/src/arch/verification-hazop.md@45ffd0570a
F-112: doc:doc/src/arch/verification-hazop.md@45ffd0570a
F-113: doc:doc/src/arch/verification-hazop.md@45ffd0570a
F-114: doc:doc/src/arch/verification-hazop.md@45ffd0570a
<!-- F-111 was fixed by the CAUSE rename (Phase C) and needed only a stamp. F-112/F-113/F-114 are one defect
     counted three ways — the 2026-10-08 `DECISION` relabel left append-only bodies saying `PARTLY`/`OPEN` — and
     four of the five sites were corrected on 2026-10-11. The fifth, `OBL-Z13`, was not, and finding it is the
     value of stamping rather than assuming: a sweep reported over five sites that reached four. -->
F-115: doc:doc/src/arch/verification-hazop.md@4209139246
F-116: doc:doc/src/arch/verification-hazop.md@4209139246
F-117: doc:doc/src/arch/verification-hazop.md@4209139246
F-118: doc:doc/src/arch/verification-hazop.md@4209139246
F-119: doc:doc/src/arch/verification-hazop.md@4209139246
F-120: doc:doc/src/arch/verification-hazop.md@4209139246
F-121: doc:doc/src/arch/verification-hazop.md@4209139246
F-122: doc:doc/src/arch/verification-hazop.md@4209139246
F-123: doc:doc/src/arch/verification-hazop.md@4209139246
F-124: doc:doc/src/arch/verification-hazop.md@4209139246
F-125: doc:doc/src/arch/verification-hazop.md@4209139246
F-126: doc:doc/src/arch/verification-hazop.md@4209139246
F-127: doc:doc/src/arch/verification-hazop.md@4209139246
F-128: doc:doc/src/arch/verification-hazop.md@4209139246
F-129: doc:doc/src/arch/verification-hazop.md@4209139246
F-130: doc:doc/src/arch/verification-hazop.md@4209139246
F-131: doc:doc/src/arch/verification-hazop.md@4209139246
F-132: doc:doc/src/arch/verification-hazop.md@4209139246
F-133: doc:doc/src/arch/verification-hazop.md@4209139246
F-134: doc:doc/src/arch/verification-hazop.md@4209139246
<!-- Twenty rows stamped together, and the honest note is *how* they were verified: the register already carries
     a `docs-review F-11x` append for every one of them, so what was checked is the *current text* rather than
     the presence of a note — five spot-reads (Z2, Z18, Z19, Z13, Z23) plus a live gate run for the collision
     F-120 is about. One of the five was stale by one: Z23's own correction of a moving figure had itself moved,
     which is recorded in `4209139246` and is this register's oldest failure mode arriving inside a fix for it.
     F-129 is dispatched rather than repaired — the misplaced severity cell is `OBL-C149`'s class and that row is
     OPEN — and that is the point of stamping it here: the finding has a home. -->
<!-- /promotion-log -->
