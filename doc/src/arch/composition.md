# Capability Composition

This document describes how the primitive capability types of
[the type system](type-system.md) §8.1 **compose** into the emergent capability
types of §6, the algebra that governs that composition, and where the stack
follows that algebra versus where it diverges. It is analysis, not new
normative law: every rule here is sourced from `type-system.md`, the Lean4
proofs in `proofs/lean/src/DarkFi/Capability/`, and the Rust mirror in
`src/sdk/src/capability.rs`.

The one-line thesis: **a capability is not a primitive — it is the union of the
barbs of the primitives it composes, and it is a valid type for an action iff
that union covers the barbs the action requires.**

## 1. The composition algebra

### 1.1 Primitives carry fixed barb sets

Each §8.1 primitive is a nominal type carrying a fixed set of barbs
(observable actions). The Rust source of truth is `Primitive::barbs()`
(`src/sdk/src/capability.rs:284-297`), mirroring Lean `Types.lean`:

| Primitive | barbs |
|-----------|-------|
| `SecretKey` | `↓spend`, `↓derive` |
| `PublicKey` | `↓verify`, `↓encrypt` |
| `Nullifier` | `↓nullify` |
| `Commitment` | `↓commit` |
| `ContractId` | `↓dispatch` |
| `FuncId` | `↓gate` |
| `AssetId` | `↓denominate` |
| `MerkleNode` | `↓prove-inclusion` |
| `OwnedSecretKey` | `↓spend` |
| `MiningRecipient` | `↓spend`, `↓mine` |

### 1.2 `compose` is barb union

The composition operator is the **union of barb sets over a primitive list**
(`Composition.lean:26-29`):

```
compose []        = ∅
compose (p :: ps) = p.barbs ∪ compose ps
```

Algebraically this is the free **commutative, idempotent monoid** on
`Finset Barb`: identity `∅`, operation `∪`. Order and duplication of primitives
are irrelevant to the resulting barb set. The Rust mirror computes exactly this
deduplicated union in `wallet_construct` (`capability.rs:348-356`).

**Consequence that governs everything below:** `compose` can only ever yield a
barb that *some primitive in the list already carries*. It cannot manufacture a
new barb. Composition adds structure, never authority.

### 1.3 The existence rule (the core theorem)

A capability type over a resource `r` and action `s` *is* the proof that its
primitives cover the resource's required barbs (`Composition.lean:80-84`):

```
structure CapabilityType (r : Resource) (s : Action) where
  primitives  : List PrimitiveType
  coversBarbs : r.requiredBarbs ⊆ compose primitives
```

and the existence-iff-coverage statement is `authorizationInversion_TypeLevel`
(`Inversion.lean:95-105`):

```
Nonempty (CapabilityType r s)  ↔  ∃ primitives, r.requiredBarbs ⊆ compose primitives
```

> **A capability type over `(r, s)` EXISTS iff the union of its primitives'
> barbs covers `r.requiredBarbs`.**

Spec side: §6 states `CapabilityType(r, s) ≡ L_{r,s}` (type-system.md:190-194);
§6.1 gives the `compose(...)` construction (type-system.md:206-219). The Rust
executable form is `wallet_construct(resource, action, primitives, required_barbs)
-> Option<TypedCapability>` (`capability.rs:342-368`): it unions the barbs and
returns `Some` iff `required_barbs ⊆ barbs`, else `None`. `TypedCapability::covers`
(`capability.rs:317-319`) is the runtime `coversBarbs` check.

### 1.4 Two algebras, not one

This is the distinction that most often trips people up. There are **two**
independent operators, and only the first builds capability *types*:

**(a) Type-construction — `compose` (barb union).** Manufactures a capability
TYPE from primitives. Monoid `(Finset Barb, ∪, ∅)`. This is the only operator in
§6.

**(b) Authorization-requirement — `CapabilityExpression`.** A boolean predicate
over named capability *instances* that GATES an action; it does not build types.
Encoded twice, in agreement:

| Expr | Manifest TOML (`manifest.rs:86-94`) | SDK enum (`capability.rs:160-177`) | Meaning |
|------|-------------------------------------|------------------------------------|---------|
| `none` | `{type="none"}` (the default, `manifest.rs:74-82`) | — (absence) | no capability required |
| `any` | `{type="any", capabilities=[…]}` | `Any(Vec<CapabilityId>)` | OR |
| `all` | `{type="all", capabilities=[…]}` | `All(Vec<CapabilityId>)` | AND |
| `not` | `{type="not", capability="x"}` | `Not(Box<…>)` | negation |
| `threshold` | `{type="threshold", count, total, capabilities=[…]}` | `Threshold{…}` | k-of-n |

The manifest form is stringly-typed (`expr_type: String`); the SDK form is a
recursive enum (`Not` boxes a sub-expression). The **bridge** from an
authorization requirement back to a type is
`ContractManifest::resolve_capability(function_code)` (`manifest.rs:179-203`):
`function_by_code → action_for_function → produces.first → capability_by_name`,
yielding a `ResolvedCapability { discriminant, name, function, primitives,
consumable }` (`manifest.rs:143-155`).

### 1.5 Manifest Type-Checking Specification

A contract manifest SHALL declare capabilities and actions. The runtime
type-checking algorithm is:

1. For each action `a` with non-empty `required_barbs`:
   a. Resolve the produced capability `c` (from `a.produces[0].name`).
   b. Look up `c.primitives` from the capability declaration.
   c. Compute `composed = union_{p in c.primitives} p.barbs`.
   d. Assert: `a.required_barbs ⊆ composed`.
   e. If assertion fails: the manifest is ill-typed — the capability's
      primitives do not cover the action's required barbs.

2. Invariants that SHALL hold:
   a. **No barb manufacture**: Every barb in `composed` MUST be carried by
      some primitive in `c.primitives`. `compose` adds structure, never authority.
   b. **Composition order irrelevance**: `composed` is a SET (deduplicated,
      sorted). Declaration order of primitives is irrelevant.
   c. **Fail closed**: An unknown primitive or barb name SHALL cause the
      capability to be None (untyped), never a partial type with dropped barbs.
   d. **Coverage, not least privilege**: The check proves `required ⊆ composed`
      (you have AT LEAST the needed authority). It does NOT prove
      `composed ⊆ required` (you have AT MOST the needed authority).
      See ocap.md §5.1 "Defined Privilege Containment" for the trade-off.

3. Cross-reference to Lean: The Lean4 `CapabilityType r s` structure
   (`proofs/lean/src/DarkFi/Capability/Composition.lean:80-84`) defines
   `coversBarbs : r.requiredBarbs ⊆ compose primitives` — the exact property
   this algorithm checks at runtime. The Rust executable form is
   `wallet_construct()` → `resolve_capability_type()` → `covers()`.
   The exhaustive test `test_all_shipped_manifests_pass_coverage_gate`
   (`src/sdk/src/manifest.rs`) mechanically enforces this for every shipped
   manifest.

## 2. The invariants that make composition "follow its own logic"

1. **Barb preservation under composition** — §10.3, `barbPreservation`
   (`Composition.lean:39-54`): `p ∈ primitives → p.barbs ⊆ compose primitives`.
   Composing never *erases* a barb. §6.2 (type-system.md:227-242) states the
   prerequisite: the construction is sound iff every primitive preserves its
   barbs across every module boundary. This is precisely why the primitive-type
   migration matters — a `AssetId` silently degraded to `pallas::Base` at a
   module boundary drops `↓denominate` from every capability that composes it,
   and the union collapses.

2. **No accidental unification** — §10.1 `primitiveTypesAreParetoEfficient`
   (`Pareto.lean`): all primitives have pairwise-distinct barb sets, so each
   contributes something under `∪`. Cross-checked in Rust by
   `test_all_primitives_have_distinct_barb_sets` (`capability.rs:524-540`).
   §10.2 proves the 10 non-unifiable pairs of §8.4.

3. **`wallet_construct` is sound, complete, deterministic** — §10.5
   (`Wallet.lean`): returns `some` iff coverage holds (`_sound`, `_complete`),
   returns exactly the input primitives (`_preservesPrimitives`), and rejects
   empty primitives against any non-empty requirement (`_rejects_emptyPrimitives`;
   Rust `test_empty_primitives_rejected`, `capability.rs:489-497`).

4. **Predicate-bypass prevention** — §10.4 `capabilityPredicateBypass_prevention`
   (`Inversion.lean:132-136`): if `↓prove ∈ requiredBarbs` then `↓prove ∈
   compose primitives`. Closes HAZOP Pattern 4 ("capability predicate result is a
   free witness"). See §5 for the tension this creates.

5. **Type Distinction underneath it all** — §2 / §2.2: distinct behavioral
   positions cannot be unified, and no type may round-trip through `[u8; 32]`
   across a module boundary. This is what keeps each primitive's barb
   contribution meaningful under `∪`.

## 3. The stack map — one schema, three erasure boundaries

Every layer instantiates the same schema:

> **commitment = `poseidon_hash([ fixed tuple of primitives ]) → newtype(pallas::Base)`**

Types are **explicit** at the Rust struct boundary and collapse to **raw
`pallas::Base`** at three boundaries: (a) inside a Poseidon hash, (b) as a ZK
circuit public input (`constrain_instance`), and (c) as serialized calldata.
Two mechanical facts recur at every hash site: `PublicKey` (a `pallas::Point`)
is flattened to `(x, y)` via `.xy()` because Poseidon operates over
`pallas::Base`; and `u64` values widen via `pallas::Base::from(v)`, with
`BaseBlind` conventionally the final (hiding) tuple element.

| Layer | Composite | Primitives composed | Barbs | Boundary |
|-------|-----------|---------------------|-------|----------|
| Commitment | `Commitment` ← `CommitmentAttributes` (`native_token/src/model/mod.rs:70-127`) | `PublicKey`(→x,y) + value + `AssetId` + `FuncId` + user_data + `BaseBlind` | `↓commit` | typed struct → 7-elem raw Poseidon tuple |
| Tx I/O | `Input` / `Output` (`native_token/src/model/mod.rs:140-204`) | `Nullifier`, `MerkleNode`, `FuncId`, `Commitment`, `PublicKey`, `AeadEncryptedNote` + raw `token_commit`/`user_data_enc` | `↓nullify`, `↓prove-inclusion`, `↓gate`, `↓commit` | mixed: capabilities typed, ZK inputs raw |
| Params | `*ParamsV1` / `*UpdateV1` | `Vec<Input>`/`Vec<Output>`; `*UpdateV1` surfaces `Vec<Nullifier>` + `Vec<Commitment>` | `↓invoke` | serialized to `ContractCall.data: Vec<u8>` |
| Consensus | `PoWRewardParamsV1` (`native_token/src/model/mod.rs:251-290`); §8.2 `CoinbaseTransaction` (`src/linear/src/transaction.rs:211-235`) | `ClearInput` + `Output` + `Nullifier` (+ `MiningRecipient` barbs) | `↓mine` | `public_inputs: [[u8;32];9]`, note as `Vec<u8>` |
| Transaction | §8.2 `Transaction`/`ContractCall` (`src/tx/mod.rs:96-113`, `sdk/src/tx.rs:80-87`) | `ContractId` + `Vec<Nullifier>` + opaque calldata | `↓process`, `↓invoke` | `data: Vec<u8>`, `tx_commitment: [u8;32]` |
| Structural | `AeadEncryptedNote` (`sdk/src/crypto/note.rs:38-42`); `BlockHeader` (`src/linear/src/block.rs`) | `PublicKey`; merkle roots as `blake3::Hash` | `↓discover`/`↓encrypt`; `↓validate-pow` | blake3 for chain trees, `MerkleNode` for ZK trees |
| Non-money | `DaoEscrowBulla`, `MembershipNote`, `PurseId`, `BoxId` (`dao_escrow/src/model/mod.rs`, `purse`, `box`) | parent-bulla + `PublicKey`(→x,y) + `AssetId` + value/expiry + `BaseBlind` | `↓commit` | same Poseidon schema as `Commitment` |

**Recorded divergences.** PromissoryNote replaces the `PublicKey`(Point)
primitive with a field-element pubkey (`poseidon_hash(secret)`), a 6-element
tuple (`promissory_note/src/model/mod.rs:110-156`). The `src/linear` consensus
layer maintains a **parallel** newtype family — `CoinCommitment`,
`TokenCommitment`, `PedersenCoordinate` (`src/linear/src/transaction.rs:49/87/143`)
— rather than reusing SDK `Commitment`/`AssetId`. `BlockHeader` uses `blake3::Hash`
for chain-structural trees per §8.2, never the `pallas::Base` `MerkleNode`
(which is reserved for ZK-friendly commitment/nullifier trees referenced inside
`Input.merkle_root`). `CapabilityProof` carries an `IntentNullifier`, a type
§8.4 declares non-unifiable with `Nullifier`.

The takeaway: composition is *structurally uniform* (always a Poseidon hash over
a primitive tuple) but the type system only survives at the Rust struct
boundary; the three erasure boundaries are exactly where this session's
migration bugs surfaced, and exactly what `scripts/check_pipeline_build.sh` now
guards.

## 4. The wallet — the emergent-composition engine

The wallet is where composition is most important and most complex, because the
wallet is the one place that must construct capability *types* generically, for
*every* contract, from on-chain facts. Per §6.1 and wallet.md §2.2 the intended
path is:

> **discover commitment → decrypt note → resolve manifest → compose primitives
> → check barb coverage.**

### 4.1 What the logic demands

For each discovered output, the wallet resolves the producing contract's
manifest to learn *which primitives* the capability composes and *which barbs*
its action requires, calls `wallet_construct`, and — critically — treats a
`None` result as an error, not a skip: per §13, if the primitives don't cover
the required barbs, "fix the composition, not the wallet." The nine target
compositions are enumerated (and proved constructible) in
`capability.rs:379-487` and `Composition.lean`: native-token coinbase &
transfer, DAO vote, purse balance & withdraw, box take, identity credential,
multisig approval, attestation.

### 4.2 What exists vs what is wired

The composition **kernel** is built and unit-tested against the Lean mirror
(`Primitive`, `Barb`, `wallet_construct`, `TypedCapability::covers` in
`src/sdk/src/capability.rs`). But it has **zero callers in `bin/dww`** — it is
spec/proof-only. The live wallet uses a shallower, pre-composition path:

- **Path 1 — native token (bespoke, working, the one §13-sanctioned special
  case):** `scan.rs:187-289` discovers coinbase/transfer/spend/burn outputs and
  writes a `CapRecord` directly.
- **Path 2 — generic (partial):** `scan.rs:514-655` trial-decrypts foreign
  notes but re-decodes every one as the NativeToken format (`scan.rs:614-625`);
  anything else logs `"unknown-format"`. A `u8` capability **discriminant** is
  stamped afterward via `manifest.resolve_capability` in the impure
  `scan_block_linear` (`scan.rs:821-833`), while the integration TODO sits in the
  pure `scan_block` (`scan.rs:579-584`).
- **Display:** `bin/dww/src/capability.rs` has a *second, unrelated*
  `TypedCapability` (display metadata) whose resolver hardcodes
  `capability_name = "unknown"` (`:105`) and `function_code: 0` (`:139`).

### 4.3 The gaps between typed primitives and a working composition layer

1. **No call site** — `wallet_construct` is never invoked in `bin/dww`.
2. **The type bridge is missing** — `resolve_capability` fills
   `ResolvedCapability.primitives: Vec<String>` from
   `action.requires.capabilities` (`manifest.rs:188-191`), i.e. **capability
   names** ("creator", "treasury_governor"), *not* SDK `Primitive` types. The
   manifest schema (`ManifestCapability`, `manifest.rs:54-60`) declares no
   composing primitive types and no `required_barbs`. Nothing can convert a
   manifest into `(Vec<Primitive>, &[Barb])` today.
3. **Two divergent `TypedCapability` types** — SDK composition
   (`capability.rs:302-312`) vs dww display (`bin/dww/src/capability.rs:22-34`).
4. **Path 2 is NativeToken-shaped** — no manifest-driven decode/typing.
5. **Resolution is a `u8`, not a type** — a discriminant tag, no barb coverage
   check.
6. **The display resolver is inert** — always `"unknown"`.
7. **TODO placement mismatch** — pure `scan_block` has the TODO; the only
   manifest resolution added lives in impure `scan_block_linear`.

Closing gaps 1-3, 5-7 makes the wallet construct real composed capabilities for
what it already discovers; closing gap 4 (a manifest-driven note format) is the
true completion of the generic Path 2. This is the roadmap tracked in the
capability-composition plan.

## 5. Open questions / tensions

These are places where "composition follows its own logic" is not yet fully
closed. They are recorded so they are tracked, not silently assumed.

1. **`↓prove` is now carried by the `dleqProof` primitive** — RESOLVED.
   `dleqProof` (`Types.lean:185-188`) carries `{Barb.prove}` and was added to
   both `tenderBidType` and `bridgeWithdrawType` (`Composition.lean:159-174,
   396-410`). The Lean now models `↓prove` as a primitive barb carried by the
   DLEq proof primitive, matching the physical model where a discrete-log
   equality proof IS a discrete cryptographic primitive. The Rust kernel also
   carries `Barb::Prove` in its capability-type barb set and `Primitive::from_name`
   parses `"Prove"` as a valid barb. The emergent-barb tension (§1.2 consequence)
   is closed: `compose` yields `↓prove` because `dleqProof` carries it.

2. **Composition-level distinctness is unproved.** `compositionalDistinction`
   (`Pareto.lean:108-110`) is a `trivial`/`True` stub. Pareto-efficiency is
   machine-checked only at the primitive layer; "no accidental unification of
   *emergent* types" is stated, not proved.

3. **The ZK-soundness bridge is an axiom.** `circuitSoundnessBridge`
   (`Inversion.lean:44-61`) — the step from "capability type inhabited" to "a
   sound ZK proof exists" — is an axiom pending the full Halo2 model (§10.6,
   FUTURE WORK), justified for now by the manual audit of the circuits.

4. **Manifest → binary binding is pending.** The on-chain manifest hash
   ("Deployooor hardening", `manifest.md:16`) is not yet implemented, so a
   contract's declared composition is trust-verified socially (the trust-tier
   model, `manifest.rs:217-227`), not cryptographically bound at deploy time.

## References

- [Type System](type-system.md) — §6 (capability engine), §8 (namespace),
  §10 (verified properties), §13 (contracts are instances).
- [Wallet Architecture](wallet.md) — the wallet as capability construction engine.
- [Contract Manifest](manifest.md) — the on-chain capability-native ABI.
- `src/sdk/src/capability.rs` — the Rust composition kernel.
- `src/sdk/src/manifest.rs` — `resolve_capability`.
- `proofs/lean/src/DarkFi/Capability/{Types,Composition,Wallet,Inversion,Pareto,Distinction}.lean`
  — the machine-checked algebra.
