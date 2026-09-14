# Genesis & Consensus Adversarial Audit

**Scope.** The genesis contract set (Deployooor, NativeToken, PromissoryNote,
Identity, Oracle, Attestation, Purse, Box, MultiSig) and the consensus-critical
path: `dwow_chain` (`src/linear/`), the block-acceptance orchestrator
(`bin/dwowd/src/block_acceptor.rs`), the native_token contract, and the wallet's
native_token scan.

**Date.** 2026-09-13. Branch `linear-master`.

**Trigger.** The plaintext-PoW-reward deprecation (series ending `1dead327b9`)
removed the `Mint_V2` circuit from the coinbase/uncle path. Its stated security
argument was that "every removed ZK check is re-covered by a plaintext host check".
This audit tested that claim.

---

## Method

The highest-yield lens, applied to every consensus field, is three questions with a
`file:line` answer each:

1. Where is the value **originated**? (wire / producer / chain state)
2. Where, if anywhere, is it **re-derived** from independent state?
3. What **aborts** the block if it diverges?

A field that is originated but never re-derived is a **producer-declared trust
boundary**. C1, C1b, C4 and C13 below are all instances of that one disease, and it
is the pattern to look for first when reviewing any future consensus field.

A second lens: the **Python model is the executable spec**
(`contrib/model/*.py`). Where the model and the Rust disagree, the model is right
unless the model is shown to be wrong. Two findings (C2, C3) were cases where the
Rust deviated from a model that was already correct.

Findings are line-referenced to the pre-fix revision where fixed; the fix column
names the change.

---

## Findings

| # | Severity | Finding | Status |
|---|---|---|---|
| C1 | **CRITICAL** | Coinbase/uncle note VALUE unbound → arbitrary over-mint | Fixed |
| C1b | **CRITICAL** | Uncle rewards publicly spendable by any observer | Fixed |
| C13 | **HIGH** | Wallet never scanned 0x07 — uncle rewards undiscoverable by their owner | Fixed |
| C2 | HIGH | `build_uncle_merkle` panicked at 5 or 6 uncles — remote DoS | Fixed |
| C3 | HIGH | Uncle PoW judged against the wrong height's target | Fixed |
| C4 | HIGH | Reward split producer-declared, not derived | Fixed |
| C5 | MEDIUM | `UncleProof` redundant fields → 2N RandomX inits per block | Fixed |
| C6 | MEDIUM | Σ-pin implemented three times; invariant violation swallowed | Fixed |
| C7 | MEDIUM | Depth-0 uncles admitted; off-by-one vs `MAX_UNCLE_DEPTH` | Fixed |
| C10 | MEDIUM | Genesis manifest drift (identity, promissory_note, orphan bins, dead list) | Fixed |
| C12 | MEDIUM | Mint-time uncle nullifier written to the nullifier tree unauthenticated | Fixed |
| C8 | MEDIUM | Sync handshake genesis check fail-open | Fixed |
| C9 | MEDIUM | PoW-covered header fields permanently zero | Documented as reserved |
| C14 | MEDIUM | `Commitment::from_attributes` used a different hash rule than the circuits | Fixed |
| C15 | LOW | Two doctests in the test-harness don't compile (pre-existing, red at HEAD) | Reported |
| C16 | MEDIUM | Test fixtures declare `header.miner = 0` while minting to a real key | Fixed (2 proven); wider suite unverified |
| C17 | MEDIUM | Stratum falls back to `miner = [0u8;32]` for a submission with no template | Reported — see below |
| C18 | MEDIUM | Checked-in `dwowd_config.toml` cannot be parsed by the current binary | Reported — see below |
| C19 | LOW | The genesis pin is checked AFTER the block is committed to the datadir | Reported — see below |
| C20 | **HIGH** | An INCREMENTAL contract rebuild changes the genesis hash, silently invalidating the pin | Reported — see below |
| C11 | — | Block-size gate measures JSON, not the canonical encoding | **Withdrawn** — see below |

---

### C1 — CRITICAL: the coinbase/uncle note VALUE was unbound (over-mint)

`PoWRewardParamsV1` (`src/contract/native_token/src/model/mod.rs`) had no
`effective_value` field, and nothing on-chain re-derived `output.commitment`. The
commitment was built client-side over `effective_value`
(`client/transfer/proof.rs:167-182`) while `value_commit` committed to the **full**
base — and the Mint_V2 circuit was the only thing linking them. Removing it left
the committed value unconstrained.

The host checks (`bin/dwowd/src/block_acceptor.rs:219-286`) compared
`header.total_reward`, `total_pin` and `Σ uncle_note.input.value` — all
producer-declared. `block_acceptor.rs:245` still asserted "*The circuit constrains
`effective_value + total_pin == value`*", which was a false claim of soundness.

**Failure scenario.** A producer that loses the race at H−1 and wins at H includes
its own stale H−1 block as an uncle: it declares `total_pin = base/2`, emits the
uncle note for `base/2`, and commits the coinbase note to the **full** base. Total
spendable = `1.5 × base`, while `S_H`/`TOTAL_SUPPLY` advance by only `base` — so the
Pedersen supply audit cannot see it. The bound was not even `base`: the coinbase's
spend key is the producer's own, so any value was realisable.

**Why this was the load-bearing invariant.**
`src/linear/src/proof_of_token_balance.rs:105-107` skips the coinbase tx and
`:126-128` skips `PoWRewardV1`/`FeeCollectV1`/`UncleMintV1` — exactly the set of
value-*originating* calls. The mass balance was therefore vacuous for the mint
side, leaving C1's check as the only gate on token supply.
`doc/src/arch/consensus/uncle_merkle.md` §"Spendable-note mass balance" already
*required* this check; it had never been implemented.

**Fix.** `effective_value` and the note's plaintext `CommitmentAttributes` preimage
are now carried in the call data; `pow_reward_v1` recomputes
`commitment_attrs.to_commitment()` and requires it to equal the committed note, and
requires `effective_value + total_pin == input.value`; `validate_block_structure`
and `block_acceptor` enforce the same plus
`effective_value + total_pin == expected_reward(H)` and
`commitment_attrs.public_key == header.miner`.
`compute_transfer_mint_revealed` now returns the preimage it actually used, because
it derives the commitment's public key from `spend_secret` rather than from the
caller's `output` — a caller-supplied preimage would have recomputed a different
commitment and failed the equality.

> **Correction — the `commitment_attrs.public_key == header.miner` check as first
> written BROKE GENESIS, and the dwowd suite caught it.** It was applied
> unconditionally, but genesis deliberately carries no miner identity:
> `init_genesis` sets `miner: [0u8; 32]` and `genesis.md` documents that as "No miner
> identity in the header — the coinbase binds the mining key", while the genesis
> coinbase note binds to the authority's real key. Genesis runs through the same
> acceptance path, so every genesis block was rejected and every test calling
> `init_genesis` failed. The check is now guarded by
> `height != BlockHeight::GENESIS`, matching the neighbouring genesis exemptions for
> the block-size cap (0.5) and witness verification (2.5), whose justification is the
> same: genesis authenticity is the pinned genesis hash. Nothing in this report
> should be read as "the added checks are safe at genesis" — that exception is
> explicit and load-bearing.

---

### C1b — CRITICAL: uncle rewards were publicly spendable

`client/uncle_mint.rs:82-86` derived the note's spend authority as

```rust
let spend_secret = SecretKey::from_base(poseidon_hash([
    uncle_hash_base,                       // blake3(uncle.header) — PUBLIC
    h_base,                                // canonical height    — PUBLIC
    pallas::Base::from(DOMAIN_SPEND_SECRET),// 20                 — PUBLIC
]));
```

and that secret *was* the spend authority: the nullifier is
`Nullifier::new(spend_secret, commitment)`, the commitment's public key is
`from_secret(spend_secret)`, and the blinds derive from it too. Every witness needed
to spend an uncle note was computable from the accepted block's public uncle header.
Any network observer could rebuild the commitment, reconstruct the `Burn_V2`
witness, publish the nullifier and take the note — no decryption needed, so the AEAD
encryption to `uncle.header.miner` was irrelevant. `COINBASE_MATURITY` delayed this,
it did not prevent it.

It was a regression, not a design choice: the coinbase and fee-collect notes key off
the miner's wallet secret. Only the uncle path was rewritten to a public derivation.

**Verified against the circuit**: `src/contract/native_token/proof/burn.zk:49-63`
derives `pub = ec_mul_base(spend_secret, NULLIFIER_K)` in-circuit and builds the
coin hash from `pub`'s coordinates. So a commitment bound to `uncle.header.miner`
genuinely requires the uncle miner's secret — **no circuit change was needed**.

**Fix.** The commitment commits to `uncle.header.miner`; the mint publishes no
nullifier (`Output.nullifier` is now `Option<Nullifier>`, with `None` for this call
only — the encoding the codebase already prescribes at
`src/sdk/src/crypto/nullifier.rs:109`); the nullifier is revealed at spend. The
`UncleMintV1` call carries `effective_value` + the plaintext preimage, and the host
binds each note to an included uncle by `(pin value, header.miner)`.

---

### C13 — HIGH: the wallet never scanned uncle-mint outputs

`bin/dww/src/scan.rs:677` gated output discovery on
`matches!(function_code, 0x00 | 0x03 | 0x04 | 0x05 | 0x06 | 0x08)` — **0x07 was
absent**, although a `NativeTokenSource::UncleMintV1` arm (`:121`) and its mapping
(`:536`) existed. The wallet therefore never discovered its own uncle rewards. This
is the other half of C1b: the note was simultaneously unspendable by its rightful
owner **and** spendable by any observer.

**Fix.** 0x07 added to the gate, and — now that the preimage is plaintext in the
call data — the wallet resolves the spend key by matching
`commitment_attrs.public_key` against the decrypting secret instead of guessing from
the AEAD payload (`declared_note_preimage` / `build_native_token_cap_record`).

---

### C2 — HIGH: `build_uncle_merkle` panicked at 5 or 6 uncles (remote DoS)

`src/linear/src/block.rs` padded only the **leaf** layer. With 5 uncles → 6 leaves →
an intermediate layer of 3 → `chunks(2)` yielded a 1-element chunk →
`debug_assert` fired in debug and the pair index **panicked in release**. With 6
uncles it panicked one level later. `MAX_UNCLE_COUNT` is 6 and nothing clamped below
it, so any peer could crash a node via `block_acceptor.rs:99`, and the miner crashed
itself in `create_block_with_uncles`.

The sibling `compute_merkle_root` padded **every** odd layer — two merkle
implementations, one wrong. **The Python model was already correct**:
`contrib/model/chain_validation_model.py:690` padded every layer and returned the
leaf itself as the root for n=1, while its docstring claimed to match the Rust. The
Rust was the one that had diverged. The existing tests used 0, 1, 3 and 7 uncles —
every count that pads cleanly — which is why the suite stayed green over the hole.

**Fix.** One shared `merkle_layers` construction for the whole chain, used by both
trees. Regression test covers every admissible count 0..=6.

**Consensus-visible side effect, covered by the genesis re-roll:** for a **single**
uncle the old root was `H(leaf‖leaf)` with path depth 1; the fixed code returns the
leaf itself with path depth 0, matching `compute_merkle_root` and Bitcoin. Counts 2,
3, 4 and 7 produce an identical root before and after.

---

### C3 — HIGH: uncle PoW judged against the wrong height's target

`block_acceptor.rs:115` passed `block.header.target` into `check_uncles` →
`verify_uncle_proof`. The target is recomputed **every** block from a sliding
timestamp window (`consensus.rs::get_next_work_required`), so an uncle legitimately
mined at H−3 was judged against target(H): honest uncles were rejected, and which
stale work is payable became a function of the current target.

**The Python model was correct**: `chain_validation_model.py:812` passes
`uncle.header.target`. Another Rust-vs-spec divergence.

**Fix.** `CChainState::block_target_at(height)` resolves the target in force at a
height through `get_next_work_required` — the same source of truth block Stage-2
validation uses — and `check_uncles` now takes a per-uncle target slice, failing
closed if the slice length disagrees with the uncle count.

---

### C4 — HIGH: the reward split was producer-declared

`pin_confirmed` is a plain wire field consumed verbatim
(`block_acceptor.rs:231`, `chain_state.rs:997`, `supply_chain.rs:304`). Spec
`uncle_merkle.md` §"Reward Distribution" says
`pin_confirmed_i = base_reward / 2^depth_i`; it was never re-derived at validation.
Worse, `uncle_merkle_root` commits only to `blake3(to_mining_blob(&u.header))`, so
`pin_accepted`, `pin_confirmed` **and** the uncle's `transactions` sit outside the
commitment and could be rewritten by any relaying producer.

**Fix.** An ACCEPTED pin is re-derived in `check_uncles`:
`pin_confirmed == expected_reward(H) / 2^depth`, else the block is rejected. A
rejected pin is deliberately unconstrained (it pays nothing). Committing the fields
into the merkle leaf was **not** needed once rewriting is detectable. Verified that
all uncle-creation sites pass `expected_reward(<new block height>)`, so honest
uncles still pass.

---

### C5 / C6 / C7 — structural

- **C5.** `UncleProof` carried `header` (duplicating `uncles[i]`) and `pow_hash`,
  which the verifier recomputed — so the builder ran a RandomX cache+VM
  initialisation per uncle purely to fill a field nobody trusted, and
  `verify_uncle_proof` ran its own: 2N initialisations of the deliberately expensive
  step per block, on the accept path. Fixed: the proof is now
  `{ merkle_path, position }`, the header comes from the `UncleBlock`, and
  `build_uncle_merkle` is pure blake3.
- **C6.** Σ pin was implemented four times with different filters and different
  overflow behaviour (`u64::sum()` panics in debug and wraps in release; bare `+`;
  `saturating_add`). `compute_reward` also **swallowed** a violated supply invariant
  (`checked_sub(..).unwrap_or(0)` + a log line) instead of aborting. Fixed: one
  `total_accepted_pin` helper returning `Err` on overflow, used by the builder, the
  host check and the connect-time check; `compute_reward` returns `Result`.
- **C7.** `check_uncles`' recency test admitted a sibling block at the SAME height
  as the referencing block, which yields `split_for_uncle(0)` = 100% of the base
  reward; the maximum effective depth was also 5, not the documented 6. Fixed: depth
  is explicit and bounded `1..=MAX_UNCLE_DEPTH`.

---

### C12 — MEDIUM: unauthenticated write into the nullifier tree

`chain_state.rs` inserted an uncle mint's `params.nullifier` into the nullifiers
tree as a kind-0 claim record **without re-deriving it**. That nullifier was
producer-supplied, so an attacker could poison an arbitrary nullifier and block a
legitimate spend of some other note. Found while fixing C1b; the mint no longer
publishes a nullifier at all, so there is nothing to write.

---

### C8 — MEDIUM: sync handshake genesis check was fail-open

`src/linear/src/sync_connection.rs:439-441` accepted a peer whose `genesis_hash` was
absent, and accepted everything when the local node had no genesis hash. The spec
(`chain_validation_model.py::apply_genesis_filter`, Path A) filters on
`peer.genesis_hash == our_genesis`, so a peer that omits the hash is **not**
compatible.

**Fix.** When we hold a genesis, the peer's hash must match — omission is now a
rejection. A node with no genesis still accepts, because Path B (the plurality vote
over peer tips) is a multi-peer decision that cannot be made on a single connection.

**Gap noted, not fixed:** the spec's three-mode machinery (`Off` / `Relaxed` /
`Strict`) and the Path-B tie-breaker have **no Rust implementation** at all. The
handshake implements Path A only. This should be reconciled deliberately.

---

### C9 — MEDIUM: PoW-covered header fields are permanently zero

`BlockHeader.commitment_merkle_root` and `nullifier_root` are inside the 260-byte
mining blob, i.e. PoW-covered, yet only ever `[0u8; 32]` in production (only test
fixtures set them). They are the hook `scaling.md` describes.

**Action.** Documented as RESERVED on the fields themselves, with the consequence
made explicit: populating them changes the mining preimage and therefore the block
hash, so it is a deliberate consensus change, never a silent one. No assertion added
— that would itself be a new consensus rule and needs a spec first.

---

### C10 — MEDIUM: genesis manifest drift

- `src/contract/identity/manifest.toml` declared `create_claim` (code 3) and the
  circuit `CreateClaimV2`. Both were removed from the contract
  (`identity/src/lib.rs` rejects `0x03`; only 2 circuits are registered;
  `tests/integration.rs` asserts the rejection). Removed, and the header corrected
  to "8 functions, 2 ZK circuits" with a warning against reintroducing code 3.
- `src/contract/promissory_note/manifest.toml` named circuits without the
  underscore (`TransferV2`), while the registered namespaces are
  `Transfer_V2` etc. (`promissory_note/src/lib.rs:216-224`). Fixed for all five
  circuits, in both `proof_circuit` references and `[[circuits]]` entries.
- `src/contract/native_token/proof/` held orphan `fee_collect.zk.bin` and
  `fee_threshold_v1.zk.bin`. Verified via `git log --diff-filter=D` that their
  `.zk` sources were **deliberately** deleted (`cd6b1cb680`, `82b358098d`) and that
  nothing `include_bytes!`s them: untracked build residue. Removed.
- `deployooor` / `native_token` ship a `manifest.toml` but are deployed with empty
  manifest bytes, contradicting a comment claiming they "have no manifests".
  Resolved in favour of NOT deploying them — the wallet handles those two natively
  (Path 1), so deploying their manifests would make the wallet scan them twice. The
  code comment and `genesis.md` now state that, and say plainly that the on-disk
  manifests are interface documentation, not consensus artefacts.
- `GENESIS_CONTRACT_IDS_BYTES` (`src/sdk/src/crypto/contract_id.rs`) listed
  PromissoryNote before NativeToken, disagreeing with the consensus order in
  `execution.rs::genesis_contracts()`. It has **zero consumers**. Aligned and
  documented as non-authoritative, since `doc/src/arch/genesis.md` instructs
  developers to maintain it.

---

### C14 — MEDIUM: a second, incompatible commitment rule

`Commitment::from_attributes` (`src/contract/native_token/src/model/mod.rs`)
hashed the note attributes **without** the `DRK_POSEIDON_DOMAIN_COMMITMENT`
separator (its doc said "same as promissory_note::Commitment" — a convention
imported from another contract), while `CommitmentAttributes::to_commitment()` and
the burn/spend circuit both include it (`burn.zk`: `coin = poseidon_hash(
DOMAIN_COMMITMENT, pub_x, pub_y, value, …)`). The two rules therefore disagreed.

It has **no production callers** — only test fixtures — so this was not a live
exploit, but it is a trap: a fixture built with it produces a "valid" commitment
that the real entrypoint and circuit would reject, so tests built on it asserted
nothing about a spendable commitment.

It was **found by C1's fix**: the new
`commitment_attrs.to_commitment() == output.commitment` check failed against a
fixture that used it.

**Fix.** `from_attributes` now delegates to `CommitmentAttributes::to_commitment()`,
so native_token has exactly one commitment rule and the fixtures describe real,
spendable commitments.

### C15 — LOW: pre-existing broken doctests (reported, not fixed)

`cargo test -p dwow-contract-test-harness` is red at HEAD for two **doctests**:
`src/contract/test-harness/src/contract_graph.rs` (line 37) and `harness.rs`
(line 39) both `use …::contract_graph::{Contract, get_contracts}`, and
`get_contracts` exists **only inside those doc comments** — it was never defined.
So the package's documented API does not exist, and the crate's test target has
been failing independently of any consensus work. Out of scope for this audit
(the crate is untouched by it); recorded so the red suite is not mistaken for a
regression.

Also noted while running that package: one test in it takes ~5,481 s (≈91 min)
in a single run. Not a failure, but worth a look for CI budget.

---

### C16 — MEDIUM: test fixtures declare `miner = [0u8; 32]` while minting to a real key

Found by the genesis re-roll. The C1 coinbase-to-miner binding is CORRECT for
production — `registry/model.rs:693` sets
`miner: recipient_config.recipient.public().to_bytes()`, the same key the coinbase note
binds to, and Stratum/merge-mining propagate `template.miner`. But hand-built *test*
blocks call `build_linear_coinbase(recipient, …)` and then construct the header with
`miner: [0u8; 32]`, so the binding rejects them.

Two were proven by the suite (`genesis.rs` `test_block_creation` and
`test_zero_fee_block_accepted`); both now capture the recipient's public key before the
recipient is moved into the builder, and both pass. **The class is not exhausted**:
`harness.rs:84`, `harness.rs:217` and `genesis.rs:989` also hardcode `miner = 0`, and
although they appear benign (they build stub coinbases with no PoWRewardV1 call, which
`validate_block_structure` rejects earlier for a different reason), that was not
verified exhaustively. Any fixture that pairs a real `build_linear_coinbase` with a
manually built header must set `miner`.

### C17 — MEDIUM: Stratum's `miner` fallback can now be rejected

`bin/dwowd/src/rpc/stratum.rs:485` assembles a submitted block's header as
`miner: template.as_ref().map(|t| t.miner).unwrap_or([0u8; 32])`. A submission with no
template therefore declares miner 0 while its coinbase may bind a real key, and the C1
binding rejects it. Such a block appears unacceptable for other reasons too (no template
⇒ no coinbase ⇒ `validate_block_structure` fails), so this is probably unreachable — but
it is exactly the "declared vs derived" shape this audit exists to catch, and it should
either fail closed with a clear error or be shown unreachable by test.

### C18 — MEDIUM: the checked-in config cannot be parsed

`bin/dwowd/dwowd_config.toml` contains no `create_genesis` field in any section, but it
is a REQUIRED field of `BlockchainNetwork`. Launching the binary against it fails
immediately:

```
[ERROR] Failed parsing requested network configuration: missing field `create_genesis` at line 45 column 1
Error: ParseFailed("Failed parsing requested network configuration")
```

The docker entrypoint never notices because `contrib/docker/darkwow-testnet/lib/config.sh`
GENERATES the container's config (including `create_genesis`) rather than using the
template. So the pipeline is unaffected, but anyone following the local-run instructions
hits this at once, and the template has silently drifted from the schema. It should be
regenerated from the same field set the generator writes, or removed in favour of the
generator.

### C19 — LOW: the pin is verified after the genesis chain is committed

Observed while proving the pin bites. A run whose identity does NOT match the pin still
executes the full genesis — WASM deployment of the 9 contracts and the sled commit —
and only then reports the mismatch:

```
[INFO] WASM execution complete (44.6s)
[INFO] Block 1 at height 1 committed
[ERROR] GENESIS HASH MISMATCH: computed=77087c35… expected=02f58ad0…
```

So a rejected genesis leaves a committed height-1 chain in the datadir, and the NEXT
start takes the restart-guard path (which re-verifies the stored genesis against the
pin and errors again). It fails safely — nothing diverges — but it does ~45 s of WASM
work and writes state before rejecting, and it leaves the datadir in a half-onboarded
state. The pin could be checked before `accept_block`, since
`hash_block_with_cached_vm` needs only the built block and its VM.

### C20 — HIGH (CORRECTED): an unguarded contract rebuild silently invalidates the genesis pin

Found while re-rolling genesis, and it cost a full 98-failure suite run to see.

**Established mechanism.** Genesis embeds the contract WASM bytes: the nine
deployment transactions carry `DeployParamsV1.wasm_bincode`, those transactions feed
`compute_merkle_root`, and that root sits inside the mined header. So the genesis
block hash is a function of the exact WASM bytes.

**The chain, evidenced in the `make test` log.** This section originally attributed the
failure to `cargo` reusing cached link artifacts. That is **not** what happened, and the
real chain is longer — three separate build defects had to line up:

1. An edit to `src/sdk/src/crypto/contract_id.rs` made `deployooor` out of date: its
   `WASM_SRC` includes `src/sdk/**/*.rs`.
2. `make test` depends on `contracts` (`Makefile:182`), and `contracts:` invokes
   `$(MAKE) -C src/contract/<c>` **with no target**. Make's default goal is a
   Makefile's *first* target, which for 21 of the 32 contract Makefiles is
   `check-zkas-version` — so those 21 were silent no-ops that never built anything and
   never ran `check-source-hash`.
3. `deployooor` is not one of them: its first target already **was** `all`, and it had
   **no `.source_hash` guard at all**. So it rebuilt.
4. `cargo test` then recompiled `dwowd`, whose `include_bytes!` had captured the new
   WASM, and the genesis merkle root moved.

The result was 98 of 124 `dwowd` tests failing with

```
init_genesis: Custom("Genesis hash does not match compiled-in constant.
  The genesis parameters (contract WASM, timestamp, key) have changed. …")
```

**The previous mechanism does not reproduce.** Re-measured with a path remap in place:
an incremental build and a `clean all` build of an unchanged `purse` tree agree
**byte-for-byte** (`ccbe13a370aa4c6f3f125eca751398e6`, 306453 bytes, both ways), and
deleting `proof/*.zk.bin` to force circuit regeneration changes neither the circuits
(byte-identical to the stored ones) nor the WASM. So link caching and circuit staleness
are both excluded; the source-driven chain above is the explanation. The earlier numbers
predate the path remap and their artifacts are gone, so this is recorded as **not
reproduced** rather than as refuted.

**What survives, and is the useful part.** The diagnostic signature is sound: in the
node log, `GENESIS HASH MISMATCH: computed=fb928eea… expected=02f58ad0…` appeared with an
UNCHANGED coinbase commitment — the key was right and the difference was entirely in the
WASM-derived merkle root. **A changed genesis hash with an unchanged coinbase commitment
means the WASM bytes moved, not the identity.**

**Rule.** The pin is only valid for the exact WASM bytes it was computed with, so the
contract build must be clean and must precede the ceremony:

1. `make -C src/contract/<c> clean all` for all nine genesis contracts,
2. then run the ceremony and read `Computed hash:`,
3. then write `genesis_hash.txt`, rebuild, and do not rebuild the contracts afterwards
   without re-pinning.

The guard now enforces most of this: it refuses to build a stale artifact, `all` is the
declared default goal so a rebuild cannot be skipped silently, and the recorded hash
covers the Rust sources and the manifest rather than only `proof/*.zk`. What remains
manual is re-running the ceremony and re-recording the pin — which is exactly what
`genesis_pin_is_current` in `bin/dwowd/src/tests/genesis.rs` reports.


### C11 — WITHDRAWN: the block-size gate measures JSON

`block_acceptor.rs:126-146` measures the block with `serde_json::to_vec(..).len()`
and applies a 1% margin. Initially flagged as a non-canonical measurement in the
consensus path.

**Withdrawn on investigation.** Blocks are broadcast as JSON
(`bin/dwowd/src/proto/linear_broadcast.rs:112,144`), so JSON *is* the wire encoding
and measuring it is the correct DoS measure; the canonical binary encoding would
*under*-measure the bytes a peer must buffer and weaken the gate. The residual
concern — a false positive in the consensus path is a chain split — is real and
already documented in the code, and the right fix is to enforce the cap at the p2p
layer where the bytes arrive (which `linear_broadcast.rs:156` partly does). Recorded
as a design note, not a defect.

---

### B-series — defects found by three independent re-audits after C20

The C-series above came from one pass. C20 then cost a four-hour suite run whose result
was misreported, so three independent red-team audits were commissioned. They found the
defects below, and contradicted two of the explanations the C-series had recorded (C20's
mechanism, corrected above, and the claim that the pin was doing its job).

**B1 — HIGH: the pin was enforced inside a shared test fixture, so its failures named
nothing.** The compare lived in `init_genesis` (`bin/dwowd/src/lib.rs:615-639`), which
every genesis-building test calls — 15 call sites across `tests/modules/chain_setup.rs`,
`tests/pipeline.rs`, `tests/wallet_sync_integration.rs`, `tests/merge_mining.rs`,
`tests/heavyweight_pipeline.rs` and others. No test asserted the pin: `grep -rn
"genesis_hash" bin/dwowd/src/tests/` returned three hits, all comments or inert struct
fields. So all 98 failures were aborts at that one line, before the test reached its
subject. Verified for two of them:
`test_box_take_accepts_through_accept_block` (`tests/capability_scan_integration.rs:1115`)
died at `:1117` before the Box gate, and `test_daemon_pull_sync_converges`
(`tests/daemon_sync_integration.rs:181`) died inside `build_authority_chain` at
`:118-119` before any P2P object existed — a fixture panic reported as a **sync
regression**. Fixed: enforcement moved to `check_genesis_pin`, called from the node path
(`init_linear`) on both the fresh-creation and datadir-reuse branches, and
`genesis_pin_is_current` is now the single test that asserts the pin.

**B2 — HIGH: the contract build was not reproducible off this machine.** The WASM
embedded 2-32 absolute host paths per contract (`strings -a <wasm> | grep "$HOME"`;
`deployooor`: 32) from rustc's panic locations. Nothing set `--remap-path-prefix`, so a
pin rolled here could never match a build elsewhere — **at most one environment could ever
satisfy it**. The obvious fix does not work, and measurement is what showed that:
`[target.wasm32-unknown-unknown] rustflags` in `.cargo/config.toml` **is** honoured (all 97
rustc invocations received the flag) but cargo does **not** expand `${HOME}` in config
values, so rustc was handed the literal `--remap-path-prefix=${HOME}/.cargo=/cargo`, which
matches nothing; and `[env] RUSTFLAGS` expands but is read too late — cargo computes
rustflags from the process environment before applying `[env]`, so the crate recompiled
with zero remap flags. The flags therefore live in the per-contract Makefiles, where
`$(HOME)` is make-expanded, and the container gets them from an `ENV` in
`contrib/docker/darkwow-testnet/Dockerfile`. Both failed mechanisms are recorded in
`.cargo/config.toml` so they are not re-added. Relatedly, the toolchain was not pinned at
all: `rust-toolchain.toml` said `channel = "stable"`, which **overrides** whatever the
devnet image installs — so the image's "pinned for deterministic builds" `1.95.0` was
inert and a stable bump would have re-rolled genesis. (A fourth source of truth was found
on the development host: a rustup *directory override* to `stable`, which likewise beats
`rust-toolchain.toml`.)

**B3 — HIGH: `make` did not build 21 of the 32 contracts.** The root `contracts:` target
(`Makefile:49`) invokes `$(MAKE) -C src/contract/<c>` with no target, and make's default
goal is the file's first target — `check-zkas-version` for 21 of them. So `make contracts`
was a silent no-op for those contracts: nothing built, and `check-source-hash` never ran.
Four contracts had no guard at all (`deployooor`, `bridge`, `dex`, `stablecoin`);
`deployooor` is the one that caused this incident, and `bridge` additionally never
followed the template and never passed any `RUSTFLAGS`. Fixed: `all` is the declared
default goal in every contract Makefile, and all four have a guard. A related trap was
found while verifying: `make` skips a recipe entirely when the target is newer than its
prerequisites, so a "rebuild" can be a no-op that also never re-records the hash — three
contracts were in exactly that state after the first pass.

**B4 — HIGH: the guard could not detect what broke it, and four recorded values were
already wrong.** `COMPUTE_SOURCE_HASH` was `$(shell cat $(ZK_SRC) | sha256sum | cut -d' '
-f1)` with `ZK_SRC := $(wildcard proof/*.zk)` — contents only, in glob order. It hashed
**no Rust source**, so the change that actually moved the genesis hash (`src/sdk`) was
invisible to it; paths were not included, so a rename or a content swap was invisible too;
and the glob order is not stable across filesystems, so a hash recorded on one machine
could fail on another for no reason. Measured: on `oracle`, `LC_ALL=C` reproduced
`9e0e0333…` while the committed `.source_hash` was `4339ab88…`. Consequently the committed
`.source_hash` for `attestation`, `identity`, `multisig` and `oracle` **did not match the
sources committed beside them** — the guard would false-fail on a clean checkout, had it
run at all (B3). Fixed: one sorted `path<TAB>sha256` manifest over the Rust sources, the
manifest and the circuits; all 32 recorded values regenerated by the build itself.

**B5 — MEDIUM: one genesis contract shipped a documented bypass of the guard.**
`native_token`'s Makefile carried an `update-hash` target that wrote the current source
hash without rebuilding, and the guard's own failure text advertised it ("Or run 'make
update-hash' to accept the new sources"). The recorded hash is what attests an artifact
was built from these sources, so a target that rewrites it on demand makes the attestation
false. Removed — it was the only one, and it was on a genesis contract.

**B6 — MEDIUM: three failing tests asserted on fabrications, not on the code.** Recorded
verbatim from the four-hour run:

```
registry::model::tests::sum_block_fee_v3_sums_plaintext_fees  panicked at registry/model.rs:758
  left: FeeAmount(0)   right: FeeAmount(42)
rpc::mm_rpc::tests::test_pow_source_discriminator  panicked at rpc/mm_rpc.rs:867
  left: 260   right: 228
rpc::mm_rpc::tests::test_mining_blob_len  panicked at rpc/mm_rpc.rs:846
  left: 260   right: 228
```

- The two `mm_rpc` failures were **stale literals in the test**, not a consensus defect:
  `MINING_BLOB_LEN` is 260 (`src/linear/src/block.rs:265`), the builder appends 32 bytes of
  `miner` after the discriminator (`:236-258`), and the crate's own test asserts 260
  (`:1059`). Only the test's hard-coded `228` was stale, left over from before the `miner`
  field was added to the blob. The discriminator offset was likewise a bare `blob[227]`
  even though its sibling had a named constant. Fixed: both now use
  `MINING_BLOB_LEN` / a new `POW_SOURCE_OFFSET`, and the crate's own test pins both offsets.
- `sum_block_fee_v3_sums_plaintext_fees` hand-built its call data as
  `[0x08] + fee.to_le_bytes()` padded to 444 bytes and called that "FeeParamsV3 encoded".
  It is not: the selector and length gates accept it (`src/sdk/src/mass_balance_call_data.rs:191`),
  but the function under test runs the real `FeeParamsV3::decode`
  (`src/contract/native_token/src/model/fee.rs:126`) on it and parses an `Input` out of
  `fee.to_le_bytes()` followed by zeros. Decode fails, the malformed call is skipped, and
  the sum stays zero — so the test asserted against its own fabrication. Rewritten to
  build a real `FeeParamsV3` and encode it with the real encoder.

**B7 — MEDIUM: the container is a second, divergent build path.** The devnet image builds
every contract with bare `cargo build --target wasm32-unknown-unknown -p <pkg>`
(`contrib/docker/darkwow-testnet/Dockerfile:81-98`): it never runs the Makefiles, so it
never saw the guard, the zkas-version check, or any `RUSTFLAGS`. Because genesis embeds the
WASM, a pin is only meaningful if both paths produce identical bytes, which is why the
remap has to be present in both and why a container cross-check gates the pin.

**B8 — LOW: seven non-genesis WASM files are tracked despite `.gitignore:*.wasm`**
(`baccarat`, `bearer_bond`, `betting_stake`, `darkbet_exchange`, `darktoshi_dice`,
`lottery`, `roulette`). None of the nine genesis WASMs is tracked, which is the important
half. The tracked seven are stale-prone: a fresh clone has them with an old mtime, so
`make` treats them as up to date.

**B9 — LOW: the genesis identity is duplicated.** The canonical `node0` secret is a string
literal in 20 places across the test tree (`wallet_sync_integration.rs:116`,
`genesis.rs:241,454,667,794`, `wallet_transfer_integration.rs:56,214,402`,
`wallet_integration.rs:87,946,1270,1488`, …) while a shared const,
`tests/modules/chain_setup.rs::GENESIS_KEYS_TOML`, exists and documents that every
genesis-building test MUST use it. The value is centralised; the reference is not.

**B10 — MEDIUM: seven of the 65 `UNVERIFIED` markers name commands that cannot run as
written, so what they guard was never exercised.** The markers exist to record "this code
has not been run yet"; a marker whose command is unrunnable converts that honest gap into
a permanent one, because nobody discovers the gap without attempting the command.

- Three cite package names that do not exist: `-p dwow-native-token-contract`
  (`src/contract/native_token/src/error.rs:44`, `src/contract/native_token/Cargo.toml:16`)
  and `-p dww` (`bin/dww/src/sync_task.rs:221`). The real packages are
  `dwow_native_token_contract` and `dwow_wallet` — the markers name *directories*. Both
  spellings abort with "did not match any packages".
- Four place `--test-threads=2` outside the `--` separator
  (`src/linear/src/validation.rs:69`, `src/linear/src/proof_of_token_balance.rs:65`,
  `src/sdk/src/tx.rs:104`, `bin/dwowd/Cargo.toml:32`). Test-harness flags must follow
  `--`; cargo rejects them in that position.
- Separately, `-j 2` appears in about fifteen markers as though it were a convention. It
  is build parallelism, not a requirement, and it changes nothing about what is verified.

This is worth recording because it is invisible from reading: a marker that names a
plausible-looking command is indistinguishable, on inspection, from one that is precise.
Many of these markers *are* precise — `HYG-3-2` correctly distinguishes the removed
`is_localnet` function from the `is_localnet` field that `rpc/miner.rs` still reads, and
`HYG-3-1`, `-3-3`, `-3-4` and `-3-5` all check out under grep. So the set is a mix, and
only running the commands separates the two.

**One correction to the record, not to the code.** An earlier claim in this work that
"everything was tested" was false. There is no `/tmp` evidence for
`cargo test -p dwowd --lib -- uncle_minting daemon_sync_integration` (that run was killed
mid-test), `./verify_cumulative_supply.sh`, `cargo test -p dwow-sdk`, the
`cargo check --features async` variants, or the `--test-threads=2` variants. The
`UNVERIFIED(...)` markers that name those commands were therefore left in place rather
than deleted, and `./verify_cumulative_supply.sh` needs a live docker devnet so no
host-side run can satisfy it.

---

## Verified clean

- **Genesis bootstrap.** `execution.rs::apply_genesis_deployments` checks
  `txs.len() == 1 + 9`, `is_genesis_deployment_tx`, `params.singleton`,
  `singleton_name == table[i].1`, non-empty WASM, and deploys to the **table's**
  `ContractId` (not `derive_public`) — position and singleton binding are sound.
- **The other six genesis contracts** (Oracle, Attestation, Purse, Box, MultiSig,
  PromissoryNote) are inert with respect to supply, the commitment tree and the
  nullifier SMT: their manifests match their registered circuits and dispatch.
- **`UncleMintUpdateV1`/`apply_uncle_mint`** were already commitment-only, which is
  why the mint's nullifier was decorative even before C12.

---

## Open items (not fixed in this pass)

1. **Spec-vs-Rust gap for the genesis filter modes** (C8 note above): `Off` /
   `Relaxed` / `Strict` and the Path-B tie-breaker are specified in the Python model
   but absent from the Rust.
2. **`compute_reward`/`verify_uncle_split` signature parity.** The connect path now
   passes a one-element slice to `verify_uncle_split`, which is equivalent but
   reads oddly; the model mirrors the slice signature, so changing it should be done
   in the model first.
3. **Persisting `uncle_commitment_set`** (the audit Pedersen commitments written in
   `connect_block`) is documented as Phase-2 work; a restart cannot reconstruct it.
   Needs a decision: persist, or derive on demand.
4. **`disconnect_block` symmetry** was reviewed for the uncle-note path (commitment
   only, now) but not diffed line-by-line against connect for every tree; the
   classification predicates in the two paths should be compared explicitly.
5. **`stored_uncle_hashes`** truncates sled keys to 32 bytes with zero padding
   (`chain_state.rs:704-712`). Correct while every key is a 32-byte blake3 hash, but
   fragile — any shorter key form would compare on zero-padded data.
6. **`competing_seen`** is not pruned; verify it cannot grow unbounded from network
   input.

---

## Verification of the fixes

Baseline and regression commands, plus the new tests, are recorded in the
implementation plan (`~/.claude/plans/i-want-a-full-hidden-shore.md`) — the
governing rule for this codebase is that the Python model is the executable spec, so
each fix landed spec-first and the model gained a failing-then-passing negative test
for C1, C4 and C7.
