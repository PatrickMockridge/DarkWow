#!/bin/bash
# DarkWow Testnet Full Pipeline
#
# Single entry point for all DarkWow testnet builds and tests.
# Every mode builds the image, starts the stack, and verifies correctness.
#
# Usage:
#   ./test_pipeline.sh --mode native        # 3-node local devnet, native mining
#   ./test_pipeline.sh --mode merge         # 3-node local devnet, merge mining
#   ./test_pipeline.sh --mode bridge        # 3-node + bridge-node, full bridge lifecycle
#   ./test_pipeline.sh --mode join-native   # Single node joins public testnet, native
#   ./test_pipeline.sh --mode join-merge    # Single node joins public testnet, merge
#
# Sequential determinism:
#   Every phase runs to completion before the next begins. No background tasks,
#   no parallel operations. One machine, one thing at a time. This guarantees
#   reproducible results across different machines.
#
# After the pipeline passes, run contract tests:
#   ./test-contracts.sh --mode native
#   ./test-contracts.sh --mode merge

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Instance lockfile — prevents concurrent pipeline runs from corrupting
# each other's Docker state (containers, volumes, images, ports).
LOCKFILE="/tmp/darkwow-pipeline.lock"
exec 200>"$LOCKFILE"
if ! flock -n 200; then
    echo "ERROR: Another pipeline instance is running (lock: $LOCKFILE)"
    echo "  If no pipeline is running, remove the lock: rm -f $LOCKFILE"
    exit 1
fi
# Lock is auto-released on script exit (fd 200 closes).

# --- Library modules (order: dependencies before dependents) ---
source "$SCRIPT_DIR/lib/output.sh"
source "$SCRIPT_DIR/lib/config.sh"
source "$SCRIPT_DIR/lib/traps.sh"
source "$SCRIPT_DIR/lib/helpers.sh"
source "$SCRIPT_DIR/lib/phase_01_clean.sh"
source "$SCRIPT_DIR/lib/phase_02_build.sh"
source "$SCRIPT_DIR/lib/phase_03_prereqs.sh"
source "$SCRIPT_DIR/lib/phase_04_wallet.sh"
source "$SCRIPT_DIR/lib/phase_05_start.sh"
source "$SCRIPT_DIR/lib/phase_06_verify.sh"
source "$SCRIPT_DIR/lib/phase_08_mining.sh"
source "$SCRIPT_DIR/lib/phase_09_blocks.sh"
source "$SCRIPT_DIR/lib/phase_10_wallet_tests.sh"
source "$SCRIPT_DIR/lib/phase_12_bridge.sh"
source "$SCRIPT_DIR/lib/phase_20_report.sh"
source "$SCRIPT_DIR/lib/phase_21_persistence.sh"
source "$SCRIPT_DIR/lib/phase_99_contract_tests.sh"

# Phase timing helper — call at start and end of each phase
phase_time_start() { PHASE_START_TIME=$SECONDS; }
phase_time_end() {
    local name="$1"
    local elapsed=$((SECONDS - PHASE_START_TIME))
    info "Phase '${name}' completed in ${elapsed}s"
}

# Phase gate — stop execution if the previous phase recorded failures.
# Pass the phase name as $1. Reads global FAIL counter.
phase_gate() {
    local phase_name="$1"
    local current_fail="$FAIL"
    if [ "${_PHASE_FAIL_BEFORE:-0}" -lt "$current_fail" ]; then
        local new_fails=$((current_fail - _PHASE_FAIL_BEFORE))
        error "Phase '${phase_name}' recorded ${new_fails} failure(s) — stopping"
        error "Fix the failures above before continuing. Use --resume-from to skip past this phase."
        exit 1
    fi
    _PHASE_FAIL_BEFORE="$current_fail"
}

# ==============================================================================
# Environment sanitization — prevent E2BIG from accumulated env vars.
# The conda build toolchain (30+ compiler vars), ROS2, MKL, NVM, Snap,
# Claude Code, and API keys can push the environment past ARG_MAX (3.2MB).
# This causes execve() to fail with E2BIG ("Argument list too long") on
# any external command, including docker.
# ==============================================================================
for var in $(env | grep -E '^(CONDA_|CMAKE_|ROS_|AMENT_|COLCON_|MKL_|NVM_|NVM_|SNAP_|CLAUDE_|ANTHROPIC_|OPENAI_|GITHUB_|VSCODE_|ELECTRON_|DBUS_|GTK_|XDG_|WAYLAND_|PULSE_|JAVA_|GOPATH|CC$|CXX$|LD$|AR$|AS$|GCC$|CPP$|FC$|F77$|F90$|NM$|OBJCOPY$|OBJDUMP$|RANLIB$|READELF$|STRIP$|CFLAGS|CXXFLAGS|LDFLAGS|CPPFLAGS|PKG_CONFIG_PATH|LD_LIBRARY_PATH|LIBRARY_PATH|CPATH|PYTHONPATH|MANPATH|INFOPATH|GIT_|LESSOPEN|LESSCLOSE|GROFF_|PERL_|LC_|LS_COLORS|PROMPT_COMMAND|_$)' 2>/dev/null | cut -d= -f1); do
    unset "$var" 2>/dev/null
done
echo "  Environment sanitized ($(env | wc -c) bytes)"

# ==============================================================================
# Main dispatch — sequential, one phase at a time.
# --resume-from N skips phases 1 through N-1.
#   Safe to resume from phase 6+ (verification only, no side effects).
#   Phases 1-5 have side effects (clean, build, keygen) — resuming past
#   them requires --skip-build or pre-existing images/keys.
# phase_gate stops execution if the previous phase recorded failures.
# ==============================================================================

_PHASE_FAIL_BEFORE=0

# --phase N: single-phase mode with precondition validation.
if [ "$PHASE_ONLY" -gt 0 ] 2>/dev/null; then
    info "Running single phase $PHASE_ONLY with precondition check..."
    if ! _check_preconditions "$PHASE_ONLY"; then
        error "Preconditions for phase $PHASE_ONLY not met"
    fi
fi

# --resume-from: validate preconditions before resuming.
if [ "$RESUME_FROM" -gt 0 ] 2>/dev/null; then
    if ! _check_preconditions "$RESUME_FROM"; then
        error "Cannot resume from phase $RESUME_FROM — preconditions not met"
    fi
fi

# Stop-after helper — call after each phase_gate to optionally exit early.
_stop_after() {
    local phase_num="$1"
    if [ "$STOP_AFTER" -le "$phase_num" ] 2>/dev/null; then
        info "Stop-after: phase $phase_num reached, stopping (--stop-after $STOP_AFTER)"
        report
        exit 0
    fi
}

# Phase 1: Clean
if [ "$RESUME_FROM" -le 1 ]; then
    phase_time_start; phase_clean;              phase_time_end "clean"
    phase_gate "clean"
    _stop_after 1
fi

# Phase 2: Build
if [ "$RESUME_FROM" -le 2 ]; then
    phase_time_start; phase_build;              phase_time_end "build"
    phase_gate "build"
    _stop_after 2
fi

# Phase 3: Validate prereqs
if [ "$RESUME_FROM" -le 3 ]; then
    phase_time_start; phase_prereqs;            phase_time_end "prereqs"
    phase_gate "prereqs"
    _stop_after 3
fi

# Phase 4: Generate wallet
if [ "$RESUME_FROM" -le 4 ]; then
    phase_time_start; phase_wallet;             phase_time_end "wallet"
    phase_gate "wallet"
    _stop_after 4
fi

if is_wallet_mode; then
    echo "=== Wallet-only mode: complete ==="
    echo "Wallet image built and keypair generated."
    exit 0
fi

# Phase 5: Start containers (local devnet) / Static Config (join modes)
if [ "$RESUME_FROM" -le 5 ]; then
    phase_time_start
    if is_join_mode; then
        phase_join_config; phase_time_end "join_config"
    else
        phase_start;       phase_time_end "start"
    fi
    phase_gate "start_or_config"
    _stop_after 5
fi

# Phase 6: Verify containers (local) / Container Lifecycle (join)
if [ "$RESUME_FROM" -le 6 ]; then
    phase_time_start
    if is_join_mode; then
        phase_join_lifecycle; phase_time_end "join_lifecycle"
    else
        phase_verify;         phase_time_end "verify"
    fi
    phase_gate "verify_or_lifecycle"
    _stop_after 6
fi

# Phase 7: Mining activity (local) / P2P Connectivity (join)
if [ "$RESUME_FROM" -le 7 ]; then
    phase_time_start
    if is_join_mode; then
        phase_join_p2p;        phase_time_end "join_p2p"
    else
        phase_mining_activity; phase_time_end "mining_activity"
    fi
    # Mining activity must be detected — gate blocks pipeline
    phase_gate "mining_or_p2p"
    _stop_after 7
fi

# Phase 8: Block production (local) / Blockchain Sync (join)
if [ "$RESUME_FROM" -le 8 ]; then
    phase_time_start
    if is_join_mode; then
        phase_join_sync; phase_time_end "join_sync"
    else
        phase_blocks;    phase_time_end "blocks"
    fi
    # Block production must be verified — gate blocks pipeline
    phase_gate "blocks_or_sync"
    _stop_after 8
fi

# Wallet verification — only when --with-wallet is used.
if [ "${WITH_WALLET:-0}" -gt 0 ] && ! is_join_mode; then
    if [ "$RESUME_FROM" -le 9 ]; then
        phase_time_start; phase_wallet_verify;   phase_time_end "wallet_verify"
        phase_gate "wallet_verify"
        _stop_after 9
    fi
    if [ "${WITH_WALLET:-0}" -ge 2 ]; then
        if [ "$RESUME_FROM" -le 10 ]; then
            phase_time_start; phase_wallet_transfer; phase_time_end "wallet_transfer"
            phase_gate "wallet_transfer"
            _stop_after 10
        fi
    fi
fi

# Bridge-specific phases (resume-from 11 through 18)
if is_bridge_mode; then
    if [ "$RESUME_FROM" -le 11 ]; then
        phase_time_start; phase_bridge_deploy;              phase_time_end "bridge_deploy"
        phase_gate "bridge_deploy"
    fi
    if [ "$RESUME_FROM" -le 12 ]; then
        phase_time_start; phase_bridge_init;                phase_time_end "bridge_init"
        phase_gate "bridge_init"
    fi
    if [ "$RESUME_FROM" -le 13 ]; then
        phase_time_start; phase_bridge_register_relayer;    phase_time_end "bridge_register_relayer"
        phase_gate "bridge_register_relayer"
    fi
    if [ "$RESUME_FROM" -le 14 ]; then
        phase_time_start; phase_bridge_deposit;             phase_time_end "bridge_deposit"
        phase_gate "bridge_deposit"
    fi
    if [ "$RESUME_FROM" -le 15 ]; then
        phase_time_start; phase_bridge_withdraw;            phase_time_end "bridge_withdraw"
        phase_gate "bridge_withdraw"
    fi
    if [ "$RESUME_FROM" -le 16 ]; then
        phase_time_start; phase_bridge_accept;              phase_time_end "bridge_accept"
        phase_gate "bridge_accept"
    fi
    if [ "$RESUME_FROM" -le 17 ]; then
        phase_time_start; phase_bridge_execute;             phase_time_end "bridge_execute"
        phase_gate "bridge_execute"
    fi
    if [ "$RESUME_FROM" -le 18 ]; then
        phase_time_start; phase_bridge_verify;              phase_time_end "bridge_verify"
        phase_gate "bridge_verify"
    fi
fi

if [ "$RESUME_FROM" -le 19 ]; then
    phase_time_start
    if is_join_mode; then
        phase_join_mining; phase_time_end "join_mining"
    elif is_bridge_mode; then
        report;            phase_time_end "report"
    else
        report;            phase_time_end "report"
    fi
    # Observation phases — never block pipeline (diagnostics only)
fi
_stop_after 19

if [ "$RESUME_FROM" -le 20 ]; then
    phase_time_start; phase_persistence;        phase_time_end "persistence"
    # Observation phase — never blocks pipeline (diagnostics only)
    _stop_after 20
fi

if is_join_mode; then
    report
fi

# Contract E2E tests
phase_time_start; phase_contract_tests;     phase_time_end "contract_tests"
phase_gate "contract_tests"

# ==============================================================================
# Continuous monitoring loop — after all infrastructure checks pass, keep
# observing. "Pass" doesn't mean "finished" — it means "healthy right now."
# The user decides when to stop (Ctrl-C).
# ==============================================================================
if ! is_join_mode; then
    info ""
    info "All infrastructure checks passed. Entering continuous monitoring..."
    info "Press Ctrl-C to stop."
    info ""

    # Build node list for pinging
    _build_monitor_list() {
        MONITOR_NODES=("${NODE0}:31345")
        if [ "$MODE" = "native" ]; then
            case "$NATIVE_NODES" in
                2) MONITOR_NODES+=("dwow-node1:31346") ;;
                5) MONITOR_NODES+=("dwow-node1:31346" "dwow-node2:31350" "dwow-node3:31353" "dwow-node4:31356") ;;
            esac
        elif [ "$MODE" = "merge" ]; then
            MONITOR_NODES+=("dwow-node2:31350")
        fi
    }
    _build_monitor_list

    # Default: one tick then exit. CI and automation need a clean exit code.
    # Set PIPELINE_EXIT_AFTER_SUCCESS=false for continuous monitoring (legacy).
    # NOTE: top-level scope — `local` is illegal outside a function and
    # aborted the pipeline here after all checks passed.
    tick=0
    max_ticks=1
    [ "${PIPELINE_EXIT_AFTER_SUCCESS:-true}" = "false" ] && max_ticks=999999
    while [ "$tick" -lt "$max_ticks" ]; do
        echo ""
        info "--- monitor tick $(date +%H:%M:%S) ---"
        for node_spec in "${MONITOR_NODES[@]}"; do
            NODE_NAME="${node_spec%%:*}"
            NODE_PORT="${node_spec##*:}"
            if container_running "$NODE_NAME" 2>/dev/null; then
                for attempt in 1 2 3; do
                    NODE_BLOCK=$(jsonrpc_get_block "$NODE_NAME" "$NODE_PORT" 2 2>/dev/null) && break
                    sleep 1
                done
                h=$(echo "$NODE_BLOCK" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*' || echo "?")
                info "  $NODE_NAME height=$h"
            else
                warn "  $NODE_NAME not running"
            fi
        done
        tick=$((tick + 1))
        [ "$tick" -lt "$max_ticks" ] && sleep 60
    done
fi
