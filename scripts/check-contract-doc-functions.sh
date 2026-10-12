#!/usr/bin/env bash
#
# A contract doc's function/opcode table is a claim about its manifest.
#
# WHY THIS EXISTS. Three declarations of a contract's function surface exist: the manifest's
# `[[functions]]`, the `lib.rs` enum, and the doc's Function table. `check-manifest-entrypoint-
# agreement.sh` compares manifest <-> enum; the doc is compared to neither. A wrong opcode in a doc
# is what a reader copies into a client. This gate reads the pair nothing does.
#
# WHAT IT CHECKS. It parses the rows of the doc's function section(s) — a table under a heading
# naming *functions* or *entrypoints* — and, per row, takes the one `0xNN` code and the one
# `<Name>V<n>` identifier if the row holds exactly one of each (the row shapes differ across docs:
# name-first `| `NameV1` | 0x00 |`, code-first `| 0x00 | NameV1 |`, and code-in-parens
# `| 1 | `NameV1` (0x07) |` all satisfy it; error-code, capability and param tables carry no code
# and no versioned name, so they drop out):
#   F1 no doc row names a function the manifest lacks;
#   F2 for a function present in both, the doc's code equals the manifest's.
# A function the manifest declares that the doc's table does not list is a NOTE (coverage), not a
# failure: several docs document their functions in a state-machine diagram instead of the table, so
# an incomplete table is not by itself drift.
#
# It does NOT read `lib.rs` (the manifest<->enum gate owns that), and it ignores version suffixes on
# purpose (norm strips a trailing `vN`), because the manifest names are unversioned while enum/doc
# names are not — the V1/V2 axis is the client<->circuit pair, not this one.
#
# Usage: scripts/check-contract-doc-functions.sh
#        scripts/check-contract-doc-functions.sh --self-test   (planted defects; non-zero exit)
# Exit status: 1 on any finding; 0 otherwise.

set -uo pipefail

ROOT="${CONTRACT_DOC_FUNCTIONS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
export CONTRACT_DOC_FUNCTIONS_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, sys, tomllib, pathlib, tempfile, shutil

ROOT = pathlib.Path(os.environ["CONTRACT_DOC_FUNCTIONS_ROOT"])

NAME = re.compile(r'\b([A-Z][A-Za-z0-9]*V\d+)\b')
CODE = re.compile(r'0x[0-9a-fA-F]{1,4}\b')
HEAD = re.compile(r'^(#{2,4})\s+(.*)$')
FUNHEAD = re.compile(r'(contract\s+)?functions?\b|function\s+(ids|opcodes|surface|reference|table)|entrypoint', re.I)


def norm(s):
    s = re.sub(r'(?<=[a-z0-9])(?=[A-Z])', ' ', s)     # split camelCase
    s = re.sub(r'[^A-Za-z0-9]', '', s).lower()        # drop non-alphanumerics, lowercase
    return re.sub(r'v\d+$', '', s)                    # strip a trailing vN


def doc_rows(text):
    """normalized-name -> code, from rows under a functions/entrypoint heading.

    A row whose Function cell marks the entrypoint **removed/retired** is skipped:
    this gate compares what the doc *lists* against the manifest, and a retirement
    row claims the opposite — that the manifest does not have it. Skipping is
    restricted to the Function cell so a stray "removed" in a description cannot
    hide a wrong name. (Added 2026-10-12 for F-514/OBL-C232, where the row
    `| 0x0c | *(removed)* | ... |` was reported as a function the manifest lacks.)
    """
    rows, section = {}, None
    for line in text.splitlines():
        h = HEAD.match(line)
        if h:
            section = h.group(2)
            continue
        if not line.lstrip().startswith('|'):
            continue
        if not (section and FUNHEAD.search(section)):
            continue
        cells = [c.strip() for c in line.strip().strip('|').split('|')]
        if len(cells) >= 2 and re.search(r'\bremoved\b|\bretired\b', cells[1], re.I):
            continue
        codes, names = CODE.findall(line), NAME.findall(line)
        if len(codes) == 1 and len(names) == 1:
            rows.setdefault(norm(names[0]), int(codes[0], 16))
    return rows


def scan(root):
    findings, notes, docs, rows_total = [], [], 0, 0
    for man in sorted((root / "src/contract").glob("*/manifest.toml")):
        name = man.parent.name
        doc = root / "doc" / "src" / "contract" / f"{name}.md"
        if not doc.exists():
            continue
        docs += 1
        try:
            M = tomllib.loads(man.read_text())
        except Exception as e:
            findings.append((name, f"manifest does not parse: {e}"))
            continue
        mfun = {norm(f["name"]): f["code"] for f in M.get("functions", [])}
        rows = doc_rows(doc.read_text())
        rows_total += len(rows)
        if not rows:
            continue
        for k in sorted(set(rows) - set(mfun)):
            findings.append((name, f"F1 doc lists {k!r} (0x{rows[k]:02x}) — not in the manifest"))
        for k in sorted(set(rows) & set(mfun)):
            if rows[k] != mfun[k]:
                findings.append((name, f"F2 {k!r}: doc 0x{rows[k]:02x} vs manifest 0x{mfun[k]:02x}"))
        missing = sorted(set(mfun) - set(rows))
        if missing:
            notes.append((name, f"table lists {len(rows)} of {len(mfun)} manifest functions; missing {missing}"))
    return docs, rows_total, findings, notes


def main():
    if "--self-test" in sys.argv:
        with tempfile.TemporaryDirectory() as tmp:
            t = pathlib.Path(tmp)
            c = "attestation"  # a doc with a '## Contract Functions' table
            for sub in ("doc/src/contract", f"src/contract/{c}"):
                (t / sub).mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / "doc" / "src" / "contract" / f"{c}.md", t / "doc" / "src" / "contract" / f"{c}.md")
            shutil.copy(ROOT / "src" / "contract" / c / "manifest.toml", t / "src" / "contract" / c / "manifest.toml")
            doc = t / "doc" / "src" / "contract" / f"{c}.md"
            base = doc.read_text()

            # Control F1 — a function the manifest does not declare.
            anchor = next((l for l in base.splitlines()
                           if l.lstrip().startswith('|') and CODE.search(l) and NAME.search(l)), None)
            if anchor is None:
                print("FAIL: --self-test found no function row to anchor on")
                return 1
            doc.write_text(base.replace(anchor, anchor.rstrip('\n') + "\n| `ProbeFnV1` | 0x77 | phantom |", 1))
            _, _, f1, _ = scan(t)
            if not any(x[1].startswith("F1") for x in f1):
                print("FAIL: --self-test added a phantom function and F1 did not fire")
                return 1
            print("OK: --self-test — F1 reports a doc function absent from the manifest")

            # Control F2 — a wrong opcode for a function the manifest declares.
            m = re.search(r'0x[0-9a-fA-F]{1,4}', anchor)
            wrong = "0xee" if m.group(0).lower() != "0xee" else "0xdd"
            doc.write_text(base.replace(anchor, anchor[:m.start()] + wrong + anchor[m.end():], 1))
            _, _, f2, _ = scan(t)
            if not any(x[1].startswith("F2") for x in f2):
                print("FAIL: --self-test changed an opcode and F2 did not fire")
                return 1
            print("OK: --self-test — F2 reports a doc opcode that disagrees with the manifest")
        return 0

    root = pathlib.Path(os.environ["CONTRACT_DOC_FUNCTIONS_ROOT"])
    docs, rows, findings, notes = scan(root)
    for name, msg in findings:
        print(f"FAIL: {name}: {msg}")
    for name, msg in notes:
        print(f"NOTE: {name}: {msg}")
    tag = "PASS" if not findings else "FAIL"
    print(f"[{tag}] docs={docs} table_rows={rows} notes={len(notes)} findings={len(findings)}")
    return 1 if findings else 0


sys.exit(main())
PY
