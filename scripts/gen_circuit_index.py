#!/usr/bin/env python3
"""The `(r, s) -> circuit` join, as data — the step `Axioms.NoFreeInstances` has been waiting on.

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

## What it does NOT do

It does not decide whether a pair's circuit set is *sufficient* for `NoFreeInstances` — it reports the
set. And `claim_coinbase` resolving to **no circuit** is a recorded decision, not a gap: that path is
host-side.
"""
import importlib.util
import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MAP = REPO / "script" / "circuit_index_map.txt"
COMPOSITION = REPO / "proofs" / "lean" / "src" / "DarkFi" / "Capability" / "Composition.lean"
LEAN_MAP = {"BridgeDepositResource": "bridgeDeposit", "BridgeWithdrawResource": "bridgeWithdraw"}


def transcription():
    """The transcription generator, imported for `circuit_name` — not re-implemented."""
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


def main():
    gen = transcription()
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

    failures, resolved, none_circuit = [], 0, 0
    print(f"{(len(pairs))} (r, s) pair(s) instantiated in Capability/Composition.lean\n")
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
            print(f"  {typ:26s} ({rname}, {aname})\n      -> {contract}/{function}: NO CIRCUIT "
                  f"(host-side; see the map's reason)")
            none_circuit += 1
            continue
        path = identity.get((contract, circuit))
        if path is None:
            failures.append(
                f"{typ}: {contract}/{function} names circuit {circuit!r}, which no .zk declares"
            )
            continue
        print(f"  {typ:26s} ({rname}, {aname})\n      -> {contract}/{function}: {circuit}  [{gen.circuit_name(path)}]")
        resolved += 1

    print(f"\n  resolved: {resolved}   resolved to no circuit: {none_circuit}   failures: {len(failures)}")
    for f in failures:
        print(f"  FAIL: {f}")
    if failures:
        sys.exit(f"FAIL: {len(failures)} pair(s) do not resolve — each needs a map entry or a decision")
    # The control: a regex that silently stopped matching would report success over nothing. 14
    # resources and 14 actions are defined, and the instantiated pairs are those `CapabilityType`
    # definitions that exist — measured 2026-09-24 as 14.
    if len(res) != 14 or len(act) != 14:
        sys.exit(f"FAIL: {len(res)} resource(s) and {len(act)} action(s) found, 14 and 14 expected — "
                 "the extraction above stopped working")
    if len(pairs) < 4:
        sys.exit(f"FAIL: only {len(pairs)} instantiated pair(s) found — the extraction stopped working")


if __name__ == "__main__":
    main()
