#!/usr/bin/env bash
#
# A doc's path citation is a claim about the tree.
#
# WHY THIS EXISTS. The docs-vs-code review found documents citing files that do not exist:
# `manifest_verify.rs` (wallet.md, wallet-vs-daemon.md, contract-trust-model.md, dev/contracts/
# identity.md), `bin/dww/src/rpc.rs` (manifest.md ×3), `bin/dww/src/cache.rs` (wallet_model.py),
# `bin/universal_relayer` (relayer.md, bridge-node.md), the `dwow_mm_*` FFI (merge-mining-ffi.md),
# `script/research/x3dh` (research.md) — docs-review F-324/327/328/331/616/617/619/620/621.
# `check-register-artifacts.sh` reads the *register's* path citations against HEAD; nothing read a
# doc's. This is that reader for `doc/src`.
#
# WHAT IT CHECKS. Every backticked path token in `doc/src/**/*.md` of the form
# `(src|bin|crates|proofs|contrib|script|scripts)/…` ending in a source extension must resolve to a
# file, from the repo root (the docs' convention). A token whose first path segment is not one of
# those repo roots is left alone — a doc may legitimately cite a path *inside* another repo, or a
# illustrative layout. A path that is deliberately aspirational or an example is declared, with a
# reason, in `script/doc_path_exceptions.txt` (the reason-per-entry idiom of
# `script/register_artifact_exceptions.txt`); a declared path that *does* resolve, or that the corpus
# no longer cites, is reported as a stale entry.
#
# Usage: scripts/check-doc-code-paths.sh
#        scripts/check-doc-code-paths.sh --self-test   (planted nonexistent path; non-zero exit)
# Exit status: 1 on any finding; 0 otherwise.

set -uo pipefail

ROOT="${DOC_CODE_PATHS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
export DOC_CODE_PATHS_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, sys, pathlib

ROOT = pathlib.Path(os.environ["DOC_CODE_PATHS_ROOT"])
DOCS = ROOT / "doc" / "src"
EXC = ROOT / "script" / "doc_path_exceptions.txt"

ROOTS = ("src/", "bin/", "crates/", "proofs/", "contrib/", "script/", "scripts/")
TOKEN = re.compile(
    r'`((?:src|bin|crates|proofs|contrib|script|scripts)/'
    r'[A-Za-z0-9_./-]*?\.(?:rs|zk|toml|md|py|lean|sh|json|txt))(?::[\d-]+)?`')


def resolves(root, tok):
    """The token exists at the repo root, or under `proofs/lean/` (the Lean tree's `src/` shorthand)."""
    return (root / tok).exists() or (root / "proofs" / "lean" / tok).exists()


def load_exceptions():
    """path -> set of finding-kind, from `path : reason` lines."""
    out = {}
    if EXC.exists():
        for line in EXC.read_text().splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split(":", 1)]
            if parts and parts[0]:
                out[parts[0]] = parts[1] if len(parts) > 1 else ""
    return out


def scan(root):
    exceptions = load_exceptions()
    findings, cited = [], set()
    for doc in sorted(root.rglob("*.md")):
        rel = os.path.relpath(doc, root)
        text = doc.read_text(encoding="utf-8", errors="replace")
        for m in TOKEN.finditer(text):
            tok = m.group(1)
            cited.add(tok)
            if "..." in tok:            # an elided path (`proofs/lean/.../X.lean`) — not a citation
                continue
            if not resolves(root, tok):
                if tok not in exceptions:
                    lineno = text[:m.start()].count("\n") + 1
                    findings.append((rel, lineno, tok))
    stale = [p for p in exceptions if p not in cited or resolves(root, p)]
    return findings, len(cited), stale


def main():
    if "--self-test" in sys.argv:
        import tempfile, shutil
        with tempfile.TemporaryDirectory() as tmp:
            t = pathlib.Path(tmp)
            (t / "script").mkdir()
            (t / "doc" / "src").mkdir(parents=True)
            dn = "a" + ".md"   # built so check-doc-index.sh's removed-document scan does not read it
            (t / "doc" / "src" / dn).write_text("see `src/does_not_exist_xyz.rs` for details\n")
            findings, _, _ = scan(t)
            if not any("src/does_not_exist_xyz.rs" in f[2] for f in findings):
                print("FAIL: --self-test planted a nonexistent path citation and it was not reported")
                return 1
            print("OK: --self-test — a doc citing a nonexistent path is reported")
            (t / "doc" / "src" / dn).write_text("see `src/tx/mod.rs` for details\n")
            (t / "src" / "tx").mkdir(parents=True)
            (t / "src" / "tx" / "mod.rs").write_text("// x\n")
            findings2, _, _ = scan(t)
            if findings2:
                print(f"FAIL: --self-test a resolving path was reported: {findings2}")
                return 1
            print("OK: --self-test — a resolving path passes")
        return 0

    root = pathlib.Path(os.environ["DOC_CODE_PATHS_ROOT"])
    findings, n_cited, stale = scan(root)
    for rel, lineno, tok in findings:
        print(f"FAIL: {rel}:{lineno}: cites {tok}, which does not exist")
    for p in stale:
        print(f"FAIL: stale exception for {p} — it resolves, or nothing cites it")
    tag = "PASS" if not findings and not stale else "FAIL"
    print(f"[{tag}] docs_cited={n_cited} findings={len(findings)} stale_exceptions={len(stale)}")
    return 1 if findings or stale else 0


sys.exit(main())
PY
