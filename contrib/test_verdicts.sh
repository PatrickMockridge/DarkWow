#!/usr/bin/env bash
#
# Verdict inventory: turns libtest output into a per-test table of target / test / verdict.
#
# WHY THIS EXISTS. The repository has no per-test verdict record anywhere — no JUnit output, no
# captured `--format json`, no script that produces one. So "which tests are red and which are
# green" could not be answered on demand; the only whole-suite figure available was a single
# log twelve hours stale, and every fix since had individual verdicts only. That is a knowledge
# gap, not a testing gap: the tests run and report, and nothing keeps the report.
#
# It parses the same text output a developer reads, so a recorded log and a fresh run go
# through identical code — a table built from `make_test.txt` and a table built from a run
# today are comparable line for line.
#
# Usage:
#   contrib/test_verdicts.sh parse <log> [<log> ...]     # table on stdout
#   contrib/test_verdicts.sh parse --causes <log> ...    # plus the verbatim failure lines
#   contrib/test_verdicts.sh run <cargo-test-args...>    # run, log to /tmp, then parse
#
# Table columns (tab-separated):
#   target  <TAB>  test  <TAB>  verdict
# where verdict is one of: ok | FAILED | ignored | filtered-out | target-summary
#
# `run` mode never filters and never truncates: the whole output goes to a /tmp file whose path
# is printed, because a table you cannot trace back to a log is a claim, not a measurement.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# libtest's text output, with colour stripped (a recorded log may or may not be coloured).
parse_one() {
    # shellcheck disable=SC2016
    sed 's/\x1b\[[0-9;]*m//g' "$1" | awk '
        # cargo announces each test binary it runs; that is the target boundary.
        /^[[:space:]]+Running / {
            line = $0
            sub(/^[[:space:]]+Running /, "", line)
            split(line, parts, " ")
            target = parts[1]
            next
        }
        # a per-test verdict
        /^test .+ \.\.\. / {
            line = $0
            sub(/^test /, "", line)
            if (line ~ / \.\.\. ok$/)          { v = "ok" }
            else if (line ~ / \.\.\. FAILED$/) { v = "FAILED" }
            else if (line ~ / \.\.\. ignored/) { v = "ignored" }
            else if (line ~ / \.\.\. filtered out/) { v = "filtered-out" }
            else next
            sub(/ \.\.\. .*$/, "", line)
            print (target == "" ? "(unknown target)" : target) "\t" line "\t" v
            next
        }
        # the per-target roll-up, kept because it is the line everyone quotes
        /^test result:/ {
            line = $0
            sub(/^test result: /, "", line)
            print (target == "" ? "(unknown target)" : target) "\t(target-summary)\t" line
            next
        }
    ' | sort -u
}

causes() {
    sed 's/\x1b\[[0-9;]*m//g' "$1" | grep -E "^Error: |panicked at |^ *left:|^ *right:|^ *INFRA-FAIL|^ *TEST-FAIL" || true
}

mode="${1:-}"
shift || true

case "$mode" in
    parse)
        show_causes=0
        if [ "${1:-}" = "--causes" ]; then show_causes=1; shift; fi
        if [ $# -eq 0 ]; then
            echo "test_verdicts: parse needs at least one log file" >&2
            exit 2
        fi
        for f in "$@"; do
            [ -f "$f" ] || { echo "test_verdicts: no such log: $f" >&2; exit 2; }
            parse_one "$f"
            if [ "$show_causes" = 1 ]; then
                causes "$f" | sed "s|^|$(basename "$f")\tCAUSE\t|"
            fi
        done
        ;;
    run)
        if [ $# -eq 0 ]; then
            echo "test_verdicts: run needs cargo test arguments, e.g. run -p dwow_chain --lib" >&2
            exit 2
        fi
        slug="$(printf '%s' "$*" | tr ' /-' '___')"
        log="/tmp/verdicts_${slug}.txt"
        echo "test_verdicts: running \`cargo test $*\` → $log" >&2
        # No filter, no truncation, no custom timeout: the whole run is the measurement.
        # RAYON_NUM_THREADS defaults to 10; export a lower value (e.g. 4) to bound LLVM
        # codegen memory on memory-constrained shared hosts (compile-fragilities-hazop.md F4).
        RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}" RUST_MIN_STACK=67108864 cargo test "$@" > "$log" 2>&1
        rc=$?
        echo "test_verdicts: cargo exit=$rc" >&2
        parse_one "$log"
        ;;
    *)
        sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 2
        ;;
esac
