#!/usr/bin/env bash
#
# A doc's path citation is a claim about the tree.
#
# WHY THIS EXISTS. The docs-vs-code review found documents citing files that do not exist:
# `manifest_verify.rs` (wallet.md, wallet-vs-daemon.md, contract-trust-model.md, dev/contracts/
# identity.md), `bin/dww/src/rpc.rs` (manifest.md ×3), `bin/dww/src/cache.rs` (wallet_model.py),
# `bin/universal_relayer` (relayer.md, bridge-node.md), the `dwow_mm_*` FFI (merge-mining-ffi.md),
# `script/research/x3dh` (research.md) — docs-review F-324/327/328/331/616/617/619/620/621.
# The tracker gate resolves each row's locus; nothing read a *document's* path citations. This is
# that reader for `doc/src`.
#
# WHAT IT CHECKS. Every backticked path token in `doc/src/**/*.md` of the form
# `(src|bin|crates|proofs|contrib|script|scripts)/…` ending in a source extension must resolve to a
# file, from the repo root (the docs' convention). A token whose first path segment is not one of
# those repo roots is left alone — a doc may legitimately cite a path *inside* another repo, or a
# illustrative layout. A path that is deliberately aspirational or an example is declared, with a
# reason, in `script/doc_path_exceptions.txt` (the reason-per-entry idiom used throughout
# `script/*_exceptions.txt`); a declared path that *does* resolve, or that the corpus no longer
# cites, is reported as a stale entry.
#
# SCOPE. The corpus is `doc/src` — the published book, and exactly the documents the docs-vs-code
# review (2026-10-10) read and remediated. That is all of what this gate was built for: every
# citation it was written to catch (F-324/327/328/331/616/617/619/620/621) is in a `doc/src`
# document. It is deliberately NOT a repo-wide `.md` scan. A citation in a `doc/`-adjacent work
# product — a dated audit under `contrib/model/`, a campaign plan under `plans/`, a simulation
# report under `sim/`, an agent's scratch under `.claude/` — is a historical record or a proposal,
# not a claim about the tree that a reader acts on; a "stale" path in it is evidence of how the tree
# changed, and rewriting it would corrupt the record. Such trees are not exempted from a check; they
# are never in scope for one.
#
# EXCLUDED TREES. The traversal also refuses generated build caches and fetched/vendored
# dependencies — `**/.lake/**`, `target/`, `vendor/`, `node_modules/`. This repository does not own
# their contents and they change underfoot: a vendored README's own `scripts/...` citation (e.g.
# `proofs/lean/.lake/packages/batteries/README.md`) is a claim about *that* project, and it fails
# here only because the path is anchored to the wrong root. `doc/src` holds no such tree today; the
# guard is written out explicitly so that a widened corpus, or an override of
# `DOC_CODE_PATHS_ROOT`, cannot silently re-admit them.
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

ROOTS = ("src/", "bin/", "crates/", "proofs/", "contrib/", "script/", "scripts/")

# Path components that mark a tree this repository does not own: generated build caches and
# fetched/vendored dependencies. See "EXCLUDED TREES" in the header. Matched as a whole path
# component, so any depth of `**/.lake/**` (or `vendor/`, `target/`, `node_modules/`) is skipped.
SKIP_PARTS = {".lake", "target", "vendor", "node_modules", ".git"}
TOKEN = re.compile(
    r'`((?:src|bin|crates|proofs|contrib|script|scripts)/'
    r'[A-Za-z0-9_./-]*?\.(?:rs|zk|toml|md|py|lean|sh|json|txt))(?::[\d-]+)?`')


def resolves(root, tok):
    """The token exists at the repo root, or under `proofs/lean/` (the Lean tree's `src/` shorthand)."""
    return (root / tok).exists() or (root / "proofs" / "lean" / tok).exists()


def load_exceptions(root):
    """path -> reason, from `path : reason` lines under `root` (the temp root in --self-test)."""
    out = {}
    exc = root / "script" / "doc_path_exceptions.txt"
    if exc.exists():
        for line in exc.read_text().splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split(":", 1)]
            if parts and parts[0]:
                out[parts[0]] = parts[1] if len(parts) > 1 else ""
    return out


def scan(corpus, root):
    """Walk the document `corpus` (`doc/src`), resolving each cited token against repo root `root`."""
    exceptions = load_exceptions(root)
    findings, cited = [], set()
    for doc in sorted(corpus.rglob("*.md")):
        rel = os.path.relpath(doc, root)
        if SKIP_PARTS & set(pathlib.PurePath(rel).parts):
            continue                    # a build cache or vendored tree — never in scope
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
            findings, _, _ = scan(t / "doc" / "src", t)
            if not any("src/does_not_exist_xyz.rs" in f[2] for f in findings):
                print("FAIL: --self-test planted a nonexistent path citation and it was not reported")
                return 1
            print("OK: --self-test — a doc citing a nonexistent path is reported")
            (t / "doc" / "src" / dn).write_text("see `src/tx/mod.rs` for details\n")
            (t / "src" / "tx").mkdir(parents=True)
            (t / "src" / "tx" / "mod.rs").write_text("// x\n")
            findings2, _, _ = scan(t / "doc" / "src", t)
            if findings2:
                print(f"FAIL: --self-test a resolving path was reported: {findings2}")
                return 1
            print("OK: --self-test — a resolving path passes")
            # A declared exception whose path now resolves is covering nothing: the list must expire.
            (t / "script" / "doc_path_exceptions.txt").write_text(
                "src/tx/mod.rs : planted stale exception — this path resolves\n")
            _, _, stale = scan(t / "doc" / "src", t)
            if not any("src/tx/mod.rs" in p for p in stale):
                print(f"FAIL: --self-test a resolving declared exception was not reported stale: {stale}")
                return 1
            print("OK: --self-test — a declared exception that now resolves is reported stale")
        return 0

    root = pathlib.Path(os.environ["DOC_CODE_PATHS_ROOT"])
    findings, n_cited, stale = scan(DOCS, root)
    # **Blocking since 2026-10-21.** The negative-citation class is declared in
    # `script/doc_path_exceptions.txt` (reason per entry, stale entries fail), so a negative citation
    # is no longer indistinguishable from a stale one by string heuristics. `--report-only` remains
    # for authoring: it prints the same list and exits 0.
    report_only = "--report-only" in sys.argv
    for rel, lineno, tok in findings:
        print(f"{'NOTE' if report_only else 'FAIL'}: {rel}:{lineno}: cites {tok}, which does not exist")
    for p in stale:
        print(f"{'NOTE' if report_only else 'FAIL'}: stale exception for {p} — it resolves, or nothing cites it")
    clean = not findings and not stale
    tag = "PASS" if clean else ("REPORT" if report_only else "FAIL")
    print(f"[{tag}] docs_cited={n_cited} findings={len(findings)} stale_exceptions={len(stale)}")
    return 0 if (clean or report_only) else 1


sys.exit(main())
PY
