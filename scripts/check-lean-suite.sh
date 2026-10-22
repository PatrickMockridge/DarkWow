#!/usr/bin/env bash
#
# The IO simulation suite — `proofs/lean/src/Main.lean` — built and run so that it can fail.
#
# WHY THIS EXISTS. Until 2026-09-24 that file was in **no `lean_lib` and no `lean_exe`**, so
# `lake build` never compiled it; **no gate invoked it**; and it did not compile in any case — 21
# errors, on its version at HEAD as well as the working tree, so nothing in it had ever run. That
# combination is why it could both be cited as evidence (`README.md` quoted an "expected output"
# block from it) and be broken for as long as anyone looked. The file now compiles, and the checks
# inside it **throw instead of printing**: four counterexample scans printed `Bugs found: N` and
# exited 0, eight combinatorial expectations printed a ✓/✗ marker and exited 0, and the HAZOP
# summary printed four literal counts nothing could check. This gate is the last step — it runs the
# exe and refuses to swallow a failure.
#
# WHAT IT DOES. Builds and runs the exe through `scripts/lean-build.sh`, which is the only
# sanctioned way to invoke the toolchain here: the guard's cgroup ceiling is what keeps a runaway
# elaboration from taking the host down (measured 2026-09-24 — threads alone were not enough).
# The suite's own output goes to a per-run log under `/tmp`, never a shared path, because this tree
# has already lost evidence to one shared log name.
#
# An exit 0 is only evidence if the suite reached its end, so the two closing markers are required:
# a `main` that returned early would otherwise be indistinguishable from one that ran every check.
# That is a deliberately weak content check — the *checks* are the file's own assertions — and it
# exists because "exits 0 without doing the work" is the failure mode this whole campaign is about.
#
# NEGATIVE CONTROL (measured 2026-09-24, on a copy under /tmp): change `expectedTake := N` to
# `N + 1` in `Main.lean`'s 5a section, rebuild, and this exits 1 printing the assertion failure;
# restore it and it exits 0. The same was done for each check class as it was converted.
#
# Usage: scripts/check-lean-suite.sh
# Exit status: 0 if the suite ran to its end with every check passing; 1 if it failed, with the log's
#              tail; 2 if the Lean lane is held (a gate that could not run says so rather than
#              reporting the property it did not check).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="/tmp/lean-suite-$(date +%Y%m%d-%H%M%S)-$$.log"

# The gate's own completeness arm, extracted so the negative control can exercise it without a
# Lean build: a suite that exits 0 without reaching both closing markers has not run.
markers_ok() {
    local log="$1" missing=0 marker
    for marker in "Combinatorial State Space Validation Complete" "General Theorem Validation Complete"; do
        if ! grep -qF "$marker" "$log"; then
            echo "check-lean-suite: the suite exited 0 without reaching \"$marker\" — it did not run." >&2
            missing=1
        fi
    done
    return "$missing"
}

if [ "${1:-}" = "--self-test" ]; then
    TMP="$(mktemp)"
    trap 'rm -f "$TMP"' EXIT
    printf 'Combinatorial State Space Validation Complete\n' > "$TMP"
    if markers_ok "$TMP" >/dev/null 2>&1; then
        echo "SELF-TEST FAIL: a log missing the closing marker was accepted" >&2
        exit 1
    fi
    printf 'Combinatorial State Space Validation Complete\nGeneral Theorem Validation Complete\n' > "$TMP"
    if ! markers_ok "$TMP" >/dev/null 2>&1; then
        echo "SELF-TEST FAIL: a complete log was refused" >&2
        exit 1
    fi
    echo "SELF-TEST PASS: a run missing its closing marker is refused; a complete log passes"
    echo "  limit: this controls the gate's exit-0-without-reaching-the-end arm; the Lean"
    echo "  assertions themselves are the suite's, with the mutation controls recorded in the header"
    exit 0
fi

LEAN_BUILD_LOG="$LOG" "$ROOT/scripts/lean-build.sh" exe Main >/dev/null 2>&1
status=$?

if [ "$status" -eq 2 ]; then
    echo "check-lean-suite: the Lean lane is held by another build — the suite was NOT run." >&2
    exit 2
fi

if [ "$status" -ne 0 ]; then
    echo "check-lean-suite: the IO simulation suite FAILED (exit $status). Tail of $LOG:"
    tail -n 30 "$LOG"
    exit 1
fi

markers_ok "$LOG" || { echo "  full output: $LOG" >&2; exit 1; }

echo "OK: the IO simulation suite ran to its end, every check in it passed."
echo "    $(grep -c '✓' "$LOG") marker(s) printed clear, 0 failures. Full output: $LOG"
exit 0
