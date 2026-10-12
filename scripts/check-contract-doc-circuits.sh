#!/usr/bin/env bash
#
# A contract's ZK-circuit list is a claim about its `proof/` directory.
#
# WHY THIS EXISTS. Nothing reads a doc's prose claim about a code fact and compares it to the code.
# `check-doc-index.sh` guards the docs *structurally* (SUMMARY<->tree, links, include anchors, every
# `OBL-*` citation resolves) and `check-manifest-entrypoint-agreement.sh` guards the manifest against
# the code — but `doc/src/contract/<n>.md`'s "All N circuits" sentence and its circuit table are
# compared to nothing. Measured 2026-10-10: five docs are already stale —
#   attestation "All 10 circuits" vs 12 in proof/;  escrow "All 4" vs 5;  game_room "All 5" vs 12;
#   slot (no count) table of 2 vs 3;  stablecoin "All 9" vs 10.
# This is the instrument that was missing (register RC6, spec/model/register drift).
#
# WHAT IT CHECKS — and it is deliberately narrow. For each `doc/src/contract/<n>.md` with a sibling
# `src/contract/<n>/proof/`:
#   D1 the set of `.zk` basenames in the doc's circuit table equals the set in `proof/*.zk`;
#   D2 a "All N circuits" sentence, where present, equals `ls proof/*.zk | wc -l`.
# It does NOT read `manifest.toml [[circuits]]` (its `V2` names are a different namespace from the
# `.zk` filenames, and `manifest_proof_declarations.rs`/OBL-C91 already covers manifest<->proof), and
# it does NOT judge prose circuit mentions outside a table — those escalate to the LLM review pass.
# A contract whose proof/ has circuits but whose doc has no circuit table is reported as a NOTE
# (coverage), not a failure: there is nothing to have drifted.
#
# Usage: scripts/check-contract-doc-circuits.sh
#        scripts/check-contract-doc-circuits.sh --self-test   (planted defects; non-zero exit)
# Exit status: 1 on any finding; 0 otherwise.

set -uo pipefail

ROOT="${CONTRACT_DOC_CIRCUITS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
export CONTRACT_DOC_CIRCUITS_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, sys, pathlib, tempfile, shutil

ROOT = pathlib.Path(os.environ["CONTRACT_DOC_CIRCUITS_ROOT"])

TBL_ZK = re.compile(r'`([A-Za-z0-9_]+\.zk)`')
COUNT = re.compile(r'All\s+\*{0,2}(\d+)\*{0,2}\s+circuits')
# D1 reads rows under a **circuit** heading, not every table row in the document. Until 2026-10-12 it
# unioned the `.zk` mentions of any line starting with `|`, so a contract could satisfy it by naming
# `.zk` files in an unrelated table — `slot.md` did exactly that, scoring `with_table=18` before its
# circuit table existed, with the "Composability" table's two rows supplying `commit_bet` and
# `settle_bet` and only `reveal_spin` reported missing. That is the fixture-weaker-than-downstream
# pattern this campaign keeps finding, one level up: a check whose *name* says "circuit table" and
# whose test said "a pipe-row somewhere". The heading requirement is modelled on the sibling
# `check-contract-doc-functions.sh`'s `FUNHEAD`, which has always read rows under a functions heading.
HEAD = re.compile(r'^(#{2,4})\s+(.*)$')
CIRHEAD = re.compile(r'circuits?\b|zk\s+proof|proof\s+structure|circuit\s+table', re.I)


def scan(root):
    findings, notes, docs, with_table = [], [], 0, 0
    for doc in sorted((root / "doc/src/contract").glob("*.md")):
        name = doc.stem
        pd = root / "src" / "contract" / name / "proof"
        if not pd.is_dir():
            continue
        docs += 1
        proof = sorted(p.name[:-3] for p in pd.glob("*.zk"))  # strip ".zk"
        txt = doc.read_text()
        rows, section = set(), None
        for line in txt.splitlines():
            h = HEAD.match(line)
            if h:
                section = h.group(2)
                continue
            if not line.lstrip().startswith('|'):
                continue
            if not (section and CIRHEAD.search(section)):
                continue
            rows |= {z[:-3] for z in TBL_ZK.findall(line)}
        if not rows:
            notes.append((name, f"no circuit table (proof/ holds {len(proof)})"))
            continue
        with_table += 1
        only_doc = sorted(rows - set(proof))
        only_proof = sorted(set(proof) - rows)
        if only_doc or only_proof:
            findings.append((name, f"D1 table != proof/: doc-only={only_doc} proof-only={only_proof}"))
        m = COUNT.search(txt)
        if m and int(m.group(1)) != len(proof):
            findings.append((name, f"D2 count sentence says {m.group(1)}, proof/ holds {len(proof)}"))
    return docs, with_table, findings, notes


def main():
    if "--self-test" in sys.argv:
        with tempfile.TemporaryDirectory() as tmp:
            t = pathlib.Path(tmp)
            c = "dao_escrow"  # a contract whose doc currently matches its proof/
            shutil.copytree(ROOT / "src" / "contract" / c, t / "src" / "contract" / c,
                            ignore=shutil.ignore_patterns("target"))
            (t / "doc" / "src" / "contract").mkdir(parents=True)
            doc = t / "doc" / "src" / "contract" / f"{c}.md"
            shutil.copy(ROOT / "doc" / "src" / "contract" / f"{c}.md", doc)
            base = doc.read_text()

            # Control D1 — drop one circuit row: the table no longer matches proof/.
            m = re.search(r'^.*`[A-Za-z0-9_]+\.zk`.*\n', base, re.M)
            if not m:
                print("FAIL: --self-test could not find a circuit row to drop in the sample doc")
                return 1
            doc.write_text(base.replace(m.group(0), "", 1))
            _, _, f1, _ = scan(t)
            if not any(x[1].startswith("D1") for x in f1):
                print("FAIL: --self-test dropped a circuit row and D1 did not fire")
                return 1
            print("OK: --self-test — D1 reports a circuit table that does not match proof/")

            # Control D2 — a false "All N circuits" count beside a correct table.
            doc.write_text(base + "\nAll 99 circuits compiled to `.zk.bin`.\n")
            _, _, f2, _ = scan(t)
            if not any("count sentence says 99" in x[1] for x in f2):
                print("FAIL: --self-test added a false count sentence and D2 did not fire")
                return 1
            print("OK: --self-test — D2 reports a false 'All N circuits' count")
        return 0

    root = pathlib.Path(os.environ["CONTRACT_DOC_CIRCUITS_ROOT"])
    docs, with_table, findings, notes = scan(root)
    for name, msg in findings:
        print(f"FAIL: {name}: {msg}")
    for name, msg in notes:
        print(f"NOTE: {name}: {msg}")
    tag = "PASS" if not findings else "FAIL"
    print(f"[{tag}] docs_scanned={docs} with_table={with_table} notes={len(notes)} findings={len(findings)}")
    return 1 if findings else 0


sys.exit(main())
PY
