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
source "$SCRIPT_DIR/lib/phase_fee_window.sh"
source "$SCRIPT_DIR/lib/phase_12_bridge.sh"
source "$SCRIPT_DIR/lib/phase_20_report.sh"
source "$SCRIPT_DIR/lib/phase_21_persistence.sh"
source "$SCRIPT_DIR/lib/phase_98_capability_tests.sh"
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

# ══════════════════════════════════════════════════════════════════════════════
# Self-test — the controls behind three closed rows, without Docker.
#
# R8: a control is not a control until it can fail. Each case runs the real function from the
# sourced libs against a fixture of the daemon's log format, and plants the pre-fix parse beside
# it; if the planted parse still succeeds, the fixture does not exercise the defect and the test
# fails. OBL-C264 (phase 9 names the genesis/artifact failure), OBL-C268 (duplicate listeners are
# refused and named), OBL-C270 (the field-bearing boundary line is the one parsed).
# ══════════════════════════════════════════════════════════════════════════════
_pipeline_self_test() {
    local _st_fails=0
    local _st_log
    _st_log="$(mktemp)"

    _st_node0() {
        printf '%s\n' \
          'Fee window boundary at height 19: circuit_premium=CfValue(900000) circuit_standard=CfValue(800000) wasm_premium=CfValue(950000) wasm_standard=CfValue(850000) flags=0x0101' \
          'Fee window boundary at height 20: circuit_premium=CfValue(1000000) circuit_standard=CfValue(1000000) wasm_premium=CfValue(1100000) wasm_standard=CfValue(900000) flags=0x0101' \
          'Fee window boundary at height 20: connect-side derivation applied' \
          'miner_task: caught up'
    }
    _st_node1() {
        printf '%s\n' \
          'Fee window boundary at height 20: circuit_premium=CfValue(1000000) circuit_standard=CfValue(1000000) wasm_premium=CfValue(1100000) wasm_standard=CfValue(900000) flags=0x0101'
    }
    _st_artifact() {
        printf '%s\n' \
          'thread main panicked: contract memory export declares maximum 0' \
          'miner_task: caught up'
    }
    # Shadow the docker binary for the duration of this function.
    docker() {
        case "$1:$2" in
            logs:dwow-node0) _st_node0 ;;
            logs:dwow-node1) _st_node1 ;;
            logs:dwow-artifact) _st_artifact ;;
            *) return 1 ;;
        esac
    }
    _st_eq() {
        local name="$1" want="$2" got="$3"
        if [ "$got" != "$want" ]; then
            echo "SELF-TEST FAIL: ${name}: want '${want}' got '${got}'"
            _st_fails=$((_st_fails + 1))
        fi
    }
    # The pre-fix shape: last boundary line, no field filter. It must return empty on this fixture.
    _st_prefix_flags() {
        docker logs "$1" 2>&1 | grep "Fee window boundary" | tail -1 \
            | grep -oP 'flags=0x[0-9a-fA-F]+' | cut -d'=' -f2 || true
    }
    _st_prefix_cf() {
        docker logs "$1" 2>&1 | grep "Fee window boundary" | tail -1 \
            | grep -oP "circuit_premium=CfValue\(\K[0-9]+" || true
    }

    _st_eq "the fixture exercises the pre-fix flags defect" "" "$(_st_prefix_flags dwow-node0)"
    _st_eq "the fixture exercises the pre-fix cf defect" "" "$(_st_prefix_cf dwow-node0)"
    _st_eq "C270 flags from a field-poor tail" "0x0101" "$(_get_flags_from_log dwow-node0)"
    _st_eq "C270 cf from a field-poor tail" "1000000" "$(_get_cf_from_log dwow-node0 circuit_premium)"
    _st_eq "C270 cf at the named height" "1000000" "$(_get_cf_at_height dwow-node0 circuit_premium 20)"
    _st_eq "C270 flags at the named height" "0x0101" "$(_get_flags_at_height dwow-node0 20)"
    _st_eq "C270 height selection excludes another window" "900000" \
        "$(_get_cf_at_height dwow-node0 circuit_premium 19)"
    _st_eq "C270 nodes agree at the boundary" "$(_get_cf_at_height dwow-node0 wasm_premium 20)" \
        "$(_get_cf_at_height dwow-node1 wasm_premium 20)"
    _st_eq "C264 names the genesis/artifact line" \
        "thread main panicked: contract memory export declares maximum 0" \
        "$(_fatal_artifact_logs dwow-artifact | head -1)"
    _st_eq "C264 is silent on a clean log" "" "$(_fatal_artifact_logs dwow-node1)"
    if bash "$SCRIPT_DIR/entrypoint.sh" --self-test >"$_st_log" 2>&1; then
        :
    else
        echo "SELF-TEST FAIL: C268 entrypoint port self-test failed:"
        sed 's/^/  /' "$_st_log"
        _st_fails=$((_st_fails + 1))
    fi
    rm -f "$_st_log"

    if [ "$_st_fails" -ne 0 ]; then
        echo "SELF-TEST FAIL: ${_st_fails} case(s)"
        return 1
    fi
    echo "SELF-TEST PASS: C264/C268/C270 parsers refuse the pre-fix shape and pass the current one."
    return 0
}

if [ "${SELFTEST:-false}" = "true" ]; then
    if _pipeline_self_test; then
        exit 0
    else
        exit 1
    fi
fi

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

# ══════════════════════════════════════════════════════════════════════════════
# Verification preflight — before any phase, before every mode (wallet-only included).
#
# A finding is decided or it is not. While any tracker row is OPEN, BLOCKED or DEFERRED, this
# pipeline would be computing a verdict over code the register calls defective. It refuses, names
# every blocking row, and has no override. The build clones origin/PIPELINE_BRANCH, so the tracker
# read is that ref and HEAD must equal it; uncommitted tracker edits are refused too.
# ══════════════════════════════════════════════════════════════════════════════
if ! bash "$REPO_ROOT/scripts/check-tracker-settled.sh" \
        --ref "origin/${PIPELINE_BRANCH}" --require-clean; then
    PREFLIGHT_REFUSED=1
    error "Refusing to run: the defect register has undecided findings." || true
    error "Legal exits: CLOSED with a failing-before control, or RESIDUAL with an ALARP record." || true
    exit 1
fi

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

# Start Docker events monitor — captures container deaths (including SIGSEGV)
# for post-mortem analysis. Killed in cleanup_on_exit trap.
docker events --filter type=container --filter event=die \
    --format '{{.Time}} {{.Actor.Attributes.name}} exit={{.Actor.Attributes.exitCode}}' \
    > /tmp/container-deaths.log 2>&1 &
DOCKER_EVENTS_PID=$!
export DOCKER_EVENTS_PID

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
# P2#16: Docker daemon liveness gate before wallet phases that depend on
# docker exec. Prevents "container missing" from being misinterpreted as
# daemon failure vs container crash.
if [ "${WITH_WALLET:-0}" -gt 0 ] && ! is_join_mode; then
    if ! docker info >/dev/null 2>&1; then
        fail "Docker daemon is not reachable before wallet phases — cannot proceed"
        report; exit 1
    fi
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

    # Fee window signalling verification (runs after wallet transfer when wallets exist)
    if [ "$RESUME_FROM" -le 11 ]; then
        phase_time_start; phase_fee_window; phase_time_end "fee_window"
        phase_gate "fee_window"
        _stop_after 11
    fi
fi

# Bridge-specific phases (resume-from 12 through 19)
if is_bridge_mode; then
    if [ "$RESUME_FROM" -le 12 ]; then
        phase_time_start; phase_bridge_deploy;              phase_time_end "bridge_deploy"
        phase_gate "bridge_deploy"
    fi
    if [ "$RESUME_FROM" -le 13 ]; then
        phase_time_start; phase_bridge_init;                phase_time_end "bridge_init"
        phase_gate "bridge_init"
    fi
    if [ "$RESUME_FROM" -le 14 ]; then
        phase_time_start; phase_bridge_register_relayer;    phase_time_end "bridge_register_relayer"
        phase_gate "bridge_register_relayer"
    fi
    if [ "$RESUME_FROM" -le 15 ]; then
        phase_time_start; phase_bridge_deposit;             phase_time_end "bridge_deposit"
        phase_gate "bridge_deposit"
    fi
    if [ "$RESUME_FROM" -le 16 ]; then
        phase_time_start; phase_bridge_withdraw;            phase_time_end "bridge_withdraw"
        phase_gate "bridge_withdraw"
    fi
    if [ "$RESUME_FROM" -le 17 ]; then
        phase_time_start; phase_bridge_accept;              phase_time_end "bridge_accept"
        phase_gate "bridge_accept"
    fi
    if [ "$RESUME_FROM" -le 18 ]; then
        phase_time_start; phase_bridge_execute;             phase_time_end "bridge_execute"
        phase_gate "bridge_execute"
    fi
    if [ "$RESUME_FROM" -le 19 ]; then
        phase_time_start; phase_bridge_verify;              phase_time_end "bridge_verify"
        phase_gate "bridge_verify"
    fi
fi

if [ "$RESUME_FROM" -le 20 ]; then
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
_stop_after 20

if [ "$RESUME_FROM" -le 21 ]; then
    phase_time_start; phase_persistence;        phase_time_end "persistence"
    # Observation phase — never blocks pipeline (diagnostics only)
    _stop_after 21
fi

if is_join_mode; then
    report
fi

# L1 capability write-path tests (opt-in via --capability-tests)
phase_time_start; phase_capability_tests;   phase_time_end "capability_tests"
phase_gate "capability_tests"

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

    # Default: keep monitoring indefinitely so the devnet stays up for
    # interactive work (contract tests, non-genesis deploys, fuzzing).
    # Set PIPELINE_EXIT_AFTER_SUCCESS=true for CI/automation (one tick, then a
    # clean exit). NOTE: top-level scope — `local` is illegal outside a function.
    tick=0
    max_ticks=999999
    [ "${PIPELINE_EXIT_AFTER_SUCCESS:-false}" = "true" ] && max_ticks=1
    while [ "$tick" -lt "$max_ticks" ]; do
        echo ""
        info "--- monitor tick $(date +%H:%M:%S) ---"
        for node_spec in "${MONITOR_NODES[@]}"; do
            NODE_NAME="${node_spec%%:*}"
            NODE_PORT="${node_spec##*:}"
            if container_running "$NODE_NAME" 2>/dev/null; then
                h=$(jsonrpc_get_height "$NODE_NAME" "$NODE_PORT" 2>/dev/null)
                h=$(echo "$h" | tr -dc '0-9')
                info "  $NODE_NAME height=${h:-?}"
            else
                warn "  $NODE_NAME not running"
            fi
        done
        tick=$((tick + 1))
        [ "$tick" -lt "$max_ticks" ] && sleep 60
    done
fi

# Reachable only when PIPELINE_EXIT_AFTER_SUCCESS=true (the monitor loop exits
# after one tick). The loop's final `[ "$tick" -lt "$max_ticks" ] && sleep` returns
# non-zero when tick reaches max_ticks, so end on an explicit success code.
exit 0
