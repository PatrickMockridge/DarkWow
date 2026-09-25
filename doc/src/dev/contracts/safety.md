# Smart Contract Inherent Safety

The failure classes that have actually occurred in this codebase, stated as root causes, with the
findings that taught each one as its evidence.

**Two companion documents, not three copies of this one:**

- [Contract Safety Checklist](checklist.md) — the operative pre-commit checklist. It is derived from
  the root causes below; this document is the explanation, that one is the instrument.
- [Verification Obligation Register](../../arch/verification-hazop.md) — every property the system
  must hold, where it is enforced, and whether anything checks it today. **Open obligations live
  there.** A resolved finding is a root cause entry below; an unresolved one is an `OBL-*` row there.

A finding is never recorded in both places. If you are looking for the current status of an open
item, it is in the register — not here.

## Fundamentals

Smart contract safety begins with a counterintuitive principle: **the safest code is the code you
never write**. Every feature is a potential vulnerability, every code path an attack surface, every
authorization check a point of failure. This is not a statement about code quality — it is about
combinatorial complexity. A contract with 3 functions has a manageable state space to audit. A
contract with 12 functions, ACL-gated minting, governance-controlled parameters, and cross-contract
child calls has an exponentially larger space of interactions to verify.

### The Principle of Minimum Functionality

```
Security ∝ 1 / (features × code_paths × authorization_gates)
```

Three corollaries:

1. **Isolate blast radius.** Put the minimum viable logic in the most frequently called contracts.
   Sophisticated business logic goes in less critical contracts where failures are contained.
2. **Remove, don't gate.** If you think a feature needs an ACL gate to be safe, ask whether the
   feature should exist at all. Authorization is itself an attack surface.
3. **Separate concerns by failure cost.** A bug in a DEX loses user funds for that trade. A bug in
   the consensus token loses block rewards for every miner. These are not the same severity.

### Design Exemplar: NativeToken vs PromissoryNote

The token architecture is the concrete expression of these principles: functionality split across
two contracts with deliberately asymmetric safety requirements.

**NativeToken** handles exactly what consensus requires — block rewards, fee payment, value
transfer — and is deliberately minimal:

| What it does | What it deliberately omits |
|---|---|
| PoW block rewards (`PoWRewardV1`) | No token freezing |
| Network fee payment (`FeeV3`) | No governance coupling |
| Private transfers (Mint/Burn/Transfer) | No multi-token support |
| | No authorization gates |
| | No token registry |
| | No business logic |

Every omission is a security property. No freeze means no freeze-key attack. No governance coupling
means no plutocratic takeover of consensus. No multi-token support means no token-ID confusion. No
authorization gates means no auth bypass. **In consensus-critical code, the feature you don't add is
the vulnerability you don't create.** NativeToken is the most frequently called contract in the
system; a bug here cascades to every transaction, every block, every miner reward.

**PromissoryNote** carries the business logic DeFi contracts must compose — multi-token support,
authorization, cross-contract value verification. Still minimal by DeFi standards (no AMM, no
lending pools, no governance), but more logic than NativeToken because composition demands it:

| What it adds | Why it's needed |
|---|---|
| `RegisterTypeV1` | Permissionless token creation for stablecoins, wrapped assets, LP tokens |
| Multi-token support (`asset_id`) | DEX, lending, yield — all need multiple token types |
| Token registry | Prevents unauthorized minting of unregistered token types |
| `BlindOutput_V1` circuit | Proves all output commitments are correctly formed, fully private |
| `validate_child_value_commit` | Lets a parent verify a child call's amount by commitment comparison |

**Why not one contract?** A monolithic token contract creates a single point of failure — a bug in
DeFi token logic would break consensus. Separating them gives failure isolation (a PromissoryNote bug
cannot break mining rewards), different audit postures (maximum review vs. flexibility), independent
evolution, and process safety (DeFi developers never touch consensus-critical code).

### The o-cap / ZK-proof symbiosis

Object-capability security and zero-knowledge proofs are complementary, not two independent design
choices. Each addresses a weakness in the other:

| O-cap provides | ZK-proof provides |
|---|---|
| Fine-grained per-action authorization | Hiding *who* holds the capability |
| State-machine transitions (produce/consume) | Hiding *which* capability is being exercised |
| Blast-radius containment (one cap = one action) | Hiding the relationship between capabilities |
| Auditable on-chain state (who can do what) | Unlinkability across transactions |
| Revocability (consume the cap) | Privacy of the revocation event |

**O-cap without ZK** is a permission system with full surveillance: every capability exercise is
visible, every holder linkable. Secure but not private. **ZK without o-cap** is a privacy layer over a
monolithic authorization scheme: you can hide who authorized an action, but one compromised key
controls everything. Private but not secure.

Together they give a system where each action needs one specific, unlinkable capability proof. This
is why contracts separate **capability derivation** (on-chain, per-instance, auditable) from
**capability exercise** (ZK-proven, off-chain, unlinkable). A `CapabilityId` is
`poseidon_hash(contract_id, capability_type, instance_seed)` — deterministic, unique per action per
instance, and meaningful only to someone who already knows all three inputs. The proof constrains
that the prover knows a valid capability secret without revealing which one. **The whole of RC8 is the
set of ways that separation leaks**, which is why it is stated here rather than with the findings.

---

## The Root Causes

Twelve classes account for every contract vulnerability found in this codebase's review history.
They are **root causes, not symptoms**: `MAX_BYTES` is wrong is a symptom; *all `MAX_BYTES` values
assume binary encoding but the encoding is JSON* is closer; *the encoding format is not enforced by
the type system, so `MAX_BYTES` values can silently mismatch* is the root cause.

| RC | Class | The rule in one line |
|----|-------|---------------------|
| [RC1](#rc1--a-check-that-cannot-fail) | A check that cannot fail | A function named `verify_*` must be able to return `Err` |
| [RC2](#rc2--a-witness-the-circuit-does-not-bind) | A witness the circuit does not bind | Every `constrain_instance(X)` needs an in-circuit derivation of `X` |
| [RC3](#rc3--type-erasure-at-the-hash-boundary) | Type erasure at the hash boundary | Every hash that stands for a typed value carries a domain separator |
| [RC4](#rc4--arithmetic-in-the-wrong-domain) | Arithmetic in the wrong domain | No `base_div` where the quantity is an integer |
| [RC5](#rc5--two-representations-of-one-fact-one-of-them-updated) | Two representations of one fact, one of them updated | Derive one side from the other, or make drift a compile error |
| [RC6](#rc6--irreversible-work-before-the-check-that-guards-it) | Irreversible work before the check that guards it | All fallible work precedes the destructive work |
| [RC7](#rc7--an-on-chain-invariant-computed-off-circuit) | An on-chain invariant computed off-circuit | Structural conservation is not cryptographic conservation |
| [RC8](#rc8--private-material-reaching-a-public-surface) | Private material reaching a public surface | Authorization goes in nullifiers; nothing identity-derived goes on-chain |
| [RC9](#rc9--safety-that-is-opt-in) | Safety that is opt-in | `Default::default()` is the secure configuration |
| [RC10](#rc10--a-value-with-no-bound) | A value with no bound | Every user-supplied value has a stated ceiling |
| [RC11](#rc11--a-consensus-path-that-can-disagree) | A consensus path that can disagree | Two nodes reading the same chain reach the same conclusion, bit for bit |
| [RC12](#rc12--an-error-dropped-panicked-on-or-made-indistinguishable) | An error dropped, panicked on, or made indistinguishable | Each failure has its own error and its own recovery |

### RC1 — A check that cannot fail

**What makes it possible.** A guard whose failure branch is a no-op, a verifier that only checks
shape, or a guard that is skipped when the thing it guards is unconfigured. The check is present in
the source, so review reads it as protection, but no input makes it return failure.

**How it has manifested.**

* *A two-step authorization whose second step never verifies the first.* PromissoryNote's
  `AuthTokenMintV1` → `IssueV1` pair: `IssueV1` accepted an `auth_proof` containing a nullifier and
  **never checked that the nullifier was spent**. The ZK proof verified correctly; the on-chain
  authorization step was decorative. Anyone could mint without ever calling the authorization
  function. Removed in May 2026 — `AuthTokenMintV1` and `RotateMintAuthorityV1` were deleted, and
  `IssueV1` now proves knowledge of the backing secret directly against the stored
  `token_auth_parent`. There is no prior step left to forget.
* *Stub verifiers.* The Bridge's `verify_xmr_deposit`, `verify_zcash_deposit` and siblings validated
  *shape* — non-empty, valid point, non-zero — and returned `Ok`, with the real cryptographic
  verification deferred behind `FIXME`. Any function named `verify_*` that returns `Ok(())` after
  checking `.is_empty()` and `.len()` is in this class.
* *Trusting a payload's self-description.* `BurnSpendHookPayload` carries `caller_contract_id`. A
  handler that trusts it accepts forged callbacks. The payload proves commitments were burned; it
  does not prove who initiated the burn.
* *A guard conditional on configuration.* `if value != ContractId::ZERO { validate(value) }` and
  `if let Some(token) = auth_token { check(token) }` mean an unconfigured deployment runs with the
  check disabled — the default deployment is the insecure one. See also RC9.
* *An authorization value the circuit never derived.* `IssueV1` exposed `public_key` as an independent
  witness, so a prover could mint commitments to keys they do not control — permanently unspendable
  rewards. Fixed by `constrain_equal_base(public_key, mint_public)` with `mint_public` bound to
  `backing_secret`. This is the circuit-side form of the same class; the circuit-side mechanism is
  RC2.
* *A placeholder that passes as a value.* `signature: pallas::Base::zero()` compiles wherever a
  signature is expected. A scalar zero is not a signature.
* *A verified artifact that does not have to be verified.* A VRF proof's output could be read without
  the proof ever being checked, because "has been verified" was a fact the caller had to remember
  rather than a property of the value. The repair shape is the type-state one — a `Verified<T>`
  constructible only by the verifying function, so passing an unverified value where a verified one is
  required does not compile. The same shape is the structural fix for the bridge's stub verifiers
  (`OBL-C21`), and it is the reason `verify_*` functions that return `Ok(())` are a code smell: their
  result is a unit, and a unit cannot carry the fact that the check happened.

**The rule.** **Deny by default: enumerate the conditions that permit access, and reject everything
else.** A function named `verify_*` must have a reachable `Err`. If step 2 requires step 1 to have
occurred, step 1's artifact must be verified in step 2 — or, better, the two steps must be collapsed
into one proof. In an o-cap system the single-step form is always available: prove knowledge of the
capability secret, and that proof *is* the authority.

**Detection.**

- For every `constrain_instance` of a value the entrypoint compares against on-chain state: does the
  circuit constrain how that value was derived? (RC2's check, same site.)
- Grep for `!= .*ZERO` guards, `unwrap_or(`, and `verify_` functions whose body has no `Err` return.
  A `verify_*` with no reachable error path is the defect, whatever it computes.
- For every cross-contract call, check **both** `contract_id` and `function_code`. Opcodes are
  namespaced per contract — `0x04` is `PromissoryNote::TransferV1` *and* `Attestation::VerifyClaimV1`.
  Checking `data[0]` alone is blind to which contract runs. This is the concrete form of its
  historical statement as "validate the target, not just the action".
- For every spend_hook receiver: `caller_contract_id` verified against a stored expected value,
  nullifiers tracked for replay, and the handler deterministic — it runs in the same overlay as the
  burn and must not consult oracle prices, cross-chain state, or anything that can change between
  proof generation and callback execution. Use `define_contract_with_spend_hook!`, not
  `define_contract!`.
- Is a signature-typed field declared as `pallas::Base` or `[u8; 32]` instead of `schnorr::Signature`?
  The type system is a security tool; raw scalar types invite `::zero()` and `::dummy()`.

**Taught by.** Lessons 1, 2, 11, 15; HAZOP RC1 sub-class B; old `RC-A`, `RC-F`; HAZOP
`pattern4_capability_bypass`. The class labels `ESC-001`, `INS-001`, `DAO-001`, `DAO-002`, `MV-001`
were used for it in an earlier review and are defined nowhere else.

---

### RC2 — A witness the circuit does not bind

**What makes it possible.** The prover supplies a value, the circuit publishes it, and nothing in
the circuit relates it to anything else. The statement verified is weaker than the statement
intended, and the gap is silent — the proof verifies.

**How it has manifested.**

* *The Orchard class: an unconstrained witness in a published position.* PromissoryNote's `Mint_V1`
  declared `mint_public` as a witness and exposed it via `constrain_instance` with **no constraint**
  relating it to `poseidon_hash(backing_secret)` — the `backing_secret` witness did not exist. The
  comment above the witness block asserted the derivation; a comment is not a constraint. A prover
  could read the public `stored_auth` from the token registry, set `mint_public = stored_auth`, and
  mint any registered token type. Fixed June 2026 by adding the secret witness and
  `constrain_equal_base(derived_mint_public, mint_public)`.
* *Vacuous acceptance through a conditional gadget.* `zero_cond(value, leaf)` returns the tree's zero
  leaf when `value == 0`. Without a `less_than_strict(ZERO, value)` guard, setting `value = 0` makes
  `merkle_root` succeed at every position in every tree — the prover forges inclusion without
  possessing a commitment.
* *A boolean check on an amount.* `bool_check(value)` is `small_range_check` with range 2. Applied to
  a u64 amount it restricts every operation to 0 or 1 token unit — a destructive constriction
  alongside an already-correct `range_check(64, value)`.
* *A u64 witness with no range check.* Field overflow in `amount`, `payment`, `stake`, `fee` produces
  arithmetic results with no relation to the integers intended.
* *An output with no proof of formation.* `TransferV1` and `OtcSwapV1` outputs were constructed
  client-side with only a uniqueness check on-chain. A buggy or malicious client could inject
  commitments that were never proven correctly formed.

**The rule.** **Every `constrain_instance(X)` must have an in-circuit derivation
`X = f(witnesses)`.** A published value with no derivation is a free variable and the circuit proves
nothing about it. Every u64-valued witness gets `range_check(64, value)` before entering arithmetic.
Every conditional gadget whose output feeds a cryptographic verifier must constrain its branch
condition. Every output carries a ZK proof of correct formation — client-side construction is not a
security boundary.

**What is mechanized, and what is not.** The rule is enforced structurally over the circuit sources
by `script/circuit_instance_derivation.py` (gate: `scripts/check-circuit-instance-derivation.sh`),
which classifies every `constrain_instance` in all `.zk` files as derived, bound, redundant, or
declared free with a host-side mechanism in `script/circuit_free_instances.txt`. Two things it does
**not** establish, and they must not be read as settled: a *derivation* proving what the circuit
intends is the opcode-semantics layer, still named by the uninterpreted, unconsumed obligation
`Axioms.NoFreeInstances`; and `ECOps.detect_orchard_class_vulnerability` is a `def` over a *modelled*
gadget that carries no weight for a real circuit. This is **not** "the circuits are formally
verified" — `proofs/lean/src/DarkFi/Circuits/` contains no Lean declarations, and an earlier version
of this document said otherwise.

**Detection.**

- For every `constrain_instance(X)` in a `.zk` circuit: is `X` a witness? Trace it. Is it constrained
  equal to a circuit-computed value via `constrain_equal_base`? Is it an input to a computed value
  that is published? Is it declared and never referenced (an unused witness — dead weight in the
  proving key)? A witness satisfying none of these is free.
- Every `zero_cond(value, leaf)` feeding `merkle_root` is preceded by `less_than_strict(ZERO, value)`.
- Every `Base` witness representing a u64 quantity has `range_check(64, value)`.
- `bool_check` does not appear on a u64-valued witness.
- Every output commitment has a `BlindOutput_V1` proof — no conditional privacy leakage.

**Taught by.** Lesson 16; Orchard lesson 20; HAZOP RC1 (sub-classes A and E), RC2; old `RC-A`
(reference-defect half), `RC-I`; HAZOP `pattern1_free_instance`, `pattern2_zero_cond`,
`pattern6_bool_check_u64`, `pattern7_missing_range`. Open residue: `OBL-Z16` (fifteen sites),
`OBL-Z17` (the authorization primitive itself), `OBL-Z6`.

---

### RC3 — Type erasure at the hash boundary

**What makes it possible.** `poseidon_hash` maps every typed input to the same output type. A
nullifier hash, a token commitment and a Merkle leaf are all `pallas::Base` and are
indistinguishable without a domain separator. Two types that must not unify do unify at that
boundary.

**How it has manifested.**

* *Undifferentiated hashes across 177 circuits.* A nullifier hash in one circuit was bitwise
  identical to a token commitment hash in another, given the same inputs. Every `poseidon_hash`
  invocation needed a domain constant as its first argument. Fixed by porting every circuit to a V2
  namespace with `DOMAIN_*` constants, enforced permanently by
  `scripts/check-circuit-domain-separation.sh`.
* *A nullifier not bound to its operation.* A nullifier proves a commitment is spent; it does not say
  *for what*. Without binding, the same commitments can be resubmitted for a different proposal,
  swap, or job — bypassing a threshold because the nullifier is not linked to any specific operation.
  The DAO proposal-input-reuse exploit is the canonical instance. Fix:
  `input_nullifier = poseidon_hash(commitment_nullifier, operation_bulla)`.
* *A type declaration that erased a type.* The zkas compiler added `Base` as a constant type and
  circuits recompiled with it; the VM synthesizer recognised only four magic constant names. Any
  circuit with a `Base` constant outside those four crashed at keygen. Part of the problem was a
  declaration with no functional purpose — `constrain_instance` already bound the public input.
* *A shared domain between two different roles.* Schnorr signing used the same domain separator for
  the nonce and the challenge, which is a Fiat-Shamir violation: the two values play different roles
  in the transcript and must not be interchangeable. Separate domain constants fixed it. This is the
  same rule as the 177 circuits — a separator identifies a *purpose*, so two purposes cannot share one.

**The rule.** **Every hash that stands for a typed value carries a domain separator identifying its
semantic purpose.** `poseidon_hash` is a type-erasure boundary; the domain constant restores the
distinction. And **every input nullifier is bound to the operation it authorizes** — a nullifier
that says "I spent commitment X" without saying "for purpose Y" lets X be spent for Y, Z and W
simultaneously. The rule is the same in both places: the value's *position* must be part of the value.

**Detection.**

- Every `poseidon_hash(...)` in every `.zk` circuit prepends a `DOMAIN_*` constant as its first
  argument. `grep "poseidon_hash(" proof/*.zk | grep -v "DOMAIN_"` returns nothing.
- For every `constrain_instance` of a nullifier: is an operation-specific identifier (bulla, proposal
  ID, swap ID, job ID) also constrained and bound to it?
- The domain vocabulary, cross-circuit: `1 = NULLIFIER`, `2 = TOKEN_COMMIT`, `3 = TX_BINDING`,
  `4 = COIN_COMMIT`, `5 = MERKLE_LEAF`, `6 = USER_DATA_ENC`, `7 = SIGNATURE_SECRET`.

**Taught by.** HAZOP RC3; lessons 14 and 22 (metadata drift); old `RC-D`; HAZOP
`pattern5_nullifier_collision`. Closed and gated: `OBL-Z2`, `OBL-Z7`.

---

### RC4 — Arithmetic in the wrong domain

**What makes it possible.** A field operation used where an integer operation was meant. The two
agree on small inputs often enough to pass tests, and diverge exactly on the inputs an adversary
chooses.

**How it has manifested.** `base_div(a, b)` computes `a · b^(p−2) mod p` — Fermat inversion, a
*field* element. Applied to `fee = amount · bps / 10000`, to interest accrual, and to oracle price
aggregation, it produces a modular inverse, not a truncated quotient. For integers that do not divide
evenly in the field the result is a large field element with no relationship to the intended
quotient. Eight circuits across seven contracts were affected.

**The rule.** **`base_div` must not appear in any circuit with u64-valued witnesses.** Integer
division is stated as quotient–remainder constraints:

```
# floor(a/b) = q  ⇔  q*b <= a < (q+1)*b
q_times_b = base_mul(q, b);
constrain_equal_base(less_than_or_equal(q_times_b, a), ONE);
less_than_strict(a, base_mul(base_add(q, ONE), b));
```

For ratio comparisons, cross-multiply and drop the division entirely: `a/b ≥ t` becomes
`a · t_scale ≥ t · b`.

**Detection.** `grep base_div proof/*.zk` in any contract with u64 parameters. Any surviving
`base_div` needs a proof that its operands are field quantities.

**Taught by.** HAZOP RC4; HAZOP `pattern3_field_div`. The mechanized form is
`proofs/lean/src/DarkFi/BaseDivGadget.lean`; the step from the gadget to the integers it stands for
is `OBL-Z12`, still open.

---

### RC5 — Two representations of one fact, one of them updated

**What makes it possible.** The same fact is written down twice — in two languages, two files, two
layers, or a copy of itself — and only one side is edited. Nothing in the build compares them.

**How it has manifested.** This is the most frequently recurring root cause in the codebase; the
instances are worth listing, because the recurrence is the point.

* *Circuit vs. Rust metadata.* The domain-separation migration added `DOMAIN_*` constants to the
  circuits and left the Rust side computing the same values without them. Poseidon over
  `ConstantLength<N>` produces different outputs for different arities — the circuit's 4-input
  nullifier never equalled the harness's 2-input one, so every `constrain_instance` derived from a
  domain-separated hash verified against the wrong value. Eight mismatches in Box, twelve in Purse.
  The structural fix is that **metadata is a pure echo** — see the four-component law below.
* *Fixed origin, unfixed copies.* bearer_bond's `burn_v1.zk`, `redeem_v1.zk` and `blind_output_v1.zk`
  are copies of promissory_note circuits and lacked its fixes: `redeem` was missing
  `constrain_equal_base(value, ZERO)`, `burn` was missing the per-burn `signature_secret` derivation.
  Copies were not tracked as derivatives, so a fix to the original reached nothing.
* *Capability descriptor vs. dispatch table.* Every descriptor had at least one error — wrong
  `function_id`, missing action, wrong field name, nesting that would not compile. Descriptors are
  what the host runtime uses to decide which calls are permitted, so a wrong `function_id` authorizes
  the wrong function.
* *Client `to_vec()` order vs. circuit instance order.* Orderings that disagree make the proof verify
  against a mangled instance vector.
* *Manifest vs. circuit declaration vs. Rust namespace constant.* Three spellings of one circuit
  name. Manifests were found referencing V1 circuit names stale by over three months; ~70 namespace
  constants pointed at circuits that did not exist.
* *Makefile output vs. `include_bytes!` path.* 327 `include_bytes!` calls referenced `_v2.zk.bin`
  paths the Makefile never produces (`$(ZK_SRC:.zk=.zk.bin)` is a direct stem substitution).
* *File rename vs. `mod` declaration.* 39 module files were renamed to drop `_v1` and the `mod`
  declarations were not updated — six contracts stopped compiling for an unknown period.
* *Compiler vs. synthesizer.* The zkas compiler gained a feature the VM had not learned. The
  `.zk.bin` format is the contract between them; it changed on one side only.
* *Compiler vs. decoder.* zkas accepted variable reassignment chains and emitted a `.zk.bin` without
  error, which `zkas validate` then rejected as corrupted (`base_add references heap idx 48 but only
  48 entries available`) — the compiler's name resolution and the decoder's heap validation held two
  different models of the same file.
* *Spec vs. code.* A documented exponential-decay formula implemented as a linear approximation; a
  comment claiming sorted keys make `serde_json` deterministic across versions.
* *A fix applied at one site and not its siblings.* The gas-exhaustion check existed and was applied
  to one of ten host functions; `is_gas_exhausted()` was written and never called. The
  domain-separation architecture existed and 150 circuits had not been ported to it.

**The rule.** **Where the same fact must be written twice, derive one side from the other or make
disagreement impossible to compile.** Where derivation is not available, add a gate that compares
them: `scripts/check-circuit-domain-separation.sh` and
`scripts/check-circuit-metadata-alignment.sh` exist for exactly this reason. A pattern established at
one site is not established — enumerate its siblings and apply it to all of them, or the remaining
sites are the bug.

**Detection.**

- For every `constrain_instance(X)` where `X` involves a hash: does the Rust-side metadata function
  compute `X` identically — same arity, same order, same domain constant? If it computes anything at
  all, it is a second source of truth (see the four-component law).
- Every `.zk` file that is a copy of another declares its provenance.
- Descriptor `function_id`s are verified against the entrypoint dispatch table.
- Circuit/client/entrypoint agree on public-input order, position for position.
- `[[circuits]].name`, the `.zk` `circuit` string, and the Rust namespace constant are
  character-identical.
- A new zkas feature is paired with synthesizer support before any production circuit uses it; after
  any compiler change, recompile a known-good circuit and confirm `ProvingKey::build` succeeds.
- Every `.zk.bin` passes `zkas validate` before being embedded. `make all` does not do this.
- When a fix lands at one site, grep for its siblings before calling it done.

**Taught by.** Lessons 10, 12, 13, 21, 22 (both), 24, 25; HAZOP RC5; old `RC-C` (propagation),
`RC-G` (partly), `RC-I`, `RC-D` (deployment half); HAZOP `pattern1` (redundant/declared instances).
Open residue: `OBL-Z1`, `OBL-Z5`, `OBL-Z8`, `OBL-Z13`.

---

### RC6 — Irreversible work before the check that guards it

**What makes it possible.** The operation that cannot be undone happens before the check that could
have rejected it. The check runs, may even fail — but the state it was meant to protect is already
gone.

**How it has manifested.**

* *A validation after the atomic commit.* Coinbase maturity was checked *after* `sled::Batch::apply`,
  with no rollback path — a maturity failure left the block committed. The architecture had split
  validation into a pre-commit phase (structure, PoW, WASM execution) and a post-commit phase
  (maturity, signature consumption) without a transaction for the second.
* *Data consumed before the fallible operation.* `take_competing_blocks()` removed entries before
  subsequent fallible work; if that work failed, the data was lost. No savepoint existed.
* *A merge that overwrites rather than detects.* Every contract call received
  `base_overlay.clone()`, so no call saw another's writes; diffs were merged with
  `main_overlay.add_diff(diff)`, which silently overwrites duplicate keys. Two transactions spending
  the same commitment in the same block both passed their exec-phase nullifier checks and both
  appeared to succeed. Resolved: a shared overlay model for canonical calls, uncle-vs-uncle and
  Deployooor write-key conflict detection, and mempool nullifier deduplication at admission.

**The rule.** **All fallible work precedes the destructive work.** After the atomic commit, nothing
may reject the block — validation that can fail belongs before it. Where two operations can write the
same key, the merge must *detect* the conflict rather than resolve it by overwriting; silent
overwrite is never safe for value-bearing state.

**Detection.** For every `sled::Batch::apply` (or equivalent commit), list the validations that
follow it. For every `take_*`/`remove`/`delete`, list the fallible operations that follow it. For
every merge of concurrent write sets, state what happens on a duplicate key.

**Taught by.** Old `RC-B`; HAZID RC4; lesson 19 — and the coinbase-maturity instance of it is now
*closed*: the check at `src/linear/src/chain_state.rs:1117` precedes every `apply_batch` at
`:1340-1349`, both inside `connect_block`. The register records the closure so it is not re-carried.

---

### RC7 — An on-chain invariant computed off-circuit

**What makes it possible.** A relationship that the protocol depends on is established by the
client, which is a convenience rather than a security boundary. The circuit verifies each value
individually and never relates them.

**How it has manifested.**

* *Fee subtraction computed in Rust.* NativeToken's `FeeV1` circuit contained **zero** constraint
  linking `input_value` and `output_value`; `output_value = input_value − fee` was computed
  client-side. A prover could set `output_value = input_value + 1_000_000` and generate a valid
  proof. The 1-in-1-out structure provided no conservation at all. `TransferV1` likewise lacked the
  cross-proof Pedersen sum check that PromissoryNote's `verify_value_conservation()` performs.
* *Two independent witnesses where one derivation was needed.* Burn circuits had separate
  `spend_secret` (nullifier) and `signature_secret` (signing) witnesses with no cross-constraint, so
  the commitment owner and the transaction signer could be different entities. The fix derives
  `signature_secret = poseidon_hash(spend_secret, nullifier)` — binding signer to owner while keeping
  each burn unlinkable. The first attempt exposed `pub = ec_mul_base(spend_secret, K)` directly, which
  fixed the separation and created a privacy regression: every burn from one owner revealed the same
  static public key.
* *A child call's amount taken on faith.* A parent calling `promissory_note::transfer_v1` could see
  that a transfer existed but not that it transferred the expected amount — the amount was inside an
  `AeadEncryptedNote` the parent cannot decrypt, and the `value_commit` blind was unknown to it.
* *No second witness to supply integrity.* The Orchard pool's integrity rested entirely on
  per-transaction balance checks. One missing constraint collapsed the edifice, and because the pool
  is fully shielded, counterfeit commitments are cryptographically indistinguishable from legitimate
  ones — there is no way to determine whether the bug was ever exploited.

**The rule.** **Structural conservation is not cryptographic conservation.** Every value
transformation performed off-circuit (fee subtraction, interest accrual, exchange-rate conversion)
is constrained in-circuit. Where a parent must verify a child's amount, use the commitment already
present — `value_commit = poseidon_hash(value, value_blind)` with the blind derived deterministically
from the parent's own unique state — rather than adding a plaintext field. The fix for the
`public_value` attempt is the model: the `Option<u64>` field *looked* optional but was mandatory for
any composed transfer, so it made privacy conditional and broke it for exactly the case composition
exists to serve. When you find yourself adding a field that violates a core design constraint to
solve a verification problem, the answer is to use the commitments you already have.

And for any shielded asset: **ask whether a circuit bug allowing unbounded minting would be
detectable.** If not, the asset needs a cumulative commitment chain independent of any single
circuit. The test is not "has the circuit been reviewed?" — Orchard's was reviewed for four years.
NativeToken's answer is the Pedersen cumulative chain `S_H = S_{H-1} + C_H`, verified two independent
ways: the `Mint_V1` circuit constrains `ec_add(S_{H-1}, C_H) == S_H` (depends on Halo2 soundness),
and any node can check the identity in pure Pedersen arithmetic via the
`blockchain.get_cumulative_supply` RPC without verifying a proof (depends on Pedersen binding). To
hide inflation an attacker must break both simultaneously. The burden of proof falls on the miners
extending the chain, whose rewards are public — not on private users.

**Detection.** For every value transformation in a contract: is the relationship constrained
in-circuit, or asserted by the client? For every parent that moves value via a child: is the child's
`value_commit` recomputed and compared? For every authorization witness pair (`spend_*`,
`signature_*`): is the derivation constrained?

**Taught by.** Lessons 3, 4, 17, 18, 20. Registers as `OBL-C1`, `OBL-C3`, `OBL-C4`, `OBL-C5`.

---

### RC8 — Private material reaching a public surface

**What makes it possible.** A value that identifies a participant, or a secret used to authorize
them, is placed where an observer can read it, enumerate it, or correlate it. The o-cap model's
guarantee is unlinkability; this class is the ways it leaks.

**How it has manifested.**

* *A public key as a database key.* `db_set(relayers_db, &serialize(&relayer_pub), …)` makes identity
  the primary lookup dimension: anyone who knows a pubkey enumerates every record for it. The same
  pattern appeared for issuers, for `(capability_id, holder_pub)` composite keys, and for
  `(proposal_id, voter_pubkey)` — which reveals exactly how each voter voted.
* *A signature key reused across transactions.* `signature_public` was documented as a generic
  "signature public key" and builders accepted a full `Keypair` or an unconstrained secret. Reusing
  the wallet key gives every transaction the same on-chain identity link. The fix names the field
  `ephemeral_signature_secret` so the invariant is unmissable, and removes `Keypair` from builders.
* *Identity smuggled into an opaque field.* `user_data = poseidon_hash([..., sender_pub])` where
  `sender_pub = poseidon_hash([owner_secret])` — a deterministic fingerprint of the owner, committed
  into every mint and passed as a public input. Authorization was already covered by the nullifier;
  the identity added no security and removed privacy.
* *Identity fragments in a token ID.* `token_auth_parent = authority_pub[0..8]` embeds the creator in
  every commitment of that token. The token ID needs to be unique, not identity-bearing: the
  authority relation is a capability proven by the mint flow, not a fact about the ID.
* *The wallet keypair in a client builder.* `signature_keypair: Keypair` on a builder invites reuse,
  and one builder serialised the wallet secret into the note memo. Accept only the individual secrets
  an operation needs; the memo needs no secret at all.
* *One wallet key across contract instances.* The same pubkey as `owner_pubkey`, `member_pub` and
  `staker_pub` across instances links every contract a user touches. Each instance derives its own
  key via `SecretKey::derive_instance(contract_id, instance_seed)`, with a random `instance_seed`
  stored on-chain so the wallet can reconstruct it without a circular dependency.
* *A secret in a derived trait.* `SecretKey`'s and `Blind<F>`'s derived `Debug`/`Display` printed raw
  field elements. Fixed by manual `<redacted>` impls, `Drop` zeroisation, and gating `Display` behind
  a feature flag — but each such type needs individual audit, which is why a lint matters more than
  the fix. The same class, before it was found: `SecretKey` derived `Copy` and had no `Drop`, so key
  material was duplicated around the stack and never zeroised. A secret type must be neither `Copy`
  nor printable nor undroppable, and no one of the three is sufficient alone.

**The rule.** **Authorization belongs in nullifiers, not in auxiliary data; and nothing
identity-derived goes on-chain.** Hash identity material before using it as a key of any kind. Derive
per-instance keys rather than reusing one. Never let a builder hold a wallet keypair, and never let a
sensitive type derive a formatting trait.

Note the boundary — this class is *not* "all plaintext is a leak". **Intentional transparency** keeps
a field public where the aggregate is legitimately shared: bridge withdrawal amounts (both chains
know them), stablecoin pool totals (ratio checks need them), DEX order-book prices (hidden prices
prevent matching), network fees (public by design). **The heuristic:** if the value updates a global
aggregate other users depend on, it is legitimately public; if it is needed only by the two
counterparties to a transfer, it belongs behind a commitment.

**Detection.**

- Is a raw public key used as a database key? Replace with `poseidon_hash(chunks)` — lookup is
  preserved for anyone who knows the key, enumeration is not.
- Could a signature public key be reused across transactions? Is the field named `ephemeral_*`?
- Does `user_data` or any opaque field encode identity material? Grep for `poseidon_hash([owner_secret])`
  or `sender_pub` in `user_data` derivations.
- Does a token-ID derivation use identity-linked inputs?
- Does a client builder carry a full `Keypair`?
- Does the same raw wallet pubkey appear across multiple contract instances?
- Does any type in `src/sdk/src/crypto/` that wraps a field element derive `Debug` or `Display`?

**Taught by.** Lessons 5, 6, 7, 8, 9; old `RC-E`; the flakey-pattern rows for shared pubkeys and
placeholder signatures.

---

### RC9 — Safety that is opt-in

**What makes it possible.** The secure configuration is available, requires deliberate action, and is
not the default. A developer who does not read every optional setter gets the insecure system.

**How it has manifested.**

* *Client builders defaulting to the least-private value.* `nonce` and `secret_nonce` defaulted to
  `pallas::Base::zero()`, so every call with the same parameters from the same caller produced an
  identical, trivially linkable commitment. The setter existed; the default was the trap. Fixed across
  eight builders by defaulting to `pallas::Base::random(&mut OsRng)`.
* *Safety features off by default.* `DrainConfig { circuit_breaker: None, exit_queue: None }` as
  `Default` means every deployment starts unprotected and operators must opt in.
* *A random seed supplied as a constant.* The generic-prover path passed `[0u8; 32]` where the shell's
  random seed belonged, making every `blind:<name>` witness a publicly-known constant.
* *An AEAD nonce of zeros.* Notes encrypted under a fixed `[0u8; 12]` nonce rather than one derived
  from the ephemeral public key.
* *A default key that is public.* `Keypair::default()` set the secret to `42`, so any
  `unwrap_or_default()` on a keypair produced a private key the whole world holds. The `Default` impl
  was removed rather than re-seeded — a type for which no safe default exists should have none.
* *A default password.* A wallet shipped with `"changeme"`.

**The rule.** **`Default::default()` must be the secure configuration.** Opt out for exceptions;
never opt in for safety. A developer who calls `Builder::new()` without reading the optional setters
should get the private, protected behaviour.

**Detection.** Read the `Default` impl of every config struct and every builder. Does the default
produce a private, protected instance? Grep for `zero()` and literal-constant defaults in
nonce/blind/seed positions.

**Taught by.** Design Principle 2; old `RC-F` (defaults half); the `DRAIN-001` class label.

---

### RC10 — A value with no bound

**What makes it possible.** A parameter is accepted without a stated ceiling, so its cost, its
lifetime, or its effect is unbounded.

**How it has manifested.**

* *Temporal parameters unchecked.* A slash attestation accepting a `block_height` from any block —
  including the future — lets a relayer pre-register a slash for block N+1000 and block the real
  attestations at that height via the idempotency check.
* *Creation with no deactivation path.* `active: bool` flags with no function that clears them:
  underwriters could not resign, markets could not close, risk types could not be retired, governance
  could not be paused, capabilities could not be revoked. Every record, once created, was permanently
  active. Seven deactivation functions were added across four contracts.
* *User-supplied collections iterated without a limit.* Not a DoS vector under the current runtime
  ceiling, but the ceiling produces an opaque WASM trap rather than a diagnosable error, and as gas
  metering evolves the cost becomes proportional and unpredictable. Five per-call `MAX_*` constants
  were added (`DARKBET_EXCHANGE_MAX_SETTLE_MATCHES`, `POOL_STAKE_MAX_REBALANCE_MEMBERS`,
  `RELAYER_ENDOWMENT_MAX_ALLOCATIONS`, `IDENTITY_CONTRACT_MAX_DAG_CREDENTIALS`,
  `ROULETTE_CONTRACT_MAX_SETTLE_BETS`, all 100).
* *Governance-configurable values with no sanity bounds.* Bridge confirmation counts and fee
  ceilings could be set to any value; `max_deposit`/`max_withdrawal` were parsed and never written to
  state at all.
* *Costs that scale without a limit.* O(n) chain traversal in `get_next_work_required`; a verifying-key
  cache whose eviction was by insertion order and therefore manipulable.

**The rule.** **Every user-supplied value has a stated ceiling, and every `active` flag has a
function that clears it.** Creation without deletion is a state leak. An assertion with a clear
message ("Too many match IDs for settle") is debuggable; a runtime-limit trap is not. Choose bounds
generous enough for legitimate use and small enough to keep cost predictable — functions needing more
can be called repeatedly with different slices. Temporal parameters are bounded on both sides:
`block_height <= current_block` and `current_block - block_height <= MAX_AGE`.

**Detection.** For every `create`/`register` function, is there a corresponding deactivate? For every
`Vec` parameter that is iterated, is there a `MAX_*` assertion before the loop? For every
`block_height` parameter, is it bounded? For every governance-settable value, are there bounds?

**Taught by.** Design Principles 3 and 4; old `RC-H`; the `ATTEST-001`, `BET-001`, `POOL-001` class
labels.

---

### RC11 — A consensus path that can disagree

**What makes it possible.** Two nodes read the same chain data and reach different conclusions,
because the path depends on something that is not a function of the chain — memory ordering, hash
iteration order, wall-clock time, or a serialization that is not byte-deterministic.

**How it has manifested.**

* *A non-deterministic serializer in consensus paths.* `serde_json` used for block storage, block-size
  measurement, and competing-block dedup hashing, with a comment claiming sorted keys make it
  deterministic across serde versions. The codebase has `dwow_serial` for exactly this.
* *Relaxed atomics and saturating arithmetic masking invariant violations.* `saturating_*` where
  `checked_*` was meant turns an overflow bug into a silent wrong answer; `Ordering` other than
  Acquire/Release on consensus atomics admits reordering.
* *Hash-map iteration order feeding consensus decisions.* Without a deterministic tiebreaker, two
  nodes enumerate differently.
* *A soft time limit.* A 30-second wall-clock warning that does not terminate still lets a slow node
  and a fast node disagree about whether a call completed.
* *A feature scanner with an incomplete deny-list.* WASM threads/atomics (`0xFE`) were not rejected.
  Disabling the scanner entirely is worse than the gap it closes.

**The rule.** **Two nodes reading the same chain must reach the same conclusion, bit for bit.**
Consensus paths use `dwow_serial`, `checked_*`, deterministic ordering with explicit tiebreakers, and
non-relaxed atomics. Nothing in a consensus decision path may depend on wall-clock time, address
space, or iteration order.

**Detection.** Grep `src/linear/` and `bin/dwowd/` for `serde_json`, `HashMap::iter`, `Ordering::Relaxed`,
and `saturating_` in comparison or invariant positions. Any such site in a consensus decision path is
in this class.

**Taught by.** HAZID RC3, RC6 (arithmetic half); old `RC-G`, `RC-I`.

---

### RC12 — An error dropped, panicked on, or made indistinguishable

**What makes it possible.** A `Result` is discarded, unwrapped, or collapsed into an error code that
several distinct failures share. The failure is either not handled, or handled indistinguishably.

**How it has manifested.**

* *A derive macro below the type system.* Every contract originally used
  `#[derive(SerialEncodable, SerialDecodable)]` on state and parameter types. The derived `Decodable`
  reads `pallas::Base` directly — it never calls the validating `Nullifier::from_bytes()` and silently
  accepts zero and non-canonical values. A `serialize(&purse)` was observed to produce 9 bytes where
  129 were expected, accepted without complaint, yielding a corrupt Purse with zeroed fields. The
  error surfaced only as `ContractError::IoError("Unknown")` at the WASM boundary. Every contract now
  uses explicit `encode()`/`decode()` with fixed layouts and per-field validation through named
  constructors.
* *A host function with one error code for eighteen failures.* `merkle_add` had 18 distinct failure
  sites, all returning `ContractError::Internal` (code 2). A failing heavyweight test reported
  `ContractError(Internal)` — impossible to attribute. Replacing them with six distinct variants plus
  three existing ones produced `ContractError(DbGetEmpty)`, which identified the cause in minutes (a
  guard in `init_contract` skipping tree initialisation).
* *Errors substituted with defaults.* `.unwrap_or(default)` on a PoW hash comparison, on chain-state
  reads, and on `let _ = set_return_data(...)` turns a failure into a silently wrong consensus value.
  Consensus code in particular must fail closed rather than substitute.
* *A panic on untrusted input.* Raw `.unwrap()`/`.expect()` throughout production code — RandomX
  `.expect()` in chain state, `Scalar::from_repr().unwrap()` at a crypto boundary, `File::create()
  .unwrap()`. A panic is a liveness failure on input an adversary chooses.

**The rule.** **Every failure site has its own error, and every `Result` is acted on.** The policy is
enforced at compile time:

```rust
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
```

A panic-capable unwrap that is *provably safe* — a locally-provable invariant, a compile-time
constant, an FFI or byte-encode boundary, a length-checked `try_into()` — is a documented
dispensation rather than a bare `.unwrap()`:

```rust
#[expect(clippy::unwrap_used, reason = "type-system.md §2.3.2 — len checked above")]
```

`#[expect]` documents the reason *and* warns if the unwrap is later removed, so the annotation cannot
go stale.

In host functions serving hard-path contracts, `ContractError::Internal` (code 2) SHALL NOT be a
catch-all: each distinct recoverable failure — missing data, corrupt data, handle out of bounds,
deserialization failure — gets its own variant. It is reserved for genuinely unrecoverable conditions
(host memory faults, environment crashes). New variants follow the `ComponentOperationFailed` naming
convention, and the host-side `error!()` log includes the contract ID and relevant keys.

> **A naming collision worth knowing.** `safety.md C1` in a `#[expect(…, reason = …)]` under
> `bin/dwowd/src/` refers to the **unwrap-audit tier C1** (RandomX `.expect()`), not to the red-team
> finding `C-1` and not to HAZOP root cause `RC1`. The tiers were `C1` (RandomX `.expect()`), `C2`
> (`unwrap_or([0u8; N])` PoW bypass), `H3` (`Scalar::from_repr().unwrap()`), `H4` (precision loss),
> `M1–M7` (mutex poison, daemon/miner/client unwraps).

**Detection.** `cargo clippy` with the two denies above is the instrument. Beyond it: is every
`Result` from a host function propagated or explicitly matched? Does any `Err` path share the
`Internal` code with a different cause? Does a consensus path use `unwrap_or` where `?` belongs?

**Taught by.** Lesson 21; the raw-unwrap/expect audit; the error-propagation rule; HAZID RC1
(swallowed-error half).

---

## Flakey Patterns: How These Fail Review

A **flakey pattern** passes functional tests while violating a core architectural invariant. It looks
correct in isolation — the code compiles, the tests pass, the immediate problem is solved — and it
undermines the property the system exists to provide. These are the most dangerous bugs because they
survive review and automated testing. They are also the primary way **blast radius expands without
anyone noticing**: one flakey signature check in a shared validation path turns a single-capability
compromise into a cross-contract exploit.

### Anatomy

Every flakey pattern has three characteristics:

1. **Solves the immediate problem** — the functional requirement is met.
2. **Breaks a core invariant** — a non-negotiable constraint is sacrificed.
3. **Disguises the breakage** — hidden behind optional types, configurable defaults, or conditional
   logic that makes it look safe. `Option<u64>` *looks* like privacy is preserved.

### Warning signs

| Signal | Example | Root cause |
|---|---|---|
| Plaintext data in a privacy struct | `public_value: Option<u64>` on `Output` | RC8 |
| Optional fields that are mandatory for correctness | `public_value` must be `Some(..)` for any composed transfer | RC8 |
| A new circuit revealing what the old one hid | `TransferOutput_V1` vs `BlindOutput_V1` | RC8 |
| A field added to satisfy one caller | Bridge needed amount verification → `Output` grew a field | RC8 |
| Type-level safety without invariant enforcement | `Option<u64>` is type-safe and enforces nothing | RC8 |
| "Backed by a ZK proof" without an on-chain check | `auth_proof` fields only ZK-verified | RC1 |
| An opcode check without a contract-ID check | `data[0] == 0x04` alone | RC1 |
| A raw pubkey as a database key | `db_set(relayers_db, &serialize(&relayer_pub), …)` | RC8 |
| A signature secret shared across transactions | `signature_secret` reused from the wallet key | RC8 |
| Identity material in an opaque field | `user_data = poseidon_hash([.., sender_pub])` | RC8 |
| Identity fragments in a token derivation | `token_auth_parent = authority_pub[..8]` | RC8 |
| A full wallet keypair in a client builder | `signature_keypair: Keypair` on a builder | RC8 |
| One raw pubkey across contract instances | Same pubkey for `owner_pubkey`, `member_pub`, `staker_pub` | RC8 |
| A silent authorization bypass | `verify_capability_for_action` returns `Ok(())` when governance is inactive | RC1 |
| A placeholder signature in production params | `signature: pallas::Base::zero()` | RC1 |
| Safety features disabled by default | `DrainConfig { circuit_breaker: None, .. }` as `Default` | RC9 |
| Missing temporal validation | A slash attestation accepting any `block_height` | RC10 |
| A descriptor out of sync with dispatch | Descriptor says `0x01`, dispatch maps `0x00` | RC5 |
| Circuit/client public-input ordering mismatch | Circuit `[x, y, id, bid]`, client `[id, bid, x, y]` | RC5 |

### The fix pattern

Two approaches resolve almost every instance: **use the cryptographic commitments you already
have**, or **derive per-instance keys deterministically**.

```
FLAKEY:  Add a plaintext field + a new circuit to prove the plaintext matches the hidden value
PROPER:  Compare the existing commitments, both sides computing the blind deterministically

FLAKEY:  Reuse the raw wallet pubkey across contract instances
PROPER:  Derive per-instance via SecretKey::derive_instance(contract_id, instance_seed)

FLAKEY:  Return Ok(()) when an authorization check finds nothing
PROPER:  Return Err — deny by default, enumerate only the success conditions

FLAKEY:  Accept pallas::Base or [u8; 32] as a signature type
PROPER:  Use schnorr::Signature — let the type system require that signing occurred

FLAKEY:  Safety features None by default, requiring operator opt-in
PROPER:  Safety features enabled by default — Default::default() is the secure configuration

FLAKEY:  Accept block_height with no bounds
PROPER:  block_height <= current_block && current_block - block_height <= MAX_AGE

FLAKEY:  Capability descriptor drifts from the entrypoint dispatch table
PROPER:  Treat them as a matched pair; updating one alone is a half-implemented change
```

---

## Design Constraints: Hardening by Construction

These are not failure classes. They are the rules a new contract is built to from the start, derived
from the same review history.

### The L1 combinatorial bound

Box and Purse were upgraded from L2 (singleton, deterministic KV lookup) to L1 (anonymous encrypted
objects, Merkle inclusion proofs, full ZK). The upgrade introduces an **exponential** jump in the
state space: in L2, K sequential operations have exactly 1 valid state trajectory; in L1 with N
concurrent anonymous objects, K operations have N^K. This is the foundational reason L1 contracts
require different reasoning.

Every L1 contract proposal passes the triage **before implementation begins**:

| Tier | Public inputs | Witness values | Operations | Verdict |
|------|--------------|----------------|------------|---------|
| Safe | ≤9 | ≤13 | ≤3 | Pure L1, bounded by construction |
| Scrutiny | 10–15 | 14–20 | 4–6 | Explicit bounds proof required |
| Exceeds | >15 | >20 | >6 | Not valid as single-contract L1 — use L2 or sharding |

The ceiling constants are **derived, not observed**:

| Constant | Value | Derivation |
|----------|-------|------------|
| `P_CEILING` | 9 | 1/7 instance-column proportion × k=13 rows × 1% density per operation |
| `W_CEILING` | 13 | 4 minimum + 1 merkle_path + 2 contents + 6 balance/blinds |
| `O_CEILING` | 3 | consume + create + read-only query; 4+ operations exceeds the wallet scan |
| `PRACTICAL_MAX_OBJECTS` | 120,000 | 1000 scans/sec mobile × 120 s block interval |

Purse *is* the ceiling: Box Put 5/9, Box Take 4/7, Purse Deposit 9/13, Purse Withdraw 9/13, Purse
Balance 7/11. Any contract more complex than Purse exceeds safe single-contract L1 bounds.

Two structural invariants come with it. **Consume+create:** each non-terminal L1 operation nullifies
exactly one old state and creates exactly one new Merkle leaf, keeping the active object count bounded
at N — without it, stale objects accumulate unboundedly (RC10) and degrade anonymity for everyone.
**Per-contract anonymity:** o-cap composition gives each contract its own Merkle tree, so one
contract's state space does not merge into another's.

> **A claim that was withdrawn.** An earlier version of this section asserted a formal *additive*
> composition theorem — `|T(A ∘ B)| = |T(A)| + |T(B)|`. There is no such theorem, and
> `GeneralTheorem.ocap_preserves_safety` was `… : True := by trivial` with all four parameters unused;
> it has been deleted. What *is* additive is the size of one composed capability
> (`Combinations.card_biUnion_le_sum : |⋃_{c∈S} B c| ≤ Σ_{c∈S} |B c|`). What is a *product*, even with
> per-contract trees, is the number of distinct ways to combine operations across contracts
> (`Combinations.combinationCount`). Isolating state does not divide the count of ways to combine.

Enforced by architectural review, not by the compiler. Formal material:
`proofs/lean/src/DarkFi/Combinatorial/` (`StateSpace`, `Transitions`, `ComplexityJump`,
`CompositionBounds`, `Limits`, `CeilingDerivation`, `GeneralTheorem`, `Combinations`), core Lean 4
with zero Mathlib. The full L1 type system — trajectory identification, barb ordering under N^K,
additive composition, nominal L1 domain types, combinatorial error theory — is normative in
[contract-wasm-type-system.md Part C](../../arch/contract-wasm-type-system.md).

### The four-component L1 operation

Every L1 operation separates into four components with a single data flow: caller provides all values
→ circuit constrains → metadata echoes → host verifies → exec validates state → apply writes state.

1. **Circuit** (`.zk`): constrains cryptographic relationships. Every `constrain_instance` value is a
   caller-provided witness. The circuit computes a value, constrains it equal to the witness via
   `constrain_equal_base`, then publishes the witness. No circuit-local variable ever appears in
   `constrain_instance`.
2. **Params** (model): carries every value the circuit and the metadata both need — each
   `constrain_instance` position maps to a field.
3. **Metadata** (entrypoint `get_metadata`): a **pure echo**. Reads `params.field` directly — no
   domain constants, no `poseidon_hash`, no field arithmetic, no computation of any kind. The metadata
   function *is* the specification of the public-input vector order.
4. **Exec + Apply** (entrypoint `process_instruction` + `process_update`): exec validates against chain
   state (nullifier unspent, root in DB); apply writes state (`merkle_add`, `db_set`). Neither computes
   cryptographic values — the circuit already proved everything.

**The invariant:** `metadata[i] == proof_instance[i]` for all `i`. When metadata is a pure echo this
holds by construction. When metadata computes anything it introduces a second source of truth that can
drift from the circuit — which is exactly what happened, as the eight Box and twelve Purse mismatches
in RC5.

**Why params must carry everything.** A `constrain_instance` value whose hash inputs include
witness-only data — `owner_secret` in a nullifier, `balance_blind` in Pedersen coordinates, Merkle
leaf and derived IDs — *cannot* be recomputed by the metadata function, because the metadata function
sees only public data. Such a value must be a caller-provided field in params, with the circuit
constraining `constrain_equal_base(computed, params_value)` before publishing the witness. This is the
only reason a value appears in both params and the circuit, and it is the rule that made the earlier
"replicate the domain constant on the Rust side" workaround unnecessary for anything but values whose
inputs are *all* public.

### Version every state struct

`pub version: u8` is the first field of every state struct (~60 structs across 22 contracts),
defaulting to 0; the entrypoint reads it, deserialises the old format, and migrates on write. The cost
is one byte per record. The alternative — a hard fork to fix unreadable on-chain data — is
catastrophic. Version your state before you need to.

### Secure defaults are the only defaults

See RC9. `Default::default()` produces a secure instance; opt out for exceptions.

### Creation requires a deactivation path

See RC10. For every "create" or "register" function there is a corresponding "deactivate" function,
verifying caller authorization (owner, capability holder, or governance proof) before mutating state.
This belongs in the contract scaffolding template, not in a review finding.

### Bound all user-supplied iteration

See RC10. An assertion with a clear message is debuggable; a runtime-limit trap is not.

### Architectural concerns not yet actionable

Two patterns need network-level infrastructure that does not exist yet, and are recorded rather than
fixed at the contract level:

- **Oracle centralisation.** Contracts use single-oracle models (`darkbet_exchange` checks one
  `oracle_id`, `insurance_market` uses one `oracle_commitment`). Threshold oracles, M-of-N
  attestations and oracle rotation require the oracle network to support those primitives first. When
  it does, dependent contracts should accept M-of-N rather than a single key.
- **Rate limiting.** Only `drain_protection` implements it. Per-block or per-epoch limits on
  state-creating functions are defense-in-depth once fee markets and gas accounting exist; until
  then, transaction fees are the rate limit.

### Naming conventions

Circuit, manifest, and entrypoint naming rules — source filenames carry no version suffix, circuit
names inside `.zk` files carry the V2 suffix, manifest entries match the `.zk` declaration exactly,
Rust namespace constants match it character-for-character, enum variants carry the *contract API*
version (independent of the circuit version), and entrypoint module filenames carry no version suffix
— are normative in [Circuit Versioning](../../arch/circuit-versioning.md), which also carries the
V1→V2 migration rationale. Each rule there prevents a specific instance of RC5.

---

## Legacy identifiers

This document previously numbered its findings as Lessons 1–25, with a second, colliding `RC1–RC5`
scheme for the July-2026 circuit campaign. Lesson numbers are cited from other documents and from
Rust comments, so the mapping is kept. **The root-cause ID is the stable name; the lesson number is
an alias.**

| Legacy | Root cause | Legacy | Root cause |
|---|---|---|---|
| Lesson 1 — two-step auth | RC1 | Lesson 15 — parent call validation | RC1 |
| Lesson 2 — cross-contract routing | RC1 | Lesson 16 — unconstrained witnesses | RC2 |
| Lesson 3 — unproven outputs | RC2 | Lesson 17 — off-circuit conservation | RC7 |
| Lesson 4 — composition amount blindness | RC7 | Lesson 18 — independent witness separation | RC7 |
| Lesson 5 — pubkey as DB key | RC8 | Lesson 19 — isolated execution overlays | RC6 |
| Lesson 6 — signature key reuse | RC8 | Lesson 20 — supply audit capability | RC7 |
| Lesson 7 — user data encoding identity | RC8 | Lesson 21 — serialization-derived safety | RC12 |
| Lesson 8 — token ID identity fragments | RC8 | Lesson 22 (first) — generic-prover serialization | RC5 |
| Lesson 9 — full keypair in builders | RC8 | Lesson 22 (second) — metadata hash drift | RC5 |
| Lesson 10 — capability descriptors | RC5 | Lesson 23 — L1 combinatorial complexity | Design constraint |
| Lesson 11 — spend hook callback safety | RC1 | Lesson 24 — zkas variable reassignment | RC5 |
| Lesson 12 — compiler-synthesizer drift | RC5 | Lesson 25 — generic-prover write path | RC5, RC7, RC9, RC12 |
| Lesson 13 — hash impedance mismatch | RC5 | | |
| Lesson 14 — input reuse attacks | RC3 | | |

The July-2026 circuit campaign's `RC1`–`RC5` map as: `RC1` witness non-binding → **RC2**;
`RC2` vacuous proof acceptance → **RC2**; `RC3` missing domain separation → **RC3**;
`RC4` arithmetic domain confusion → **RC4**; `RC5` fix propagation failure → **RC5**.

Cross-cutting identifiers that appear in other documents map as: red-team `RC-A` verification-as-
format-check, `RC-F` validation-gated-on-configuration → **RC1**; `RC-B` post-commit validation →
**RC6**; `RC-C` gas-accounting bypass and `RC-I` incomplete feature gating → **RC5**; `RC-D` domain
separation absent → **RC3**; `RC-E` sensitive-type auto-traits → **RC8**; `RC-G` non-deterministic
serialization → **RC11**; `RC-H` design decisions with known gaps → **RC10**. The consensus HAZID's
`RC1` swallowed errors → **RC12** and **RC1**; `RC2` missing implementations → **RC1**;
`RC3` non-determinism → **RC11**; `RC4` destructive-before-fallible → **RC6**; `RC5` two sources of
truth → **RC5**; `RC6` spec/code formula mismatch → **RC5**. The Lean HAZOP catalogue's
`pattern1_free_instance`, `pattern2_zero_cond`, `pattern6_bool_check_u64`, `pattern7_missing_range` →
**RC2**; `pattern3_field_div` → **RC4**; `pattern4_capability_bypass` → **RC1**;
`pattern5_nullifier_collision` → **RC3**.

Ten identifiers — `ESC-001`, `INS-001`, `DAO-001`, `DAO-002`, `MV-001`, `DRAIN-001`, `TENDER-002`,
`ATTEST-001`, `BET-001`, `POOL-001` — appear nowhere in this repository except in the five sentences
that introduced them. They were class labels without per-finding detail, not findings. Their intended
classes, read from those sentences, were **RC1** (`ESC-001`, `INS-001`, `DAO-001`, `DAO-002`,
`MV-001`), **RC9** (`DRAIN-001`), **RC5** (`TENDER-002`) and **RC10** (`ATTEST-001`, `BET-001`,
`POOL-001`). Nothing depends on them and they are not carried forward.

## References

- [Contract Safety Checklist](checklist.md) — the operative checklist derived from these root causes
- [Verification Obligation Register](../../arch/verification-hazop.md) — every property, where it is
  enforced, and what checks it; the home of open obligations
- [Contract WASM Standards & Best Practices](../../arch/contract-wasm-standards-best-practices.md) —
  canonical encoding and entrypoint patterns
- [Circuit Versioning](../../arch/circuit-versioning.md) — circuit/manifest/entrypoint naming rules
- [Contract WASM Type System](../../arch/contract-wasm-type-system.md) — the L1 type system, Part C
- [NativeToken](native_token.md) — consensus token with zero business logic
- [Standards](standards.md) — ZK circuit, token, and testing standards
- [Composability](../../contract/composability.md) — cross-contract child-call patterns
- [PromissoryNote](../../contract/promissory_note.md) — the DeFi bearer instrument
