#!/bin/bash
# The one sanctioned way to invoke the Lean4 toolchain in this repository.
#
# WHY THIS EXISTS (2026-09-24, measured — see proofs/lean/README.md for the full account):
#
# `LEAN_NUM_THREADS=4` was this repository's whole guardrail against the Lean toolchain taking the
# machine down, and it is not sufficient. `LEAN_NUM_THREADS` bounds how many `lean` *processes* run
# at once; it says nothing about how much RAM one of them uses, and `decide` is a kernel-evaluation
# workload where a single module can exceed any per-process share of memory at any thread count. On
# 2026-09-24 a `LEAN_NUM_THREADS=4` build of the `DarkFi` library froze this host anyway: the
# previous boot's journal ends mid-chatter with no shutdown sequence and no OOM-killer line, i.e.
# swap thrash — the same signature as the two 2026-09-23 freezes that produced the thread rule.
#
# So threads and memory are two different axes and both have to be bounded. This script bounds the
# second one with a cgroup, which converts the failure mode from "freeze, lose every open window"
# into "the build is killed, exit 137, desktop untouched". That is the whole point: a build that
# dies loudly is a normal build failure; a build that takes the desktop with it is not.
#
# WHY A CGROUP AND NOT `ulimit -v`: Lean mmaps far more than it touches, so a virtual-memory limit
# false-fails on a healthy build. `MemoryMax` bounds *resident* memory, and it applies to the whole
# process tree — every `lean` child that lake spawns inherits the scope, which is exactly what a
# per-process limit would fail to do.
#
# Usage:
#   scripts/lean-build.sh                    # `lake build DarkFi` (the proofs)
#   scripts/lean-build.sh build DarkFi Transcribed
#   scripts/lean-build.sh --stream env lean --run src/CheckAxioms.lean
#
# Everything after the script name is passed to `lake` verbatim, because every Lean4 invocation in
# this tree — build, exe, `env lean --run` — is the same full elaborator and carries the same
# hazard. The script always runs in proofs/lean, where Lake requires its working directory.
#
# `--stream` ALSO passes the command's stdout through to the caller. It exists for the axiom
# collector, which writes a TSV to stdout and whose caller *reads* it: a redirect-only wrapper would
# hand the Python an empty stream, so `script/check_lean_axioms.py` would report "the collector
# reported no theorems" — the guard would have broken the gate it was added to protect. Builds do not
# need it and should not use it; their output belongs in the log.
#
# There is deliberately NO way to bypass the ceiling. A build in an environment that cannot provide
# one (a container without a user systemd) should be a decision made by a person, with the outer
# sandbox's own limit recorded, not a flag passed in the moment.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LEAN_DIR="$REPO_ROOT/proofs/lean"

# The hard thread cap. Fixed rather than configurable: the rule is a maximum of 4, and an option to
# raise it is an option to rediscover the freeze. Lake has no `-j` (its default is the core count).
LEAN_THREADS=4

# The memory ceiling. Chosen to leave well over 30 GiB of this 47 GiB host for the desktop and the
# always-on baseline (microk8s, docker, prometheus, keybase), so that a runaway elaboration hits the
# ceiling long before it reaches swap. `MemorySwapMax=0` is load-bearing: without it the kernel
# swaps instead of killing, which is the freeze this exists to prevent.
#
# If a build legitimately exceeds this, raise it in a commit that records the measured peak — never
# by exporting an env var in the moment.
LEAN_MEMORY_MAX="${LEAN_MEMORY_MAX:-16G}"

STREAM=0
if [ "${1:-}" = "--stream" ]; then
  STREAM=1
  shift
fi

LAKE_ARGS=("$@")
if [ ${#LAKE_ARGS[@]} -eq 0 ]; then
  LAKE_ARGS=(build DarkFi)
fi

LOG="${LEAN_BUILD_LOG:-/tmp/lean-build.log}"

die() {
  echo "lean-build: $1" >&2
  exit 2
}

[ -d "$LEAN_DIR" ] || die "no proofs/lean directory at $LEAN_DIR"
command -v lake >/dev/null 2>&1 || die "\`lake\` is not on PATH"
command -v systemd-run >/dev/null 2>&1 || die "\`systemd-run\` is not on PATH — this script
  refuses to elaborate Lean without a memory ceiling, because that is what took this host down.
  If this is a container, its own memory limit has to be established first; see the header."

# One heavy Lean lane at a time. The thread cap is *per build script*, so two concurrent lanes
# multiply it — the reason the two rules have to hold together.
LOCK="${XDG_RUNTIME_DIR:-/tmp}/darkwow-lean-build.lock"
exec 9>"$LOCK" || die "cannot open lock file $LOCK"
if ! flock -n 9; then
  die "another Lean build holds $LOCK — run one heavy Lean lane at a time"
fi

# This script's own progress goes to stderr, always. Under `--stream`, stdout is the command's output
# and nothing else — a caller that parses it must not have to skip our chatter.
echo "lean-build: lake ${LAKE_ARGS[*]}" >&2
echo "lean-build: LEAN_NUM_THREADS=$LEAN_THREADS, MemoryMax=$LEAN_MEMORY_MAX (swap 0), log $LOG" >&2

# `--scope` runs the child in the caller's context (synchronously, sharing stdio), so all output
# lands in the log and the exit status below is the build's own; `-q` suppresses systemd's chatter.
#
# Two details here are load-bearing, and both were found by testing this script rather than by
# reading about it:
#
#   * `exec 9>&-` closes the lock fd for the child. Without it the child *inherits* it, because a
#     scope's processes are forked from the caller's context — so a `lean` that outlives the wrapper
#     would hold the build lock and block every later build.
#   * `--unit` gives the scope a name that can be queried afterwards. The exit status alone cannot
#     say whether the ceiling fired: a build killed by `MemoryMax` surfaces as SIGTERM (143) at
#     least as readily as SIGKILL (137), because systemd stops the scope after the kernel kills
#     inside it. Measured both ways. `Result=oom-kill` is the fact; the signal is a guess.
cd "$LEAN_DIR" || die "cannot cd to $LEAN_DIR"
UNIT="darkwow-lean-build-$$.scope"
SCOPE=(systemd-run --user --scope -q --unit="$UNIT"
       -p MemoryMax="$LEAN_MEMORY_MAX" -p MemorySwapMax=0
       -- bash -c 'exec 9>&-; exec env "LEAN_NUM_THREADS=$1" lake "${@:2}"'
       lean-build "$LEAN_THREADS" "${LAKE_ARGS[@]}")
if [ "$STREAM" = 1 ]; then
  "${SCOPE[@]}" > >(tee "$LOG") 2>&1
else
  "${SCOPE[@]}" >"$LOG" 2>&1
fi
STATUS=$?

if [ $STATUS -eq 0 ]; then
  echo "lean-build: OK — full output in $LOG" >&2
  exit 0
fi

# A non-zero exit is either the ceiling firing or an ordinary build failure, and telling those apart
# is the point of this script — so it is read from systemd, not guessed from the signal.
RESULT="$(systemctl --user show "$UNIT" -p Result --value 2>/dev/null || true)"
if [ "$RESULT" = "oom-kill" ]; then
  cat >&2 <<EOF
lean-build: the build exceeded MemoryMax=$LEAN_MEMORY_MAX and was killed (unit $UNIT, exit $STATUS).
lean-build: THE DESKTOP IS INTACT — this is the guardrail working, not a corrupt run.
lean-build: read $LOG to see which module was elaborating, then either raise LEAN_MEMORY_MAX in a
lean-build: commit that records the measured peak, or fix what is growing without bound.
EOF
elif [ $STATUS -eq 137 ] || [ $STATUS -eq 143 ]; then
  cat >&2 <<EOF
lean-build: the build died by signal (exit $STATUS) and the scope's recorded result could not be read
lean-build: (Result='${RESULT:-unavailable}'), so the ceiling is the likely but unconfirmed cause.
lean-build: last 60 lines of $LOG:
EOF
  tail -n 60 "$LOG" >&2
else
  echo "lean-build: FAILED (exit $STATUS) — last 60 lines of $LOG:" >&2
  tail -n 60 "$LOG" >&2
fi
exit $STATUS
