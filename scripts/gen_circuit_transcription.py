#!/usr/bin/env python3
"""Emit the Lean transcription of every circuit's statement list.

The bridge `Axioms.NoFreeInstances` needs is `(r, s) ↦ the circuit source`, and a Lean term cannot
read a `.zk` file. This generator supplies the file side of that bridge as *data*: one `List Stmt`
per circuit, in the vocabulary `DarkFi.Circuits.InstanceDerivation` defines, so the property is
checkable per circuit by `decide` rather than by hand.

Three things this does NOT do, each stated rather than implied:

* it does not *decide* the property in the sense that matters — the kernel does. This script
  predicts each verdict from its own implementation of the model's rule (a deliberately second
  implementation, ~30 lines, mirroring `derivedB`/`determinedB`/`bindEq`/`boundWalk`), and
  `Transcribed.lean` closes each prediction by `decide`. A disagreement is a build failure;
* it does not make the transcription *independent* of the checker. It parses with
  `script/circuit_instance_derivation.py`'s own lexer (`parse_circuit`, `split_args`), the one
  `scripts/check-circuit-instance-derivation.sh` gates on — so a lexer bug is invisible to both.
  What it buys is that the data is machine-transcribed rather than hand-typed, which is the
  fidelity claim the assumption entry needs;
* it does not transcribe every statement. `less_than_strict`, `bool_check` and `less_than_loose`
  appear as bare opcode calls; they constrain but expose nothing, so the instance property is
  unaffected — the checker skips them for the same reason. The count goes into the preamble so the
  omission is visible rather than silent, and any *other* unrecognised form fails the generator.

It does one thing the three above do not: for every circuit the model refutes, it asks the *checker*
(`classify`, the gate's own classifier, given the repository-relative path its manifest is keyed by)
what it made of the same exposure, and records that class beside the verdict. The two predicates are
therefore compared inside one generated artefact rather than in prose — which is what makes the
headline count decomposable instead of merely asserted.

**It writes one module.** The 181 verdicts once could not be elaborated at all — more than 24 GiB in one
`lean` process, no `.olean` ever produced — and while the cause was unknown the transcription was cut
into shards. Extracting one arm of the model's `boundWalk` into `bindAssign`
(`DarkFi/Circuits/InstanceDerivation.lean`) removed the blow-up, and the artefact now builds whole in
**~71 s and 743 MB**; the shards went with the symptom. The *explanation* is deliberately not asserted
here: `bindAssign`'s docstring records what was measured, what was retracted, and what would settle it.

Usage:
    python3 scripts/gen_circuit_transcription.py            # write the module
    python3 scripts/gen_circuit_transcription.py --check    # exit 1 if the module is stale
"""

import argparse
import importlib.util
import os
import re
import sys
import textwrap
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
# A module of its own library, and under `src/` rather than under `src/DarkFi/` on purpose: this
# module is the tree's most expensive elaboration (181 `decide` proofs over 2747 statements), and
# while it sat on `lake build DarkFi`'s path a 4-threaded build of that library exhausted this host's
# memory and froze it (2026-09-24). Lake's `Glob` has no exclusion constructor, so "off the default
# path" has to mean "outside `DarkFi`'s module subtree" — `lakefile.lean` declares `lean_lib
# Transcribed` for it, and `src/` is still scanned by `script/check_lean_axioms.py`, so the move costs
# it no coverage. See `scripts/lean-build.sh`.
OUT = REPO / "proofs" / "lean" / "src" / "Transcribed.lean"

# Opcode calls the checker ignores and this transcription therefore omits. Listed individually so the
# preamble can count them; anything else unrecognised lands in UNHANDLED and fails the generator.
IGNORED_CALLS = {"less_than_strict", "bool_check", "less_than_loose"}
UNHANDLED: list[tuple[str, str]] = []

ASSIGN_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+)$", re.DOTALL)
CALL_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\s*\((.*)\)\s*$", re.DOTALL)
IDENT_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
NUM_RE = re.compile(r"^\d+$")


def checker():
    """The gate's module, imported — one lexer, not two."""
    spec = importlib.util.spec_from_file_location(
        "circuit_instance_derivation", REPO / "script" / "circuit_instance_derivation.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def lean_string(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def balanced(text):
    depth = 0
    for c in text:
        if c in "([":
            depth += 1
        elif c in ")":
            depth -= 1
        elif c == "]":
            depth -= 1
        if depth < 0:
            return False
    return depth == 0


def expr_of(ner, text):
    """A source expression as `("lit", n, [])` / `("var", name, [])` / `("op", name, args)`.

    A bracketed list (`poseidon_hash([a, b])`) is flattened into the opcode's argument list: the
    bracket is a calling convention of the source, and the derivation rule reads free names, not
    group structure. Said here because a reader comparing the two texts will see the difference.
    """
    text = text.strip()
    while text.startswith("(") and text.endswith(")") and balanced(text[1:-1]):
        text = text[1:-1].strip()
    if text.startswith("[") and text.endswith("]"):
        inner = text[1:-1].strip()
        args = [] if not inner else [expr_of(ner, a) for a in ner.split_args(inner)]
        if any(a is None for a in args):
            return None
        return ("op", "list", args)
    if NUM_RE.match(text):
        return ("lit", text, [])
    if IDENT_RE.match(text):
        return ("var", text, [])
    m = CALL_RE.match(text)
    if m:
        args = [expr_of(ner, a) for a in ner.split_args(m.group(2))]
        if any(a is None for a in args):
            return None
        return ("op", m.group(1), args)
    return None


def stmt_of(ner, path, s):
    """A source statement as a `(kind, ...)` tuple, or None for the ignored opcode calls."""
    m = ASSIGN_RE.match(s)
    if m and not s.startswith(("constrain", "range_check")):
        e = expr_of(ner, m.group(2))
        if e is None:
            UNHANDLED.append((path, s))
            return None
        return ("assign", m.group(1), e)
    c = CALL_RE.match(s)
    if not c:
        UNHANDLED.append((path, s))
        return None
    name, args = c.group(1), ner.split_args(c.group(2))
    if name in ("constrain_equal_base", "constrain_equal_point") and len(args) == 2:
        a, b = expr_of(ner, args[0]), expr_of(ner, args[1])
        if a is None or b is None:
            UNHANDLED.append((path, s))
            return None
        return ("constrainEq", a, b)
    if name == "constrain_instance" and len(args) == 1:
        e = expr_of(ner, args[0])
        if e is None:
            UNHANDLED.append((path, s))
            return None
        return ("constrainInstance", e)
    if name == "range_check" and len(args) >= 1:
        e = expr_of(ner, args[0])
        if e is None:
            UNHANDLED.append((path, s))
            return None
        return ("rangeCheck", e)
    if name in IGNORED_CALLS:
        return None
    UNHANDLED.append((path, s))
    return None


def render_expr(e):
    kind, val, args = e
    if kind == "lit":
        return ".lit " + val
    if kind == "var":
        return ".var " + lean_string(val)
    return ".op " + lean_string(val) + " [" + ", ".join(render_expr(a) for a in args) + "]"


def render_stmt(s):
    if s[0] == "assign":
        return "  .assign " + lean_string(s[1]) + " (" + render_expr(s[2]) + ")"
    if s[0] == "constrainEq":
        return "  .constrainEq (" + render_expr(s[1]) + ") (" + render_expr(s[2]) + ")"
    if s[0] == "constrainInstance":
        return "  .constrainInstance (" + render_expr(s[1]) + ")"
    return "  .rangeCheck (" + render_expr(s[1]) + ")"


def model_verdict(held, stmts):
    """The model's rule, in Python — a second implementation, and the kernel is the judge.

    Mirrors `InstanceDerivation`'s `derivedB`/`determinedB`/`bindEq`/`boundWalk` exactly: a literal
    is derived and determined; a name is derived if held-or-bound and determined only if bound; an
    opcode call is derived when its arguments are and determined when its arguments are *derived*;
    an assignment binds its name when its right-hand side is derived; an equality binds the name on
    one side when the other side is determined. Returns (verdict, the first undetermined exposure).
    """
    held = set(held)
    bound = set()

    def derived(e):
        kind, val, args = e
        if kind == "lit":
            return True
        if kind == "var":
            return val in held or val in bound
        return all(derived(a) for a in args)

    def determined(e):
        kind, val, args = e
        if kind == "lit":
            return True
        if kind == "var":
            return val in bound
        return all(derived(a) for a in args)

    first = None
    for s in stmts:
        if s[0] == "assign":
            if derived(s[2]):
                bound.add(s[1])
        elif s[0] == "constrainEq":
            if determined(s[1]) and s[2][0] == "var":
                bound.add(s[2][1])
        elif s[0] == "constrainInstance":
            if not determined(s[1]) and first is None:
                first = s[1]
    return first is None, first


def checker_class(ner, rel, constants, witnesses, stmts, opcodes, manifest, first):
    """What the *checker* made of the exposure the model refused, in the checker's own vocabulary.

    Two predicates, two readings of one circuit — and decomposing the model's refutations by the
    checker's class is what turns the headline count into a measurement instead of an assertion.

    `rel` is a **repository-relative** path and must stay one: `classify` keys its
    `circuit_free_instances.txt` lookups by `(path, name)`, so an absolute path silently misses every
    entry and reports each declared-free instance as a failure. Measured 2026-09-24: calling it with
    absolute paths turned 11 checker failures into 51.
    """
    findings, instances, _ = ner.classify(rel, constants, witnesses, stmts, opcodes, manifest)
    cls = {}
    for name, verdict in instances:
        cls.setdefault(name, verdict)
    for _, arg, _ in findings:
        cls.setdefault(arg, "checker-fails")
    if first[0] != "var":
        return "inline"
    return cls.get(first[1], "unclassified-by-the-checker")


CLASS_TEXT = {
    "redundant": "The checker resolves it as `redundant` — pinned by another exposed determination, "
                 "which the model's sequential rule does not follow.",
    "declared-free": "The checker resolves it as `declared-free`, from a host-side justification in "
                     "`script/circuit_free_instances.txt`.",
    "bound": "The checker resolves it as `bound`, through a `constrain_equal_base` whose determining "
             "side is a declared constant.",
    "checker-fails": "**The checker fails it too** — one of the instances `OBL-Z16` names, where the "
                     "model and the checker agree.",
    "inline": "It is an inline expression rather than a bare name, so the checker's class is the "
              "expression's and not a name's.",
    "unclassified-by-the-checker": "The checker places it under none of its verdicts.",
}


def circuit_name(path):
    """`src/contract/bridge/proof/withdraw.zk` -> `bridge_withdraw`.

    The path is made **repository-relative first**, and that is not cosmetic: `zk_files()` yields
    absolute paths, so a name taken from one bakes the generator's own home directory into every
    identifier of a committed file. The transcription would then be a function of where the tree is
    checked out, and the freshness check would pass only on the machine that wrote it. Measured
    2026-09-24: the first version of this function named all 181 circuits `__home_patrick_…`.
    """
    parts = Path(os.path.relpath(path, REPO)).parts
    drop = {"src", "contract", "proof", "proofs", "core", "bin"}
    kept = [p for p in parts if p not in drop and not p.endswith(".zk")]
    return re.sub(r"[^A-Za-z0-9_]", "_", "_".join(kept + [Path(path).stem]))


HEADER = '''/-
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
not how much memory one of them uses, and 181 kernel `decide` evaluations is where in this tree that
difference bites. The gate builds it under `scripts/lean-build.sh`, which adds the cgroup memory
ceiling the thread cap never was. **A `lake build DarkFi` therefore does not type-check this file; the
gate does.** `CheckAxioms.lean` imports it directly, so the axiom walk still covers all 181 theorems.

What is here is one `List Stmt` per circuit in `InstanceDerivation`'s vocabulary, the names the
circuit holds (its `constant` and `witness` declarations), and one verdict per circuit closed by
`decide`. **The verdict is the model's, not the checker's**, and the two differ by design: the model
asks whether every exposed value is *determined* by what precedes it, while the checker additionally
accepts an exposed value that the circuit pins elsewhere (`redundant`) or that a host-side
justification declares free (`script/circuit_free_instances.txt`).

{measured}

**The 170 are decomposed rather than asserted, and the decomposition is the finding.** For each
refuted circuit the generator asks the *checker* — `classify`, the gate's own classifier — what it
made of the exposure the model refused, and the answer is that the two rules disagree by design
almost everywhere:

{decomposition}

So the model does not contradict the checker; it **refines** it, and every one of the 170 is the
checker's weaker rule or the single boundary the model note names. Two consequences a reader should
take from this file rather than infer:

* `Axioms.NoFreeInstances`' *name* is a **strict** reading this tree mostly does not meet — 170 of
  181 circuits are refuted under it — while the property the tree actually enforces is the checker's
  four-verdict rule, whose failures are the 11 instances of `OBL-Z16`. The axiom is uninterpreted, so
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
one — measured, every undetermined exposure across the 170 refutations is a witness and none is a
constant — so that direction is untested rather than settled, while the direction above is met.

Not transcribed, and counted rather than dropped silently: {ignored} bare opcode-call statements
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
transcription — all 181 verdicts — builds in **~71 s and 743 MB** as one module. **Which circuits were
expensive, and why, is not established** — see `bindAssign`'s docstring, which carries the controlled
comparison that justifies the change and the rival explanations it does not settle, and retracts the
short-circuit story this header first told. Sharding the artefact was tried while the cause was unknown
and has been withdrawn: it was a workaround for a defect, not a property of the data.
-/
'''

PREAMBLE = '''
import DarkFi.Circuits.InstanceDerivation

namespace Circuits.Transcribed

open Circuits.InstanceDerivation
'''

def block_text(block):
    """One circuit: its held names, its statement list, and its verdict.

    Self-contained by construction — every declaration it emits is prefixed with the circuit's own
    name — which is what makes slicing the block list a safe way to bound a `lean` process.
    """
    path, name, held, stmts, verdict, first, klass = block
    held_txt = ", ".join(lean_string(h) for h in sorted(held))
    out = [f"\n/-- `{path}` — {sum(1 for s in stmts if s[0] == 'constrainInstance')} "
           f"exposure(s). -/\n"]
    out.append(f"def {name}_held : List Name := [{held_txt}]\n\n")
    out.append(f"def {name}_stmts : List Stmt :=\n[\n"
               + ",\n".join(render_stmt(s) for s in stmts) + "\n]\n")
    if verdict:
        out.append(f"\n/-- **The property holds** for `{path}`. -/\n"
                   "@[axiom_budget 0]\n"
                   f"theorem {name}_no_free_instance :\n"
                   f"    NoFreeInstance {name}_held {name}_stmts := by\n"
                   "  unfold NoFreeInstance\n  decide\n")
    else:
        out.append(f"\n/-- **The property fails** for `{path}`: its first undetermined exposure is\n"
                   f"    `{render_expr(first)}`, which the circuit does not bind before exposing.\n"
                   f"    {CLASS_TEXT[klass]} -/\n"
                   "@[axiom_budget 0]\n"
                   f"theorem {name}_has_a_free_instance :\n"
                   f"    ¬ NoFreeInstance {name}_held {name}_stmts := by\n"
                   "  unfold NoFreeInstance\n  decide\n")
    # `"\n".join`, matching the uncut renderer exactly: the blocks in the shards are then
    # byte-identical to the blocks the single file carried, so a shard's diff is its header and
    # nothing else. Restore this if the renderer is refactored again.
    return "\n".join(out)


def render(blocks, ignored, holds, fails_):
    """The whole transcription as one module: the account, the preamble, and the 181 verdicts."""
    counts = Counter(b[6] for b in blocks if not b[4])
    decomposition = "\n".join(
        textwrap.fill(f"* **{n}** of the {fails_} — {CLASS_TEXT[k]}", width=98,
                      subsequent_indent="  ", break_long_words=False)
        for k, n in sorted(counts.items(), key=lambda kv: -kv[1]))
    doc = (HEADER
           .replace("{ignored}", str(ignored))
           .replace("{decomposition}", decomposition)
           .replace("{measured}",
                    f"Measured: **{fails_}** of {len(blocks)} circuits expose at least one value the model does "
                    f"not\nfind determined in-circuit, and **{holds}** hold. Each refuted circuit names the first "
                    "such\nexposure **and the checker's class for that exposure**."))
    summary = (f"\n/-! ===== The circuits, in source order =====\n\n"
               f"{len(blocks)} circuits, {sum(len(b[3]) for b in blocks)} statements transcribed; "
               f"**{holds}** satisfy the model's property and **{fails_}** do not, the latter named by the\n"
               "first undetermined exposure in each, with the checker's class for that exposure named "
               "beneath it.\n-/\n")
    return doc + PREAMBLE + summary + "\n".join(block_text(b) for b in blocks)


def emit():
    ner = checker()
    opcodes = ner.known_opcodes() | ner.EXTERNAL
    manifest = ner.load_manifest()
    blocks = []
    seen = {}
    ignored = 0
    for path in sorted(ner.zk_files()):
        name = circuit_name(path)
        if name in seen:
            sys.exit(f"FAIL: two circuits map to the Lean name {name}: {seen[name]} and {path}")
        seen[name] = path
        rel = os.path.relpath(path, REPO)
        text = ner.strip_comments(Path(path).read_text())
        constants, witnesses, stmts = ner.parse_circuit(text)
        held = sorted(constants | witnesses)
        body = []
        for s in stmts:
            c = CALL_RE.match(s)
            if c and c.group(1) in IGNORED_CALLS:
                ignored += 1
                continue
            t = stmt_of(ner, path, s)
            if t is not None:
                body.append(t)
        if UNHANDLED:
            for p, s in UNHANDLED:
                print(f"FAIL: unhandled statement in {p}: {s[:120]}", file=sys.stderr)
            sys.exit(1)
        verdict, first = model_verdict(held, body)
        klass = None if verdict else checker_class(
            ner, rel, constants, witnesses, stmts, opcodes, manifest, first)
        blocks.append((rel, name, held, body, verdict, first, klass))
    holds = sum(1 for b in blocks if b[4])
    return blocks, ignored, holds, len(blocks) - holds


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="exit 1 if the committed module is not what the sources produce")
    args = ap.parse_args()
    blocks, ignored, holds, fails_ = emit()
    text = render(blocks, ignored, holds, fails_)
    summary = (f"{len(blocks)} circuits, {sum(len(b[3]) for b in blocks)} statements, "
               f"{ignored} ignored opcode calls, {holds} hold, {fails_} fail")
    if args.check:
        if not OUT.exists():
            print(f"FAIL: {os.path.relpath(OUT, REPO)} does not exist; run the generator",
                  file=sys.stderr)
            return 1
        if OUT.read_text() != text:
            print(f"FAIL: {os.path.relpath(OUT, REPO)} is stale — re-run "
                  "scripts/gen_circuit_transcription.py", file=sys.stderr)
            return 1
        print(f"OK: {os.path.relpath(OUT, REPO)} matches the sources — {summary}")
        return 0
    OUT.write_text(text)
    print(f"wrote {os.path.relpath(OUT, REPO)} — {summary}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
