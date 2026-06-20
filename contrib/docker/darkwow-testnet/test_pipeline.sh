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

# --- Library modules (order: dependencies before dependents) ---
source "$SCRIPT_DIR/lib/output.sh"
source "$SCRIPT_DIR/lib/traps.sh"
source "$SCRIPT_DIR/lib/config.sh"
source "$SCRIPT_DIR/lib/helpers.sh"
source "$SCRIPT_DIR/lib/phase_01_clean.sh"
source "$SCRIPT_DIR/lib/phase_02_build.sh"
source "$SCRIPT_DIR/lib/phase_03_prereqs.sh"
source "$SCRIPT_DIR/lib/phase_04_wallet.sh"
source "$SCRIPT_DIR/lib/phase_05_start.sh"
source "$SCRIPT_DIR/lib/phase_06_verify.sh"
source "$SCRIPT_DIR/lib/phase_07_rpc.sh"
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

# Phase 1: Clean
if [ "$RESUME_FROM" -le 1 ]; then
    phase_time_start; phase_clean;              phase_time_end "clean"
    phase_gate "clean"
fi

# Phase 2: Build
if [ "$RESUME_FROM" -le 2 ]; then
    phase_time_start; phase_build;              phase_time_end "build"
    phase_gate "build"
fi

# Phase 3: Validate prereqs
if [ "$RESUME_FROM" -le 3 ]; then
    phase_time_start; phase_prereqs;            phase_time_end "prereqs"
    phase_gate "prereqs"
fi

# Phase 4: Generate wallet
if [ "$RESUME_FROM" -le 4 ]; then
    phase_time_start; phase_wallet;             phase_time_end "wallet"
    phase_gate "wallet"
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
fi

# Phase 7: RPC health (local) / Seed Fallback (join)
if [ "$RESUME_FROM" -le 7 ]; then
    phase_time_start
    if is_join_mode; then
        phase_join_fallback; phase_time_end "join_fallback"
    else
        phase_rpc_health;    phase_time_end "rpc_health"
    fi
    phase_gate "rpc_or_fallback"
fi

# Phase 8: Mining activity (local) / P2P Connectivity (join)
if [ "$RESUME_FROM" -le 8 ]; then
    phase_time_start
    if is_join_mode; then
        phase_join_p2p;        phase_time_end "join_p2p"
    else
        phase_mining_activity; phase_time_end "mining_activity"
    fi
    phase_gate "mining_or_p2p"
fi

# Phase 9: Block production (local) / Blockchain Sync (join)
if [ "$RESUME_FROM" -le 9 ]; then
    phase_time_start
    if is_join_mode; then
        phase_join_sync; phase_time_end "join_sync"
    else
        phase_blocks;    phase_time_end "blocks"
    fi
    phase_gate "blocks_or_sync"
fi

# Wallet verification — only when --with-wallet is used.
if [ "${WITH_WALLET:-0}" -gt 0 ] && ! is_join_mode; then
    if [ "$RESUME_FROM" -le 10 ]; then
        phase_time_start; phase_wallet_verify;   phase_time_end "wallet_verify"
        phase_gate "wallet_verify"
    fi
    if [ "${WITH_WALLET:-0}" -ge 2 ]; then
        if [ "$RESUME_FROM" -le 11 ]; then
            phase_time_start; phase_wallet_transfer; phase_time_end "wallet_transfer"
            phase_gate "wallet_transfer"
        fi
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
    phase_gate "report_or_mining"
fi

if [ "$RESUME_FROM" -le 21 ]; then
    phase_time_start; phase_persistence;        phase_time_end "persistence"
    phase_gate "persistence"
fi

if is_join_mode; then
    report
fi

# Contract E2E tests
phase_time_start; phase_contract_tests;     phase_time_end "contract_tests"
