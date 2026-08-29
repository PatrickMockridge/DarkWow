# DarkWow Testnet Pipeline — traps and cleanup
#
# Error handling: ERR trap catches command failures, signal traps catch
# external kills, EXIT trap tears down Docker resources regardless of
# how the script terminates.
#
# Sourced by test_pipeline.sh after output.sh and config.sh.
# cleanup_on_exit references $COMPOSE_FILE, which is defined in config.sh.

set -e
set -E   # inherit ERR trap into shell functions
set -o pipefail  # pipe failures trigger ERR (not just last command)

# Tiered error handling: critical vs soft sections.
# Default is critical — any command failure kills the pipeline.
# enter_soft_section() downgrades failures to warnings (pipeline continues).
# enter_critical_section() restores fatal-on-failure behavior.
# Use soft sections for: docker exec on containers that may restart,
# RPC polls that can legitimately timeout, transient network errors.
# Use critical sections for: consensus verification, genesis checks,
# build failures, anything where a failure means the test is invalid.
_ERR_MODE="critical"
_PIPELINE_STOP_FILE="/tmp/darkwow_pipeline_fatal"

enter_critical_section() {
    _ERR_MODE="critical"
}

enter_soft_section() {
    _ERR_MODE="soft"
}

# Tiered ERR trap — reports source file, line, and exit code.
# In soft mode: logs a warning and continues (does not exit).
# In critical mode: logs fatal and exits (current behavior).
# With set -o pipefail + set -E, every failure anywhere in the pipeline
# is caught and attributed to the exact source file and line.
_err_handler() {
    local rc=$1
    local lineno=$2
    if [ "$_ERR_MODE" = "soft" ]; then
        echo "[WARN] Soft failure at ${BASH_SOURCE[1]} line $lineno — exit code $rc (non-fatal, continuing)" >&2
    else
        echo "[FATAL] Pipeline failed in ${BASH_SOURCE[1]} at line $lineno — exit code $rc" >&2
        # Write stop file so subsequent phases know to abort gracefully
        echo "1" > "$_PIPELINE_STOP_FILE"
        exit $rc
    fi
}
trap '_err_handler $? $LINENO' ERR

# Signal traps — catch kills that bypass ERR (tmux crash, timeout, ^C).
# EXIT trap handles cleanup; these just print the signal source and exit.
trap 'echo "[FATAL] Pipeline killed by signal — last line ~$BASH_LINENO" >&2; exit 1' INT TERM HUP
# SIGPIPE: fd 2 may be the very pipe that broke (config.sh `2>&1` into tee), so
# echoing to it here would write to a dead pipe and segfault bash. Just exit.
trap 'exit 1' PIPE

# EXIT trap — catches explicit exit (error(), phase_gate) which bypass ERR.
# Ensures containers are torn down regardless of how the script terminates.
trap cleanup_on_exit EXIT

# -------------------------------------------------------------------
# Cleanup handler — runs on EXIT regardless of termination cause.
#
# NEVER destroys containers. Phase 1 (clean) handles deliberate
# teardown. The EXIT trap only cleans temp secret files (keys).
#
# Containers are preserved on ALL exit paths — failure, signal, or
# explicit exit. The user decides when to tear down by running Phase 1:
#   ./test_pipeline.sh --mode native --phase 1
# -------------------------------------------------------------------
cleanup_on_exit() {
    local exit_code=$?

    # Kill the docker events monitor if it was started.
    [ -n "${DOCKER_EVENTS_PID:-}" ] && kill "$DOCKER_EVENTS_PID" 2>/dev/null || true

    # (Secret-file cleanup removed — keys.toml declaration model, no .secrets files.)

    # ── Container death diagnostics ─────────────────────────────────
    # Inspect exited dwow containers for ExitCode, OOMKilled, and host OOM log.
    _container_death_diagnostics() {
        local died
        died=$(docker ps -a --filter status=exited --filter name=dwow --format '{{.Names}}' 2>/dev/null)
        if [ -n "$died" ]; then
            echo ""
            echo "  ── Exited container diagnostics ──"
            for c in $died; do
                echo "  Container $c:"
                docker inspect "$c" --format '    ExitCode={{.State.ExitCode}} OOMKilled={{.State.OOMKilled}} Error={{.State.Error}}' 2>/dev/null || true
            done
            echo "  ── Host OOM killer log (last 10 entries) ──"
            dmesg -T 2>/dev/null | grep -i 'oom\|out of memory\|killed process' | tail -10 || echo "    (dmesg unavailable or no OOM entries)"
            echo ""
        fi
    }

    # ── SIGSEGV / signal-kill diagnostics ──────────────────────────
    if [ "$exit_code" -ge 128 ] 2>/dev/null; then
        local sig_num=$((exit_code - 128))
        echo ""
        echo "==========================================="
        echo "  Pipeline exited with code $exit_code (killed by signal $sig_num)."
        echo ""
        echo "  To identify which binary crashed:"
        echo "    dmesg -T | grep -i segfault | tail -20"
        echo "    docker ps -a --filter status=exited --format '{{.Names}} exit={{.ExitCode}}'"
        if [ -f /tmp/container-deaths.log ]; then
            echo ""
            echo "  Container death events captured during run:"
            cat /tmp/container-deaths.log
        fi
        echo "==========================================="
        _container_death_diagnostics
    fi

    if [ "${FAIL:-0}" -gt 0 ]; then
        echo ""
        echo "==========================================="
        echo "  Pipeline failed — containers preserved."
        _container_death_diagnostics
        echo ""
        echo "  Inspect logs:"
        for c in $(docker ps -q --filter name=dwow 2>/dev/null); do
            name=$(docker ps --format '{{.Names}}' --filter "id=$c" 2>/dev/null)
            [ -n "$name" ] && echo "    docker logs $name"
        done
        echo ""
        echo "  Tear down when done:"
        echo "    docker compose -f $COMPOSE_FILE --profile native down -v"
        echo "==========================================="
    fi

    # Kill the tee process spawned by config.sh's exec redirection. This MUST be
    # last: the diagnostics above write into the tee's pipe (config.sh `2>&1`),
    # and killing it early leaves a readerless pipe whose EPIPE→SIGPIPE segfaults
    # the shell when the PIPE trap tries to report it. Kill only after all output.
    [ -n "${_TEAD_PID:-}" ] && kill "$_TEAD_PID" 2>/dev/null || true
}
