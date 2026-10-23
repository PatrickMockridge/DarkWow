# Verification defect register — a tracker, not a task

This file tracks what is broken so it gets fixed. It is not a work product: a change to it is
worth nothing unless the code changed in the same commit. The normative rule is
[`doc/src/dev/hazop-rules.md`](../dev/hazop-rules.md) §"Definition of closed out".

**Row shape.** `ID | Status | Defect (one sentence) | Locus (path:function) | Control`.
`Status` is `OPEN` or `CLOSED`; there is no `PARTLY`, `FIXED`, `DECISION` or
`ACCEPTED-WITH-REASON` here. Non-defects live in [`decisions.md`](decisions.md). History
lives in git. A live row is capped at 600 characters.

**Closure is code.** `scripts/check-defect-register.sh` refuses a row that moves to `CLOSED`
in a change set that touches only docs/reports/exceptions, refuses a `CLOSED` row whose
control does not resolve or is not wired, and refuses a late closure whose evidence commit
does not change code. The pre-commit hook enforces the same two rules. The one legitimate
register-only commit is minting a new `OPEN` row.

| ID | Status | Defect | Locus | Control |
|---|---|---|---|---|
| OBL-C102 | OPEN | A protected fund's holdings are inert bookkeeping (`total_funds: 0`); the payout is not bound to the fund's purse. | `src/contract/drain_protection/src/entrypoint.rs:init_fund_process_instruction_v1` | `test_heavyweight_drain_protection` |
| OBL-C109 | OPEN | The Python maturity model keys by commitment while the code keys by nullifier; they are not the same source of truth. | `contrib/model/chain_model.py:is_commitment_mature` | `—` |
| OBL-C130 | OPEN | The wallet selects a capability by asset id only, not by `ContractId` and barbs as §6.4.1 requires. | `bin/dww/src/dispatch.rs:dispatch_sync` | `scripts/check-wallet-kernel.sh` |
| OBL-C136 | OPEN | A fixed 5-second wait, not wait-until-consistent, makes the event-graph DAG test fail under load with no stated cause. | `src/event_graph/tests.rs:bootstrap_nodes` | `—` |
| OBL-C160 | OPEN | A vote's capability secret is read from public call data, so the one-vote property rests on the group approval rather than a secret. | `src/contract/dao_escrow/src/entrypoint.rs:vote_claim_v1` | `test_heavyweight_dao_escrow` |
| OBL-C202 | OPEN | An instrument is a gate only over what it was pointed at; the bearer_bond round-trip covers two arms and the spec covers five. | `src/contract/test-harness/tests/encode_roundtrip.rs` | `scripts/check-codec-size-derivation.sh` |
| OBL-C205 | OPEN | No prove+verify control goes through native_token's burn arm; its vector is built by hand and can disagree with the circuit. | `src/contract/native_token/src/entrypoint/mod.rs:burn_get_metadata` | `—` |
| OBL-C206 | OPEN | A PI-7 block-hash mismatch is not reproducible under an identical configuration, so a single failure cannot be read as a verdict. | `bin/dwowd/src/tests/uniform_runner.rs:run_heavyweight_test` | `test_heavyweight_stablecoin` |
| OBL-C214 | OPEN | The wallet's hardcoded contract registry is not compared with any `manifest.toml`; the three homes can drift. | `bin/dww/src/contract_metadata.rs:CONTRACT_METADATA_REGISTRY` | `—` |
| OBL-C239 | OPEN | `validate_field_type`'s default arm accepts an unknown type the encoder then rejects. | `bin/dww/src/manifest_resolver.rs:validate_field_type` | `—` |
| OBL-C240 | OPEN | VM exclusivity is type-carried, but the lock that carries it is a blocking `std::sync::Mutex` (residual is OBL-C259). | `src/linear/src/execution.rs:TxBackend` | `per_vm_mutex_serializes_hash_calls` |
| OBL-C241 | OPEN | `Membership::derive_note` hashes a different preimage than the circuit and client, so the helper builds calls the circuit refuses. | `src/contract/dao_escrow/src/model/mod.rs:Membership::derive_note` | `—` |
| OBL-C246 | OPEN | The same-block relayer `initialize`+`deploy_capital` arrangement was removed rather than repaired; no endpoint-named control covers it. | `bin/dwowd/src/tests/heavyweight_pipeline.rs:test_relayer_lifecycle_heavyweight` | `test_relayer_lifecycle_heavyweight` |
| OBL-C248 | OPEN | `prove_coverage_v1` is unauthenticated and `total_staked` is never updated; only the empty-success half is fixed. | `src/contract/bearer_bond/src/entrypoint/mod.rs:verify_coverage_v1` | `test_heavyweight_bearer_bond` |
| OBL-C250 | OPEN | The deploy's stored WASM hash was a local addition: it committed to the module's length rather than its bytes and nothing read it. The upstream port deletes the write, the read-back and the field; closure awaits the heavyweight control. | `src/contract/deployooor/src/entrypoint/deploy_v1.rs:deploy_process_instruction_v1` | `test_heavyweight_deployooor` |
| OBL-C251 | OPEN | A redemption pays no collateral to the redeemer and the spend hook counts burned nullifiers as redeemed value; the asset is not bound. | `src/contract/stablecoin/src/entrypoint.rs:process_redeem_stable_instruction` | `test_heavyweight_stablecoin` |
| OBL-C253 | OPEN | An exit trusts the caller for member identity and weight, and the payout is not checked against the fund's purse. | `src/contract/drain_protection/src/entrypoint.rs:exit_process_instruction_v1` | `test_heavyweight_drain_protection` |
| OBL-C259 | OPEN | A thread waiting on the RandomX VM lock blocks a smol executor thread. | `src/linear/src/chain_state.rs:CChainState::vm_cache` | `—` |
| OBL-C260 | OPEN | The generic manifest client constructs one proof per call, so a multi-proof arm (PN redeem/transfer/otc_swap) cannot be built; it now fails closed. | `src/sdk/src/contract_client.rs:ManifestCallPlan::prove` | `—` |
| OBL-C261 | OPEN | The 400M gas exhaustion is in **DeployV1** at height 2, not LockV1: the local `wasm_content_commitment` ran one Poseidon hash per 64-byte chunk over the 292,574-byte drain_protection module (~4,571 hashes) inside the guest (`fn_code=0x01` was the call-tree length prefix, not the selector). The upstream port deletes the content commitment. | `src/contract/deployooor/src/entrypoint/deploy_v1.rs:deploy_process_instruction_v1` | `test_heavyweight_deployooor` |
| OBL-C262 | OPEN | Scan-time manifest/WASM Layer-2 comparison has no manifest to read: upstream `DeployV1` `ix` is the new contract's `__initialize` payload, and the port removed the local manifest envelope. The comparison remains available for manifests the wallet holds, but a user deployment's manifest is wallet-side and is not checked against that deployment's WASM at scan time. | `bin/dww/src/scan.rs:scan_block_linear` | `—` |
| OBL-C263 | OPEN | The container build hand-duplicated the contract recipe as bare cargo commands: no `-C link-arg=--max-memory`, and strip/location-detail only for the first of 32 packages. The image's genesis artifacts declared no memory maximum and were refused at instantiation; the same drift breaks genesis-hash parity. | `contrib/docker/darkwow-testnet/Dockerfile:contracts` | `contrib/wasm_artifact_check.sh` |
| OBL-C264 | OPEN | The pipeline counted a `Restarting` node as running, passed mining from a startup banner, and reported the genesis refusal as a mining timeout; the true artifact error appeared only in the final log tail. | `contrib/docker/darkwow-testnet/lib/phase_09_blocks.sh:phase_blocks` | `—` |
| OBL-C265 | CLOSED | A wire VarInt length was reserved and zero-filled before its bytes were read, so one corrupt length (0xFE/0xFF) in scan call data or `DeployParamsV1` OOM-killed the wallet or node; the introducing commit `e42f2c1c1a`'s `try_reserve` bound is false under Linux overcommit. The control passes (`23 passed`) and its planted-defect run FAILED. | `src/serial/src/lib.rs:u8::decode_vec` | `vec_u8_decode_rejects_length_one_over_cap` |
| OBL-C266 | OPEN | PoW hashes in RandomX cache (light) mode on every path — no dataset is ever allocated — while consensus documents claim a ~4 GB dataset that forces memory-hard computation; either the property is not enforced or the documents are false. The node-memory audit found it; no fix is claimed. | `src/linear/src/chain_state.rs:get_vm` | `—` |
| OBL-C45 | OPEN | The contracts tree and the supply-chain tree are not reconciled against each other; the RPC returns stored state. | `bin/dwowd/src/rpc/blockchain.rs:blockchain_get_cumulative_supply` | `test_supply_chain_audit` |
| OBL-C54 | OPEN | A stratum submit can be judged against a template a concurrent login has already replaced; writes are serialised but unversioned. | `bin/dwowd/src/rpc/stratum.rs:stratum_submit` | `—` |
| OBL-C56 | OPEN | The sync-state check and the mining act are not atomic; a transition can land in the window before accept. | `bin/dwowd/src/lib.rs:miner_task` | `—` |
| OBL-C74 | OPEN | 24 of 32 manifests declare no `required_barbs`/`note_schema`, and six declare no `[[capabilities]]`. | `src/contract/stablecoin/manifest.toml` | `—` |
| OBL-C92 | OPEN | A stablecoin redemption releases the collateral it claims in the ledger but pays no collateral to the redeemer. | `src/contract/stablecoin/src/entrypoint.rs:process_redeem_stable_instruction` | `test_heavyweight_stablecoin` |
| OBL-C97 | OPEN | The exit arm of a protected fund does not apply the fund's stored spend authority (the member-authorization design is OBL-C253). | `src/contract/drain_protection/src/entrypoint.rs:exit_process_instruction_v1` | `test_heavyweight_drain_protection` |
| OBL-T22 | OPEN | The write path's encode step is unmodelled, so byte-identity of the Lean wallet state is proved in one direction only. | `proofs/lean/src/DarkFi/Capability/WalletState.lean:encodeConfirmed` | `scripts/check-lean-suite.sh` |
| OBL-T24 | OPEN | The mempool's admission and expiry rules are modelled nowhere in the Lean wallet model. | `proofs/lean/src/DarkFi/Capability/WalletState.lean` | `—` |
| OBL-Z12 | OPEN | A circuit's integer-quotient proof is about field residues unless the operand bound is established; the Lean theorem states the bound and nothing on chain establishes it. | `proofs/lean/src/DarkFi/BaseDivGadget.lean:qr_unique` | `scripts/check-circuit-instance-derivation.sh` |
| OBL-Z18 | OPEN | 51 `constrain_equal_base` sites have no verifier-visible operand; each is adjudicated, but the sweep is not closed. | `scripts/check-pubkey-binding.sh` | `scripts/check-pubkey-binding.sh` |
| OBL-Z23 | OPEN | 117 values a circuit computes reach no constraint (100 dead witnesses, 17 dead derivations); no undeclared one is in a genesis circuit. | `scripts/check-circuit-dead-values.sh` | `scripts/check-circuit-dead-values.sh` |
| OBL-Z3 | OPEN | The circuit-domain gate accepts any `DOMAIN_`/`witness_base` token in a `poseidon_hash` call and never checks it is the right domain for that hash's purpose. | `scripts/check-circuit-domain-separation.sh` | `scripts/check-circuit-domain-separation.sh` |

## Non-defects

44 decisions, accepted risks and spec interpretations are in
[`decisions.md`](decisions.md). They are not rows here and cannot green a defect.

## Settled ids (citation appendix)

These ids were settled before the tracker migration. Their analysis is in git
(`git log -S "<id>" -- doc/src/arch/verification-hazop.md`); this appendix exists so that
citations elsewhere in the tree still resolve.

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
| OBL-C42 | accepted |
| OBL-C66 | accepted |
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
