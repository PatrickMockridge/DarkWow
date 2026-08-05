#!/bin/bash
# Anti-pattern scanner for heavyweight tests
# Checks for all 9 prohibited patterns from heavyweight-spec.md §4.
# Usage: ./scan_heavyweight_antipatterns.sh [--json]
# Exit 0 = clean, Exit 1 = violations found, Exit 2 = scanner error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

HEAVYWEIGHT_FILE="$REPO_ROOT/bin/dwowd/src/tests/heavyweight_pipeline.rs"
BLOCKCHAIN_FILE="$REPO_ROOT/bin/dwowd/src/tests/blockchain.rs"
HARNESS_DIR="$REPO_ROOT/src/contract/test-harness/src/harness"

VIOLATIONS=0
JSON_MODE=false
if [[ "${1:-}" == "--json" ]]; then
    JSON_MODE=true
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

# ── Pattern 1: match-Err-skip (§4.1) ───────────────────────────────────────
# match harness.X() { Ok(d) => { ... } Err(e) => println!("...skipped...") }
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    violation "match-Err-skip" "$HEAVYWEIGHT_FILE" "$lineno" "match with Err arm that prints 'skipped' instead of failing"
done < <(grep -n 'println!(".*skipped' "$HEAVYWEIGHT_FILE" 2>/dev/null || true)

# ── Pattern 2: ZK-proof-only (§4.2) ────────────────────────────────────────
# let _pv = harness.X(...)?; — result discarded
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    violation "ZK-proof-only" "$HEAVYWEIGHT_FILE" "$lineno" "harness result discarded with let _ — never submitted to accept_block"
done < <(grep -n 'let _[a-z].*=.*harness\.' "$HEAVYWEIGHT_FILE" 2>/dev/null || true)

# Also check for _ = harness pattern
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    violation "ZK-proof-only" "$HEAVYWEIGHT_FILE" "$lineno" "harness result discarded — never submitted to accept_block"
done < <(grep -n '= harness\.' "$HEAVYWEIGHT_FILE" 2>/dev/null | grep '_ [a-z]' || true)

# ── Pattern 3: Comment-deferred (§4.3) ─────────────────────────────────────
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    violation "comment-deferred" "$HEAVYWEIGHT_FILE" "$lineno" "accept_block routing deferred by comment"
done < <(grep -n -i 'deferred until\|deferred —\|deferred—\|is deferred' "$HEAVYWEIGHT_FILE" 2>/dev/null || true)

# ── Pattern 4: Explicit skip (§4.4) ────────────────────────────────────────
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    violation "explicit-skip" "$HEAVYWEIGHT_FILE" "$lineno" "test explicitly skips endpoint with comment"
done < <(grep -n -i '(skipped\|(skip\|skipped —\|skipped—' "$HEAVYWEIGHT_FILE" 2>/dev/null || true)

# ── Pattern 5: strict_zk toggling (§4.5) ───────────────────────────────────
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    violation "strict_zk-toggling" "$HEAVYWEIGHT_FILE" "$lineno" "strict_zk = false bypasses ZK proof enforcement"
done < <(grep -n 'strict_zk = false' "$HEAVYWEIGHT_FILE" 2>/dev/null || true)

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
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    violation "println-skipped" "$HEAVYWEIGHT_FILE" "$lineno" "println with 'skipped' — endpoint not verified"
done < <(grep -n 'println!(".*[Ss]kipped' "$HEAVYWEIGHT_FILE" 2>/dev/null || true)

# ── Pattern 8: Early-return on Error (§4.8) ────────────────────────────────
while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    lineno=$(echo "$line" | cut -d: -f1)
    violation "early-return-on-err" "$HEAVYWEIGHT_FILE" "$lineno" "Err branch returns Ok(()) — silently skips subsequent tests"
done < <(grep -n 'Err.*=>.*return Ok(())' "$HEAVYWEIGHT_FILE" 2>/dev/null || true)

# ── Pattern 9: empty_witnesses in harness METHODS (§4.9) ────────────────────
# empty_witnesses in spawn() for PK building is correct.
# empty_witnesses in a harness method (after spawn) is a stub.
grep -rn 'empty_witnesses' "$HARNESS_DIR" 2>/dev/null | while IFS=: read -r file lineno rest; do
    [[ -z "$file" ]] && continue
    # Skip spawn() constructors — ProvingKey building requires empty_witnesses
    if [[ "$rest" =~ spawn|ProvingKey::build|ZkCircuit::new.*empty_witnesses ]]; then
        continue
    fi
    violation "empty-witnesses" "$file" "$lineno" "empty_witnesses() in harness method — proves nothing about contract logic"
done

# ── Also check: stubs that create proofs with empty public inputs ───────────
# Exclude spawn() constructors — empty_witnesses for PK building is correct there
# Exclude verify_zk_coverage — it validates key building with empty witnesses
grep -rn 'Proof::create.*&\[\]' "$HARNESS_DIR" 2>/dev/null | while IFS=: read -r file lineno rest; do
    [[ -z "$file" ]] && continue
    # Skip spawn() — ProvingKey construction uses empty_witnesses legitimately
    if [[ "$rest" =~ spawn|verify_zk_coverage|empty_witnesses.*unwrap ]]; then
        continue
    fi
    violation "empty-proof-stub" "$file" "$lineno" "Proof::create with empty public inputs (&[])"
done

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
