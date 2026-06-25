# DarkWow Testnet Pipeline — traps and cleanup
#
# Error handling: ERR trap catches command failures, signal traps catch
# external kills, EXIT trap tears down Docker resources regardless of
# how the script terminates.
#
# Sourced by test_pipeline.sh after output.sh and config.sh.
# cleanup_on_exit references $COMPOSE_FILE, which is defined in config.sh.

set -e
set -E  # inherit ERR trap into shell functions

# Fatal error trap — every failure must be visible.
# set -e kills the script on any non-zero exit; without this trap
# the log just stops mid-line with no clue what failed.
trap 'rc=$?; echo "[FATAL] Pipeline failed at line $BASH_LINENO — exit code $rc" >&2; exit $rc' ERR

# Signal traps — catch kills that bypass ERR (tmux crash, timeout, ^C).
# EXIT trap handles cleanup; these just print the signal source and exit.
trap 'echo "[FATAL] Pipeline killed by signal — last line ~$BASH_LINENO" >&2; exit 1' INT TERM HUP PIPE

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
    # Temp files: clean up secret files (these contain keys — always clean)
    for sf in /tmp/dwow_mining_secret_*; do
        [ -e "$sf" ] && rm -f "$sf" 2>/dev/null || true
    done

    if [ "${FAIL:-0}" -gt 0 ]; then
        echo ""
        echo "==========================================="
        echo "  Pipeline failed — containers preserved."
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
}
