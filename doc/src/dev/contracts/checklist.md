# Contract Safety Checklist

The operative pre-commit gates, derived from the twelve root causes in
[Smart Contract Inherent Safety](safety.md) and the structural rules in
[Contract WASM Standards & Best Practices](../../arch/contract-wasm-standards-best-practices.md).

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
  [Consensus & Coinbase](../../arch/consensus-coinbase.md).
- [ ] **Handle allocation vs. query.** `db_lookup` allocates a handle without querying sled and
  always returns `Ok`; idempotency guards use `db_contains_key`. See `RC1`, and
  [Contract WASM Type System](../../arch/contract-wasm-type-system.md).
- [ ] **Per-block key derivation.** Miner and wallet compute `sk_H = derive_instance(sk, cid, height)`
  independently, with no shared state, producing identical output. See `RC8`, and
  [Key & Account Management](../../arch/key-management.md).
- [ ] **Cumulative supply verification.** `Σ outputs + Σ burns + Σ fees == Σ inputs` is enforced at
  every block acceptance path with no bypass. See `RC7`, and `OBL-C1`/`OBL-C3` in the register.
- [ ] **No `unwrap_or(zero)` on typed identifiers.** `from_repr().unwrap_or(zero)` silently
  substitutes zero for invalid data. Every typed identifier goes through a fallible
  `from_bytes`/`from_repr` returning `Result`. See `RC12`, and standards §1.3 and §8.1.
- [ ] **Ephemeral signatures.** Every signature uses a fresh per-transaction secret; the wallet
  secret is never a signing key. See `RC8`.

## DeFi / Application Contracts

For contracts that handle user funds (PromissoryNote, Stablecoin, Bridge, DEX, …):

- [ ] **Two-step auth audit.** Every authorization spanning multiple function calls is replaced by a
  single-step ZK proof — the proof *is* the authorization. No "step 1 creates an artifact, step 2
  checks it" pattern. See `RC1`.
- [ ] **Child call verification.** When a child contract moves value, the parent verifies the amount
  via `validate_child_value_commit` with deterministic blind derivation — never by trusting the
  builder. See `RC7`.
- [ ] **Input nullifier binding.** Every input nullifier is bound to the operation it authorizes: a
  nullifier valid for "transfer 5 DRKW" cannot authorize "transfer 500". See `RC3`.
- [ ] **Parent call validation.** Every parent call check verifies **both** `contract_id` **and**
  `func_code`. Checking only `contract_id` lets any function in that contract authorize; checking
  only `data[0]` is blind to which contract runs. See `RC1`.
- [ ] **Value conservation in ZK.** Every value transformation performed off-circuit — fee
  subtraction, interest accrual, exchange-rate conversion — is constrained in-circuit. The Rust
  client is a convenience, not a security boundary. See `RC7`.
- [ ] **Structural conservation is not enough.** A 1-in-1-out commitment structure does not imply
  value conservation; the circuit explicitly constrains the intended transformation. See `RC7`.
- [ ] **Deactivation path.** Every `create`/`register` function has a corresponding `deactivate`
  function that verifies caller authorization before mutating state. See `RC10`.
- [ ] **Bounded iteration.** Every user-supplied `Vec` parameter has an explicit `MAX_*` assertion
  before the loop, with a message that names the limit. See `RC10`.
- [ ] **Temporal bounds.** Every `block_height` parameter satisfies `block_height <= current_block`
  and `current_block - block_height <= MAX_AGE`. See `RC10`.
- [ ] **Secure defaults.** The `Default` impl of every config struct and builder produces a private,
  protected instance. Safety features are opt-out, never opt-in. See `RC9`.
- [ ] **Fail closed on unconfigured state.** A guard conditional on configuration
  (`if value != ContractId::ZERO { validate(..) }`) is replaced by an explicit rejection when the
  configuration is absent. See `RC1` and `RC9`.

## Exec / Apply Phasing

For every contract, without exception. The host ACL enforces both rules mechanically, and a
violation is not a style problem — it is a call that **cannot succeed**. `vm_runtime.rs:954` runs
`apply` as `ContractSection::Update`, and no read function admits `Update`. See `RC6` (irreversible
work before the check that guards it) and the type system §A.4.7 / §B.2.2.

- [ ] **Apply writes blindly.** No `db_get`, `db_contains_key`, `get_object_size` or
  `get_object_bytes` is reachable from an `apply` function — the read triad is denied in `Update`
  and returns `CallerAccessDenied` at runtime. Any value apply needs is computed in `exec` and
  carried through the update struct. See §B.2.2 and `OBL-C72` in the register.
- [ ] **Exec does not write.** No `db_set`, `db_del`, `merkle_add` or
  `sparse_merkle_insert_batch` is reachable from an `exec` function; all mutation is in `apply`.
  A write in exec fails the same way. See §A.4.7 and `OBL-C73` in the register.
- [ ] **The bridge is the only channel.** Everything `apply` acts on arrived in the update struct,
  which `exec` built. `db_lookup` allocates a handle and is legal in both phases — it is the only
  one that is.
- [ ] **Manifest declares the capability block.** `[[actions]]` with `required_barbs`,
  `[[capabilities]]` with `primitives` and `note_schema` — the declaration `wallet_construct`
  composes against. Without it the contract is not constructible by the generic wallet. See
  `ocap.md` §7, `type-system.md` §13, and `OBL-C74` in the register.

## ZK Circuit Development

For any new or modified circuit:

- [ ] **Witness derivation constrained.** Every witness used for authorization (`mint_public`,
  `auth_parent`, `spend_hook`) has its derivation constrained in-circuit. An aspirational comment in
  the Rust code is not a constraint. See `RC2`.
- [ ] **No free variables in authorization checks.** Every `constrain_instance(X)` has an in-circuit
  derivation `X = f(witnesses)`; no circuit-local variable is published directly. See `RC2`.
- [ ] **Domain separation.** Every `poseidon_hash` prepends the appropriate `DOMAIN_*` constant as
  its first argument: `1 = NULLIFIER`, `2 = TOKEN_COMMIT`, `3 = TX_BINDING`, `4 = COIN_COMMIT`,
  `5 = MERKLE_LEAF`, `6 = USER_DATA_ENC`, `7 = SIGNATURE_SECRET`. See `RC3`.
- [ ] **Range checks.** Every u64-valued witness has `range_check(64, value)` before entering
  arithmetic, and `bool_check` is never applied to an amount. See `RC2`.
- [ ] **No field division on integers.** `base_div` does not appear in any circuit with u64-valued
  witnesses; integer division uses quotient–remainder constraints, and ratios cross-multiply. See
  `RC4`.
- [ ] **Conditional gadgets guarded.** Every `zero_cond(value, leaf)` feeding `merkle_root` is
  preceded by `less_than_strict(ZERO, value)` — otherwise `value = 0` verifies against the tree's
  zero leaf. See `RC2`.
- [ ] **Metadata is a pure echo.** `get_metadata` reads `params.field` directly — no hashing, no
  arithmetic, no computation. Values whose inputs include witness-only data are caller-provided
  through params. The invariant is `metadata[i] == proof_instance[i]` for all `i`. See `RC5`.
- [ ] **Public-input ordering.** The circuit's `constrain_instance` order, the client's `to_vec()`
  order, and the entrypoint's instance vector are identical, position for position. See `RC5`.
- [ ] **Merkle hash parity.** Every hash used by a circuit opcode has an off-circuit implementation
  producing identical output — `merkle_root` is Sinsemilla via `OrchardHashDomains::MerkleCrh` while
  `MerkleNode::combine` is Poseidon. See `RC5`.
- [ ] **Provenance declared.** A `.zk` file copied from another circuit declares its provenance and
  has every fix from the origin applied. See `RC5`.
- [ ] **`zkas validate` passes.** Every `.zk.bin` passes `zkas validate` before being embedded;
  `make all` does not perform this check. Recompile a known-good circuit after any zkas version
  change and confirm `ProvingKey::build` succeeds. See `RC5`.
- [ ] **Opaque field audit.** Fields committed into commitment hashes or passed as ZK public inputs
  carry no identity-derived data; authorization goes in nullifiers, not auxiliary data. See `RC8`.
- [ ] **Token ID unlinkability.** Token IDs are derived with randomized inputs, so a token's
  existence reveals nothing about who created it. See `RC8`.

## Privacy and Key Material

- [ ] **No raw public key as a database key.** Identity material is hashed before use as any key. See
  `RC8`.
- [ ] **No wallet keypair in a client builder.** Builders accept the individual secrets an operation
  needs, never a `Keypair`. See `RC8`.
- [ ] **Per-instance keys.** A wallet's identity is derived per contract instance via
  `SecretKey::derive_instance`; one raw pubkey never appears across instances. See `RC8`.
- [ ] **No secret in a formatting trait.** No type in `src/sdk/src/crypto/` that wraps a field
  element derives `Debug` or `Display`. See `RC8`.

## Consensus Paths

For any change under `src/linear/` or `bin/dwowd/` that affects a consensus decision:

- [ ] **Deterministic serialization.** `dwow_serial`, never `serde_json`, in a consensus path. See
  `RC11`.
- [ ] **Deterministic iteration.** No `HashMap` iteration feeds a consensus decision without an
  explicit tiebreaker. See `RC11`.
- [ ] **Checked arithmetic.** `checked_*` where overflow is a bug; `saturating_*` never masks an
  invariant violation. Non-relaxed atomics on consensus data. See `RC11`.
- [ ] **No wall-clock dependence** in a consensus decision path. See `RC11`.

## Pre-Deployment Gates

Run before deploying any contract, even in devnet:

- [ ] `cargo test -p dwowd test_all_contracts_deploy` — Level 1 deployment
- [ ] `./bin/dwowd/src/tests/heavyweight.sh --all` — Level 2 ZK proofs
- [ ] Python model tests pass for the relevant contract (if a model exists)
- [ ] Capability descriptor updated: every new entrypoint function has a matching descriptor action,
  with the correct `function_id`. See `RC5`.
- [ ] `cargo check --tests` clean — zero warnings in the contract crate
- [ ] `cargo clippy` clean under `deny(clippy::unwrap_used, clippy::expect_used)`; every dispensation
  carries an `#[expect(…, reason = …)]`. See `RC12`.

> **USE AT YOUR OWN RISK.** These checklists are derived from internal review.
> No third-party audit has been performed.
