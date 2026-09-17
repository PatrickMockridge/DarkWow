#!/usr/bin/env bash
#
# Report the peak resident set size (and wall-clock) of a command.
#
# Why: the build/test parallelism budget is derived from "memory per rustc" and
# "memory per spawned node" (compile-fragilities-hazop.md F9). This wraps
# /usr/bin/time -v so those numbers are re-measurable instead of guessed.
#
# Usage:
#   contrib/measure_rss.sh cargo build -p dwowd -j 1
#   contrib/measure_rss.sh cargo test -p dwowd --lib -- --test-threads=1
#
# `/usr/bin/time -v` accumulates children's rusage via wait4, so "Maximum
# resident set size" is the peak of the whole process tree (cargo + its rustc /
# test children), which is the number the budget needs.

set -uo pipefail

/usr/bin/time -v "$@" 2>&1 | grep -E "Maximum resident set size|Elapsed \(wall clock\)" \
    | sed -e 's/^\t//' -e 's/kbytes)/KiB)/'
