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
| F-006 | DOC-STALE | M | `arch/verification-hazop.md:461` — "**Inventory: 180 circuits.**" | the gate walks **178**; same file prints 178 at :820 (independently confirmed by F-110/F-127/F-136) | proposed |
| F-007 | DOC-STALE | M | `for-researchers.md:14` — "**228 ZK circuits**" | `src/contract/*/proof/*.zk` holds **166** (178 walked) | proposed |

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
<!-- /promotion-log -->
