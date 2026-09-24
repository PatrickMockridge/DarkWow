/-
# The circuits, transcribed — every statement list, as data

**GENERATED FILE — do not edit.** `scripts/gen_circuit_transcription.py` writes it from the `.zk`
sources, and `scripts/run-all-tests.sh` re-runs the generator in `--check` mode, so a stale copy is a
gate failure. The generator parses with `script/circuit_instance_derivation.py`'s own lexer, the one
`scripts/check-circuit-instance-derivation.sh` gates on, which is what makes the transcription
machine-made rather than hand-typed — and what makes the fidelity claim "the data is the sources'
data", not "the data is the sources' meaning".

**It is off the default build path, deliberately, and that is a correctness requirement rather than a
speed one.** This module lives at `src/Transcribed.lean` in the library `lean_lib Transcribed`, built
by the gate as `lake build DarkFi Transcribed`, and is not reachable from `lake build DarkFi`. It held
that place in the `DarkFi` library until 2026-09-24, when a `LEAN_NUM_THREADS=4` build of that library
exhausted this host's memory and froze the machine: a thread cap bounds how many `lean` processes run,
not how much memory one of them uses, and kernel `decide` evaluations at this scale is where in this
tree that difference bites — 181 of them the day it froze, 178 now. The gate builds it under
`scripts/lean-build.sh`, which adds the cgroup memory
ceiling the thread cap never was. **A `lake build DarkFi` therefore does not type-check this file; the
gate does.** `CheckAxioms.lean` imports it directly, so the axiom walk still covers all 178
theorems.

What is here is one `List Stmt` per circuit in `InstanceDerivation`'s vocabulary, the names the
circuit holds (its `constant` and `witness` declarations), and one verdict per circuit closed by
`decide`. **The verdict is the model's, not the checker's**, and the two differ by design: the model
asks whether every exposed value is *determined* by what precedes it, while the checker additionally
accepts an exposed value that the circuit pins elsewhere (`redundant`) or that a host-side
justification declares free (`script/circuit_free_instances.txt`).

Measured: **167** of 178 circuits expose at least one value the model does not
find determined in-circuit, and **11** hold. Each refuted circuit names the first such
exposure **and the checker's class for that exposure**.

**The 167 are decomposed rather than asserted, and the decomposition is the finding.** For each
refuted circuit the generator asks the *checker* — `classify`, the gate's own classifier — what it
made of the exposure the model refused, and the answer is that the two rules disagree by design
almost everywhere:

* **154** of the 167 — The checker resolves it as `redundant` — pinned by another exposed
  determination, which the model's sequential rule does not follow.
* **11** of the 167 — The checker resolves it as `declared-free`, from a host-side justification
  in `script/circuit_free_instances.txt`.
* **1** of the 167 — The checker resolves it as `bound`, through a `constrain_equal_base` whose
  determining side is a declared constant.
* **1** of the 167 — **The checker fails it too** — one of the instances `OBL-Z16` names, where
  the model and the checker agree.

So the model does not contradict the checker; it **refines** it, and every one of the 167 is the
checker's weaker rule or the single boundary the model note names. Two consequences a reader should
take from this file rather than infer:

* `Axioms.NoFreeInstances`' *name* is a **strict** reading this tree mostly does not meet — 167 of
  178 circuits are refuted under it — while the property the tree actually enforces is the checker's
  four-verdict rule, whose failures are the instances `OBL-Z16` names (11 when this sentence was
  written on 2026-09-24, and re-run `scripts/check-circuit-instance-derivation.sh` for the count now —
  it is 4 as of that evening, because one site was repaired and one circuit deleted).
  The axiom is uninterpreted, so
  nothing false is assumed; a reader who takes its name literally is over-reading it, and the
  per-circuit class recorded below is where that is written down;
* the model's *one* disagreement with the checker that is not a documented weaker class is `bound`:
  the checker's `is_determined` counts a declared **constant** as a determination, while the model's
  `determinedB` accepts only a *bound* name, and a constant is held and never bound. It costs exactly
  the circuits recorded `bound` below — a `constrain_equal_base` whose determining side is a constant
  — so the model's constant boundary is **met**, in that direction, not hypothetical.

The verdicts were computed twice: once here, by the generator's own implementation of the same rule,
and once by the kernel from the transcription. A disagreement fails the build, which is the only
reason the generator is allowed to predict at all.

One boundary in the *other* direction stays untested, stated because it would show up as a false
positive the day a circuit meets it: a bare `constant` exposed by `constrain_instance` would fail the
model's property, where the checker accepts a constant by declaration. No circuit in this tree exposes
one — measured, every undetermined exposure across the 167 refutations is a witness and none is a
constant — so that direction is untested rather than settled, while the direction above is met.

Not transcribed, and counted rather than dropped silently: 21 bare opcode-call statements
(`less_than_strict`, `bool_check`, `less_than_loose`) which constrain but expose nothing, so the
instance property is unaffected — the checker skips them for the same reason. Everything else in the
sources is here; an unrecognised statement form fails the generator rather than being omitted.

`Axioms.NoFreeInstances` is **not** replaced by this file: it is `(r, s)`-indexed, and the mapping
from a resource/action pair to a circuit is not in the tree, so this supplies the data the bridge
needs without supplying the bridge. See `OBL-T7` in `doc/src/arch/verification-hazop.md`.

**The verdicts were once unbuildable here, and one extracted function in the model fixed it.** Until
2026-09-24 this module exceeded 24 GiB in a single `lean` process and was OOM-killed at both a 16 GiB
and a 24 GiB ceiling, so no `.olean` had ever been produced and the kernel had closed none of the
verdicts below. Extracting `boundWalk`'s `assign` arm into `bindAssign` removed it: the whole
transcription — all 178 verdicts as it now stands, 181 when that was measured — builds in
**~71 s and 743 MB** as one module. **Which circuits were
expensive, and why, is not established** — see `bindAssign`'s docstring, which carries the controlled
comparison that justifies the change and the rival explanations it does not settle, and retracts the
short-circuit story this header first told. Sharding the artefact was tried while the cause was unknown
and has been withdrawn: it was a workaround for a defect, not a property of the data.
-/

import DarkFi.Circuits.InstanceDerivation

namespace Circuits.Transcribed

open Circuits.InstanceDerivation

/-! ===== The circuits, in source order =====

178 circuits, 2677 statements transcribed; **11** satisfy the model's property and **167** do not, the latter named by the
first undetermined exposure in each, with the checker's class for that exposure named beneath it.
-/

/-- `bin/darkirc/proof/rlnv2-diff-signal.zk` — 6 exposure(s). -/

def darkirc_rlnv2_diff_signal_held : List Name := ["epoch", "external_nullifier", "identity_leaf_pos", "identity_nullifier", "identity_path", "identity_trapdoor", "message_id", "user_message_limit", "x"]


def darkirc_rlnv2_diff_signal_stmts : List Stmt :=
[
  .constrainInstance (.var "epoch"),
  .constrainInstance (.var "external_nullifier"),
  .rangeCheck 64 (.var "message_id"),
  .rangeCheck 64 (.var "user_message_limit"),
  .assign "a_0" (.op "poseidon_hash" [.var "identity_nullifier", .var "identity_trapdoor"]),
  .assign "a_1" (.op "poseidon_hash" [.var "a_0", .var "external_nullifier", .var "message_id"]),
  .assign "x_a_1" (.op "base_mul" [.var "x", .var "a_1"]),
  .assign "y" (.op "base_add" [.var "a_0", .var "x_a_1"]),
  .constrainInstance (.var "x"),
  .constrainInstance (.var "y"),
  .assign "internal_nullifier" (.op "poseidon_hash" [.var "a_1"]),
  .constrainInstance (.var "internal_nullifier"),
  .assign "identity_commitment" (.op "poseidon_hash" [.var "a_0", .var "user_message_limit"]),
  .assign "root" (.op "merkle_root" [.var "identity_leaf_pos", .var "identity_path", .var "identity_commitment"]),
  .constrainInstance (.var "root")
]


/-- **The property fails** for `bin/darkirc/proof/rlnv2-diff-signal.zk`: its first undetermined exposure is
    `.var "epoch"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem darkirc_rlnv2_diff_signal_has_a_free_instance :
    ¬ NoFreeInstance darkirc_rlnv2_diff_signal_held darkirc_rlnv2_diff_signal_stmts := by
  unfold NoFreeInstance
  decide


/-- `bin/darkirc/proof/rlnv2-diff-slash.zk` — 1 exposure(s). -/

def darkirc_rlnv2_diff_slash_held : List Name := ["identity_leaf_pos", "identity_path", "secret_key"]


def darkirc_rlnv2_diff_slash_stmts : List Stmt :=
[
  .assign "identity_derivation_path" (.op "witness_base" [.lit 11]),
  .assign "identity_commit" (.op "poseidon_hash" [.var "identity_derivation_path", .var "secret_key"]),
  .assign "root" (.op "merkle_root" [.var "identity_leaf_pos", .var "identity_path", .var "identity_commit"]),
  .constrainInstance (.var "root")
]


/-- **The property holds** for `bin/darkirc/proof/rlnv2-diff-slash.zk`. -/
@[axiom_budget 0]
theorem darkirc_rlnv2_diff_slash_no_free_instance :
    NoFreeInstance darkirc_rlnv2_diff_slash_held darkirc_rlnv2_diff_slash_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/arithmetic.zk` — 3 exposure(s). -/

def arithmetic_held : List Name := ["a", "b"]


def arithmetic_stmts : List Stmt :=
[
  .assign "sum" (.op "base_add" [.var "a", .var "b"]),
  .constrainInstance (.var "sum"),
  .assign "product" (.op "base_mul" [.var "a", .var "b"]),
  .constrainInstance (.var "product"),
  .assign "difference" (.op "base_sub" [.var "a", .var "b"]),
  .constrainInstance (.var "difference")
]


/-- **The property holds** for `proofs/core/arithmetic.zk`. -/
@[axiom_budget 0]
theorem arithmetic_no_free_instance :
    NoFreeInstance arithmetic_held arithmetic_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/burn.zk` — 8 exposure(s). -/

def burn_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "leaf_pos", "path", "secret", "serial", "signature_secret", "token", "token_blind", "value", "value_blind"]


def burn_stmts : List Stmt :=
[
  .assign "nullifier" (.op "poseidon_hash" [.var "secret", .var "serial"]),
  .constrainInstance (.var "nullifier"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .assign "value_commit_x" (.op "ec_get_x" [.var "value_commit"]),
  .assign "value_commit_y" (.op "ec_get_y" [.var "value_commit"]),
  .constrainInstance (.var "value_commit_x"),
  .constrainInstance (.var "value_commit_y"),
  .assign "tcv" (.op "ec_mul_base" [.var "token", .var "NULLIFIER_K"]),
  .assign "tcr" (.op "ec_mul" [.var "token_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "token_commit" (.op "ec_add" [.var "tcv", .var "tcr"]),
  .assign "token_commit_x" (.op "ec_get_x" [.var "token_commit"]),
  .assign "token_commit_y" (.op "ec_get_y" [.var "token_commit"]),
  .constrainInstance (.var "token_commit_x"),
  .constrainInstance (.var "token_commit_y"),
  .assign "pub" (.op "ec_mul_base" [.var "secret", .var "NULLIFIER_K"]),
  .assign "pub_x" (.op "ec_get_x" [.var "pub"]),
  .assign "pub_y" (.op "ec_get_y" [.var "pub"]),
  .assign "C" (.op "poseidon_hash" [.var "pub_x", .var "pub_y", .var "value", .var "token", .var "serial"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "C"]),
  .constrainInstance (.var "root"),
  .assign "signature_public" (.op "ec_mul_base" [.var "signature_secret", .var "NULLIFIER_K"]),
  .assign "signature_x" (.op "ec_get_x" [.var "signature_public"]),
  .assign "signature_y" (.op "ec_get_y" [.var "signature_public"]),
  .constrainInstance (.var "signature_x"),
  .constrainInstance (.var "signature_y")
]


/-- **The property holds** for `proofs/core/burn.zk`. -/
@[axiom_budget 0]
theorem burn_no_free_instance :
    NoFreeInstance burn_held burn_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/encrypt.zk` — 5 exposure(s). -/

def encrypt_held : List Name := ["ephem_secret", "pubkey", "value_1", "value_2", "value_3"]


def encrypt_stmts : List Stmt :=
[
  .assign "ephem_pub" (.op "ec_mul_var_base" [.var "ephem_secret", .var "pubkey"]),
  .assign "ephem_pub_x" (.op "ec_get_x" [.var "ephem_pub"]),
  .assign "ephem_pub_y" (.op "ec_get_y" [.var "ephem_pub"]),
  .constrainInstance (.var "ephem_pub_x"),
  .constrainInstance (.var "ephem_pub_y"),
  .assign "shared_secret" (.op "poseidon_hash" [.var "ephem_pub_x", .var "ephem_pub_y"]),
  .assign "N1" (.op "witness_base" [.lit 1]),
  .assign "N2" (.op "witness_base" [.lit 2]),
  .assign "N3" (.op "witness_base" [.lit 3]),
  .assign "blind_1" (.op "poseidon_hash" [.var "shared_secret", .var "N1"]),
  .assign "blind_2" (.op "poseidon_hash" [.var "shared_secret", .var "N2"]),
  .assign "blind_3" (.op "poseidon_hash" [.var "shared_secret", .var "N3"]),
  .assign "enc_value_1" (.op "base_mul" [.var "value_1", .var "blind_1"]),
  .assign "enc_value_2" (.op "base_mul" [.var "value_2", .var "blind_2"]),
  .assign "enc_value_3" (.op "base_mul" [.var "value_3", .var "blind_3"]),
  .constrainInstance (.var "enc_value_1"),
  .constrainInstance (.var "enc_value_2"),
  .constrainInstance (.var "enc_value_3")
]


/-- **The property holds** for `proofs/core/encrypt.zk`. -/
@[axiom_budget 0]
theorem encrypt_no_free_instance :
    NoFreeInstance encrypt_held encrypt_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/inclusion_proof.zk` — 2 exposure(s). -/

def inclusion_proof_held : List Name := ["blind", "leaf", "leaf_pos", "path"]


def inclusion_proof_stmts : List Stmt :=
[
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "leaf"]),
  .constrainInstance (.var "root"),
  .assign "enc_leaf" (.op "poseidon_hash" [.var "leaf", .var "blind"]),
  .constrainInstance (.var "enc_leaf")
]


/-- **The property holds** for `proofs/core/inclusion_proof.zk`. -/
@[axiom_budget 0]
theorem inclusion_proof_no_free_instance :
    NoFreeInstance inclusion_proof_held inclusion_proof_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/mint.zk` — 5 exposure(s). -/

def mint_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "pub_x", "pub_y", "serial", "token", "token_blind", "value", "value_blind"]


def mint_stmts : List Stmt :=
[
  .assign "C" (.op "poseidon_hash" [.var "pub_x", .var "pub_y", .var "value", .var "token", .var "serial"]),
  .constrainInstance (.var "C"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .assign "value_commit_x" (.op "ec_get_x" [.var "value_commit"]),
  .assign "value_commit_y" (.op "ec_get_y" [.var "value_commit"]),
  .constrainInstance (.var "value_commit_x"),
  .constrainInstance (.var "value_commit_y"),
  .assign "tcv" (.op "ec_mul_base" [.var "token", .var "NULLIFIER_K"]),
  .assign "tcr" (.op "ec_mul" [.var "token_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "token_commit" (.op "ec_add" [.var "tcv", .var "tcr"]),
  .assign "token_commit_x" (.op "ec_get_x" [.var "token_commit"]),
  .assign "token_commit_y" (.op "ec_get_y" [.var "token_commit"]),
  .constrainInstance (.var "token_commit_x"),
  .constrainInstance (.var "token_commit_y")
]


/-- **The property holds** for `proofs/core/mint.zk`. -/
@[axiom_budget 0]
theorem mint_no_free_instance :
    NoFreeInstance mint_held mint_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/nested.zk` — 1 exposure(s). -/

def nested_held : List Name := ["a"]


def nested_stmts : List Stmt :=
[
  .assign "triple_hash" (.op "poseidon_hash" [.op "poseidon_hash" [.op "poseidon_hash" [.var "a"]]]),
  .constrainInstance (.var "triple_hash")
]


/-- **The property holds** for `proofs/core/nested.zk`. -/
@[axiom_budget 0]
theorem nested_no_free_instance :
    NoFreeInstance nested_held nested_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/opcodes.zk` — 11 exposure(s). -/

def opcodes_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "a", "b", "blind", "cond", "ephem_secret", "leaf_pos", "path", "pubkey", "secret", "value", "value_blind"]


def opcodes_stmts : List Stmt :=
[
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .assign "value_commit_x" (.op "ec_get_x" [.var "value_commit"]),
  .assign "value_commit_y" (.op "ec_get_y" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "vcv2" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr2" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit2" (.op "ec_add" [.var "vcv2", .var "vcr2"]),
  .constrainEq (.var "value_commit") (.var "value_commit2"),
  .assign "zero" (.op "witness_base" [.lit 0]),
  .assign "one" (.op "witness_base" [.lit 1]),
  .assign "two" (.op "witness_base" [.lit 2]),
  .assign "c" (.op "poseidon_hash" [.var "one", .var "two", .var "blind"]),
  .constrainInstance (.var "c"),
  .assign "d" (.op "poseidon_hash" [.var "one", .var "blind", .op "ec_get_x" [.var "value_commit"], .op "ec_get_y" [.var "value_commit"]]),
  .constrainInstance (.var "d"),
  .assign "d2" (.op "poseidon_hash" [.var "one", .var "blind", .op "ec_get_x" [.var "value_commit2"], .op "ec_get_y" [.var "value_commit2"]]),
  .constrainEq (.var "d") (.var "d2"),
  .rangeCheck 64 (.var "a"),
  .rangeCheck 253 (.var "b"),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "c"]),
  .constrainInstance (.var "root"),
  .assign "public" (.op "ec_mul_base" [.var "secret", .var "NULLIFIER_K"]),
  .constrainInstance (.op "ec_get_x" [.var "public"]),
  .constrainInstance (.op "ec_get_y" [.var "public"]),
  .assign "ephem_public" (.op "ec_mul_var_base" [.var "ephem_secret", .var "pubkey"]),
  .constrainInstance (.op "ec_get_x" [.var "ephem_public"]),
  .constrainInstance (.op "ec_get_y" [.var "ephem_public"]),
  .assign "out" (.op "cond_select" [.var "cond", .var "a", .var "b"]),
  .constrainInstance (.var "out"),
  .assign "zz" (.op "zero_cond" [.var "zero", .var "c"]),
  .constrainInstance (.var "zz")
]


/-- **The property holds** for `proofs/core/opcodes.zk`. -/
@[axiom_budget 0]
theorem opcodes_no_free_instance :
    NoFreeInstance opcodes_held opcodes_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/smt.zk` — 1 exposure(s). -/

def smt_held : List Name := ["leaf", "path"]


def smt_stmts : List Stmt :=
[
  .assign "root" (.op "sparse_merkle_root" [.var "leaf", .var "path", .var "leaf"]),
  .constrainInstance (.var "root")
]


/-- **The property holds** for `proofs/core/smt.zk`. -/
@[axiom_budget 0]
theorem smt_no_free_instance :
    NoFreeInstance smt_held smt_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/tx.zk` — 9 exposure(s). -/

def tx_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "c1_cm_path", "c1_cm_pos", "c1_opening", "c1_rho", "c1_root_sk", "c1_sk", "c1_sk_path", "c1_sk_pos", "c1_sn", "c1_value", "c3_opening", "c3_pk", "c3_rho", "c3_value", "c4_opening", "c4_pk", "c4_rho", "c4_value", "root"]


def tx_stmts : List Stmt :=
[
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "PREFIX_EVL" (.op "witness_base" [.lit 2]),
  .assign "PREFIX_SEED" (.op "witness_base" [.lit 3]),
  .assign "PREFIX_CM" (.op "witness_base" [.lit 4]),
  .assign "PREFIX_PK" (.op "witness_base" [.lit 5]),
  .assign "PREFIX_SN" (.op "witness_base" [.lit 6]),
  .assign "c1_pk" (.op "poseidon_hash" [.var "PREFIX_PK", .var "c1_root_sk"]),
  .assign "c1_cm_msg" (.op "poseidon_hash" [.var "PREFIX_CM", .var "c1_pk", .var "c1_value", .var "c1_rho"]),
  .assign "c1_cm_v" (.op "ec_mul_base" [.var "c1_cm_msg", .var "NULLIFIER_K"]),
  .assign "c1_cm_r" (.op "ec_mul" [.var "c1_opening", .var "VALUE_COMMIT_RANDOM"]),
  .assign "c1_cm" (.op "ec_add" [.var "c1_cm_v", .var "c1_cm_r"]),
  .assign "c1_cm_x" (.op "ec_get_x" [.var "c1_cm"]),
  .assign "c1_cm_y" (.op "ec_get_y" [.var "c1_cm"]),
  .assign "c1_cm_hash" (.op "poseidon_hash" [.var "c1_cm_x", .var "c1_cm_y"]),
  .constrainInstance (.var "c1_cm_x"),
  .constrainInstance (.var "c1_cm_y"),
  .assign "c3_cm_msg" (.op "poseidon_hash" [.var "PREFIX_CM", .var "c3_pk", .var "c3_value", .var "c3_rho"]),
  .assign "c3_cm_v" (.op "ec_mul_base" [.var "c3_cm_msg", .var "NULLIFIER_K"]),
  .assign "c3_cm_r" (.op "ec_mul" [.var "c3_opening", .var "VALUE_COMMIT_RANDOM"]),
  .assign "c3_cm" (.op "ec_add" [.var "c3_cm_v", .var "c3_cm_r"]),
  .assign "c3_cm_x" (.op "ec_get_x" [.var "c3_cm"]),
  .constrainInstance (.var "c3_cm_x"),
  .assign "c3_cm_y" (.op "ec_get_x" [.var "c3_cm"]),
  .constrainInstance (.var "c3_cm_y"),
  .assign "c4_cm_msg" (.op "poseidon_hash" [.var "PREFIX_CM", .var "c4_pk", .var "c4_value", .var "c4_rho"]),
  .assign "c4_cm_v" (.op "ec_mul_base" [.var "c4_cm_msg", .var "NULLIFIER_K"]),
  .assign "c4_cm_r" (.op "ec_mul" [.var "c4_opening", .var "VALUE_COMMIT_RANDOM"]),
  .assign "c4_cm" (.op "ec_add" [.var "c4_cm_v", .var "c4_cm_r"]),
  .assign "c4_cm_x" (.op "ec_get_x" [.var "c4_cm"]),
  .constrainInstance (.var "c4_cm_x"),
  .assign "c4_cm_y" (.op "ec_get_y" [.var "c4_cm"]),
  .constrainInstance (.var "c4_cm_y"),
  .assign "outval" (.op "base_add" [.var "c3_value", .var "c4_value"]),
  .constrainEq (.var "c1_value") (.var "outval"),
  .assign "c1_root" (.op "merkle_root" [.var "c1_cm_pos", .var "c1_cm_path", .var "c1_cm_hash"]),
  .constrainInstance (.var "c1_root"),
  .assign "c1_sk_root" (.op "merkle_root" [.var "c1_sk_pos", .var "c1_sk_path", .var "c1_sk"]),
  .constrainInstance (.var "c1_sk_root"),
  .assign "c1_sn" (.op "poseidon_hash" [.var "PREFIX_SN", .var "c1_root_sk", .var "c1_rho", .var "ZERO"]),
  .constrainInstance (.var "c1_sn")
]


/-- **The property holds** for `proofs/core/tx.zk`. -/
@[axiom_budget 0]
theorem tx_no_free_instance :
    NoFreeInstance tx_held tx_stmts := by
  unfold NoFreeInstance
  decide


/-- `proofs/core/voting.zk` — 4 exposure(s). -/

def voting_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "leaf_pos", "path", "process_id_0", "process_id_1", "secret_key", "vote", "vote_blind"]


def voting_stmts : List Stmt :=
[
  .assign "process_id" (.op "poseidon_hash" [.var "process_id_0", .var "process_id_1"]),
  .assign "nullifier" (.op "poseidon_hash" [.var "secret_key", .var "process_id"]),
  .constrainInstance (.var "nullifier"),
  .assign "public_key" (.op "ec_mul_base" [.var "secret_key", .var "NULLIFIER_K"]),
  .assign "public_x" (.op "ec_get_x" [.var "public_key"]),
  .assign "public_y" (.op "ec_get_y" [.var "public_key"]),
  .assign "pk_hash" (.op "poseidon_hash" [.var "public_x", .var "public_y"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "pk_hash"]),
  .constrainInstance (.var "root"),
  .assign "vcv" (.op "ec_mul_short" [.var "vote", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "vote_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "vote_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .assign "vote_commit_x" (.op "ec_get_x" [.var "vote_commit"]),
  .assign "vote_commit_y" (.op "ec_get_y" [.var "vote_commit"]),
  .constrainInstance (.var "vote_commit_x"),
  .constrainInstance (.var "vote_commit_y")
]


/-- **The property holds** for `proofs/core/voting.zk`. -/
@[axiom_budget 0]
theorem voting_no_free_instance :
    NoFreeInstance voting_held voting_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/attest_slash.zk` — 2 exposure(s). -/

def attestation_attest_slash_held : List Name := ["NULLIFIER_K", "attester_pub_x", "attester_pub_y", "attester_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def attestation_attest_slash_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/attest_slash.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_attest_slash_has_a_free_instance :
    ¬ NoFreeInstance attestation_attest_slash_held attestation_attest_slash_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/check_not_revoked.zk` — 2 exposure(s). -/

def attestation_check_not_revoked_held : List Name := ["NULLIFIER_K", "nonce", "tx_binding", "tx_commitment", "tx_nonce"]


def attestation_check_not_revoked_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "leaf" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/check_not_revoked.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_check_not_revoked_has_a_free_instance :
    ¬ NoFreeInstance attestation_check_not_revoked_held attestation_check_not_revoked_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/commit_fee_schedule.zk` — 2 exposure(s). -/

def attestation_commit_fee_schedule_held : List Name := ["NULLIFIER_K", "attester_pub_x", "attester_pub_y", "attester_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def attestation_commit_fee_schedule_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/commit_fee_schedule.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_commit_fee_schedule_has_a_free_instance :
    ¬ NoFreeInstance attestation_commit_fee_schedule_held attestation_commit_fee_schedule_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/consume_claim.zk` — 6 exposure(s). -/

def attestation_consume_claim_held : List Name := ["NULLIFIER_K", "claim_id", "consumer_pub_x", "consumer_pub_y", "consumer_secret", "nullifier", "tx_binding", "tx_commitment", "tx_nonce"]


def attestation_consume_claim_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "consumer_pub" (.op "ec_mul_base" [.var "consumer_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "consumer_pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "consumer_pub"]),
  .constrainEq (.var "derived_pub_x") (.var "consumer_pub_x"),
  .constrainEq (.var "derived_pub_y") (.var "consumer_pub_y"),
  .assign "derived_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "claim_id", .var "consumer_secret"]),
  .constrainEq (.var "derived_nullifier") (.var "nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "claim_id"),
  .constrainInstance (.var "consumer_pub_x"),
  .constrainInstance (.var "consumer_pub_y"),
  .constrainInstance (.var "nullifier"),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/consume_claim.zk`: its first undetermined exposure is
    `.var "claim_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_consume_claim_has_a_free_instance :
    ¬ NoFreeInstance attestation_consume_claim_held attestation_consume_claim_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/create_attestation.zk` — 2 exposure(s). -/

def attestation_create_attestation_held : List Name := ["NULLIFIER_K", "attester_pub_x", "attester_pub_y", "attester_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def attestation_create_attestation_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/create_attestation.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_create_attestation_has_a_free_instance :
    ¬ NoFreeInstance attestation_create_attestation_held attestation_create_attestation_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/create_claim.zk` — 2 exposure(s). -/

def attestation_create_claim_held : List Name := ["NULLIFIER_K", "claim_data", "creator_pub_x", "creator_pub_y", "creator_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def attestation_create_claim_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/create_claim.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_create_claim_has_a_free_instance :
    ¬ NoFreeInstance attestation_create_claim_held attestation_create_claim_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/delegate_attestation.zk` — 3 exposure(s). -/

def attestation_delegate_attestation_held : List Name := ["NULLIFIER_K", "delegatee_pub_x", "delegatee_pub_y", "delegator_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def attestation_delegate_attestation_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "delegatee_leaf" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "delegatee_pub_x", .var "delegatee_pub_y"]),
  .constrainInstance (.var "delegatee_leaf"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/delegate_attestation.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_delegate_attestation_has_a_free_instance :
    ¬ NoFreeInstance attestation_delegate_attestation_held attestation_delegate_attestation_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/update_delegation.zk` — 2 exposure(s). -/

def attestation_update_delegation_held : List Name := ["NULLIFIER_K", "delegator_pub_x", "delegator_pub_y", "delegator_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def attestation_update_delegation_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/update_delegation.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_update_delegation_has_a_free_instance :
    ¬ NoFreeInstance attestation_update_delegation_held attestation_update_delegation_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/verify_chain.zk` — 2 exposure(s). -/

def attestation_verify_chain_held : List Name := ["NULLIFIER_K", "chain_root", "tx_binding", "tx_commitment", "tx_nonce", "verifier_pub_x", "verifier_pub_y", "verifier_secret"]


def attestation_verify_chain_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/verify_chain.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_verify_chain_has_a_free_instance :
    ¬ NoFreeInstance attestation_verify_chain_held attestation_verify_chain_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/attestation/proof/verify_claim.zk` — 2 exposure(s). -/

def attestation_verify_claim_held : List Name := ["NULLIFIER_K", "attestation_data", "evidence", "nonce", "tx_binding", "tx_commitment", "tx_nonce"]


def attestation_verify_claim_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "evidence_hash" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "evidence"]),
  .assign "attestation_hash" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "attestation_data"]),
  .assign "leaf" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/attestation/proof/verify_claim.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem attestation_verify_claim_has_a_free_instance :
    ¬ NoFreeInstance attestation_verify_claim_held attestation_verify_claim_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/auction/proof/claim_winnings.zk` — 2 exposure(s). -/

def auction_claim_winnings_held : List Name := ["NULLIFIER_K", "auction_id", "tx_binding", "tx_commitment", "tx_nonce", "winner_pub_x", "winner_pub_y", "winner_secret"]


def auction_claim_winnings_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "winner_pub" (.op "ec_mul_base" [.var "winner_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "winner_pub"]) (.var "winner_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "winner_pub"]) (.var "winner_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/auction/proof/claim_winnings.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem auction_claim_winnings_has_a_free_instance :
    ¬ NoFreeInstance auction_claim_winnings_held auction_claim_winnings_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/auction/proof/close_auction.zk` — 2 exposure(s). -/

def auction_close_auction_held : List Name := ["NULLIFIER_K", "auction_id", "seller_pub_x", "seller_pub_y", "seller_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def auction_close_auction_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "seller_pub" (.op "ec_mul_base" [.var "seller_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "seller_pub"]) (.var "seller_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "seller_pub"]) (.var "seller_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/auction/proof/close_auction.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem auction_close_auction_has_a_free_instance :
    ¬ NoFreeInstance auction_close_auction_held auction_close_auction_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/auction/proof/create_auction.zk` — 3 exposure(s). -/

def auction_create_auction_held : List Name := ["NULLIFIER_K", "asset_id", "block_height", "nonce", "seller_pub_x", "seller_pub_y", "seller_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def auction_create_auction_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "seller_pub" (.op "ec_mul_base" [.var "seller_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "seller_pub"]) (.var "seller_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "seller_pub"]) (.var "seller_pub_y"),
  .assign "seller_commitment" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "seller_pub_x", .var "seller_pub_y"]),
  .assign "auction_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "seller_pub_x", .var "seller_pub_y", .var "asset_id", .var "block_height", .var "nonce"]),
  .constrainInstance (.var "auction_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/auction/proof/create_auction.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem auction_create_auction_has_a_free_instance :
    ¬ NoFreeInstance auction_create_auction_held auction_create_auction_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/auction/proof/place_bid.zk` — 3 exposure(s). -/

def auction_place_bid_held : List Name := ["NULLIFIER_K", "amount", "auction_id", "bidder_pub_x", "bidder_pub_y", "bidder_secret", "block_height", "nonce", "tx_binding", "tx_commitment", "tx_nonce"]


def auction_place_bid_stmts : List Stmt :=
[
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "bidder_pub" (.op "ec_mul_base" [.var "bidder_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "bidder_pub"]) (.var "bidder_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "bidder_pub"]) (.var "bidder_pub_y"),
  .assign "bid_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "auction_id", .var "bidder_pub_x", .var "bidder_pub_y", .var "amount", .var "nonce"]),
  .constrainInstance (.var "bid_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/auction/proof/place_bid.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem auction_place_bid_has_a_free_instance :
    ¬ NoFreeInstance auction_place_bid_held auction_place_bid_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/auction/proof/refund_bid.zk` — 3 exposure(s). -/

def auction_refund_bid_held : List Name := ["NULLIFIER_K", "bid_id", "bidder_pub_x", "bidder_pub_y", "bidder_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def auction_refund_bid_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "bidder_pub" (.op "ec_mul_base" [.var "bidder_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "bidder_pub"]) (.var "bidder_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "bidder_pub"]) (.var "bidder_pub_y"),
  .assign "refund_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "bid_id", .var "bidder_secret"]),
  .constrainInstance (.var "refund_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/auction/proof/refund_bid.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem auction_refund_bid_has_a_free_instance :
    ¬ NoFreeInstance auction_refund_bid_held auction_refund_bid_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/auction/proof/settle_auction.zk` — 3 exposure(s). -/

def auction_settle_auction_held : List Name := ["NULLIFIER_K", "auction_id", "seller_pub_x", "seller_pub_y", "seller_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def auction_settle_auction_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "seller_pub" (.op "ec_mul_base" [.var "seller_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "seller_pub"]) (.var "seller_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "seller_pub"]) (.var "seller_pub_y"),
  .assign "settlement_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "auction_id", .var "seller_secret"]),
  .constrainInstance (.var "settlement_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/auction/proof/settle_auction.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem auction_settle_auction_has_a_free_instance :
    ¬ NoFreeInstance auction_settle_auction_held auction_settle_auction_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/baccarat/proof/commit_bet.zk` — 5 exposure(s). -/

def baccarat_commit_bet_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "bet_type", "bet_value", "blind", "player_pub_x", "player_pub_y", "secret_nonce", "tx_binding", "tx_commitment", "tx_nonce", "value_blind"]


def baccarat_commit_bet_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "bet_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "player_pub_x", .var "player_pub_y", .var "bet_type", .var "bet_value", .var "secret_nonce", .var "blind", .var "asset_id"]),
  .constrainInstance (.var "bet_id"),
  .assign "vcv" (.op "ec_mul_short" [.var "bet_value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/baccarat/proof/commit_bet.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem baccarat_commit_bet_has_a_free_instance :
    ¬ NoFreeInstance baccarat_commit_bet_held baccarat_commit_bet_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/baccarat/proof/draw_cards.zk` — 4 exposure(s). -/

def baccarat_draw_cards_held : List Name := ["bet_id", "secret_nonce", "secret_nonce_commit", "tx_binding", "tx_commitment", "tx_nonce"]


def baccarat_draw_cards_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "computed_commit" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "secret_nonce"]),
  .constrainEq (.var "computed_commit") (.var "secret_nonce_commit"),
  .constrainInstance (.var "bet_id"),
  .constrainInstance (.var "secret_nonce_commit"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/baccarat/proof/draw_cards.zk`: its first undetermined exposure is
    `.var "bet_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem baccarat_draw_cards_has_a_free_instance :
    ¬ NoFreeInstance baccarat_draw_cards_held baccarat_draw_cards_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/baccarat/proof/house_close.zk` — 6 exposure(s). -/

def baccarat_house_close_held : List Name := ["NULLIFIER_K", "bet_id", "close_nullifier", "house_pub_x", "house_pub_y", "house_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def baccarat_house_close_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "house_pub" (.op "ec_mul_base" [.var "house_secret", .var "NULLIFIER_K"]),
  .assign "house_pub_x_computed" (.op "ec_get_x" [.var "house_pub"]),
  .assign "house_pub_y_computed" (.op "ec_get_y" [.var "house_pub"]),
  .constrainEq (.var "house_pub_x_computed") (.var "house_pub_x"),
  .constrainEq (.var "house_pub_y_computed") (.var "house_pub_y"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "bet_id", .var "house_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "close_nullifier"),
  .constrainInstance (.var "bet_id"),
  .constrainInstance (.var "house_pub_x"),
  .constrainInstance (.var "house_pub_y"),
  .constrainInstance (.var "close_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/baccarat/proof/house_close.zk`: its first undetermined exposure is
    `.var "bet_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem baccarat_house_close_has_a_free_instance :
    ¬ NoFreeInstance baccarat_house_close_held baccarat_house_close_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/baccarat/proof/settle_bet.zk` — 3 exposure(s). -/

def baccarat_settle_bet_held : List Name := ["asset_id", "bet_id", "bet_type", "bet_value", "blind", "player_pub_x", "player_pub_y", "secret_nonce", "tx_binding", "tx_commitment", "tx_nonce"]


def baccarat_settle_bet_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_bet_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "player_pub_x", .var "player_pub_y", .var "bet_type", .var "bet_value", .var "secret_nonce", .var "blind", .var "asset_id"]),
  .constrainInstance (.var "derived_bet_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/baccarat/proof/settle_bet.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem baccarat_settle_bet_has_a_free_instance :
    ¬ NoFreeInstance baccarat_settle_bet_held baccarat_settle_bet_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/bearer_bond/proof/blind_output.zk` — 7 exposure(s). -/

def bearer_bond_blind_output_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "asset_id_blind", "coin_public", "coin_spend_hook", "commitment_blind", "tx_binding", "tx_commitment", "tx_nonce", "user_data", "value", "value_blind"]


def bearer_bond_blind_output_stmts : List Stmt :=
[
  .assign "DOMAIN_TOK_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "coin_public", .var "value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .constrainInstance (.var "coin"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "token_commit" (.op "poseidon_hash" [.var "DOMAIN_TOK_COMMIT", .var "asset_id", .var "asset_id_blind"]),
  .constrainInstance (.var "token_commit"),
  .constrainInstance (.var "coin_spend_hook"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "value")
]


/-- **The property fails** for `src/contract/bearer_bond/proof/blind_output.zk`: its first undetermined exposure is
    `.var "coin_spend_hook"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem bearer_bond_blind_output_has_a_free_instance :
    ¬ NoFreeInstance bearer_bond_blind_output_held bearer_bond_blind_output_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/bearer_bond/proof/burn.zk` — 10 exposure(s). -/

def bearer_bond_burn_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "asset_id_blind", "coin_spend_hook", "commitment_blind", "leaf_pos", "path", "signature_secret", "spend_secret", "tx_binding", "tx_commitment", "tx_nonce", "user_data", "user_data_blind", "value", "value_blind"]


def bearer_bond_burn_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TOK_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_USER_DATA_ENC" (.op "witness_base" [.lit 6]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "pub" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "spend_secret"]),
  .assign "coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "pub", .var "value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "spend_secret", .var "coin"]),
  .constrainInstance (.var "nullifier"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "token_commit" (.op "poseidon_hash" [.var "DOMAIN_TOK_COMMIT", .var "asset_id", .var "asset_id_blind"]),
  .constrainInstance (.var "token_commit"),
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .assign "coin_incl" (.op "zero_cond" [.var "value", .var "coin"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "coin_incl"]),
  .constrainInstance (.var "root"),
  .assign "user_data_enc" (.op "poseidon_hash" [.var "DOMAIN_USER_DATA_ENC", .var "user_data", .var "user_data_blind"]),
  .constrainInstance (.var "user_data_enc"),
  .constrainInstance (.var "coin_spend_hook"),
  .assign "derived_signature_secret" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "spend_secret", .var "nullifier"]),
  .constrainEq (.var "derived_signature_secret") (.var "signature_secret"),
  .assign "signature_public" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "signature_secret"]),
  .constrainInstance (.var "signature_public"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "value")
]


/-- **The property fails** for `src/contract/bearer_bond/proof/burn.zk`: its first undetermined exposure is
    `.var "coin_spend_hook"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem bearer_bond_burn_has_a_free_instance :
    ¬ NoFreeInstance bearer_bond_burn_held bearer_bond_burn_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/bearer_bond/proof/prove_coverage.zk` — 4 exposure(s). -/

def bearer_bond_prove_coverage_held : List Name := ["coverage_ratio_bps", "reserve_amount", "total_interest_obligation", "total_outstanding"]


def bearer_bond_prove_coverage_stmts : List Stmt :=
[
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "BPS" (.op "witness_base" [.lit 10000]),
  .rangeCheck 64 (.var "reserve_amount"),
  .rangeCheck 64 (.var "total_outstanding"),
  .rangeCheck 64 (.var "total_interest_obligation"),
  .rangeCheck 64 (.var "coverage_ratio_bps"),
  .assign "total_obligation" (.op "base_add" [.var "total_outstanding", .var "total_interest_obligation"]),
  .assign "crb_times_obligation" (.op "base_mul" [.var "coverage_ratio_bps", .var "total_obligation"]),
  .assign "res_times_bps" (.op "base_mul" [.var "reserve_amount", .var "BPS"]),
  .assign "lte_check" (.op "less_than_or_equal" [.var "crb_times_obligation", .var "res_times_bps"]),
  .constrainEq (.var "lte_check") (.var "ONE"),
  .assign "crb_plus_one" (.op "base_add" [.var "coverage_ratio_bps", .var "ONE"]),
  .assign "upper" (.op "base_mul" [.var "crb_plus_one", .var "total_obligation"]),
  .constrainInstance (.var "reserve_amount"),
  .constrainInstance (.var "total_outstanding"),
  .constrainInstance (.var "total_interest_obligation"),
  .constrainInstance (.var "coverage_ratio_bps")
]


/-- **The property fails** for `src/contract/bearer_bond/proof/prove_coverage.zk`: its first undetermined exposure is
    `.var "reserve_amount"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem bearer_bond_prove_coverage_has_a_free_instance :
    ¬ NoFreeInstance bearer_bond_prove_coverage_held bearer_bond_prove_coverage_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/bearer_bond/proof/redeem.zk` — 8 exposure(s). -/

def bearer_bond_redeem_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "asset_id_blind", "coin_public", "coin_spend_hook", "commitment_blind", "tx_binding", "tx_commitment", "tx_nonce", "user_data", "value", "value_blind"]


def bearer_bond_redeem_stmts : List Stmt :=
[
  .assign "DOMAIN_TOK_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "coin_public", .var "value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .constrainInstance (.var "coin"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "token_commit" (.op "poseidon_hash" [.var "DOMAIN_TOK_COMMIT", .var "asset_id", .var "asset_id_blind"]),
  .constrainInstance (.var "token_commit"),
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .constrainEq (.var "value") (.var "ZERO"),
  .constrainInstance (.var "value"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "coin_spend_hook")
]


/-- **The property fails** for `src/contract/bearer_bond/proof/redeem.zk`: its first undetermined exposure is
    `.var "value"`, which the circuit does not bind before exposing.
    The checker resolves it as `bound`, through a `constrain_equal_base` whose determining side is a declared constant. -/
@[axiom_budget 0]
theorem bearer_bond_redeem_has_a_free_instance :
    ¬ NoFreeInstance bearer_bond_redeem_held bearer_bond_redeem_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/betting_stake/proof/claim.zk` — 6 exposure(s). -/

def betting_stake_claim_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "accumulated_earnings", "asset_id", "current_amount", "nonce", "staker_nullifier", "staker_pub_x", "staker_pub_y", "staker_secret", "table_id", "tx_binding", "tx_commitment", "tx_nonce", "value_blind"]


def betting_stake_claim_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "staker_pub" (.op "ec_mul_base" [.var "staker_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "staker_pub"]) (.var "staker_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "staker_pub"]) (.var "staker_pub_y"),
  .rangeCheck 64 (.var "current_amount"),
  .assign "derived_stake_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "table_id", .var "staker_pub_x", .var "staker_pub_y", .var "current_amount", .var "nonce"]),
  .constrainInstance (.var "derived_stake_id"),
  .assign "vcv" (.op "ec_mul_short" [.var "current_amount", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "derived_stake_id", .var "staker_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "staker_nullifier"),
  .constrainInstance (.var "staker_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/betting_stake/proof/claim.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem betting_stake_claim_has_a_free_instance :
    ¬ NoFreeInstance betting_stake_claim_held betting_stake_claim_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/betting_stake/proof/init.zk` — 3 exposure(s). -/

def betting_stake_init_held : List Name := ["betting_contract_id", "house_edge_bp", "nonce", "risk_profile", "tx_binding", "tx_commitment", "tx_nonce"]


def betting_stake_init_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_table_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "betting_contract_id", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_table_id")
]


/-- **The property fails** for `src/contract/betting_stake/proof/init.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem betting_stake_init_has_a_free_instance :
    ¬ NoFreeInstance betting_stake_init_held betting_stake_init_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/betting_stake/proof/stake.zk` — 6 exposure(s). -/

def betting_stake_stake_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "amount", "asset_id", "nonce", "staker_nullifier", "staker_pub_x", "staker_pub_y", "staker_secret", "table_id", "tx_binding", "tx_commitment", "tx_nonce", "value_blind"]


def betting_stake_stake_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "staker_pub" (.op "ec_mul_base" [.var "staker_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "staker_pub"]) (.var "staker_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "staker_pub"]) (.var "staker_pub_y"),
  .rangeCheck 64 (.var "amount"),
  .assign "derived_stake_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "table_id", .var "staker_pub_x", .var "staker_pub_y", .var "amount", .var "nonce"]),
  .constrainInstance (.var "derived_stake_id"),
  .assign "vcv" (.op "ec_mul_short" [.var "amount", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "derived_stake_id", .var "staker_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "staker_nullifier"),
  .constrainInstance (.var "staker_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/betting_stake/proof/stake.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem betting_stake_stake_has_a_free_instance :
    ¬ NoFreeInstance betting_stake_stake_held betting_stake_stake_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/betting_stake/proof/unstake.zk` — 6 exposure(s). -/

def betting_stake_unstake_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "accumulated_earnings", "asset_id", "current_amount", "nonce", "original_amount", "staker_nullifier", "staker_pub_x", "staker_pub_y", "staker_secret", "table_id", "tx_binding", "tx_commitment", "tx_nonce", "value_blind"]


def betting_stake_unstake_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "staker_pub" (.op "ec_mul_base" [.var "staker_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "staker_pub"]) (.var "staker_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "staker_pub"]) (.var "staker_pub_y"),
  .rangeCheck 64 (.var "original_amount"),
  .rangeCheck 64 (.var "current_amount"),
  .assign "derived_stake_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "table_id", .var "staker_pub_x", .var "staker_pub_y", .var "original_amount", .var "nonce"]),
  .constrainInstance (.var "derived_stake_id"),
  .assign "vcv" (.op "ec_mul_short" [.var "original_amount", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "derived_stake_id", .var "staker_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "staker_nullifier"),
  .constrainInstance (.var "staker_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/betting_stake/proof/unstake.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem betting_stake_unstake_has_a_free_instance :
    ¬ NoFreeInstance betting_stake_unstake_held betting_stake_unstake_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/betting_stake/proof/update_risk.zk` — 3 exposure(s). -/

def betting_stake_update_risk_held : List Name := ["accumulated_losses", "betting_contract_id", "house_edge_bp", "nonce", "risk_profile", "total_stake", "tx_binding", "tx_commitment", "tx_nonce"]


def betting_stake_update_risk_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .rangeCheck 64 (.var "total_stake"),
  .rangeCheck 64 (.var "accumulated_losses"),
  .rangeCheck 64 (.var "house_edge_bp"),
  .assign "derived_table_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "betting_contract_id", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_table_id")
]


/-- **The property fails** for `src/contract/betting_stake/proof/update_risk.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem betting_stake_update_risk_has_a_free_instance :
    ¬ NoFreeInstance betting_stake_update_risk_held betting_stake_update_risk_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/box/proof/put.zk` — 5 exposure(s). -/

def box_put_held : List Name := ["box_id", "expected_root", "leaf_pos", "new_contents_commit", "new_leaf", "new_state_nonce", "nullifier", "old_contents_commit", "old_state_nonce", "owner_secret", "path", "tx_binding", "tx_commitment", "tx_nonce"]


def box_put_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_MERKLE_LEAF" (.op "witness_base" [.lit 5]),
  .assign "nullifier_circuit" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "box_id", .var "old_state_nonce"]),
  .constrainEq (.var "nullifier_circuit") (.var "nullifier"),
  .constrainInstance (.var "nullifier"),
  .assign "old_leaf" (.op "poseidon_hash" [.var "DOMAIN_MERKLE_LEAF", .var "box_id", .var "old_contents_commit", .var "old_state_nonce"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "old_leaf"]),
  .constrainEq (.var "root") (.var "expected_root"),
  .constrainInstance (.var "expected_root"),
  .assign "new_leaf_circuit" (.op "poseidon_hash" [.var "DOMAIN_MERKLE_LEAF", .var "box_id", .var "new_contents_commit", .var "new_state_nonce"]),
  .constrainEq (.var "new_leaf_circuit") (.var "new_leaf"),
  .constrainInstance (.var "new_leaf"),
  .assign "tx_binding_circuit" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainEq (.var "tx_binding_circuit") (.var "tx_binding"),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/box/proof/put.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem box_put_has_a_free_instance :
    ¬ NoFreeInstance box_put_held box_put_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/box/proof/take.zk` — 4 exposure(s). -/

def box_take_held : List Name := ["box_id", "contents_commit", "expected_root", "leaf_pos", "nullifier", "owner_secret", "path", "state_nonce", "tx_binding", "tx_commitment", "tx_nonce"]


def box_take_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_MERKLE_LEAF" (.op "witness_base" [.lit 5]),
  .assign "nullifier_circuit" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "box_id", .var "state_nonce"]),
  .constrainEq (.var "nullifier_circuit") (.var "nullifier"),
  .constrainInstance (.var "nullifier"),
  .assign "box_leaf" (.op "poseidon_hash" [.var "DOMAIN_MERKLE_LEAF", .var "box_id", .var "contents_commit", .var "state_nonce"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "box_leaf"]),
  .constrainEq (.var "root") (.var "expected_root"),
  .constrainInstance (.var "expected_root"),
  .assign "tx_binding_circuit" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainEq (.var "tx_binding_circuit") (.var "tx_binding"),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/box/proof/take.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem box_take_has_a_free_instance :
    ¬ NoFreeInstance box_take_held box_take_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/bridge/proof/deposit.zk` — 3 exposure(s). -/

def bridge_deposit_held : List Name := ["NULLIFIER_K", "amount", "bridge_nonce", "recipient_pub_x", "recipient_pub_y", "secret", "tx_binding", "tx_commitment", "tx_nonce"]


def bridge_deposit_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "bridge_secret" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "recipient_pub_x", .var "recipient_pub_y", .var "bridge_nonce"]),
  .assign "bridge_pub" (.op "ec_mul_base" [.var "bridge_secret", .var "NULLIFIER_K"]),
  .assign "bridge_pub_x" (.op "ec_get_x" [.var "bridge_pub"]),
  .assign "bridge_pub_y" (.op "ec_get_y" [.var "bridge_pub"]),
  .assign "bridge_address" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "bridge_pub_x", .var "bridge_pub_y"]),
  .assign "derived_commitment" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "secret", .var "amount", .var "bridge_address"]),
  .constrainInstance (.var "derived_commitment"),
  .rangeCheck 64 (.var "amount"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/bridge/proof/deposit.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem bridge_deposit_has_a_free_instance :
    ¬ NoFreeInstance bridge_deposit_held bridge_deposit_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/bridge/proof/withdraw.zk` — 4 exposure(s). -/

def bridge_withdraw_held : List Name := ["NULLIFIER_K", "amount", "nullifier", "recipient_hash", "secret", "tx_binding", "tx_commitment", "tx_nonce"]


def bridge_withdraw_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "secret", .var "recipient_hash"]),
  .constrainInstance (.var "computed_nullifier"),
  .constrainEq (.var "computed_nullifier") (.var "nullifier"),
  .assign "derived_recipient" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "recipient_hash"]),
  .constrainInstance (.var "derived_recipient"),
  .rangeCheck 64 (.var "amount"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/bridge/proof/withdraw.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem bridge_withdraw_has_a_free_instance :
    ¬ NoFreeInstance bridge_withdraw_held bridge_withdraw_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dao_escrow/proof/init.zk` — 4 exposure(s). -/

def dao_escrow_init_held : List Name := ["NULLIFIER_K", "bulla_blind", "dao_bulla", "endowment_asset_id", "owner_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def dao_escrow_init_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "owner_pub" (.op "ec_mul_base" [.var "owner_secret", .var "NULLIFIER_K"]),
  .assign "owner_pub_x" (.op "ec_get_x" [.var "owner_pub"]),
  .assign "owner_pub_y" (.op "ec_get_y" [.var "owner_pub"]),
  .assign "endowment_bulla" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "dao_bulla", .var "owner_pub_x", .var "owner_pub_y", .var "endowment_asset_id", .var "bulla_blind"]),
  .constrainInstance (.var "dao_bulla"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "endowment_bulla")
]


/-- **The property fails** for `src/contract/dao_escrow/proof/init.zk`: its first undetermined exposure is
    `.var "dao_bulla"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dao_escrow_init_has_a_free_instance :
    ¬ NoFreeInstance dao_escrow_init_held dao_escrow_init_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dao_escrow/proof/pay_premium.zk` — 2 exposure(s). -/

def dao_escrow_pay_premium_held : List Name := ["NULLIFIER_K", "asset_id", "current_block", "dao_escrow_bulla", "expiry", "member_pub_x", "member_pub_y", "member_secret", "membership_blind", "mpc_secret_1", "mpc_secret_2", "mpc_secret_3", "tx_binding", "tx_commitment", "tx_nonce", "value"]


def dao_escrow_pay_premium_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .rangeCheck 64 (.var "current_block"),
  .rangeCheck 64 (.var "expiry"),
  .assign "member_pub" (.op "ec_mul_base" [.var "member_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "member_pub"]) (.var "member_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "member_pub"]) (.var "member_pub_y"),
  .assign "computed_bulla" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "member_pub_x", .var "member_pub_y", .var "mpc_secret_1", .var "mpc_secret_2", .var "mpc_secret_3"]),
  .constrainEq (.var "computed_bulla") (.var "dao_escrow_bulla"),
  .assign "membership_note" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "member_pub_x", .var "member_pub_y", .var "value", .var "asset_id", .var "expiry", .var "membership_blind"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dao_escrow/proof/pay_premium.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dao_escrow_pay_premium_has_a_free_instance :
    ¬ NoFreeInstance dao_escrow_pay_premium_held dao_escrow_pay_premium_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dao_escrow/proof/propose_claim.zk` — 3 exposure(s). -/

def dao_escrow_propose_claim_held : List Name := ["NULLIFIER_K", "capability_id", "capability_secret", "claim_amount", "claim_blind", "claim_id", "dao_escrow_bulla", "proposer_pub_x", "proposer_pub_y", "proposer_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def dao_escrow_propose_claim_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "proposer_pub" (.op "ec_mul_base" [.var "proposer_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "proposer_pub"]) (.var "proposer_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "proposer_pub"]) (.var "proposer_pub_y"),
  .assign "capability_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "capability_id", .var "capability_secret"]),
  .assign "proposal_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "capability_secret", .var "dao_escrow_bulla", .var "claim_id"]),
  .assign "claim_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "claim_id", .var "claim_amount", .var "claim_blind"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "claim_commit")
]


/-- **The property fails** for `src/contract/dao_escrow/proof/propose_claim.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dao_escrow_propose_claim_has_a_free_instance :
    ¬ NoFreeInstance dao_escrow_propose_claim_held dao_escrow_propose_claim_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dao_escrow/proof/resolve_dispute.zk` — 3 exposure(s). -/

def dao_escrow_resolve_dispute_held : List Name := ["NULLIFIER_K", "arbitrator_pub_x", "arbitrator_pub_y", "arbitrator_secret", "capability_id", "capability_secret", "dao_escrow_bulla", "dispute_id", "resolution_blind", "resolution_type", "tx_binding", "tx_commitment", "tx_nonce"]


def dao_escrow_resolve_dispute_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "arbitrator_pub" (.op "ec_mul_base" [.var "arbitrator_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "arbitrator_pub"]) (.var "arbitrator_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "arbitrator_pub"]) (.var "arbitrator_pub_y"),
  .assign "capability_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "capability_id", .var "capability_secret", .var "dao_escrow_bulla"]),
  .assign "dispute_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "capability_secret", .var "dispute_id"]),
  .assign "resolution_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "dispute_id", .var "resolution_type", .var "resolution_blind"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "resolution_commit")
]


/-- **The property fails** for `src/contract/dao_escrow/proof/resolve_dispute.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dao_escrow_resolve_dispute_has_a_free_instance :
    ¬ NoFreeInstance dao_escrow_resolve_dispute_held dao_escrow_resolve_dispute_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dao_escrow/proof/set_governance_config.zk` — 5 exposure(s). -/

def dao_escrow_set_governance_config_held : List Name := ["NULLIFIER_K", "dao_escrow_bulla", "owner_nullifier", "owner_pub_x", "owner_pub_y", "owner_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def dao_escrow_set_governance_config_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "owner_pub" (.op "ec_mul_base" [.var "owner_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "owner_pub"]) (.var "owner_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "owner_pub"]) (.var "owner_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_pub_x", .var "owner_pub_y", .var "owner_secret", .var "dao_escrow_bulla"]),
  .constrainEq (.var "computed") (.var "owner_nullifier"),
  .constrainInstance (.var "owner_pub_x"),
  .constrainInstance (.var "owner_pub_y"),
  .constrainInstance (.var "owner_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dao_escrow/proof/set_governance_config.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dao_escrow_set_governance_config_has_a_free_instance :
    ¬ NoFreeInstance dao_escrow_set_governance_config_held dao_escrow_set_governance_config_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dao_escrow/proof/verify_member_capability.zk` — 3 exposure(s). -/

def dao_escrow_verify_member_capability_held : List Name := ["NULLIFIER_K", "capability_id", "capability_secret", "dao_escrow_bulla", "holder_pub_x", "holder_pub_y", "holder_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def dao_escrow_verify_member_capability_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "holder_pub" (.op "ec_mul_base" [.var "holder_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "holder_pub"]) (.var "holder_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "holder_pub"]) (.var "holder_pub_y"),
  .assign "capability_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "capability_id", .var "capability_secret", .var "dao_escrow_bulla"]),
  .assign "holder_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "holder_pub_x", .var "holder_pub_y", .var "capability_secret"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "capability_commit")
]


/-- **The property fails** for `src/contract/dao_escrow/proof/verify_member_capability.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dao_escrow_verify_member_capability_has_a_free_instance :
    ¬ NoFreeInstance dao_escrow_verify_member_capability_held dao_escrow_verify_member_capability_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dao_escrow/proof/vote_claim.zk` — 3 exposure(s). -/

def dao_escrow_vote_claim_held : List Name := ["NULLIFIER_K", "capability_id", "capability_secret", "dao_escrow_bulla", "proposal_id", "tx_binding", "tx_commitment", "tx_nonce", "vote_blind", "vote_type", "voter_pub_x", "voter_pub_y", "voter_secret"]


def dao_escrow_vote_claim_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "voter_pub" (.op "ec_mul_base" [.var "voter_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "voter_pub"]) (.var "voter_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "voter_pub"]) (.var "voter_pub_y"),
  .assign "capability_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "capability_id", .var "capability_secret"]),
  .assign "vote_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "capability_secret", .var "proposal_id", .var "voter_pub_x", .var "voter_pub_y"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "vote_nullifier")
]


/-- **The property fails** for `src/contract/dao_escrow/proof/vote_claim.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dao_escrow_vote_claim_has_a_free_instance :
    ¬ NoFreeInstance dao_escrow_vote_claim_held dao_escrow_vote_claim_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/add_liquidity.zk` — 4 exposure(s). -/

def darkbet_exchange_add_liquidity_held : List Name := ["NULLIFIER_K", "amount", "block_height", "market_id", "provider_pub_x", "provider_pub_y", "provider_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def darkbet_exchange_add_liquidity_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_lp_share_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "market_id", .var "provider_pub_x", .var "provider_pub_y", .var "amount", .var "block_height"]),
  .constrainInstance (.var "derived_lp_share_id"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "derived_lp_share_id", .var "provider_secret"]),
  .constrainInstance (.var "computed_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/add_liquidity.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_add_liquidity_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_add_liquidity_held darkbet_exchange_add_liquidity_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/buy_position.zk` — 4 exposure(s). -/

def darkbet_exchange_buy_position_held : List Name := ["NULLIFIER_K", "amount", "block_height", "market_id", "outcome", "owner_pub_x", "owner_pub_y", "owner_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def darkbet_exchange_buy_position_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_position_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "market_id", .var "owner_pub_x", .var "owner_pub_y", .var "outcome", .var "amount", .var "block_height"]),
  .constrainInstance (.var "derived_position_id"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "derived_position_id", .var "owner_secret"]),
  .constrainInstance (.var "computed_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/buy_position.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_buy_position_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_buy_position_held darkbet_exchange_buy_position_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/cancel_order.zk` — 5 exposure(s). -/

def darkbet_exchange_cancel_order_held : List Name := ["NULLIFIER_K", "order_id", "tx_binding", "tx_commitment", "tx_nonce", "user_nullifier", "user_pub_x", "user_pub_y", "user_secret"]


def darkbet_exchange_cancel_order_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "user_pub" (.op "ec_mul_base" [.var "user_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "user_pub"]) (.var "user_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "user_pub"]) (.var "user_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "order_id", .var "user_secret"]),
  .constrainEq (.var "computed") (.var "user_nullifier"),
  .constrainInstance (.var "user_pub_x"),
  .constrainInstance (.var "user_pub_y"),
  .constrainInstance (.var "user_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/cancel_order.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_cancel_order_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_cancel_order_held darkbet_exchange_cancel_order_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/claim_winnings.zk` — 3 exposure(s). -/

def darkbet_exchange_claim_winnings_held : List Name := ["NULLIFIER_K", "amount", "block_height", "market_id", "outcome", "owner_pub_x", "owner_pub_y", "owner_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def darkbet_exchange_claim_winnings_stmts : List Stmt :=
[
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "owner_pub" (.op "ec_mul_base" [.var "owner_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "owner_pub"]) (.var "owner_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "owner_pub"]) (.var "owner_pub_y"),
  .assign "derived_claim_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "market_id", .var "owner_pub_x", .var "owner_pub_y", .var "outcome", .var "amount"]),
  .constrainInstance (.var "derived_claim_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/claim_winnings.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_claim_winnings_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_claim_winnings_held darkbet_exchange_claim_winnings_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/create_market.zk` — 4 exposure(s). -/

def darkbet_exchange_create_market_held : List Name := ["NULLIFIER_K", "block_height", "close_block", "creator_pub_x", "creator_pub_y", "creator_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def darkbet_exchange_create_market_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_market_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "creator_pub_x", .var "creator_pub_y", .var "close_block", .var "block_height"]),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "derived_market_id", .var "creator_secret"]),
  .constrainInstance (.var "derived_market_id"),
  .constrainInstance (.var "computed_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/create_market.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_create_market_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_create_market_held darkbet_exchange_create_market_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/match_orders.zk` — 5 exposure(s). -/

def darkbet_exchange_match_orders_held : List Name := ["NULLIFIER_K", "market_id", "tx_binding", "tx_commitment", "tx_nonce", "user_nullifier", "user_pub_x", "user_pub_y", "user_secret"]


def darkbet_exchange_match_orders_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "user_pub" (.op "ec_mul_base" [.var "user_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "user_pub"]) (.var "user_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "user_pub"]) (.var "user_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "market_id", .var "user_secret"]),
  .constrainEq (.var "computed") (.var "user_nullifier"),
  .constrainInstance (.var "user_pub_x"),
  .constrainInstance (.var "user_pub_y"),
  .constrainInstance (.var "user_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/match_orders.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_match_orders_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_match_orders_held darkbet_exchange_match_orders_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/place_back.zk` — 5 exposure(s). -/

def darkbet_exchange_place_back_held : List Name := ["NULLIFIER_K", "market_id", "tx_binding", "tx_commitment", "tx_nonce", "user_nullifier", "user_pub_x", "user_pub_y", "user_secret"]


def darkbet_exchange_place_back_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "user_pub" (.op "ec_mul_base" [.var "user_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "user_pub"]) (.var "user_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "user_pub"]) (.var "user_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "market_id", .var "user_secret"]),
  .constrainEq (.var "computed") (.var "user_nullifier"),
  .constrainInstance (.var "user_pub_x"),
  .constrainInstance (.var "user_pub_y"),
  .constrainInstance (.var "user_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/place_back.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_place_back_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_place_back_held darkbet_exchange_place_back_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/place_lay.zk` — 5 exposure(s). -/

def darkbet_exchange_place_lay_held : List Name := ["NULLIFIER_K", "market_id", "tx_binding", "tx_commitment", "tx_nonce", "user_nullifier", "user_pub_x", "user_pub_y", "user_secret"]


def darkbet_exchange_place_lay_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "user_pub" (.op "ec_mul_base" [.var "user_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "user_pub"]) (.var "user_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "user_pub"]) (.var "user_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "market_id", .var "user_secret"]),
  .constrainEq (.var "computed") (.var "user_nullifier"),
  .constrainInstance (.var "user_pub_x"),
  .constrainInstance (.var "user_pub_y"),
  .constrainInstance (.var "user_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/place_lay.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_place_lay_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_place_lay_held darkbet_exchange_place_lay_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/remove_liquidity.zk` — 5 exposure(s). -/

def darkbet_exchange_remove_liquidity_held : List Name := ["NULLIFIER_K", "market_id", "provider_nullifier", "provider_pub_x", "provider_pub_y", "provider_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def darkbet_exchange_remove_liquidity_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "provider_pub" (.op "ec_mul_base" [.var "provider_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "provider_pub"]) (.var "provider_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "provider_pub"]) (.var "provider_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "market_id", .var "provider_secret"]),
  .constrainEq (.var "computed") (.var "provider_nullifier"),
  .constrainInstance (.var "provider_pub_x"),
  .constrainInstance (.var "provider_pub_y"),
  .constrainInstance (.var "provider_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/remove_liquidity.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_remove_liquidity_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_remove_liquidity_held darkbet_exchange_remove_liquidity_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darkbet_exchange/proof/resolve_market.zk` — 5 exposure(s). -/

def darkbet_exchange_resolve_market_held : List Name := ["NULLIFIER_K", "market_id", "oracle_nullifier", "oracle_pub_x", "oracle_pub_y", "oracle_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def darkbet_exchange_resolve_market_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "oracle_pub" (.op "ec_mul_base" [.var "oracle_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "oracle_pub"]) (.var "oracle_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "oracle_pub"]) (.var "oracle_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "market_id", .var "oracle_secret"]),
  .constrainEq (.var "computed") (.var "oracle_nullifier"),
  .constrainInstance (.var "oracle_pub_x"),
  .constrainInstance (.var "oracle_pub_y"),
  .constrainInstance (.var "oracle_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darkbet_exchange/proof/resolve_market.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darkbet_exchange_resolve_market_has_a_free_instance :
    ¬ NoFreeInstance darkbet_exchange_resolve_market_held darkbet_exchange_resolve_market_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darktoshi_dice/proof/commit_bet.zk` — 5 exposure(s). -/

def darktoshi_dice_commit_bet_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "bet_value", "blind", "player_pub_x", "player_pub_y", "secret_nonce", "target", "tx_binding", "tx_commitment", "tx_nonce", "value_blind"]


def darktoshi_dice_commit_bet_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "bet_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "player_pub_x", .var "player_pub_y", .var "bet_value", .var "target", .var "secret_nonce", .var "blind", .var "asset_id"]),
  .constrainInstance (.var "bet_id"),
  .assign "vcv" (.op "ec_mul_short" [.var "bet_value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darktoshi_dice/proof/commit_bet.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darktoshi_dice_commit_bet_has_a_free_instance :
    ¬ NoFreeInstance darktoshi_dice_commit_bet_held darktoshi_dice_commit_bet_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darktoshi_dice/proof/house_close.zk` — 6 exposure(s). -/

def darktoshi_dice_house_close_held : List Name := ["NULLIFIER_K", "bet_id", "close_nullifier", "house_pub_x", "house_pub_y", "house_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def darktoshi_dice_house_close_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "house_pub" (.op "ec_mul_base" [.var "house_secret", .var "NULLIFIER_K"]),
  .assign "house_pub_x_computed" (.op "ec_get_x" [.var "house_pub"]),
  .assign "house_pub_y_computed" (.op "ec_get_y" [.var "house_pub"]),
  .constrainEq (.var "house_pub_x_computed") (.var "house_pub_x"),
  .constrainEq (.var "house_pub_y_computed") (.var "house_pub_y"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "bet_id", .var "house_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "close_nullifier"),
  .constrainInstance (.var "bet_id"),
  .constrainInstance (.var "house_pub_x"),
  .constrainInstance (.var "house_pub_y"),
  .constrainInstance (.var "close_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darktoshi_dice/proof/house_close.zk`: its first undetermined exposure is
    `.var "bet_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darktoshi_dice_house_close_has_a_free_instance :
    ¬ NoFreeInstance darktoshi_dice_house_close_held darktoshi_dice_house_close_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darktoshi_dice/proof/reveal_roll.zk` — 4 exposure(s). -/

def darktoshi_dice_reveal_roll_held : List Name := ["bet_id", "secret_nonce", "secret_nonce_commit", "tx_binding", "tx_commitment", "tx_nonce"]


def darktoshi_dice_reveal_roll_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "computed_commit" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "secret_nonce"]),
  .constrainEq (.var "computed_commit") (.var "secret_nonce_commit"),
  .constrainInstance (.var "bet_id"),
  .constrainInstance (.var "secret_nonce_commit"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/darktoshi_dice/proof/reveal_roll.zk`: its first undetermined exposure is
    `.var "bet_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem darktoshi_dice_reveal_roll_has_a_free_instance :
    ¬ NoFreeInstance darktoshi_dice_reveal_roll_held darktoshi_dice_reveal_roll_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/darktoshi_dice/proof/settle_bet.zk` — 4 exposure(s). -/

def darktoshi_dice_settle_bet_held : List Name := ["asset_id", "bet_value", "blind", "block_hash", "player_pub_x", "player_pub_y", "secret_nonce", "target", "tx_binding", "tx_commitment", "tx_nonce"]


def darktoshi_dice_settle_bet_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_bet_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "player_pub_x", .var "player_pub_y", .var "bet_value", .var "target", .var "secret_nonce", .var "blind", .var "asset_id"]),
  .constrainInstance (.var "derived_bet_id"),
  .assign "roll_hash" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "block_hash", .var "derived_bet_id", .var "secret_nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "roll_hash")
]


/-- **The property fails** for `src/contract/darktoshi_dice/proof/settle_bet.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem darktoshi_dice_settle_bet_has_a_free_instance :
    ¬ NoFreeInstance darktoshi_dice_settle_bet_held darktoshi_dice_settle_bet_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dex/proof/accept_swap.zk` — 6 exposure(s). -/

def dex_accept_swap_held : List Name := ["NULLIFIER_K", "acceptor_secret", "blind", "offer_amount", "offer_token", "proposer_lock_commitment", "signature_public_x", "signature_public_y", "signature_secret", "swap_id", "tx_binding", "tx_commitment", "tx_nonce"]


def dex_accept_swap_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_CAP_COMMIT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "SPEND_HOOK" (.op "witness_base" [.lit 0]),
  .assign "USER_DATA" (.op "witness_base" [.lit 0]),
  .assign "acceptor_public_key" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "acceptor_secret"]),
  .assign "computed_lock" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "acceptor_public_key", .var "offer_token", .var "offer_amount", .var "SPEND_HOOK", .var "USER_DATA", .var "blind"]),
  .constrainInstance (.var "computed_lock"),
  .assign "acceptor_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "acceptor_secret", .var "computed_lock"]),
  .constrainInstance (.var "acceptor_nullifier"),
  .assign "signature_public" (.op "ec_mul_base" [.var "signature_secret", .var "NULLIFIER_K"]),
  .assign "derived_sig_pub_x" (.op "ec_get_x" [.var "signature_public"]),
  .assign "derived_sig_pub_y" (.op "ec_get_y" [.var "signature_public"]),
  .constrainEq (.var "derived_sig_pub_x") (.var "signature_public_x"),
  .constrainEq (.var "derived_sig_pub_y") (.var "signature_public_y"),
  .constrainInstance (.var "signature_public_x"),
  .constrainInstance (.var "signature_public_y"),
  .rangeCheck 64 (.var "offer_amount"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dex/proof/accept_swap.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dex_accept_swap_has_a_free_instance :
    ¬ NoFreeInstance dex_accept_swap_held dex_accept_swap_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dex/proof/cancel_swap.zk` — 4 exposure(s). -/

def dex_cancel_swap_held : List Name := ["NULLIFIER_K", "amount", "blind", "lock_commitment", "request_amount", "request_token", "secret", "token", "tx_binding", "tx_commitment", "tx_nonce"]


def dex_cancel_swap_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_CAP_COMMIT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "SPEND_HOOK" (.op "witness_base" [.lit 0]),
  .assign "USER_DATA" (.op "witness_base" [.lit 0]),
  .assign "public_key" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "secret"]),
  .assign "derived_lock" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "public_key", .var "token", .var "amount", .var "SPEND_HOOK", .var "USER_DATA", .var "blind"]),
  .constrainEq (.var "derived_lock") (.var "lock_commitment"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "secret", .var "lock_commitment"]),
  .constrainInstance (.var "computed_nullifier"),
  .assign "computed_swap_id" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "lock_commitment", .var "request_token", .var "request_amount"]),
  .constrainInstance (.var "computed_swap_id"),
  .rangeCheck 64 (.var "amount"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dex/proof/cancel_swap.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dex_cancel_swap_has_a_free_instance :
    ¬ NoFreeInstance dex_cancel_swap_held dex_cancel_swap_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dex/proof/create_swap.zk` — 7 exposure(s). -/

def dex_create_swap_held : List Name := ["NULLIFIER_K", "blind", "offer_amount", "offer_token", "request_amount", "request_token", "secret", "signature_public_x", "signature_public_y", "signature_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def dex_create_swap_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_CAP_COMMIT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "SPEND_HOOK" (.op "witness_base" [.lit 0]),
  .assign "USER_DATA" (.op "witness_base" [.lit 0]),
  .assign "public_key" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "secret"]),
  .assign "computed_lock" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "public_key", .var "offer_token", .var "offer_amount", .var "SPEND_HOOK", .var "USER_DATA", .var "blind"]),
  .constrainInstance (.var "computed_lock"),
  .assign "computed_swap_id" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "computed_lock", .var "request_token", .var "request_amount"]),
  .constrainInstance (.var "computed_swap_id"),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "secret", .var "computed_lock"]),
  .constrainInstance (.var "nullifier"),
  .assign "signature_public" (.op "ec_mul_base" [.var "signature_secret", .var "NULLIFIER_K"]),
  .assign "derived_sig_pub_x" (.op "ec_get_x" [.var "signature_public"]),
  .assign "derived_sig_pub_y" (.op "ec_get_y" [.var "signature_public"]),
  .constrainEq (.var "derived_sig_pub_x") (.var "signature_public_x"),
  .constrainEq (.var "derived_sig_pub_y") (.var "signature_public_y"),
  .constrainInstance (.var "signature_public_x"),
  .constrainInstance (.var "signature_public_y"),
  .rangeCheck 64 (.var "offer_amount"),
  .rangeCheck 64 (.var "request_amount"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dex/proof/create_swap.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dex_create_swap_has_a_free_instance :
    ¬ NoFreeInstance dex_create_swap_held dex_create_swap_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dex/proof/execute_swap.zk` — 7 exposure(s). -/

def dex_execute_swap_held : List Name := ["NULLIFIER_K", "alice_amount", "alice_blind", "alice_lock", "alice_otc_func_id", "alice_secret", "alice_token", "bob_amount", "bob_blind", "bob_lock", "bob_otc_func_id", "bob_secret", "bob_token", "fill_amount", "tx_binding", "tx_commitment", "tx_nonce"]


def dex_execute_swap_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_CAP_COMMIT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "SPEND_HOOK" (.op "witness_base" [.lit 0]),
  .assign "USER_DATA" (.op "witness_base" [.lit 0]),
  .assign "alice_public_key" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "alice_secret"]),
  .assign "alice_lock_check" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "alice_public_key", .var "alice_token", .var "alice_amount", .var "SPEND_HOOK", .var "USER_DATA", .var "alice_blind"]),
  .constrainEq (.var "alice_lock_check") (.var "alice_lock"),
  .assign "bob_public_key" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "bob_secret"]),
  .assign "bob_lock_check" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "bob_public_key", .var "bob_token", .var "bob_amount", .var "SPEND_HOOK", .var "USER_DATA", .var "bob_blind"]),
  .constrainEq (.var "bob_lock_check") (.var "bob_lock"),
  .assign "alice_nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "alice_secret", .var "alice_lock"]),
  .assign "bob_nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "bob_secret", .var "bob_lock"]),
  .constrainInstance (.var "alice_nullifier_check"),
  .constrainInstance (.var "bob_nullifier_check"),
  .constrainInstance (.var "alice_otc_func_id"),
  .constrainInstance (.var "bob_otc_func_id"),
  .assign "computed_swap_id" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "alice_lock", .var "bob_token", .var "bob_amount"]),
  .constrainInstance (.var "computed_swap_id"),
  .rangeCheck 64 (.var "alice_amount"),
  .rangeCheck 64 (.var "bob_amount"),
  .rangeCheck 64 (.var "fill_amount"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "is_lte" (.op "less_than_or_equal" [.var "fill_amount", .var "alice_amount"]),
  .constrainEq (.var "is_lte") (.var "ONE"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dex/proof/execute_swap.zk`: its first undetermined exposure is
    `.var "alice_otc_func_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem dex_execute_swap_has_a_free_instance :
    ¬ NoFreeInstance dex_execute_swap_held dex_execute_swap_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dex/proof/execute_swap_fee.zk` — 6 exposure(s). -/

def dex_execute_swap_fee_held : List Name := ["NULLIFIER_K", "alice_amount", "alice_blind", "alice_lock", "alice_secret", "alice_token", "bob_amount", "bob_blind", "bob_lock", "bob_secret", "bob_token", "fee", "fee_bps", "fill_amount", "tx_binding", "tx_commitment", "tx_nonce"]


def dex_execute_swap_fee_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_CAP_COMMIT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "SPEND_HOOK" (.op "witness_base" [.lit 0]),
  .assign "USER_DATA" (.op "witness_base" [.lit 0]),
  .assign "alice_public_key" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "alice_secret"]),
  .assign "alice_lock_check" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "alice_public_key", .var "alice_token", .var "alice_amount", .var "SPEND_HOOK", .var "USER_DATA", .var "alice_blind"]),
  .constrainEq (.var "alice_lock_check") (.var "alice_lock"),
  .assign "bob_public_key" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "bob_secret"]),
  .assign "bob_lock_check" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "bob_public_key", .var "bob_token", .var "bob_amount", .var "SPEND_HOOK", .var "USER_DATA", .var "bob_blind"]),
  .constrainEq (.var "bob_lock_check") (.var "bob_lock"),
  .assign "alice_nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "alice_secret", .var "alice_lock"]),
  .assign "bob_nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "bob_secret", .var "bob_lock"]),
  .constrainInstance (.var "alice_nullifier_check"),
  .constrainInstance (.var "bob_nullifier_check"),
  .assign "computed_swap_id" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "alice_lock", .var "bob_token", .var "bob_amount"]),
  .constrainInstance (.var "computed_swap_id"),
  .rangeCheck 64 (.var "alice_amount"),
  .rangeCheck 64 (.var "bob_amount"),
  .rangeCheck 64 (.var "fill_amount"),
  .rangeCheck 64 (.var "fee_bps"),
  .rangeCheck 64 (.var "fee"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "BPS" (.op "witness_base" [.lit 10000]),
  .assign "is_lte" (.op "less_than_or_equal" [.var "fill_amount", .var "alice_amount"]),
  .constrainEq (.var "is_lte") (.var "ONE"),
  .assign "fee_ok" (.op "less_than_or_equal" [.var "fee_bps", .var "BPS"]),
  .constrainEq (.var "fee_ok") (.var "ONE"),
  .assign "fill_times_fee_bps" (.op "base_mul" [.var "fill_amount", .var "fee_bps"]),
  .assign "fee_times_bps" (.op "base_mul" [.var "fee", .var "BPS"]),
  .assign "lte_check" (.op "less_than_or_equal" [.var "fee_times_bps", .var "fill_times_fee_bps"]),
  .constrainEq (.var "lte_check") (.var "ONE"),
  .assign "fee_plus_one" (.op "base_add" [.var "fee", .var "ONE"]),
  .assign "fee_upper" (.op "base_mul" [.var "fee_plus_one", .var "BPS"]),
  .constrainInstance (.var "fee"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dex/proof/execute_swap_fee.zk`: its first undetermined exposure is
    `.var "fee"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem dex_execute_swap_fee_has_a_free_instance :
    ¬ NoFreeInstance dex_execute_swap_fee_held dex_execute_swap_fee_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dex/proof/execute_swap_slippage.zk` — 5 exposure(s). -/

def dex_execute_swap_slippage_held : List Name := ["NULLIFIER_K", "alice_amount", "alice_blind", "alice_lock", "alice_secret", "alice_token", "bob_amount", "bob_blind", "bob_lock", "bob_secret", "bob_token", "fill_amount", "slippage_bps", "tx_binding", "tx_commitment", "tx_nonce"]


def dex_execute_swap_slippage_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_CAP_COMMIT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "SPEND_HOOK" (.op "witness_base" [.lit 0]),
  .assign "USER_DATA" (.op "witness_base" [.lit 0]),
  .assign "alice_public_key" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "alice_secret"]),
  .assign "alice_lock_check" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "alice_public_key", .var "alice_token", .var "alice_amount", .var "SPEND_HOOK", .var "USER_DATA", .var "alice_blind"]),
  .constrainEq (.var "alice_lock_check") (.var "alice_lock"),
  .assign "bob_public_key" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "bob_secret"]),
  .assign "bob_lock_check" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "bob_public_key", .var "bob_token", .var "bob_amount", .var "SPEND_HOOK", .var "USER_DATA", .var "bob_blind"]),
  .constrainEq (.var "bob_lock_check") (.var "bob_lock"),
  .assign "alice_nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "alice_secret", .var "alice_lock"]),
  .assign "bob_nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "bob_secret", .var "bob_lock"]),
  .constrainInstance (.var "alice_nullifier_check"),
  .constrainInstance (.var "bob_nullifier_check"),
  .assign "computed_swap_id" (.op "poseidon_hash" [.var "DOMAIN_CAP_COMMIT", .var "alice_lock", .var "bob_token", .var "bob_amount"]),
  .constrainInstance (.var "computed_swap_id"),
  .rangeCheck 64 (.var "alice_amount"),
  .rangeCheck 64 (.var "bob_amount"),
  .rangeCheck 64 (.var "fill_amount"),
  .rangeCheck 64 (.var "slippage_bps"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "is_lte" (.op "less_than_or_equal" [.var "fill_amount", .var "alice_amount"]),
  .constrainEq (.var "is_lte") (.var "ONE"),
  .assign "BPS" (.op "witness_base" [.lit 10000]),
  .assign "slippage_ok" (.op "less_than_or_equal" [.var "slippage_bps", .var "BPS"]),
  .constrainEq (.var "slippage_ok") (.var "ONE"),
  .assign "slippage_sub" (.op "base_sub" [.var "BPS", .var "slippage_bps"]),
  .assign "left" (.op "base_mul" [.var "fill_amount", .var "BPS"]),
  .assign "right" (.op "base_mul" [.var "slippage_sub", .var "alice_amount"]),
  .assign "is_satisfied" (.op "less_than_or_equal" [.var "right", .var "left"]),
  .constrainEq (.var "is_satisfied") (.var "ONE"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dex/proof/execute_swap_slippage.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dex_execute_swap_slippage_has_a_free_instance :
    ¬ NoFreeInstance dex_execute_swap_slippage_held dex_execute_swap_slippage_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dex/proof/set_transparency_level.zk` — 5 exposure(s). -/

def dex_set_transparency_level_held : List Name := ["NULLIFIER_K", "gov_nullifier", "gov_pub_x", "gov_pub_y", "gov_secret", "pair_id", "tx_binding", "tx_commitment", "tx_nonce"]


def dex_set_transparency_level_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "gov_pub" (.op "ec_mul_base" [.var "gov_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "gov_pub"]) (.var "gov_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "gov_pub"]) (.var "gov_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "gov_pub_x", .var "gov_pub_y", .var "gov_secret", .var "pair_id"]),
  .constrainEq (.var "computed") (.var "gov_nullifier"),
  .constrainInstance (.var "gov_pub_x"),
  .constrainInstance (.var "gov_pub_y"),
  .constrainInstance (.var "gov_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dex/proof/set_transparency_level.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dex_set_transparency_level_has_a_free_instance :
    ¬ NoFreeInstance dex_set_transparency_level_held dex_set_transparency_level_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/dex/proof/update_config.zk` — 5 exposure(s). -/

def dex_update_config_held : List Name := ["NULLIFIER_K", "config_nullifier", "gov_pub_x", "gov_pub_y", "gov_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def dex_update_config_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "gov_pub" (.op "ec_mul_base" [.var "gov_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "gov_pub"]) (.var "gov_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "gov_pub"]) (.var "gov_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "gov_pub_x", .var "gov_pub_y", .var "gov_secret"]),
  .constrainEq (.var "computed") (.var "config_nullifier"),
  .constrainInstance (.var "gov_pub_x"),
  .constrainInstance (.var "gov_pub_y"),
  .constrainInstance (.var "config_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/dex/proof/update_config.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem dex_update_config_has_a_free_instance :
    ¬ NoFreeInstance dex_update_config_held dex_update_config_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/drain_protection/proof/execute.zk` — 5 exposure(s). -/

def drain_protection_execute_held : List Name := ["NULLIFIER_K", "authority_nullifier", "authority_pub_x", "authority_pub_y", "authority_secret", "fund_id", "tx_binding", "tx_commitment", "tx_nonce"]


def drain_protection_execute_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "authority_pub" (.op "ec_mul_base" [.var "authority_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "authority_pub"]) (.var "authority_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "authority_pub"]) (.var "authority_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "fund_id", .var "authority_secret"]),
  .constrainEq (.var "computed") (.var "authority_nullifier"),
  .constrainInstance (.var "authority_pub_x"),
  .constrainInstance (.var "authority_pub_y"),
  .constrainInstance (.var "authority_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/drain_protection/proof/execute.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem drain_protection_execute_has_a_free_instance :
    ¬ NoFreeInstance drain_protection_execute_held drain_protection_execute_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/drain_protection/proof/exit.zk` — 2 exposure(s). -/

def drain_protection_exit_held : List Name := ["DIVISOR", "EXIT_MULTIPLIER", "NULLIFIER_K", "contribution_amount", "contribution_weight", "current_block", "dao_escrow_bulla", "dao_escrow_merkle_root", "dao_leaf_pos", "dao_membership_note", "dao_path", "deposited_at", "fund_id", "member_pub_x", "member_pub_y", "member_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def drain_protection_exit_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/drain_protection/proof/exit.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem drain_protection_exit_has_a_free_instance :
    ¬ NoFreeInstance drain_protection_exit_held drain_protection_exit_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/drain_protection/proof/initialize.zk` — 5 exposure(s). -/

def drain_protection_initialize_held : List Name := ["NULLIFIER_K", "authority_nullifier", "authority_pub_x", "authority_pub_y", "authority_secret", "fund_id", "tx_binding", "tx_commitment", "tx_nonce"]


def drain_protection_initialize_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "authority_pub" (.op "ec_mul_base" [.var "authority_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "authority_pub"]) (.var "authority_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "authority_pub"]) (.var "authority_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "fund_id", .var "authority_secret"]),
  .constrainEq (.var "computed") (.var "authority_nullifier"),
  .constrainInstance (.var "authority_pub_x"),
  .constrainInstance (.var "authority_pub_y"),
  .constrainInstance (.var "authority_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/drain_protection/proof/initialize.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem drain_protection_initialize_has_a_free_instance :
    ¬ NoFreeInstance drain_protection_initialize_held drain_protection_initialize_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/drain_protection/proof/lock.zk` — 5 exposure(s). -/

def drain_protection_lock_held : List Name := ["NULLIFIER_K", "authority_nullifier", "authority_pub_x", "authority_pub_y", "authority_secret", "fund_id", "tx_binding", "tx_commitment", "tx_nonce"]


def drain_protection_lock_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "authority_pub" (.op "ec_mul_base" [.var "authority_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "authority_pub"]) (.var "authority_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "authority_pub"]) (.var "authority_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "fund_id", .var "authority_secret"]),
  .constrainEq (.var "computed") (.var "authority_nullifier"),
  .constrainInstance (.var "authority_pub_x"),
  .constrainInstance (.var "authority_pub_y"),
  .constrainInstance (.var "authority_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/drain_protection/proof/lock.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem drain_protection_lock_has_a_free_instance :
    ¬ NoFreeInstance drain_protection_lock_held drain_protection_lock_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/drain_protection/proof/propose.zk` — 5 exposure(s). -/

def drain_protection_propose_held : List Name := ["NULLIFIER_K", "authority_nullifier", "authority_pub_x", "authority_pub_y", "authority_secret", "fund_id", "tx_binding", "tx_commitment", "tx_nonce"]


def drain_protection_propose_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "authority_pub" (.op "ec_mul_base" [.var "authority_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "authority_pub"]) (.var "authority_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "authority_pub"]) (.var "authority_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "fund_id", .var "authority_secret"]),
  .constrainEq (.var "computed") (.var "authority_nullifier"),
  .constrainInstance (.var "authority_pub_x"),
  .constrainInstance (.var "authority_pub_y"),
  .constrainInstance (.var "authority_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/drain_protection/proof/propose.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem drain_protection_propose_has_a_free_instance :
    ¬ NoFreeInstance drain_protection_propose_held drain_protection_propose_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/drain_protection/proof/transfer.zk` — 5 exposure(s). -/

def drain_protection_transfer_held : List Name := ["NULLIFIER_K", "authority_nullifier", "authority_pub_x", "authority_pub_y", "authority_secret", "fund_id", "tx_binding", "tx_commitment", "tx_nonce"]


def drain_protection_transfer_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "authority_pub" (.op "ec_mul_base" [.var "authority_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "authority_pub"]) (.var "authority_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "authority_pub"]) (.var "authority_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "fund_id", .var "authority_secret"]),
  .constrainEq (.var "computed") (.var "authority_nullifier"),
  .constrainInstance (.var "authority_pub_x"),
  .constrainInstance (.var "authority_pub_y"),
  .constrainInstance (.var "authority_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/drain_protection/proof/transfer.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem drain_protection_transfer_has_a_free_instance :
    ¬ NoFreeInstance drain_protection_transfer_held drain_protection_transfer_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/drain_protection/proof/unlock.zk` — 5 exposure(s). -/

def drain_protection_unlock_held : List Name := ["NULLIFIER_K", "authority_nullifier", "authority_pub_x", "authority_pub_y", "authority_secret", "fund_id", "tx_binding", "tx_commitment", "tx_nonce"]


def drain_protection_unlock_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "authority_pub" (.op "ec_mul_base" [.var "authority_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "authority_pub"]) (.var "authority_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "authority_pub"]) (.var "authority_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "fund_id", .var "authority_secret"]),
  .constrainEq (.var "computed") (.var "authority_nullifier"),
  .constrainInstance (.var "authority_pub_x"),
  .constrainInstance (.var "authority_pub_y"),
  .constrainInstance (.var "authority_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/drain_protection/proof/unlock.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem drain_protection_unlock_has_a_free_instance :
    ¬ NoFreeInstance drain_protection_unlock_held drain_protection_unlock_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/drain_protection/proof/update_config.zk` — 5 exposure(s). -/

def drain_protection_update_config_held : List Name := ["NULLIFIER_K", "authority_nullifier", "authority_pub_x", "authority_pub_y", "authority_secret", "fund_id", "tx_binding", "tx_commitment", "tx_nonce"]


def drain_protection_update_config_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "authority_pub" (.op "ec_mul_base" [.var "authority_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "authority_pub"]) (.var "authority_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "authority_pub"]) (.var "authority_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "fund_id", .var "authority_secret"]),
  .constrainEq (.var "computed") (.var "authority_nullifier"),
  .constrainInstance (.var "authority_pub_x"),
  .constrainInstance (.var "authority_pub_y"),
  .constrainInstance (.var "authority_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/drain_protection/proof/update_config.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem drain_protection_update_config_has_a_free_instance :
    ¬ NoFreeInstance drain_protection_update_config_held drain_protection_update_config_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/drain_protection/proof/vote.zk` — 5 exposure(s). -/

def drain_protection_vote_held : List Name := ["NULLIFIER_K", "authority_nullifier", "authority_pub_x", "authority_pub_y", "authority_secret", "fund_id", "tx_binding", "tx_commitment", "tx_nonce"]


def drain_protection_vote_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "authority_pub" (.op "ec_mul_base" [.var "authority_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "authority_pub"]) (.var "authority_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "authority_pub"]) (.var "authority_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "fund_id", .var "authority_secret"]),
  .constrainEq (.var "computed") (.var "authority_nullifier"),
  .constrainInstance (.var "authority_pub_x"),
  .constrainInstance (.var "authority_pub_y"),
  .constrainInstance (.var "authority_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/drain_protection/proof/vote.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem drain_protection_vote_has_a_free_instance :
    ¬ NoFreeInstance drain_protection_vote_held drain_protection_vote_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/escrow/proof/cancel.zk` — 6 exposure(s). -/

def escrow_cancel_held : List Name := ["NULLIFIER_K", "buyer_pub_x", "buyer_pub_y", "buyer_secret", "escrow_id", "tx_binding", "tx_commitment", "tx_nonce"]


def escrow_cancel_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "buyer_pub" (.op "ec_mul_base" [.var "buyer_secret", .var "NULLIFIER_K"]),
  .assign "buyer_pub_x_computed" (.op "ec_get_x" [.var "buyer_pub"]),
  .assign "buyer_pub_y_computed" (.op "ec_get_y" [.var "buyer_pub"]),
  .constrainEq (.var "buyer_pub_x_computed") (.var "buyer_pub_x"),
  .constrainEq (.var "buyer_pub_y_computed") (.var "buyer_pub_y"),
  .assign "cancel_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "escrow_id", .var "buyer_secret"]),
  .constrainInstance (.var "escrow_id"),
  .constrainInstance (.var "buyer_pub_x"),
  .constrainInstance (.var "buyer_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "cancel_nullifier")
]


/-- **The property fails** for `src/contract/escrow/proof/cancel.zk`: its first undetermined exposure is
    `.var "escrow_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem escrow_cancel_has_a_free_instance :
    ¬ NoFreeInstance escrow_cancel_held escrow_cancel_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/escrow/proof/claim.zk` — 5 exposure(s). -/

def escrow_claim_held : List Name := ["NULLIFIER_K", "escrow_id", "escrow_seller_commitment", "seller_secret", "seller_x", "seller_y", "tx_binding", "tx_commitment", "tx_nonce"]


def escrow_claim_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "seller_pub" (.op "ec_mul_base" [.var "seller_secret", .var "NULLIFIER_K"]),
  .assign "seller_pub_x" (.op "ec_get_x" [.var "seller_pub"]),
  .assign "seller_pub_y" (.op "ec_get_y" [.var "seller_pub"]),
  .assign "seller_commitment_computed" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "seller_pub_x", .var "seller_pub_y"]),
  .constrainEq (.var "seller_commitment_computed") (.var "escrow_seller_commitment"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "escrow_id", .var "seller_secret"]),
  .constrainEq (.var "seller_pub_x") (.var "seller_x"),
  .constrainEq (.var "seller_pub_y") (.var "seller_y"),
  .constrainInstance (.var "escrow_id"),
  .constrainInstance (.var "escrow_seller_commitment"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "spent_nullifier")
]


/-- **The property fails** for `src/contract/escrow/proof/claim.zk`: its first undetermined exposure is
    `.var "escrow_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem escrow_claim_has_a_free_instance :
    ¬ NoFreeInstance escrow_claim_held escrow_claim_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/escrow/proof/create_escrow.zk` — 4 exposure(s). -/

def escrow_create_escrow_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "buyer_pub_x", "buyer_pub_y", "buyer_secret", "seller_pub_x", "seller_pub_y", "timeout", "tx_binding", "tx_commitment", "tx_nonce", "value"]


def escrow_create_escrow_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "buyer_pub_computed" (.op "ec_mul_base" [.var "buyer_secret", .var "NULLIFIER_K"]),
  .assign "buyer_pub_computed_x" (.op "ec_get_x" [.var "buyer_pub_computed"]),
  .assign "buyer_pub_computed_y" (.op "ec_get_y" [.var "buyer_pub_computed"]),
  .constrainEq (.var "buyer_pub_x") (.var "buyer_pub_computed_x"),
  .constrainEq (.var "buyer_pub_y") (.var "buyer_pub_computed_y"),
  .assign "seller_commitment" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "seller_pub_x", .var "seller_pub_y"]),
  .assign "C" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "buyer_pub_x", .var "buyer_pub_y", .var "seller_commitment", .var "value", .var "asset_id", .var "timeout"]),
  .constrainInstance (.var "C"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "seller_commitment")
]


/-- **The property fails** for `src/contract/escrow/proof/create_escrow.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem escrow_create_escrow_has_a_free_instance :
    ¬ NoFreeInstance escrow_create_escrow_held escrow_create_escrow_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/escrow/proof/fund.zk` — 6 exposure(s). -/

def escrow_fund_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "escrow_id", "merkle_leaf_pos", "merkle_path", "tx_binding", "tx_commitment", "tx_nonce", "value", "value_blind"]


def escrow_fund_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .constrainInstance (.var "escrow_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.op "merkle_root" [.var "merkle_leaf_pos", .var "merkle_path", .var "escrow_id"])
]


/-- **The property fails** for `src/contract/escrow/proof/fund.zk`: its first undetermined exposure is
    `.var "escrow_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem escrow_fund_has_a_free_instance :
    ¬ NoFreeInstance escrow_fund_held escrow_fund_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/escrow/proof/refund.zk` — 8 exposure(s). -/

def escrow_refund_held : List Name := ["NULLIFIER_K", "buyer_secret", "current_block", "escrow_buyer_pub_x", "escrow_buyer_pub_y", "escrow_id", "input_buyer_pub_x", "input_buyer_pub_y", "timeout", "tx_binding", "tx_commitment", "tx_nonce"]


def escrow_refund_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "buyer_pub" (.op "ec_mul_base" [.var "buyer_secret", .var "NULLIFIER_K"]),
  .assign "buyer_pub_x" (.op "ec_get_x" [.var "buyer_pub"]),
  .assign "buyer_pub_y" (.op "ec_get_y" [.var "buyer_pub"]),
  .constrainEq (.var "buyer_pub_x") (.var "escrow_buyer_pub_x"),
  .constrainEq (.var "buyer_pub_y") (.var "escrow_buyer_pub_y"),
  .constrainEq (.var "buyer_pub_x") (.var "input_buyer_pub_x"),
  .constrainEq (.var "buyer_pub_y") (.var "input_buyer_pub_y"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "escrow_id", .var "buyer_secret"]),
  .constrainInstance (.var "escrow_id"),
  .constrainInstance (.var "timeout"),
  .constrainInstance (.var "current_block"),
  .constrainInstance (.var "input_buyer_pub_x"),
  .constrainInstance (.var "input_buyer_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "spent_nullifier")
]


/-- **The property fails** for `src/contract/escrow/proof/refund.zk`: its first undetermined exposure is
    `.var "escrow_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem escrow_refund_has_a_free_instance :
    ¬ NoFreeInstance escrow_refund_held escrow_refund_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/call.zk` — 5 exposure(s). -/

def game_room_call_held : List Name := ["NULLIFIER_K", "player_nullifier", "player_pub_x", "player_pub_y", "player_secret", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_call_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 10]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "player_pub" (.op "ec_mul_base" [.var "player_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "player_pub"]) (.var "player_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "player_pub"]) (.var "player_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "room_id", .var "player_secret"]),
  .constrainEq (.var "computed") (.var "player_nullifier"),
  .constrainInstance (.var "player_pub_x"),
  .constrainInstance (.var "player_pub_y"),
  .constrainInstance (.var "player_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/game_room/proof/call.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_call_has_a_free_instance :
    ¬ NoFreeInstance game_room_call_held game_room_call_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/claim.zk` — 3 exposure(s). -/

def game_room_claim_held : List Name := ["nonce", "payout_amount", "pot_id", "room_id", "tx_binding", "tx_commitment", "tx_nonce", "winner_pub_x", "winner_pub_y"]


def game_room_claim_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_claim_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "pot_id", .var "winner_pub_x", .var "payout_amount", .var "nonce"]),
  .constrainInstance (.var "derived_claim_id"),
  .assign "derived_winner_key" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "winner_pub_x", .var "winner_pub_y"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/game_room/proof/claim.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_claim_has_a_free_instance :
    ¬ NoFreeInstance game_room_claim_held game_room_claim_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/close_pot.zk` — 5 exposure(s). -/

def game_room_close_pot_held : List Name := ["NULLIFIER_K", "player_nullifier", "player_pub_x", "player_pub_y", "player_secret", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_close_pot_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 12]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "player_pub" (.op "ec_mul_base" [.var "player_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "player_pub"]) (.var "player_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "player_pub"]) (.var "player_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "room_id", .var "player_secret"]),
  .constrainEq (.var "computed") (.var "player_nullifier"),
  .constrainInstance (.var "player_pub_x"),
  .constrainInstance (.var "player_pub_y"),
  .constrainInstance (.var "player_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/game_room/proof/close_pot.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_close_pot_has_a_free_instance :
    ¬ NoFreeInstance game_room_close_pot_held game_room_close_pot_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/contribute_entropy.zk` — 5 exposure(s). -/

def game_room_contribute_entropy_held : List Name := ["NULLIFIER_K", "player_nullifier", "player_pub_x", "player_pub_y", "player_secret", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_contribute_entropy_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 13]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "player_pub" (.op "ec_mul_base" [.var "player_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "player_pub"]) (.var "player_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "player_pub"]) (.var "player_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "room_id", .var "player_secret"]),
  .constrainEq (.var "computed") (.var "player_nullifier"),
  .constrainInstance (.var "player_pub_x"),
  .constrainInstance (.var "player_pub_y"),
  .constrainInstance (.var "player_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/game_room/proof/contribute_entropy.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_contribute_entropy_has_a_free_instance :
    ¬ NoFreeInstance game_room_contribute_entropy_held game_room_contribute_entropy_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/create_pot.zk` — 6 exposure(s). -/

def game_room_create_pot_held : List Name := ["NULLIFIER_K", "nonce", "player_nullifier", "player_pub_x", "player_pub_y", "player_secret", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_create_pot_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 14]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "player_pub" (.op "ec_mul_base" [.var "player_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "player_pub"]) (.var "player_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "player_pub"]) (.var "player_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "room_id", .var "player_secret"]),
  .constrainEq (.var "computed") (.var "player_nullifier"),
  .assign "pot_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "room_id", .var "player_pub_x", .var "nonce"]),
  .constrainInstance (.var "pot_id"),
  .constrainInstance (.var "player_pub_x"),
  .constrainInstance (.var "player_pub_y"),
  .constrainInstance (.var "player_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/game_room/proof/create_pot.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_create_pot_has_a_free_instance :
    ¬ NoFreeInstance game_room_create_pot_held game_room_create_pot_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/create_room.zk` — 3 exposure(s). -/

def game_room_create_room_held : List Name := ["asset_id", "block_height", "nonce", "owner_pub_x", "owner_pub_y", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_create_room_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_room_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "owner_pub_x", .var "owner_pub_y", .var "asset_id", .var "block_height", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_room_id")
]


/-- **The property fails** for `src/contract/game_room/proof/create_room.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_create_room_has_a_free_instance :
    ¬ NoFreeInstance game_room_create_room_held game_room_create_room_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/deposit.zk` — 4 exposure(s). -/

def game_room_deposit_held : List Name := ["amount", "nonce", "player_pub_x", "player_pub_y", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_deposit_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_account_key" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "room_id", .var "player_pub_x"]),
  .constrainInstance (.var "derived_account_key"),
  .assign "derived_player_key" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "player_pub_x", .var "player_pub_y"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_player_key")
]


/-- **The property fails** for `src/contract/game_room/proof/deposit.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_deposit_has_a_free_instance :
    ¬ NoFreeInstance game_room_deposit_held game_room_deposit_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/fold.zk` — 5 exposure(s). -/

def game_room_fold_held : List Name := ["NULLIFIER_K", "player_nullifier", "player_pub_x", "player_pub_y", "player_secret", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_fold_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 11]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "player_pub" (.op "ec_mul_base" [.var "player_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "player_pub"]) (.var "player_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "player_pub"]) (.var "player_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "room_id", .var "player_secret"]),
  .constrainEq (.var "computed") (.var "player_nullifier"),
  .constrainInstance (.var "player_pub_x"),
  .constrainInstance (.var "player_pub_y"),
  .constrainInstance (.var "player_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/game_room/proof/fold.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_fold_has_a_free_instance :
    ¬ NoFreeInstance game_room_fold_held game_room_fold_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/place_bet.zk` — 4 exposure(s). -/

def game_room_place_bet_held : List Name := ["amount", "block_height", "nonce", "player_pub_x", "player_pub_y", "pot_id", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_place_bet_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_bet_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "pot_id", .var "player_pub_x", .var "amount", .var "block_height"]),
  .constrainInstance (.var "derived_bet_id"),
  .assign "derived_commitment" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "amount", .var "nonce", .var "block_height"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_commitment")
]


/-- **The property fails** for `src/contract/game_room/proof/place_bet.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_place_bet_has_a_free_instance :
    ¬ NoFreeInstance game_room_place_bet_held game_room_place_bet_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/raise.zk` — 5 exposure(s). -/

def game_room_raise_held : List Name := ["NULLIFIER_K", "player_nullifier", "player_pub_x", "player_pub_y", "player_secret", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_raise_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 9]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "player_pub" (.op "ec_mul_base" [.var "player_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "player_pub"]) (.var "player_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "player_pub"]) (.var "player_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "room_id", .var "player_secret"]),
  .constrainEq (.var "computed") (.var "player_nullifier"),
  .constrainInstance (.var "player_pub_x"),
  .constrainInstance (.var "player_pub_y"),
  .constrainInstance (.var "player_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/game_room/proof/raise.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_raise_has_a_free_instance :
    ¬ NoFreeInstance game_room_raise_held game_room_raise_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/settle_pot.zk` — 4 exposure(s). -/

def game_room_settle_pot_held : List Name := ["house_pub_x", "house_pub_y", "nonce", "num_winners", "pot_id", "pot_total", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_settle_pot_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_room_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "house_pub_x", .var "house_pub_y", .var "nonce"]),
  .constrainInstance (.var "derived_room_id"),
  .assign "derived_pot_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "room_id", .var "pot_total", .var "house_pub_x"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_pot_id")
]


/-- **The property fails** for `src/contract/game_room/proof/settle_pot.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_settle_pot_has_a_free_instance :
    ¬ NoFreeInstance game_room_settle_pot_held game_room_settle_pot_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/game_room/proof/withdraw.zk` — 5 exposure(s). -/

def game_room_withdraw_held : List Name := ["NULLIFIER_K", "player_nullifier", "player_pub_x", "player_pub_y", "player_secret", "room_id", "tx_binding", "tx_commitment", "tx_nonce"]


def game_room_withdraw_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 8]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "player_pub" (.op "ec_mul_base" [.var "player_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "player_pub"]) (.var "player_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "player_pub"]) (.var "player_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "room_id", .var "player_secret"]),
  .constrainEq (.var "computed") (.var "player_nullifier"),
  .constrainInstance (.var "player_pub_x"),
  .constrainInstance (.var "player_pub_y"),
  .constrainInstance (.var "player_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/game_room/proof/withdraw.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem game_room_withdraw_has_a_free_instance :
    ¬ NoFreeInstance game_room_withdraw_held game_room_withdraw_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/identity/proof/issue_credential.zk` — 3 exposure(s). -/

def identity_issue_credential_held : List Name := ["NULLIFIER_K", "attribute_1", "attribute_1_name", "attribute_2", "attribute_2_name", "attribute_blind", "commitment", "credential_secret", "expires_at", "holder_pub_x", "holder_pub_y", "issued_at", "issuer_pub_x", "issuer_pub_y", "issuer_secret", "schema_hash", "tx_binding", "tx_commitment", "tx_nonce"]


def identity_issue_credential_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_ATTRIBUTE" (.op "witness_base" [.lit 10]),
  .assign "issuer_public" (.op "ec_mul_base" [.var "issuer_secret", .var "NULLIFIER_K"]),
  .assign "issuer_public_x" (.op "ec_get_x" [.var "issuer_public"]),
  .assign "issuer_public_y" (.op "ec_get_y" [.var "issuer_public"]),
  .constrainEq (.var "issuer_public_x") (.var "issuer_pub_x"),
  .constrainEq (.var "issuer_public_y") (.var "issuer_pub_y"),
  .assign "attribute_1_hash" (.op "poseidon_hash" [.var "DOMAIN_ATTRIBUTE", .var "attribute_1_name", .var "attribute_1"]),
  .assign "attribute_2_hash" (.op "poseidon_hash" [.var "DOMAIN_ATTRIBUTE", .var "attribute_2_name", .var "attribute_2"]),
  .assign "credential_data" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "issuer_pub_x", .var "issuer_pub_y", .var "holder_pub_x", .var "holder_pub_y", .var "schema_hash", .var "attribute_1_hash", .var "attribute_2_hash", .var "attribute_blind"]),
  .assign "commitment_check" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "credential_data", .var "credential_secret", .var "issued_at", .var "expires_at"]),
  .constrainInstance (.var "commitment_check"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainEq (.var "commitment_check") (.var "commitment")
]


/-- **The property fails** for `src/contract/identity/proof/issue_credential.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem identity_issue_credential_has_a_free_instance :
    ¬ NoFreeInstance identity_issue_credential_held identity_issue_credential_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/identity/proof/verify_capability.zk` — 10 exposure(s). -/

def identity_verify_capability_held : List Name := ["NULLIFIER_K", "attribute_1", "attribute_1_name", "attribute_2", "attribute_2_name", "attribute_blind", "commitment", "credential_secret", "expires_at", "holder_pub_x", "holder_pub_y", "issued_at", "issuer_pub_x", "issuer_pub_y", "predicate_result", "schema_hash", "threshold", "tx_binding", "tx_commitment", "tx_nonce"]


def identity_verify_capability_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_ATTRIBUTE" (.op "witness_base" [.lit 10]),
  .assign "attribute_1_hash" (.op "poseidon_hash" [.var "DOMAIN_ATTRIBUTE", .var "attribute_1_name", .var "attribute_1"]),
  .assign "attribute_2_hash" (.op "poseidon_hash" [.var "DOMAIN_ATTRIBUTE", .var "attribute_2_name", .var "attribute_2"]),
  .assign "credential_data" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "issuer_pub_x", .var "issuer_pub_y", .var "holder_pub_x", .var "holder_pub_y", .var "schema_hash", .var "attribute_1_hash", .var "attribute_2_hash", .var "attribute_blind"]),
  .assign "commitment_check" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "credential_data", .var "credential_secret", .var "issued_at", .var "expires_at"]),
  .constrainEq (.var "commitment_check") (.var "commitment"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "credential_secret", .var "commitment"]),
  .constrainInstance (.var "computed_nullifier"),
  .rangeCheck 64 (.var "attribute_1"),
  .rangeCheck 64 (.var "threshold"),
  .assign "is_lte" (.op "less_than_or_equal" [.var "threshold", .var "attribute_1"]),
  .constrainEq (.var "is_lte") (.var "predicate_result"),
  .constrainInstance (.var "schema_hash"),
  .constrainInstance (.var "issuer_pub_x"),
  .constrainInstance (.var "issuer_pub_y"),
  .constrainInstance (.var "attribute_1_name"),
  .constrainInstance (.var "threshold"),
  .constrainInstance (.var "predicate_result"),
  .constrainInstance (.var "commitment"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/identity/proof/verify_capability.zk`: its first undetermined exposure is
    `.var "schema_hash"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem identity_verify_capability_has_a_free_instance :
    ¬ NoFreeInstance identity_verify_capability_held identity_verify_capability_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/insurance_market/proof/purchase_coverage.zk` — 5 exposure(s). -/

def insurance_market_purchase_coverage_held : List Name := ["NULLIFIER_K", "buyer_nullifier", "buyer_pub_x", "buyer_pub_y", "buyer_secret", "purchase_nonce", "tx_binding", "tx_commitment", "tx_nonce"]


def insurance_market_purchase_coverage_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "buyer_pub" (.op "ec_mul_base" [.var "buyer_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "buyer_pub"]) (.var "buyer_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "buyer_pub"]) (.var "buyer_pub_y"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "buyer_pub_x", .var "buyer_pub_y", .var "buyer_secret", .var "purchase_nonce"]),
  .constrainEq (.var "computed_nullifier") (.var "buyer_nullifier"),
  .constrainInstance (.var "buyer_pub_x"),
  .constrainInstance (.var "buyer_pub_y"),
  .constrainInstance (.var "buyer_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/insurance_market/proof/purchase_coverage.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem insurance_market_purchase_coverage_has_a_free_instance :
    ¬ NoFreeInstance insurance_market_purchase_coverage_held insurance_market_purchase_coverage_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/insurance_market/proof/purchase_coverage_with_capability.zk` — 6 exposure(s). -/

def insurance_market_purchase_coverage_with_capability_held : List Name := ["NULLIFIER_K", "buyer_nullifier", "buyer_pub_x", "buyer_pub_y", "buyer_secret", "capability_predicate_result", "required_capability_id", "tx_binding", "tx_commitment", "tx_nonce"]


def insurance_market_purchase_coverage_with_capability_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "buyer_pub" (.op "ec_mul_base" [.var "buyer_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "buyer_pub"]) (.var "buyer_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "buyer_pub"]) (.var "buyer_pub_y"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .constrainEq (.var "capability_predicate_result") (.var "ONE"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "buyer_pub_x", .var "buyer_pub_y", .var "buyer_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "buyer_nullifier"),
  .constrainInstance (.var "buyer_pub_x"),
  .constrainInstance (.var "buyer_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "required_capability_id"),
  .constrainInstance (.var "buyer_nullifier")
]


/-- **The property fails** for `src/contract/insurance_market/proof/purchase_coverage_with_capability.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem insurance_market_purchase_coverage_with_capability_has_a_free_instance :
    ¬ NoFreeInstance insurance_market_purchase_coverage_with_capability_held insurance_market_purchase_coverage_with_capability_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/insurance_market/proof/purchase_coverage_with_dag.zk` — 5 exposure(s). -/

def insurance_market_purchase_coverage_with_dag_held : List Name := ["NULLIFIER_K", "buyer_nullifier", "buyer_pub_x", "buyer_pub_y", "buyer_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def insurance_market_purchase_coverage_with_dag_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "buyer_pub" (.op "ec_mul_base" [.var "buyer_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "buyer_pub"]) (.var "buyer_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "buyer_pub"]) (.var "buyer_pub_y"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "buyer_pub_x", .var "buyer_pub_y", .var "buyer_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "buyer_nullifier"),
  .constrainInstance (.var "buyer_pub_x"),
  .constrainInstance (.var "buyer_pub_y"),
  .constrainInstance (.var "buyer_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/insurance_market/proof/purchase_coverage_with_dag.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem insurance_market_purchase_coverage_with_dag_has_a_free_instance :
    ¬ NoFreeInstance insurance_market_purchase_coverage_with_dag_held insurance_market_purchase_coverage_with_dag_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/insurance_market/proof/underwrite_with_capability.zk` — 5 exposure(s). -/

def insurance_market_underwrite_with_capability_held : List Name := ["NULLIFIER_K", "capability_predicate_result", "required_capability_id", "tx_binding", "tx_commitment", "tx_nonce", "underwriter_pub_x", "underwriter_pub_y", "underwriter_secret"]


def insurance_market_underwrite_with_capability_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "underwriter_pub" (.op "ec_mul_base" [.var "underwriter_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "underwriter_pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "underwriter_pub"]),
  .constrainEq (.var "derived_pub_x") (.var "underwriter_pub_x"),
  .constrainEq (.var "derived_pub_y") (.var "underwriter_pub_y"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .constrainEq (.var "capability_predicate_result") (.var "ONE"),
  .constrainInstance (.var "underwriter_pub_x"),
  .constrainInstance (.var "underwriter_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "required_capability_id")
]


/-- **The property fails** for `src/contract/insurance_market/proof/underwrite_with_capability.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem insurance_market_underwrite_with_capability_has_a_free_instance :
    ¬ NoFreeInstance insurance_market_underwrite_with_capability_held insurance_market_underwrite_with_capability_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/labor_market/proof/accept_job.zk` — 6 exposure(s). -/

def labor_market_accept_job_held : List Name := ["NULLIFIER_K", "job_id", "tx_binding", "tx_commitment", "tx_nonce", "worker_pub_x", "worker_pub_y", "worker_secret"]


def labor_market_accept_job_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "ACCEPT_TAG" (.op "witness_base" [.lit 7]),
  .assign "worker_pub" (.op "ec_mul_base" [.var "worker_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "worker_pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "worker_pub"]),
  .constrainEq (.var "derived_pub_x") (.var "worker_pub_x"),
  .constrainEq (.var "derived_pub_y") (.var "worker_pub_y"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "ACCEPT_TAG", .var "job_id", .var "worker_secret"]),
  .constrainInstance (.var "spent_nullifier"),
  .constrainInstance (.var "job_id"),
  .constrainInstance (.var "worker_pub_x"),
  .constrainInstance (.var "worker_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/labor_market/proof/accept_job.zk`: its first undetermined exposure is
    `.var "job_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem labor_market_accept_job_has_a_free_instance :
    ¬ NoFreeInstance labor_market_accept_job_held labor_market_accept_job_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/labor_market/proof/accept_job_with_capability.zk` — 7 exposure(s). -/

def labor_market_accept_job_with_capability_held : List Name := ["NULLIFIER_K", "capability_id", "job_id", "tx_binding", "tx_commitment", "tx_nonce", "worker_pub_x", "worker_pub_y", "worker_secret"]


def labor_market_accept_job_with_capability_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "ACCEPT_WITH_CAP_TAG" (.op "witness_base" [.lit 8]),
  .assign "worker_pub" (.op "ec_mul_base" [.var "worker_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "worker_pub"]) (.var "worker_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "worker_pub"]) (.var "worker_pub_y"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "ACCEPT_WITH_CAP_TAG", .var "job_id", .var "worker_secret"]),
  .constrainInstance (.var "spent_nullifier"),
  .constrainInstance (.var "job_id"),
  .constrainInstance (.var "worker_pub_x"),
  .constrainInstance (.var "worker_pub_y"),
  .constrainInstance (.var "capability_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/labor_market/proof/accept_job_with_capability.zk`: its first undetermined exposure is
    `.var "job_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem labor_market_accept_job_with_capability_has_a_free_instance :
    ¬ NoFreeInstance labor_market_accept_job_with_capability_held labor_market_accept_job_with_capability_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/labor_market/proof/confirm_delivery.zk` — 6 exposure(s). -/

def labor_market_confirm_delivery_held : List Name := ["NULLIFIER_K", "employer_pub_x", "employer_pub_y", "employer_secret", "job_id", "tx_binding", "tx_commitment", "tx_nonce"]


def labor_market_confirm_delivery_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "CONFIRM_TAG" (.op "witness_base" [.lit 4]),
  .assign "employer_pub" (.op "ec_mul_base" [.var "employer_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "employer_pub"]) (.var "employer_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "employer_pub"]) (.var "employer_pub_y"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "CONFIRM_TAG", .var "job_id", .var "employer_secret"]),
  .constrainInstance (.var "spent_nullifier"),
  .constrainInstance (.var "job_id"),
  .constrainInstance (.var "employer_pub_x"),
  .constrainInstance (.var "employer_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/labor_market/proof/confirm_delivery.zk`: its first undetermined exposure is
    `.var "job_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem labor_market_confirm_delivery_has_a_free_instance :
    ¬ NoFreeInstance labor_market_confirm_delivery_held labor_market_confirm_delivery_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/labor_market/proof/create_job.zk` — 5 exposure(s). -/

def labor_market_create_job_held : List Name := ["NULLIFIER_K", "attestation_id", "employer_pub_x", "employer_pub_y", "employer_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def labor_market_create_job_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "employer_pub" (.op "ec_mul_base" [.var "employer_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "employer_pub"]) (.var "employer_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "employer_pub"]) (.var "employer_pub_y"),
  .constrainInstance (.var "employer_pub_x"),
  .constrainInstance (.var "employer_pub_y"),
  .constrainInstance (.var "attestation_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/labor_market/proof/create_job.zk`: its first undetermined exposure is
    `.var "attestation_id"`, which the circuit does not bind before exposing.
    **The checker fails it too** — one of the instances `OBL-Z16` names, where the model and the checker agree. -/
@[axiom_budget 0]
theorem labor_market_create_job_has_a_free_instance :
    ¬ NoFreeInstance labor_market_create_job_held labor_market_create_job_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/labor_market/proof/dispute.zk` — 7 exposure(s). -/

def labor_market_dispute_held : List Name := ["NULLIFIER_K", "dispute_reason_hash", "disputer_pub_x", "disputer_pub_y", "disputer_secret", "job_id", "tx_binding", "tx_commitment", "tx_nonce"]


def labor_market_dispute_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DISPUTE_TAG" (.op "witness_base" [.lit 9]),
  .assign "disputer_pub" (.op "ec_mul_base" [.var "disputer_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "disputer_pub"]) (.var "disputer_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "disputer_pub"]) (.var "disputer_pub_y"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "DISPUTE_TAG", .var "job_id", .var "disputer_secret", .var "dispute_reason_hash"]),
  .constrainInstance (.var "spent_nullifier"),
  .constrainInstance (.var "job_id"),
  .constrainInstance (.var "disputer_pub_x"),
  .constrainInstance (.var "disputer_pub_y"),
  .constrainInstance (.var "dispute_reason_hash"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/labor_market/proof/dispute.zk`: its first undetermined exposure is
    `.var "job_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem labor_market_dispute_has_a_free_instance :
    ¬ NoFreeInstance labor_market_dispute_held labor_market_dispute_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/labor_market/proof/milestone_payment.zk` — 7 exposure(s). -/

def labor_market_milestone_payment_held : List Name := ["NULLIFIER_K", "employer_pub_x", "employer_pub_y", "employer_secret", "job_id", "milestone_payment_amount", "tx_binding", "tx_commitment", "tx_nonce"]


def labor_market_milestone_payment_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "MILESTONE_TAG" (.op "witness_base" [.lit 2]),
  .assign "employer_pub" (.op "ec_mul_base" [.var "employer_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "employer_pub"]) (.var "employer_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "employer_pub"]) (.var "employer_pub_y"),
  .rangeCheck 64 (.var "milestone_payment_amount"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "MILESTONE_TAG", .var "job_id", .var "employer_secret"]),
  .constrainInstance (.var "spent_nullifier"),
  .constrainInstance (.var "job_id"),
  .constrainInstance (.var "employer_pub_x"),
  .constrainInstance (.var "employer_pub_y"),
  .constrainInstance (.var "milestone_payment_amount"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/labor_market/proof/milestone_payment.zk`: its first undetermined exposure is
    `.var "job_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem labor_market_milestone_payment_has_a_free_instance :
    ¬ NoFreeInstance labor_market_milestone_payment_held labor_market_milestone_payment_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/labor_market/proof/refund.zk` — 7 exposure(s). -/

def labor_market_refund_held : List Name := ["NULLIFIER_K", "completed_payment", "employer_pub_x", "employer_pub_y", "employer_secret", "job_id", "refund_amount", "total_payment", "tx_binding", "tx_commitment", "tx_nonce"]


def labor_market_refund_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "REFUND_TAG" (.op "witness_base" [.lit 7]),
  .assign "employer_pub" (.op "ec_mul_base" [.var "employer_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "employer_pub"]) (.var "employer_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "employer_pub"]) (.var "employer_pub_y"),
  .rangeCheck 64 (.var "completed_payment"),
  .rangeCheck 64 (.var "refund_amount"),
  .rangeCheck 64 (.var "total_payment"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "REFUND_TAG", .var "job_id", .var "employer_secret"]),
  .constrainInstance (.var "spent_nullifier"),
  .constrainInstance (.var "job_id"),
  .constrainInstance (.var "employer_pub_x"),
  .constrainInstance (.var "employer_pub_y"),
  .constrainInstance (.var "refund_amount"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/labor_market/proof/refund.zk`: its first undetermined exposure is
    `.var "job_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem labor_market_refund_has_a_free_instance :
    ¬ NoFreeInstance labor_market_refund_held labor_market_refund_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/labor_market/proof/submit_deliverable.zk` — 6 exposure(s). -/

def labor_market_submit_deliverable_held : List Name := ["NULLIFIER_K", "job_id", "tx_binding", "tx_commitment", "tx_nonce", "worker_pub_x", "worker_pub_y", "worker_secret"]


def labor_market_submit_deliverable_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "SUBMIT_TAG" (.op "witness_base" [.lit 5]),
  .assign "worker_pub" (.op "ec_mul_base" [.var "worker_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "worker_pub"]) (.var "worker_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "worker_pub"]) (.var "worker_pub_y"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "SUBMIT_TAG", .var "job_id", .var "worker_secret"]),
  .constrainInstance (.var "spent_nullifier"),
  .constrainInstance (.var "job_id"),
  .constrainInstance (.var "worker_pub_x"),
  .constrainInstance (.var "worker_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/labor_market/proof/submit_deliverable.zk`: its first undetermined exposure is
    `.var "job_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem labor_market_submit_deliverable_has_a_free_instance :
    ¬ NoFreeInstance labor_market_submit_deliverable_held labor_market_submit_deliverable_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/labor_market/proof/submit_git_deliverable.zk` — 6 exposure(s). -/

def labor_market_submit_git_deliverable_held : List Name := ["NULLIFIER_K", "job_id", "tx_binding", "tx_commitment", "tx_nonce", "worker_pub_x", "worker_pub_y", "worker_secret"]


def labor_market_submit_git_deliverable_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "GIT_SUBMIT_TAG" (.op "witness_base" [.lit 6]),
  .assign "worker_pub" (.op "ec_mul_base" [.var "worker_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "worker_pub"]) (.var "worker_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "worker_pub"]) (.var "worker_pub_y"),
  .assign "spent_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "GIT_SUBMIT_TAG", .var "job_id", .var "worker_secret"]),
  .constrainInstance (.var "spent_nullifier"),
  .constrainInstance (.var "job_id"),
  .constrainInstance (.var "worker_pub_x"),
  .constrainInstance (.var "worker_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/labor_market/proof/submit_git_deliverable.zk`: its first undetermined exposure is
    `.var "job_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem labor_market_submit_git_deliverable_has_a_free_instance :
    ¬ NoFreeInstance labor_market_submit_git_deliverable_held labor_market_submit_git_deliverable_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/lottery/proof/claim_prize.zk` — 3 exposure(s). -/

def lottery_claim_prize_held : List Name := ["NULLIFIER_K", "ticket_id", "ticket_pub_x", "ticket_pub_y", "ticket_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def lottery_claim_prize_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "ticket_pub" (.op "ec_mul_base" [.var "ticket_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "ticket_pub"]) (.var "ticket_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "ticket_pub"]) (.var "ticket_pub_y"),
  .assign "computed_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "ticket_id", .var "ticket_secret"]),
  .constrainInstance (.var "computed_commit"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/lottery/proof/claim_prize.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem lottery_claim_prize_has_a_free_instance :
    ¬ NoFreeInstance lottery_claim_prize_held lottery_claim_prize_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/lottery/proof/commit_ticket.zk` — 3 exposure(s). -/

def lottery_commit_ticket_held : List Name := ["NULLIFIER_K", "amount", "lottery_id", "nonce", "ticket_pub_x", "ticket_pub_y", "ticket_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def lottery_commit_ticket_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "computed_ticket_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "lottery_id", .var "ticket_pub_x", .var "ticket_pub_y", .var "amount", .var "nonce"]),
  .constrainInstance (.var "computed_ticket_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/lottery/proof/commit_ticket.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem lottery_commit_ticket_has_a_free_instance :
    ¬ NoFreeInstance lottery_commit_ticket_held lottery_commit_ticket_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/lottery/proof/draw_winners.zk` — 5 exposure(s). -/

def lottery_draw_winners_held : List Name := ["NULLIFIER_K", "house_nullifier", "house_pub_x", "house_pub_y", "house_secret", "lottery_id", "tx_binding", "tx_commitment", "tx_nonce"]


def lottery_draw_winners_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "house_pub" (.op "ec_mul_base" [.var "house_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "house_pub"]) (.var "house_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "house_pub"]) (.var "house_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "lottery_id", .var "house_secret"]),
  .constrainEq (.var "computed") (.var "house_nullifier"),
  .constrainInstance (.var "house_pub_x"),
  .constrainInstance (.var "house_pub_y"),
  .constrainInstance (.var "house_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/lottery/proof/draw_winners.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem lottery_draw_winners_has_a_free_instance :
    ¬ NoFreeInstance lottery_draw_winners_held lottery_draw_winners_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/lottery/proof/expire_lottery.zk` — 5 exposure(s). -/

def lottery_expire_lottery_held : List Name := ["NULLIFIER_K", "house_nullifier", "house_pub_x", "house_pub_y", "house_secret", "lottery_id", "tx_binding", "tx_commitment", "tx_nonce"]


def lottery_expire_lottery_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "house_pub" (.op "ec_mul_base" [.var "house_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "house_pub"]) (.var "house_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "house_pub"]) (.var "house_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "lottery_id", .var "house_secret"]),
  .constrainEq (.var "computed") (.var "house_nullifier"),
  .constrainInstance (.var "house_pub_x"),
  .constrainInstance (.var "house_pub_y"),
  .constrainInstance (.var "house_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/lottery/proof/expire_lottery.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem lottery_expire_lottery_has_a_free_instance :
    ¬ NoFreeInstance lottery_expire_lottery_held lottery_expire_lottery_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/lottery/proof/reveal_ticket.zk` — 2 exposure(s). -/

def lottery_reveal_ticket_held : List Name := ["NULLIFIER_K", "lottery_id", "ticket_number", "ticket_pub_x", "ticket_pub_y", "ticket_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def lottery_reveal_ticket_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/lottery/proof/reveal_ticket.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem lottery_reveal_ticket_has_a_free_instance :
    ¬ NoFreeInstance lottery_reveal_ticket_held lottery_reveal_ticket_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/multisig/proof/create_group.zk` — 5 exposure(s). -/

def multisig_create_group_held : List Name := ["group_id", "threshold", "total_keys", "tx_commitment", "tx_nonce"]


def multisig_create_group_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .rangeCheck 64 (.var "threshold"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "group_id"),
  .constrainInstance (.var "threshold"),
  .constrainInstance (.var "total_keys")
]


/-- **The property fails** for `src/contract/multisig/proof/create_group.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem multisig_create_group_has_a_free_instance :
    ¬ NoFreeInstance multisig_create_group_held multisig_create_group_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/multisig/proof/finalize.zk` — 5 exposure(s). -/

def multisig_finalize_held : List Name := ["approval_commit", "group_id", "message_hash", "tx_commitment", "tx_nonce"]


def multisig_finalize_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "computed_approval" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "group_id", .var "message_hash"]),
  .constrainEq (.var "computed_approval") (.var "approval_commit"),
  .constrainInstance (.var "group_id"),
  .constrainInstance (.var "message_hash"),
  .constrainInstance (.var "approval_commit"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/multisig/proof/finalize.zk`: its first undetermined exposure is
    `.var "group_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem multisig_finalize_has_a_free_instance :
    ¬ NoFreeInstance multisig_finalize_held multisig_finalize_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/multisig/proof/sign.zk` — 6 exposure(s). -/

def multisig_sign_held : List Name := ["group_id", "message_hash", "signer_secret", "tx_commitment", "tx_nonce"]


def multisig_sign_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_MEMBER_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "member_commitment" (.op "poseidon_hash" [.var "DOMAIN_MEMBER_COMMITMENT", .var "signer_secret"]),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "signer_secret", .var "group_id", .var "message_hash"]),
  .constrainInstance (.var "group_id"),
  .constrainInstance (.var "message_hash"),
  .constrainInstance (.var "member_commitment"),
  .constrainInstance (.var "nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/multisig/proof/sign.zk`: its first undetermined exposure is
    `.var "group_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem multisig_sign_has_a_free_instance :
    ¬ NoFreeInstance multisig_sign_held multisig_sign_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/native_token/proof/burn.zk` — 11 exposure(s). -/

def native_token_burn_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "asset_id_blind", "coin_spend_hook", "commitment_blind", "leaf_pos", "path", "signature_public_x", "signature_public_y", "signature_secret", "spend_secret", "tx_binding", "tx_commitment", "tx_nonce", "user_data", "user_data_blind", "value", "value_blind"]


def native_token_burn_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TOKEN_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_USER_DATA_ENC" (.op "witness_base" [.lit 6]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "pub" (.op "ec_mul_base" [.var "spend_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "pub"]),
  .assign "coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "derived_pub_x", .var "derived_pub_y", .var "value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "spend_secret", .var "coin"]),
  .constrainInstance (.var "nullifier"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "asset_id_commit" (.op "poseidon_hash" [.var "DOMAIN_TOKEN_COMMIT", .var "asset_id", .var "asset_id_blind"]),
  .constrainInstance (.var "asset_id_commit"),
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .assign "coin_incl" (.op "zero_cond" [.var "value", .var "coin"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "coin_incl"]),
  .constrainInstance (.var "root"),
  .assign "user_data_enc" (.op "poseidon_hash" [.var "DOMAIN_USER_DATA_ENC", .var "user_data", .var "user_data_blind"]),
  .constrainInstance (.var "user_data_enc"),
  .constrainInstance (.var "coin_spend_hook"),
  .assign "derived_signature_secret" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "spend_secret", .var "nullifier"]),
  .constrainEq (.var "derived_signature_secret") (.var "signature_secret"),
  .assign "signature_public" (.op "ec_mul_base" [.var "signature_secret", .var "NULLIFIER_K"]),
  .assign "derived_sig_pub_x" (.op "ec_get_x" [.var "signature_public"]),
  .assign "derived_sig_pub_y" (.op "ec_get_y" [.var "signature_public"]),
  .constrainEq (.var "derived_sig_pub_x") (.var "signature_public_x"),
  .constrainEq (.var "derived_sig_pub_y") (.var "signature_public_y"),
  .constrainInstance (.var "signature_public_x"),
  .constrainInstance (.var "signature_public_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "value")
]


/-- **The property fails** for `src/contract/native_token/proof/burn.zk`: its first undetermined exposure is
    `.var "coin_spend_hook"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem native_token_burn_has_a_free_instance :
    ¬ NoFreeInstance native_token_burn_held native_token_burn_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/native_token/proof/fee.zk` — 15 exposure(s). -/

def native_token_fee_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "fee", "fee_value_blind", "input_commitment_blind", "input_leaf_pos", "input_path", "input_secret", "input_spend_hook", "input_user_data", "input_user_data_blind", "input_value", "input_value_blind", "output_commitment_blind", "output_spend_hook", "output_user_data", "output_value", "output_value_blind", "signature_public_x", "signature_public_y", "signature_secret", "token", "token_blind", "tx_binding", "tx_commitment", "tx_nonce"]


def native_token_fee_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TOKEN_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_USER_DATA_ENC" (.op "witness_base" [.lit 6]),
  .assign "pub" (.op "ec_mul_base" [.var "input_secret", .var "NULLIFIER_K"]),
  .assign "pub_x" (.op "ec_get_x" [.var "pub"]),
  .assign "pub_y" (.op "ec_get_y" [.var "pub"]),
  .assign "input_coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "pub_x", .var "pub_y", .var "input_value", .var "token", .var "input_spend_hook", .var "input_user_data", .var "input_commitment_blind"]),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "input_secret", .var "input_coin"]),
  .constrainInstance (.var "nullifier"),
  .assign "input_vcv" (.op "ec_mul_short" [.var "input_value", .var "VALUE_COMMIT_VALUE"]),
  .assign "input_vcr" (.op "ec_mul" [.var "input_value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "input_value_commit" (.op "ec_add" [.var "input_vcv", .var "input_vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "input_value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "input_value_commit"]),
  .assign "token_commit" (.op "poseidon_hash" [.var "DOMAIN_TOKEN_COMMIT", .var "token", .var "token_blind"]),
  .constrainInstance (.var "token_commit"),
  .assign "root" (.op "merkle_root" [.var "input_leaf_pos", .var "input_path", .var "input_coin"]),
  .constrainInstance (.var "root"),
  .assign "user_data_enc" (.op "poseidon_hash" [.var "DOMAIN_USER_DATA_ENC", .var "input_user_data", .var "input_user_data_blind"]),
  .constrainInstance (.var "user_data_enc"),
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .constrainEq (.var "input_spend_hook") (.var "ZERO"),
  .assign "signature_public" (.op "ec_mul_base" [.var "signature_secret", .var "NULLIFIER_K"]),
  .assign "derived_sig_pub_x" (.op "ec_get_x" [.var "signature_public"]),
  .assign "derived_sig_pub_y" (.op "ec_get_y" [.var "signature_public"]),
  .constrainEq (.var "derived_sig_pub_x") (.var "signature_public_x"),
  .constrainEq (.var "derived_sig_pub_y") (.var "signature_public_y"),
  .constrainInstance (.var "signature_public_x"),
  .constrainInstance (.var "signature_public_y"),
  .assign "output_coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "pub_x", .var "pub_y", .var "output_value", .var "token", .var "output_spend_hook", .var "output_user_data", .var "output_commitment_blind"]),
  .constrainInstance (.var "output_coin"),
  .assign "output_vcv" (.op "ec_mul_short" [.var "output_value", .var "VALUE_COMMIT_VALUE"]),
  .assign "output_vcr" (.op "ec_mul" [.var "output_value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "output_value_commit" (.op "ec_add" [.var "output_vcv", .var "output_vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "output_value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "output_value_commit"]),
  .assign "computed_sum" (.op "base_add" [.var "output_value", .var "fee"]),
  .constrainEq (.var "computed_sum") (.var "input_value"),
  .assign "fee_vcv" (.op "ec_mul_short" [.var "fee", .var "VALUE_COMMIT_VALUE"]),
  .assign "fee_vcr" (.op "ec_mul" [.var "fee_value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "fee_value_commit" (.op "ec_add" [.var "fee_vcv", .var "fee_vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "fee_value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "fee_value_commit"]),
  .rangeCheck 64 (.var "input_value"),
  .rangeCheck 64 (.var "output_value"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "fee")
]


/-- **The property fails** for `src/contract/native_token/proof/fee.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem native_token_fee_has_a_free_instance :
    ¬ NoFreeInstance native_token_fee_held native_token_fee_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/native_token/proof/mint.zk` — 10 exposure(s). -/

def native_token_mint_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "asset_id_blind", "coin_spend_hook", "commitment_blind", "effective_value", "new_cumulative_x", "new_cumulative_y", "old_cumulative_blind", "old_cumulative_value", "public_x", "public_y", "spend_secret", "total_pin", "tx_commitment", "tx_nonce", "user_data", "value", "value_blind"]


def native_token_mint_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TOKEN_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "pk" (.op "ec_mul_base" [.var "spend_secret", .var "NULLIFIER_K"]),
  .assign "pk_x" (.op "ec_get_x" [.var "pk"]),
  .assign "pk_y" (.op "ec_get_y" [.var "pk"]),
  .constrainEq (.var "pk_x") (.var "public_x"),
  .constrainEq (.var "pk_y") (.var "public_y"),
  .assign "C" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "public_x", .var "public_y", .var "effective_value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .constrainInstance (.var "C"),
  .assign "nf" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "spend_secret", .var "C"]),
  .constrainInstance (.var "nf"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "asset_id_commit" (.op "poseidon_hash" [.var "DOMAIN_TOKEN_COMMIT", .var "asset_id", .var "asset_id_blind"]),
  .constrainInstance (.var "asset_id_commit"),
  .assign "old_cum_vcv" (.op "ec_mul_short" [.var "old_cumulative_value", .var "VALUE_COMMIT_VALUE"]),
  .assign "old_cum_vcr" (.op "ec_mul" [.var "old_cumulative_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "old_cumulative" (.op "ec_add" [.var "old_cum_vcv", .var "old_cum_vcr"]),
  .assign "new_cumulative" (.op "ec_add" [.var "old_cumulative", .var "value_commit"]),
  .assign "new_cum_x" (.op "ec_get_x" [.var "new_cumulative"]),
  .assign "new_cum_y" (.op "ec_get_y" [.var "new_cumulative"]),
  .constrainEq (.var "new_cum_x") (.var "new_cumulative_x"),
  .constrainEq (.var "new_cum_y") (.var "new_cumulative_y"),
  .constrainInstance (.var "new_cumulative_x"),
  .constrainInstance (.var "new_cumulative_y"),
  .rangeCheck 64 (.var "value"),
  .rangeCheck 64 (.var "effective_value"),
  .assign "effective_plus_pin" (.op "base_add" [.var "effective_value", .var "total_pin"]),
  .constrainEq (.var "effective_plus_pin") (.var "value"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "total_pin"),
  .rangeCheck 64 (.var "old_cumulative_value")
]


/-- **The property fails** for `src/contract/native_token/proof/mint.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem native_token_mint_has_a_free_instance :
    ¬ NoFreeInstance native_token_mint_held native_token_mint_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/oracle/proof/aggregate.zk` — 8 exposure(s). -/

def oracle_aggregate_held : List Name := ["max_result", "min_result", "oracle_id", "oracle_secret", "result", "sum_weights", "tx_binding", "tx_commitment", "tx_nonce", "value_0", "value_1", "value_2", "value_3", "weight_0", "weight_1", "weight_2", "weight_3"]


def oracle_aggregate_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_OPERATOR_COMMITMENT" (.op "witness_base" [.lit 8]),
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .rangeCheck 64 (.var "value_0"),
  .rangeCheck 64 (.var "value_1"),
  .rangeCheck 64 (.var "value_2"),
  .rangeCheck 64 (.var "value_3"),
  .rangeCheck 64 (.var "weight_0"),
  .rangeCheck 64 (.var "weight_1"),
  .rangeCheck 64 (.var "weight_2"),
  .rangeCheck 64 (.var "weight_3"),
  .rangeCheck 64 (.var "sum_weights"),
  .rangeCheck 64 (.var "result"),
  .rangeCheck 64 (.var "min_result"),
  .rangeCheck 64 (.var "max_result"),
  .assign "weighted_0" (.op "base_mul" [.var "value_0", .var "weight_0"]),
  .assign "weighted_1" (.op "base_mul" [.var "value_1", .var "weight_1"]),
  .assign "weighted_2" (.op "base_mul" [.var "value_2", .var "weight_2"]),
  .assign "weighted_3" (.op "base_mul" [.var "value_3", .var "weight_3"]),
  .assign "weighted_sum_0" (.op "base_add" [.var "weighted_0", .var "weighted_1"]),
  .assign "weighted_sum_1" (.op "base_add" [.var "weighted_2", .var "weighted_3"]),
  .assign "weighted_sum" (.op "base_add" [.var "weighted_sum_0", .var "weighted_sum_1"]),
  .assign "claimed_sum_0" (.op "base_add" [.var "weight_0", .var "weight_1"]),
  .assign "claimed_sum_1" (.op "base_add" [.var "weight_2", .var "weight_3"]),
  .assign "claimed_sum" (.op "base_add" [.var "claimed_sum_0", .var "claimed_sum_1"]),
  .constrainEq (.var "claimed_sum") (.var "sum_weights"),
  .assign "res_times_sum" (.op "base_mul" [.var "result", .var "sum_weights"]),
  .assign "lte_check" (.op "less_than_or_equal" [.var "res_times_sum", .var "weighted_sum"]),
  .constrainEq (.var "lte_check") (.var "ONE"),
  .assign "res_plus_one" (.op "base_add" [.var "result", .var "ONE"]),
  .assign "upper" (.op "base_mul" [.var "res_plus_one", .var "sum_weights"]),
  .assign "diff_max" (.op "base_sub" [.var "max_result", .var "result"]),
  .rangeCheck 64 (.var "diff_max"),
  .assign "diff_min" (.op "base_sub" [.var "result", .var "min_result"]),
  .rangeCheck 64 (.var "diff_min"),
  .assign "oracle_commitment" (.op "poseidon_hash" [.var "DOMAIN_OPERATOR_COMMITMENT", .var "oracle_secret", .var "oracle_id"]),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "oracle_secret", .var "oracle_id", .var "result"]),
  .constrainInstance (.var "oracle_id"),
  .constrainInstance (.var "oracle_commitment"),
  .constrainInstance (.var "result"),
  .constrainInstance (.var "min_result"),
  .constrainInstance (.var "max_result"),
  .constrainInstance (.var "nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/oracle/proof/aggregate.zk`: its first undetermined exposure is
    `.var "oracle_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem oracle_aggregate_has_a_free_instance :
    ¬ NoFreeInstance oracle_aggregate_held oracle_aggregate_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/oracle/proof/attest_value.zk` — 8 exposure(s). -/

def oracle_attest_value_held : List Name := ["attestation_id", "oracle_id", "oracle_secret", "predicate", "threshold", "tx_binding", "tx_commitment", "tx_nonce", "value"]


def oracle_attest_value_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_OPERATOR_COMMITMENT" (.op "witness_base" [.lit 8]),
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "oracle_commitment" (.op "poseidon_hash" [.var "DOMAIN_OPERATOR_COMMITMENT", .var "oracle_secret", .var "oracle_id"]),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "oracle_secret", .var "oracle_id", .var "attestation_id"]),
  .constrainInstance (.var "oracle_id"),
  .constrainInstance (.var "oracle_commitment"),
  .constrainInstance (.var "attestation_id"),
  .constrainInstance (.var "predicate"),
  .constrainInstance (.var "threshold"),
  .constrainInstance (.var "nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/oracle/proof/attest_value.zk`: its first undetermined exposure is
    `.var "oracle_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem oracle_attest_value_has_a_free_instance :
    ¬ NoFreeInstance oracle_attest_value_held oracle_attest_value_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/oracle/proof/push_value.zk` — 6 exposure(s). -/

def oracle_push_value_held : List Name := ["oracle_id", "oracle_secret", "tx_binding", "tx_commitment", "tx_nonce", "value"]


def oracle_push_value_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_OPERATOR_COMMITMENT" (.op "witness_base" [.lit 8]),
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "oracle_commitment" (.op "poseidon_hash" [.var "DOMAIN_OPERATOR_COMMITMENT", .var "oracle_secret", .var "oracle_id"]),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "oracle_secret", .var "oracle_id", .var "value"]),
  .constrainInstance (.var "oracle_id"),
  .constrainInstance (.var "oracle_commitment"),
  .constrainInstance (.var "value"),
  .constrainInstance (.var "nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/oracle/proof/push_value.zk`: its first undetermined exposure is
    `.var "oracle_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem oracle_push_value_has_a_free_instance :
    ¬ NoFreeInstance oracle_push_value_held oracle_push_value_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/oracle/proof/push_value_commitment.zk` — 6 exposure(s). -/

def oracle_push_value_commitment_held : List Name := ["commitment", "nonce", "oracle_id", "staker_secret", "tx_binding", "tx_commitment", "tx_nonce", "value"]


def oracle_push_value_commitment_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_OPERATOR_COMMITMENT" (.op "witness_base" [.lit 8]),
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "oracle_commitment" (.op "poseidon_hash" [.var "DOMAIN_OPERATOR_COMMITMENT", .var "staker_secret", .var "oracle_id"]),
  .assign "computed_commitment" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "value", .var "nonce"]),
  .constrainEq (.var "computed_commitment") (.var "commitment"),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "staker_secret", .var "oracle_id", .var "commitment"]),
  .constrainInstance (.var "oracle_id"),
  .constrainInstance (.var "oracle_commitment"),
  .constrainInstance (.var "commitment"),
  .constrainInstance (.var "nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/oracle/proof/push_value_commitment.zk`: its first undetermined exposure is
    `.var "oracle_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem oracle_push_value_commitment_has_a_free_instance :
    ¬ NoFreeInstance oracle_push_value_commitment_held oracle_push_value_commitment_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/oracle/proof/register_oracle.zk` — 4 exposure(s). -/

def oracle_register_oracle_held : List Name := ["oracle_id", "oracle_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def oracle_register_oracle_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_OPERATOR_COMMITMENT" (.op "witness_base" [.lit 8]),
  .assign "oracle_commitment" (.op "poseidon_hash" [.var "DOMAIN_OPERATOR_COMMITMENT", .var "oracle_secret", .var "oracle_id"]),
  .constrainInstance (.var "oracle_id"),
  .constrainInstance (.var "oracle_commitment"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/oracle/proof/register_oracle.zk`: its first undetermined exposure is
    `.var "oracle_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem oracle_register_oracle_has_a_free_instance :
    ¬ NoFreeInstance oracle_register_oracle_held oracle_register_oracle_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/oracle/proof/set_oracle_active.zk` — 5 exposure(s). -/

def oracle_set_oracle_active_held : List Name := ["is_active", "oracle_id", "oracle_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def oracle_set_oracle_active_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_OPERATOR_COMMITMENT" (.op "witness_base" [.lit 8]),
  .assign "oracle_commitment" (.op "poseidon_hash" [.var "DOMAIN_OPERATOR_COMMITMENT", .var "oracle_secret", .var "oracle_id"]),
  .constrainInstance (.var "oracle_id"),
  .constrainInstance (.var "oracle_commitment"),
  .constrainInstance (.var "is_active"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/oracle/proof/set_oracle_active.zk`: its first undetermined exposure is
    `.var "oracle_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem oracle_set_oracle_active_has_a_free_instance :
    ¬ NoFreeInstance oracle_set_oracle_active_held oracle_set_oracle_active_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/otc_swap/proof/cancel_swap.zk` — 8 exposure(s). -/

def otc_swap_cancel_swap_held : List Name := ["NULLIFIER_K", "alice_pub_x", "alice_pub_y", "alice_secret", "current_block", "recipient_x", "recipient_y", "swap_id", "timeout", "tx_binding", "tx_commitment", "tx_nonce"]


def otc_swap_cancel_swap_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "alice_pub_computed" (.op "ec_mul_base" [.var "alice_secret", .var "NULLIFIER_K"]),
  .assign "alice_pub_computed_x" (.op "ec_get_x" [.var "alice_pub_computed"]),
  .assign "alice_pub_computed_y" (.op "ec_get_y" [.var "alice_pub_computed"]),
  .constrainEq (.var "alice_pub_x") (.var "alice_pub_computed_x"),
  .constrainEq (.var "alice_pub_y") (.var "alice_pub_computed_y"),
  .assign "N" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "swap_id", .var "alice_secret"]),
  .constrainInstance (.var "swap_id"),
  .constrainInstance (.var "timeout"),
  .constrainInstance (.var "current_block"),
  .constrainInstance (.var "alice_pub_x"),
  .constrainInstance (.var "alice_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "N")
]


/-- **The property fails** for `src/contract/otc_swap/proof/cancel_swap.zk`: its first undetermined exposure is
    `.var "swap_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem otc_swap_cancel_swap_has_a_free_instance :
    ¬ NoFreeInstance otc_swap_cancel_swap_held otc_swap_cancel_swap_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/otc_swap/proof/create_swap.zk` — 4 exposure(s). -/

def otc_swap_create_swap_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "alice_pub_x", "alice_pub_y", "alice_secret", "bob_pub_x", "bob_pub_y", "recv_asset_id", "recv_value", "send_asset_id", "send_value", "timeout", "tx_binding", "tx_commitment", "tx_nonce"]


def otc_swap_create_swap_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "alice_pub_computed" (.op "ec_mul_base" [.var "alice_secret", .var "NULLIFIER_K"]),
  .assign "alice_pub_computed_x" (.op "ec_get_x" [.var "alice_pub_computed"]),
  .assign "alice_pub_computed_y" (.op "ec_get_y" [.var "alice_pub_computed"]),
  .constrainEq (.var "alice_pub_x") (.var "alice_pub_computed_x"),
  .constrainEq (.var "alice_pub_y") (.var "alice_pub_computed_y"),
  .assign "bob_commitment" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "bob_pub_x", .var "bob_pub_y"]),
  .assign "C" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "alice_pub_x", .var "alice_pub_y", .var "bob_commitment", .var "send_value", .var "send_asset_id", .var "recv_value", .var "recv_asset_id", .var "timeout"]),
  .constrainInstance (.var "C"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "bob_commitment")
]


/-- **The property fails** for `src/contract/otc_swap/proof/create_swap.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem otc_swap_create_swap_has_a_free_instance :
    ¬ NoFreeInstance otc_swap_create_swap_held otc_swap_create_swap_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/otc_swap/proof/execute_swap.zk` — 5 exposure(s). -/

def otc_swap_execute_swap_held : List Name := ["NULLIFIER_K", "alice_recipient_x", "alice_recipient_y", "bob_pub_x", "bob_pub_y", "bob_recipient_x", "bob_recipient_y", "bob_secret", "swap_id", "tx_binding", "tx_commitment", "tx_nonce"]


def otc_swap_execute_swap_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "bob_pub_computed" (.op "ec_mul_base" [.var "bob_secret", .var "NULLIFIER_K"]),
  .assign "bob_pub_computed_x" (.op "ec_get_x" [.var "bob_pub_computed"]),
  .assign "bob_pub_computed_y" (.op "ec_get_y" [.var "bob_pub_computed"]),
  .constrainEq (.var "bob_pub_x") (.var "bob_pub_computed_x"),
  .constrainEq (.var "bob_pub_y") (.var "bob_pub_computed_y"),
  .assign "bob_commitment" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "bob_pub_x", .var "bob_pub_y"]),
  .assign "N" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "swap_id", .var "bob_secret"]),
  .constrainInstance (.var "swap_id"),
  .constrainInstance (.var "bob_commitment"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "N")
]


/-- **The property fails** for `src/contract/otc_swap/proof/execute_swap.zk`: its first undetermined exposure is
    `.var "swap_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem otc_swap_execute_swap_has_a_free_instance :
    ¬ NoFreeInstance otc_swap_execute_swap_held otc_swap_execute_swap_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/otc_swap/proof/fund_swap.zk` — 6 exposure(s). -/

def otc_swap_fund_swap_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "merkle_leaf_pos", "merkle_path", "swap_id", "tx_binding", "tx_commitment", "tx_nonce", "value", "value_blind"]


def otc_swap_fund_swap_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .constrainInstance (.var "swap_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.op "merkle_root" [.var "merkle_leaf_pos", .var "merkle_path", .var "swap_id"])
]


/-- **The property fails** for `src/contract/otc_swap/proof/fund_swap.zk`: its first undetermined exposure is
    `.var "swap_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem otc_swap_fund_swap_has_a_free_instance :
    ¬ NoFreeInstance otc_swap_fund_swap_held otc_swap_fund_swap_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/pool_stake/proof/allocate_coverage.zk` — 3 exposure(s). -/

def pool_stake_allocate_coverage_held : List Name := ["coverage_amount", "member_pub_x", "member_pub_y", "nonce", "pool_id", "tx_binding", "tx_commitment", "tx_nonce", "withdrawal_id"]


def pool_stake_allocate_coverage_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_allocation_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "pool_id", .var "member_pub_x", .var "member_pub_y", .var "coverage_amount", .var "withdrawal_id", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_allocation_id")
]


/-- **The property fails** for `src/contract/pool_stake/proof/allocate_coverage.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem pool_stake_allocate_coverage_has_a_free_instance :
    ¬ NoFreeInstance pool_stake_allocate_coverage_held pool_stake_allocate_coverage_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/pool_stake/proof/create_pool.zk` — 3 exposure(s). -/

def pool_stake_create_pool_held : List Name := ["creator_pub_x", "creator_pub_y", "nonce", "pool_config_hash", "tx_binding", "tx_commitment", "tx_nonce"]


def pool_stake_create_pool_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_pool_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "creator_pub_x", .var "creator_pub_y", .var "pool_config_hash", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_pool_id")
]


/-- **The property fails** for `src/contract/pool_stake/proof/create_pool.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem pool_stake_create_pool_has_a_free_instance :
    ¬ NoFreeInstance pool_stake_create_pool_held pool_stake_create_pool_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/pool_stake/proof/join_pool.zk` — 5 exposure(s). -/

def pool_stake_join_pool_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "member_pub_x", "member_pub_y", "nonce", "pool_id", "stake_amount", "tx_binding", "tx_commitment", "tx_nonce", "value_blind"]


def pool_stake_join_pool_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_member_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "pool_id", .var "member_pub_x", .var "member_pub_y", .var "stake_amount", .var "nonce"]),
  .constrainInstance (.var "derived_member_id"),
  .assign "vcv" (.op "ec_mul_short" [.var "stake_amount", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"])
]


/-- **The property fails** for `src/contract/pool_stake/proof/join_pool.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem pool_stake_join_pool_has_a_free_instance :
    ¬ NoFreeInstance pool_stake_join_pool_held pool_stake_join_pool_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/pool_stake/proof/slash_coverage.zk` — 3 exposure(s). -/

def pool_stake_slash_coverage_held : List Name := ["allocation_id", "nonce", "slashed_amount", "slashed_to_pub_x", "slashed_to_pub_y", "tx_binding", "tx_commitment", "tx_nonce"]


def pool_stake_slash_coverage_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_slash_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "allocation_id", .var "slashed_amount", .var "slashed_to_pub_x", .var "slashed_to_pub_y", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_slash_id")
]


/-- **The property fails** for `src/contract/pool_stake/proof/slash_coverage.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem pool_stake_slash_coverage_has_a_free_instance :
    ¬ NoFreeInstance pool_stake_slash_coverage_held pool_stake_slash_coverage_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/promissory_note/proof/issue.zk` — 9 exposure(s). -/

def promissory_note_issue_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "backing_secret", "coin_public", "coin_spend_hook", "commitment_blind", "mint_public", "token_leaf_pos", "token_path", "tx_binding", "tx_commitment", "tx_nonce", "user_data", "value", "value_blind"]


def promissory_note_issue_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TOK_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "token_root" (.op "merkle_root" [.var "token_leaf_pos", .var "token_path", .var "asset_id"]),
  .constrainInstance (.var "token_root"),
  .assign "derived_mint_public" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "backing_secret"]),
  .constrainEq (.var "derived_mint_public") (.var "mint_public"),
  .constrainInstance (.var "mint_public"),
  .constrainEq (.var "coin_public") (.var "mint_public"),
  .assign "coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "coin_public", .var "value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .constrainInstance (.var "coin"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .constrainInstance (.var "asset_id"),
  .constrainInstance (.var "coin_spend_hook"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "value")
]


/-- **The property fails** for `src/contract/promissory_note/proof/issue.zk`: its first undetermined exposure is
    `.var "asset_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem promissory_note_issue_has_a_free_instance :
    ¬ NoFreeInstance promissory_note_issue_held promissory_note_issue_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/promissory_note/proof/redeem.zk` — 8 exposure(s). -/

def promissory_note_redeem_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "asset_id_blind", "coin_public", "coin_spend_hook", "commitment_blind", "tx_binding", "tx_commitment", "tx_nonce", "user_data", "value", "value_blind"]


def promissory_note_redeem_stmts : List Stmt :=
[
  .assign "DOMAIN_TOK_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "coin_public", .var "value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .constrainInstance (.var "coin"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "token_commit" (.op "poseidon_hash" [.var "DOMAIN_TOK_COMMIT", .var "asset_id", .var "asset_id_blind"]),
  .constrainInstance (.var "token_commit"),
  .constrainInstance (.var "value"),
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .constrainEq (.var "value") (.var "ZERO"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "coin_spend_hook")
]


/-- **The property fails** for `src/contract/promissory_note/proof/redeem.zk`: its first undetermined exposure is
    `.var "value"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem promissory_note_redeem_has_a_free_instance :
    ¬ NoFreeInstance promissory_note_redeem_held promissory_note_redeem_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/promissory_note/proof/register_type.zk` — 8 exposure(s). -/

def promissory_note_register_type_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "coin_asset_id", "coin_public", "coin_spend_hook", "commitment_blind", "token_auth_parent", "token_blind", "token_user_data", "tx_binding", "tx_commitment", "tx_nonce", "user_data", "value", "value_blind"]


def promissory_note_register_type_stmts : List Stmt :=
[
  .assign "DOMAIN_TOK_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "asset_id" (.op "poseidon_hash" [.var "DOMAIN_TOK_COMMIT", .var "token_auth_parent", .var "token_user_data", .var "token_blind"]),
  .constrainInstance (.var "asset_id"),
  .constrainInstance (.var "token_auth_parent"),
  .assign "coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "coin_public", .var "value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .constrainInstance (.var "coin"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .constrainInstance (.var "coin_spend_hook"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "value")
]


/-- **The property fails** for `src/contract/promissory_note/proof/register_type.zk`: its first undetermined exposure is
    `.var "token_auth_parent"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem promissory_note_register_type_has_a_free_instance :
    ¬ NoFreeInstance promissory_note_register_type_held promissory_note_register_type_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/promissory_note/proof/revoke.zk` — 10 exposure(s). -/

def promissory_note_revoke_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "asset_id_blind", "coin_spend_hook", "commitment_blind", "leaf_pos", "path", "signature_secret", "spend_secret", "tx_binding", "tx_commitment", "tx_nonce", "user_data", "user_data_blind", "value", "value_blind"]


def promissory_note_revoke_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TOK_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_USER_DATA_ENC" (.op "witness_base" [.lit 6]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "pub" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "spend_secret"]),
  .assign "coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "pub", .var "value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .assign "nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "spend_secret", .var "coin"]),
  .constrainInstance (.var "nullifier"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "token_commit" (.op "poseidon_hash" [.var "DOMAIN_TOK_COMMIT", .var "asset_id", .var "asset_id_blind"]),
  .constrainInstance (.var "token_commit"),
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .assign "coin_incl" (.op "zero_cond" [.var "value", .var "coin"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "coin_incl"]),
  .constrainInstance (.var "root"),
  .assign "user_data_enc" (.op "poseidon_hash" [.var "DOMAIN_USER_DATA_ENC", .var "user_data", .var "user_data_blind"]),
  .constrainInstance (.var "user_data_enc"),
  .constrainInstance (.var "coin_spend_hook"),
  .assign "derived_signature_secret" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "spend_secret", .var "nullifier"]),
  .constrainEq (.var "derived_signature_secret") (.var "signature_secret"),
  .assign "signature_public" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "signature_secret"]),
  .constrainInstance (.var "signature_public"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "value")
]


/-- **The property fails** for `src/contract/promissory_note/proof/revoke.zk`: its first undetermined exposure is
    `.var "coin_spend_hook"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem promissory_note_revoke_has_a_free_instance :
    ¬ NoFreeInstance promissory_note_revoke_held promissory_note_revoke_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/promissory_note/proof/transfer.zk` — 7 exposure(s). -/

def promissory_note_transfer_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "asset_id_blind", "coin_public", "coin_spend_hook", "commitment_blind", "tx_binding", "tx_commitment", "tx_nonce", "user_data", "value", "value_blind"]


def promissory_note_transfer_stmts : List Stmt :=
[
  .assign "DOMAIN_TOK_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "coin" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "coin_public", .var "value", .var "asset_id", .var "coin_spend_hook", .var "user_data", .var "commitment_blind"]),
  .constrainInstance (.var "coin"),
  .assign "vcv" (.op "ec_mul_short" [.var "value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "token_commit" (.op "poseidon_hash" [.var "DOMAIN_TOK_COMMIT", .var "asset_id", .var "asset_id_blind"]),
  .constrainInstance (.var "token_commit"),
  .constrainInstance (.var "coin_spend_hook"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "value")
]


/-- **The property fails** for `src/contract/promissory_note/proof/transfer.zk`: its first undetermined exposure is
    `.var "coin_spend_hook"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem promissory_note_transfer_has_a_free_instance :
    ¬ NoFreeInstance promissory_note_transfer_held promissory_note_transfer_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/purse/proof/balance.zk` — 7 exposure(s). -/

def purse_balance_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "balance", "balance_blind", "balance_commit_x", "balance_commit_y", "derived_purse_id", "expected_root", "leaf_pos", "owner_pub", "owner_secret", "path", "purse_id", "state_nonce", "token_blind", "token_commit", "tx_binding", "tx_commitment", "tx_nonce"]


def purse_balance_stmts : List Stmt :=
[
  .assign "DOMAIN_TOK_COMMIT" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_MERKLE_LEAF" (.op "witness_base" [.lit 5]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "dp_circuit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "owner_pub", .var "asset_id", .var "purse_id"]),
  .constrainEq (.var "dp_circuit") (.var "derived_purse_id"),
  .constrainInstance (.var "derived_purse_id"),
  .assign "derived_owner" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "owner_secret"]),
  .constrainEq (.var "derived_owner") (.var "owner_pub"),
  .assign "purse_leaf" (.op "poseidon_hash" [.var "DOMAIN_MERKLE_LEAF", .var "purse_id", .var "balance", .var "state_nonce"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "purse_leaf"]),
  .constrainEq (.var "root") (.var "expected_root"),
  .constrainInstance (.var "expected_root"),
  .assign "bvcv" (.op "ec_mul_short" [.var "balance", .var "VALUE_COMMIT_VALUE"]),
  .assign "bvcr" (.op "ec_mul" [.var "balance_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "balance_commit" (.op "ec_add" [.var "bvcv", .var "bvcr"]),
  .constrainEq (.op "ec_get_x" [.var "balance_commit"]) (.var "balance_commit_x"),
  .constrainInstance (.var "balance_commit_x"),
  .constrainEq (.op "ec_get_y" [.var "balance_commit"]) (.var "balance_commit_y"),
  .constrainInstance (.var "balance_commit_y"),
  .assign "tc_circuit" (.op "poseidon_hash" [.var "DOMAIN_TOK_COMMIT", .var "asset_id", .var "token_blind"]),
  .constrainEq (.var "tc_circuit") (.var "token_commit"),
  .constrainInstance (.var "token_commit"),
  .assign "tb_circuit" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainEq (.var "tb_circuit") (.var "tx_binding"),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "balance")
]


/-- **The property fails** for `src/contract/purse/proof/balance.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem purse_balance_has_a_free_instance :
    ¬ NoFreeInstance purse_balance_held purse_balance_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/purse/proof/deposit.zk` — 9 exposure(s). -/

def purse_deposit_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "deposit_amount", "deposit_blind", "expected_root", "leaf_pos", "new_balance", "new_balance_blind", "new_commit_x", "new_commit_y", "new_leaf", "nullifier", "old_balance", "old_balance_blind", "old_commit_x", "old_commit_y", "owner_pub", "owner_secret", "path", "purse_id", "state_nonce", "tx_binding", "tx_commitment", "tx_nonce"]


def purse_deposit_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_MERKLE_LEAF" (.op "witness_base" [.lit 5]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "derived_owner" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "owner_secret"]),
  .constrainEq (.var "derived_owner") (.var "owner_pub"),
  .assign "nf_circuit" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "purse_id", .var "state_nonce"]),
  .constrainEq (.var "nf_circuit") (.var "nullifier"),
  .constrainInstance (.var "nullifier"),
  .assign "old_leaf" (.op "poseidon_hash" [.var "DOMAIN_MERKLE_LEAF", .var "purse_id", .var "old_balance", .var "state_nonce"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "old_leaf"]),
  .constrainEq (.var "root") (.var "expected_root"),
  .constrainInstance (.var "expected_root"),
  .assign "old_vcv" (.op "ec_mul_short" [.var "old_balance", .var "VALUE_COMMIT_VALUE"]),
  .assign "old_vcr" (.op "ec_mul" [.var "old_balance_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "old_commit" (.op "ec_add" [.var "old_vcv", .var "old_vcr"]),
  .assign "dep_vcv" (.op "ec_mul_short" [.var "deposit_amount", .var "VALUE_COMMIT_VALUE"]),
  .assign "dep_vcr" (.op "ec_mul" [.var "deposit_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "dep_commit" (.op "ec_add" [.var "dep_vcv", .var "dep_vcr"]),
  .assign "new_vcv" (.op "ec_mul_short" [.var "new_balance", .var "VALUE_COMMIT_VALUE"]),
  .assign "new_vcr" (.op "ec_mul" [.var "new_balance_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "new_commit" (.op "ec_add" [.var "new_vcv", .var "new_vcr"]),
  .assign "sum_commit" (.op "ec_add" [.var "old_commit", .var "dep_commit"]),
  .constrainEq (.var "sum_commit") (.var "new_commit"),
  .constrainEq (.op "ec_get_x" [.var "old_commit"]) (.var "old_commit_x"),
  .constrainInstance (.var "old_commit_x"),
  .constrainEq (.op "ec_get_y" [.var "old_commit"]) (.var "old_commit_y"),
  .constrainInstance (.var "old_commit_y"),
  .constrainEq (.op "ec_get_x" [.var "new_commit"]) (.var "new_commit_x"),
  .constrainInstance (.var "new_commit_x"),
  .constrainEq (.op "ec_get_y" [.var "new_commit"]) (.var "new_commit_y"),
  .constrainInstance (.var "new_commit_y"),
  .assign "computed_new" (.op "base_add" [.var "old_balance", .var "deposit_amount"]),
  .constrainEq (.var "computed_new") (.var "new_balance"),
  .assign "new_nonce" (.op "base_add" [.var "state_nonce", .var "ONE"]),
  .assign "new_leaf_circuit" (.op "poseidon_hash" [.var "DOMAIN_MERKLE_LEAF", .var "purse_id", .var "new_balance", .var "new_nonce"]),
  .constrainEq (.var "new_leaf_circuit") (.var "new_leaf"),
  .constrainInstance (.var "new_leaf"),
  .assign "tb_circuit" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainEq (.var "tb_circuit") (.var "tx_binding"),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "old_balance"),
  .rangeCheck 64 (.var "deposit_amount"),
  .rangeCheck 64 (.var "new_balance")
]


/-- **The property fails** for `src/contract/purse/proof/deposit.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem purse_deposit_has_a_free_instance :
    ¬ NoFreeInstance purse_deposit_held purse_deposit_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/purse/proof/withdraw.zk` — 9 exposure(s). -/

def purse_withdraw_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "expected_root", "leaf_pos", "new_balance", "new_balance_blind", "new_commit_x", "new_commit_y", "new_leaf", "nullifier", "old_balance", "old_balance_blind", "old_commit_x", "old_commit_y", "owner_pub", "owner_secret", "path", "purse_id", "state_nonce", "tx_binding", "tx_commitment", "tx_nonce", "withdraw_amount", "withdraw_blind"]


def purse_withdraw_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_MERKLE_LEAF" (.op "witness_base" [.lit 5]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "ZERO" (.op "witness_base" [.lit 0]),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "derived_owner" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "owner_secret"]),
  .constrainEq (.var "derived_owner") (.var "owner_pub"),
  .assign "is_within" (.op "less_than_or_equal" [.var "withdraw_amount", .var "old_balance"]),
  .constrainEq (.var "is_within") (.var "ONE"),
  .assign "nf_circuit" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "purse_id", .var "state_nonce"]),
  .constrainEq (.var "nf_circuit") (.var "nullifier"),
  .constrainInstance (.var "nullifier"),
  .assign "old_leaf" (.op "poseidon_hash" [.var "DOMAIN_MERKLE_LEAF", .var "purse_id", .var "old_balance", .var "state_nonce"]),
  .assign "root" (.op "merkle_root" [.var "leaf_pos", .var "path", .var "old_leaf"]),
  .constrainEq (.var "root") (.var "expected_root"),
  .constrainInstance (.var "expected_root"),
  .assign "old_vcv" (.op "ec_mul_short" [.var "old_balance", .var "VALUE_COMMIT_VALUE"]),
  .assign "old_vcr" (.op "ec_mul" [.var "old_balance_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "old_commit" (.op "ec_add" [.var "old_vcv", .var "old_vcr"]),
  .assign "new_vcv" (.op "ec_mul_short" [.var "new_balance", .var "VALUE_COMMIT_VALUE"]),
  .assign "new_vcr" (.op "ec_mul" [.var "new_balance_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "new_commit" (.op "ec_add" [.var "new_vcv", .var "new_vcr"]),
  .assign "wdr_vcv" (.op "ec_mul_short" [.var "withdraw_amount", .var "VALUE_COMMIT_VALUE"]),
  .assign "wdr_vcr" (.op "ec_mul" [.var "withdraw_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "withdraw_commit" (.op "ec_add" [.var "wdr_vcv", .var "wdr_vcr"]),
  .assign "reconstructed" (.op "ec_add" [.var "new_commit", .var "withdraw_commit"]),
  .constrainEq (.var "reconstructed") (.var "old_commit"),
  .constrainEq (.op "ec_get_x" [.var "old_commit"]) (.var "old_commit_x"),
  .constrainInstance (.var "old_commit_x"),
  .constrainEq (.op "ec_get_y" [.var "old_commit"]) (.var "old_commit_y"),
  .constrainInstance (.var "old_commit_y"),
  .constrainEq (.op "ec_get_x" [.var "new_commit"]) (.var "new_commit_x"),
  .constrainInstance (.var "new_commit_x"),
  .constrainEq (.op "ec_get_y" [.var "new_commit"]) (.var "new_commit_y"),
  .constrainInstance (.var "new_commit_y"),
  .assign "computed_new" (.op "base_sub" [.var "old_balance", .var "withdraw_amount"]),
  .constrainEq (.var "computed_new") (.var "new_balance"),
  .assign "new_nonce" (.op "base_add" [.var "state_nonce", .var "ONE"]),
  .assign "new_leaf_circuit" (.op "poseidon_hash" [.var "DOMAIN_MERKLE_LEAF", .var "purse_id", .var "new_balance", .var "new_nonce"]),
  .constrainEq (.var "new_leaf_circuit") (.var "new_leaf"),
  .constrainInstance (.var "new_leaf"),
  .assign "tb_circuit" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainEq (.var "tb_circuit") (.var "tx_binding"),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .rangeCheck 64 (.var "old_balance"),
  .rangeCheck 64 (.var "withdraw_amount"),
  .rangeCheck 64 (.var "new_balance")
]


/-- **The property fails** for `src/contract/purse/proof/withdraw.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem purse_withdraw_has_a_free_instance :
    ¬ NoFreeInstance purse_withdraw_held purse_withdraw_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/relayer_endowment/proof/claim_fees.zk` — 3 exposure(s). -/

def relayer_endowment_claim_fees_held : List Name := ["backer_pub_x", "backer_pub_y", "deployment_id", "fee_share", "nonce", "tx_binding", "tx_commitment", "tx_nonce"]


def relayer_endowment_claim_fees_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_claim_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "deployment_id", .var "backer_pub_x", .var "backer_pub_y", .var "fee_share", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_claim_id")
]


/-- **The property fails** for `src/contract/relayer_endowment/proof/claim_fees.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem relayer_endowment_claim_fees_has_a_free_instance :
    ¬ NoFreeInstance relayer_endowment_claim_fees_held relayer_endowment_claim_fees_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/relayer_endowment/proof/deploy_capital.zk` — 5 exposure(s). -/

def relayer_endowment_deploy_capital_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "backer_pub_x", "backer_pub_y", "deploy_amount", "endowment_id", "nonce", "tx_binding", "tx_commitment", "tx_nonce", "value_blind"]


def relayer_endowment_deploy_capital_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_deployment_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "endowment_id", .var "backer_pub_x", .var "backer_pub_y", .var "deploy_amount", .var "nonce"]),
  .constrainInstance (.var "derived_deployment_id"),
  .assign "vcv" (.op "ec_mul_short" [.var "deploy_amount", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"])
]


/-- **The property fails** for `src/contract/relayer_endowment/proof/deploy_capital.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem relayer_endowment_deploy_capital_has_a_free_instance :
    ¬ NoFreeInstance relayer_endowment_deploy_capital_held relayer_endowment_deploy_capital_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/relayer_endowment/proof/initialize.zk` — 3 exposure(s). -/

def relayer_endowment_initialize_held : List Name := ["config_hash", "nonce", "relayer_pub_x", "relayer_pub_y", "tx_binding", "tx_commitment", "tx_nonce"]


def relayer_endowment_initialize_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_endowment_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "relayer_pub_x", .var "relayer_pub_y", .var "config_hash", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_endowment_id")
]


/-- **The property fails** for `src/contract/relayer_endowment/proof/initialize.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem relayer_endowment_initialize_has_a_free_instance :
    ¬ NoFreeInstance relayer_endowment_initialize_held relayer_endowment_initialize_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/roulette/proof/house_close.zk` — 6 exposure(s). -/

def roulette_house_close_held : List Name := ["NULLIFIER_K", "close_nullifier", "house_pub_x", "house_pub_y", "house_secret", "table_id", "tx_binding", "tx_commitment", "tx_nonce"]


def roulette_house_close_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 2]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "house_pub" (.op "ec_mul_base" [.var "house_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "house_pub"]) (.var "house_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "house_pub"]) (.var "house_pub_y"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "table_id", .var "house_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "close_nullifier"),
  .constrainInstance (.var "table_id"),
  .constrainInstance (.var "house_pub_x"),
  .constrainInstance (.var "house_pub_y"),
  .constrainInstance (.var "close_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/roulette/proof/house_close.zk`: its first undetermined exposure is
    `.var "table_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem roulette_house_close_has_a_free_instance :
    ¬ NoFreeInstance roulette_house_close_held roulette_house_close_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/roulette/proof/place_bet.zk` — 2 exposure(s). -/

def roulette_place_bet_held : List Name := ["amount", "bet_id", "bet_type", "nonce", "nullifier", "player_pub_x", "player_pub_y", "table_id", "tx_binding", "tx_commitment", "tx_nonce"]


def roulette_place_bet_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_bet_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "table_id", .var "player_pub_x", .var "player_pub_y", .var "amount"]),
  .constrainEq (.var "derived_bet_id") (.var "bet_id"),
  .assign "derived_nullifier" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "bet_id", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainEq (.var "derived_nullifier") (.var "nullifier")
]


/-- **The property fails** for `src/contract/roulette/proof/place_bet.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem roulette_place_bet_has_a_free_instance :
    ¬ NoFreeInstance roulette_place_bet_held roulette_place_bet_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/roulette/proof/settle_bet.zk` — 3 exposure(s). -/

def roulette_settle_bet_held : List Name := ["bet_id", "payout", "table_id", "tx_binding", "tx_commitment", "tx_nonce", "won"]


def roulette_settle_bet_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "payout")
]


/-- **The property fails** for `src/contract/roulette/proof/settle_bet.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem roulette_settle_bet_has_a_free_instance :
    ¬ NoFreeInstance roulette_settle_bet_held roulette_settle_bet_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/roulette/proof/spin_wheel.zk` — 6 exposure(s). -/

def roulette_spin_wheel_held : List Name := ["NULLIFIER_K", "house_pub_x", "house_pub_y", "house_secret", "spin_nullifier", "table_id", "tx_binding", "tx_commitment", "tx_nonce"]


def roulette_spin_wheel_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "house_pub" (.op "ec_mul_base" [.var "house_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "house_pub"]) (.var "house_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "house_pub"]) (.var "house_pub_y"),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "table_id", .var "house_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "spin_nullifier"),
  .constrainInstance (.var "table_id"),
  .constrainInstance (.var "house_pub_x"),
  .constrainInstance (.var "house_pub_y"),
  .constrainInstance (.var "spin_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/roulette/proof/spin_wheel.zk`: its first undetermined exposure is
    `.var "table_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem roulette_spin_wheel_has_a_free_instance :
    ¬ NoFreeInstance roulette_spin_wheel_held roulette_spin_wheel_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/slot/proof/commit_bet.zk` — 5 exposure(s). -/

def slot_commit_bet_held : List Name := ["VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "bet_value", "blind", "paylines", "player_pub_x", "player_pub_y", "secret_nonce", "tx_binding", "tx_commitment", "tx_nonce", "value_blind"]


def slot_commit_bet_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "spin_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "player_pub_x", .var "player_pub_y", .var "bet_value", .var "paylines", .var "secret_nonce", .var "blind", .var "asset_id"]),
  .constrainInstance (.var "spin_id"),
  .assign "vcv" (.op "ec_mul_short" [.var "bet_value", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .constrainInstance (.op "ec_get_x" [.var "value_commit"]),
  .constrainInstance (.op "ec_get_y" [.var "value_commit"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/slot/proof/commit_bet.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem slot_commit_bet_has_a_free_instance :
    ¬ NoFreeInstance slot_commit_bet_held slot_commit_bet_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/slot/proof/reveal_spin.zk` — 4 exposure(s). -/

def slot_reveal_spin_held : List Name := ["secret_nonce", "secret_nonce_commit", "spin_id", "tx_binding", "tx_commitment", "tx_nonce"]


def slot_reveal_spin_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "computed_commit" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "secret_nonce"]),
  .constrainEq (.var "computed_commit") (.var "secret_nonce_commit"),
  .constrainInstance (.var "spin_id"),
  .constrainInstance (.var "secret_nonce_commit"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/slot/proof/reveal_spin.zk`: its first undetermined exposure is
    `.var "spin_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem slot_reveal_spin_has_a_free_instance :
    ¬ NoFreeInstance slot_reveal_spin_held slot_reveal_spin_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/slot/proof/settle_bet.zk` — 4 exposure(s). -/

def slot_settle_bet_held : List Name := ["asset_id", "bet_value", "blind", "match_count", "paylines", "payout", "player_pub_x", "player_pub_y", "position_0", "position_1", "position_2", "secret_nonce", "tx_binding", "tx_commitment", "tx_nonce"]


def slot_settle_bet_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_spin_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "player_pub_x", .var "player_pub_y", .var "bet_value", .var "paylines", .var "secret_nonce", .var "blind", .var "asset_id"]),
  .constrainInstance (.var "derived_spin_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "payout")
]


/-- **The property fails** for `src/contract/slot/proof/settle_bet.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem slot_settle_bet_has_a_free_instance :
    ¬ NoFreeInstance slot_settle_bet_held slot_settle_bet_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/accrue_interest.zk` — 5 exposure(s). -/

def stablecoin_accrue_interest_held : List Name := ["NULLIFIER_K", "accumulator_pub_x", "accumulator_pub_y", "accumulator_secret", "interest_amount", "new_total_debt", "old_total_debt", "rate_per_second", "time_elapsed", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_accrue_interest_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "accumulator_public" (.op "ec_mul_base" [.var "accumulator_secret", .var "NULLIFIER_K"]),
  .assign "accumulator_public_x" (.op "ec_get_x" [.var "accumulator_public"]),
  .assign "accumulator_public_y" (.op "ec_get_y" [.var "accumulator_public"]),
  .constrainEq (.var "accumulator_public_x") (.var "accumulator_pub_x"),
  .constrainEq (.var "accumulator_public_y") (.var "accumulator_pub_y"),
  .constrainInstance (.var "accumulator_pub_x"),
  .constrainInstance (.var "accumulator_pub_y"),
  .constrainInstance (.var "old_total_debt"),
  .rangeCheck 64 (.var "old_total_debt"),
  .rangeCheck 64 (.var "new_total_debt"),
  .rangeCheck 64 (.var "rate_per_second"),
  .rangeCheck 64 (.var "time_elapsed"),
  .rangeCheck 64 (.var "interest_amount"),
  .assign "DENOM" (.op "witness_base" [.lit 315360000000]),
  .assign "debt_times_rate" (.op "base_mul" [.var "old_total_debt", .var "rate_per_second"]),
  .assign "debt_rate_time" (.op "base_mul" [.var "debt_times_rate", .var "time_elapsed"]),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "interest_times_denom" (.op "base_mul" [.var "interest_amount", .var "DENOM"]),
  .assign "lte_check" (.op "less_than_or_equal" [.var "interest_times_denom", .var "debt_rate_time"]),
  .constrainEq (.var "lte_check") (.var "ONE"),
  .assign "int_plus_one" (.op "base_add" [.var "interest_amount", .var "ONE"]),
  .assign "upper" (.op "base_mul" [.var "int_plus_one", .var "DENOM"]),
  .assign "new_debt_check" (.op "base_add" [.var "old_total_debt", .var "interest_amount"]),
  .constrainEq (.var "new_debt_check") (.var "new_total_debt"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/stablecoin/proof/accrue_interest.zk`: its first undetermined exposure is
    `.var "old_total_debt"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem stablecoin_accrue_interest_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_accrue_interest_held stablecoin_accrue_interest_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/add_collateral.zk` — 4 exposure(s). -/

def stablecoin_add_collateral_held : List Name := ["added_collateral", "collateral_blind", "collateral_type", "debt_blind", "leaf_index", "merkle_proof_0", "merkle_proof_1", "merkle_proof_2", "merkle_proof_3", "old_collateral", "old_debt", "owner_pub", "owner_secret", "position_commitment", "position_nullifier", "position_root", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_add_collateral_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "owner_pub_check" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "owner_secret"]),
  .constrainEq (.var "owner_pub_check") (.var "owner_pub"),
  .assign "new_collateral" (.op "base_add" [.var "old_collateral", .var "added_collateral"]),
  .rangeCheck 64 (.var "new_collateral"),
  .rangeCheck 64 (.var "added_collateral"),
  .assign "collateral_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "new_collateral", .var "collateral_blind"]),
  .assign "debt_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "old_debt", .var "debt_blind"]),
  .assign "position_check" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "collateral_commit", .var "debt_commit", .var "owner_pub", .var "collateral_type"]),
  .constrainInstance (.var "position_check"),
  .constrainEq (.var "position_check") (.var "position_commitment"),
  .assign "nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "position_commitment"]),
  .constrainInstance (.var "nullifier_check"),
  .constrainEq (.var "nullifier_check") (.var "position_nullifier"),
  .assign "two_times_debt" (.op "base_add" [.var "old_debt", .var "old_debt"]),
  .rangeCheck 64 (.var "two_times_debt"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "is_lte" (.op "less_than_or_equal" [.var "two_times_debt", .var "new_collateral"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainEq (.var "is_lte") (.var "ONE")
]


/-- **The property fails** for `src/contract/stablecoin/proof/add_collateral.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem stablecoin_add_collateral_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_add_collateral_held stablecoin_add_collateral_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/governance_report.zk` — 9 exposure(s). -/

def stablecoin_governance_report_held : List Name := ["NULLIFIER_K", "collateral_ratio_bps", "interest_accrued", "outstanding", "rate_per_second", "reporter_pub_x", "reporter_pub_y", "reporter_secret", "time_elapsed", "total_collateral", "total_debt", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_governance_report_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "BPS" (.op "witness_base" [.lit 10000]),
  .assign "reporter_public" (.op "ec_mul_base" [.var "reporter_secret", .var "NULLIFIER_K"]),
  .assign "reporter_public_x" (.op "ec_get_x" [.var "reporter_public"]),
  .assign "reporter_public_y" (.op "ec_get_y" [.var "reporter_public"]),
  .constrainEq (.var "reporter_public_x") (.var "reporter_pub_x"),
  .constrainEq (.var "reporter_public_y") (.var "reporter_pub_y"),
  .constrainInstance (.var "reporter_pub_x"),
  .constrainInstance (.var "reporter_pub_y"),
  .rangeCheck 64 (.var "total_collateral"),
  .rangeCheck 64 (.var "total_debt"),
  .rangeCheck 64 (.var "outstanding"),
  .rangeCheck 64 (.var "rate_per_second"),
  .rangeCheck 64 (.var "time_elapsed"),
  .rangeCheck 64 (.var "collateral_ratio_bps"),
  .rangeCheck 64 (.var "interest_accrued"),
  .constrainInstance (.var "total_collateral"),
  .constrainInstance (.var "total_debt"),
  .constrainInstance (.var "outstanding"),
  .assign "crb_times_outstanding" (.op "base_mul" [.var "collateral_ratio_bps", .var "outstanding"]),
  .assign "coll_times_bps" (.op "base_mul" [.var "total_collateral", .var "BPS"]),
  .assign "lte_check" (.op "less_than_or_equal" [.var "crb_times_outstanding", .var "coll_times_bps"]),
  .constrainEq (.var "lte_check") (.var "ONE"),
  .assign "crb_plus_one" (.op "base_add" [.var "collateral_ratio_bps", .var "ONE"]),
  .assign "upper" (.op "base_mul" [.var "crb_plus_one", .var "outstanding"]),
  .constrainInstance (.var "collateral_ratio_bps"),
  .assign "debt_times_rate" (.op "base_mul" [.var "total_debt", .var "rate_per_second"]),
  .assign "debt_rate_time" (.op "base_mul" [.var "debt_times_rate", .var "time_elapsed"]),
  .assign "DENOM" (.op "witness_base" [.lit 315360000000]),
  .assign "interest_times_denom" (.op "base_mul" [.var "interest_accrued", .var "DENOM"]),
  .assign "less_than_or_equal_result" (.op "less_than_or_equal" [.var "interest_times_denom", .var "debt_rate_time"]),
  .constrainEq (.var "less_than_or_equal_result") (.var "ONE"),
  .assign "interest_plus_one" (.op "base_add" [.var "interest_accrued", .var "ONE"]),
  .assign "interest_upper" (.op "base_mul" [.var "interest_plus_one", .var "DENOM"]),
  .constrainInstance (.var "interest_accrued"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/stablecoin/proof/governance_report.zk`: its first undetermined exposure is
    `.var "total_collateral"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem stablecoin_governance_report_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_governance_report_held stablecoin_governance_report_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/init.zk` — 3 exposure(s). -/

def stablecoin_init_held : List Name := ["contract_salt", "deployer_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_init_stmts : List Stmt :=
[
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "deployer_auth" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "deployer_secret", .var "contract_salt"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "deployer_auth")
]


/-- **The property fails** for `src/contract/stablecoin/proof/init.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem stablecoin_init_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_init_held stablecoin_init_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/liquidate.zk` — 5 exposure(s). -/

def stablecoin_liquidate_held : List Name := ["collateral_amount", "collateral_blind", "collateral_type", "current_price", "debt_amount", "debt_blind", "liquidation_penalty", "liquidator_reward", "new_commitment", "old_commitment", "owner_secret", "position_nullifier", "position_root", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_liquidate_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "owner_pub" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "owner_secret"]),
  .assign "nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "old_commitment"]),
  .constrainInstance (.var "nullifier_check"),
  .constrainEq (.var "nullifier_check") (.var "position_nullifier"),
  .assign "old_collateral_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "collateral_amount", .var "collateral_blind"]),
  .assign "old_debt_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "debt_amount", .var "debt_blind"]),
  .assign "old_position" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "old_collateral_commit", .var "old_debt_commit", .var "owner_pub", .var "collateral_type"]),
  .constrainEq (.var "old_position") (.var "old_commitment"),
  .constrainInstance (.var "old_position"),
  .assign "new_collateral" (.op "base_sub" [.var "collateral_amount", .var "liquidator_reward"]),
  .assign "new_collateral_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "new_collateral", .var "collateral_blind"]),
  .assign "new_debt_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "debt_amount", .var "debt_blind"]),
  .assign "new_position" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "new_collateral_commit", .var "new_debt_commit", .var "owner_pub", .var "collateral_type"]),
  .constrainEq (.var "new_position") (.var "new_commitment"),
  .constrainInstance (.var "new_position"),
  .assign "debt_value" (.op "base_mul" [.var "debt_amount", .var "current_price"]),
  .assign "collateral_value" (.op "base_mul" [.var "collateral_amount", .op "witness_base" [.lit 10000]]),
  .assign "LIQ_THRESHOLD" (.op "witness_base" [.lit 15000]),
  .assign "threshold_debt" (.op "base_mul" [.var "debt_value", .var "LIQ_THRESHOLD"]),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "collat_plus_one" (.op "base_add" [.var "collateral_value", .var "ONE"]),
  .assign "is_undercollateralized" (.op "less_than_or_equal" [.var "collat_plus_one", .var "threshold_debt"]),
  .constrainEq (.var "is_undercollateralized") (.var "ONE"),
  .rangeCheck 64 (.var "collateral_amount"),
  .rangeCheck 64 (.var "debt_amount"),
  .rangeCheck 64 (.var "liquidator_reward"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "is_lte" (.op "less_than_or_equal" [.var "liquidator_reward", .var "collateral_amount"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainEq (.var "is_lte") (.var "ONE")
]


/-- **The property fails** for `src/contract/stablecoin/proof/liquidate.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem stablecoin_liquidate_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_liquidate_held stablecoin_liquidate_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/mint_stable.zk` — 5 exposure(s). -/

def stablecoin_mint_stable_held : List Name := ["collateral_blind", "collateral_type", "debt_blind", "mint_amount", "new_collateral", "new_commitment", "new_debt", "old_collateral", "old_commitment", "old_debt", "owner_secret", "position_nullifier", "position_root", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_mint_stable_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "owner_pub" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "owner_secret"]),
  .assign "nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "old_commitment"]),
  .constrainInstance (.var "nullifier_check"),
  .constrainEq (.var "nullifier_check") (.var "position_nullifier"),
  .assign "mint_check" (.op "base_add" [.var "old_debt", .var "mint_amount"]),
  .constrainEq (.var "mint_check") (.var "new_debt"),
  .constrainEq (.var "old_collateral") (.var "new_collateral"),
  .assign "old_collateral_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "old_collateral", .var "collateral_blind"]),
  .assign "old_debt_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "old_debt", .var "debt_blind"]),
  .assign "old_position" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "old_collateral_commit", .var "old_debt_commit", .var "owner_pub", .var "collateral_type"]),
  .constrainEq (.var "old_position") (.var "old_commitment"),
  .constrainInstance (.var "old_position"),
  .assign "new_collateral_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "new_collateral", .var "collateral_blind"]),
  .assign "new_debt_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "new_debt", .var "debt_blind"]),
  .assign "new_position" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "new_collateral_commit", .var "new_debt_commit", .var "owner_pub", .var "collateral_type"]),
  .constrainEq (.var "new_position") (.var "new_commitment"),
  .constrainInstance (.var "new_position"),
  .rangeCheck 64 (.var "mint_amount"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/stablecoin/proof/mint_stable.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem stablecoin_mint_stable_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_mint_stable_held stablecoin_mint_stable_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/open_position.zk` — 4 exposure(s). -/

def stablecoin_open_position_held : List Name := ["collateral_amount", "collateral_blind", "collateral_type", "debt_amount", "debt_blind", "leaf_index", "merkle_proof_0", "merkle_proof_1", "merkle_proof_2", "merkle_proof_3", "owner_pub", "owner_secret", "position_commitment", "position_nullifier", "position_root", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_open_position_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "owner_pub_check" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "owner_secret"]),
  .constrainEq (.var "owner_pub_check") (.var "owner_pub"),
  .assign "collateral_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "collateral_amount", .var "collateral_blind"]),
  .assign "debt_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "debt_amount", .var "debt_blind"]),
  .assign "position_check" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "collateral_commit", .var "debt_commit", .var "owner_pub", .var "collateral_type"]),
  .constrainInstance (.var "position_check"),
  .constrainEq (.var "position_check") (.var "position_commitment"),
  .assign "nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "position_commitment"]),
  .constrainInstance (.var "nullifier_check"),
  .constrainEq (.var "nullifier_check") (.var "position_nullifier"),
  .rangeCheck 64 (.var "collateral_amount"),
  .rangeCheck 64 (.var "debt_amount"),
  .assign "two_times_debt" (.op "base_add" [.var "debt_amount", .var "debt_amount"]),
  .rangeCheck 64 (.var "two_times_debt"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "is_lte" (.op "less_than_or_equal" [.var "two_times_debt", .var "collateral_amount"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainEq (.var "is_lte") (.var "ONE")
]


/-- **The property fails** for `src/contract/stablecoin/proof/open_position.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem stablecoin_open_position_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_open_position_held stablecoin_open_position_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/remove_collateral.zk` — 4 exposure(s). -/

def stablecoin_remove_collateral_held : List Name := ["collateral_blind", "collateral_type", "debt_blind", "leaf_index", "merkle_proof_0", "merkle_proof_1", "merkle_proof_2", "merkle_proof_3", "old_collateral", "old_debt", "owner_pub", "owner_secret", "position_commitment", "position_nullifier", "position_root", "removed_collateral", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_remove_collateral_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "owner_pub_check" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "owner_secret"]),
  .constrainEq (.var "owner_pub_check") (.var "owner_pub"),
  .assign "new_collateral" (.op "base_sub" [.var "old_collateral", .var "removed_collateral"]),
  .rangeCheck 64 (.var "new_collateral"),
  .rangeCheck 64 (.var "removed_collateral"),
  .assign "collateral_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "new_collateral", .var "collateral_blind"]),
  .assign "debt_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "old_debt", .var "debt_blind"]),
  .assign "position_check" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "collateral_commit", .var "debt_commit", .var "owner_pub", .var "collateral_type"]),
  .constrainInstance (.var "position_check"),
  .constrainEq (.var "position_check") (.var "position_commitment"),
  .assign "nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "position_commitment"]),
  .constrainInstance (.var "nullifier_check"),
  .constrainEq (.var "nullifier_check") (.var "position_nullifier"),
  .assign "two_times_debt" (.op "base_add" [.var "old_debt", .var "old_debt"]),
  .rangeCheck 64 (.var "two_times_debt"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "is_lte" (.op "less_than_or_equal" [.var "two_times_debt", .var "new_collateral"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainEq (.var "is_lte") (.var "ONE")
]


/-- **The property fails** for `src/contract/stablecoin/proof/remove_collateral.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem stablecoin_remove_collateral_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_remove_collateral_held stablecoin_remove_collateral_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/repay_stable.zk` — 4 exposure(s). -/

def stablecoin_repay_stable_held : List Name := ["collateral_blind", "collateral_type", "debt_blind", "leaf_index", "merkle_proof_0", "merkle_proof_1", "merkle_proof_2", "merkle_proof_3", "old_collateral", "old_debt", "owner_pub", "owner_secret", "position_commitment", "position_nullifier", "position_root", "repaid_debt", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_repay_stable_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "DOMAIN_SIGNATURE_SECRET" (.op "witness_base" [.lit 7]),
  .assign "owner_pub_check" (.op "poseidon_hash" [.var "DOMAIN_SIGNATURE_SECRET", .var "owner_secret"]),
  .constrainEq (.var "owner_pub_check") (.var "owner_pub"),
  .assign "new_debt" (.op "base_sub" [.var "old_debt", .var "repaid_debt"]),
  .rangeCheck 64 (.var "new_debt"),
  .rangeCheck 64 (.var "repaid_debt"),
  .assign "collateral_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "old_collateral", .var "collateral_blind"]),
  .assign "debt_commit" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "new_debt", .var "debt_blind"]),
  .assign "position_check" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "collateral_commit", .var "debt_commit", .var "owner_pub", .var "collateral_type"]),
  .constrainInstance (.var "position_check"),
  .constrainEq (.var "position_check") (.var "position_commitment"),
  .assign "nullifier_check" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "owner_secret", .var "position_commitment"]),
  .constrainInstance (.var "nullifier_check"),
  .constrainEq (.var "nullifier_check") (.var "position_nullifier"),
  .assign "two_times_debt" (.op "base_add" [.var "new_debt", .var "new_debt"]),
  .rangeCheck 64 (.var "two_times_debt"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .assign "is_lte" (.op "less_than_or_equal" [.var "two_times_debt", .var "old_collateral"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainEq (.var "is_lte") (.var "ONE")
]


/-- **The property fails** for `src/contract/stablecoin/proof/repay_stable.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem stablecoin_repay_stable_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_repay_stable_held stablecoin_repay_stable_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/stablecoin/proof/update_config.zk` — 5 exposure(s). -/

def stablecoin_update_config_held : List Name := ["NULLIFIER_K", "config_nullifier", "gov_pub_x", "gov_pub_y", "gov_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def stablecoin_update_config_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "gov_pub" (.op "ec_mul_base" [.var "gov_secret", .var "NULLIFIER_K"]),
  .constrainEq (.op "ec_get_x" [.var "gov_pub"]) (.var "gov_pub_x"),
  .constrainEq (.op "ec_get_y" [.var "gov_pub"]) (.var "gov_pub_y"),
  .assign "computed" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "gov_pub_x", .var "gov_pub_y", .var "gov_secret"]),
  .constrainEq (.var "computed") (.var "config_nullifier"),
  .constrainInstance (.var "gov_pub_x"),
  .constrainInstance (.var "gov_pub_y"),
  .constrainInstance (.var "config_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/stablecoin/proof/update_config.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem stablecoin_update_config_has_a_free_instance :
    ¬ NoFreeInstance stablecoin_update_config_held stablecoin_update_config_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/subscription/proof/cancel.zk` — 4 exposure(s). -/

def subscription_cancel_held : List Name := ["spent_nullifier", "subscriber_secret", "subscription_id", "tx_binding", "tx_commitment", "tx_nonce"]


def subscription_cancel_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "subscription_id", .var "subscriber_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "spent_nullifier"),
  .constrainInstance (.var "subscription_id"),
  .constrainInstance (.var "spent_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/subscription/proof/cancel.zk`: its first undetermined exposure is
    `.var "subscription_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem subscription_cancel_has_a_free_instance :
    ¬ NoFreeInstance subscription_cancel_held subscription_cancel_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/subscription/proof/renew.zk` — 4 exposure(s). -/

def subscription_renew_held : List Name := ["spent_nullifier", "subscriber_secret", "subscription_id", "tx_binding", "tx_commitment", "tx_nonce"]


def subscription_renew_stmts : List Stmt :=
[
  .assign "DOMAIN_NULLIFIER" (.op "witness_base" [.lit 1]),
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "computed_nullifier" (.op "poseidon_hash" [.var "DOMAIN_NULLIFIER", .var "subscription_id", .var "subscriber_secret"]),
  .constrainEq (.var "computed_nullifier") (.var "spent_nullifier"),
  .constrainInstance (.var "subscription_id"),
  .constrainInstance (.var "spent_nullifier"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/subscription/proof/renew.zk`: its first undetermined exposure is
    `.var "subscription_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem subscription_renew_has_a_free_instance :
    ¬ NoFreeInstance subscription_renew_held subscription_renew_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/subscription/proof/subscribe.zk` — 3 exposure(s). -/

def subscription_subscribe_held : List Name := ["NULLIFIER_K", "VALUE_COMMIT_RANDOM", "VALUE_COMMIT_VALUE", "asset_id", "deposit", "lock_until_block", "nonce", "plan_id", "subscriber_pub_x", "subscriber_pub_y", "subscriber_secret", "subscription_id", "tx_binding", "tx_commitment", "tx_nonce", "value_blind", "value_commit_x", "value_commit_y"]


def subscription_subscribe_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_subscriber_pub" (.op "ec_mul_base" [.var "subscriber_secret", .var "NULLIFIER_K"]),
  .assign "derived_sub_x" (.op "ec_get_x" [.var "derived_subscriber_pub"]),
  .assign "derived_sub_y" (.op "ec_get_y" [.var "derived_subscriber_pub"]),
  .constrainEq (.var "derived_sub_x") (.var "subscriber_pub_x"),
  .constrainEq (.var "derived_sub_y") (.var "subscriber_pub_y"),
  .assign "derived_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "subscriber_pub_x", .var "subscriber_pub_y", .var "plan_id", .var "deposit", .var "asset_id", .var "lock_until_block", .var "subscriber_secret", .var "nonce"]),
  .constrainEq (.var "derived_id") (.var "subscription_id"),
  .assign "vcv" (.op "ec_mul_short" [.var "deposit", .var "VALUE_COMMIT_VALUE"]),
  .assign "vcr" (.op "ec_mul" [.var "value_blind", .var "VALUE_COMMIT_RANDOM"]),
  .assign "value_commit_computed" (.op "ec_add" [.var "vcv", .var "vcr"]),
  .assign "computed_x" (.op "ec_get_x" [.var "value_commit_computed"]),
  .assign "computed_y" (.op "ec_get_y" [.var "value_commit_computed"]),
  .constrainEq (.var "computed_x") (.var "value_commit_x"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainEq (.var "computed_y") (.var "value_commit_y"),
  .constrainInstance (.var "derived_id")
]


/-- **The property fails** for `src/contract/subscription/proof/subscribe.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem subscription_subscribe_has_a_free_instance :
    ¬ NoFreeInstance subscription_subscribe_held subscription_subscribe_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/subscription/proof/update_usage.zk` — 3 exposure(s). -/

def subscription_update_usage_held : List Name := ["nonce", "subscriber_pub_x", "subscriber_pub_y", "subscription_id", "tx_binding", "tx_commitment", "tx_nonce", "usage_timestamp"]


def subscription_update_usage_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "subscription_id", .var "subscriber_pub_x", .var "subscriber_pub_y", .var "usage_timestamp", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainInstance (.var "derived_id")
]


/-- **The property fails** for `src/contract/subscription/proof/update_usage.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem subscription_update_usage_has_a_free_instance :
    ¬ NoFreeInstance subscription_update_usage_held subscription_update_usage_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/subscription/proof/verify_access.zk` — 3 exposure(s). -/

def subscription_verify_access_held : List Name := ["NULLIFIER_K", "expected_capability", "lock_until_block", "nonce", "plan_id", "subscriber_pub_x", "subscriber_pub_y", "subscriber_secret", "subscription_id", "tx_binding", "tx_commitment", "tx_nonce"]


def subscription_verify_access_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "derived_pub" (.op "ec_mul_base" [.var "subscriber_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "derived_pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "derived_pub"]),
  .constrainEq (.var "derived_pub_x") (.var "subscriber_pub_x"),
  .constrainEq (.var "derived_pub_y") (.var "subscriber_pub_y"),
  .assign "derived_capability" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "subscriber_pub_x", .var "subscriber_pub_y", .var "plan_id", .var "subscription_id", .var "lock_until_block", .var "nonce"]),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce"),
  .constrainEq (.var "derived_capability") (.var "expected_capability"),
  .constrainInstance (.var "derived_capability")
]


/-- **The property fails** for `src/contract/subscription/proof/verify_access.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem subscription_verify_access_has_a_free_instance :
    ¬ NoFreeInstance subscription_verify_access_held subscription_verify_access_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/tender/proof/create_tender.zk` — 4 exposure(s). -/

def tender_create_tender_held : List Name := ["NULLIFIER_K", "requester_pub_x", "requester_pub_y", "requester_secret", "tx_binding", "tx_commitment", "tx_nonce"]


def tender_create_tender_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "requester_pub" (.op "ec_mul_base" [.var "requester_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "requester_pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "requester_pub"]),
  .constrainEq (.var "derived_pub_x") (.var "requester_pub_x"),
  .constrainEq (.var "derived_pub_y") (.var "requester_pub_y"),
  .constrainInstance (.var "requester_pub_x"),
  .constrainInstance (.var "requester_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/tender/proof/create_tender.zk`: its first undetermined exposure is
    `.var "tx_nonce"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem tender_create_tender_has_a_free_instance :
    ¬ NoFreeInstance tender_create_tender_held tender_create_tender_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/tender/proof/reveal_bid.zk` — 7 exposure(s). -/

def tender_reveal_bid_held : List Name := ["NULLIFIER_K", "bid_id", "bidder_pub_x", "bidder_pub_y", "bidder_secret", "revealed_amount", "tender_id", "tx_binding", "tx_commitment", "tx_nonce"]


def tender_reveal_bid_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "bidder_pub" (.op "ec_mul_base" [.var "bidder_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "bidder_pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "bidder_pub"]),
  .constrainEq (.var "derived_pub_x") (.var "bidder_pub_x"),
  .constrainEq (.var "derived_pub_y") (.var "bidder_pub_y"),
  .rangeCheck 64 (.var "revealed_amount"),
  .constrainInstance (.var "tender_id"),
  .constrainInstance (.var "bid_id"),
  .constrainInstance (.var "revealed_amount"),
  .constrainInstance (.var "bidder_pub_x"),
  .constrainInstance (.var "bidder_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/tender/proof/reveal_bid.zk`: its first undetermined exposure is
    `.var "tender_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem tender_reveal_bid_has_a_free_instance :
    ¬ NoFreeInstance tender_reveal_bid_held tender_reveal_bid_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/tender/proof/select_winner.zk` — 6 exposure(s). -/

def tender_select_winner_held : List Name := ["NULLIFIER_K", "requester_pub_x", "requester_pub_y", "requester_secret", "tender_id", "tx_binding", "tx_commitment", "tx_nonce", "winner_bid_id"]


def tender_select_winner_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "requester_pub" (.op "ec_mul_base" [.var "requester_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "requester_pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "requester_pub"]),
  .constrainEq (.var "derived_pub_x") (.var "requester_pub_x"),
  .constrainEq (.var "derived_pub_y") (.var "requester_pub_y"),
  .constrainInstance (.var "tender_id"),
  .constrainInstance (.var "winner_bid_id"),
  .constrainInstance (.var "requester_pub_x"),
  .constrainInstance (.var "requester_pub_y"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/tender/proof/select_winner.zk`: its first undetermined exposure is
    `.var "tender_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `declared-free`, from a host-side justification in `script/circuit_free_instances.txt`. -/
@[axiom_budget 0]
theorem tender_select_winner_has_a_free_instance :
    ¬ NoFreeInstance tender_select_winner_held tender_select_winner_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/tender/proof/submit_bid.zk` — 6 exposure(s). -/

def tender_submit_bid_held : List Name := ["NULLIFIER_K", "amount", "bid_nonce", "bidder_pub_x", "bidder_pub_y", "bidder_secret", "tender_id", "tx_binding", "tx_commitment", "tx_nonce"]


def tender_submit_bid_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "bidder_pub" (.op "ec_mul_base" [.var "bidder_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "bidder_pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "bidder_pub"]),
  .constrainEq (.var "derived_pub_x") (.var "bidder_pub_x"),
  .constrainEq (.var "derived_pub_y") (.var "bidder_pub_y"),
  .rangeCheck 64 (.var "amount"),
  .constrainInstance (.var "bidder_pub_x"),
  .constrainInstance (.var "bidder_pub_y"),
  .assign "bid_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "tender_id", .var "bidder_pub_x", .var "bidder_pub_y", .var "amount", .var "bid_nonce"]),
  .constrainInstance (.var "tender_id"),
  .constrainInstance (.var "bid_id"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/tender/proof/submit_bid.zk`: its first undetermined exposure is
    `.var "tender_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem tender_submit_bid_has_a_free_instance :
    ¬ NoFreeInstance tender_submit_bid_held tender_submit_bid_stmts := by
  unfold NoFreeInstance
  decide


/-- `src/contract/tender/proof/submit_bid_with_capability.zk` — 8 exposure(s). -/

def tender_submit_bid_with_capability_held : List Name := ["NULLIFIER_K", "amount", "bid_nonce", "bidder_pub_x", "bidder_pub_y", "bidder_secret", "capability_predicate_result", "required_capability_id", "tender_id", "tx_binding", "tx_commitment", "tx_nonce"]


def tender_submit_bid_with_capability_stmts : List Stmt :=
[
  .assign "DOMAIN_TX_BINDING" (.op "witness_base" [.lit 3]),
  .assign "DOMAIN_COMMITMENT" (.op "witness_base" [.lit 4]),
  .assign "bidder_pub" (.op "ec_mul_base" [.var "bidder_secret", .var "NULLIFIER_K"]),
  .assign "derived_pub_x" (.op "ec_get_x" [.var "bidder_pub"]),
  .assign "derived_pub_y" (.op "ec_get_y" [.var "bidder_pub"]),
  .constrainEq (.var "derived_pub_x") (.var "bidder_pub_x"),
  .constrainEq (.var "derived_pub_y") (.var "bidder_pub_y"),
  .rangeCheck 64 (.var "amount"),
  .constrainInstance (.var "bidder_pub_x"),
  .constrainInstance (.var "bidder_pub_y"),
  .assign "ONE" (.op "witness_base" [.lit 1]),
  .constrainEq (.var "capability_predicate_result") (.var "ONE"),
  .assign "bid_id" (.op "poseidon_hash" [.var "DOMAIN_COMMITMENT", .var "tender_id", .var "bidder_pub_x", .var "bidder_pub_y", .var "amount", .var "bid_nonce"]),
  .constrainInstance (.var "tender_id"),
  .constrainInstance (.var "bid_id"),
  .constrainInstance (.var "required_capability_id"),
  .constrainInstance (.var "capability_predicate_result"),
  .assign "tx_binding" (.op "poseidon_hash" [.var "DOMAIN_TX_BINDING", .var "tx_commitment", .var "tx_nonce"]),
  .constrainInstance (.var "tx_binding"),
  .constrainInstance (.var "tx_nonce")
]


/-- **The property fails** for `src/contract/tender/proof/submit_bid_with_capability.zk`: its first undetermined exposure is
    `.var "tender_id"`, which the circuit does not bind before exposing.
    The checker resolves it as `redundant` — pinned by another exposed determination, which the model's sequential rule does not follow. -/
@[axiom_budget 0]
theorem tender_submit_bid_with_capability_has_a_free_instance :
    ¬ NoFreeInstance tender_submit_bid_with_capability_held tender_submit_bid_with_capability_stmts := by
  unfold NoFreeInstance
  decide
