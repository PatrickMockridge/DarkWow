# DAO-Escrow Contract

A DAO-governed endowment. Members pay premiums into an endowment; claims against it are authorised by a
**MultiSig group** the endowment's owner installs once, through a ZK-proven `UpdateV1`.

> **Read this first if you are here from the older revision of this page.** That revision documented an
> OCap/Identity governance model, a `governance_active` feature toggle, four delegated governance roles
> exercised through `Box::TakeV1`, and Purse-backed pool balances. **None of those exist in the contract.**
> The record that supports them was migrated to MultiSig groups and the documentation was not; what
> follows is the code as it is, and [What is not implemented](#what-is-not-implemented) names each gap.

## Composition

The contract composes with genesis primitives through **child calls it validates**, not through storage it
shares:

- **`promissory_note::TransferV1` (0x04)** — every value-moving endpoint requires one as a child and
  checks the child's target contract id, its function selector, and that one of its outputs commits to
  `poseidon_hash(value, dao_escrow_bulla)`. The transfer *is* the money movement; this contract's apply
  only rewrites the endowment record.
- **`multisig::FinalizeV1` (0x03)** — the governance approval (see
  [Governance](#governance-the-multisig-group)).
- **`identity::VerifyCapabilityV1` (`0x06`)** — required by `VerifyMemberCapabilityV1` only, and its routing
  check is **skipped** while `identity_cid` is the zero placeholder `init_contract` seeds, so the child's
  contract id is not compared to anything. The selector is the only binding.
- **DrainProtection** — an association, not a call: `drain_protection_enabled` and
  `drain_protection_bulla` are two fields on the record. The contract never addresses the DrainProtection
  contract.

The `Purse::DepositV1`/`WithdrawV1` and `Box::TakeV1` composition the older revision described is **not
implemented** — see [What is not implemented](#what-is-not-implemented).

## Governance: the MultiSig group

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  DAO-Escrow governance                                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  OFF-CHAIN: the members' group is created in the MultiSig contract           │
│     CreateGroupV1 (0x01) → group_id                                          │
│              │                                                               │
│              ▼                                                               │
│  ONCE, BY THE OWNER: UpdateV1 (0x01)                                         │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │  ZK: SetGovernanceConfigV2                                   │         │
│     │    owner_pub = ec_mul_base(owner_secret, NULLIFIER_K)        │         │
│     │    constrain_instance(owner_pub_x, owner_pub_y)              │         │
│     │    owner_nullifier = poseidon_hash(1, ox, oy,                │         │
│     │                                   owner_secret, bulla)       │         │
│     │    constrain_instance(owner_nullifier)                       │         │
│     └──────────────────────────────────────────────────────────────┘         │
│     exec compares the proven coordinates to the record's owner,              │
│     records owner_nullifier (replay is refused), and writes the group id     │
│     into a field that was zero. A record that already has a group is         │
│     refused: THERE IS NO ROTATION.                                           │
│              │                                                               │
│              ▼                                                               │
│  PER ACTION: a child multisig::FinalizeV1 (0x03) whose decoded               │
│     group_id == endowment.multisig_group_id                                  │
│     message_hash == governance_message(role, action_id)                      │
│              │                                                               │
│              ▼                                                               │
│  six gated endpoints (roles 1-6)                                             │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**The owner is proved, not asserted.** `UpdateV1` carries a ZK proof: the exposed owner coordinates are
bound to knowledge of `owner_secret`, and `owner_nullifier` is derived deterministically in
`(owner_secret, dao_escrow_bulla)` so it can only be used once. A plaintext comparison of public keys —
which is what `WithdrawV1`'s owner path still does — would gate nothing, because a public key is published
in order to receive funds.

**No rotation.** Once a group is installed there is no path back to owner-key control and no path to a
different group: an `UpdateV1` against a record that already carries a group is refused
(`GovernanceAlreadyActive`). This is a design decision, and it is the opposite of what the older revision
claimed.

### The approval's message is role-tagged

```rust
governance_message(role: u8, action_id: pallas::Base) -> pallas::Base
    = poseidon_hash([DOMAIN_GOVERNANCE_APPROVAL = 11, role, action_id])
```

| role | endpoint | action id |
|------|----------|-----------|
| 1 | `ProposeClaimV1` (0x07) | `claim_id` |
| 2 | `VoteClaimV1` (0x08) | `claim_id` |
| 3 | `ResolveDisputeV1` (0x0c) | the contract's own `dispute_id` derivation |
| 4 | `EndowmentWithdrawV1` (0x04) | `(bulla, value, recipient_x)` |
| 5 | `TreasurySpendV1` (0x05) | `(bulla, value, recipient_x)` |
| 6 | `WithdrawV1` (0x03) | `(bulla, value, recipient_x)` |
| 7 | `EnableDrainProtectionV1` (0x06) | `(bulla, drain_protection_bulla)` |
| 8 | `RegisterCapabilityRequirementV1` (0x0a) | `(bulla, capability_id)` |
| 9 | `DeactivateCapabilityRequirementV1` (0x10) | `(bulla, capability_id)` of the stored record |
| 10 | `CancelClaimV1` (0x0d) | `claim_id` |

Roles 7–10 were added on 2026-09-27: those four endpoints performed **authenticated state changes with no
authorization check at all** (`OBL-C152`). Two of them bind a value rather than an id, because their
governance branch is reached with no id to bind — the same reason the money endpoints use a triple.

**One boundary is recorded rather than solved**: roles 8 and 9 bind the *capability* and not the role key,
because the role is a `Vec<u8>` table key and `pallas::Base::from_repr` takes exactly 32 canonical bytes.
A group approval for one role's requirement can therefore be presented for another role's — stated in
`governance_role::REGISTER_CAPABILITY_REQUIREMENT` rather than left to be discovered.

Two properties are structural rather than stylistic:

- **The role tag stops approval reuse.** A MultiSig approval is spend-once — its nullifier is
  `H(1, member_secret, group_id, message_hash)`, consumed by `FinalizeV1`. `propose_claim` and `vote_claim`
  both key on `claim_id`, so without the role tag a vote's approval would re-use the proposal's.
- **The three money endpoints use a triple, not an id.** Their governance branch is reached exactly when
  `proposal_id == 0`, so an id-based message would be zero for every call and one approval of zero would
  authorise every withdrawal from that endowment forever. The action id is refused if zero.

## Entrypoints

| Opcode | Function | ZK circuit | Child calls | Authorization |
|--------|----------|-----------|-------------|---------------|
| `0x00` | `InitializeV1` | `InitV2` (4) | none | none; a duplicate bulla is refused |
| `0x01` | `UpdateV1` | `SetGovernanceConfigV2` (5) | none | **owner, proved**; one-shot `owner_nullifier` |
| `0x02` | `PayPremiumV1` | `PayPremiumV2` (2) | `TransferV1` at slot 0 | none in-contract |
| `0x03` | `WithdrawV1` | — | `TransferV1` at 0, `FinalizeV1` at 1 when governance active | owner pubkey, or role 6 |
| `0x04` | `EndowmentWithdrawV1` | — | `TransferV1` at 0, `FinalizeV1` at 1 when governance active | role 4, or an `Approved` proposal |
| `0x05` | `TreasurySpendV1` | — | `TransferV1` at 0, `FinalizeV1` at 1 | role 5, behind a mode gate that cannot pass |
| `0x06` | `EnableDrainProtectionV1` | — | `FinalizeV1` at 0 | role 7 |
| `0x07` | `ProposeClaimV1` | `ProposeClaimV2` (3) | `FinalizeV1` at 0 | role 1 |
| `0x08` | `VoteClaimV1` | `VoteClaimV2` (3) | `FinalizeV1` at 0 | role 2 |
| `0x09` | `ExecuteClaimV1` | — | `TransferV1` at 0 | proposal must be `Approved` |
| `0x0a` | `RegisterCapabilityRequirementV1` | — | `FinalizeV1` at 0 | role 8 |
| `0x0b` | `VerifyMemberCapabilityV1` | `VerifyMemberCapabilityV2` (3) | `identity::VerifyCapabilityV1` (`0x06`) | the proof; the routing check is skipped while `identity_cid` is zero |
| `0x0c` | `ResolveDisputeV1` | `ResolveDisputeV2` (3) | `FinalizeV1` at 0 | role 3 |
| `0x0d` | `CancelClaimV1` | — | `FinalizeV1` at 0 | role 10 |
| `0x0e` | `SetGovernanceConfigV1` | — | — | **retired no-op** |
| `0x0f` | `SetGovernanceActiveV1` | — | — | **retired no-op** |
| `0x10` | `DeactivateCapabilityRequirementV1` | — | `FinalizeV1` at 0 | role 9 |

`(n)` after a circuit is its `constrain_instance` count, which is what the metadata arm must publish.

Every endpoint whose metadata arm is absent from `get_metadata` must return an **encoded** empty
`zk_public_inputs` — not a bare `vec![]`, which the host decodes as a rejection signal and which made
every non-ZK function uncallable for a week (`OBL-C77`).

## Claims

`ProposeClaimV1` writes a `Proposal` in `ProposalState::Pending`, with a voting window and an execution
deadline. `VoteClaimV1` increments one tally, refuses a second vote from the same nullifier, and expires
the proposal automatically when the window has closed. `ExecuteClaimV1` requires a `TransferV1` child and
checks that the proposal is `Approved` and matches the `(bulla, value, recipient)` of the call.

**Nothing in the contract writes `ProposalState::Approved`.** The only occurrence of that variant outside
the enum's definition is the read in `verify_proposal_approved`, so `ExecuteClaimV1` and the
`proposal_id` path of the three money endpoints are unreachable as the contract stands — recorded as
`OBL-C154`.

`CancelClaimV1` requires `proposal.state == Pending` and compares `proposal.proposer_pubkey` to
`params.proposer_pubkey`. Both are public values, so the comparison admits any caller who knows the
proposer's key (`OBL-C152`).

## Dispute resolution

`ResolveDisputeV1` (0x0c) is the arbitrator path:

```
1. Oracles push values off-chain-sourced (oracle::PushValueV1) and attest (attestation::CreateAttestationV1)
2. An arbitrator calls ResolveDisputeV1 with a list of attestation ids, a payout and a recipient
3. The contract:
   a. requires a FinalizeV1 approval over role 3 and the derived dispute_id
   b. consumes the named attestation ids in the nullifiers tree
   c. carries the resolution record to apply
4. dispute_id = poseidon_hash(proposal_id, attestation_count, payout_recipient_x)
```

The `dispute_id` is the contract's own derivation from the call's own contents — not a caller-supplied id —
so the approval and the anti-replay key cannot be chosen independently of the call. The child count is a
**minimum**, not an exact match: the contract's apply validates no attestation child itself, so the
attestation verification this endpoint's comment describes is not, today, performed here.

## ZK circuits

| Circuit | `constrain_instance` order |
|---------|---------------------------|
| `init.zk` (`InitV2`) | `dao_bulla`, `tx_binding`, `tx_nonce`, `endowment_bulla` |
| `pay_premium.zk` (`PayPremiumV2`) | `tx_binding`, `tx_nonce` |
| `propose_claim.zk` (`ProposeClaimV2`) | `tx_binding`, `tx_nonce`, `claim_commit` |
| `vote_claim.zk` (`VoteClaimV2`) | `tx_binding`, `tx_nonce`, `vote_nullifier` |
| `verify_member_capability.zk` (`VerifyMemberCapabilityV2`) | `tx_binding`, `tx_nonce`, `capability_commit` |
| `resolve_dispute.zk` (`ResolveDisputeV2`) | `tx_binding`, `tx_nonce`, `resolution_commit` |
| `set_governance_config.zk` (`SetGovernanceConfigV2`) | `owner_pub_x`, `owner_pub_y`, `owner_nullifier`, `tx_binding`, `tx_nonce` |

All seven are compiled and their `.zk.bin` files are committed. The order above is the order the metadata
arm publishes and the order the client's `to_vec` commits to; all three must agree, and
`scripts/check-circuit-metadata-alignment.sh` is the instrument that compares them.

`SetGovernanceConfigV2` is called by **`UpdateV1` (0x01)**. `manifest.toml` declares it there; the retired
`SetGovernanceConfigV1` (0x0e) declares no proof, because it does nothing.

## Database trees

| Tree | Written by | Read by |
|------|-----------|---------|
| `info` | `init_contract` (version, contract ids) | the child-call routing checks, `UpdateV1` |
| `bullas` | `initialize_apply_v1` (non-empty marker) | `initialize_v1`'s duplicate guard |
| `endowment` | every state-writing endpoint | every endpoint that loads the record |
| `membership` | `pay_premium_apply_v1` | `pay_premium_v1`'s duplicate guard |
| `proposals` | `propose`/`vote`/`execute`/`cancel` apply | the same endpoints' exec |
| `votes` | — | — |
| `capability_requirements` | `register`/`deactivate` apply | `deactivate` exec |
| `disputes` | `resolve_dispute_apply_v1` | its anti-replay guard |
| `nullifiers` | `update`/`vote`/`resolve_dispute`/`cancel` apply | `update`'s reuse check, `vote`'s double-vote check |
| `governance` | — | — |

`votes` and `governance` are declared in `manifest.toml` and in `lib.rs` but are not touched by any code
path.

## Trust model

| Aspect | What actually protects it |
|--------|--------------------------|
| Owner-only state change | A ZK ownership proof, one-shot per `(owner_secret, bulla)` |
| Governance | A MultiSig group's threshold, enforced by `FinalizeV1` in the child — not re-counted here |
| Approval reuse | Role-tagged message + the approval's own spend-once nullifier |
| Double vote | `vote_nullifier` recorded in the nullifiers tree, checked in exec |
| Dispute replay | `dispute_id` derived from the call, recorded in the disputes tree |
| Value movement | The `promissory_note::TransferV1` child, checked for target, selector and value commitment |
| Treasury / endowment balances | **Nothing in this contract** — see below |
| Owner-key withdrawal (`WithdrawV1`) | **A public-key comparison, which gates nothing** (`OBL-C152`) |

## DrainProtection

The contract can record an association with a DrainProtection instance: `EnableDrainProtectionV1` (0x06)
sets `drain_protection_enabled = true` and `drain_protection_bulla`. It does **not** call the
DrainProtection contract, and it does **not** check who is calling — the association is unauthenticated
today.

## What is not implemented

Each item is a gap between what a reader would infer and what the code does. The register carries the
measured form of each.

- **The three operating modes.** `DaoEscrowMode` has three variants and the record stores one, but
  `InitializeParamsV1` carries no mode field and `initialize_apply_v1` writes `DaoEscrowMode::Escrow` as a
  constant. No caller can choose a mode, and `TreasurySpendV1`'s gate on `Treasury`/`TreasuryEndowment`
  therefore rejects every call (`OBL-C154`).
- **Pool and purse balances.** `pool_purse_id`, `treasury_purse_id` and `endowment_purse_id` are written
  zero, never read, and there is no `Purse::DepositV1` or `Purse::WithdrawV1` call anywhere.
- **Balance checks.** The "insufficient balance" guards in `endowment_withdraw_v1`, `treasury_spend_v1`,
  `execute_claim_v1`, `withdraw_v1` and `resolve_dispute_v1` are `if false` blocks.
- **`ProposalState::Approved`.** No code path writes it, so the proposal lifecycle has no successful exit.
- **The capability-requirement table is dead state.** `0x0a` and `0x10` are gated by the group now
  (roles 8 and 9), but nothing reads what they write: the governance path that consulted the table was
  deleted with `verify_capability_for_action`, so `0x10` is the only reader of the records `0x0a`
  creates. The endpoints are authenticated and inert, which is one step better than authenticated and
  live, and two steps from useful.
- **`WithdrawV1`'s owner path still compares two public values** (`OBL-C152`): with governance inactive,
  `endowment.owner_pubkey != params.recipient_pubkey` admits anyone who knows the owner's address. Every
  other instance of this class in the contract is repaired; this one needs the same ownership proof
  `UpdateV1` uses, which means a circuit reference and a codec change for `WithdrawParamsV1`. The fixture
  asserts the current behaviour so the repair fails that row.
- **`CancelClaimV1`'s `proposer_pubkey` field is now unread.** Cancellation is authorised by the group
  (role 10); the field owes removal in the unit that gives a proposer a real proof, if that is wanted.
- **`member_count`.** `PayPremiumV1` increments the record's count in exec and carries the whole record;
  the update's own `member_count` field has no reader.
- **DrainProtection enforcement.** An association only; the contract does not participate in rate limiting
  or exit queues. The two fields it writes are read nowhere.

## Build and test

The contract's heavyweight integration test is
`bin/dwowd/src/tests/heavyweight_pipeline.rs::test_heavyweight_dao_escrow`, run through
`bin/dwowd/src/tests/heavyweight.sh --dao-escrow`. It is green: `1 passed; 0 failed`, 809.70s. The
contract compiles without warnings.

**Every row builds the children its endpoint demands and names the check it expects.** That is a change of
kind rather than of coverage, and it is what makes the table above testable. Until 2026-09-27 six rows
passed `children: vec![]` and asserted a rejection that any earlier failure in the frame satisfied — and
that is why the colliding child-slot checks in the table survived a green run: no row ever built the call,
so the checks that read `children_indexes` were never executed.

Two of the rows asserted **Success over an authorization that was missing or vacuous** — `0x0a` and
`CancelClaimV1` — deliberately, with the defect named in the row, because a test that asserted the
*desired* behaviour would have been red and indistinguishable from a broken frame while a test that
asserted what the contract actually did would fail loudly the moment a gate was added. **That is what
happened**: both gates arrived on 2026-09-27 (`OBL-C152`), both rows failed, and both are now the
approval-carrying positives beside a no-approval negative that names `Custom(33)`. The pattern costs two
edits per repair and buys the guarantee that the repair is a change rather than a claim.

Two gates in the tree read this contract specifically:

```sh
scripts/check-artifact-freshness.sh                     # the wasm matches its sources
scripts/check-circuit-metadata-alignment.sh             # circuit order == metadata order == client order
```

## See Also
- [Contract Manifest](../arch/manifest.md) — the TOML manifest format; this contract's own manifest is
  [`src/contract/dao_escrow/manifest.toml`](../../../src/contract/dao_escrow/manifest.toml)
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis
- [DAO-Escrow Contract README](../../../src/contract/dao_escrow/README.md)
- [Obligation register](../arch/verification-hazop.md) — `OBL-C151` (the governance setter),
  `OBL-C152` (vacuous authorization), `OBL-C154` (the endpoints this page marks unreachable)
- [MultiSig Contract](multisig.md) — `CreateGroupV1`, `SignV1`, `FinalizeV1`
- [Promissory Note](promissory_note.md) — the value carrier every money endpoint moves
- [Composability](composability.md) — the cross-contract child call mechanism
- [DrainProtection Contract](drain_protection.md)
- [Purse](purse.md) and [Box](box.md) — genesis primitives this contract does **not** yet compose with
- [O-Cap Architecture](../arch/ocap.md)
- [Subscription Contract](subscription.md)
