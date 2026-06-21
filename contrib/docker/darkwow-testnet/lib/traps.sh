# DarkWow Testnet Pipeline — traps and cleanup
#
# Error handling: ERR trap catches command failures, signal traps catch
# external kills, EXIT trap tears down Docker resources regardless of
# how the script terminates.
#
# Sourced by test_pipeline.sh after output.sh and before config.sh.
# cleanup_on_exit references $COMPOSE_FILE, which is defined in config.sh.
# Config is sourced before traps in test_pipeline.sh.

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
# Tears down Docker resources and removes temp files so the host is
# left clean regardless of how the pipeline exits.
# -------------------------------------------------------------------
cleanup_on_exit() {
    # Containers: stop all dwow-* containers from any profile
    for c in $(docker ps -q --filter name=dwow 2>/dev/null); do
        docker stop "$c" 2>/dev/null || true
        docker rm -f "$c" 2>/dev/null || true
    done
    # Networks/volumes: compose down all profiles (ignore errors — some may not be up)
    for profile in native merge bridge wallet join-merge; do
        docker compose -f "$COMPOSE_FILE" --profile "$profile" down 2>/dev/null || true
    done
    # Temp files: clean up secret files
    for sf in /tmp/dwow_mining_secret_*; do
        [ -e "$sf" ] && rm -f "$sf" 2>/dev/null || true
    done
}
