# DAO-Escrow Contract

One endowment pool, governed by one owner-installed MultiSig group. Members pay premiums into the pool
and receive time-locked membership notes. Every spend out of the pool — and every step of the claim
lifecycle — is authorised by a `multisig::FinalizeV1` approval that the endowment's group has signed.

> **Read this first if you are here from the older revision of this page.** That revision documented an
> OCap/Identity model, per-role capability requirements verified through the `Identity` contract,
> four delegated governance roles exercised through `Box::TakeV1`, Purse-backed pool balances, a
> `drain_protection_enabled` association, a `governance_active` toggle, a vote tally compared against a
> quorum, and a two-pool fee split. **None of those exist in the contract.** Seven selectors and two
> circuits have been retired rather than left as no-ops, the record went from nineteen fields to four,
> and the whole capability registry is gone. What follows is the code as it is; every section states
> its source, and [What is not implemented](#what-is-not-implemented) names each gap that remains.

## Composition

The contract composes with genesis primitives through **child calls it validates**, not through storage
it shares. Two child compositions exist in the crate and no others:

- **`promissory_note::TransferV1` (0x04)** — every value-moving endpoint requires one as its slot-0 child
  and checks three things: that the child's target contract id equals the `promissory_note_cid` recorded
  in the `info` tree, that the child's selector is `0x04`, and that one of its outputs commits to
  `poseidon_hash(value, dao_escrow_bulla)` (`validate_child_contract_id` and
  `validate_child_value_commit`). The transfer *is* the money movement; this contract's `apply` only
  re-stores the endowment record, which is unchanged by a spend.
- **`multisig::FinalizeV1` (0x03)** — the governance approval. `require_governance_child` decodes the
  child's `FinalizeParamsV1`, checks the child targets the `multisig_cid` from the `info` tree, that the
  decoded `group_id` is the endowment's own `multisig_group_id`, and that the decoded `message_hash` is
  the exact action message (see [Governance](#governance-the-owner-the-group-a-member)).

There is no third. `Purse::DepositV1`/`WithdrawV1`, `Box::TakeV1`, `identity::VerifyCapabilityV1`,
`attestation` and the DrainProtection contract are **not called anywhere in this crate**. The `info`
tree holds two keys (`promissory_note_cid`, `multisig_cid`) and both have readers; the `identity_cid`,
`box_cid` and `purse_cid` keys were retired, because each had one `db_set` at init and zero `db_get`,
and `identity_cid` was additionally seeded as `[0u8; 32]` with a reader that treated zero as "skip the
routing check" — fail-open, recorded as `OBL-C152`.

## Governance: the owner, the group, a member

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  DAO-Escrow governance                                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  THE OWNER creates the endowment: InitializeV1 (0x00)                        │
│     mode, min_premium, owner_pubkey → endowment record, multisig_group_id=0  │
│              │                                                               │
│              ▼                                                               │
│  THE OWNER installs the group ONCE: UpdateV1 (0x01)                          │
│     ┌──────────────────────────────────────────────────────────────┐         │
│     │  ZK: SetGovernanceConfigV2                                   │         │
│     │    owner_pub = ec_mul_base(owner_secret, NULLIFIER_K)        │         │
│     │    constrain_instance(owner_pub_x, owner_pub_y)              │         │
│     │    owner_nullifier = poseidon_hash(1, ox, oy,                │         │
│     │                                   owner_secret, bulla)       │         │
│     │    constrain_instance(owner_nullifier)                       │         │
│     └──────────────────────────────────────────────────────────────┘         │
│     the proof exposed those coordinates; exec compares them to the           │
│     record's owner, records owner_nullifier (replay is refused), and         │
│     writes the group id into a field that was zero. A record that            │
│     already carries a group is refused: THERE IS NO ROTATION.                │
│              │                                                               │
│              ▼                                                               │
│  THE GROUP authorises, per action: a multisig::FinalizeV1 (0x03) child       │
│     whose decoded group_id == endowment.multisig_group_id                    │
│     and whose message_hash == governance_message(role, action_id)            │
│     The multisig contract refuses below the group's threshold, and           │
│     consumes each member's signature exactly once.                           │
│              │                                                               │
│              ▼                                                               │
│  six governance-gated paths — one per role that remains                      │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**What the owner does.** Calls `InitializeV1` once, choosing the `mode` and the `min_premium`;
those are the creator's and the record is immutable in both afterwards. Calls `UpdateV1` once to
install the group. While `multisig_group_id` is zero the endowment is owner-controlled: `WithdrawV1`'s
owner path is the only way out, and every other spend refuses with `GovernanceNotActive`. After the
group is installed there is no path back to owner-key control and no path to a different group.

**What the group does.** Signs a message naming one action, off-chain, through the MultiSig
contract's `SignV1`; anyone may then present the collected signatures in a `FinalizeV1` child of the
action's endpoint. The group's **threshold is the quorum**: `FinalizeV1` refuses below it, a failing
child fails the parent transaction, and this contract re-counts nothing — a second source of truth
that could disagree with the first is the argument `OBL-C101` records.

**What a member gets.** `PayPremiumV1` stores a `Membership` record under the note id: the member's
public key, the premium paid, the asset id, an expiry block and the block the membership was created
at. Membership is time-locked by `expiry`, but **nothing in the contract reads that expiry** — no gate
consults it, because no endpoint is member-gated. Members do not vote. Paying at least `min_premium`
is the only requirement, and `min_premium` is the creator's value, carried in the record and compared
in `pay_premium_v1`.

### The signature: `governance_message(role, action_id)`

```rust
governance_message(role: u8, action_id: pallas::Base) -> pallas::Base
    = poseidon_hash([DOMAIN_GOVERNANCE_APPROVAL = 11, role, action_id])
```

`DAO_ESCROW_DOMAIN_GOVERNANCE_APPROVAL` is `11`, the next free value after the SDK's registry of
`1..=10`; it lives in this contract rather than in `src/sdk/src/crypto/constants.rs` so that a change
to it does not stale all 32 artifacts.

| role | endpoint | action id |
|------|----------|-----------|
| 1 | `ProposeClaimV1` (0x07) | `claim_id` |
| 2 | `VoteClaimV1` (0x08) | `poseidon_hash(claim_id, voter_x, voter_y, direction)` |
| 4 | `EndowmentWithdrawV1` (0x04) | `poseidon_hash(bulla, value, recipient_x)` |
| 5 | `TreasurySpendV1` (0x05) | `poseidon_hash(bulla, value, recipient_x)` |
| 10 | `CancelClaimV1` (0x0d) | `claim_id` |

Roles 3, 6, 7, 8 and 9 — `RESOLVE_DISPUTE`, `WITHDRAW`, `ENABLE_DRAIN_PROTECTION`,
`REGISTER_CAPABILITY_REQUIREMENT` and `DEACTIVATE_CAPABILITY_REQUIREMENT` — retired with their
endpoints and are **left unassigned**, on the same rule the selectors follow: a number that meant
something keeps meaning it, so what a recorded approval names cannot change. `WITHDRAW` (6) went with
`withdraw_v1`'s group branch: that endpoint's authority is the owner's proof, which is not a message any
group signs (`OBL-C168`).

Two properties are structural rather than stylistic:

- **The role tag stops approval reuse.** A MultiSig approval is spend-once — each member's nullifier is
  `H(1, member_secret, group_id, message_hash)`, consumed by `FinalizeV1`. `propose_claim` and
  `vote_claim` both key on the same `claim_id`, so an untagged message would have the vote's approval
  name nullifiers the proposal had already spent, and the child would fail.
- **The three money endpoints use a triple, not an id.** Their governance branch is reached exactly
  when there is no proposal id to bind, so an id-based message would be zero for every call and one
  approval of zero would authorise every withdrawal from that endowment forever. `require_governance_child`
  refuses an all-zero action id outright. The triple binds the instance, the amount and the payee;
  it binds `recipient_x` only, not `recipient_y`.

## Spend paths

Four ways value leaves the pool. Each ends in a `promissory_note::TransferV1` child; what differs is
what the parent demands before it will return the update.

| Path | Endpoint | Requires |
|------|----------|----------|
| Owner withdrawal | `WithdrawV1` (0x03) | The `SetGovernanceConfigV2` ownership proof, and `endowment.owner_pubkey == params.recipient_pubkey` — which the proof is what makes meaningful (`OBL-C152`, repaired) |
| Claim payout | `EndowmentWithdrawV1` (0x04) | Role 4 approval over `(bulla, value, recipient_x)`; refuses when `mode == Treasury` |
| Operational spend | `TreasurySpendV1` (0x05) | Role 5 approval over `(bulla, value, recipient_x)`; refuses unless `mode` is `Treasury` or `TreasuryEndowment` |
| Lifecycle payout | `ExecuteClaimV1` (0x09) | The proposal named must be `Approved` (which only `VoteClaimV1` writes) and inside its execution deadline; no approval child |

**The mode decides which endpoint is legitimate, not which pool holds value** — this contract holds no
balance of its own. `Purse` does, and `Purse::WithdrawV1` is where a balance check belongs; the guards
that stood in these handlers were `if false { … }` blocks and are now removed rather than left dead.
Until this contract actually calls the Purse contract, **nothing in this contract bounds a spend by the
pool's balance** — see [What is not implemented](#what-is-not-implemented).

## Endpoints

Ten selectors survive, and they keep their original numbers because they are explicit literals and
nothing renumbers. The seven that retired — 0x06, 0x0a, 0x0b, 0x0c, 0x0e, 0x0f, 0x10 — are
**deliberately absent from the function enum rather than mapped to no-op arms**: a caller sending one
now reaches `InvalidFunction`, which is a refusal a caller can read, where the two governance
functions used to return `Ok(())` while doing nothing.

| Opcode | Function | Proof circuit | Child calls | Authorization | Verdict |
|--------|----------|---------------|-------------|---------------|---------|
| `0x00` | `InitializeV1` | `InitV2` (4) | none | none — anyone may create an endowment | reachable; a duplicate **derived** bulla is refused (`OBL-C158`) |
| `0x01` | `UpdateV1` | `SetGovernanceConfigV2` (5) | none | **owner, proved**; one-shot `owner_nullifier`; no rotation | reachable; see the `None` footgun below |
| `0x02` | `PayPremiumV1` | `PayPremiumV2` (2) | `TransferV1` at slot 0 (exactly one) | none in-contract; `value >= min_premium`, endowment must exist, note must be new | reachable |
| `0x03` | `WithdrawV1` | `SetGovernanceConfigV2` (5) | `TransferV1` only, exactly 1 child | the ownership proof; the payee must be the owner | reachable. The group branch and role 6 retired (`OBL-C168`) — a single `requires_proof` flag cannot describe a path that carries a proof and one that must not |
| `0x04` | `EndowmentWithdrawV1` | — | `TransferV1` at 0, `FinalizeV1` at 1 (1 or 2 children) | role 4; refuses `Treasury` mode | reachable |
| `0x05` | `TreasurySpendV1` | — | `TransferV1` at 0, `FinalizeV1` at 1 (1 or 2 children) | role 5; refuses unless the mode is a treasury mode | reachable only for an endowment created in `Treasury` or `TreasuryEndowment` mode |
| `0x07` | `ProposeClaimV1` | `ProposeClaimV2` (3) | `FinalizeV1` at slot 0 (one or more children) | role 1; endowment must carry a group; claim id must be new | reachable |
| `0x08` | `VoteClaimV1` | `VoteClaimV2` (3) | `FinalizeV1` at slot 0 (one or more children) | role 2; proposal must be `Pending` and inside its voting window | reachable; this is the only writer of `Approved` (`OBL-C159`) |
| `0x09` | `ExecuteClaimV1` | — | `TransferV1` at slot 0 (exactly one) | the proposal must be `Approved` and inside its execution deadline | reachable; does **not** consult the mode |
| `0x0d` | `CancelClaimV1` | — | `FinalizeV1` at slot 0 (one or more children) | role 10; proposal must be `Pending` | reachable |

`(n)` after a circuit is its `constrain_instance` count — the number of instances the metadata arm must
publish. Five endpoints are ZK (`0x00`, `0x01`, `0x02`, `0x07`, `0x08`) and `get_metadata` has an arm
for each; the other five fall through to a default arm that returns an **encoded** empty
`zk_public_inputs` — not a bare `vec![]`, which the host decodes as a rejection signal and which made
every non-ZK function uncallable for a week (`OBL-C77`).

Child counts are worth stating because two of them were wrong in the same way. `endowment_withdraw_v1`
and `treasury_spend_v1` accept **one or two** children — empty and above two are both refused — with
the payment pinned to slot 0 because its validation runs before the endowment is loaded, and the
approval read from slot 1. Each used to require *exactly* one child while reading the approval from
slot 0, which the same function pinned to selector `0x04`, so the governance path could not be built by
any caller (`OBL-C154`). `withdraw_v1` now takes **exactly one** child — the payment — because its
authority is its own proof rather than an approval child (`OBL-C168`). `propose_claim`, `vote_claim` and
`cancel_claim` enforce no child count at all: they require at least one child and read the approval at
slot 0, so extra children ride along unvalidated (`OBL-C165`).

## The claim lifecycle

```
                 group approval (role 1, claim_id)
   ┌───────────────────────────┐
   │                           ▼
   │                     ┌──────────┐   vote window closes   ┌─────────┐
   │                     │ Pending  │───────────────────────▶│ Expired │
   │                     └────┬─────┘                        └─────────┘
   │        group approval    │            group approval (role 10, claim_id)
   │        (role 2, vote_id) │                        │
   │                          ▼                        ▼
   │            ┌──────────────────────────┐    ┌───────────┐
   │            │ Approved  /  Rejected    │    │ Cancelled │
   │            └────────────┬─────────────┘    └───────────┘
   │                         │ ExecuteClaimV1 (no approval child)
   │                         ▼
   │                   ┌──────────┐
   └───────────────────│ Executed │
                       └──────────┘
```

1. **Propose** — `ProposeClaimV1` (0x07). Needs the endowment, an installed group, a role-1 approval
   over `claim_id`, and a claim id that is not already in the `proposals` tree. `apply` stores a
   `Proposal` in `Pending` with `voting_ends_at = current_block + 1000` and
   `execution_deadline = voting_ends_at + 1000`. Those two windows are **hardcoded**, though the
   handler's own comment says windows and claim limits are group configuration rather than contract
   parameters. The action id is the claim id alone — not the value and not the recipient (see
   [Recorded defects](#recorded-defects)).
2. **Vote** — `VoteClaimV1` (0x08). Loads the proposal and refuses a state other than `Pending` with a
   *specific* error (`ClaimAlreadyApproved`, `ClaimAlreadyRejected`, `ClaimAlreadyExecuted`,
   `ClaimAlreadyCancelled`, `ClaimExpired`) rather than a generic "not pending". If the block is past
   `voting_ends_at` the proposal is set to `Expired`, no nullifier is spent, and **no approval is
   required for that path**. Otherwise it requires a role-2 approval over
   `poseidon_hash(claim_id, voter_x, voter_y, direction)`, derives
   `vote_nullifier = poseidon_hash(1, capability_secret, claim_id, voter_x, voter_y)` — term for term
   what `vote_claim_get_metadata` publishes and `proof/vote_claim.zk` constrains — refuses if that
   nullifier is already in the `nullifiers` tree, and then writes the decision:
   `Yes → ProposalState::Approved`, `No → ProposalState::Rejected`.
   **The approval is the quorum.** There is no tally: a successful `FinalizeV1` means the group, not
   one member, has decided. That is why `Approved` had a reader and no writer before this
   (`OBL-C159`), and why the vote's message names the voter and the direction (`OBL-C160`).
3. **Execute** — `ExecuteClaimV1` (0x09). No approval child: the authority is the `Approved` state the
   group already wrote. Requires exactly one `TransferV1` child, a proposal that is `Approved` and
   inside `execution_deadline`, whose `(bulla, value, recipient)` match the call exactly, and an
   endowment record that exists. `apply` writes `Executed`.
4. **Cancel** — `CancelClaimV1` (0x0d), alongside. Requires a role-10 approval over `claim_id` and a
   proposal still in `Pending`; writes `Cancelled`. Cancellation is a **governance action**, not a
   proposer's exclusive right: the check it replaced compared `proposal.proposer_pubkey` to
   `params.proposer_pubkey`, two public values, and so admitted any caller who knew the proposer's key
   (`OBL-C152`). `CancelClaimParamsV1` no longer carries a proposer key at all.

`Rejected` is terminal — nothing re-opens it, and both `vote_claim` and `cancel_claim` refuse it.

## ZK circuits

Five circuits, all compiled, all with `.zk.bin` committed beside their source in
`src/contract/dao_escrow/proof/`. The column is the order the metadata arm publishes, which the
client's `to_vec` also commits to; `scripts/check-circuit-metadata-alignment.sh` is the instrument
that compares the three.

| Circuit | `constrain_instance` order | Built by |
|---------|---------------------------|----------|
| `init.zk` (`InitV2`) | `dao_bulla`, `tx_binding`, `tx_nonce`, `endowment_bulla` | `client/init.rs` |
| `pay_premium.zk` (`PayPremiumV2`) | `tx_binding`, `tx_nonce` | `client/pay_premium.rs` |
| `propose_claim.zk` (`ProposeClaimV2`) | `tx_binding`, `tx_nonce`, `claim_commit` | `client/propose_claim.rs` |
| `vote_claim.zk` (`VoteClaimV2`) | `tx_binding`, `tx_nonce`, `vote_nullifier` | `client/vote_claim.rs` |
| `set_governance_config.zk` (`SetGovernanceConfigV2`) | `owner_pub_x`, `owner_pub_y`, `owner_nullifier`, `tx_binding`, `tx_nonce` | `client/update.rs` |

`SetGovernanceConfigV2` is called by **`UpdateV1` (0x01)**, and `manifest.toml` declares it there.
Until 2026-09-27 the manifest declared it on the retired `set_governance_config` (0x0e) — a no-op —
while `update`, its actual caller, declared no proof at all (`OBL-C155`).

**Two derived values are worth naming, because both have been wrong.**

- `endowment_bulla = poseidon_hash(DRK_POSEIDON_DOMAIN_COMMITMENT, dao_bulla, owner_pub_x, owner_pub_y,
  endowment_asset_id, bulla_blind)`. Four sites derive it — the circuit, the metadata arm, the client
  and `DaoEscrow::derive_bulla`, the value the record is actually **stored under** — and the fourth
  hashed five elements with no domain constant. Prover and host agreed, so every proof verified while
  the chain stored a different key; a client deriving it as documented computed a value the contract
  had never written and every call failed `DaoEscrowNotFound` (`OBL-C156`, closed).
- `tx_binding = poseidon_hash(3, tx_commitment, tx_nonce)` with the constant pair `(0, 0)`. This was
  a literal `pallas::Base::zero()` labelled a "pass-through placeholder", which is a *different value*
  — publishing it required `poseidon(3, 0, 0) == 0`, a preimage, so every proof for those circuits was
  unsatisfiable rather than merely unbound (`OBL-C78`).

`claim_commit = poseidon_hash(4, claim_id, value, claim_blind)`, with the blind carried as a params
field (`ProposeClaimParamsV1.claim_blind`); the contract used to substitute
`capability_proof.capability_secret` for a blind its params did not carry, so the proof's instance
vector and the published one disagreed (`OBL-C153`).

`VoteClaimV2` and `ProposeClaimV2` still take a `capability_id`/`capability_secret` witness pair, and
`VoteClaimParamsV1` still carries a `CapabilityProof`; only `capability_secret` is read, and it is
public call data. See [Recorded defects](#recorded-defects).

## Database trees

Six trees, all initialised in `init_contract`. `votes`, `capability_requirements`, `disputes` and
`governance` were declared and written by nobody, and are removed rather than re-wired: the tally
lives on the `Proposal`, which is the record actually read, and the other three belonged to endpoints
that no longer exist.

| Tree | Written by | Read by |
|------|-----------|---------|
| `info` | `init_contract` — `promissory_note_cid`, `multisig_cid` | every child-call routing check |
| `bullas` | `initialize_apply_v1` (non-empty marker) | `initialize_v1`'s duplicate guard |
| `membership` | `pay_premium_apply_v1` | `pay_premium_v1`'s duplicate guard |
| `endowment` | `initialize`, `update`, `pay_premium`, `withdraw`, `endowment_withdraw`, `treasury_spend` apply | every handler that loads the record |
| `proposals` | `propose`, `vote`, `execute`, `cancel` apply | the same endpoints' exec, plus `verify_proposal_approved` |
| `nullifiers` | `update_apply_v1` (`owner_nullifier`), `vote_claim_apply_v1` (`vote_nullifier`) | `update_v1`'s replay check, `vote_claim_v1`'s double-vote check |

Every `apply` writes a value its `exec` carried in the update rather than reading it back, because
`db_get` is not admitted to the update section — the rule `OBL-C72` records across 66 sites.

Three keys were removed with the trees: `db_version`, `merkle_tree` and `last_root`, each of which had
one `db_set` and no reader, and no merkle root was ever written at all.

## Trust model

| Aspect | What actually protects it |
|--------|--------------------------|
| Owner-only state change | A ZK ownership proof (`SetGovernanceConfigV2`), one-shot per `(owner_secret, bulla)` |
| Governance | The MultiSig group's threshold, enforced by `FinalizeV1` in the child — not re-counted here |
| Approval reuse | Role-tagged message + the approval's own spend-once nullifier |
| Claim decision | The group's approval *is* the quorum; the vote writes `Approved`/`Rejected` directly |
| Double vote | `vote_nullifier` derived in-contract, checked in exec, spent in apply |
| Value movement | The `promissory_note::TransferV1` child, checked for target, selector and value commitment |
| Pool balances | **Nothing in this contract** — there is no balance check and no Purse call |
| Owner-key withdrawal (`WithdrawV1`) | The `SetGovernanceConfigV2` ownership proof, which binds the payee to knowledge of `owner_secret` (`OBL-C152`, repaired) |
| Membership expiry | **Nothing** — `expiry` is stored and read by no gate |

## What is not implemented

Each item is a gap between what a reader would infer and what the code does. The first four below — the
fee split, the member roll, attestation-conditioned resolution and DrainProtection enforcement — were
documented before this contract was re-wired, and **none of the four was ever built**.

- **The two-pool fee split.** A premium is never split between a treasury share and an endowment share.
  `FeeConfig` and the record field that held it are gone, and nothing in this tree divides an incoming
  payment between two pools.
- **The member roll.** There is no member count, no cap, and no roll. The record's `member_count` was
  written and never read, and it is removed; `pay_premium_v1` adds a `Membership` record and returns.
- **Attestation-conditioned resolution.** `ResolveDisputeV1` (0x0c) and its circuit retired with the
  OCap model; there is no oracle, no attestation consumption and no resolution record. A claim has no
  path but propose → vote → execute.
- **DrainProtection enforcement.** The contract does not participate in rate limiting or exit queues,
  and no longer even records an association: the `drain_protection_enabled` flag, the
  `drain_protection_bulla` field, the info-tree flag and `EnableDrainProtectionV1` (0x06) are all
  removed. The contract never once addressed the DrainProtection contract.
- **Purse and Box composition.** No `Purse::DepositV1`, no `Purse::WithdrawV1`, no `Box::TakeV1`
  anywhere in the crate. Consequently there is no balance arithmetic and no balance check either: the
  `if false { … }` guards that stood in the spend handlers were removed with a comment naming where
  the check belongs, because a refusal no input can reach is not a guard.
- **The capability model.** The OCap/Identity governance model — per-role capability requirements
  verified through the `Identity` contract, a member-capability proof as an authorization — is gone.
  Nothing ever registered a requirement, so every gate that read one refused every call even once it
  was reachable, which is what `OBL-C151` records; a check that cannot pass is indistinguishable from
  a broken one. `src/contract/dao_escrow/src/capability.rs` still declares a `CapabilityDescriptor`,
  and its own header states that **nothing in this tree reads it** — it is a statement of intent, kept
  accurate, not a check any code performs.
- **Rotation, and any change to the mode.** Once a group is installed there is no path to a different
  one, and `mode` is written at `initialize` and immutable thereafter. Both are decisions, and both
  are the opposite of what a reader might infer from "update".
- **`manifest.toml` declares `native_token_v1`** in its `dependencies` list. Nothing in the crate or
  in its `Cargo.toml` references it; the only contract this one calls is `promissory_note` (and
  `multisig` for the approval child).

## Recorded defects

These are in the code as it stands, and a caller or a reviewer should know each one.

- **`UpdateV1` with `multisig_group_id = None` burned the owner's one-shot proof and installed
  nothing — FIXED 2026-09-27 (`OBL-C161`).** `update_v1` used to treat `None` as "write nothing" and
  not refuse; `update_apply_v1` then recorded the `owner_nullifier` in the nullifiers tree regardless.
  Because that nullifier is deterministic in `(owner_secret, dao_escrow_bulla)`, the owner could never
  produce a different one — so a single `UpdateV1` carrying `None` made it **impossible to ever install
  a governance group** on that endowment. The `None` arm now refuses with `NoGovernanceGroup`
  (`Custom(57)`). The check is in `exec` and the write is in `apply`, which is why the two halves were
  not read together: `OBL-C72` forces every write in this tree into that split. **A negative control is
  owed** — the fixture's `update` rows all name a group, so nothing currently distinguishes the fix.
- **`WithdrawV1`'s owner path — repaired 2026-09-27 (`OBL-C152`).** It compared two public values, so
  anyone who knew the owner's address could call it. The payee restriction meant the funds could only
  ever reach the owner — the effect was narrower than the defect reads, and what a stranger could do was
  *trigger* the transfer, not redirect it. The endpoint now carries the `SetGovernanceConfigV2`
  ownership proof, which binds the payee's coordinates to knowledge of `owner_secret`.
  **Its group branch was removed rather than kept** (`OBL-C168`): one `requires_proof` declaration
  cannot describe a path that carries a proof and one that must not, and the group's spend has two
  better homes in `EndowmentWithdrawV1` and `TreasurySpendV1`. Role 6 retired with it.
  **And the plan's "reuse the one-shot `owner_nullifier`" was deliberately not followed**: that value is
  deterministic in `(owner_secret, dao_escrow_bulla)` and `UpdateV1` records it under its raw bytes, so
  recording it here would have meant a single withdrawal permanently preventing a group from ever being
  installed — `OBL-C161`'s shape one call later. The field is carried because the circuit publishes it
  as an instance, and not written.
- **`ProposeClaimV1`'s approval binds the claim id and nothing else** (`OBL-C165`). The action id is
  `claim_id`, so one group approval for `(role 1, claim_id)` authorises a proposal carrying **any**
  value and **any** recipient; whoever presents it first chooses them. This is the class `OBL-C160`
  names — a message naming the action but not all of what the action decides. `cancel_claim`'s id-only
  message is exact because the id *is* the whole action; `propose_claim`'s is not. The same row records
  the other half: `propose_claim_v1`, `vote_claim_v1` and `cancel_claim_v1` validate no child count, so
  extra calls ride along in a governance-gated transaction unvalidated by this contract, where the
  vetted endpoints pin theirs.
- **`EndowmentWithdrawV1` is not tied to the claim lifecycle at all** (`OBL-C166`, **fixed 2026-10-08**).
  It never loads the `proposals` tree, and its `claim_id` appeared in no approval message, no lookup and
  no write: `EndowmentWithdrawParamsV1.claim_id` was copied into
  `EndowmentWithdrawUpdateV1.claim_id` and read by nothing. **Both fields were deleted** rather than
  bound — the policy `D1` is that an inert wire field is removed, not documented: the params are now
  `(dao_escrow_bulla, recipient_pubkey, value)`, a 72-byte frame, and the update 52 bytes plus the
  endowment. So an "endowment withdrawal" is a group-authorised transfer, not the execution of a claim,
  and the code now says that rather than carrying an id that implies a lifecycle link it does not have.
  The alternative — binding `claim_id` into the message and requiring an `Approved` proposal — was
  rejected because it reintroduces the second lifecycle executor the re-wire removed.
- **The vote's anti-double-vote key is derived from public call data.** `capability_proof.capability_secret`
  is 32 bytes in the params and 32 bytes on the wire, so the "secret" the name promises is published
  by the struct that carries it (`OBL-C160`, residual). The property holds today through the group
  approval's one-shot spend and the state check, not through a secret. Retiring the field moves the
  params codec and `VoteClaimV2`'s instance set together.
- **`proposal_nullifier` is computed in `propose_claim.zk` and used nowhere** — it is not an instance,
  not a params field, and not read by the contract. `ProposeClaimV1`'s replay guard is the
  `proposals`-tree existence check plus the approval's spend-once, not that value.
- **`ExecuteClaimV1` never consults `mode`.** `EndowmentWithdrawV1` refuses when the mode is `Treasury`,
  because "`treasury_spend` is legal; `endowment_withdraw` is not" is what that mode means — but the
  lifecycle's own executor has no such gate, so a claim can be proposed, voted and executed against an
  endowment in `Treasury` mode. The two payouts out of the same pool are gated inconsistently.
- **`Expired` needs no approval.** Any caller can flip a `Pending` proposal whose voting window has
  closed, because the auto-expiry path is checked before the approval. It casts no vote and spends no
  nullifier, which is why it is written that way — but it is an unauthenticated state transition.
- **`verify_proposal_approved` refuses a proposal that is not `Approved` with `ProposalNotPending`**
  (`Custom(38)`), and reports a bulla/value/recipient mismatch as `ProposalNotFound` (`Custom(37)`).
  A reader debugging `ExecuteClaimV1` is sent to the wrong name: the two states this check can
  actually see are `Pending` (not yet decided) and `Executed` (already spent), and a caller whose
  `value` does not match is told the proposal does not exist (`OBL-C163`).
- **29 of the enum's 55 error variants were constructed nowhere — FIXED 2026-09-27 (`OBL-C162`).**
  They carried the retired model's vocabulary — `QuorumNotMet`, `ApprovalRatioNotMet`,
  `OracleThresholdNotMet`, `AttestationAlreadyConsumed`, `DisputeNotFound`,
  `CapabilityRequirementNotRegistered` — and the enum was an auditable statement of what the contract
  refuses, two thirds of which it could not reach. All 29 are removed and `NoGovernanceGroup` (57) was
  appended, so `src/error.rs` now holds one variant per refusal the contract can actually produce.
  The removal renumbers nothing: every arm maps to an explicit `Custom(N)`, and the retired codes are
  left unmapped rather than reused.

## Build and test

The contract's heavyweight integration test is
`bin/dwowd/src/tests/heavyweight_pipeline.rs::test_heavyweight_dao_escrow`, run through
`bin/dwowd/src/tests/heavyweight.sh --dao-escrow`.

**The re-wire is green, measured 2026-09-27**: `1 passed; 0 failed`, **730.94s** — the first green run
of the ten-endpoint contract. It covers every endpoint below except `PayPremiumV1`, which is deferred on
a circuit bug, and it drives the claim lifecycle end to end for the first time
(`ProposeClaimV1_Lifecycle` → `VoteClaimV1_Approved` → `ExecuteClaimV1_Approved`).

Two older numbers on this page are history rather than statements about the code here. A run of
**809.70s** measured the contract *before* the re-wire — seventeen selectors, the capability-requirement
endpoints, the DrainProtection association, a nineteen-field record; and the re-wire's first measurement
was **655.00s**, which predates the lifecycle rows and the two controls they carry. Reachability on this
page is read from the source; the three properties the run does *not* cover are named in
[Recorded defects](#recorded-defects) and in the register rows.

The contract crate also carries its own tests in `src/contract/dao_escrow/tests/integration.rs`,
including a round trip for every parameter and update type, a test that the six surviving selectors
resolve and that the seven retired ones are refused, and a `Proposal` round trip over all six states.
No run of those against this revision is recorded here either.

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
- [Obligation register](../arch/verification-hazop.md) — `OBL-C151` (the governance setter),
  `OBL-C152` (vacuous authorization, all five instances repaired), `OBL-C154` (the child-slot collision
  and the mode), `OBL-C156` (the endowment bulla, four ways), `OBL-C158` (the duplicate guard's key),
  `OBL-C159` and `OBL-C160` (the claim lifecycle's terminal state and the approval's message),
  `OBL-C161` (a no-op that spent the owner's only proof), `OBL-C162` (29 error variants nothing
  constructed), `OBL-C163`–`OBL-C167` (the refusals, gates and inert fields the removal exposed),
  `OBL-C168` (the one-shot credential two actions would have shared)
- [MultiSig Contract](multisig.md) — `CreateGroupV1`, `SignV1`, `FinalizeV1`
- [Promissory Note](promissory_note.md) — the value carrier every money endpoint moves
- [Composability](composability.md) — the cross-contract child call mechanism
- [DrainProtection Contract](drain_protection.md) — **not** composed with by this contract
- [Purse](purse.md) and [Box](box.md) — genesis primitives this contract does **not** yet compose with
- [O-Cap Architecture](../arch/ocap.md)
- [Subscription Contract](subscription.md)
