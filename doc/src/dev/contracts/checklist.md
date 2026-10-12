# Contract Safety Checklist

The pre-deployment review checklist, derived from the twelve root causes in
[Smart Contract Inherent Safety](safety.md) and the structural rules in
[Contract WASM Standards & Best Practices](../../arch/contract-wasm-standards-best-practices.md).

> **This is a human review checklist, not a set of enforced pre-commit gates.** Each item carries an
> enforcement tag naming what actually checks it — **MECHANICAL: `script`** where a script reads the
> property, **MANUAL** where nothing does and a reviewer must read the code. There is no CI in this
> repository; the only check that runs on every commit is `hooks/pre-commit` (a `.zk` circuit check),
> and the scripted items below run by hand through `scripts/run-all-tests.sh`. A `MANUAL` item is a
> rule a person must apply, and nothing fails if they do not.

> **Prerequisite**: read [safety.md](safety.md) first. This is the instrument; that is the
> explanation of why each line exists. Each item names the root cause it defends against, so a
> failure here points at the reasoning, not just the rule.
>
> Open obligations — properties that are known *not* to hold yet — are in the
> [Verification Obligation Register](../../arch/verification-hazop.md), not here. A checklist item
> that cannot pass today is a bug in the contract, not a known limitation.

## Consensus-Critical Contracts

For any contract whose failure could halt the chain (NativeToken, Deployooor):

- [ ] **Nullifier zero-rejection.** Every nullifier path rejects `Nullifier::zero()` — a zero
  nullifier matches every commitment, so accepting it authorizes everything. See `RC1`, and
  [Consensus & Coinbase](../../arch/consensus-coinbase.md). **MANUAL.**
- [ ] **Handle allocation vs. query.** `db_lookup` allocates a handle without querying sled and
  always returns `Ok`; idempotency guards use `db_contains_key`. See `RC1`, and
  [Contract WASM Type System](../../arch/contract-wasm-type-system.md). **MANUAL.**
- [ ] **Per-block key derivation.** Miner and wallet compute `sk_H = derive_instance(sk, cid, height)`
  independently, with no shared state, producing identical output. See `RC8`, and
  [Key & Account Management](../../arch/key-management.md). **MANUAL.**
- [ ] **Cumulative supply verification.** `Σ outputs + Σ burns + Σ fees == Σ inputs` is enforced at
  every block acceptance path with no bypass. See `RC7`, and `OBL-C1`/`OBL-C3` in the register.
  **MANUAL.**
- [ ] **No `unwrap_or(zero)` on typed identifiers.** `from_repr().unwrap_or(zero)` silently
  substitutes zero for invalid data. Every typed identifier goes through a fallible
  `from_bytes`/`from_repr` returning `Result`. See `RC12`, and standards §1.3 and §8.1. **MANUAL.**
- [ ] **Ephemeral signatures.** Every signature uses a fresh per-transaction secret; the wallet
  secret is never a signing key. See `RC8`. **MANUAL.**

## DeFi / Application Contracts

For contracts that handle user funds (PromissoryNote, Stablecoin, Bridge, DEX, …):

- [ ] **Two-step auth audit.** Every authorization spanning multiple function calls is replaced by a
  single-step ZK proof — the proof *is* the authorization. No "step 1 creates an artifact, step 2
  checks it" pattern. See `RC1`. **MANUAL.**
- [ ] **Child call verification.** When a child contract moves value, the parent verifies the amount
  via `validate_child_value_commit` with deterministic blind derivation — never by trusting the
  builder. See `RC7`. **MANUAL.**
- [ ] **Input nullifier binding.** Every input nullifier is bound to the operation it authorizes: a
  nullifier valid for "transfer 5 DRKW" cannot authorize "transfer 500". See `RC3`. **MANUAL.**
- [ ] **Parent call validation.** Every parent call check verifies **both** `contract_id` **and**
  `func_code`. Checking only `contract_id` lets any function in that contract authorize; checking
  only `data[0]` is blind to which contract runs. See `RC1`. **MANUAL.**
- [ ] **Value conservation in ZK.** Every value transformation performed off-circuit — fee
  subtraction, interest accrual, exchange-rate conversion — is constrained in-circuit. The Rust
  client is a convenience, not a security boundary. See `RC7`. **MANUAL.**
- [ ] **Structural conservation is not enough.** A 1-in-1-out commitment structure does not imply
  value conservation; the circuit explicitly constrains the intended transformation. See `RC7`.
  **MANUAL.**
- [ ] **Deactivation path.** Every `create`/`register` function has a corresponding `deactivate`
  function that verifies caller authorization before mutating state. See `RC10`. **MANUAL.**
- [ ] **Bounded iteration.** Every user-supplied `Vec` parameter has an explicit `MAX_*` assertion
  before the loop, with a message that names the limit. See `RC10`. **MANUAL.**
- [ ] **Temporal bounds.** Every `block_height` parameter satisfies `block_height <= current_block`
  and `current_block - block_height <= MAX_AGE`. See `RC10`. **MANUAL.**
- [ ] **Secure defaults.** The `Default` impl of every config struct and builder produces a private,
  protected instance. Safety features are opt-out, never opt-in. See `RC9`. **MANUAL.**
- [ ] **Fail closed on unconfigured state.** A guard conditional on configuration
  (`if value != ContractId::ZERO { validate(..) }`) is replaced by an explicit rejection when the
  configuration is absent. See `RC1` and `RC9`. **MANUAL.**

## Exec / Apply Phasing

For every contract, without exception. The host ACL enforces both rules mechanically, and a
violation is not a style problem — it is a call that **cannot succeed**. `vm_runtime.rs:954` runs
`apply` as `ContractSection::Update`, and no read function admits `Update`. See `RC6` (irreversible
work before the check that guards it) and the type system §A.4.7 / §B.2.2.

- [ ] **Apply writes blindly.** No `db_get`, `db_contains_key`, `get_object_size` or
  `get_object_bytes` is reachable from an `apply` function — the read triad is denied in `Update`
  and returns `CallerAccessDenied` at runtime. Any value apply needs is computed in `exec` and
  carried through the update struct. See §B.2.2 and `OBL-C72` in the register.
  **MECHANICAL: `scripts/check-phase-host-functions.sh`** (and the host ACL at runtime).
- [ ] **Exec does not write.** No `db_set`, `db_del`, `merkle_add` or
  `sparse_merkle_insert_batch` is reachable from an `exec` function; all mutation is in `apply`.
  A write in exec fails the same way. See §A.4.7 and `OBL-C73` in the register.
  **MECHANICAL: `scripts/check-phase-host-functions.sh`** (and the host ACL at runtime).
- [ ] **The bridge is the only channel.** Everything `apply` acts on arrived in the update struct,
  which `exec` built. `db_lookup` allocates a handle and is legal in both phases — it is the only
  one that is. **MECHANICAL: the host ACL** — `ContractSection::Update` denies the read triad; no
  script enforces this one on its own.
- [ ] **Manifest declares the capability block.** `[[actions]]` with `required_barbs`,
  `[[capabilities]]` with `primitives` and `note_schema` — the declaration `wallet_construct`
  composes against. Without it the contract is not constructible by the generic wallet. See
  `ocap.md` §7, `type-system.md` §13, and `OBL-C74` in the register. **MANUAL** — no gate requires
  the block; `scripts/check-wallet-kernel.sh` and `contrib/capability_type_diff.sh` read adjacent
  facts, not the declaration's presence.

## ZK Circuit Development

For any new or modified circuit:

- [ ] **Witness derivation constrained.** Every witness used for authorization (`mint_public`,
  `auth_parent`, `spend_hook`) has its derivation constrained in-circuit. An aspirational comment in
  the Rust code is not a constraint. See `RC2`.
  **MECHANICAL: `scripts/check-circuit-instance-derivation.sh`.**
- [ ] **No free variables in authorization checks.** Every `constrain_instance(X)` has an in-circuit
  derivation `X = f(witnesses)`; no circuit-local variable is published directly. See `RC2`.
  **MECHANICAL: `scripts/check-circuit-instance-derivation.sh`.**
- [ ] **Domain separation.** Every `poseidon_hash` prepends the appropriate `DOMAIN_*` constant as
  its first argument: `1 = NULLIFIER`, `2 = TOKEN_COMMIT`, `3 = TX_BINDING`, `4 = COIN_COMMIT`,
  `5 = MERKLE_LEAF`, `6 = USER_DATA_ENC`, `7 = SIGNATURE_SECRET`. See `RC3`.
  **MECHANICAL: `scripts/check-circuit-domain-separation.sh`** (circuit side);
  `scripts/check-host-domain-separation.sh` covers the host side report-only.
- [ ] **Range checks.** Every u64-valued witness has `range_check(64, value)` before entering
  arithmetic, and `bool_check` is never applied to an amount. See `RC2`. **MANUAL.**
- [ ] **No field division on integers.** `base_div` does not appear in any circuit with u64-valued
  witnesses; integer division uses quotient–remainder constraints, and ratios cross-multiply. See
  `RC4`. **MANUAL.**
- [ ] **Conditional gadgets guarded.** Every `zero_cond(value, leaf)` feeding `merkle_root` is
  preceded by `less_than_strict(ZERO, value)` — otherwise `value = 0` verifies against the tree's
  zero leaf. See `RC2`. **MANUAL.**
- [ ] **Metadata is a pure echo.** `get_metadata` reads `params.field` directly — no hashing, no
  arithmetic, no computation. Values whose inputs include witness-only data are caller-provided
  through params. The invariant is `metadata[i] == proof_instance[i]` for all `i`. See `RC5`.
  **MECHANICAL: `scripts/check-circuit-metadata-alignment.sh`** (and `scripts/check-metadata-arms.sh`).
- [ ] **Public-input ordering.** The circuit's `constrain_instance` order, the client's `to_vec()`
  order, and the entrypoint's instance vector are identical, position for position. See `RC5`.
  **The gate does not enforce this for you.** `scripts/check-circuit-metadata-alignment.sh` fails on
  a *count* mismatch and only warns on order, because mapping a circuit variable to a Rust
  expression is a heuristic — so a vector of the right length with the wrong entries passes the
  hard check. Read the circuit's `constrain_instance` list and print it beside your
  `to_vec()` before changing either; `pool_stake`'s `create_pool` was transposed exactly this way,
  and no proof could verify. See `OBL-C78`. **PARTIAL:
  `scripts/check-circuit-metadata-alignment.sh`** — the count is a hard fail, the order is a warning
  only.
- [ ] **Merkle hash parity.** Every hash used by a circuit opcode has an off-circuit implementation
  producing identical output — `merkle_root` is Sinsemilla via `OrchardHashDomains::MerkleCrh` while
  `MerkleNode::combine` is Poseidon. See `RC5`. **MANUAL.**
- [ ] **Provenance declared.** A `.zk` file copied from another circuit declares its provenance and
  has every fix from the origin applied. See `RC5`. **MANUAL.**
- [ ] **`zkas validate` passes.** Every `.zk.bin` passes `zkas validate` before being embedded;
  `make all` does not perform this check. Recompile a known-good circuit after any zkas version
  change and confirm `ProvingKey::build` succeeds. See `RC5`.
  **MECHANICAL: `scripts/validate_zk_bins.sh`** (run by `scripts/run-all-tests.sh`).
- [ ] **Opaque field audit.** Fields committed into commitment hashes or passed as ZK public inputs
  carry no identity-derived data; authorization goes in nullifiers, not auxiliary data. See `RC8`.
  **MANUAL.**
- [ ] **Token ID unlinkability.** Token IDs are derived with randomized inputs, so a token's
  existence reveals nothing about who created it. See `RC8`. **MANUAL.**

## Privacy and Key Material

- [ ] **No raw public key as a database key.** Identity material is hashed before use as any key. See
  `RC8`. **MANUAL.**
- [ ] **No wallet keypair in a client builder.** Builders accept the individual secrets an operation
  needs, never a `Keypair`. See `RC8`. **MANUAL.**
- [ ] **Per-instance keys.** A wallet's identity is derived per contract instance via
  `SecretKey::derive_instance`; one raw pubkey never appears across instances. See `RC8`. **MANUAL.**
- [ ] **No secret in a formatting trait.** No type in `src/sdk/src/crypto/` that wraps a field
  element derives `Debug` or `Display`. See `RC8`. **MANUAL.**

## Consensus Paths

For any change under `src/linear/` or `bin/dwowd/` that affects a consensus decision:

- [ ] **Deterministic serialization.** `dwow_serial`, never `serde_json`, in a consensus path. See
  `RC11`. **MANUAL.**
- [ ] **Deterministic iteration.** No `HashMap` iteration feeds a consensus decision without an
  explicit tiebreaker. See `RC11`. **MANUAL.**
- [ ] **Checked arithmetic.** `checked_*` where overflow is a bug; `saturating_*` never masks an
  invariant violation. Non-relaxed atomics on consensus data. See `RC11`. **MANUAL.**
- [ ] **No wall-clock dependence** in a consensus decision path. See `RC11`. **MANUAL.**

## Pre-Deployment Gates

Run before deploying any contract, even in devnet:

- [ ] `cargo test -p dwowd test_all_contracts_deploy` — Level 1 deployment.
  **MECHANICAL: `make test`** (`scripts/run-all-tests.sh` runs `make test` =
  `cargo test --workspace`).
- [ ] `./bin/dwowd/src/tests/heavyweight.sh --all` — Level 2 ZK proofs. **MANUAL** — run by hand;
  it is not part of the fast umbrella.
- [ ] Python model tests pass for the relevant contract (if a model exists).
  **MECHANICAL: `scripts/run-all-tests.sh`** (it runs every `contrib/model/*.py`).
- [ ] Capability descriptor updated: every new entrypoint function has a matching descriptor action,
  with the correct `function_id`. See `RC5`. **MANUAL.**
- [ ] `cargo check --tests` clean — zero warnings in the contract crate. **MANUAL.**
- [ ] `cargo clippy` clean under `deny(clippy::unwrap_used, clippy::expect_used)`; every dispensation
  carries an `#[expect(…, reason = …)]`. See `RC12`.
  **MECHANICAL: `contrib/clippy_unwrap_audit.sh`** (run by `scripts/run-all-tests.sh`).

> **USE AT YOUR OWN RISK.** These checklists are derived from internal review.
> No third-party audit has been performed.
