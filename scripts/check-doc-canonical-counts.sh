#!/usr/bin/env bash
#
# The counts a doc states about the tree must equal the tree.
#
# WHY THIS EXISTS. Population figures are the most drift-prone prose in the repository, and nothing
# reads them. Measured 2026-10-10: `for-researchers.md:14` says "**228 ZK circuits**", and the
# register says "Inventory: **180** circuits" at :461 while its own gate line at :820 says "178
# circuits walked". The tree holds 166 contract circuits (178 walked by the Orchard-class gate).
#
# WHAT IT CHECKS — and it is deliberately narrow, because a blanket "every N circuits" scanner is
# UNSOUND: docs legitimately state dozens of *local* counts ("2 circuits" for a feature, "150" for a
# historical population), and an earlier draft of this gate produced >10 false positives against them.
# So it fires only on the *canonical tree-population phrasings*, each with a derived truth:
#   P1 `N contract directories`            -> the count of `src/contract/*/`
#   P2 `N ... a proof/ directory`          -> the count of `src/contract/*/proof/`
#   P3 `Inventory: N circuits`             -> the gate count
#   P4 `gate walks ... N circuits`         -> the gate count
#   P5 `N `.zk` circuits`                  -> the `src/contract/*/proof/*.zk` count
#   P6 `NNN ZK circuits` (three-digit)     -> either count
# A number that is correct-in-context but not the derived value (a historical population, e.g. the
# audit's "144") is waived in `script/doc_count_exceptions.txt` (`path : number : reason`). The class
# this gate does NOT cover — an unflagged local count that has gone stale — is left to the LLM review
# pass; the gate records that limitation rather than pretending to close it.
#
# Usage: scripts/check-doc-canonical-counts.sh
#        scripts/check-doc-canonical-counts.sh --self-test   (planted defect; non-zero exit)
# Exit status: 1 on any finding; 0 otherwise.

set -uo pipefail

ROOT="${DOC_CANONICAL_COUNTS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
export DOC_CANONICAL_COUNTS_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, sys, pathlib

ROOT = pathlib.Path(os.environ["DOC_CANONICAL_COUNTS_ROOT"])

# (name, compiled regex with group(1)=number, kind) where kind is 'eq:<derived key>' or 'in:<key>,<key>'
PATTERNS = [
    ("P1", re.compile(r'(\d+)\s+contract directories\b'), "eq:contract_dirs"),
    ("P2", re.compile(r'(\d+)\s+(?:of them\s+)?(?:carrying|with)\s+a\s+`?proof/?`?'), "eq:with_proof"),
    ("P3", re.compile(r'Inventory:\s*\*{0,2}(\d+)\s+circuits'), "eq:circuits_gate"),
    ("P4", re.compile(r'gate walks[^0-9]{0,14}\*{0,2}(\d+)\s+circuits'), "eq:circuits_gate"),
    ("P5", re.compile(r'(\d+)\s+`?\.zk`?\s+circuits\b'), "eq:circuits_contract"),
    ("P6", re.compile(r'(\d{3})\s+ZK circuits\b'), "in:circuits_contract,circuits_gate"),
]


def derive(root):
    c = root / "src" / "contract"
    dirs = [d for d in c.iterdir() if d.is_dir()]
    with_proof = [d for d in dirs if (d / "proof").is_dir()]
    contract = list(c.glob("*/proof/*.zk"))
    gate = contract + list((root / "proofs" / "core").glob("*.zk")) \
        + list((root / "bin" / "darkirc" / "proof").glob("*.zk"))
    return {"contract_dirs": len(dirs), "with_proof": len(with_proof),
            "circuits_contract": len(contract), "circuits_gate": len(gate)}


def load_waivers(root):
    """path-substring -> set of numbers waived, from script/doc_count_exceptions.txt."""
    w = {}
    f = root / "script" / "doc_count_exceptions.txt"
    if f.exists():
        for line in f.read_text().splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split(":")]
            if len(parts) >= 2 and parts[1].isdigit():
                w.setdefault(parts[0], set()).add(int(parts[1]))
    return w


def scan(root):
    facts = derive(root)
    waivers = load_waivers(root)
    findings, hits = [], 0
    for doc in sorted((root / "doc" / "src").rglob("*.md")):
        rel = str(doc.relative_to(root))
        for i, line in enumerate(doc.read_text().splitlines(), 1):
            for pid, rx, kind in PATTERNS:
                for m in rx.finditer(line):
                    hits += 1
                    n = int(m.group(1))
                    if kind.startswith("eq:"):
                        expect = facts[kind[3:]]
                        ok = (n == expect)
                    else:
                        ok = (n in {facts[k] for k in kind[3:].split(",")})
                    waived = any(sub in rel for sub in waivers) and \
                        any(n in ws for sub, ws in waivers.items() if sub in rel)
                    if not ok and not waived:
                        want = facts[kind[3:]] if kind.startswith("eq:") else \
                            "/".join(str(facts[k]) for k in kind[3:].split(","))
                        findings.append((rel, i, f"{pid} says {n}, the tree holds {want}: {line.strip()[:90]}"))
    return facts, hits, findings


def main():
    if "--self-test" in sys.argv:
        findings = []
        # A planted "99 contract directories" must not match the derived 34.
        line = "The tree holds 99 contract directories and that is all."
        for pid, rx, kind in PATTERNS:
            for m in rx.finditer(line):
                if kind == "eq:contract_dirs" and int(m.group(1)) != 34:
                    findings.append((pid, m.group(1)))
        if not any(p == "P1" for p, _ in findings):
            print("FAIL: --self-test planted 99 contract directories and P1 did not fire")
            return 1
        print("OK: --self-test — P1 refuses a wrong contract-directory count")
        line2 = "Inventory: 180 circuits."
        n = [int(m.group(1)) for pid, rx, kind in PATTERNS if pid == "P3" for m in rx.finditer(line2)]
        if 180 not in n:
            print("FAIL: --self-test 'Inventory: 180 circuits' did not parse under P3")
            return 1
        if 180 == derive(pathlib.Path(os.environ["DOC_CANONICAL_COUNTS_ROOT"]))["circuits_gate"]:
            print("FAIL: --self-test the derived gate count equals 180; cannot exercise P3")
            return 1
        print("OK: --self-test — P3 parses and would reject 'Inventory: 180 circuits'")
        return 0

    root = pathlib.Path(os.environ["DOC_CANONICAL_COUNTS_ROOT"])
    facts, hits, findings = scan(root)
    for rel, i, msg in findings:
        print(f"FAIL: {rel}:{i}: {msg}")
    tag = "PASS" if not findings else "FAIL"
    print(f"[{tag}] derived={facts} claims_seen={hits} findings={len(findings)}")
    return 1 if findings else 0


sys.exit(main())
PY
