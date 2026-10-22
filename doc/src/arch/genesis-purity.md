# Genesis Primitive Purity

> **Status:** the sweep and its structural gate are current at 2026-10-21. This page is a
> **register and a doctrine**, not a proof: it records the classification of every parent-facing
> surface in the nine genesis primitives, the one consumer-driven debt and its retirement, and the
> structural check that keeps the set closed. The honest limits are §7.

## 1. Doctrine — what "pure" means here

The nine genesis contracts are **o-cap primitives**, not applications. A primitive provides
generic mechanics; an application composes them through child calls it validates itself. The
composition rule is in [Composability](../contract/composability.md): a parent checks the child's
**function code and contract id** and compares the child's published terms against its own record.
The primitive neither knows nor names the parent.

| Class | Meaning | Rule |
|---|---|---|
| **A** | Generic primitive mechanic | Allowed: commitments, leaves, nullifiers, selectors, Merkle inclusion, proof metadata, encode/decode/constructors, init/exec/apply/metadata dispatch. |
| **B** | Generic composability helper or tag | Allowed, at most one or two helper families per primitive: a helper over caller-supplied values, or a **one-way function** of the object's id/contents (a tag) that publishes neither raw. Every B surface is registered here with its generic justification and a verifier. |
| **C** | Consumer-specific | **Forbidden in code.** A live C surface is allowed only as declared, expiring debt matching an existing L1 wire declaration or an explicit accepted-debt decision. |

The supporting normative statements:

- `doc/src/dev/testing/heavyweight-spec.md:42-43,99` — the nine are standalone; **zero
  cross-contract child calls**.
- `doc/src/contract/composability.md:53-59` — a parent validates the child's function code **and**
  contract id; the parent owns the child validation.
- `doc/src/arch/privacy.md:51-55` — every field that leaves the witness is declared with the
  verifier that reads it; a one-way function of the id is the allowed form.
- `doc/src/contract/multisig.md:17-20` — the primitive verifies the threshold; the composing
  contract checks for the approval capability.
- `doc/src/arch/contract-wasm-type-system.md:112` — a consensus contract SHALL NOT contain code
  for any other contract. The sweep applies the same rule to the whole genesis set.

**The helper budget.** A primitive may export generic helpers, but each one is a surface. The
register records every non-codec public helper; a third helper family in one primitive needs a
decision row rather than an addition.

## 2. The Nine

| Counter | Contract | Crate | Tier | Manifest sections |
|---|---|---|---|---|
| 2 | Deployooor | `dwow_deployooor_contract` | consensus | functions, capabilities, actions, trees, circuits |
| 3 | Promissory Note | `dwow_promissory_note_contract` | ecosystem | functions, capabilities, actions, trees, circuits, parameters |
| 4 | NativeToken | `dwow_native_token_contract` | consensus | functions, capabilities, actions, trees, circuits |
| 5 | Identity | `dwow_identity_contract` | ecosystem | functions, circuits, trees |
| 6 | Oracle | `dwow_oracle_contract` | ecosystem | functions, capabilities, actions, trees, circuits |
| 7 | Attestation | `dwow_attestation_contract` | ecosystem | functions, capabilities, actions, trees, circuits |
| 8 | Purse | `dwow_purse_contract` | ecosystem | functions, circuits, parameters, trees, capabilities, actions |
| 9 | Box | `dwow_box_contract` | ecosystem | functions, circuits, parameters, trees, capabilities, actions |
| 10 | MultiSig | `dwow_multisig_contract` | ecosystem | functions, capabilities, actions, circuits, trees |

Source: [Genesis Contracts](genesis.md) (the canonical list).

## 3. Surface register

The table below is the human view; the TOML block after it is the same ledger in the form
`scripts/check-genesis-primitive-purity.sh` parses. **Class A is enumerated per contract from the
manifest sections** — the gate expands them, so there is no hand-maintained list of fields to
drift. **B and C are per symbol**, because that is where a consumer can hide.

| ID | Contract | Item (`file:symbol`) | Class | Consumer(s) | Verifier | Decision |
|---|---|---|---|---|---|---|
| GP-B-001 | promissory_note | `src/contract/promissory_note/src/validation.rs:validate_child_contract_id` | B | all PN intermediaries | gate helper registration; composability.md | keep — compares a caller-supplied id to a caller-supplied id |
| GP-B-002 | promissory_note | `src/contract/promissory_note/src/validation.rs:validate_child_value_commit` | B | ~22 value-moving contracts | gate helper registration; composability.md | keep — Pedersen comparison; no consumer policy |
| GP-B-003 | promissory_note | `src/contract/promissory_note/src/validation.rs:validate_child_redeem_v1` | B | stablecoin | gate helper registration; second-caller test | keep — decodes RedeemV1 and returns the receipt commitments; zero-value receipt is a RedeemV1 circuit invariant, not issuer policy |
| GP-B-004 | promissory_note | `src/contract/promissory_note/src/model/mod.rs:RevokeSpendHookPayload` | B | any issuer; stablecoin is the reference receiver | `spend_hook` doc; gate foreign-token scan | keep — generic callback channel; the target is the commitment's caller-chosen `spend_hook` field |
| GP-B-005 | purse | `src/contract/purse/src/model/mod.rs:DepositParams.derived_purse_id` | B | none today (dormant) | `deposit.zk`/`withdraw.zk`/`balance.zk` constrain_instance; L1 gate | keep — one-way `poseidon(4, owner_pub, asset_id, purse_id)`; the precedent for a parent-visible object tag |
| GP-B-006 | box | `src/contract/box/src/model/mod.rs:TakeParams.box_tag` | B | escrow::ClaimV1 | `take.zk` constrain_instance; escrow claim controls; L1 gate | live — replaced the class C `contents_commit` |
| GP-B-007 | box | `src/contract/box/src/model/mod.rs:derive_box_tag` | B | box tests, escrow | second-caller test | live — `poseidon(4, box_id, contents_commit)`; no consumer name |
| GP-B-008 | purse | `src/contract/purse/src/model/mod.rs:derive_purse_id` | B | none today (dormant) | purse tag consistency test; three circuits constrain it | live — `poseidon(4, owner_pub, asset_id, purse_id)`; the tag's one definition |
| GP-C-001 | box | `src/contract/box/src/model/mod.rs:TakeParams.contents_commit` | C | escrow::ClaimV1 | `scripts/check-l1-wire-conformance.sh` | retired — replaced by GP-B-006/007; the L1 ledger is now empty |

### The ledger

<!-- genesis-purity-register:begin -->
```toml
version = 1

[[contract]]
name = "deployooor"
crate = "dwow_deployooor_contract"
counter = 2
tier = "consensus"
class_a = ["functions", "capabilities", "actions", "trees", "circuits"]

[[contract]]
name = "promissory_note"
crate = "dwow_promissory_note_contract"
counter = 3
tier = "ecosystem"
class_a = ["functions", "capabilities", "actions", "trees", "circuits", "parameters"]

[[contract]]
name = "native_token"
crate = "dwow_native_token_contract"
counter = 4
tier = "consensus"
class_a = ["functions", "capabilities", "actions", "trees", "circuits"]

[[contract]]
name = "identity"
crate = "dwow_identity_contract"
counter = 5
tier = "ecosystem"
class_a = ["functions", "circuits", "trees"]

[[contract]]
name = "oracle"
crate = "dwow_oracle_contract"
counter = 6
tier = "ecosystem"
class_a = ["functions", "capabilities", "actions", "trees", "circuits"]

[[contract]]
name = "attestation"
crate = "dwow_attestation_contract"
counter = 7
tier = "ecosystem"
class_a = ["functions", "capabilities", "actions", "trees", "circuits"]

[[contract]]
name = "purse"
crate = "dwow_purse_contract"
counter = 8
tier = "ecosystem"
class_a = ["functions", "circuits", "parameters", "trees", "capabilities", "actions"]

[[contract]]
name = "box"
crate = "dwow_box_contract"
counter = 9
tier = "ecosystem"
class_a = ["functions", "circuits", "parameters", "trees", "capabilities", "actions"]

[[contract]]
name = "multisig"
crate = "dwow_multisig_contract"
counter = 10
tier = "ecosystem"
class_a = ["functions", "capabilities", "actions", "circuits", "trees"]

[[surface]]
id = "GP-B-001"
contract = "promissory_note"
file = "src/contract/promissory_note/src/validation.rs"
symbol = "validate_child_contract_id"
class = "B"
kind = "helper"
consumers = ["all PN intermediary contracts"]
verifier = "gate helper registration; composability.md"
decision = "keep — compares a caller-supplied id to a caller-supplied id"
status = "live"

[[surface]]
id = "GP-B-002"
contract = "promissory_note"
file = "src/contract/promissory_note/src/validation.rs"
symbol = "validate_child_value_commit"
class = "B"
kind = "helper"
consumers = ["all PN value-moving contracts"]
verifier = "gate helper registration; composability.md"
decision = "keep — Pedersen comparison; no consumer policy"
status = "live"

[[surface]]
id = "GP-B-003"
contract = "promissory_note"
file = "src/contract/promissory_note/src/validation.rs"
symbol = "validate_child_redeem_v1"
class = "B"
kind = "helper"
consumers = ["stablecoin"]
verifier = "gate helper registration; second-caller test"
decision = "keep — decodes RedeemV1 and returns the receipt commitments; zero-value receipt is a circuit invariant, not issuer policy"
status = "live"

[[surface]]
id = "GP-B-004"
contract = "promissory_note"
file = "src/contract/promissory_note/src/model/mod.rs"
symbol = "RevokeSpendHookPayload"
class = "B"
kind = "callback"
consumers = ["any issuer; stablecoin is the reference receiver"]
verifier = "spend_hook doc; gate foreign-token scan"
decision = "keep — generic callback; the target is the commitment's caller-chosen spend_hook"
status = "live"

[[surface]]
id = "GP-B-005"
contract = "purse"
file = "src/contract/purse/src/model/mod.rs"
symbol = "DepositParams.derived_purse_id"
class = "B"
kind = "tag"
consumers = ["none today (dormant)"]
verifier = "deposit.zk/withdraw.zk/balance.zk constrain_instance; L1 gate"
decision = "keep — one-way poseidon(4, owner_pub, asset_id, purse_id)"
status = "live"

[[surface]]
id = "GP-B-006"
contract = "box"
file = "src/contract/box/src/model/mod.rs"
symbol = "TakeParams.box_tag"
class = "B"
kind = "tag"
consumers = ["escrow::ClaimV1"]
verifier = "take.zk constrain_instance; escrow claim controls; L1 gate"
decision = "add — poseidon(4, box_id, contents_commit); replaces GP-C-001"
status = "live"

[[surface]]
id = "GP-B-007"
contract = "box"
file = "src/contract/box/src/model/mod.rs"
symbol = "derive_box_tag"
class = "B"
kind = "helper"
consumers = ["box tests", "escrow"]
verifier = "second-caller test"
decision = "add — poseidon(4, box_id, contents_commit); no consumer name"
status = "live"

[[surface]]
id = "GP-B-008"
contract = "purse"
file = "src/contract/purse/src/model/mod.rs"
symbol = "derive_purse_id"
class = "B"
kind = "helper"
consumers = ["none today (dormant)"]
verifier = "purse tag consistency test; deposit.zk/withdraw.zk/balance.zk constrain_instance"
decision = "live — poseidon(4, owner_pub, asset_id, purse_id); the tag's one definition"
status = "live"

[[class_c]]
id = "GP-C-001"
contract = "box"
file = "src/contract/box/src/model/mod.rs"
symbol = "contents_commit"
class = "C"
l1_declared = true
l1_key = "box/Take/1/contents_commit"
consumer = "escrow::ClaimV1"
expiry = "retired by the GP-B-006/007 tag change, 2026-10-21"
status = "retired"
```
<!-- genesis-purity-register:end -->

## 4. Decision — how box::Take names a box

**Decision (2026-10-21): a generic one-way tag replaces `contents_commit` on the wire.**
`box_tag = poseidon_hash([DRK_POSEIDON_DOMAIN_COMMITMENT, box_id, contents_commit])`, computed and
constrained in `take.zk`, published as a public input. `box_id` and `contents_commit` remain
witness values (`note:object_id`, `note:user_data`); neither is on the wire. A parent that knows
the box's id and contents commitment derives the tag with `derive_box_tag` and compares it.

`escrow::ClaimV1` is the first consumer: its claim box is created with
`box_id = Escrow::derive_claim_box_id(id, value)` and
`contents = Escrow::derive_claim_box_contents(id, value)`; the claim compares
`derive_box_tag(claim_box_id, claim_box_contents)`. The derivations are the consumer's; the tag is
the primitive's generic surface.

**Rejected alternatives.**

- **Keep `contents_commit`.** It is the single declared L1 wire leak, its publication is qualified
  by privacy.md, and three documents say it is on the wire for escrow. It leaves `OBL-C184` and
  LR-11 open and does not answer the request.
- **Remove the escrow-box composition.** It closes the leak but deletes the one-shot claim model
  rather than giving the primitive a generic object tag.
- **An owner-based tag.** The box's `owner_pub` is `poseidon(7, owner_secret)` (a field); a parent
  holds a curve public key, not that field, and the escrow's claim box is not owned by its
  `seller_pubkey`. It needs a new consumer field and does not strengthen the binding.

**Privacy, stated honestly.** The tag hides the id and contents from an observer without the
preimages, and it is strictly less information than publishing `contents_commit`. For escrow both
preimages are derivable from the public `CreateEscrowParamsV1` (`escrow_id`, `value`), so an
observer who computes them can still link the claim to the escrow. The tag does not create secrecy
the consumer did not have; it removes the literal commitment and protects applications whose
contents preimage is secret. This is a one-way pseudonym, not a proof of hiding.

## 5. Dormant and single-consumer surfaces

- **Purse `derived_purse_id`** is a generic one-way tag on all three circuits and the precedent
  for the box tag. No consumer decodes purse params today; it stays as the composability API, and
  a consistency test pins the derivation on Deposit, Withdraw and Balance.
- **PN `validate_child_redeem_v1`** has one caller (stablecoin) but decodes a generic RedeemV1 and
  enforces no issuer policy. It stays as the RedeemV1 accessor; a neutral second-caller test pins
  that it is not stablecoin-shaped.
- **PN spend-hook callback** is the generic issuer-notification channel. Stablecoin is the
  reference receiver; the target comes from the commitment's `spend_hook`, not a hardcoded id.
- **Identity possession** is the holder's signature (`OBL-C171`), not a `Box::Put` delegation.
  Five stale comments and a long note said otherwise until 2026-10-21; they now describe the
  signature and keep the box-wire analysis as the reason the delegation was not used.

## 6. Findings outside purity (recorded, not fixed here)

- **`identity::revoke_capability` did not verify `params.signature`** — anyone could write a
  revocation marker for any `(capability_id, holder_pub)`. This was an authorization hole in a
  genesis primitive, not consumer-specific logic (`OBL-C255` / LR-22). **Fixed 2026-10-21:** the
  call carries the credential it revokes; the host loads the capability and the credential, requires
  the credential's stored holder key to equal the named `holder_pub`, and accepts the Schnorr
  signature over `CAPABILITY_REVOKE_DOMAIN ‖ capability_id ‖ holder_pub ‖ credential_nullifier ‖
  tx_nonce ‖ reason` from the **holder** or the capability's **issuer**. The marker is
  `poseidon(CAPABILITY_REVOKE_KEY_DOMAIN, capability_id, holder_x, holder_y)` and
  `VerifyCapabilityV1` reads it (after the holder signature) and refuses with `CapabilityRevoked`
  (`Custom(22)`), so revocation denies use, not just writes. The first repair was issuer-only and
  left the marker unread; the red-team audit reopened it, and this is the corrected state.
- **`dao_escrow` carries a frozen local copy of identity's old `CapabilityProof`** while identity's
  model has diverged. That is consumer-side drift, not a leak into a primitive; recorded for a
  follow-up.

## 7. Verification and honest limits

**Gate:** `scripts/check-genesis-primitive-purity.sh` (checks 1–6 in its header, with a planted-
defect `--self-test`). It checks set closure, foreign code/manifest dependencies, foreign ids and
child-call tokens, registered validation helpers, the L1 wire ledger, and register/source
consistency. It is wired into `run-all-tests.sh` and `report-status.sh`.

**This is the record of the 2026-10-21 batch; the tree has moved since (see the next paragraph).**
**Measured 2026-10-21, after the implementation and the single re-roll.** The gate prints
`PASS: nine genesis primitives — no foreign code/manifest dependency, no foreign id, no
cross-contract child call, every validation helper registered, L1 ledger and register agree`;
its six-defect `--self-test` reports every planted defect and the clean twin passes.
`check-l1-wire-conformance.sh` prints `PASS: 0 declared wire leak(s) … none new, none stale`.
All 32 artifacts are fresh. The five genesis inputs this change moves — `identity` and the four
manifest-repaired contracts (`deployooor`, `oracle`, `attestation`, `multisig`) — were rebuilt from
clean twice and are byte-identical; the nine genesis wasms are 9/9 CLEAN.
`bin/dwowd/genesis_hash.txt` was observed stale against the new identity WASM, re-recorded **once**
from the genesis path's own computed hash, and the currency test plus wrong-hash control then report
**2 passed, 0 failed**. The batch measured **six** of the nine genesis heavyweights green after the
identity change — native_token (911.98 s; the `OBL-C257` fixture repair landed, so the row is no
longer red), promissory_note (1167.60 s), purse (834.65 s), box (739.25 s), identity (847.15 s),
attestation (1489.12 s). `multisig`, `oracle` and `deployooor` were **not re-run in this batch**: it
was stopped at the user's direction to finish the register work, so their last recorded measured
runs remain the previous batch's (134.59 s, 314.02 s and 103.67 s, all green) and their artifacts
are fresh and pure. These timings are longer than the previous batch's because this one ran the
debug test binary rather than `make test`'s release profile. The earlier batch also found and fixed
`OBL-C256` (the `no-entrypoint` feature was dead in four contract crates, so host links exported
duplicate WASM symbols).

**2026-10-22: code-complete, not yet built or pinned.** An adversarial review of the nine primitives
then landed code changes in `deployooor` (WASM content commitment and the two missing genesis
manifests), `identity` (capability-id derivation, issuer schema authorization, issuance validation,
manifest declarations), `attestation` (the vacuous `check_not_revoked` endpoint, circuit, client and
manifest entry removed), `multisig` (`create_group.zk` binds the group id to the first member and
counts), `purse` (the leaf commits to `asset_id`), `promissory_note` and `native_token` manifests
(`proof_circuits` lists), plus `src/sdk` (`ManifestFunction::proof_circuits`, the optional asset
operand in the wallet's leaf rule). **None of it has been built by the agent, and the pin has not
moved.** `src/sdk` is in all 32 contracts' `SOURCE_MANIFEST`, so **all 32 artifacts are stale** and
`bin/dwowd/genesis_hash.txt` is stale by construction. The build, the byte-identical rebuild, the
pin re-record and the nine heavyweight specs are the pipeline's steps; this document must not be
read as claiming them for this tree.

**What it cannot establish.**

- **Semantics.** It finds structural leakage; it cannot tell whether a generic-shaped helper
  encodes a policy for one caller. The identity comments were prose outrunning code, and a gate
  cannot read prose.
- **Indirection.** Macros, aliases, trait objects, code generation and raw-index dispatch can hide
  a child call or a consumer id.
- **The tag's privacy.** It is a one-way pseudonym; escrow's link is computable from public create
  params.
- **Production genericity from a caller count.** A second-caller test proves a helper is not
  hard-wired; it does not prove a second production consumer.
- **Register truth.** The gate checks that registered symbols resolve and that classes have the
  required declarations; the class and the consumer list are human decisions.
- **Third-party review.** This register and gate are internal; they do not substitute for an
  external review of the intended semantic purity.
