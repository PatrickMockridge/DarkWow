# Launch Risk Register — Public Testnet and Mainnet

> **What this is.** The ranked set of risks that decide whether DarkWow can run a public testnet or
> mainnet, what each one would do to the network or its users, and the run or decision that closes
> it. It separates the two launch stages and records the disposition of every unsettled severity-C
> and severity-H obligation (`OPEN`, `PARTLY`, `FAILS` or `DECISION`).
>
> **What this is not.** It is **not** an obligation ledger. `doc/src/arch/verification-hazop.md` is
> the only place where an obligation's status is recorded, and it remains authoritative over every
> word here. This register has no Status column and never restates an obligation's status. Its
> `Disposition` column is a different axis: what the **launch gate** does about the risk. A `BLOCKS`
> row does not mean the cited obligation is open; it means the stage cannot launch while the risk is
> unresolved. A settled obligation can still anchor a launch-scope `DECIDE` row, and an open one can
> be `WATCH`ed.
>
> **How the gate keeps that true.** `scripts/check-launch-register.sh` reads the cited rows' status
> cells from the obligation register at run time. It fails when a new unsettled severity-C or H
> obligation is neither ranked nor excluded, when a `FIX` row cites only settled obligations, when an `ACCEPTED`
> row has no decision of record, when a `RETIRED` row still cites an open obligation, and when a row
> restates an obligation status. No status text is stored here, so there is nothing to drift.

## The launch ladder

| Stage | What it is | Normative gate |
|---|---|---|
| L3 | Containerized localnet: real PoW, real P2P over TLS, real build-from-source | [`l3-readiness-spec.md`](../dev/testing/l3-readiness-spec.md) G1–G8 and AC-1–AC-5 |
| Public testnet | `darkwow-testnet`: the public coordination network, magic `DRKW`, ports 31xxx | [`bootstrapping.md`](../testnet/bootstrapping.md) phases 1–5; [`network-types.md`](network-types.md) |
| Mainnet | Production network, currently a placeholder: magic, ports and seeds are unset | [`network-types.md`](network-types.md); [`level-3-localnet.md`](../dev/testing/level-3-localnet.md) transition table |

A `TESTNET` row blocks the public testnet stage. A `MAINNET` row blocks mainnet only; it may be
accepted for a testnet. A `BOTH` row blocks either stage. Mainnet is downstream of the public
testnet, so the release checklist treats every blocking row as blocking mainnet.

## How to read a row

| Column | Meaning |
|---|---|
| `Rank` | Position in this register, 1 to N. Unique. Re-ranking never changes an `LR` id. |
| `LR` | Stable citation identity. `LR-` is a deliberate new scheme; no existing scheme in this tree was free. It is never an obligation id. |
| `Stage` | `TESTNET`, `MAINNET` or `BOTH`. |
| `Class` | `DEFECT`, `ASSURANCE`, `OPERATIONS`, `ECONOMICS` or `DECISION`. |
| `Sev` | The existing C/H/M/L legend, applied to this risk's launch impact. It is not a copy of the cited obligation's severity; where they differ, the failure scenario says why. |
| `Failure scenario` | The concrete event: what happens, to whom, at which stage. |
| `Evidence` | `OBL-*` ids, `LR-D*` decisions, `path:line` citations, or a quoted command and date. |
| `Disposition` | What the stage gate does: `BLOCKS` (no launch while unresolved), `CONDITIONAL` (blocks unless the named condition holds), `ACCEPTED` (a dated decision accepts it), `RETIRED` (closed with evidence), `WATCH` (tracked, not a stage gate). |
| `Action` | What changes the disposition: `FIX`, `DECIDE`, `MEASURE`, `SEQUENCE`, `NONE`. |
| `Exit criterion` | The condition, command or decision that closes the row. A row is never retired by assertion. |

## The ranked risks

<!-- launch-risk-rows -->
| Rank | LR | Stage | Class | Sev | Failure scenario | Evidence | Disposition | Action | Exit criterion |
|---:|---|---|---|---|---|---|---|---|---|
| 1 | LR-01 | BOTH | DEFECT | C | A non-merging node admits a fabricated merge-mined block and credits its reward when `monerod_url` is unset; the startup gate bounds producers, not receivers. | `OBL-C80`; `src/linear/src/monero/verify.rs`; `bin/dwowd/src/block_acceptor.rs`; `bin/dwowd/src/main.rs` (the `mm_rpc` startup gate) | CONDITIONAL | DECIDE | `LR-D1`: merge mining is outside the launch surface until the hole is closed, or every admitting node sets `monerod_url` and the checklist proves it; a fabricated merge-mined block is rejected. |
| 2 | LR-02 | BOTH | DEFECT | H | A transaction declares fewer nullifiers than its calls spend; the chain replay gate, the coinbase-maturity rule and the mempool filter read the declaration. | `OBL-C146`; `src/linear/src/nullifiers.rs`; `src/linear/src/chain_state.rs:1211-1235`; `crates/dwow-mempool/src/lib.rs:372-380` | RETIRED | NONE | measured 2026-10-20: `derive_l1_nullifiers` + `reconcile_declared` run in `connect_block` and in the mempool; omitted, extra, exact, non-L1-subset and claim-transaction controls pass; `test_heavyweight_fee_v3`, purse, box and promissory_note, both fee-integration lifecycles and both mempool accept-block paths are green. |
| 3 | LR-03 | BOTH | ASSURANCE | H | The launch decision would rest on incomplete evidence: the 2026-10-20 report is 29 green and 3 red (`wallet_model`, `wasm_artifact_genesis`, `fee_guardrails`), leaves C3, C4, C5, C11 and C12 not met or not established, and the last complete heavy sweep was 2026-09-25. | `reports/status-2026-10-20.md`; `reports/status-2026-09-27.md`; `scripts/run-all-tests.sh`; `OBL-C137`; `OBL-C202`; `OBL-C205`; `OBL-C206`; `OBL-C210`; `OBL-C215`; `doc/src/dev/contracts/checklist.md` | BLOCKS | MEASURE | A fresh `report-status.sh`, `run-all-tests.sh` and chunked full heavy sweep on the launch commit; the report's C3, C4, C5 and C11 criteria re-measured; every red gate green, wired, or named. |
| 4 | LR-04 | MAINNET | ASSURANCE | H | Genesis artifacts are not pure or reproducible: 9 of 9 carry panic machinery, 5 embed first-party paths, the clearing levers are absent, and the purity gate is report-only. | measured 2026-10-20: `bash contrib/wasm_artifact_check.sh --genesis` reports 9 of 9 dirty; `reports/status-2026-10-20.md` (G4 and G6 not met); `OBL-C140`; `OBL-C147`; `OBL-C210`; `OBL-T26`; `scripts/run-all-tests.sh:643-647` | RETIRED | NONE | measured 2026-10-20: 9/9 CLEAN; 32/32 byte-identical across two clean builds; the pin was re-recorded once and the pin test plus its wrong-hash control pass. `OBL-C210` and `OBL-T26` are covered by the recipe gate. |
| 5 | LR-05 | BOTH | ASSURANCE | H | Heavyweight evidence is red and incomplete: the last complete sweep was 45 passed and 10 failed, and the missing full sweep leaves L3 G3 and the genesis heavyweight criterion G11 unestablished. | `doc/src/arch/verification-hazop.md` (the 2026-09-25 sweep, with verbatim causes); `reports/status-2026-10-20.md` (C11 not established); `doc/src/dev/testing/l3-readiness-spec.md` | BLOCKS | MEASURE | A chunked full heavy sweep (`bin/dwowd/src/tests/heavyweight.sh`) on the launch commit with 0 failed, or each failure tracked and accepted; the nine genesis heavyweight tests green; the coverage gaps closed or declared. |
| 6 | LR-06 | TESTNET | OPERATIONS | H | A native wallet following the public-testnet guide gets the localnet profile: TLS certificate validation is disabled and its only peer is loopback. | `OBL-C247`; `bin/dww/dww_config.toml` (`[network_config."darkwow-testnet".net]`); `bin/dww/src/config.rs`; `src/net/{connector.rs,transport/tls.rs,known_hosts.rs}`; `contrib/docker/darkwow-testnet/entrypoint-wallet.sh` | RETIRED | NONE | measured 2026-10-21: the public profile is `localnet = false`, `DRKW`, with no loopback peer and no upstream `testnet` profile; the Docker entrypoint takes `LOCALNET`/`PEER_ADDR` explicitly and refuses a public run with no peer; TLS validates the SAN and pins per peer host, persisted in the P2P datastore (the store had never been wired); the embedded-config test and the wrong-SAN/changed-pin/second-peer/persisted-pin controls pass. |
| 7 | LR-07 | BOTH | DEFECT | H | A wallet invokes a deployed contract without verifying that the manifest it read matches the WASM the chain will run. | `OBL-C237`; `bin/dww/src/manifest_verify.rs`; `bin/dww/src/scan.rs`; `doc/src/arch/contract-trust-model.md` | RETIRED | NONE | measured 2026-10-21: scan-time Layer 2 parses the deploy's WASM, checks the four fixed exports, matches declared circuits to embedded zkas as a namespace multiset, refuses undeclared embedded circuits, and stores the verdict; five controls pass, including tampered manifest (missing circuit) and undeclared-circuit cases; `contract show` displays the verdict. The check found `LR-21`. |
| 8 | LR-08 | MAINNET | DEFECT | H | Identity's capability check proves a credential preimage but requires no box child, and the box wire has no owner key, so possession is not established. | `OBL-C171`; `src/contract/identity/src/entrypoint.rs:581-724`; `src/contract/box/src/model/mod.rs` (the `TakeParams` wire) | RETIRED | NONE | measured 2026-10-20: the credential holder signs the call's nonce and verifier, and the host checks it against the stored `holder_pub`; the identity heavyweight passes in 146 s. A `preimage-without-box` fixture remains a useful second control. |
| 9 | LR-09 | MAINNET | ASSURANCE | H | Launch materials can overstate assurance: the circuit transcriptions last measured 11 hold and 168 fail, the Lean layer rests on named assumptions, and no third-party audit exists. | `doc/src/arch/formal-specification.md`; `doc/src/arch/security-analysis.md`; `reports/status-2026-10-20.md`; `proofs/lean/src/DarkFi/Axioms.lean` | BLOCKS | DECIDE | `LR-D3`: one canonical status page separates proved, assumed, transcribed and unverified; the testing claims are corrected; a third-party audit is committed or the launch is explicitly unaudited; the Lean and axiom gates are green on the launch commit. |
| 10 | LR-10 | MAINNET | DEFECT | H | A stablecoin redemption credits `redeem_amount * total_collateral / total_debt` from caller-chosen values, pays no collateral, and the spend hook counts burned nullifiers rather than value. | `OBL-C92`; `OBL-C251`; `src/contract/stablecoin/src/entrypoint.rs` (`process_redeem_stable_instruction`, `process_spend_hook`); `src/contract/stablecoin/src/model/mod.rs` (`RedeemStableParamsV1`) | BLOCKS | DECIDE | The value binding is landed (`RedeemStableParamsV1.redeem_value_blind`, burned-nullifier replay anchor, `redemptions` report tree); the custody/payout design and value-based redemption accounting are decided and implemented, with controls for a paid redemption, a wrong-value note, a wrong-asset note and a mismatched report. |
| 11 | LR-11 | MAINNET | DEFECT | H | Escrow binds its box child through `contents_commit`, whose publication the privacy clause forbids; a caller can supply a different box carrying a chosen commitment. (The reachable recipient-authorization hole found in the same review is fixed: claim requires the seller, refund the buyer.) | `OBL-C184`; `src/contract/escrow/src/entrypoint.rs` (`escrow_claim_process_instruction_v1`, `escrow_refund_process_instruction_v1`); `src/contract/box/src/model/mod.rs`; `doc/src/arch/privacy.md` | RETIRED | NONE | measured 2026-10-21: `box::Take` publishes `box_tag = poseidon(4, box_id, contents_commit)` and escrow's `ClaimV1` compares it against the tag it derives from its own record (`CLAIM_BOX_ID_DOMAIN = 14`), so a different child box is refused; neither operand reaches the wire and `scripts/check-l1-wire-conformance.sh` reports 0 declared leaks; box 15/15, escrow 21/21, the wallet-driven take and both heavyweight runs are green. The tag is a one-way pseudonym, not secrecy from a party that knows escrow's public create params — recorded in `OBL-C184`. |
| 12 | LR-12 | MAINNET | DEFECT | H | Drain protection's authority check is only partly landed: vote now compares the proof point to the stored `spend_authority`, but `exit` trusts a caller-chosen member key and weight, and the fund records no member keys, so an exit is unauthenticated and does not pay from the fund's purse. | `OBL-C97`; `OBL-C253`; `src/contract/drain_protection/src/entrypoint.rs` (`exit_process_instruction_v1`); `src/contract/drain_protection/src/model/mod.rs` (`MemberWeight`) | BLOCKS | FIX | Every fund-holding arm compares the proof point to the stored authority; exit has a member-admission surface, uses the stored weight, pays from the fund's purse and binds the member in `exit.zk`; a `non-authority-secret` vote and a non-member exit are refused. |
| 13 | LR-13 | MAINNET | DEFECT | H | The DAO vote message names the voter and direction and the vote/cancel arms now refuse a claim from another endowment, but `capability_secret` remains public call data and the residual circuit change is owed. | `OBL-C160`; `src/contract/dao_escrow/src/entrypoint.rs` (`vote_claim_v1`, `cancel_claim_v1`); `src/contract/dao_escrow/src/model/mod.rs` (`CapabilityProof`); `src/contract/multisig/src/entrypoint/mod.rs` | BLOCKS | DECIDE | Every approval binds the actor and every value; `capability_secret` is retired or proven non-secret; a second endowment's group cannot vote or cancel another endowment's claim (control owed); a signature cannot be replayed by another member. |
| 14 | LR-14 | MAINNET | DEFECT | H | The bridge cannot verify an external chain: `bridge-verify` is enabled by no build, three of five verifiers are unwritten or uncompiled, two need an absent pairing dependency, and the Ethereum arm returned `Ok` after an RLP decode. The feature is now compile-error-guarded and Ethereum fails closed. | `OBL-C21`; `src/contract/bridge/src/lib.rs` (the `compile_error!` guard); `src/contract/bridge/src/verify/ethereum.rs`; `src/contract/bridge/src/verify/{zcash,aztec}.rs` | BLOCKS | DECIDE | `LR-D4`: the bridge is outside the launch deployment surface until `bridge-verify` builds with real verifiers; each advertised chain refuses an empty or malformed proof. |
| 15 | LR-15 | MAINNET | ECONOMICS | H | Bearer-instrument guarantees are not enforced: the coverage endpoint now returns the stored report, but coverage submission is unauthenticated, `total_staked` is never updated (interest is 0), reserves and profits are self-reported, and issuance has no cap. | `OBL-C248`; `src/contract/bearer_bond/src/entrypoint/mod.rs` (`verify_coverage_v1`, `prove_coverage_v1`, `apply_issue_stake`); `doc/src/arch/economics-caveat-emptor.md` | BLOCKS | FIX | Coverage submission is bound to the series issuer, `total_staked` tracks issued principal, the non-guarantees are published at launch, and the supply-cap and redemption decisions are recorded. |
| 16 | LR-16 | BOTH | ASSURANCE | H | Fixture controls that cannot fail: the relayer double-spend control was once satisfied by a missing child and the relayer lifecycle test was a known red; the same class remains in the instrument rows `C202`/`C205`/`C136`. The relayer frame, the deposit-row promotion and the no-op `verify_state` closures are fixed; the lifecycle test is green. | `OBL-C246`; `OBL-C202`; `OBL-C205`; `OBL-C136`; `bin/dwowd/src/tests/heavyweight_pipeline.rs` (`test_relayer_lifecycle_heavyweight`); `bin/dwowd/src/tests/specs/bridge_spec.rs` | WATCH | FIX | Every control is proven to fail for its own reason; required children are attached; the `test_heavyweight_*` control output is quoted. |
| 17 | LR-17 | BOTH | ASSURANCE | H | No checker ties a manifest's `[[parameters]]` to the contract's decoder: `witness` means the client fills and publishes, `off_wire` means not on the wire, and a schema can stop agreeing with `encode` with no gate noticing. The model's hardcoded wire table is a third copy and is already stale: the 2026-10-20 `wallet_model` run fails on `box.put` because the table still expects `tx_binding`, which left the wire in `OBL-C198`. | `OBL-C179`; `OBL-C198`; `src/sdk/src/manifest.rs:284-327`; `src/sdk/src/contract_client.rs:456-474`; `src/contract/purse/manifest.toml`; `reports/status-2026-10-20.md` | RETIRED | NONE | measured 2026-10-20: the tag quadrants are normative in `doc/src/arch/contract-wasm-type-system.md` §A.6.3.1; `scripts/check-schema-decoder-agreement.py` compares 5 box/purse pairs and its control removes a published field; `wallet_model` is 97/97. |
| 18 | LR-18 | TESTNET | OPERATIONS | M | Public-testnet operations are not production-shaped: the pipeline pattern shares one secret between miner and wallet, monitoring is a health-check snippet with no alerting or incident runbook, and the faucet and explorer are listed as future work rather than run. | `doc/src/dev/testing/level-3-localnet.md` (the secret-sharing pattern); `doc/src/testnet/bootstrapping.md`; `doc/src/arch/audit/docs-review-2026-10-10.md` (Phase 4) | WATCH | DECIDE | Separate miner and wallet keys in the public runbook; a `doc/src/testnet/` checklist covering monitoring, seeds, faucet and explorer is exercised once. |
| 19 | LR-19 | BOTH | DEFECT | M | Key material is cloned and not zeroized, and no test covers redaction, zeroization or instance-scoped derivation. | `OBL-C12`; `src/sdk/src/crypto/keypair.rs:144-167`; `src/sdk/src/crypto/keypair.rs:202-222` | RETIRED | NONE | measured 2026-10-21: `SecretKey`'s `Drop` clears the field element with volatile stores (the plain `memset` was removable), `derive_instance` and `poseidon_hash` clear their preimage copies, and `from_secret`/`fp_mod_fv` clear the scalar and field copies they make; `cargo test -p dwow-sdk --lib crypto::keypair` → 6 passed, including the `Copy` negative assertion, redaction, determinism/scoping and a `MaybeUninit` before/after-drop zeroization control. The obligation row is settled; the remaining secret-equivalent temporaries in `ecvrf`/`diffie_hellman`/`note`/`schnorr` are named in that row's residual rather than folded into this retirement. |
| 20 | LR-20 | BOTH | DEFECT | M | Accepted liveness risks remain: a 30 s wall-clock overrun is warned and not terminated, expensive BN254 and Keccak work is undercharged by the gas cost function, and the sync-state and stratum-template races are bounded rather than eliminated. | `OBL-C19`; `OBL-C56`; `OBL-C54`; `src/runtime/vm_runtime.rs:774-788`; `src/linear/src/opcode_cost.rs` | ACCEPTED | DECIDE | `LR-D2`: the wall-clock warning is the node-local detection signal and gas is the consensus bound; operators alarm on it; a gas-accuracy review covers the named expensive operations. |
| 21 | LR-21 | BOTH | DEFECT | H | A wallet following a genesis manifest cannot load the circuit it is about to prove: `native_token`, `promissory_note` and `identity` declare a contract-wide `namespace`, while their zkas binaries carry the circuit name (`Transfer_V2`, `Fee_V3`, `IssueCredentialV2`), so the generic prover's `(namespace, circuit)` lookup misses. | `OBL-C249`; `src/contract/{native_token,promissory_note,identity}/manifest.toml`; `bin/dww/src/scan.rs`; `src/sdk/src/contract_client.rs:429` | BLOCKS | FIX | The three manifests name the compiled circuit; each genesis contract's Layer-2 verdict reports `ok`; a test decodes each `[[circuits]]` entry's own `.zk.bin` and asserts the namespace agrees; the change rides the scheduled C181/Z23 genesis re-roll. |
| 22 | LR-22 | BOTH | DEFECT | H | An identity capability can be revoked by anyone: `revoke_capability` writes a marker for a caller-chosen `holder_pub` and never reads `params.signature` or the capability record. | `OBL-C255`; `src/contract/identity/src/entrypoint.rs` (`revoke_capability`); `src/contract/identity/src/model/mod.rs` (`RevokeCapabilityParams.signature`) | BLOCKS | DECIDE | The revocation authority and signed message are decided (`RevokeCapabilityParams`); the signature is verified against the stored record; a non-holder/non-issuer revocation is refused; the change rides the identity pin batch. |
<!-- /launch-risk-rows -->

## Decisions of record

| ID | Date | Decision | Rationale | Scope |
|---|---|---|---|---|
| LR-D1 | 2026-10-20 | Merge mining is outside the public-testnet and mainnet launch surface until `OBL-C80` is closed. If a launch includes merge-mined blocks, every node that admits them must set `monerod_url`, and the release checklist proves it. | The obligation's own row says a non-merging node still admits unverified merge-mined blocks; the startup gate bounds producers only. | LR-01; OBL-C80 |
| LR-D2 | 2026-10-20 | The 30 s wall-clock warning is accepted as a node-local detection signal; gas is the consensus bound. Operators alarm on the warning, and gas-cost accuracy for the named expensive operations is tracked separately. | A wall-clock rule would make acceptance a function of hardware; gas is the value every node must agree on. | LR-20; OBL-C19 OBL-C56 OBL-C54 |
| LR-D3 | 2026-10-20 | Launch materials claim only the mechanized verification subset and the audit status of the launch commit. No whole-system "formally verified" or "audited" claim is made without a current report. | The circuit transcriptions are a model-to-source comparison, not a proof of the shipped circuits, and no current third-party audit exists. | LR-09 |
| LR-D4 | 2026-10-20 | The bridge is not part of the launch deployment surface until `bridge-verify` is enabled with real per-chain verifiers. | `bridge-verify` is declared but enabled by no build; three of five verifiers are unwritten or uncompiled. | LR-14; OBL-C21 |

## Exclusions

<!-- launch-risk-exclusions -->
<!-- (none: every unsettled severity-C or H obligation is ranked above) -->
<!-- /launch-risk-exclusions -->

An exclusion is a named obligation judged not launch-relevant, with a reason. The gate fails an
exclusion that names a settled row, a row that is not severity C or H, a row that does not exist, or
a row that is also ranked. The pressure valve exists so a new C/H obligation can be dispositioned
without forcing it into a launch blocker.

## What this register does not establish

- **The ranking is editorial.** Severity and order are judgements. The cited evidence is a pointer,
  not a re-audit, and the gate cannot decide whether a risk is real.
- **The cited obligations are read, not re-verified.** Their status cells are read live from the
  obligation register on each run; the gate proves the citation resolves, not that the row is
  current.
- **No runtime measurement is implied.** The 2026-09-27 report and the 2026-09-25 sweep are dated
  observations. Rows that use them say so and carry the command that replaces them. A green static
  gate is not a launch verdict.
- **The set is not exhaustive.** It ranks what the current registers and a fresh read of the
  launch-relevant surface cover; an unregistered defect is still unregistered.
- **It does not decide policy.** A `DECIDE` row names the choice and its cost; the launch owner
  decides, and the decision is recorded above.
- **It cannot substitute for the missing third-party audit** or for the formal verification the tree
  does not yet have.

## Maintenance and verification

- Re-run `bash scripts/check-launch-register.sh` after any edit. It checks the schema, the live
  citations, the coverage of every unsettled severity-C/H obligation, and the decision references,
  and it forbids restating an obligation status.
- `bash scripts/check-launch-register.sh --require-clear TESTNET` and
  `--require-clear MAINNET` are the release-checklist modes. They fail while any blocking row
  stands. They are deliberately not wired into the umbrella: a register that records a blocker must
  not make the ordinary test run permanently red.
- `bash scripts/check-launch-register.sh --self-test` plants a duplicate id, an invalid stage, a
  missing exit criterion, a dangling obligation id, a status restatement and an unranked open C/H
  obligation; every one must fail and be named, and a clean fixture must pass.
- A `RETIRED` row must carry a commit hash or a `measured YYYY-MM-DD` run in its evidence. A row
  moves to `RETIRED` only with the run quoted, never by editing the disposition.
