# shellcheck shell=bash
# Heavy-run wrappers for the CI lanes.
#
# Two mechanical constraints from AGENTS.md are enforced here rather than repeated in every lane:
#   * one heavy job at a time (`flock /tmp/darkwow-heavy.lock`), because the umbrella, the Docker
#     pipeline, Lean and the clippy census each assume the machine is theirs;
#   * a cgroup memory ceiling with swap disabled for cargo-heavy work. Lean is the exception: it
#     always goes through `scripts/lean-build.sh`, which owns its own 16G ceiling and lock, so it is
#     never wrapped here.
#
# Sourced by `contrib/ci/run-ci-lane.sh`.

CI_HEAVY_LOCK="${CI_HEAVY_LOCK:-/tmp/darkwow-heavy.lock}"

ci_heavy_lock() {
    # Wait rather than fail: a queued nightly behind a merge is the intended serialization. The
    # workflow's concurrency group is the first line of defence; this is the second.
    exec 9>"$CI_HEAVY_LOCK"
    if ! flock -w "${CI_HEAVY_LOCK_WAIT:-7200}" 9; then
        echo "CI: could not acquire the heavy lock within ${CI_HEAVY_LOCK_WAIT:-7200}s" >&2
        return 1
    fi
    echo "CI: heavy lock acquired ($CI_HEAVY_LOCK)"
}

ci_heavy_scope() {
    local cap="${1:-20G}"; shift
    if [ "${CI_NO_CGROUP:-0}" = "1" ]; then
        "$@"
        return $?
    fi
    if command -v systemd-run >/dev/null 2>&1 && systemd-run --user --scope true >/dev/null 2>&1; then
        systemd-run --user --scope -q --unit="darkwow-ci-$$-$RANDOM" \
            -p "MemoryMax=$cap" -p MemorySwapMax=0 -- "$@"
    else
        echo "CI WARNING: systemd-run --user is unavailable; running '$1' without a memory ceiling" >&2
        "$@"
    fi
}

ci_require_command() {
    local cmd="$1" why="$2"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "CI BLOCKED: '$cmd' is required ($why) but is not on PATH." >&2
        return 78
    fi
}
