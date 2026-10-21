#!/bin/bash
# OBL-C77: a `get_metadata` arm that answers with a bare empty vector is the host's **rejection**
# signal, not "no public inputs".
#
# The rule is `contract-standards.md` §3, and the host enforces it: `execution.rs` decodes the first
# encoded vector out of the metadata and, on an empty buffer, fails the call at
# `metadata-decode-zkp` — before exec runs. So an arm whose *success* path returns `vec![]` (or
# `Ok(vec![])`) makes the instruction **impossible to call**, and an arm that *means* "reject this
# call" is indistinguishable from one that forgot to encode. Both are worth knowing about, and the
# difference is a judgement a reader makes per site: `plaintext_call_get_metadata`
# (`native_token/src/entrypoint/mod.rs:922`) is the reference for the first case — it returns the
# encoded empty vector on success and the empty buffer only for the two rejection cases (no params,
# undecodable params).
#
# This gate finds the arms. Every one must then be declared in
# `script/metadata_plaintext_exceptions.txt` with the reason it is empty — repaired arms come out of
# the list in the same commit, and a stale entry is reported so the list cannot outlive its sites.
#
# Scope: the arm's *value* is a literal empty vector. An arm returning a variable, or calling a
# helper, is out of scope by construction: the gate is a shape check, and a shape check that tried
# to follow values would be the cleverer classifier this repository has twice rejected as worse than
# a shallow one a reviewer reads.
#
# Exit 0: every empty arm is declared. Exit 1: at least one is not. Exit 2: the list is malformed.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SELF="$SCRIPT_DIR/$(basename "$0")"
REPO_ROOT="${METADATA_ARMS_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"

# ── Negative control ───────────────────────────────────────────────────────────────────────────────
# A gate that cannot fail is not a gate. Plant one `get_metadata` arm whose success value is a bare
# empty vector — the shape the host reads as a rejection — and require a non-zero verdict that names
# the file and the arm. Then answer with an encoded value and require the fixture to pass.
if [ "${1:-}" = "--self-test" ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    mkdir -p "$TMP/src/contract/demo/src"
    cat > "$TMP/src/contract/demo/src/entrypoint.rs" <<'FIXTURE'
impl Demo {
    fn get_metadata(&self) -> Vec<u8> {
        match x {
            Demo::Call => vec![],
            _ => encoded,
        }
    }
}
FIXTURE
    if OUT="$(METADATA_ARMS_ROOT="$TMP" "$SELF" 2>&1)"; then
        echo "SELF-TEST FAIL: the planted empty metadata arm was not reported" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    ok=1
    printf '%s' "$OUT" | grep -q "src/contract/demo/src/entrypoint.rs" || {
        echo "SELF-TEST FAIL: the offending file was not named" >&2; ok=0; }
    printf '%s' "$OUT" | grep -q "Demo::Call" || {
        echo "SELF-TEST FAIL: the offending arm was not named" >&2; ok=0; }
    [ "$ok" -eq 1 ] || { printf '%s\n' "$OUT" | sed 's/^/  | /' >&2; exit 1; }

    # Clean twin: the same arm answering with an encoded value must pass.
    cat > "$TMP/src/contract/demo/src/entrypoint.rs" <<'FIXTURE'
impl Demo {
    fn get_metadata(&self) -> Vec<u8> {
        match x {
            Demo::Call => encoded,
            _ => encoded,
        }
    }
}
FIXTURE
    if ! OUT="$(METADATA_ARMS_ROOT="$TMP" "$SELF" 2>&1)"; then
        echo "SELF-TEST FAIL: the clean fixture does not pass" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    echo "SELF-TEST PASS: an empty get_metadata arm is named; the encoded twin passes"
    exit 0
fi

REPO_ROOT="$REPO_ROOT" python3 - <<'PYEOF'
import glob, os, re, sys

repo = os.environ["REPO_ROOT"]

paths = sorted(glob.glob(f"{repo}/src/contract/*/src/entrypoint.rs")
               + glob.glob(f"{repo}/src/contract/*/src/entrypoint/*.rs"))

# `<Enum>::<Variant> => vec![],` / `=> Ok(vec![]),` / `_ => vec![],` — the arm's whole value.
ARM_RE = re.compile(r"^\s*(?P<arm>\w+::\w+|_)\s*=>\s*(?:Ok\()?vec!\[\](?:\))?,?\s*$")

findings = []
for path in paths:
    rel = os.path.relpath(path, repo)
    try:
        lines = open(path, errors="replace").read().split("\n")
    except OSError:
        continue
    if not any("fn get_metadata" in ln for ln in lines):
        continue
    in_meta = False
    for i, ln in enumerate(lines, 1):
        if re.match(r"\s*(pub\s+)?fn get_metadata\b", ln):
            in_meta = True
            continue
        if in_meta and re.match(r"\s*(pub\s+)?fn \w+", ln):
            break
        if not in_meta:
            continue
        m = ARM_RE.match(ln)
        if m:
            findings.append((f"{rel} : {m.group('arm')} => vec![]", f"{rel}:{i}: {ln.strip()}"))

declared = {}
exc_path = os.path.join(repo, "script", "metadata_plaintext_exceptions.txt")
if os.path.isfile(exc_path):
    for raw in open(exc_path, errors="replace").read().splitlines():
        entry = raw.split("#")[0].strip()
        if not entry:
            continue
        parts = [p.strip() for p in entry.split(" : ", 2)]
        if len(parts) != 3:
            print(f"ERROR: malformed line in {os.path.basename(exc_path)}: {entry!r}")
            print("       expected: <rel path> : <arm> : <reason citing a register ID>")
            sys.exit(2)
        declared.setdefault(f"{parts[0]} : {parts[1]}", parts[2])

undeclared = [(k, v) for k, v in findings if k not in declared]
stale = [k for k in declared if k not in {k for k, _ in findings}]

if stale:
    print("NOTE: declared but no longer an empty arm — the repair landed, so remove these lines:")
    for k in sorted(stale):
        print(f"  {k}")
    print()

for k, _ in findings:
    if k in declared:
        print(f"DECLARED: {k}")
        print(f"          {declared[k]}")

if undeclared:
    print("")
    print(f"FAIL: {len(undeclared)} get_metadata arm(s) answer with a bare empty vector, which the host")
    print("      reads as a REJECTION — the instruction cannot be called:")
    for _, v in undeclared:
        print(f"  {v}")
    print("")
    print("Answer with the encoded empty public-input vector instead (the reference is")
    print("native_token's plaintext_call_get_metadata), or declare the arm in")
    print("script/metadata_plaintext_exceptions.txt with the reason it must reject.")
    sys.exit(1)

print("")
print("PASS: every empty get_metadata arm is declared.")
sys.exit(0)
PYEOF
