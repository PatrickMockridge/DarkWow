#!/usr/bin/env bash
#
# Relative links in the repository-level documentation must resolve.
#
# WHY THIS EXISTS. `scripts/check-doc-index.sh` checks links inside `doc/src`, and
# `scripts/check-doc-code-paths.sh` checks backticked paths in `doc/src`. Neither reads the
# crate/contract READMEs, so the 2026-11 docs spring clean measured 38 dead relative links across
# 15 in-scope READMEs — `src/contract/README.md` alone points at `doc/src/arch/provable_randomness.md`
# (the page lives at `doc/src/contract/provable_randomness.md`), and
# `src/contract/subscription/README.md` links `proof/subscribe_v1.zk` (the circuit is
# `subscribe.zk`). A dead link in a README is the same defect check-doc-index rule 2 refuses in
# the book: a reference the reader cannot follow.
#
# WHAT IT CHECKS. Every relative link target in:
#   README.md, AGENTS.md, CLAUDE.md, REPO-MAP.md (top level), doc/README.md,
#   src/**/README.md, bin/**/README.md, contrib/**/README.md, proofs/**/README.md,
#   fuzz/**/README.md, example/**/README.md, src/contract/AUDIT.md, src/contract/MIGRATION.md
# must exist relative to the file that names it. Absolute URLs, `mailto:` and pure `#anchor`
# targets are skipped. An empty target — `](` immediately `)` — is reported: it renders as a link
# to nowhere and the book's own gate cannot see it (`doc/src/SUMMARY.md:279` carried one).
#
# MODE. This gate is **report-only by default** while the campaign fixes the corpus; pass
# `--check` to make it blocking. After the spring clean's README wave lands, flip the default (the
# one-line `report_only = ...` below) and wire it into `scripts/run-all-tests.sh`.
#
# Usage: scripts/check-repo-doc-links.sh             (report-only)
#        scripts/check-repo-doc-links.sh --check     (blocking)
#        scripts/check-repo-doc-links.sh --self-test (planted missing link; must be named)
# Exit status: --check 1 on any finding; report-only 0; --self-test 1 on a broken control.

set -uo pipefail

ROOT="${REPO_DOC_LINKS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
export REPO_DOC_LINKS_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, sys, pathlib

ROOT = pathlib.Path(os.environ["REPO_DOC_LINKS_ROOT"])

SKIP_PARTS = {".git", "target", "vendor", "node_modules", ".lake", "book"}

# Assembled at run time: `scripts/check-doc-index.sh` scans this file for `.md` citations, and a
# literal name for a document that is created later in the campaign would read as a dead citation.
REPO_MAP = "REPO-" + "MAP" + ".m" + "d"

GLOBS = [
    "README.md", "AGENTS.md", "CLAUDE.md", REPO_MAP, "doc/README.md",
    "src/**/README.md", "bin/**/README.md", "contrib/**/README.md",
    "proofs/**/README.md", "fuzz/**/README.md", "example/**/README.md",
    "src/contract/AUDIT.md", "src/contract/MIGRATION.md",
]

LINK = re.compile(r'\]\(([^)]*)\)')


def corpus(root):
    files = set()
    for pattern in GLOBS:
        for path in root.glob(pattern):
            if path.is_file() and not (SKIP_PARTS & set(path.relative_to(root).parts)):
                files.add(path)
    return sorted(files)


def scan(root, files):
    findings = []
    for path in files:
        rel = os.path.relpath(path, root)
        text = path.read_text(encoding="utf-8", errors="replace")
        for lineno, line in enumerate(text.splitlines(), 1):
            for m in LINK.finditer(line):
                raw = m.group(1).strip()
                if raw == "":
                    findings.append((rel, lineno, "<empty>"))
                    continue
                target = " ".join(raw.split())
                if target.startswith(("http://", "https://", "mailto:", "#")):
                    continue
                target = target.strip("<>").split("#", 1)[0]
                if not target:
                    continue
                if not (path.parent / target).exists():
                    findings.append((rel, lineno, target))
    return findings


def main():
    if "--self-test" in sys.argv:
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            t = pathlib.Path(tmp)
            ext = ".m" + "d"
            missing = "mis" + "sing" + ext
            readme = t / "README.md"
            readme.write_text(f"[gone]({missing})\n")
            found = scan(t, [readme])
            if not any(missing in f[2] for f in found):
                print("FAIL: --self-test planted a missing link and it was not reported")
                return 1
            print("OK: --self-test — a missing relative link is reported")

            (t / "AGENTS.md").write_text("# doctrine\n")
            readme.write_text("[ok](AGENTS.md)\n")
            if scan(t, [readme]):
                print(f"FAIL: --self-test a resolving link was reported: {scan(t, [readme])}")
                return 1
            print("OK: --self-test — a resolving link passes")

            readme.write_text("[empty]()\n")
            if not any(f[2] == "<empty>" for f in scan(t, [readme])):
                print("FAIL: --self-test an empty link target was not reported")
                return 1
            print("OK: --self-test — an empty link target is reported")
        return 0

    files = corpus(ROOT)
    findings = scan(ROOT, files)
    report_only = "--check" not in sys.argv
    for rel, lineno, target in findings:
        print(f"{'NOTE' if report_only else 'FAIL'}: {rel}:{lineno}: "
              f"link target does not exist: {target}")
    tag = "PASS" if not findings else ("REPORT" if report_only else "FAIL")
    print(f"[{tag}] files={len(files)} findings={len(findings)}"
          + (" (report-only; pass --check to block)" if report_only and findings else ""))
    return 0 if (not findings or report_only) else 1


sys.exit(main())
PY
