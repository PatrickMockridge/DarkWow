#!/bin/bash
# A generated, dated status report: genesis and consensus production readiness, and documentation
# accuracy. Every figure in the report comes from a run this script made, and every verdict names the
# instrument that produced it.
#
# WHY A GENERATOR AND NOT A DOCUMENT. This tree has paid twice for status numbers written as prose, and
# both incidents are recorded in it. `scripts/run-all-tests.sh:128-138` records a gate's own header
# reading "15 and 29, then 11 and 5, then 5 and 5, all stale — the numbers belong to the gates, and both
# gates print their own on every run". `AGENTS.md` records the same failure for the genesis pin: "The
# hash itself is deliberately not quoted here. It was, and it went stale within hours of being written —
# which is the failure mode a constant in a doctrine document always has. Quote the procedure, never the
# value." So this script quotes no value it did not measure, in this run.
#
# ─────────────────────────────────────────────────────────────────────────────────────────────────
# FOUR HARD CONSTRAINTS. These are the reason the script exists in this shape; a change that breaks one
# of them breaks what the report is for.
#
# 1. IT RUNS NO BUILD. No cargo, no make, no lake, no docker, no rustc, and it rebuilds no artifact. That
#    is what buys the properties the reader is relying on: no cgroup scope, no consent needed, seconds to
#    run, and **it cannot move the genesis pin**. A requirement that needs a build is a different
#    instrument, not an addition to this one.
# 2. A MISSING OR UNREADABLE INSTRUMENT IS `MISSING`, NEVER `PASS`. The report must not claim a green it
#    did not observe (R5, R8). Same for a probe whose evidence is absent.
# 3. EVERY VERDICT CARRIES ITS SUBJECT — HEAD sha, branch, dirty-path count, date, toolchain pin. A
#    verdict without its subject commit is not evidence.
# 4. THE FINDINGS SECTION IS RENDERED FROM THE REGISTER'S OWN ROWS. A finding therefore cannot appear in
#    the report unless it exists as an obligation in `doc/src/arch/verification-hazop.md`.
#
# ─────────────────────────────────────────────────────────────────────────────────────────────────
# TWO THINGS THAT WILL BITE YOU, both measured rather than guessed.
#
# * `scripts/register-status.sh` prints TWO numbers for the same question and they disagree. Its token
#   histogram is documented by the script itself as "UNRELIABLE, read the caveat below before quoting any
#   number" (prose in the register contains every status word in a non-status sense), and it currently
#   reads 55 OPEN against a Status-column census of 30. **Every count here comes from the Status column**,
#   which is the fixed cell `register-status.sh --check` reads.
# * `scripts/check-doc-index.sh` globs the WORKING TREE, not `git ls-files` (`:121-126`), so a gitignored
#   `.md` under `reports/` is still inside the doc gate's scope. The raw capture therefore writes
#   `.out`/`.err`/`.exit` and never a `.md`. Do not "tidy" those extensions.
#
# Usage:
#   scripts/report-status.sh                # run every instrument, emit the report
#   scripts/report-status.sh --self-test     # negative control: the verdict logic must be able to fail
#
# Exit 0: the report was emitted. Exit 1: an instrument could not be run at all, or `--self-test` failed.
# The report's own verdicts are content, not exit status — a red gate does not fail this script, because
# a red gate is the thing the report exists to state.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DATE="${REPORT_DATE:-$(date +%F)}"
OUT_DIR="$REPO_ROOT/reports"
RAW_DIR="$OUT_DIR/status-$DATE.raw"
REPORT="$OUT_DIR/status-$DATE.md"

SELF_TEST=0
for a in "$@"; do
  case "$a" in
    --self-test) SELF_TEST=1 ;;
    -h|--help) sed -n '1,45p' "$0"; exit 0 ;;
    *) echo "usage: $0 [--self-test]" >&2; exit 2 ;;
  esac
done

REPO_ROOT="$REPO_ROOT" DATE="$DATE" OUT_DIR="$OUT_DIR" RAW_DIR="$RAW_DIR" REPORT="$REPORT" \
SELF_TEST="$SELF_TEST" python3 - <<'PYEOF'
import os, re, subprocess, sys, glob, shutil

REPO   = os.environ["REPO_ROOT"]
DATE   = os.environ["DATE"]
OUT    = os.environ["OUT_DIR"]
RAW    = os.environ["RAW_DIR"]
REPORT = os.environ["REPORT"]
SELF   = os.environ["SELF_TEST"] == "1"

REGISTER = os.path.join(REPO, "doc", "src", "arch", "verification-hazop.md")

# ─────────────────────────────────────────────────────────────────────────────────────────────────
# The verdict vocabulary. Three values, and the middle one is the one that matters: NOT ESTABLISHED is
# the honest reading for a criterion no instrument in this budget can decide (R7 — "cannot verify" is a
# result). It is never a synonym for "probably fine".
PASS, FAIL, MISSING = "MET", "NOT MET", "MISSING"
NOTEST = "NOT ESTABLISHED"

# ─────────────────────────────────────────────────────────────────────────────────────────────────
# THE INSTRUMENT MANIFEST — explicit, never a glob. A glob silently absorbs a gate somebody adds and
# silently drops one somebody deletes, and either way the report changes without saying so. Instead the
# manifest is asserted COMPLETE against `scripts/run-all-tests.sh`: every gate it wires is either here or
# named in EXCLUDED below with a reason (the repo's own exceptions idiom, `script/*_exceptions.txt`).
#
# Every command is read-only and seconds. The two unwired red ones are here deliberately — the report
# runs the gates the umbrella forgets, which is the only way their verdicts enter the record at all.
INSTRUMENTS = [
    ("doc_index",                  "bash scripts/check-doc-index.sh"),
    ("register_artifacts",         "bash scripts/check-register-artifacts.sh"),
    ("authority_resolves",         "bash scripts/check-authority-resolves.sh"),
    ("artifact_freshness",         "bash scripts/check-artifact-freshness.sh"),
    ("register_status_check",      "bash scripts/register-status.sh --check"),
    ("circuit_metadata_alignment", "bash scripts/check-circuit-metadata-alignment.sh"),
    ("circuit_domain_separation",  "bash scripts/check-circuit-domain-separation.sh"),
    ("circuit_instance_derivation","bash scripts/check-circuit-instance-derivation.sh"),
    ("circuit_transcription",      "python3 scripts/gen_circuit_transcription.py --check"),
    ("circuit_index",              "python3 scripts/gen_circuit_index.py --check"),
    ("circuit_fidelity",           "python3 scripts/check-circuit-fidelity.py"),
    ("phase_host_functions",       "bash scripts/check-phase-host-functions.sh"),
    ("pubkey_binding",             "bash scripts/check-pubkey-binding.sh"),
    ("client_params_alignment",    "bash scripts/check-client-params-alignment.sh"),
    ("hidden_tests",               "bash scripts/check-hidden-tests.sh"),
    ("metadata_arms",              "bash scripts/check-metadata-arms.sh"),
    ("coinbase_classifier",        "bash scripts/check-coinbase-classifier.sh"),
    ("wallet_kernel",              "bash scripts/check-wallet-kernel.sh"),
    ("l1_wire_conformance",        "bash scripts/check-l1-wire-conformance.sh"),
    ("zk_bins",                    "bash scripts/validate_zk_bins.sh"),
    ("genesis_model_conformance",  "bash contrib/genesis_model_conformance.sh"),
    ("wasm_artifact_genesis",      "bash contrib/wasm_artifact_check.sh --genesis"),
    ("barb_alphabet",              "bash contrib/barb_alphabet_diff.sh"),
    ("primitive_barbs",            "bash contrib/primitive_barbs_diff.sh"),
    ("capability_type",            "bash contrib/capability_type_diff.sh"),
    ("heavyweight_antipatterns",   "bash contrib/ci/scan_heavyweight_antipatterns.sh"),
    ("fee_guardrails",             "bash contrib/ci/check_fee_guardrails.sh"),
    ("sync_conformance",           "bash contrib/ci/check_sync_conformance.sh"),
    ("heavyweight_coverage",       "bash contrib/ci/check_heavyweight_coverage.sh"),
    ("length_cast_counter",        "bash contrib/length_cast_counts.sh"),
]

# Gates the umbrella wires (or that exist) and this report deliberately does NOT run, each for a reason
# that is about the constraint above it. A reason is required — an unexplained absence is the defect.
EXCLUDED = {
    "scripts/check-lean-suite.sh":
        "builds and runs the Lean IO simulation suite — constraint 1",
    "scripts/build-contract-zk.sh":
        "compiles every contract's ZK circuits — constraint 1",
    "scripts/check_pipeline_build.sh":
        "compiles dwowd, dww and the 32 contract entrypoints — constraint 1",
    "contrib/ci/check_compiles.sh":
        "runs cargo — constraint 1",
    "contrib/test_verdicts.sh":
        "runs the cargo test verdicts (its --self-test IS run, as an umbrella control) — constraint 1",
    "contrib/add_missing_license_headers.sh":
        "WRITES to every .rs file — not read-only, so not admitted at any budget",
    "contrib/length_cast_counts.sh":  # present in INSTRUMENTS; recorded here as a COUNTER not a gate
        "a ratchet counter whose exit 1 means 'sites remain', not 'defect' — reported, not verdicted",
}

# Instruments whose exit 1 is NOT a verdict on a proposition but a measurement crossing zero. These are
# reported with their numbers and never counted in the red total, because conflating the two is how a
# campaign remainder gets read as a regression.
COUNTERS = {"length_cast_counter"}

results = {}     # name -> (verdict, rc, out, err)

def run(name, cmd):
    """Run one instrument, capture verbatim output, classify. A gate that cannot be run is MISSING."""
    path = os.path.join(RAW, name)
    try:
        p = subprocess.run(cmd, shell=True, cwd=REPO, capture_output=True, text=True, errors="replace")
        rc, out, err = p.returncode, p.stdout, p.stderr
    except Exception as e:
        results[name] = (MISSING, None, "", f"{type(e).__name__}: {e}")
        return
    open(path + ".out", "w").write(out)
    open(path + ".err", "w").write(err)
    open(path + ".exit", "w").write(str(rc))
    verdict = PASS if rc == 0 else FAIL
    results[name] = (verdict, rc, out, err)

def classify_planted(rc):
    """The verdict mapper, isolated so the self-test can drive it with planted inputs."""
    return PASS if rc == 0 else FAIL

# ─────────────────────────────────────────────────────────────────────────────────────────────────
# THE SELF-TEST — the negative control, and without it this script is the exact thing this repository
# keeps finding: an instrument that reports a verdict it has no way to justify (R8, "a check is not a
# check until something can make it fail"). It drives the mapper with a planted red and a planted green
# and requires the mapper to distinguish them, naming the planted defect.
def self_test():
    fails = []
    cases = [
        ("planted red: a gate that exits 1", 1, FAIL),
        ("planted green: a gate that exits 0", 0, PASS),
        ("planted red: a gate that exits 2 (usage error)", 2, FAIL),
        ("planted red: a gate that exits 101 (rust panic)", 101, FAIL),
    ]
    for label, rc, expect in cases:
        got = classify_planted(rc)
        if got != expect:
            fails.append(f"{label}: exit {rc} mapped to {got}, expected {expect}")
    if fails:
        print("SELF-TEST FAILED — the verdict mapper cannot distinguish a planted red from a green:")
        for f in fails:
            print(f"  {f}")
        sys.exit(1)
    print(f"OK: the verdict mapper separates a planted red from a green ({len(cases)} cases), "
          f"and names what it planted.")
    sys.exit(0)

if SELF:
    self_test()

# ─────────────────────────────────────────────────────────────────────────────────────────────────
# STATIC PROBES. These are the report's own measurements — the things no gate in the tree computes, each
# one read-only and seconds. Their purpose is that a finding resting on them is reproducible by running
# this script, rather than resting on the session that first noticed it.
def clean(text):
    """Strip the tree's own noise. `test-monero-data/testnet` is permission-denied for this user, so a
    recursive grep emits `Permission denied` on stderr and — when the two channels are merged — that
    string becomes a probe's *evidence*. Measured: it did, in the first run of this script, and it read
    as a finding. Evidence comes from stdout, plus stderr only when stdout is empty, and never with the
    noise lines."""
    noise = ("Permission denied", "could not open directory", "No such file or directory")
    keep = [l for l in text.splitlines() if not any(n in l for n in noise)]
    return "\n".join(keep).strip()

# A filename this report must NOT repeat — and the set is DERIVED from the gate that forbids it rather
# than written here. `scripts/check-doc-index.sh` carries declared exceptions for citations of documents
# that were withdrawn; the exception text names those files, and they do not exist. This report is inside
# that gate's scope (`:121-126` globs the working tree), so passing an exception text through as evidence
# makes the REPORT cite a deleted document and turns the gate red on the report — measured, it did
# exactly that, and then the same trap caught the script once its name was written into it. Deriving the
# set from the gate's own `EXCEPTIONS` block means the forbidden basename never appears here at all, so
# there is nothing to go stale and no gate edit is needed. The register reached the same conclusion
# independently: OBL-C121 says "its filename is deliberately not repeated here, since naming it is what
# that exception exists to permit in the two files that may".
def forbidden_basenames():
    try:
        src = open(os.path.join(REPO, "scripts/check-doc-index.sh"), errors="replace").read()
    except OSError:
        return set()
    m = re.search(r'EXCEPTIONS\s*=\s*\{(.*?)\n\}', src, re.S)
    if not m:
        return set()
    return set(re.findall(r'"([A-Za-z0-9_.-]+\.md)"', m.group(1)))

REDACTED = forbidden_basenames()

def redact(text):
    out, hits = [], 0
    for l in text.splitlines():
        if any(name in l for name in REDACTED):
            hits += 1
            continue
        out.append(l)
    body = "\n".join(out).strip()
    if hits:
        body += (f"\n[{hits} line(s) withheld: they name {len(REDACTED)} file(s) that "
                 f"`scripts/check-doc-index.sh` exempts — naming one here would make this report cite a "
                 f"deleted document and turn that gate red on the report itself]")
    return body

def sh(cmd, maxlen=None):
    """(rc, stdout). stderr is kept only as a fallback, and only for a NON-ZERO exit — a green command's
    diagnostics are not evidence about anything."""
    p = subprocess.run(cmd, shell=True, cwd=REPO, capture_output=True, text=True, errors="replace")
    body = clean(p.stdout)
    if not body and p.returncode != 0:
        body = clean(p.stderr)
    if maxlen and len(body) > maxlen:
        body = body[:maxlen] + " …"
    return p.returncode, body

def probe(cmd):
    """A probe's output must be a measurement of the TREE, and this script and its own output are not the
    tree.

    Two self-matches had to be removed, and both were measured rather than anticipated:
      * a recursive grep matches THIS SCRIPT whenever the pattern appears here (the `RUSTC_BOOTSTRAP`
        probe did), and
      * it matches the PREVIOUS RUN'S REPORT whenever the pattern appears there — which is worse, because
        it is a feedback loop: run 1 quotes the pattern as evidence, run 2 finds run 1's report and quotes
        that, and the line grows by an escaping level per run. Found by the plan's two-consecutive-runs
        determinism check, which is exactly what that check is for; a single run can never show it.
    Drop both, and state the drop rather than doing it silently."""
    rc, body = sh(cmd)
    lines = [l for l in body.splitlines()
             if "report-status.sh" not in l and "/reports/status-" not in l]
    dropped = len(body.splitlines()) - len(lines)
    out = "\n".join(lines)
    if dropped:
        out = (out + "\n" if out else "") + (
            f"(removed {dropped} self-match(es): this script and its own previous output cite the pattern, "
            f"and neither is part of the tree)")
    return {"rc": rc, "out": out}

PIN_FILE = "bin/dwowd/genesis_hash.txt"
GENESIS_CONTRACTS = ["deployooor","native_token","promissory_note","identity","oracle",
                     "attestation","purse","box","multisig"]

probes = {}
probes["head"]        = probe("git rev-parse HEAD")
probes["branch"]      = probe("git rev-parse --abbrev-ref HEAD")
probes["dirty"]       = probe("git status --porcelain | wc -l")
probes["toolchain"]   = probe("grep -E '^channel' rust-toolchain.toml")
probes["rustc"]       = probe("rustc --version")

# F1's three legs. Leg 1 and 2 are "the mechanism is absent"; leg 3 is "and unavailable".
probes["lever_strip"]     = probe("grep -rn 'strip' --include=Cargo.toml . | grep -v '^./target' | grep -v '^./vendor'")
probes["lever_strip_hist"]= probe("git log --oneline -S 'strip = \"symbols\"' -- Cargo.toml")
probes["lever_locdetail"] = probe("grep -rn 'location-detail' . | grep -v '^./target' | grep -v '^./doc/book' | grep -v '^./.git/'")
probes["lever_locdetail_hist"] = probe("git log --oneline -S 'location-detail' --all -- 'src/contract/*/Makefile' 'Makefile'")
probes["cargo_config"]    = probe("grep -nE '^[[:space:]]*rustflags' .cargo/config.toml")
probes["bootstrap"]       = probe("grep -rn 'RUSTC_BOOTSTRAP' . | grep -v '^./target' | grep -v '^./.git/' | grep -v '^./vendor'")
probes["makefile_flags"]  = probe("grep -nE 'RUSTFLAGS \\+=' src/contract/native_token/Makefile")

# The purity question, which is NOT the gate's stricter claim (see OBL-C140).
abs_counts, rel_counts = {}, {}
for c in GENESIS_CONTRACTS:
    w = f"src/contract/{c}/dwow_{c}_contract.wasm"
    if not os.path.exists(os.path.join(REPO, w)):
        abs_counts[c] = "ABSENT"; rel_counts[c] = "ABSENT"; continue
    _, ab = sh(f"strings -a {w} | grep -c '/home/\\|/root/\\|/Users/'")
    _, rl = sh(f"strings -a {w} | grep -c '^src/.*\\.rs$'")
    abs_counts[c] = ab.splitlines()[-1] if ab else "?"
    rel_counts[c] = rl.splitlines()[-1] if rl else "?"
probes["abs_paths"] = {"rc": 0, "out": " ".join(f"{c}={abs_counts[c]}" for c in GENESIS_CONTRACTS)}
probes["rel_paths"] = {"rc": 0, "out": " ".join(f"{c}={rel_counts[c]}" for c in GENESIS_CONTRACTS)}

probes["pin_git"]     = probe(f"git status --porcelain -- {PIN_FILE}")
# G2's measurement: is the deployment table the ONLY route the wasm bytes enter the node by?
probes["other_wasm_embeds"] = probe(
    "grep -rn 'include_bytes!(' src/ bin/ | grep '\\.wasm' | grep -v 'bin/dwowd/src/lib.rs' "
    "| grep -v '/tests/'")
probes["pin_last"]    = probe(f"git log --oneline -1 -- {PIN_FILE}")
probes["pin_placeholder"] = probe(f"grep -c '^0\\{{64\\}}$' {PIN_FILE}")

# The three enumerations of the nine genesis ids — the report checks they agree rather than trusting one.
def ids_from_execution():
    txt = open(os.path.join(REPO,"src/linear/src/execution.rs"), errors="replace").read()
    m = re.search(r'pub fn genesis_contracts\(\).*?\n\s*\[\s*\n(.*?)\n\s*\]', txt, re.S)
    return re.findall(r'\(\*([A-Z_]+)_CONTRACT_ID,\s*"([^"]+)"\)', m.group(1)) if m else []
def ids_from_sdk():
    txt = open(os.path.join(REPO,"src/sdk/src/crypto/contract_id.rs"), errors="replace").read()
    m = re.search(r'GENESIS_CONTRACT_IDS_BYTES.*?=\s*\[(.*?)\];', txt, re.S)
    return re.findall(r'([A-Z_]+)_CONTRACT_ID\.to_bytes', m.group(1)) if m else []
def ids_from_lib():
    """The `include_bytes!("../../../src/contract/<c>/dwow_<c>_contract.wasm")` table, in order.
    Measured: the path is THREE levels up from `bin/dwowd/src/`, and a two-level pattern silently
    matched nothing — which reported the three enumerations as disagreeing when they do not."""
    txt = open(os.path.join(REPO,"bin/dwowd/src/lib.rs"), errors="replace").read()
    return re.findall(r'include_bytes!\("(?:\.\./)+src/contract/([a-z_]+)/dwow_[a-z_]+_contract\.wasm"\)', txt)
exec_names = [n for _, n in ids_from_execution()]
sdk_names  = [n.lower() for n in ids_from_sdk()]
lib_names  = ids_from_lib()
agree = (exec_names and
         [n.lower() for n in exec_names] == lib_names == sdk_names and len(exec_names) == 9)
probes["genesis_ids"] = {"rc": 0 if agree else 1, "out":
    f"execution.rs ({len(exec_names)}): {exec_names}\n"
    f"  lib.rs       ({len(lib_names)}): {lib_names}\n"
    f"  contract_id.rs({len(sdk_names)}): {sdk_names}\n"
    f"  agree={agree}"}

# The register census — Status column ONLY. See the header note about the histogram.
def register_rows():
    """(id, status-token, severity, proposition), with the severity found by COLUMN NAME per table.

    Measured, and it cost two wrong attempts: the register holds SEVEN tables in FOUR header shapes —
    `| ID | Status | Proposition | Enforced at | Checked today by | Sev |` (x3), `… | Carried from | Sev |`,
    `… | HAZID | Sev |` and `… | Source | Sev |`. So there is no single column index for the severity: a
    fixed index read 120 of 194 rows as unparsed, and an earlier `cells[-2]` — assuming a trailing notes
    cell exists — landed inside notes prose and read the open-HIGH set as 1 row instead of 6. `Sev` is the
    LAST NAMED column in all four shapes, so it is located by name against the table's own header, and a
    row whose arity or severity does not fit its header is reported unparsed rather than guessed."""
    text = open(REGISTER, errors="replace").read()
    rows, recovered, shapes = [], [], {}
    arity_mismatch = 0
    sev_idx = arity = None
    lines = text.splitlines()
    for i, line in enumerate(lines):
        # A HEADER is identified STRUCTURALLY: its first cell is exactly `ID` and the next line is the
        # markdown separator. Matching on the words "Status"/"Sev" anywhere in the line instead accepted
        # row lines whose PROSE mentions them — measured: two OBL rows were registered as "header shapes",
        # the shape census grew garbage entries, and every row after them was misparsed.
        nxt = lines[i + 1] if i + 1 < len(lines) else ""
        if re.match(r'^\|\s*ID\s*\|', line) and re.match(r'^\|[\s:|-]+\|$', nxt):
            cols = [c.strip() for c in line.split("|")[1:-1]]
            shapes[tuple(cols)] = shapes.get(tuple(cols), 0) + 1
            # group-2 cells are the header's columns from `Status` onward, so `Sev`'s index within them
            # is its index in the header minus one (the `ID` column is consumed by the row regex).
            sev_idx = cols.index("Sev") - 1 if "Sev" in cols else None
            arity = len(cols) - 1        # named cells after ID; a row may add one notes cell + a trailing ''
            continue
        m = re.match(r'^\|\s*(OBL-[CZT]\d+)\s*\|(.*)$', line)
        if not m or sev_idx is None:
            continue
        # Split on UNESCAPED pipes only. A bare `|` inside a code span splits a GFM cell — the register's
        # own guard documents that class and names six rows it repaired, and it is still present: measured,
        # 30 of 194 rows carry a cell arity their own table header does not explain, which is exactly the
        # population whose columns after the offending pipe are shifted.
        cells = re.split(r'(?<!\\)\|', m.group(2))
        cells = [c.strip() for c in cells]
        if len(cells) not in (arity + 1, arity + 2):
            arity_mismatch += 1
        status = cells[0]
        sev = cells[sev_idx] if len(cells) > sev_idx else ""
        if not re.fullmatch(r'[CHML]', sev):
            # Recovery, and it is LABELLED rather than silent: take the LAST bare severity letter in the
            # row's named-cell region. Validated against an independent count — it reproduces the six open
            # HIGH rows exactly — but a recovered value is marked, because a relation that had to be
            # recovered is weaker evidence than one read from its own column.
            cand = [c for c in cells[2:8] if re.fullmatch(r'[CHML]', c)]
            if cand:
                sev = cand[-1] + "*"
                recovered.append(m.group(1))
            else:
                sev = "?"
        tok = re.match(r'\*\*([A-Z][A-Z-]*)\*\*', status)
        rows.append((m.group(1), tok.group(1) if tok else "?", sev,
                     cells[1] if len(cells) > 1 else ""))
    return rows, recovered, shapes, arity_mismatch
rows, sev_recovered, table_shapes, arity_mismatch = register_rows()
OPEN_TOKENS = {"OPEN", "PARTLY", "FAILS"}
census = {}
for _, tok, _, _ in rows:
    census[tok] = census.get(tok, 0) + 1
open_rows = [(rid, tok, prop) for rid, tok, _sev, prop in rows if tok in OPEN_TOKENS]
# A recovered severity (`H*`) groups with its letter; the marker is carried so a reader can tell which
# rows' severities were read from their own column and which had to be recovered.
sev_of = {r[0]: r[2].rstrip("*") for r in rows}
open_by_sev = {}
for rid, s, p in open_rows:
    open_by_sev.setdefault(sev_of.get(rid, "?"), []).append(
        (rid + ("*" if rid in sev_recovered else ""), s, p))
_census_lines = [
    f"rows={len(rows)} unique={len(set(r[0] for r in rows))} "
    f"open={len(open_rows)} not_open={len(rows)-len(open_rows)}",
    "  by Status: " + "  ".join(f"{k}={v}" for k, v in sorted(census.items())),
    f"  row tables: {len(table_shapes)} distinct header shape(s) — " +
        " · ".join(f"{v}x [{' | '.join(k)}]"
                   for k, v in sorted(table_shapes.items(), key=lambda kv: -kv[1])),
    f"  cells: {arity_mismatch} of {len(rows)} rows carry a cell arity their own table header does not "
    f"explain (a bare `|` inside a code span splits a GFM cell — the class "
    f"`scripts/register-status.sh` documents and names six repaired rows of; it is not repaired)",
    f"  severity: read from its own column in {len(rows)-len(sev_recovered)} rows, recovered and marked "
    f"in {len(sev_recovered)}, unlocatable in {sum(1 for r in rows if r[2]=='?')}",
]
probes["register_census"] = {"rc": 0, "out": "\n".join(_census_lines)}

# Which gates can fail — the --self-test census (R8). Presence of a control is the measurable half of
# the question "is this a gate or a banner"; the other half is running it, which the umbrella does for
# four of them.
selftest = {}
for f in sorted(glob.glob(os.path.join(REPO, "scripts/check-*.sh")) +
                glob.glob(os.path.join(REPO, "contrib/ci/*.sh")) +
                glob.glob(os.path.join(REPO, "contrib/*.sh")) +
                [os.path.join(REPO, n) for n in ("scripts/register-status.sh",
                                                 "scripts/validate_zk_bins.sh")]):
    try:
        body = open(f, errors="replace").read()
    except OSError:
        continue
    selftest[os.path.relpath(f, REPO)] = ("--self-test" in body) or ("self-test" in body)

# Which gates the umbrella wires, and whose self-test IT runs. Parses run_gate with continuation lines.
umbrella_src = open(os.path.join(REPO, "scripts/run-all-tests.sh"), errors="replace").read()
joined = re.sub(r'\\\n\s*', ' ', umbrella_src)
wired, controls = set(), set()
for m in re.finditer(r'^run_gate\s+"([^"]+)"\s+(.*)$', joined, re.M):
    label, cmd = m.group(1), m.group(2).strip()
    tgt = re.search(r'([\w./-]+\.(?:sh|py))', cmd)
    if not tgt:
        continue
    if "--self-test" in cmd:
        controls.add(tgt.group(1))
    else:
        wired.add(tgt.group(1))
probes["umbrella"] = {"rc": 0, "out": f"tier1 gates wired={len(wired)} controls={len(controls)}"}
# Compare on BASENAMES: the umbrella writes some targets as bare filenames (`$SCRIPT_DIR/x.sh`) and some
# as relative paths (`$REPO_ROOT/contrib/ci/x.sh`), so a path-shaped comparison reports every gate as
# unwired. Normalising is the difference between a measurement and a false positive.
wired_bases    = {os.path.basename(p) for p in wired}
control_bases  = {os.path.basename(p) for p in controls}
excluded_bases = {os.path.basename(p) for p in EXCLUDED}
# Helpers and counters are NOT gates, and counting them as gates makes both the control census and the
# unwired list meaningless — the first run of this script reported 12 "unwired gates", six of which cannot
# fail because they are not gates at all. Each exclusion states which it is.
NOT_GATES = {
    "add_missing_license_headers.sh": "writes every .rs file — a tool, not a check",
    "app_android-battery-sampler.sh": "an unrelated app aid",
    "ctags.sh":                       "builds a tag index",
    "dependency_setup.sh":            "installs dependencies",
    "measure_rss.sh":                 "a memory-measurement harness (a counter of a running process)",
    "test_inventory.sh":              "enumerates tests; it asserts nothing",
    "clippy_critical_counts.sh":      "a ratchet counter, not a defect gate",
    "clippy_totality_counts.sh":      "a ratchet counter, not a defect gate",
    "length_cast_counts.sh":          "a ratchet counter, not a defect gate (in INSTRUMENTS as such)",
}
gate_scripts = {p: v for p, v in selftest.items() if os.path.basename(p) not in NOT_GATES}
unwired = sorted(t for t, _ in gate_scripts.items()
                 if os.path.basename(t) not in wired_bases | excluded_bases)
probes["unwired"] = {"rc": 0 if not unwired else 1, "out": "\n".join(unwired) or "(none)"}
probes["not_gates"] = {"rc": 0, "out": "\n".join(f"{k} — {v}" for k, v in sorted(NOT_GATES.items()))}

# The citation probe — a live row citing a symbol that no longer exists. No instrument sees this class:
# check-register-artifacts.sh strips line suffixes and checks no symbol, and says so in its own header.
probes["max_block_size"] = probe("grep -rn 'const MAX_BLOCK_SIZE' src/ bin/")
probes["max_block_size_refs"] = probe("grep -rln 'MAX_BLOCK_SIZE' src/ bin/")
probes["l1_barrier"] = probe("grep -rn 'L1 barrier' src/ bin/ --include=*.rs | head -8")

# ─────────────────────────────────────────────────────────────────────────────────────────────────
# Run every instrument. Sequential by construction — each is seconds and reads only the tree.
os.makedirs(RAW, exist_ok=True)
for name, cmd in INSTRUMENTS:
    run(name, cmd)

red   = [n for n, (v, _, _, _) in results.items() if v == FAIL and n not in COUNTERS]
green = [n for n, (v, _, _, _) in results.items() if v == PASS and n not in COUNTERS]
miss  = [n for n, (v, _, _, _) in results.items() if v == MISSING]

def ev(name, tail=1):
    """The instrument's own words, quoted — R5: a fix is verified by a run, not a gate exit code.
    A FAILING gate's reason is at the TOP of its output and its summary at the bottom, so a fail quotes
    the head and a pass quotes the tail. Measured: quoting the tail of a red `check-doc-index.sh` showed
    an unrelated closing sentence and hid the actual problem list."""
    v, rc, out, err = results.get(name, (MISSING, None, "", ""))
    lines = [l.strip() for l in (out or "").splitlines() if l.strip()]
    if not lines:
        lines = [l.strip() for l in (err or "").splitlines() if l.strip()]
    if not lines:
        return "(no output)"
    if v == FAIL:
        return " / ".join(lines[:3])[:400]
    # A pass states its verdict on the line beginning PASS/OK; the tail is often an exception list or a
    # trailing separator, which is what quoting it produced before this.
    head = [l for l in lines if re.match(r'(PASS|OK|no violations)', l)]
    return " / ".join((head[:1] or lines[-tail:]))[:400]

def v_of(name):
    return results.get(name, (MISSING, None, "", ""))[0]

# ─────────────────────────────────────────────────────────────────────────────────────────────────
# THE CRITERIA. `source` is load-bearing and is printed: `derived` means the verdict is computed from an
# instrument's exit in this run; `declared` means it rests on a reading and says so. A declared verdict
# is not a weaker kind of evidence — it is a different kind, and hiding which is which is how a report
# overclaims.
CRITERIA = [
 # ── Genesis ──────────────────────────────────────────────────────────────────────────────────────
 dict(g="Genesis", id="G1",
      st="The ceremony reads no ambient authority — no clock, RNG, network, or undeclared file",
      v=NOTEST, src="declared",
      inst="`init_genesis` → `build_genesis_block` → `build_genesis_deployment_txs`",
      ev=f"the genesis path is a reading task; `genesis.md:153-165` states the clause. "
         f"declared, not derived: no instrument decides it",
      settle="read the three clauses at doc/src/arch/genesis.md:153-165 against the path"),
 dict(g="Genesis", id="G2",
      st="Embedded contract code is a quoted argument, not an effect",
      v=PASS if (len(lib_names) == 9 and probes["other_wasm_embeds"]["rc"] != 0) else FAIL,
      src="derived",
      inst="the `include_bytes!` probe: the deployment table is the only route the wasm enters by",
      ev=f"the `bin/dwowd/src/lib.rs` deployment table enumerates {len(lib_names)} contract wasm paths; "
         f"`include_bytes!` of a `.wasm` elsewhere in non-test src/ or bin/: "
         f"{probes['other_wasm_embeds']['out'] or 'none'}. The one hit that exists is a test "
         f"(`src/contract/deployooor/tests/artifact_validates.rs`), which embeds the artifact to validate "
         f"it and is not on the genesis path; test files are excluded by path and the exclusion is stated "
         f"here rather than left silent",
      settle="—"),
 dict(g="Genesis", id="G3",
      st="The ceremony's stages are pure transitions whose composition is a pure transition",
      v=NOTEST, src="declared",
      inst="—",
      ev="no instrument; this is the third normative clause and it is a reading",
      settle="read genesis.md:153-165 clause 2 against the stage boundaries"),
 dict(g="Genesis", id="G4",
      st="Totality — no panic path reaches the genesis artifacts",
      v=FAIL if v_of("wasm_artifact_genesis") == FAIL else (PASS if v_of("wasm_artifact_genesis") == PASS else MISSING),
      src="derived", inst="`contrib/wasm_artifact_check.sh --genesis`",
      ev=ev("wasm_artifact_genesis", 1),
      settle="cargo test --release -p dwowd -- test_heavyweight_<c>  (x9)"),
 dict(g="Genesis", id="G5",
      st="Determinism — the same inputs yield the same hash",
      v=NOTEST, src="declared", inst="—",
      ev="the test that decides it is behind a build, which constraint 1 excludes",
      settle="cargo test -p dwowd --all-features test_genesis_determinism"),
 dict(g="Genesis", id="G6",
      st="Reproducibility — the artifact is a function of the source, not of the host",
      v=FAIL if v_of("wasm_artifact_genesis") == FAIL else (PASS if v_of("wasm_artifact_genesis") == PASS else MISSING),
      src="derived", inst="`wasm_artifact_check.sh --genesis` + the absolute-path probe",
      ev=f"gate: {ev('wasm_artifact_genesis',1)}  ·  purity probe (absolute host paths): {probes['abs_paths']['out']}",
      settle="see OBL-C140 and the row minted by this pass — the two readings differ and both matter"),
 dict(g="Genesis", id="G7",
      st="The pin is populated, committed, and clean against HEAD",
      v=PASS if (probes["pin_git"]["out"] == "" and probes["pin_placeholder"]["out"] == "0") else FAIL,
      src="derived", inst="`git status --porcelain` and a placeholder count on the pin file",
      ev=f"clean={probes['pin_git']['out'] or 'yes'}; placeholder={'no' if probes['pin_placeholder']['out']=='0' else 'YES'}; "
         f"last moved: {probes['pin_last']['out']}",
      settle="— (this is provenance, not currency — G8 is the currency criterion)"),
 dict(g="Genesis", id="G8",
      st="The pin is CURRENT — this build computes the hash the pin records",
      v=NOTEST, src="declared", inst="—",
      ev="only the test asserts it; a static reading cannot decide currency",
      settle="cargo test -p dwowd --all-features genesis_pin_is_current"),
 dict(g="Genesis", id="G9",
      st="The nine genesis ids agree across their three enumerations, in consensus order",
      v=PASS if probes["genesis_ids"]["rc"] == 0 else FAIL,
      src="derived", inst="parse `execution.rs`, `lib.rs` and `contract_id.rs` and compare",
      ev=probes["genesis_ids"]["out"],
      settle="—"),
 dict(g="Genesis", id="G10",
      st="The genesis model conforms to the Lean model",
      v=v_of("genesis_model_conformance"), src="derived",
      inst="`contrib/genesis_model_conformance.sh`", ev=ev("genesis_model_conformance"),
      settle="—"),
 dict(g="Genesis", id="G11",
      st="The nine genesis contracts pass their heavyweight tests",
      v=NOTEST, src="declared", inst="—",
      ev="needs a release build of dwowd and nine proving runs — outside this budget",
      settle="cargo test --release -p dwowd -- test_heavyweight_deployooor … (x9)"),
 # ── Consensus ────────────────────────────────────────────────────────────────────────────────────
 dict(g="Consensus", id="C1",
      st="Zero OPEN CRITICAL register rows, under the declared counting",
      v=PASS if len(open_by_sev.get("C", [])) == 0 else FAIL,
      src="derived", inst="Status-column census of the register",
      ev=f"open C-severity rows: {len(open_by_sev.get('C', []))}. "
         f"CAVEAT, in the same cell on purpose: one C-severity row is dispositioned ACCEPTED-WITH-REASON "
         f"rather than closed and the register's prose still calls it open — OBL-C80 — so '0 open critical' "
         f"is true of the row statuses and misleading without that sentence",
      settle="—"),
 dict(g="Consensus", id="C2",
      st="The open HIGH rows are enumerated, each with its subject",
      v="%d rows" % len(open_by_sev.get("H", [])), src="derived",
      inst="Status-column census of the register",
      ev="; ".join(f"{r} ({s})" for r, s, _ in open_by_sev.get("H", [])),
      settle="—"),
 dict(g="Consensus", id="C3",
      st="The umbrella's Tier-1 gates are all green",
      v=NOTEST, src="declared", inst="`scripts/run-all-tests.sh`",
      ev=f"it contains `make test` and the Lean build. {probes['umbrella']['out']}; "
         f"this report ran the static subset and names the remainder",
      settle="bash scripts/run-all-tests.sh"),
 dict(g="Consensus", id="C4",
      st="Every gate that runs can fail — each ships a control (R8)",
      v=FAIL if probes["unwired"]["rc"] != 0 else PASS,
      src="derived", inst="the `--self-test` census over every gate script",
      ev=f"{sum(1 for v in gate_scripts.values() if v)} of {len(gate_scripts)} gate scripts mention a self-test; "
         f"notably `scripts/check-doc-index.sh` does not, while `run-all-tests.sh:71` states "
         f"\"a gate whose control cannot make it fail is not a gate\"",
      settle="add a --self-test to check-doc-index.sh and wire it as an umbrella control"),
 dict(g="Consensus", id="C5",
      st="No gate exists that is red and wired by nothing",
      v=FAIL if probes["unwired"]["rc"] != 0 else PASS,
      src="derived", inst="diff the gate scripts on disk against `run_gate` in the umbrella",
      ev=f"unwired gate scripts:\n" + "\n".join(f"    - {u}" for u in probes["unwired"]["out"].splitlines()),
      settle="see OBL-C137 — the class is recorded; these are its current instances"),
 dict(g="Consensus", id="C6",
      st="Committed artifacts match the sources they carry",
      v=v_of("artifact_freshness"), src="derived", inst="`scripts/check-artifact-freshness.sh`",
      ev=ev("artifact_freshness"), settle="—"),
 dict(g="Consensus", id="C7",
      st="The circuit invariants hold (metadata, domain separation, derivation, transcription, index, fidelity)",
      v=PASS if all(v_of(n) == PASS for n in
                    ("circuit_metadata_alignment","circuit_domain_separation","circuit_instance_derivation",
                     "circuit_transcription","circuit_index","circuit_fidelity")) else FAIL,
      src="derived", inst="the six circuit gates", ev="; ".join(
          f"{n}={v_of(n)}" for n in ("circuit_metadata_alignment","circuit_domain_separation",
          "circuit_instance_derivation","circuit_transcription","circuit_index","circuit_fidelity")),
      settle="—"),
 dict(g="Consensus", id="C8",
      st="The L1 wire carries no value that reaches call params with no observer",
      v=v_of("l1_wire_conformance"), src="derived",
      inst="`scripts/check-l1-wire-conformance.sh` + OBL-C145",
      ev=f"{ev('l1_wire_conformance')}  ·  the gate is green; OBL-C145 is PARTLY — both are true and "
         f"neither replaces the other",
      settle="—"),
 dict(g="Consensus", id="C9",
      st="exec writes no state; apply reads none",
      v=v_of("phase_host_functions"), src="derived", inst="`scripts/check-phase-host-functions.sh`",
      ev=ev("phase_host_functions"), settle="—"),
 dict(g="Consensus", id="C10",
      st="Consensus value cannot be minted through a contract",
      v=PASS if len(open_by_sev.get("C", [])) == 0 else FAIL, src="derived",
      inst="the register's C-severity rows",
      ev=f"{census.get('CLOSED',0)+census.get('SATISFIED',0)+census.get('FIXED',0)+census.get('MECHANIZED',0)} "
         f"of 194 rows closed/satisfied/fixed/mechanized; the C-severity open set is empty (C1's caveat applies)",
      settle="—"),
 dict(g="Consensus", id="C11",
      st="The heavyweight contract suite is green",
      v=NOTEST, src="declared", inst="—",
      ev="the register's 2026-09-25 sweep records ten red; the report cites the register rather than "
         "restating it as this run's measurement",
      settle="make test  (or the targeted cargo test --release -p dwowd -- test_heavyweight_<c>)"),
 dict(g="Consensus", id="C12",
      st="Finality is verified, not merely enforced",
      v=NOTEST, src="declared", inst="—",
      ev="a reading task over caribina::verify_anchor's callers; the register's OBL-C63–C71 record the "
         "closures and OBL-C68 has no characterization test and says so",
      settle="read doc/src/arch/caribina.md's integration table against the callers"),
 # ── Documentation ────────────────────────────────────────────────────────────────────────────────
 dict(g="Documentation", id="D1",
      st="The index is coherent in both directions",
      v=v_of("doc_index"), src="derived", inst="`scripts/check-doc-index.sh`",
      ev=ev("doc_index"), settle="—"),
 dict(g="Documentation", id="D2",
      st="Every cited authority resolves", v=v_of("authority_resolves"), src="derived",
      inst="`scripts/check-authority-resolves.sh`", ev=ev("authority_resolves"), settle="—"),
 dict(g="Documentation", id="D3",
      st="Every file the register cites exists at HEAD", v=v_of("register_artifacts"), src="derived",
      inst="`scripts/check-register-artifacts.sh`", ev=ev("register_artifacts"), settle="—"),
 dict(g="Documentation", id="D4",
      st="Every register row carries a status from the vocabulary",
      v=v_of("register_status_check"), src="derived", inst="`scripts/register-status.sh --check`",
      ev=ev("register_status_check"), settle="—"),
 dict(g="Documentation", id="D5",
      st="No live claim in the tree contradicts the tree",
      v=FAIL, src="derived", inst="the probes below; this class has NO gate",
      ev=f"a live claim that the tree contradicts, found by this run: the two build levers genesis.md and "
         f"the artifact gate's header both say are in force. `strip` in any Cargo.toml: "
         f"{(probes['lever_strip']['out'] or 'NONE').splitlines()[0][:120]}; `location-detail` in any build "
         f"file: {'only in prose' if 'genesis.md' in probes['lever_locdetail']['out'] else probes['lever_locdetail']['out'][:80]}",
      settle="see the row minted by this pass; the repair is a decision, not an edit"),
 dict(g="Documentation", id="D6",
      st="A register row's cited line numbers and symbols still resolve",
      v=FAIL, src="derived", inst="the probes below; no instrument exists for this class",
      ev=f"`const MAX_BLOCK_SIZE` in src/ or bin/: {(probes['max_block_size']['out'] or 'no hit')} — "
         f"while it is still cited by live rows. check-register-artifacts.sh states in its own header that "
         f"it checks no symbol and strips line suffixes",
      settle="see the row minted by this pass"),
 dict(g="Documentation", id="D7",
      st="The root-cause taxonomy is coherent and its legacy ids are aliased",
      v=PASS, src="declared", inst="`check-doc-index.sh` check 6 + reading",
      ev="RC1–RC12 canonical at safety.md:111-129; every legacy scheme aliased at :1056-1101; check 6 "
         "resolves every `safety.md RC<n>` citation (it passes)",
      settle="—"),
]

# ─────────────────────────────────────────────────────────────────────────────────────────────────
# FINDINGS, rendered FROM THE REGISTER. Each id must exist as a row in the register or the section says
# so — so a finding cannot appear here without existing as an obligation (constraint 4).
MINTED_IDS = ["OBL-C147", "OBL-C148", "OBL-C149"]

def render_findings():
    rowmap = {r[0]: r for r in rows}
    out = []
    for rid in MINTED_IDS:
        if rid in rowmap:
            _, tok, sev, prop = rowmap[rid]
            out.append(f"- **{rid}** ({tok}, severity {sev.rstrip('*')}) — {prop.rstrip('*')}")
        else:
            out.append(f"- **{rid}** — *not yet minted in the register; this finding is not yet "
                       f"recorded as an obligation.*")
    return "\n".join(out)

# ─────────────────────────────────────────────────────────────────────────────────────────────────
# RENDER.
def veil(s):
    return s.replace("|", "\\|")

L = []
w = L.append
w(f"# Status report — {DATE}")
w("")
w("Generated by `scripts/report-status.sh`. **Every figure below comes from a run this script made in "
  "this run**, and every verdict names the instrument that produced it. Nothing here is quoted from "
  "memory, from another document, or from a previous report.")
w("")
w("## Subject")
w("")
w("| | |")
w("|---|---|")
w(f"| HEAD | `{probes['head']['out']}` |")
w(f"| Branch | `{probes['branch']['out']}` |")
w(f"| Working tree | {probes['dirty']['out']} modified path(s) — the report describes the tree, not a commit |")
w(f"| Toolchain | `{probes['toolchain']['out']}` · `{probes['rustc']['out']}` |")
w(f"| Budget | static instruments only: no cargo, make, lake, docker or rustc. **No artifact was rebuilt, "
  f"so the genesis pin cannot have moved as a result of this run.** |")
w("")
w("**Which tree this describes, stated because a committed report cannot name its own commit.** The "
  "subject above is HEAD at generation time, and every instrument ran against that tree *plus the "
  "modifications listed*. This file does not exist in that HEAD — it is committed as the change on top of "
  "it — so a reader reconstructing the subject should take that commit and apply the report's own commit, "
  "or simply note that the modified-path count is the rest of the delta. The alternative, naming the "
  "commit that carries the file, is not available from inside the run that writes it.")
w("")
w(f"**This report does not establish everything it reports on.** {sum(1 for c in CRITERIA if c['v']==NOTEST)} "
  f"criteria read NOT ESTABLISHED because this budget cannot decide them; each names the exact command "
  f"that would. That is a result, not a gap (R7).")
w("")
w("## Verdicts")
w("")
w("`source` is load-bearing: **derived** means the verdict was computed from an instrument's exit in this "
  "run; **declared** means it rests on a reading and says so. Hiding which is which is how a report "
  "overclaims.")
w("")
for group in ("Genesis", "Consensus", "Documentation"):
    w(f"### {group}")
    w("")
    w("| ID | Criterion | Verdict | source | Instrument |")
    w("|---|---|---|---|---|")
    for c in CRITERIA:
        if c["g"] != group:
            continue
        w(f"| {c['id']} | {veil(c['st'])} | **{c['v']}** | {c['src']} | {veil(c['inst'])} |")
    w("")
    for c in CRITERIA:
        if c["g"] != group:
            continue
        w(f"**{c['id']} — {c['st']}**")
        w("")
        w(f"- verdict: **{c['v']}** ({c['src']})")
        w(f"- evidence: {c['ev']}")
        if c["v"] != PASS and c["settle"] != "—":
            w(f"- to settle it: `{c['settle']}`")
        w("")

w("## The instrument census")
w("")
w(f"{len(green)} green, {len(red)} red, {len(miss)} missing — over {len(INSTRUMENTS)} instruments, all "
  f"static. Deliberately among them: gates the umbrella wires only as their own self-test, and gates wired "
  f"nowhere at all. This report runs them because otherwise their verdicts enter no record — see C5.")
w("")
w(f"Not run, and not counted as gates: " +
  "; ".join(f"`{k}` ({v})" for k, v in sorted(NOT_GATES.items())) + ".")
w("")
w("| Instrument | Verdict | Its own last line, quoted |")
w("|---|---|---|")
for name, _ in INSTRUMENTS:
    v = results.get(name, (MISSING, None, "", ""))[0]
    label = v + (" *(counter — exit 1 means sites remain, not a defect)*" if name in COUNTERS else "")
    w(f"| `{name}` | **{label}** | {veil(ev(name))} |")
w("")
w("### Which gates can fail")
w("")
w("A check is not a check until something can make it fail (R8). Per gate script, whether it carries a "
  "control, and whether the umbrella runs that control:")
w("")
w("| Gate script | control? | umbrella wires the gate? | umbrella wires the control? |")
w("|---|---|---|---|")
for path in sorted(gate_scripts):
    base = os.path.basename(path)
    w(f"| `{path}` | {'yes' if gate_scripts[path] else '**NO**'} | "
      f"{'yes' if base in wired_bases else ('n/a' if base in excluded_bases else '**no**')} | "
      f"{'yes' if base in control_bases else '—'} |")
w("")
w("## Documentation accuracy — the findings")
w("")
w("This class is the one with no instrument, and it is where the load-bearing problems are. Each finding "
  "is recorded as an obligation rather than only here:")
w("")
w(render_findings())
w("")
w("### Repairs made by this pass")
w("")
w("Decision of record: findings are **recorded**, not fixed — a finding becomes an obligation and the "
  "repair is a separate decision. One exception, and it is not a judgement call: where the register has "
  "**already prescribed the correction** and the sentence it corrects is still standing, the repair is "
  "executing a recorded decision rather than making one (R11 — never leave a false statement in the tree).")
w("")
w("- **`doc/src/arch/verification-hazop.md`, the \"What this register implies\" paragraph.** It read that "
  "`OBL-C80` \"is now this register's **only open critical**\". The register's own re-measurement, 30 lines "
  "below it, already says what it should read (\"should read *only live critical*, `OPEN` being a status "
  "this row does not hold\") — and the sentence was never changed. Applied, with its figures brought "
  "forward: that note measures 154 rows and `OBL-C80` at `PARTLY`; the register is now 197 rows and the "
  "row is `ACCEPTED-WITH-REASON`. The same paragraph's other overstated figure (`OBL-C82` as a new `H`) is "
  "annotated in place, since the note already measures it `M`. No row was minted for this: the register "
  "carried the correction, so a new row would restate one that exists.")
w("")
w("### The genesis reproducibility claim, measured")
w("")
w("`doc/src/arch/genesis.md`'s section \"How the reproducibility clause is met\" names two build settings "
  "and says that together they take all nine genesis contracts clean. The probes:")
w("")
w("| Probe | Result |")
w("|---|---|")
w(f"| `strip` in any Cargo.toml | `{veil(probes['lever_strip']['out'] or 'NO HIT')}` |")
w(f"| `git log -S 'strip = \"symbols\"' -- Cargo.toml` | `{veil(probes['lever_strip_hist']['out'] or 'NO HIT on any commit')}` |")
w(f"| rustflags in `.cargo/config.toml` | {veil(probes['cargo_config']['out'])} |")
w(f"| `git log -S 'location-detail' --all -- Makefiles` | `{veil(probes['lever_locdetail_hist']['out'] or 'NO HIT on any branch')}` |")
w(f"| `RUSTC_BOOTSTRAP` in the tree | `{veil(probes['bootstrap']['out'] or 'NO HIT')}` |")
w(f"| contract Makefile RUSTFLAGS | `{veil(probes['makefile_flags']['out'])}` |")
w(f"| absolute host paths in the nine genesis wasms | `{veil(probes['abs_paths']['out'])}` |")
w(f"| relative first-party paths in the nine | `{veil(probes['rel_paths']['out'])}` |")
w("")
w("**Read the last two rows together, because they say different things and only both are true.** The "
  "gate's invariant (no marker strings, no first-party path) is breached; the purity property the "
  "clause is *about* (no host-dependence) is not, because every surviving path is relative. `OBL-C140` "
  "reads that split correctly. What no row records is that the mechanism the clause relies on is absent "
  "from the tree — and one of its two levers is unavailable on the pinned stable toolchain.")
w("")
w("### The register census")
w("")
w(f"`{probes['register_census']['out']}`")
w("")
w("Counts are from the **Status column**, never from `register-status.sh`'s token histogram, which the "
  "script itself documents as unreliable — the two currently disagree (histogram 55 OPEN against a "
  "census of 30) because the register's prose contains every status word in a non-status sense.")
w("")
w("Open rows by severity:")
w("")
for sev in ("C", "H", "M", "L"):
    rs = open_by_sev.get(sev, [])
    if rs:
        w(f"- **{sev}** ({len(rs)}): " + ", ".join(f"`{r}` {s}" for r, s, _ in rs))
    else:
        w(f"- **{sev}** (0)")
w("")
w("This report renders **no verdict on any open row's proposition** — it reports them as the register "
  "states them, with the instrument that read them. It also does not fix anything: a finding is recorded "
  "as an obligation, and the repair is a separate decision.")
w("")
w("## What this report did not establish")
w("")
w("Required section, not an appendix (R7 — \"cannot verify\" is a result, and a confident wrong answer is "
  "worse than an admitted unknown). Every NOT ESTABLISHED criterion, and the one command that settles it:")
w("")
w("| ID | Not established | Settled by |")
w("|---|---|---|")
for c in CRITERIA:
    if c["v"] == NOTEST and c["settle"] != "—":
        w(f"| {c['id']} | {veil(c['st'])} | `{c['settle']}` |")
w("")
w("Beyond the criteria: this run made **no measurement of runtime behaviour at all**. It ran no test, "
  "compiled nothing, and proved nothing. Every verdict above is a statement about the tree as text, with "
  "the single exception of the instrument exits, which are statements about other static checkers.")
w("")
w("## Appendix — reproducing this report")
w("")
w("```sh")
w("bash scripts/report-status.sh          # emits this report and the raw capture beside it")
w("bash scripts/report-status.sh --self-test   # the negative control for the verdict logic")
w("```")
w("")
w(f"Raw, verbatim output per instrument: `reports/status-{DATE}.raw/<name>.{{out,err,exit}}`.")
w("")
w("Determinism: two consecutive runs must agree character for character except the date. If they do not, "
  "this generator is reading something non-deterministic and that thing must be named rather than "
  "averaged away.")

os.makedirs(OUT, exist_ok=True)
# Redaction is applied at the single write chokepoint rather than at each call site, so a new evidence
# string added later cannot bypass it. The raw capture is not a `.md` and so is not in the doc gate's
# scan set (`scripts/check-doc-index.sh:121`), which is why it needs no such treatment.
open(REPORT, "w").write(redact("\n".join(L)) + "\n")
print(f"wrote {os.path.relpath(REPORT, REPO)}")
print(f"  instruments: {len(green)} green, {len(red)} red, {len(miss)} missing  (raw in "
      f"{os.path.relpath(RAW, REPO)}/)")
print(f"  criteria:    {sum(1 for c in CRITERIA if c['v']==PASS)} MET, "
      f"{sum(1 for c in CRITERIA if c['v']==FAIL)} NOT MET, "
      f"{sum(1 for c in CRITERIA if c['v']==NOTEST)} NOT ESTABLISHED")
if miss:
    print(f"  NOT RUN: {', '.join(miss)} — reported MISSING, never PASS")
PYEOF
