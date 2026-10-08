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
#   contrib/test_verdicts.sh tally <log> [<log> ...]     # whole-chunk sums of the per-suite summaries
#   contrib/test_verdicts.sh run <cargo-test-args...>    # run, log to /tmp, then parse
#
# Table columns (tab-separated):
#   target  <TAB>  suite  <TAB>  test  <TAB>  verdict
# where verdict is one of: ok | FAILED | ignored | filtered-out | target-summary
#
# WHAT `target` IS, AND WHY IT CHANGED (`OBL-C135`). It used to be the first word of cargo's
# `Running` banner, which is not an identity: every lib suite is literally `unittests`, every crate
# names its file `tests/integration.rs` (33 of them), and every doctest suite is `Doc-tests`. So 157
# suites shared 19 strings, and because the table is `sort -u`'d, rows that agreed in every column
# **collapsed** — the table reported 57 `target-summary` rows for a log carrying 157, and summing it
# gave 1166 passed where the log gave 1314, an undercount of 148 that nothing in the output hinted at.
#
# `target` is now unique per suite: the test **binary** cargo prints in the banner's parentheses
# (`dwow_a-1111111111111111`, `integration-2222222222222222`), which carries cargo's per-crate
# metadata hash, and `<crate>-doctests` for a doctest suite — the crate name being the one thing
# libtest does put on that banner. `suite` keeps the human path (`tests/integration.rs`), so a reader
# loses nothing and gains a key they can group by. The crate name behind an `integration-<hash>`
# binary is NOT recoverable from the log alone; the hash is what makes the rows distinct, and that is
# stated rather than papered over.
#
# `tally` sums the per-suite summaries into the whole-chunk figure the table exists to make possible.
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
        # A doctest suite: `   Doc-tests <crate>`. Its own rule, because the crate name is the only
        # identity the banner carries and the generic rule below would key it on the word "Doc-tests".
        /^[[:space:]]+Doc-tests / {
            line = $0
            sub(/^[[:space:]]+Doc-tests /, "", line)
            target = line "-doctests"
            suite = "Doc-tests"
            next
        }
        # cargo announces each test binary it runs: `Running <path> (<binary>)`. The BINARY is the
        # identity — see the header for why the path is not — and the path is kept in `suite`.
        /^[[:space:]]+Running / {
            line = $0
            sub(/^[[:space:]]+Running /, "", line)
            if (match(line, /\([^)]*\)[[:space:]]*$/)) {
                binary = substr(line, RSTART + 1, RLENGTH - 2)
                n = split(binary, bparts, "/")
                target = bparts[n]
                suite = substr(line, 1, RSTART - 1)
                sub(/[[:space:]]+$/, "", suite)
            } else {
                target = line
                suite = line
            }
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
            print (target == "" ? "(unknown target)" : target) "\t" (suite == "" ? "-" : suite) "\t" line "\t" v
            next
        }
        # the per-suite roll-up, kept because it is the line everyone quotes. The verdict is
        # `target-summary` and not `(target-summary)`: the header has documented it without
        # parentheses since this script was written, and `tally` filters on the documented word.
        /^test result:/ {
            line = $0
            sub(/^test result: /, "", line)
            print (target == "" ? "(unknown target)" : target) "\t" (suite == "" ? "-" : suite) "\ttarget-summary\t" line
            next
        }
    ' | sort -u
}

# Sum the per-suite summaries into the whole-chunk figure — what the table exists to make possible,
# and the number that was wrong by 148 tests before the targets were unique (`OBL-C135`).
tally() {
    for f in "$@"; do
        parse_one "$f"
    done | awk -F'\t' '
        $3 == "target-summary" {
            if (match($4, /[0-9]+ passed/))  { passed  += substr($4, RSTART, RLENGTH - 7) }
            if (match($4, /[0-9]+ failed/))  { failed  += substr($4, RSTART, RLENGTH - 7) }
            if (match($4, /[0-9]+ ignored/)) { ignored += substr($4, RSTART, RLENGTH - 8) }
            if (match($4, /[0-9]+ measured/)){ measured+= substr($4, RSTART, RLENGTH - 9) }
            suites++
        }
        END { printf "suites %d  passed %d  failed %d  ignored %d  measured %d\n",
                     suites, passed, failed, ignored, measured }
    '
}

causes() {
    sed 's/\x1b\[[0-9;]*m//g' "$1" | grep -E "^Error: |panicked at |^ *left:|^ *right:|^ *INFRA-FAIL|^ *TEST-FAIL" || true
}

mode="${1:-}"
shift || true

case "$mode" in
    --self-test)
        # Negative control. A cargo invocation that MUST fail, and fails in well
        # under a second: an argument cargo does not accept. The wrapper has to
        # propagate that status. It previously did not — see the `run` arm.
        if "$0" run --definitely-not-a-cargo-flag >/dev/null 2>&1; then
            echo "SELF-TEST FAIL: returned 0 for a cargo invocation that failed" >&2
            echo "  (a runner whose exit code is not its run's verdict is not a runner)" >&2
            exit 1
        fi
        # The table's own controls (`OBL-C135`): a synthetic log carrying the three shapes that used
        # to collapse under `sort -u` — two suites sharing the path `tests/integration.rs`, a lib
        # suite, and a doctest suite whose crate name only its own banner carries.
        tmp="$(mktemp -d)"
        trap 'rm -rf "$tmp"' EXIT
        cat > "$tmp/log" <<'LOGEOF'
     Running unittests src/lib.rs (target/release/deps/dwow_a-1111111111111111)

test foo::one ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/integration.rs (target/release/deps/integration-2222222222222222)

test t1 ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/integration.rs (target/release/deps/integration-3333333333333333)

test t1 ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests dwow_a

test src/lib.rs - (line 1) ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
LOGEOF
        out="$("$0" parse "$tmp/log")"
        t="$(printf '%s\n' "$out" | cut -f1 | sort -u | grep -c '^dwow_a-1111111111111111$')"
        if [ "$t" != 1 ]; then echo "SELF-TEST FAIL: the lib suite was not keyed on its binary" >&2; exit 1; fi
        t="$(printf '%s\n' "$out" | cut -f1 | sort -u | grep -c '^integration-')"
        if [ "$t" != 2 ]; then
            echo "SELF-TEST FAIL: two suites sharing tests/integration.rs produced $t target(s), not 2" >&2
            exit 1
        fi
        if ! printf '%s\n' "$out" | cut -f1 | grep -qF 'dwow_a-doctests'; then
            echo "SELF-TEST FAIL: the doctest suite did not carry its crate name" >&2
            exit 1
        fi
        # The collapse itself: four `test result:` lines in the log, four `target-summary` rows out.
        t="$(printf '%s\n' "$out" | cut -f3 | grep -c 'target-summary')"
        if [ "$t" != 4 ]; then
            echo "SELF-TEST FAIL: 4 suites produced $t summary row(s) — rows are still collapsing" >&2
            exit 1
        fi
        # And the whole-chunk figure the rows make possible: 1+2+3+4 = 10 passed over 4 suites.
        t="$("$0" tally "$tmp/log" | tr -s ' ' | cut -d' ' -f2,4)"
        if [ "$t" != "4 10" ]; then
            echo "SELF-TEST FAIL: tally said '$t', expected '4 10'" >&2
            exit 1
        fi
        echo "SELF-TEST PASS: a failing cargo invocation propagates a non-zero status, and the table"
        echo "               keys every suite uniquely — lib, two same-path suites, and a doctest suite"
        echo "               — with the four summaries summing to 10 passed over 4 suites"
        ;;
    tally)
        if [ $# -eq 0 ]; then
            echo "test_verdicts: tally needs at least one log file" >&2
            exit 2
        fi
        for f in "$@"; do
            [ -f "$f" ] || { echo "test_verdicts: no such log: $f" >&2; exit 2; }
        done
        tally "$@"
        ;;
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
        # codegen memory on memory-constrained shared hosts
        # (doc/src/dev/testing/build-resource-tuning.md — the budget and its derivation).
        RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}" RUST_MIN_STACK=67108864 cargo test "$@" > "$log" 2>&1
        rc=$?
        echo "test_verdicts: cargo exit=$rc" >&2
        parse_one "$log"
        # The run's verdict IS cargo's status, so it is the script's status.
        # Without this the script exited with parse_one's — `sed | awk | sort`,
        # i.e. 0 — while the real status went only to stderr. Measured
        # 2026-09-25: it printed `cargo exit=101` and exited 0 for a chunk that
        # failed, so a caller reading the exit code saw a failing run as passing.
        exit "$rc"
        ;;
    *)
        sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 2
        ;;
esac
