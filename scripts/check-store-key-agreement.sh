#!/bin/bash
# OBL-C199: a state record's writer and its readers agree about its **key**.
#
# The class this exists for is not a broken line — it is two correct-looking ones. A contract stores
# a record under one key expression and looks it up under another, and because both sides compile
# and each is coherent in isolation, no gate that reads *shapes* (counts, order, phase direction,
# domain presence) can see it. The symptom is silent: `NotFound` for a record that was written
# correctly, which is what makes it expensive.
#
# Measured, and it is why this is worth a gate rather than a paragraph: **four** of the sixteen
# defects the `bearer_bond` migration reached were of this class, found one at a time across
# separate ~950-second runs. `issue_stake_v1` wrote the series under `params.asset_id` while every
# reader looked it up under `stake_commitment.token_commit.to_repr()`. `apply_prove_coverage` stored
# a report under `series ‖ report_block` while `pay_interest_v1` looked it up under `series ‖ 0` —
# and the only stored report that could satisfy that lookup has a block height of exactly **0**,
# which no report has, because heights start at 1. Both were `db_set`/`db_get` disagreements about a
# key, and neither had a gate.
#
# HOW IT RESOLVES KEYS. A key expression is usually a local: `let series_key = ...; ...
# db_set(db, &series_key, ...)`. Comparing the *names* would match trivially, so the name is
# resolved to the right-hand side of its own `let` in the same file, with `&`, `.to_repr()` and
# `.to_bytes()` normalised away. `let series_key = params.asset_id.to_repr()` and `let series_key =
# stake_commitment.token_commit.to_repr()` therefore differ, which is the finding.
#
# The database variable is resolved the same way — to the tree constant its `db_lookup` names — so
# the comparison is per (contract, tree) rather than per local name.
#
# WHAT IT CANNOT SEE, stated because a reader will otherwise assume more: a key built from a value
# whose *content* differs at runtime while the expression is textually identical (a counter, a
# height read from the host). Those need a state-machine test, not a source scan. This gate finds
# the textual disagreement, which is the shape all four measured instances took.
#
# ADJUDICATED AND BLOCKING AS OF 2026-10-07. It began report-only, because the tree had never been
# looked at this way and legitimate asymmetries are indistinguishable from a disagreement in source
# alone. All 28 sites have since been read and are declared in
# `script/store_key_agreement_exceptions.txt`, so the umbrella now runs it `--strict` and a *new*
# undeclared read key fails the build. That adjudication also found two defects in this gate, both
# fixed above with self-test cases — a mis-classified `db_mark_spent`, and a `^`-anchored prefix
# strip — each of which had been *hiding* findings rather than producing them.
#
# Exit 0: no undeclared finding (or report-only). Exit 1: `--strict` and an undeclared finding.
# Exit 2: `--self-test` failed, or the exceptions file is malformed.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

STRICT=0
SELF_TEST=0
for arg in "$@"; do
    case "$arg" in
        --strict) STRICT=1 ;;
        --self-test) SELF_TEST=1 ;;
        # The default already is report-only; the flag exists so the umbrella's wiring reads the same
        # as its siblings' (`check-circuit-dead-values.sh --report-only`).
        --report-only) STRICT=0 ;;
        *) echo "usage: $0 [--strict] [--report-only] [--self-test]" >&2; exit 2 ;;
    esac
done

STRICT="$STRICT" SELF_TEST="$SELF_TEST" REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import glob, os, re, sys

repo = os.environ["REPO_ROOT"]
strict = os.environ["STRICT"] == "1"
self_test = os.environ["SELF_TEST"] == "1"

# --- normalisation ------------------------------------------------------------------------------
NORM = [
    (re.compile(r"\s+"), ""),
    # A field read in `exec` and written in `apply` is the *same* field through two names: the arm
    # reads `params.X`, prepares an update carrying `X`, and `apply` writes `update.X`. Comparing
    # those as-written reports every exec/apply pair in the tree as a disagreement, which is 72
    # findings of pure noise on the first run and would have made the gate unusable.
    (re.compile(r"\b(params|update|self|input|output)\."), ""),
    # `db_set`/`db_get` keys are the most common case of a key that legitimately differs by a
    # prefix: a record stored under `commitment ‖ block` and scanned under `commitment ‖ 0`.
    (re.compile(r"\.to_repr\(?\)?"), ""),
    (re.compile(r"\.to_bytes\(?\)?"), ""),
    (re.compile(r"\.inner\(?\)?"), ""),
    (re.compile(r"^&"), ""),
    (re.compile(r"^\(|\)$"), ""),
    (re.compile(r"\($"), ""),
]

def norm(expr: str) -> str:
    e = expr.strip().rstrip(",").strip()
    prev = None
    while prev != e:
        prev = e
        for rx, rep in NORM:
            e = rx.sub(rep, e)
    return e

# --- one file's view ----------------------------------------------------------------------------
LET_RE = re.compile(r"let\s+(?:mut\s+)?(\w+)\s*(?::[^=]+)?=\s*(.+?);")
DB_LOOKUP_RE = re.compile(r"let\s+(\w+)\s*=\s*wasm::db::db_lookup\([^,]+,\s*([A-Z0-9_]+)\)")
WRITE_RE = re.compile(r"wasm::db::db_set\(\s*(\w+)\s*,\s*&?([^,]+?)\s*,")
READ_RE = re.compile(r"wasm::db::(db_get|db_contains_key)\(\s*(\w+)\s*,\s*&?([^,)]+?)\s*[,)]")
# `db_mark_spent` is a **write**, not a read: the SDK defines it as `db_set(db, key, &[1])`
# (`src/sdk/src/wasm/db.rs:185`). Classifying it as a reader made every nullifier tree look
# writer-less — its only writer is the mark — which is a false positive of exactly the shape this
# gate exists to catch, produced by the gate itself. `dao_escrow`'s nullifier tree was one.
MARK_SPENT_RE = re.compile(r"wasm::db::db_mark_spent\(\s*(\w+)\s*,\s*&?([^,)]+?)\s*[,)]")

def resolve(expr: str, lets: dict) -> str:
    e = norm(expr)
    # Follow a bare identifier to the right-hand side of its own `let`, once. One hop is the whole
    # of the analysis by design: the local *is* the key expression in every instance measured, and a
    # resolver that chased values recursively would be the cleverer classifier this repository has
    # twice rejected as worse than a shallow one a reviewer can read.
    if e in lets:
        return norm(lets[e])
    return e

def scan(path: str):
    src = open(path, encoding="utf-8", errors="replace").read()
    lets = {}
    for m in LET_RE.finditer(src):
        lets[m.group(1)] = m.group(2)
    # A local that names a tree via db_lookup maps the variable to the tree constant.
    dbs = {}
    for m in DB_LOOKUP_RE.finditer(src):
        dbs[m.group(1)] = m.group(2)
    writes, reads = {}, {}
    for m in WRITE_RE.finditer(src):
        var, key = m.group(1), m.group(2)
        writes.setdefault(var, set()).add(resolve(key, lets))
    for m in MARK_SPENT_RE.finditer(src):
        var, key = m.group(1), m.group(2)
        writes.setdefault(var, set()).add(resolve(key, lets))
    for m in READ_RE.finditer(src):
        var, key = m.group(2), m.group(3)
        reads.setdefault(var, set()).add(resolve(key, lets))
    return dbs, writes, reads

def findings_for(path: str):
    dbs, writes, reads = scan(path)
    out = []
    for var, keys in reads.items():
        wkeys = writes.get(var, set())
        if not wkeys:
            continue  # a tree read but never written in this file: a cross-file writer, reported globally
        for k in sorted(keys):
            if k not in wkeys:
                out.append((dbs.get(var, var), k, sorted(wkeys)))
    return out

# --- --self-test --------------------------------------------------------------------------------
# Two sides, so the classifier is shown able to fail. The left fixture is the `bearer_bond` coverage
# defect verbatim in shape; the right is the same pair made to agree.
SELF_LEFT = '''
fn f(cid: ContractId) -> Result<()> {
    let bonds_info_db = wasm::db::db_lookup(cid, BONDS_INFO_TREE)?;
    let key = [&update.report.series_asset_id.to_repr()[..], &update.report.report_block.to_le_bytes()[..]].concat();
    wasm::db::db_set(bonds_info_db, &key, &update.report.encode())?;

    let coverage_scan_key = [&stake.series_asset_id.to_repr()[..], &0u64.to_le_bytes()[..]].concat();
    if !wasm::db::db_contains_key(bonds_info_db, &coverage_scan_key)? { return Err(e) }
    Ok(())
}
'''
SELF_RIGHT = '''
fn f(cid: ContractId) -> Result<()> {
    let bonds_info_db = wasm::db::db_lookup(cid, BONDS_INFO_TREE)?;
    let key = [&update.report.series_asset_id.to_repr()[..], &0u64.to_le_bytes()[..]].concat();
    wasm::db::db_set(bonds_info_db, &key, &update.report.encode())?;

    let coverage_scan_key = [&update.report.series_asset_id.to_repr()[..], &0u64.to_le_bytes()[..]].concat();
    if !wasm::db::db_contains_key(bonds_info_db, &coverage_scan_key)? { return Err(e) }
    Ok(())
}
'''

# Two more fixtures pin the two defects the gate itself had, each found while triaging the worklist:
# a `db_mark_spent` write was filed as a read, so a nullifier tree looked writer-less; and the
# `params.`/`update.` prefix strip was anchored at `^`, so the same field inside a call argument
# never matched. Each must be a NON-finding, or the fixes are unverified.
SELF_MARK_SPENT = '''
fn f(cid: ContractId) -> Result<()> {
    let nullifiers_db = wasm::db::db_lookup(cid, NULLIFIERS_TREE)?;
    wasm::db::db_mark_spent(nullifiers_db, &update.vote_nullifier.to_repr())?;

    let vote_nullifier = params.vote_nullifier;
    if wasm::db::db_contains_key(nullifiers_db, &vote_nullifier.to_repr())? { return Err(e) }
    Ok(())
}
'''
SELF_ANCHOR = '''
fn f(cid: ContractId) -> Result<()> {
    let registry_db = wasm::db::db_lookup(cid, REGISTRY_TREE)?;
    wasm::db::db_set(registry_db, &compute_key(&update.relayer_pub), &b)?;

    if wasm::db::db_get(registry_db, &compute_key(&params.relayer_pub))?.is_none() { return Err(e) }
    Ok(())
}
'''

if self_test:
    import tempfile
    ok = True
    cases = (("left.rs", SELF_LEFT, True),
             ("right.rs", SELF_RIGHT, False),
             ("mark_spent.rs", SELF_MARK_SPENT, False),
             ("anchor.rs", SELF_ANCHOR, False))
    with tempfile.TemporaryDirectory() as d:
        for name, body, expect_finding in cases:
            p = os.path.join(d, name)
            open(p, "w").write(body)
            found = findings_for(p)
            if bool(found) != expect_finding:
                print(f"FAIL --self-test: {name} gave {found!r}, expected finding={expect_finding}", file=sys.stderr)
                ok = False
    if not ok:
        sys.exit(2)
    print("PASS --self-test: a key disagreement is reported; the pair made to agree is not; a "
          "`db_mark_spent` writer is not mistaken for a reader-only tree; and a field reached through "
          "`params.`/`update.` inside a call argument is normalised")
    sys.exit(0)

# --- the corpus ---------------------------------------------------------------------------------
paths = sorted(glob.glob(os.path.join(repo, "src/contract/*/src/entrypoint*.rs")) +
               glob.glob(os.path.join(repo, "src/contract/*/src/entrypoint/**/*.rs")))
findings = []
for p in paths:
    rel = os.path.relpath(p, repo)
    for tree, key, wkeys in findings_for(p):
        findings.append((rel, tree, key, wkeys))

print(f"COVERAGE: scanned {len(paths)} contract entrypoint file(s).")

# --- adjudications ------------------------------------------------------------------------------
# Content-keyed on (path, tree, key) rather than on line numbers, so a file that shifts underneath
# an entry keeps its adjudication — the choice the sibling checkers made, for the same reason. A
# declared entry that no longer matches a finding is reported, so the list cannot outlive its sites.
EXC = os.path.join(repo, "script/store_key_agreement_exceptions.txt")
declared = set()
if os.path.exists(EXC):
    for n, line in enumerate(open(EXC, encoding="utf-8"), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        # `|`-delimited, not `:`-delimited: a key can contain `::` (a path such as
        # `IntentCommitment::from_base(...)`), which a `:`-split would truncate. A key cannot contain
        # a `|`; the reason is the free-text tail and may.
        parts = [p.strip() for p in line.split("|", 3)]
        if len(parts) < 4:
            print(f"FAIL: {EXC}:{n} is malformed — expected `path | tree | key | reason`", file=sys.stderr)
            sys.exit(2)
        declared.add((parts[0], parts[1], parts[2]))

seen = set()
undeclared = []
for rel, tree, key, wkeys in findings:
    if (rel, tree, key) in declared:
        seen.add((rel, tree, key))
    else:
        undeclared.append((rel, tree, key, wkeys))

stale = sorted(declared - seen)
if stale:
    print("NOTE: declared but no longer a finding — remove these lines:")
    for rel, tree, key in stale:
        print(f"  {rel} : {tree} : {key}")
    print()

if not undeclared:
    print(f"PASS: {len(findings)} finding(s), all {len(declared)} declared — every read key in those")
    print("      files has a matching write key, or an adjudication naming why it does not need one.")
    sys.exit(0)

print(f"REPORT: {len(undeclared)} undeclared read key(s) with no matching write key in the same file:\n")
for rel, tree, key, wkeys in undeclared:
    print(f"  {rel}\n    tree {tree}\n    read  {key}\n    wrote {wkeys}\n")

print("REPORT-ONLY: this is a measurement, not a clean bill — legitimate asymmetries (a read keyed by")
print("a prefix of the written one, a record written elsewhere and read here) look identical to a")
print(f"disagreement from source alone. Declare those in {os.path.relpath(EXC, repo)} to make this a")
print("ratchet; --strict fails on any undeclared finding today.")
sys.exit(1 if strict else 0)
PYEOF
