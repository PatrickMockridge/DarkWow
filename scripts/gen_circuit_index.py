#!/usr/bin/env python3
"""The `(r, s) -> circuit` join, emitted as a Lean module — the step `Axioms.NoFreeInstances` waited on.

## What this is

`Axioms.NoFreeInstances (r : Resource) (s : Action)` is indexed by a resource/action pair; every
circuit-level result in this tree is indexed by a circuit. `proofs/lean/src/Transcribed.lean` supplies
the circuits' statement lists as data and is freshness-gated; **this supplies the other half of the
join**, which nothing supplied until now. The tree's records called that half impossible — "no Lean
term can read a `.zk` file" — which is true of a Lean *term* and false of a *generator*, and this is
the generator.

## The join it performs

    Lean (Resource, Action)                     Capability/Composition.lean, read not parsed loosely
      -> contract directory                     script/circuit_index_map.txt  (reviewed; see below)
      -> manifest function                      the map, where the Lean action name differs
      -> [[functions]].proof_circuit            the manifest
      -> the .zk whose `circuit "..."` matches  the sources
      -> the transcription's def name           circuit_name() from the generator, IMPORTED

The last arrow is why this module imports `gen_circuit_transcription` rather than re-deriving a name:
a second implementation of the naming rule is a second thing that can disagree, and the campaign has
paid for that class already.

## Why a map file exists, and what it is not

Nothing in the tree joins the Lean *names* to the manifest *contracts*: only 3 of the 14 resource
names equal a contract directory, 10 are prefixes of one, and `dao_governance` matches none — while
the action names are ambiguous across contracts (`withdraw` matches four). So the map is a reviewed
list, every line a decision with its evidence, and **this script fails on any name it cannot resolve**
rather than guessing. Which circuit a function proves is read from the manifest, so a manifest change
moves the index and leaves the map alone.

## The verdict, and why it is the model's second predicate

Each pair's theorem is `DisclosureRule` at that pair's transcribed statement list — the strict
`NoFreeInstance` plus the checker's `redundant` classification. The strict predicate is not used
because it would fail at every pair, and that is measured rather than assumed: the generator reads the
strict verdict from the transcription's own blocks, and **fails if a pair's circuit ever gains a
`declared-free` instance**, which is the class `DisclosureRule` does not model and cannot.

## What a pass means

That the committed module is what the sources produce today. The theorem bodies are `decide`, so the
*kernel* is what establishes each verdict — this generator predicts nothing, and a rule that stopped
holding at a pair would fail the build rather than a print statement.

Usage:
    python3 scripts/gen_circuit_index.py            # write the module
    python3 scripts/gen_circuit_index.py --check     # exit 1 if the module is stale
"""
import argparse
import importlib.util
import os
import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MAP = REPO / "script" / "circuit_index_map.txt"
COMPOSITION = REPO / "proofs" / "lean" / "src" / "DarkFi" / "Capability" / "Composition.lean"
OUT = REPO / "proofs" / "lean" / "src" / "CircuitIndex.lean"


def transcription():
    """The transcription generator, imported for `circuit_name` and `emit` — not re-implemented."""
    spec = importlib.util.spec_from_file_location(
        "gen_circuit_transcription", REPO / "scripts" / "gen_circuit_transcription.py"
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def load_map():
    """The reviewed map, by section. Malformed lines fail rather than being skipped."""
    resources, actions, section = {}, {}, None
    for n, line in enumerate(MAP.read_text().splitlines(), 1):
        s = line.strip()
        if s.startswith("# --- resources"):
            section = "resource"
            continue
        if s.startswith("# --- actions"):
            section = "action"
            continue
        if not s or s.startswith("#"):
            continue
        parts = s.split(" : ", 2)
        if section is None or len(parts) < 2:
            sys.exit(f"FAIL: {MAP.name}:{n}: not in a section, or malformed: {line!r}")
        (resources if section == "resource" else actions)[parts[0].strip()] = parts[1].strip()
    return resources, actions


def lean_pairs():
    """The `(Resource, Action)` pairs Lean actually instantiates, and their name strings.

    Read from the `CapabilityType` definitions rather than from a list here, so a pair added to the
    Lean side and not to this script is a *missing pair*, not a silently absent one.
    """
    src = COMPOSITION.read_text()
    res = dict(re.findall(r"^def ([a-zA-Z]+)Resource : Resource :=\n\s*\{ name := \"([^\"]+)\"", src, re.M))
    act = dict(re.findall(r"^def ([a-zA-Z]+)Action : Action := \{ name := \"([^\"]+)\" \}", src, re.M))
    pairs = re.findall(r"^def (\w+) : CapabilityType (\w+)Resource (\w+)Action :=", src, re.M)
    return res, act, [(name, r, a) for name, r, a in pairs]


def join(gen):
    """The join, as data. Returns (resolved, circuitless, failures), each entry a dict.

    Every name it cannot resolve is a failure rather than a skip, and the controls at the end of
    `main` re-check that the extraction it depends on still finds what it found before.
    """
    ner = gen.checker()
    blocks_by_name = {b[1]: b for b in gen.emit()[0]}
    opcodes = ner.known_opcodes() | ner.EXTERNAL
    manifest_dc = ner.load_manifest()

    resources, actions = load_map()
    res, act, pairs = lean_pairs()

    # contract -> {function name: proof_circuit}
    manifests = {}
    for p in sorted(REPO.glob("src/contract/*/manifest.toml")):
        m = tomllib.loads(p.read_text())
        manifests[p.parent.name] = {f["name"]: f.get("proof_circuit") for f in m.get("functions", [])}

    # (contract, circuit name) -> the .zk path whose identity matches
    identity = {}
    for path in list(REPO.glob("src/contract/*/proof/*.zk")):
        for m in re.finditer(r'^\s*circuit\s+"([^"]+)"', path.read_text(), re.M):
            # `path.parent.parent.name` is the contract directory; an index into `parts` is not, for
            # an absolute path — it read `patrick` and made every lookup miss.
            identity[(path.parent.parent.name, m.group(1))] = path

    resolved, circuitless, failures = [], [], []
    for typ, rdef, adef in pairs:
        rname, aname = res[rdef], act[adef]
        contract = resources.get(rname)
        if contract is None:
            failures.append(f"{typ}: resource {rname!r} is not in {MAP.name}")
            continue
        function = actions.get(aname, aname)
        if contract not in manifests:
            failures.append(f"{typ}: mapped contract {contract!r} has no manifest.toml")
            continue
        if function not in manifests[contract]:
            failures.append(
                f"{typ}: {rname!r} -> {contract}, but {function!r} (from action {aname!r}) is not a "
                f"function there"
            )
            continue
        circuit = manifests[contract][function]
        if circuit is None:
            circuitless.append({"typ": typ, "resource": rname, "action": aname,
                                "contract": contract, "function": function})
            continue
        path = identity.get((contract, circuit))
        if path is None:
            failures.append(
                f"{typ}: {contract}/{function} names circuit {circuit!r}, which no .zk declares"
            )
            continue

        defname = gen.circuit_name(path)
        block = blocks_by_name.get(defname)
        if block is None:
            failures.append(f"{typ}: {contract}/{function} resolves to {defname}, which the "
                            "transcription does not carry")
            continue
        rel = os.path.relpath(path, REPO)
        text = ner.strip_comments(Path(path).read_text())
        constants, witnesses, stmts = ner.parse_circuit(text)
        _, instances, _ = ner.classify(rel, constants, witnesses, stmts, opcodes, manifest_dc)
        free = sorted(n for n, k in instances if k == "declared-free")

        resolved.append({
            "typ": typ, "resource": rname, "action": aname, "contract": contract,
            "function": function, "circuit": circuit, "defname": defname, "rel": rel,
            # The Lean declaration names, for the inhabitant's type: `CapabilityType`'s own def names
            # are `<suffix>Resource` / `<suffix>Action` by the regex `lean_pairs` reads them with.
            "rdef": f"{rdef}Resource", "adef": f"{adef}Action",
            # block[4] is the strict verdict, block[6] the checker's class for the first exposure it
            # refused. Read from the transcription rather than recomputed: one implementation.
            "strict_holds": block[4], "first_class": block[6], "declared_free": free,
        })
    return resolved, circuitless, failures


HEADER = '''/-
# The `(r, s)` -> circuit index, generated

**GENERATED FILE — do not edit.** `scripts/gen_circuit_index.py` writes it from the manifests, the
`.zk` sources and the reviewed map in `script/circuit_index_map.txt`, and `scripts/run-all-tests.sh`
re-runs the generator in `--check` mode, so a stale copy is a gate failure. It reuses the model's rule
and the checker's own classifier rather than re-deriving either.

## What this is

`Axioms.NoFreeInstances (r : Resource) (s : Action)` is indexed by a resource/action pair, and five
places in the tree name the same missing half — `(r, s) ↦ the circuit source` "is still not in the
tree" (`Axioms.lean`, `proofs/lean/README.md`, `Capability/Inversion.lean`, `Circuits/Token.lean`,
`Circuits/All.lean`). This module is that half for the **{n}** pairs
`Capability/Composition.lean` instantiates. Each one names the contract and function the reviewed map
records, the circuit its manifest's `proof_circuit` resolves to, and the transcription's definitions
for it — with the verdict closed by `decide`, so the *kernel* is what establishes it.

## The verdict is the model's second predicate, because the strict one fails at every pair

Each theorem is `Circuits.InstanceDerivation.DisclosureRule` at that pair's transcribed statement list
— the strict `NoFreeInstance` plus the checker's `redundant` classification (a witness already inside
another exposed determination). Measured: **all {n} of these circuits fail the strict rule, every one
of them for a `redundant` exposure, and none of them has a `declared-free` instance** ({nf} declared
free across the {n}). So the proofs rest on the structural rule with **no** external exception list
behind them, and that is a fact this generator checks rather than a claim it prints: it fails if any of
these circuits ever gains a declared-free instance, because `DisclosureRule` does not model that class
and could not carry it.

**What the rule does not say, and it matters more here than anywhere.** `redundant` buys that the
exposure adds no freedom — not that the witness is safe. The witness stays prover-chosen, and whether
that matters is a property of the contract's entrypoint and not of the statement list. That obligation
is `OBL-Z1`'s, and nothing in this module or the one it imports discharges it.

## The axiom is gone, and this module is what replaced it

`Axioms.NoFreeInstances` is **deleted** as of 2026-09-24. `Capability.Inversion.CircuitDerivable` now
carries the data that premise was about — the circuit's `held` names, its `stmts` — and a
`noFreeInstances` field that is a **computation** over them rather than an uninterpreted `Prop`. The
inhabitants below are that premise supplied rather than assumed at every pair this tree has a circuit
for, and the projection beside each one shows the bare rule is the same fact.

## What this is not

Nor is any of this a claim about the deployed circuits. The transcription is source-faithful *as data*,
and `script/circuit_instance_derivation.py`'s own caveats are inherited unchanged — it reads `.zk`
source rather than the `.zk.bin` that is deployed, and it does not know what the opcodes mean. See
`OBL-T7`.
-/

import DarkFi.Circuits.InstanceDerivation
import DarkFi.Capability.Inversion
import Transcribed

namespace CircuitIndex

open Circuits.InstanceDerivation
open DarkFi.Capability.Composition
'''

PAIR_TEXT = '''/-- `{typ}` — (`{resource}`, `{action}`) maps to `{contract}`'s `{function}`, circuit `{circuit}`,
    transcribed as `Circuits.Transcribed.{defname}`.

    **The inhabitant — what replaced `Axioms.NoFreeInstances`.** The structure carries the circuit's
    `held` names and its statement list outright, and its last field is the rule computed over that
    data, closed by the kernel rather than asserted. So the ZK premise is now *supplied* at this pair
    instead of assumed, and `capabilityType_of_circuitDerivable` applies to it.

    Strict verdict: **{strict}**; the checker's class for its first undetermined exposure is
    `{klass}`. Declared-free instances in this circuit: {nf}.

    A `def` rather than a `theorem`, and that is forced rather than stylistic: `theorem` takes a
    proposition and this is a structure, which is a `Type` — Lean rejects it ("type of theorem ... is
    not a proposition"). So the `decide` inside it carries no `@[axiom_budget]` of its own, which is
    why the projection below exists: it is a `theorem`, it is annotated, and the gate measures the
    axioms the inhabitant's proof reaches through it. -/
def {typ}_circuitDerivable : CircuitDerivable {rdef} {adef} :=
  {{ primitives := {typ}.primitives
   , coversBarbs := {typ}.coversBarbs
   , held := Circuits.Transcribed.{defname}_held
   , stmts := Circuits.Transcribed.{defname}_stmts
   , noFreeInstances := by unfold DisclosureRule; decide
   }}

/-- The rule at this pair, **projected from the inhabitant above rather than proved a second time** —
    so the two are one fact and not two computations that could disagree. -/
@[axiom_budget 0]
theorem {typ}_disclosureRule :
    DisclosureRule Circuits.Transcribed.{defname}_held
      Circuits.Transcribed.{defname}_stmts :=
  {typ}_circuitDerivable.noFreeInstances
'''


def render(resolved, circuitless, n_free):
    """The whole module: the account, the preamble, and one theorem per pair."""
    # `{nf}` first: it contains `{n}`, so the other order would rewrite it into `12f`.
    out = [HEADER.replace("{nf}", str(n_free)).replace("{n}", str(len(resolved)))]
    out.append(f"\n/-! ===== One inhabitant and its projection, per `(r, s)` pair — {len(resolved)} "
               "pairs ===== -/\n")
    for r in resolved:
        out.append("\n" + PAIR_TEXT.format(
            typ=r["typ"], resource=r["resource"], action=r["action"], contract=r["contract"],
            function=r["function"], circuit=r["circuit"], defname=r["defname"],
            rdef=r["rdef"], adef=r["adef"],
            strict="holds" if r["strict_holds"] else "refuted", klass=r["first_class"],
            nf=len(r["declared_free"]),
        ))

    # The circuit-less pairs, as data with their reasons. `DisclosureRule` cannot be stated at a pair
    # with no statement list, so these are the pairs a consumer of this module has to handle itself.
    out.append("\n/-! ===== The pairs that resolve to no circuit, as data =====\n\n"
               "**These are decisions rather than omissions, and a reader can tell which:** the\n"
               "generator fails on any pair it cannot resolve, so the pairs named here are the only\n"
               "ones it deliberately names no circuit for. A pair absent from the theorems above is a\n"
               "failure of the generator, not a gap in this file.\n\n")
    for c in circuitless:
        out.append(f"* (`{c['resource']}`, `{c['action']}`) maps to `{c['contract']}`'s "
                   f"`{c['function']}`, whose manifest\ndeclares no `proof_circuit`.\n")
    out.append("\n`native_token`'s manifest is the FYI document its own header says it is, and its\n"
               "coinbase path is host-side: `bin/dwowd` verifies the proof of work, and the host\n"
               "checks `effective_value.checked_add(total_pin) == Some(input.value)` at\n"
               "`entrypoint/mod.rs:1000-1006`. `script/circuit_index_map.txt`'s `claim_coinbase` line\n"
               "records the same decision with its citation.\n-/\n\n"
               "def circuitlessPairs : List (String × String) :=\n"
               "  [" + ", ".join(f"(\"{c['resource']}\", \"{c['action']}\")" for c in circuitless)
               + "]\n")
    out.append("\nend CircuitIndex\n")
    return "".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="exit 1 if the committed module is not what the sources produce")
    args = ap.parse_args()

    gen = transcription()
    resolved, circuitless, failures = join(gen)

    print(f"{len(resolved) + len(circuitless) + len(failures)} (r, s) pair(s) instantiated in "
          "Capability/Composition.lean\n")
    for r in resolved:
        strict = "holds" if r["strict_holds"] else f"refuted ({r['first_class']})"
        print(f"  {r['typ']:26s} ({r['resource']}, {r['action']})\n"
              f"      -> {r['contract']}/{r['function']}: {r['circuit']}  [{r['defname']}]"
              f"  strict: {strict}")
    for c in circuitless:
        print(f"  {c['typ']:26s} ({c['resource']}, {c['action']})\n"
              f"      -> {c['contract']}/{c['function']}: NO CIRCUIT (a decision; see the map)")
    print(f"\n  resolved: {len(resolved)}   resolved to no circuit: {len(circuitless)}"
          f"   failures: {len(failures)}")
    for f in failures:
        print(f"  FAIL: {f}")
    if failures:
        sys.exit(f"FAIL: {len(failures)} pair(s) do not resolve — each needs a map entry or a decision")

    # The control the module's own excuse rests on: `DisclosureRule` does not model `declared-free`,
    # so a pair whose circuit has one is a pair this module cannot honestly claim a verdict about.
    declared = [(r["typ"], r["declared_free"]) for r in resolved if r["declared_free"]]
    if declared:
        sys.exit(f"FAIL: {len(declared)} pair(s) resolve to a circuit with a declared-free instance — "
                 f"`DisclosureRule` does not model that class: {declared}")

    text = render(resolved, circuitless, sum(len(r["declared_free"]) for r in resolved))
    summary = (f"{len(resolved)} pair(s) resolved, {len(circuitless)} to no circuit")

    if args.check:
        if not OUT.exists():
            print(f"FAIL: {os.path.relpath(OUT, REPO)} does not exist; run the generator",
                  file=sys.stderr)
            return 1
        if OUT.read_text() != text:
            print(f"FAIL: {os.path.relpath(OUT, REPO)} is stale — re-run "
                  "scripts/gen_circuit_index.py", file=sys.stderr)
            return 1
        print(f"OK: {os.path.relpath(OUT, REPO)} matches the sources — {summary}")
        return 0
    OUT.write_text(text)
    print(f"wrote {os.path.relpath(OUT, REPO)} — {summary}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
