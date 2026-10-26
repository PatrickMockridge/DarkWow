# Decisions, accepted risks and interpretations

These are not defects and are not tracked. A defect lives in
[`verification-hazop.md`](verification-hazop.md) and is OPEN until the code and its control
land; nothing here can be cited to green a defect. The full analysis of each entry is in git
(`git log -S "<id>" -- doc/src/arch/verification-hazop.md`).

| ID | Kind | Statement of record |
|---|---|---|
| OBL-C11 | restated | No two outputs in one call carry the same commitment |
| OBL-C271 | decision | The protected-fund member exit is disabled: a parent contract cannot verify a member's share over the genesis `Purse` (its amounts are off-wire; `derived_purse_id` is the only public field), and `ProtectedFund.members` has no identity or population path. `ExitV1` refuses with `CustodyModelUnavailable`; the fund is a governance wrapper over externally-held notes, not a custodian. Custody requires a primitive with a parent-verifiable withdrawal amount. |
| OBL-C266 | decision | RandomX runs in cache (light) mode on every path: each height's key derives a 256 MB cache and hashes are computed from it; the ~2 GB dataset is never allocated. The memory-hardness is bounded by the cache, not the dataset. This is the network's design choice — low-memory nodes over maximum ASIC resistance — and `consensus-coinbase.md` is corrected to state it. Not a defect: the code and the design agree; only the document was false. |
| OBL-C272 | decision | Stablecoin redemption is disabled: the pool does not custody collateral (`OpenPositionV1` accepts any PN transfer whose value commitment matches and never checks the recipient), so `RedeemStableV1` decremented `total_collateral` and recorded a redemption while no child paid the redeemer. It now refuses with `CustodyModelUnavailable` before any state read. Paying it needs the same parent-verifiable custody primitive as OBL-C271. |
| OBL-C113 | decision | The authority one subscription operation consumes is not the authority another has not yet used — a recurring payment can be paid again |
| OBL-C125 | decision | A binary in this tree compiles, or a gate says it does not |
| OBL-C128 | restated | A manifest's `proof_circuit` names a circuit that exists — the field the SDK loads a proving key by |
| OBL-C134 | accepted | The plaintext status of native_token's three consensus functions (`PoWRewardV1` 0x05, `FeeCollectV1` 0x06, `UncleMintV1` 0x07) is stated where each is tabulated, and no text still presents the deleted ZK-gated coinbase as current |
| OBL-C138 | decision | A prohibited harness anti-pattern is either absent or declared, so the scanner's report is a verdict rather than noise |
| OBL-C149 | dissolved | Register-shape requirement — the tracker's own five-column schema replaces it. |
| OBL-C154 | decision | A requirement a contract imposes on a call is a requirement a caller can satisfy: the child a check reads is at a slot the call can carry it in, and a gate reads state some path can write |
| OBL-C163 | decision | A refusal names the check that refused: an error's identity is the condition it reports, not the function it was first written in |
| OBL-C164 | decision | Every path that can move value applies the same mode gate: an endpoint that pays out of a pool the mode says is not spendable is the mode not applying |
| OBL-C167 | decision | A state transition that needs no approval is one whose authority the readers of the guard will assume is there: an unauthenticated write to a governance record is a write anyone can make |
| OBL-C18 | restated | Chain traversal in `get_next_work_required` is bounded, or the cache that makes it O(1) exists |
| OBL-C188 | decision | An endpoint that records a principal names one the caller proved: a proof-less create that takes a public key as a parameter lets any caller create an object attributed to anyone |
| OBL-C19 | restated | **Premise restated 2026-10-26.** The budget that terminates a call is the deterministic gas budget (`GAS_LIMIT` in the metering middleware; an exhausted call traps and the host names it gas exhaustion, `vm_runtime.rs:940-950`), not wall-clock time. A wall-clock trap would make block acceptance depend on host load, and consensus must be a pure function of local data; the 30 s comparison stays a diagnostic warning (`:902-922`). The original "terminated, not merely warned" claim was false as written. Gas is a wall-clock bound only where the cost model prices the work: bulk-memory ops are flat-priced, so the wall-clock exposure survives and is tracked as `OBL-C277`. |
| OBL-C200 | decision | A rejection names the check that refused it: an error code identifies its condition, so a caller can act on it and an operator can diagnose it from the code alone |
| OBL-C21 | accepted | Bridge verification is real cryptography at every chain, not a shape check: Monero DLEq, Ethereum MPT, Zcash Groth16, Aztec PLONK all verify rather than returning "not yet implemented" |
| OBL-C216 | restated | Fee values are split by kind: the fee window's congestion state and the per-contract risk parameters are chain state, initialized at genesis and window-updated; the economic coefficients are protocol constants, identical in every build |
| OBL-C222 | restated | The host-import ACL rule is absolute for every host function except the one the rule itself names: `drk_log` is ungated and void-returning by design, and the reason is written where the rule is |
| OBL-C224 | restated | The rule against self-invocation is stated against the model the host actually runs: no import creates a call, so the call graph is static and the recursion the rule names cannot be constructed |
| OBL-C225 | restated | A block header's nullifier-set commitment, where one exists, is verified against the set the block writes — or the field is documented as reserved and unverified |
| OBL-C230 | restated | A document's account of what a circuit proves names a constraint the circuit has; where a check moved to the host, the document says so and says where |
| OBL-C231 | restated | A deployment's selected stablecoin model is the model its engine runs — or the document says which models exist and that the selecting field is inert |
| OBL-C233 | restated | A document's account of what an initializer registers names state the contract actually writes, and a dead constant is not offered as the explanation |
| OBL-C234 | restated | A document's account of how a signer is bound names the mechanism that binds it |
| OBL-C250 | decision | Deployooor stores no WASM content hash: the upstream contract has none. The local chunked-Poseidon commitment and its `info` write are deleted with the port, not repaired. Genesis content is authenticated by the pinned genesis hash; per-deploy content by the deployed WASM the host stores. |
| OBL-C32 | accepted | `Message::BARBS` is enforced, or the net-layer quarantine is lifted with the reason recorded |
| OBL-C66 | accepted | Caribina anchors a block to a *settled* Arweave block. **ACCEPTED-WITH-REASON 2026-09-22, premise restated.** Settlement is not checked, and **cannot be**: it lives on another chain and consensus must be a pure function of local data — t… |
| OBL-C76 | decision | A test that does not run in the configuration used to claim verification is not verification. Three mechanisms hide tests in this tree and they compound **(a fourth was found on 2026-09-23 — a module gated by its *parent* takes its `#[cf… |
| OBL-C80 | decision | Merge mining is off by default. A deployment that enables it must supply a verifier: with `monerod_url` unset the admission policy checks nothing, and the container refuses to start merge mining without one (OBL-C95). The default therefore has nothing to verify, and an enabled deployment cannot silently run unverified. |
| OBL-C82 | decision | Interest accrued on the pool's debt is determined by the contract's configured rate and the elapsed time since the last accrual, and the accrual is authorized by the authority registered at init |
| OBL-C85 | accepted | The per-uncle Pedersen audit commitment `C_uncle_i = u_i·G_v + r_i·G_r` exists where the spec says it does — as EC points in the **stored cumulative supply chain** — so the subtractive mass balance `C_effective + Σ C_uncle_i = C_base` is… |
| OBL-C95 | decision | A container that enables merge mining supplies the verifier, or does not start |
| OBL-C96 | decision | A milestone's payment is the amount the job records for that milestone, and a completed job's releases reconcile with the job's total |
| OBL-T17 | accepted | Every capability type proved in Lean is constructible by the Rust `wallet_construct`, or its unconstructibility is recorded with its cause |
| OBL-T20 | decision | §6.5's transaction status lifecycle is modelled — including which of the two observers may take which edge — and the deployment carries no transaction status at all, while the specification makes carrying one a `SHALL` |
| OBL-T23 | decision | A reorg reverses a block's contributions to the confirmed state — its observed nullifiers stop being spent, its discovered commitments stop being recognized — and the model has no such operation |
| OBL-T7 | restated | Every public input of the circuit for `(r, s)` is `constrain_instance`-derived — **that is the *strict* reading, and the tree does not meet it**; what the tree enforces is the checker's four-verdict rule, and the cell below says by how much |
| OBL-T9 | accepted | The barb alphabet distinguishes the types it claims to: distinct capability types have distinct, non-subsumed barb sets, so a barb set identifies a capability |
| OBL-Z13 | decision | A governance datum the reports are *about* is either derived from chain state or is recorded as the reporter's claim |
| OBL-Z17 | restated | A capability check authorizes the holder it names, and only for the capability it was issued for |
| OBL-Z4 | restated | A pubkey derived by `ec_mul_base` + `ec_get_x/y` is bound by `constrain_equal_base` before being exposed |
| OBL-Z5 | decision | The deliberately free witnesses are enumerated and each carries its host-side obligation |
| OBL-Z6 | decision | The Orchard-tree hash is **Sinsemilla**: 10-bit altitude ‖ two 255-bit halves under `"z.cash:Orchard-MerkleCRH"`, depth 32, empty leaf **2** |
| OBL-Z8 | accepted | ZK binaries are well-formed |

