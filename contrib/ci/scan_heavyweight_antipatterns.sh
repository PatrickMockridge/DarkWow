#!/bin/bash
# Anti-pattern scanner for heavyweight tests
# Checks for all 12 prohibited patterns from heavyweight-spec.md §4.
# Usage: ./scan_heavyweight_antipatterns.sh [--json]
# Exit 0 = clean, Exit 1 = violations found, Exit 2 = scanner error
#
# WIRING, added 2026-09-25. Wired into `scripts/run-all-tests.sh` as the gate
# "heavyweight anti-patterns (spec §4.11)". It was invoked by nothing before that,
# and it is the one red `contrib/ci/*` gate whose authority is real:
# `heavyweight-spec.md` §4.11 quotes the exact prohibited snippet — `empty_witnesses()`
# plus `Proof::create(pk, &[circuit], &[], OsRng)` — and says verbatim "CI SHALL fail if
# either pattern is found". No CI exists in this repository, so a spec clause demanding
# that CI fail has never been honoured by anything.
#
# **IT IS EXPECTED RED, and this record is a measurement (re-measured 2026-10-03 against the
# migrated spec tree).** Five findings: four in
# `src/contract/test-harness/src/harness/dex.rs` (two `empty-witnesses` + two
# `empty-proof-stub`, over two endpoints) and one `comment-deferred` in
# `bin/dwowd/src/tests/specs/dao_escrow_spec.rs:2`.
#   * The dex four are **not a defect**: dex's spec declares those two endpoints `is_zk: false`,
#     so the stubs match the spec.
#   * The `dao_escrow` one is a **genuine deferral**: `PayPremiumV1` is uncovered because the
#     funding path's proof has never been made to verify, which the contract's own header
#     records, so there is no honest row to write for it until that circuit is fixed.
# This paragraph used to claim "4 sites in `insurance_market.rs` and 4 in `dex.rs`", eight in all.
# Measured today, `insurance_market` contributes **none**: its four `empty_witnesses` sites feed
# `ProvingKey` construction, not `Proof::create`, so pattern 9 does not fire — the "8" was stale.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# Overridable so the negative control (`--self-test`) can point the scanner at a temp tree.
REPO_ROOT="${ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

HEAVYWEIGHT_FILE="$REPO_ROOT/bin/dwowd/src/tests/heavyweight_pipeline.rs"
BLOCKCHAIN_FILE="$REPO_ROOT/bin/dwowd/src/tests/blockchain.rs"
HARNESS_DIR="$REPO_ROOT/src/contract/test-harness/src/harness"
# The migrated heavyweight test tree. The tests these anti-patterns police moved out of
# `heavyweight_pipeline.rs` into `uniform_runner` + `specs/*`; a scanner pointed only at the
# pre-migration file reads code that no longer holds them (OBL-C137: wired and blind).
SPECS_DIR="$REPO_ROOT/bin/dwowd/src/tests/specs"

VIOLATIONS=0
JSON_MODE=false
if [[ "${1:-}" == "--json" ]]; then
    JSON_MODE=true
fi

# ── Negative control ────────────────────────────────────────────────────────────
# `--self-test` plants a prohibited pattern in a temp tree and requires the scanner to
# fail ON THAT FINDING. Exiting 0 from the control means the scanner cannot fail — the
# "wired and blind" failure OBL-C137 is about. The output is checked, not only the exit
# status, so a scanner that dies for an unrelated reason cannot satisfy its own control.
if [[ "${1:-}" == "--self-test" ]]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    mkdir -p "$TMP/bin/dwowd/src/tests/specs"
    # The exact shape §4.3 forbids, planted in the *spec tree* — the target this control
    # exists to prove is scanned, so a repoint that stopped reading it would fail here.
    printf '// BurnV1 accept_block routing is deferred until the harness adds call_data encoding.\n' \
        > "$TMP/bin/dwowd/src/tests/specs/planted.rs"
    OUT="$(ROOT="$TMP" "$0" 2>&1)" && {
        echo "SELF-TEST FAIL: §4.3 comment-deferred was not detected in the spec tree" >&2
        echo "  (a scanner whose control cannot make it fail is not a scanner)" >&2
        exit 1
    }
    if ! printf '%s' "$OUT" | grep -q 'comment-deferred'; then
        echo "SELF-TEST FAIL: the scanner failed, but not on the planted pattern — its control proves nothing" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    echo "SELF-TEST PASS: a planted §4.3 deferral in the spec tree was detected"
    exit 0
fi

violation() {
    local pattern="$1"
    local file="$2"
    local line="$3"
    local detail="$4"
    VIOLATIONS=$((VIOLATIONS + 1))
    if $JSON_MODE; then
        echo "{\"pattern\":\"$pattern\",\"file\":\"$file\",\"line\":\"$line\",\"detail\":\"$detail\"}"
    else
        echo "[VIOLATION] $pattern: $file:$line — $detail"
    fi
}

# The files the string-regex patterns scan: the pre-migration pipeline file and the
# migrated spec tree. `grep -rn` prints `file:line:match` for a file or a directory alike,
# so one loop covers either without a per-pattern special case.
STRING_TARGETS=("$HEAVYWEIGHT_FILE" "$SPECS_DIR")

# Emit a violation for every match of $1 (a BRE, as the patterns below are written) in
# every string target. $4, when given, is a second ERE the matched line must also contain.
scan_regex() {
    local pattern="$1" vname="$2" detail="$3" require="${4:-}" opts="${5:-}"
    local target line file lineno _ hits
    for target in "${STRING_TARGETS[@]}"; do
        # shellcheck disable=SC2086  # $opts is a deliberate flag list (e.g. "-i")
        hits="$(grep -rn $opts -- "$pattern" "$target" 2>/dev/null || true)"
        if [[ -n "$require" ]]; then
            hits="$(printf '%s\n' "$hits" | grep -E -- "$require" || true)"
        fi
        while IFS=: read -r file lineno _; do
            [[ -z "$file" ]] && continue
            violation "$vname" "$file" "$lineno" "$detail"
        done <<< "$hits"
    done
}

# ── Pattern 1: match-Err-skip (§4.1) ───────────────────────────────────────
# match harness.X() { Ok(d) => { ... } Err(e) => println!("...skipped...") }
scan_regex 'println!(".*skipped' "match-Err-skip" \
    "match with Err arm that prints 'skipped' instead of failing"

# ── Pattern 2: ZK-proof-only (§4.2) ────────────────────────────────────────
# let _pv = harness.X(...)?; — result discarded
scan_regex 'let _[a-z].*=.*harness\.' "ZK-proof-only" \
    "harness result discarded with let _ — never submitted to accept_block"

# Also check for the bare discard shape — the original post-filter (`_ [a-z]`) is the
# `require` argument.
scan_regex '= harness\.' "ZK-proof-only" \
    "harness result discarded — never submitted to accept_block" '_ [a-z]'

# ── Pattern 3: Comment-deferred (§4.3) ─────────────────────────────────────
scan_regex 'deferred until\|deferred —\|deferred—\|is deferred' "comment-deferred" \
    "accept_block routing deferred by comment" '' '-i'

# ── Pattern 4: Explicit skip (§4.4) ────────────────────────────────────────
scan_regex '(skipped\|(skip\|skipped —\|skipped—' "explicit-skip" \
    "test explicitly skips endpoint with comment" '' '-i'

# ── Pattern 5: strict_zk toggling (§4.5) ───────────────────────────────────
scan_regex 'strict_zk = false' "strict_zk-toggling" \
    "strict_zk = false bypasses ZK proof enforcement"
# The shared blockchain module is a third target, outside the spec tree.
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    violation "strict_zk-toggling" "$BLOCKCHAIN_FILE" "$lineno" "strict_zk = false bypasses ZK proof enforcement"
done < <(grep -n 'strict_zk = false' "$BLOCKCHAIN_FILE" 2>/dev/null || true)

# ── Pattern 6: Single-block batching (§4.6) ────────────────────────────────
# Flag tests that chain 5+ with_call in one block (heuristic)
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    count=$(echo "$line" | grep -o 'with_call' | wc -l)
    if [ "$count" -ge 5 ]; then
        violation "single-block-batching" "$HEAVYWEIGHT_FILE" "$lineno" "$count with_call() calls in one block — per-endpoint blocks required"
    fi
done < <(grep -n 'with_call' "$HEAVYWEIGHT_FILE" 2>/dev/null | awk -F: '{print $1}' | sort -n | \
    awk 'NR==1{start=$1; prev=$1; count=1; next} {if($1-prev<=5){count++; prev=$1} else {if(count>=5) print start":"count; start=$1; prev=$1; count=1}} END{if(count>=5) print start":"count}' || true)

# ── Pattern 7: println!("skipped") (§4.7) ──────────────────────────────────
scan_regex 'println!(".*[Ss]kipped' "println-skipped" \
    "println with 'skipped' — endpoint not verified"

# ── Pattern 8: Early-return on Error (§4.8) ────────────────────────────────
scan_regex 'Err.*=>.*return Ok(())' "early-return-on-err" \
    "Err branch returns Ok(()) — silently skips subsequent tests"

# ── Pattern 9: empty_witnesses in harness METHODS (§4.11) ───────────────────
# empty_witnesses is legitimate ONLY when it feeds ProvingKey/VerifyingKey
# building (spawn()/new()/verifying_key()). empty_witnesses feeding Proof::create
# is a stub — the proof constrains nothing about contract logic.
while IFS=: read -r file lineno; do
    [[ -z "$file" ]] && continue
    # Enclosing function body runs from this line to the next `fn <name>(` (or EOF).
    fn_end=$(awk -v target="$lineno" '
        NR > target && /fn [A-Za-z_][A-Za-z0-9_]*\(/ { print NR; exit }
    ' "$file")
    [[ -z "$fn_end" ]] && fn_end=$(awk 'END { print NR }' "$file")
    # A stub is empty_witnesses used in a function that creates a proof.
    if awk -v a="$lineno" -v b="$fn_end" '
        NR >= a && NR <= b && /Proof::create/ { found = 1 }
        END { if (found) exit 0; else exit 1 }
    ' "$file"; then
        violation "empty-witnesses" "$file" "$lineno" "empty_witnesses() in harness method — proves nothing about contract logic"
    fi
done < <(grep -rn 'empty_witnesses' "$HARNESS_DIR" 2>/dev/null | cut -d: -f1,2 || true)

# ── Also check: stubs that create proofs with empty public inputs ───────────
# Exclude spawn() constructors — empty_witnesses for PK building is correct there
# Exclude verify_zk_coverage — it validates key building with empty witnesses
while IFS=: read -r file lineno rest; do
    [[ -z "$file" ]] && continue
    # Skip spawn() — ProvingKey construction uses empty_witnesses legitimately
    if [[ "$rest" =~ spawn|verify_zk_coverage|empty_witnesses.*unwrap ]]; then
        continue
    fi
    violation "empty-proof-stub" "$file" "$lineno" "Proof::create with empty public inputs (&[])"
done < <(grep -rn 'Proof::create.*&\[\]' "$HARNESS_DIR" 2>/dev/null || true)

# ── Pattern 10: #[allow(dead_code)] suppression (RG-26) ──────────────────────
# Prohibits silencing the compiler's dead_code diagnostic.
# Equivalent severity to println!("skipped") per spec §4.7.
scan_regex '#\[allow(dead_code)\]' "allow-dead-code" \
    "#[allow(dead_code)] suppresses compiler diagnostic — equivalent to println!(\"skipped\") per spec §4.7"
# The harness crates are a third target, outside the spec tree. The file is the one
# `grep -rn` names, not `$dir/` prefixed onto an already-absolute path (the old form
# doubled the path in the reported location).
while IFS=: read -r file lineno _; do
    [[ -z "$file" ]] && continue
    violation "allow-dead-code" "$file" "$lineno" \
        "#[allow(dead_code)] suppresses compiler diagnostic — equivalent to println!(\"skipped\") per spec §4.7"
done < <(grep -rn '#\[allow(dead_code)\]' "$HARNESS_DIR" 2>/dev/null || true)

# ── Pattern 11: _old_* preserved test bodies (RG-27) ─────────────────────────
# Git history IS provenance. Old test bodies must be deleted, not preserved.
scan_regex '_old_.*_test\|__unused_after_' "preserved-old-test" \
    "_old_*_test function preserved as dead code — delete it; git history IS provenance"

# ── Summary ────────────────────────────────────────────────────────────────
if [ "$VIOLATIONS" -eq 0 ]; then
    if $JSON_MODE; then
        echo '{"status":"clean","violations":0}'
    else
        echo "[PASS] Zero anti-pattern violations found."
    fi
    exit 0
else
    if $JSON_MODE; then
        echo "{\"status\":\"violations\",\"count\":$VIOLATIONS}"
    else
        echo ""
        echo "[FAIL] $VIOLATIONS anti-pattern violation(s) found."
    fi
    exit 1
fi
