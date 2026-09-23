#!/usr/bin/env python3
"""Measure the barb alphabet the capability type system is built on.

WHY THIS EXISTS. `OBL-T9` in `doc/src/arch/verification-hazop.md` carried figures that
were hand-measured and drifted by one resource between two readings of itself, and a
second figure ("11 primitives carry no barb of their own") that does not reproduce under
the reading its own worked example uses. A calculus founded on barbs cannot have its
alphabet's properties recalled rather than computed, so the measurement is a script.

WHAT IT COMPUTES, over `proofs/lean/src/DarkFi/Capability/{Types,Composition}.lean`:

  1. every `Resource` and its `requiredBarbs` set;
  2. subsumption — resources whose barb set is contained in the union of the others';
  3. identical barb sets among resources (name the groups);
  4. the number of DISTINCT unions over all `2^n` resource subsets — the figure that
     makes the type count sublinear in the resources rather than exponential;
  5. the same for `PrimitiveType`s, under BOTH readings: contained in the union of the
     others, and contained in another single primitive's set;
  6. whether any two primitive barb sets are EQUAL, which is the property the model
     actually states.

WHAT IT DELIBERATELY DOES NOT DO: decide whether any of it is a defect. It exits 0 and
prints. The row's proposition turns on whether "distinct" means set inequality (what
`Types.lean` defines, for `PrimitiveType` only) or "distinct and non-subsumed" (what the
row asks for and no definition in the model supplies); that is a decision, and a script
that silently picked one would be making it. The distinction is printed, not resolved.

Usage:  python3 script/capability_barb_analysis.py
Exit 0 always; exit 2 if a source file cannot be read or parsed at all.
"""

import re
import sys
import itertools
import collections
import os

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TYPES = os.path.join(REPO, "proofs/lean/src/DarkFi/Capability/Types.lean")
COMPOSITION = os.path.join(REPO, "proofs/lean/src/DarkFi/Capability/Composition.lean")


def read(path):
    try:
        return open(path, errors="replace").read()
    except OSError as e:
        print(f"FAIL: cannot read {path}: {e}")
        sys.exit(2)


def barbs_of(s):
    """`{Barb.spend, Barb.nullify}` -> {'spend', 'nullify'}."""
    return {b.strip().removeprefix("Barb.") for b in s.split(",") if b.strip()}


def parse_resources(text):
    """Every `def X : Resource := { name := "…", requiredBarbs := {…} }`."""
    out = []
    for m in re.finditer(r'def\s+(\w+)\s*:\s*Resource\s*:=\s*\{(.*?)\n\s*\}', text, re.S):
        ident, body = m.group(1), m.group(2)
        name = re.search(r'name\s*:=\s*"([^"]*)"', body)
        barbs = re.search(r'requiredBarbs\s*:=\s*\{(.*?)\}', body, re.S)
        if not (name and barbs):
            print(f"FAIL: could not parse the Resource `{ident}` — its shape changed")
            sys.exit(2)
        out.append((ident, name.group(1), barbs_of(barbs.group(1))))
    return out


def parse_primitives(text):
    """Every `def X : PrimitiveType := { name := "…", barbs := {…} }`."""
    out, skipped = [], []
    for m in re.finditer(r'def\s+(\w+)\s*:\s*PrimitiveType\s*:=\s*\{(.*?)\n\s*\}', text, re.S):
        ident, body = m.group(1), m.group(2)
        name = re.search(r'name\s*:=\s*"([^"]*)"', body)
        # A barbless type writes `barbs := ∅`, not `barbs := {}` — three definitions do, and
        # they are the three the file says are not valid DarkWow types.
        if re.search(r'barbs\s*:=\s*∅', body):
            barbs = ""
        else:
            m = re.search(r'barbs\s*:=\s*\{(.*?)\}', body, re.S)
            barbs = m.group(1) if m else None
        if name is None or barbs is None:
            print(f"FAIL: could not parse the PrimitiveType `{ident}` — its shape changed")
            sys.exit(2)
        (out if barbs_of(barbs) else skipped).append(
            (ident, name.group(1), barbs_of(barbs)))
    return out, skipped


def listing(text, marker):
    m = re.search(marker + r'\s*:=\s*\[(.*?)\]', text, re.S)
    return [x.strip() for x in m.group(1).split(",") if x.strip()] if m else None


def main():
    types_src, comp_src = read(TYPES), read(COMPOSITION)

    resources = parse_resources(comp_src)
    with_barbs, barbless = parse_primitives(types_src)
    # `allPrimitiveTypes` is the model's own enumeration and is what "17 of 20" refers to.
    listed = listing(types_src, r'allPrimitiveTypes')

    print("script/capability_barb_analysis.py — the barb alphabet, measured")
    print("")
    print(f"resources : {len(resources)}")
    print(f"primitives: {len(with_barbs)} with barbs, {len(barbless)} with none "
          f"({', '.join(n for _i, n, _b in barbless) or 'none'})")
    if listed is not None:
        print(f"            `allPrimitiveTypes` lists {len(listed)}, and a barbless type is "
              f"one the file says is not a valid DarkWow type")
    print("")

    # --- distinctness, as the model defines it -----------------------------------------
    listed_names = set()
    if listed is not None:
        for _i, name, _b in with_barbs:
            if name in listed:
                listed_names.add(name)
    sets = [(name, frozenset(b)) for _i, name, b in with_barbs if
            (not listed_names or name in listed_names)]
    equal = [(a, b) for (a, sa), (b, sb) in itertools.combinations(sets, 2) if sa == sb]

    print("THE PROPERTY THE MODEL STATES — `Types.lean`:")
    print("    def typesDistinct (t1 t2 : PrimitiveType) : Prop := t1.barbs ≠ t2.barbs")
    print("  i.e. distinctness is *set inequality*, over `PrimitiveType` ONLY. It carries no")
    print("  subsumption condition, and no `Resource` or `CapabilityType` is in its domain.")
    print(f"  primitives with equal barb sets: {len(equal)}"
          + (f"  {equal}" if equal else "  — so the stated property holds as measured"))
    print("")

    # --- subsumption, resources ---------------------------------------------------------
    names = [r[1] for r in resources]
    rsets = [frozenset(r[2]) for r in resources]
    res_subsumed = []
    for i, (n, s) in enumerate(zip(names, rsets)):
        others = set().union(*[o for j, o in enumerate(rsets) if j != i])
        if s <= others:
            res_subsumed.append(n)
    groups = collections.defaultdict(list)
    for n, s in zip(names, rsets):
        groups[s].append(n)
    dup_groups = {tuple(sorted(v)): s for s, v in groups.items() if len(v) > 1}

    print(f"RESOURCES — subsumption")
    print(f"  barb set contained in the union of the others: "
          f"{len(res_subsumed)} of {len(rsets)}")
    print(f"    ({', '.join(res_subsumed)})")
    print(f"  NOT subsumed: "
          f"{', '.join(n for n in names if n not in res_subsumed) or 'none'}")
    print(f"  identical barb sets: {len(dup_groups)}")
    for members, s in dup_groups.items():
        print(f"    {' == '.join(members)}  =  {{{', '.join(sorted(s))}}}")
    print("")

    # --- distinct unions over all subsets ----------------------------------------------
    all_b = sorted({b for s in rsets for b in s})
    unions = set()
    for r in range(len(rsets) + 1):
        for combo in itertools.combinations(rsets, r):
            unions.add(frozenset(set().union(*combo)) if combo else frozenset())
    print(f"  distinct barb-set unions over all 2^{len(rsets)} = {2 ** len(rsets)} subsets: "
          f"{len(unions)}")
    print(f"  (the union of all resources is {len(all_b)} barbs)")
    print("")

    # --- subsumption, primitives, both readings ----------------------------------------
    pnames = [n for n, _b in sets]
    psets = [frozenset(b) for _n, b in sets]
    prim_union, prim_single = [], []
    for i, (n, s) in enumerate(zip(pnames, psets)):
        if s <= set().union(*[o for j, o in enumerate(psets) if j != i]):
            prim_union.append(n)
        if any(s < o for j, o in enumerate(psets) if j != i):
            prim_single.append(n)
    print("PRIMITIVES — subsumption, both readings, because the row's two figures differ by it")
    print(f"  contained in the union of the others (the reading its '11' reproduces under): "
          f"{len(prim_union)} of {len(psets)}")
    print(f"    ({', '.join(prim_union)})")
    print(f"  contained in another single primitive's set (the reading its own worked example "
          f"uses): {len(prim_single)} of {len(psets)}")
    print(f"    ({', '.join(prim_single)})")
    print("")

    print("WHAT THIS DOES NOT DECIDE. Whether subsumption is a *defect* turns on whether a")
    print("barb set is meant to IDENTIFY a type or to name the PERMISSIONS an action requires.")
    print("`type-system.md` §1.1 defines barbs as observable actions and `ocap.md` §5.1")
    print("declares 'defined privilege containment, not least privilege' on purpose, under")
    print("which two actions on one capability kind sharing a permission set is the design.")
    print("The model defines distinctness as inequality and nothing more; the stronger")
    print("property has to be chosen, and choosing it is a change to a 33-constructor")
    print("alphabet represented in four places. Printed, not taken.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
