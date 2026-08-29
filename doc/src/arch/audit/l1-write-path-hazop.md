# L1 Capability Write-Path — HAZOP

Guide-word deviation analysis over the ρ-calculus trace (`l1-capability-write-path-trace.md`). Guide words:
NO / NOT / PART OF / AS WELL AS / REVERSE / OTHER THAN / EARLY / LATE, plus MORE / LESS (`sync-hazop.md:26-27`).
Each finding cites `file:line` on `linear-master` and maps to a write-path invariant (`wallet.md:810-819`).

## Central root cause

**The derived-rule DAG (invariant #5) cannot express a rule whose operand is another rule's output.**
`compute_derived` (`bin/dww/src/prover_impl.rs:394-478`) resolves operands via `operand` (`:481-489`), which
reads only **bound witness slots**. The closed rule table (`wallet.md:763-776`) states operands are
"0-based witness-slot indices". Therefore no `derived:` rule can reference an **intermediate** heap value —
exactly the zkas VM's notion of a derived value that depends on a prior derived value (`vm.rs:630` heap).

This single limitation is the root of V1, V2, and V3. The correct fix is a **derived-rule DAG extension**
(topological, intermediate-referencing), NOT a witness-count change.

---

## Findings

### V1 — MORE (witness) → purse `deposit`/`withdraw` circuit

- **Node:** purse circuit witness list.
- **Deviation:** adding a 23rd witness (`Base new_state_nonce`) to `deposit.zk`/`withdraw.zk` to separate the
  old/new nonce makes the zkas VM heap go out of bounds — `index out of bounds: the len is 28 but the index is 28`
  at `src/zk/vm.rs:1078` (`PoseidonHash`) / `:1690` (`ConstrainEqualBase`).
- **Mechanism:** the opcodes (compiled with the 23rd witness, referencing heap index 28) disagree with the
  actual heap size (28 elements = 6 constants + 22 witnesses); the witness declaration and opcode emission are
  out of sync. The VM heap is populated in `vm.rs:737-904` (constants first, then witnesses).
- **Invariant violated:** #5 (derived-rule DAG) / #7 (type preservation) — the nonce increment was pushed into
  the circuit as a *witness* instead of a *derived value*.
- **Structural fix:** do **not** add a witness. Express the produced nonce as an in-circuit derived value
  (`new_nonce = state_nonce + 1`) and give the generic prover a `derived:` rule that can consume it (V3's DAG
  extension). See §5.1/§5.2.

### V2 — SAME / NO (nonce separation) → purse `↓nullify` + `↓commit`

- **Node:** `derived:nullifier:15,0,7` and `derived:leaf:0,5,7` (`purse/manifest.toml:52,54`).
- **Deviation:** both rules bind nonce from the **same** slot (`state_nonce`, slot 7). The nullifier consumes
  the old nonce; the produced leaf also carries that nonce. A deposit→withdraw chain forces `state_nonce`
  constant, so `nullifier(deposit) == nullifier(withdraw)` — duplicate nullifier, second op rejected.
- **Evidence:** circuit note `purse/proof/deposit.zk:1-6` ("state_nonce is shared across old_leaf, new_leaf,
  and nullifier … requires distinct owner_secret values per operation"); harness papers over it with `os=43`
  (`test-harness/src/harness/purse.rs:86-88`).
- **Invariant violated:** #5 (derived-rule DAG) — the leaf/nonce relation is under-specified; the `↓nullify`
  single-use seam (`transfer-spec.md:64-74`) is defeated.
- **Structural fix:** separate old vs new nonce (box convention `box/manifest.toml:38-44`); nullifier binds the
  OLD nonce, leaf binds the NEW nonce (`new_nonce = old + 1`). Realized by the V3 DAG extension, not a witness.

### V3 — OTHER THAN (derived operand) → PN `RevokeV2` burn `↓signature`

- **Node:** `derived:signature_secret:0,0` (`promissory_note/manifest.toml:265`).
- **Deviation:** the circuit derives `signature_secret = poseidon(7, spend_secret, nullifier)` where
  `nullifier = poseidon(1, spend_secret, coin)` and `coin = poseidon(4, pub, value, asset_id, spend_hook,
  user_data, commitment_blind)` (`revoke.zk:39-53,82`). The manifest rule `signature_secret:0,0` computes
  `poseidon(7, secret, secret)` — both operands slot 0 — not `secret + nullifier`.
- **Mechanism:** `compute_derived` cannot reference the intermediate `nullifier`/`coin` (they are not witness
  slots). The rule table (`wallet.md:772,776`) has `coin` (6-arg) and `signature_secret` (2-arg) but no way to
  chain them.
- **Invariant violated:** #5 (derived-rule DAG), #6 (public-input declaration — RevokeV2 has no
  `public_inputs` array).
- **Structural fix:** derived-rule DAG extension — allow `derived:signature_secret:<secret>,<nullifier-rule>`
  to reference the output of `derived:nullifier:<secret>,<coin-rule>` and `derived:coin:<…>` as intermediates.
  Add RevokeV2 `public_inputs` (order per `client/revoke.rs:72-83`).

### V4 — PART OF (proof) → PN `transfer`/`redeem`

- **Node:** `[[functions]]` `proof_circuit` (single) vs entrypoint `transfer_get_metadata`
  (`entrypoint/mod.rs:393-432`) / `redeem_get_metadata` (`:941-971`).
- **Deviation:** `transfer` requires `len(inputs)` RevokeV2 burns + `len(outputs)` TransferV2 mints;
  `redeem` requires RevokeV2 + RedeemV2. `ManifestContractClient::build` (`contract_client.rs:389-446`) resolves
  one `proof_circuit` and emits one proof.
- **Invariant violated:** #2 (barb-cover) / #6 — the function's proof set is under-declared.
- **Structural fix:** multi-proof function schema — `proof_circuits: Vec<{circuit, role: burn|mint,
  note: consumed|produced}>`; `build` loops burn-first-then-mint, emitting `Vec<Vec<u8>>`.

### V5 — OTHER THAN (source) → PN RevokeV2 slot 8

- **Node:** `note:user_data_blind` (`promissory_note/manifest.toml`, RevokeV2 witness_map slot 8).
- **Deviation:** `user_data_blind` is a fresh per-spend blind (`client/revoke.rs:154-162`), not a note attribute
  (`note_schema` has no `user_data_blind`); `wallet.md:746-751` classifies fresh blinds as `blind:<name>`.
- **Invariant violated:** #7 (type preservation) — `note:<field>` binding fails ("note field 'user_data_blind'
  not found").
- **Structural fix:** `blind:user_data_blind` (already applied in-tree).

### V6 — NO (tx binding) → all actions

- **Node:** `WitnessSource::TxCommitment | TxNonce` binding.
- **Deviation:** bound to `pallas::Base::zero()` (`prover_impl.rs`), so the proof does not commit to the real
  transaction; the fee call publishes seed-derived `tx_commitment`/`tx_nonce`.
- **Invariant violated:** #4 (transaction binding).
- **Structural fix:** thread seed-derived `tx_commitment`/`tx_nonce` into `ProverContext`/`ResolvedCapProvider`
  (already applied in-tree; formalized as T6).

### V7 — AS WELL AS (cap selection) → all actions

- **Node:** `generate_proof` capability selection.
- **Deviation:** `caps.iter().find(|c| c.contract_id == target_cid)` silently picks the first held capability,
  ignoring `box_id`/`purse_id`/value/leaf — wrong note/secret/Merkle proof in a multi-cap wallet.
- **Invariant violated:** #2 (barb-cover selection — "never `caps[0]`").
- **Structural fix:** multi-cap ambiguity guard (error on >1 match with no disambiguator) — already applied;
  full disambiguation by `note:commitment` deferred to the PN burn-cap binding.

---

## Checklist (feeds Workstream 5)

1. Derived-rule DAG extension (intermediate-referencing, topological) — V1/V2/V3.
2. `derived:increment` (or `base_add` operand) — V2.
3. 3-arg `nullifier` + `coin` chain + `user_data_enc` + `signature_public` rules — V3.
4. RevokeV2 `public_inputs` declaration — V3.
5. Multi-proof `proof_circuits` schema + `build` loop — V4.
6. Purse in-circuit nonce increment (no new witness) — V1/V2.
7. Purse harness single-owner (remove `os=43`) — V2.
