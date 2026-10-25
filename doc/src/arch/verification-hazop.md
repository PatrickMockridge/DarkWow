# Verification defect register — a tracker, not a task

This file tracks what is broken so it gets fixed. A change to it is worth nothing unless the code
changed in the same commit (for `CLOSED`) or the batch approval covers it (for a residual). The
normative rules are [`hazop-rules.md`](../dev/hazop-rules.md) §"Definition of closed out" and
[`risk-management.md`](risk-management.md).

**A finding is decided or it is not.** `OPEN`, `BLOCKED` and `DEFERRED` are **undecided**: while any
exists, no verification surface — a Docker run, a pipeline pass, an umbrella sweep, a release
verdict — may run. `CLOSED` (the defect is impossible in current code, with a failing-before
control) and `RESIDUAL` (an accepted risk with an ALARP record) are **decided**. There is no third
exit and no override; `scripts/check-tracker-settled.sh` is the preflight that refuses every
verification surface. A targeted control run directly by the fix in progress is not a verdict.

**Row shape.** `ID | Status | Defect | Locus | Control | Risk`. The main table uses `OPEN`,
`BLOCKED`, `DEFERRED` and `CLOSED`; any other status is a schema error, never silently dropped.
`RESIDUAL`, `MERGED` and `DECISION` are tombstones in the Settled-IDs appendix: a residual's record
is in [`residual-risk.md`](residual-risk.md), a decision's is in [`decisions.md`](decisions.md).
`Risk` is `S/L` with `S ∈ {C,H,M,L}` and `L ∈ {D,P,T}`; the band is computed from
[`risk-management.md`](risk-management.md) §4, never written by hand. An `OPEN` row carries a
resolving control or `plan:<text>`; `—` is forbidden. A row is capped at 600 characters.

**Closure is code.** `scripts/check-defect-register.sh` refuses a `CLOSED` row whose control does
not resolve, whose script control has no `--self-test`, or whose control is the locus itself; refuses
a closure commit that changes only docs, reports, `scripts/`, `hooks/` or `contrib/ci/`; refuses an
ID that disappears without a tombstone; refuses an ID live in the tracker and in `decisions.md` or
`residual-risk.md`; refuses an unknown status; requires every `OPEN` row to be in a live fix cycle;
and verifies the approval's `scope_sha256` over the acceptance boundary and its
`appendix_baseline_sha256` over the frozen historical tombstones. `CLOSED` still means impossible in
current code with a failing-before control (the nine conditions in `hazop-rules.md`).

**Counts.** 47 decisions and interpretations live in [`decisions.md`](decisions.md); the gate checks
this number.

## Fix cycles

Every `OPEN` row is in exactly one cycle. A cycle is a commitment by its owner to end every listed
row as `CLOSED` (code + failing-before control) or `RESIDUAL` (signed ALARP record) by its bound. A
lapsed bound fails the register gate; it is not re-dated silently.

| Cycle | Owner | Bound | Rows |
|---|---|---|---|
| codec | project lead | by:2026-11-04 | OBL-C246 |

| ID | Status | Defect | Locus | Control | Risk |
|---|---|---|---|---|---|
| OBL-C102 | CLOSED | A protected fund's payout was inert bookkeeping: `initialize` stored `total_funds: 0` and no members, and `ExitV1` paid a share of that zero from a caller-supplied weight. The exit arm now refuses with `CustodyModelUnavailable` before any host call or state read; the contract is governance-only (OBL-C271) and the spec states it. Run: `exit_is_refused_before_any_state_read ... ok`; twin `initialize_is_not_refused_by_the_exit_guard ... ok`. | `src/contract/drain_protection/src/entrypoint.rs:process_instruction` | `exit_is_refused_before_any_state_read` | C/P |
| OBL-C109 | CLOSED | The Python maturity model keyed by commitment while the code's rule keys by nullifier, and the model claimed a mirror that did not exist. It now has `nullifier_set`, `track_nullifier` and `is_nullifier_mature` mirroring `check_coinbase_maturity` (unknown nullifiers pass; recorded ones need COINBASE_MATURITY), and the commitment helper is renamed to say it is not the consensus rule. Run: `test_coinbase_maturity_is_nullifier_keyed: PASSED` (16/16 model tests). | `contrib/model/chain_model.py:is_nullifier_mature` | `contrib/model/chain_model_test.py` | H/P |
| OBL-C130 | CLOSED | The wallet selected a transfer capability by asset id alone. Selection now filters by asset id, requires the matches to share one `ContractId` (naming the contracts and refusing when they do not), prefers the `transfer` capability, and refuses differing capability identities (§6.4.1). Run: `transfer_selection_picks_the_single_contract`, `..._refuses_two_contracts`, `..._refuses_mixed_capability_kinds`, `..._refuses_no_match` all `ok`. | `bin/dww/src/dispatch.rs:select_transfer_capability` | `transfer_selection_refuses_two_contracts` | H/P |
| OBL-C136 | CLOSED | Seven fixed 5-second sleeps made the event-graph DAG tests fail under load with no stated cause. `bootstrap_nodes` now polls `P2p::peers_count` and the propagation waits poll a non-panicking DAG predicate (`dag.len` plus identical unreferenced tips), each with a named bound (30s; 300s for the ignored 100k test); `assert_dags` reports the cause. Run: `event_graph::tests::eventgraph_propagation ... ok` (500s). | `src/event_graph/tests.rs:bootstrap_nodes` | `eventgraph_propagation` | L/D |
| OBL-C160 | CLOSED | A vote's capability secret was a field of the public `CapabilityProof`, and the host derived the vote nullifier from it. The secret now lives only in the `VoteClaimV2` witness map; the params carry the derived `vote_nullifier`, which the circuit instances and the host records. An old-format frame carrying the secret is refused. Run: `test_capability_proof_frame_has_no_secret_slot ... ok`; `test_vote_claim_params_round_trip ... ok`. | `src/contract/dao_escrow/src/model/mod.rs:CapabilityProof` | `test_capability_proof_frame_has_no_secret_slot` | H/P |
| OBL-C202 | CLOSED | The bearer_bond round-trip covered two arms (issue, burn) while the contract accepts more. It now exercises every param struct the dispatch accepts: issue, transfer, request/pay interest, unstake, emergency unstake, burn and register series, including the unclaimed-nullifier sentinel. Run: `test_bearer_bond_encode_roundtrip ... ok`. | `src/contract/test-harness/tests/encode_roundtrip.rs` | `test_bearer_bond_encode_roundtrip` | M/D |
| OBL-C205 | CLOSED | The burn-arm public-input vector is hand-written in both `burn_get_metadata` and the client's `BurnRevealed::to_vec`; the chain-level `BurnV1` endpoint is the prove+verify control (client proof against the contract's metadata, plus the nullifier state check). Run: `test_heavyweight_native_token ... ok` — 1 passed, 877.44s, including the PI-7 two-chain comparison. | `src/contract/native_token/src/entrypoint/mod.rs:burn_get_metadata` | `test_heavyweight_native_token` | M/D |
| OBL-C214 | CLOSED | The wallet registry and `manifest.toml` were never compared. A unit test parses every registered contract's manifest and compares function names, codes, `requires_proof` and single-proof circuits in both directions, with two declared exceptions (native_token::mint is refused by every arm; deployooor has no manifest). Run: `contract_metadata::tests::registry_matches_manifests ... ok`. | `bin/dww/src/contract_metadata.rs:CONTRACT_METADATA_REGISTRY` | `registry_matches_manifests` | H/P |
| OBL-C239 | CLOSED | `validate_field_type`'s default arm accepted any unknown type, hiding manifest/encoder divergence until the encoder refused it. The vocabulary is enumerated (`u32`/`func_id`/`asset_id`/`proof`/`merkle_path` included) and an unknown type is refused. Run: `manifest_resolver::tests::unknown_field_type_is_rejected ... ok`; `known_field_types_are_accepted ... ok`. | `bin/dww/src/manifest_resolver.rs:validate_field_type` | `unknown_field_type_is_rejected` | M/P |
| OBL-C241 | CLOSED | `Membership::derive_note` hashed the DAO bulla where the circuit's `DOMAIN_COMMITMENT` belongs, so a call built through the helper carried a note `pay_premium.zk` refused. The helper now hashes the circuit's and client's preimage (`poseidon_hash(4, member_pub_x, member_pub_y, value, asset_id, expiry, blind)`), and the bulla is documented as a separate public input. Run: `test_membership_derive_note_matches_the_circuit_preimage ... ok`. | `src/contract/dao_escrow/src/model/mod.rs:Membership::derive_note` | `test_membership_derive_note_matches_the_circuit_preimage` | H/P |
| OBL-C246 | OPEN | The same-block relayer `initialize`+`deploy_capital` arrangement was removed rather than repaired; no endpoint-named control covers it. | `bin/dwowd/src/tests/heavyweight_pipeline.rs:test_relayer_lifecycle_heavyweight` | `test_relayer_lifecycle_heavyweight` | M/P |
| OBL-C248 | CLOSED | `prove_coverage_v1` was unauthenticated: it compared a caller-supplied `issuer_contract` against the public series record, so anyone could post a report, void a series and open emergency unstake. The arm now refuses with `AuthorityModelUnavailable` before any state read (OBL-C248/C275/C276). Run: `disabled_authority_arms_are_refused_before_any_state_read ... ok`. | `src/contract/bearer_bond/src/entrypoint/mod.rs:process_instruction` | `disabled_authority_arms_are_refused_before_any_state_read` | C/P |
| OBL-C275 | CLOSED | `issue_stake_v1` authenticated the issuer by comparing a caller-supplied `issuer_contract` with the public series record, so anyone could mint stake obligations into a series. The arm now refuses with `AuthorityModelUnavailable` before any state read; a key-based or contract-composition authority model is the redesign. Run: `disabled_authority_arms_are_refused_before_any_state_read ... ok`. | `src/contract/bearer_bond/src/entrypoint/mod.rs:process_instruction` | `disabled_authority_arms_are_refused_before_any_state_read` | C/P |
| OBL-C276 | CLOSED | `total_staked` was never updated after registration, so `request_interest_v1` computed interest against zero (the TODO in `apply_issue_stake`). The arm now refuses with `AuthorityModelUnavailable` before any state read; persisted series totals come with the redesign. Run: `disabled_authority_arms_are_refused_before_any_state_read ... ok`. | `src/contract/bearer_bond/src/entrypoint/mod.rs:process_instruction` | `disabled_authority_arms_are_refused_before_any_state_read` | M/P |
| OBL-C253 | CLOSED | An exit took member identity and weight from the caller (`params.contribution_weight`); `MemberWeight` carried no identity, `members` was never populated, and `exit.zk` constrained none of its membership witnesses. The arm now refuses before any state read, so no caller-supplied identity or weight reaches an arithmetic path. Run: `exit_is_refused_before_any_state_read ... ok`. | `src/contract/drain_protection/src/entrypoint.rs:process_instruction` | `exit_is_refused_before_any_state_read` | C/P |
| OBL-C259 | CLOSED | `get_vm`/`get_cache` held the map lock across the 256 MB RandomX construction, so a requester — including a smol executor thread — waited on a std mutex for the whole allocation (the residual of the C240 exclusivity fix; C240 is merged here). The lookup and the insert are now separately locked with a re-check, and the construction runs with the lock released. Run: `vm_construction_does_not_hold_the_cache_lock ... ok`, with a held-lock negative twin that makes `try_lock` fail. | `src/linear/src/chain_state.rs:get_vm` | `vm_construction_does_not_hold_the_cache_lock` | H/P |
| OBL-C261 | CLOSED | The 400M gas exhaustion was in **DeployV1** at height 2: the local `wasm_content_commitment` ran one Poseidon hash per 64-byte chunk over the 292,574-byte drain_protection module (~4,571 hashes) inside the guest (`fn_code=0x01` was the call-tree length prefix, not the selector). The upstream port deleted the content commitment; the height-2 gas failure is the failing-before evidence and `test_heavyweight_deployooor` passes. evidence:f1c551ee9b | `src/contract/deployooor/src/entrypoint/deploy_v1.rs:deploy_process_instruction_v1` | `test_heavyweight_deployooor` | H/D |
| OBL-C263 | CLOSED | The container build hand-duplicated the contract recipe as bare cargo commands: no `-C link-arg=--max-memory`, and strip/location-detail only for the first of 32 packages. The image's genesis artifacts declared no memory maximum and were refused at instantiation. The build now goes through the shared recipe with in-image gates; the artifact check's self-test is the failing-before control. evidence:24278c2508 | `contrib/docker/darkwow-testnet/Dockerfile:contracts` | `contrib/wasm_artifact_check.sh` | H/D |
| OBL-C264 | CLOSED | The pipeline counted a `Restarting` node as running, passed mining from a startup banner, and reported the genesis refusal as a mining timeout; the true artifact error appeared only in the final log tail. Phase 9 now fails fast with the named artifact logs; the 2026-10-25 run passes. evidence:24278c2508 negative:2026-10-23 genesis refusal was reported as a mining timeout | `contrib/docker/darkwow-testnet/lib/phase_09_blocks.sh:phase_blocks` | `contrib/docker/darkwow-testnet/test_pipeline.sh` | M/D |
| OBL-C265 | CLOSED | A wire VarInt length was reserved and zero-filled before its bytes were read, so one corrupt length (0xFE/0xFF) in scan call data or `DeployParamsV1` OOM-killed the wallet or node; the introducing commit `e42f2c1c1a`'s `try_reserve` bound is false under Linux overcommit. The control passes (`23 passed`) and its planted-defect run FAILED. evidence:e2b262e517 | `src/serial/src/lib.rs:u8::decode_vec` | `vec_u8_decode_rejects_length_one_over_cap` | H/D |
| OBL-C267 | CLOSED | The fee window's congestion factors were advanced only by a mining node's own mempool queue depths and never recomputed from applied blocks, so a syncing or boundary-losing node kept identity factors; the spec's identical-inputs premise was false and 13.1 contradicted FI-FLAG-2/3. The factors are now typed state derived on every applying node from the completed window's applied FeeV3 tiers, snapshotted per boundary and restored on disconnect. evidence:f2541499a5 | `src/linear/src/fee_window.rs:next_snapshot` | `test_next_snapshot_is_deterministic` | C/D |
| OBL-C268 | CLOSED | node1 set RPC_PORT=31346 while MANAGEMENT_PORT defaulted to 31346; both listeners targeted one port, management won, and blockchain.get_height was unreachable, which Phase 9 reported as sync lag. node1 now has MANAGEMENT_PORT=31344 and MM_RPC_PORT=31350, and the entrypoint refuses duplicate listeners. The 2026-10-25 run reports node1 height 38 then 1298. evidence:9af26fb23a negative:2026-10-24 node1 get_height answered method-not-found | `contrib/docker/darkwow-testnet/entrypoint.sh:MANAGEMENT_PORT` | `contrib/docker/darkwow-testnet/test_pipeline.sh` | M/D |
| OBL-C269 | CLOSED | The cumulative-commit singletons in the contracts tree and the supply-chain tree were written non-atomically on reorg (rollback under no lock, contracts undo in a separate sled write, WASM outside connect_lock), so a failed or concurrent reorg could leave them divergent. The disconnect now commits both trees atomically under the apply lock; the fault-injection control passes and failed under a planted pre-fix mutation (supply entry removed). | `bin/dwowd/src/block_acceptor.rs:rollback_cumulative_commit` | `test_disconnect_undo_failure_leaves_the_trees_consistent` | H/P |
| OBL-C270 | CLOSED | L3-FW-2 compared each node's last boundary log line with no boundary-height check or convergence wait, and a field-poor extra line made L3-FW-1 report no boundary line while two were present. The gate now reads the last flagged line at node0's named boundary and waits for each node to reach it; the 2026-10-25 run passes with node1 matching at boundary 20. evidence:9af26fb23a negative:2026-10-24 L3-FW-1 false failure | `contrib/docker/darkwow-testnet/lib/phase_fee_window.sh:_get_cf_at_height` | `contrib/docker/darkwow-testnet/test_pipeline.sh` | M/D |
| OBL-C42 | CLOSED | The stratum and merge-mining template paths took competing blocks destructively and restored on failure at the current tip, so a block connected in between moved the restore to the wrong height and lost the uncle candidate. The restore now takes the height the take used (`restore_competing_blocks(take_height, …)`); the take remains last in `prepare_block`. Run: `competing_blocks_restore_at_the_take_height ... ok`. | `src/linear/src/chain_state.rs:restore_competing_blocks` | `competing_blocks_restore_at_the_take_height` | M/P |
| OBL-C45 | CLOSED | The RPC returned the stored cumulative entry without recomputation. It now recomputes the emission schedule at read time and fails closed on a mismatch; the write path mirrors the coinbase WASM overlay into the supply-chain tree, and the contracts/supply writes are atomic under the apply lock (C269). Run: `test_supply_chain_audit ... ok`. | `bin/dwowd/src/rpc/blockchain.rs:blockchain_get_cumulative_supply` | `test_supply_chain_audit` | H/P |
| OBL-C54 | CLOSED | A stratum submit could be judged against a template a concurrent login had replaced. Every publication now carries a monotonic generation in `ActiveTemplate`, the job id names it, and a submit whose generation is not the active one is refused as stale before any PoW work. Run: `template_generation_distinguishes_replaced_templates ... ok`. | `bin/dwowd/src/rpc/stratum.rs:stratum_submit` | `template_generation_distinguishes_replaced_templates` | M/P |
| OBL-C56 | CLOSED | The sync-state check and the mining act were not atomic. The miner now holds `MiningState::mining_gate` from its CaughtUp re-check through block acceptance, and the consensus task takes the same gate while storing Behind, so a transition cannot land in the window. Run: `mining_gate_serializes_a_transition_with_the_mining_act ... ok`. | `bin/dwowd/src/lib.rs:miner_task` | `mining_gate_serializes_a_transition_with_the_mining_act` | M/D |
| OBL-C273 | CLOSED | `OpenPositionV1` accepted any PN transfer whose value commitment matched and never checked the recipient, so the pool held nothing and a depositor could mint against `total_collateral` bookkeeping. `OpenPositionV1`, `AddCollateralV1`, `MintStableV1` and `LiquidateV1` now refuse with `CustodyModelUnavailable` before any state read. Run: `disabled_custody_arms_are_refused_before_any_state_read ... ok`. | `src/contract/stablecoin/src/entrypoint.rs:process_instruction` | `disabled_custody_arms_are_refused_before_any_state_read` | C/D |
| OBL-C92 | CLOSED | A stablecoin redemption released the collateral it claimed in the ledger but paid none to the redeemer (C251 merged here): the only child burned the stablecoin note and the pool custodies no collateral. `RedeemStableV1` now refuses with `CustodyModelUnavailable` before any state read (OBL-C272); spec and manifest record it. Run: `disabled_custody_arms_are_refused_before_any_state_read ... ok`. | `src/contract/stablecoin/src/entrypoint.rs:process_instruction` | `disabled_custody_arms_are_refused_before_any_state_read` | C/D |
| OBL-C97 | CLOSED | The exit arm applied no stored authority. The member-exit path is now refused (C102/C253, OBL-C271), so no exit-authority check is reachable; the remaining value path `TransferV1` applies `require_fund_authority` against the stored `spend_authority`, as `execute` and `update_config` do. Run: `exit_is_refused_before_any_state_read ... ok`. | `src/contract/drain_protection/src/entrypoint.rs:require_fund_authority` | `exit_is_refused_before_any_state_read` | C/P |
| OBL-Z12 | CLOSED | The Lean `qr_unique` theorem gives quotient/remainder uniqueness over integers; a circuit proves it over field residues unless its operands are range-checked. The known quotient circuits (prove_coverage, governance_report, accrue_interest) do range-check every operand, and `script/quotient_bounds.txt` pins them; `check-circuit-quotient-bounds.sh` fails if a listed operand loses its check, and a new quotient circuit joins the manifest in the same change. Run: `--self-test` PASS; live PASS. | `script/quotient_bounds.txt` | `scripts/check-circuit-quotient-bounds.sh` | H/P |
| OBL-Z23 | CLOSED | 117 dead values were adjudicated while the dead-value gate was report-only, so a newly introduced value failed nothing. The 117 are declared in `script/circuit_dead_value_exceptions.txt` as a frozen baseline, and the gate now blocks: a new value is undeclared and fails the umbrella and the pre-commit hook. Run: `check-circuit-dead-values.sh --self-test` OK, and the blocking gate `PASS` (177 files walked). | `script/circuit_dead_values.py` | `scripts/check-circuit-dead-values.sh` | M/D |
| OBL-Z3 | CLOSED | The domain gate's role rule requires a derivation whose name declares a purpose to carry that purpose's domain (by role, not a conventional value); the self-test plants a `commitment = poseidon_hash(DOMAIN_NULLIFIER, …)` mismatch and requires it named. Inline `witness_base(N)` tokens have no name and remain under the presence rule. Run: `check-circuit-domain-separation.sh --self-test` PASS; live PASS (4 adjudicated). | `script/circuit_domain_separation.py:role_of` | `scripts/check-circuit-domain-separation.sh` | H/P |

## Non-defects

45 decisions, accepted risks and spec interpretations are in
[`decisions.md`](decisions.md). They are not rows here and cannot green a defect.

## Settled ids (citation appendix)

These ids were settled before the tracker migration. Their analysis is in git
(`git log -S "<id>" -- doc/src/arch/verification-hazop.md`); this appendix exists so that
citations elsewhere in the tree still resolve.

**The appendix is history, not a disposition vocabulary.** `settled`, `restated`, `accepted` and
`dissolved` are frozen records of the migration (257 rows). They are not exits from a live finding:
a new row may not use one, and the set is hashed by the approval manifest
(`appendix_baseline_sha256`), so adding or editing one fails the gate. New tombstones are only
`residual (R-n)`, `merged into OBL-X` or `decision`; each resolves to its record in
[`residual-risk.md`](residual-risk.md) or [`decisions.md`](decisions.md).

| ID | Disposition |
|---|---|
| OBL-C1 | settled |
| OBL-C10 | settled |
| OBL-C100 | settled |
| OBL-C101 | settled |
| OBL-C103 | settled |
| OBL-C104 | settled |
| OBL-C105 | settled |
| OBL-C106 | settled |
| OBL-C107 | settled |
| OBL-C108 | settled |
| OBL-C110 | settled |
| OBL-C111 | settled |
| OBL-C112 | settled |
| OBL-C114 | settled |
| OBL-C115 | settled |
| OBL-C116 | settled |
| OBL-C117 | settled |
| OBL-C118 | settled |
| OBL-C119 | settled |
| OBL-C12 | settled |
| OBL-C120 | settled |
| OBL-C121 | settled |
| OBL-C122 | settled |
| OBL-C123 | settled |
| OBL-C124 | settled |
| OBL-C126 | settled |
| OBL-C127 | settled |
| OBL-C129 | settled |
| OBL-C13 | settled |
| OBL-C131 | settled |
| OBL-C132 | settled |
| OBL-C133 | settled |
| OBL-C135 | settled |
| OBL-C137 | settled |
| OBL-C139 | settled |
| OBL-C14 | settled |
| OBL-C140 | settled |
| OBL-C141 | settled |
| OBL-C142 | settled |
| OBL-C143 | settled |
| OBL-C144 | settled |
| OBL-C145 | settled |
| OBL-C146 | settled |
| OBL-C147 | settled |
| OBL-C148 | settled |
| OBL-C15 | settled |
| OBL-C150 | settled |
| OBL-C151 | settled |
| OBL-C152 | settled |
| OBL-C153 | settled |
| OBL-C155 | settled |
| OBL-C156 | settled |
| OBL-C157 | settled |
| OBL-C158 | settled |
| OBL-C159 | settled |
| OBL-C16 | settled |
| OBL-C161 | settled |
| OBL-C162 | settled |
| OBL-C165 | settled |
| OBL-C166 | settled |
| OBL-C168 | settled |
| OBL-C169 | settled |
| OBL-C17 | settled |
| OBL-C170 | settled |
| OBL-C171 | settled |
| OBL-C172 | settled |
| OBL-C173 | settled |
| OBL-C174 | settled |
| OBL-C175 | settled |
| OBL-C176 | settled |
| OBL-C177 | settled |
| OBL-C178 | settled |
| OBL-C179 | settled |
| OBL-C180 | settled |
| OBL-C181 | settled |
| OBL-C182 | settled |
| OBL-C183 | settled |
| OBL-C184 | settled |
| OBL-C185 | settled |
| OBL-C186 | settled |
| OBL-C187 | settled |
| OBL-C189 | settled |
| OBL-C190 | settled |
| OBL-C191 | settled |
| OBL-C192 | settled |
| OBL-C193 | settled |
| OBL-C194 | settled |
| OBL-C195 | settled |
| OBL-C196 | settled |
| OBL-C197 | settled |
| OBL-C198 | settled |
| OBL-C199 | settled |
| OBL-C2 | settled |
| OBL-C20 | settled |
| OBL-C201 | settled |
| OBL-C203 | settled |
| OBL-C204 | settled |
| OBL-C207 | settled |
| OBL-C208 | settled |
| OBL-C209 | settled |
| OBL-C210 | settled |
| OBL-C211 | settled |
| OBL-C212 | settled |
| OBL-C213 | settled |
| OBL-C215 | settled |
| OBL-C217 | settled |
| OBL-C218 | settled |
| OBL-C219 | settled |
| OBL-C22 | settled |
| OBL-C220 | settled |
| OBL-C221 | settled |
| OBL-C223 | settled |
| OBL-C226 | settled |
| OBL-C227 | settled |
| OBL-C228 | settled |
| OBL-C229 | settled |
| OBL-C23 | settled |
| OBL-C232 | settled |
| OBL-C235 | settled |
| OBL-C236 | settled |
| OBL-C237 | settled |
| OBL-C238 | settled |
| OBL-C24 | settled |
| OBL-C242 | settled |
| OBL-C243 | settled |
| OBL-C244 | settled |
| OBL-C245 | settled |
| OBL-C247 | settled |
| OBL-C249 | settled |
| OBL-C25 | settled |
| OBL-C254 | settled |
| OBL-C255 | settled |
| OBL-C256 | settled |
| OBL-C257 | settled |
| OBL-C258 | settled |
| OBL-C26 | settled |
| OBL-C27 | settled |
| OBL-C28 | settled |
| OBL-C29 | settled |
| OBL-C3 | settled |
| OBL-C30 | settled |
| OBL-C31 | settled |
| OBL-C33 | settled |
| OBL-C34 | settled |
| OBL-C35 | settled |
| OBL-C36 | settled |
| OBL-C37 | settled |
| OBL-C38 | settled |
| OBL-C39 | settled |
| OBL-C4 | settled |
| OBL-C40 | settled |
| OBL-C41 | settled |
| OBL-C43 | settled |
| OBL-C44 | settled |
| OBL-C46 | settled |
| OBL-C47 | settled |
| OBL-C48 | settled |
| OBL-C49 | settled |
| OBL-C5 | settled |
| OBL-C50 | settled |
| OBL-C51 | settled |
| OBL-C52 | settled |
| OBL-C53 | settled |
| OBL-C55 | settled |
| OBL-C57 | settled |
| OBL-C58 | settled |
| OBL-C59 | settled |
| OBL-C6 | settled |
| OBL-C60 | settled |
| OBL-C61 | settled |
| OBL-C62 | settled |
| OBL-C63 | settled |
| OBL-C64 | settled |
| OBL-C65 | settled |
| OBL-C67 | settled |
| OBL-C68 | settled |
| OBL-C69 | settled |
| OBL-C7 | settled |
| OBL-C70 | settled |
| OBL-C71 | settled |
| OBL-C72 | settled |
| OBL-C73 | settled |
| OBL-C75 | settled |
| OBL-C77 | settled |
| OBL-C78 | settled |
| OBL-C79 | settled |
| OBL-C8 | settled |
| OBL-C81 | settled |
| OBL-C83 | settled |
| OBL-C84 | settled |
| OBL-C86 | settled |
| OBL-C87 | settled |
| OBL-C88 | settled |
| OBL-C89 | settled |
| OBL-C9 | settled |
| OBL-C90 | settled |
| OBL-C91 | settled |
| OBL-C93 | settled |
| OBL-C94 | settled |
| OBL-C98 | settled |
| OBL-C99 | settled |
| OBL-T1 | settled |
| OBL-T10 | settled |
| OBL-T11 | settled |
| OBL-T12 | settled |
| OBL-T13 | settled |
| OBL-T14 | settled |
| OBL-T15 | settled |
| OBL-T16 | settled |
| OBL-T18 | settled |
| OBL-T19 | settled |
| OBL-T2 | settled |
| OBL-T21 | settled |
| OBL-T25 | settled |
| OBL-T26 | settled |
| OBL-T3 | settled |
| OBL-T4 | settled |
| OBL-T5 | settled |
| OBL-T6 | settled |
| OBL-T8 | settled |
| OBL-Z1 | settled |
| OBL-Z10 | settled |
| OBL-Z11 | settled |
| OBL-Z14 | settled |
| OBL-Z15 | settled |
| OBL-Z16 | settled |
| OBL-Z19 | settled |
| OBL-Z2 | settled |
| OBL-Z20 | settled |
| OBL-Z21 | settled |
| OBL-Z22 | settled |
| OBL-Z7 | settled |
| OBL-Z9 | settled |
| OBL-C266 | decision (see decisions.md) |
| OBL-C11 | restated |
| OBL-C113 | decision |
| OBL-C125 | decision |
| OBL-C128 | restated |
| OBL-C134 | accepted |
| OBL-C138 | decision |
| OBL-C149 | dissolved |
| OBL-C154 | decision |
| OBL-C163 | decision |
| OBL-C164 | decision |
| OBL-C167 | decision |
| OBL-C18 | restated |
| OBL-C188 | decision |
| OBL-C19 | accepted |
| OBL-C200 | decision |
| OBL-C21 | accepted |
| OBL-C216 | restated |
| OBL-C222 | restated |
| OBL-C224 | restated |
| OBL-C225 | restated |
| OBL-C230 | restated |
| OBL-C231 | restated |
| OBL-C233 | restated |
| OBL-C234 | restated |
| OBL-C32 | accepted |
| OBL-C66 | residual (R-07, see residual-risk.md) |
| OBL-C260 | residual (R-08, see residual-risk.md) |
| OBL-C206 | residual (R-09, see residual-risk.md) |
| OBL-Z18 | residual (R-10, see residual-risk.md) |
| OBL-C76 | decision |
| OBL-C80 | accepted |
| OBL-C82 | decision |
| OBL-C85 | accepted |
| OBL-C95 | decision |
| OBL-C96 | decision |
| OBL-T17 | accepted |
| OBL-T20 | decision |
| OBL-T23 | decision |
| OBL-T7 | restated |
| OBL-T9 | accepted |
| OBL-Z13 | decision |
| OBL-Z17 | restated |
| OBL-Z4 | restated |
| OBL-Z5 | decision |
| OBL-Z6 | decision |
| OBL-Z8 | accepted |
| OBL-C74 | residual (R-02, see residual-risk.md) |
| OBL-C240 | merged into OBL-C259 |
| OBL-C250 | decision (see decisions.md) |
| OBL-C251 | merged into OBL-C92 |
| OBL-C262 | residual (R-01, see residual-risk.md) |
| OBL-T22 | residual (R-03, see residual-risk.md) |
| OBL-T24 | residual (R-04, see residual-risk.md) |
