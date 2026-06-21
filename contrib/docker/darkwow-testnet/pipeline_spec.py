"""
DarkWow Testnet Pipeline — Architecture Specification
======================================================
This Python module IS the specification. Every lib/*.sh file, every function,
every global variable, and every data-flow dependency is declared here.

The bash implementation in `test_pipeline.sh` and `lib/*.sh` must match this
model exactly. If they diverge, this spec is the source of truth.

Run to validate:  python3 pipeline_spec.py
Expected output:  SPEC VALID: N functions across 18 modules
"""

from dataclasses import dataclass, field
from typing import List, Dict, Optional

# ============================================================================
# Global State Registry
# ============================================================================
# Every variable that flows between modules is declared here.
# Bash sourced files share scope — all of these are visible to all modules.

# ============================================================================
# WALLET CONFIG SPECIFICATION — CRITICAL
# ============================================================================
# The [net] section in dwow_wallet config causes the binary to hide all
# subcommands. Confirmed experimentally: with [net] present in config,
# 'dwow_wallet wallet initialize' fails with:
#   error: Found argument 'wallet' which wasn't expected, or isn't valid
#   USAGE: dwow_wallet [OPTIONS]
# The binary shows NO <SUBCOMMAND> in usage. This is a binary behavior —
# not a bug in the pipeline, not a config format issue. The binary's CLI
# parser changes based on config content when [net] is present.
#
# THEREFORE: Every config file that touches the wallet binary must NOT
# contain [net] during init operations (initialize, keygen, import-secrets,
# address). The [net] section is only safe to add AFTER all local operations
# complete, immediately before P2P operations (sync, scan, broadcast).
#
# TWO-PHASE CONFIG PATTERN (used by both DWW() and entrypoint-wallet.sh):
#
# Phase 1 (init, no [net]):
#   Config contains: network, [network_config."name"] with chain_path,
#   cache_path, wallet_path, wallet_pass, production, history_path.
#   NO [net] section. Binary can use all local subcommands.
#
# Phase 2 (runtime, with [net]):
#   [net] section APPENDED after init completes. Seeds, inbound, localnet,
#   p2p_local, mining_easy, active_profiles, outbound_connections,
#   inbound_connections, magic_bytes. Binary now has P2P config for
#   sync, scan, broadcast — but subcommands are hidden for future
#   invocations. Since init is already done, this is acceptable.
#
# DWW() (lib/config.sh):
#   - Generates config via heredoc on HOST (mktemp)
#   - Mounts at binary's DEFAULT path: /root/.config/dwow/dww_config.toml:ro
#   - No -c flag (binary reads default path)
#   - Config is Phase 1 only (no [net]) — DWW() only does local operations
#   - Uses volume wallet_data_pipeline for data
#
# entrypoint-wallet.sh:
#   - Writes config at container start to CONFIGDIR/dww_config.toml
#   - CONFIGDIR=/root/.config/dwow (binary's default path)
#   - Phase 1 config (no [net]) for init/keygen/import/address
#   - APPENDS Phase 2 config ([net]) after all local operations complete
#   - All wallet() calls use default path (no -c flag)
#   - On container restart: config regenerated without [net], init skipped
#     (wallet.db exists), [net] appended unconditionally
#
# wallet-shell.sh wal():
#   - docker exec into running container
#   - Binary reads default path (no -c flag)
#   - Config already has [net] appended by entrypoint
#   - P2P operations (sync, scan, broadcast) work correctly
#
# CONFIG PATH: Always the binary's default. Never use -c flag.
#              Always /root/.config/dwow/dww_config.toml.
# ============================================================================

GLOBALS: Dict[str, dict] = {
    # --- Counters (output.sh) ---
    "PASS":  {"value": 0,  "module": "output.sh"},
    "FAIL":  {"value": 0,  "module": "output.sh"},

    # --- Pipeline config (config.sh) ---
    "MODE":          {"value": "native", "module": "config.sh"},
    "BUILD_COMMIT":  {"value": "<git rev-parse HEAD>", "module": "config.sh"},
    "NO_CACHE":      {"value": False, "module": "config.sh"},
    "FRESH":         {"value": False, "module": "config.sh"},
    "SKIP_BUILD":    {"value": False, "module": "config.sh"},
    "REBUILD_BASE":  {"value": False, "module": "config.sh"},
    "RESUME_FROM":   {"value": 0,     "module": "config.sh"},
    "WITH_WALLET":   {"value": 0,     "module": "config.sh"},
    "CONTRACT_TIER": {"value": 0,     "module": "config.sh"},
    "NATIVE_NODES":  {"value": "2",   "module": "config.sh"},

    # --- Finality config ---
    "FINALITY_MODE":             {"value": "always", "module": "config.sh"},
    "FINALITY_CARIBINA_ENABLED": {"value": "false",  "module": "config.sh"},
    "FINALITY_ENABLE_MONERO":    {"value": "false",  "module": "config.sh"},
    "MONERO_MIN_CONFIRMATIONS":  {"value": "3",      "module": "config.sh"},
    "MONEROD_RPC_URL":           {"value": "",       "module": "config.sh"},

    # --- Network / compose ---
    "NETWORK":              {"value": "darkwow-testnet", "module": "config.sh"},
    "NODE0":                {"value": "dwow-node0",      "module": "config.sh"},
    "COMPOSE_FILE":         {"value": "<SCRIPT_DIR>/docker-compose.yml", "module": "config.sh"},
    "COMPOSE_PROJECT_NAME": {"value": "darkwow-testnet", "module": "config.sh"},
    "P2P_PORT":             {"value": 31342, "module": "config.sh"},
    "RPC_PORT":             {"value": 31345, "module": "config.sh"},
    "STRATUM_PORT":         {"value": 31347, "module": "config.sh"},
    "MM_RPC_PORT":          {"value": 31348, "module": "config.sh"},
    "FALLBACK_SEED_PORT":   {"value": "31341",          "module": "config.sh"},
    "CONTAINER_NAME":       {"value": "dwow-test-node", "module": "config.sh"},
    "FALLBACK_LILITH_NAME": {"value": "dwow-fallback-lilith", "module": "config.sh"},

    # --- Join-mode paths ---
    "JOIN_TEST_DATA":    {"value": "<pwd>/test-data",          "module": "config.sh"},
    "JOIN_TEST_MONERO":  {"value": "<pwd>/test-monero-data",   "module": "config.sh"},
    "JOIN_TEST_P2POOL":  {"value": "<pwd>/test-p2pool-data",   "module": "config.sh"},
    "JOIN_TEST_FALLBACK":{"value": "<pwd>/test-fallback-data", "module": "config.sh"},
    "JOIN_TEST_PERSIST": {"value": "<pwd>/test-persist-data",  "module": "config.sh"},
    "MONERO_WALLET_ADDRESS": {"value": "", "module": "config.sh"},

    # --- Bridge constants ---
    "BRIDGE_CONTAINER":          {"value": "dwow-bridge-node", "module": "config.sh"},
    "BRIDGE_TEST_HELPER":        {"value": "<REPO_ROOT>/target/release/bridge_test_helper",  "module": "config.sh"},
    "BRIDGE_TEST_HELPER_DEBUG":  {"value": "<REPO_ROOT>/target/debug/bridge_test_helper",    "module": "config.sh"},
    "WASM_BRIDGE":             {"value": "<REPO_ROOT>/src/contract/bridge/...",              "module": "config.sh"},
    "WASM_RELAYER_ENDOWMENT":  {"value": "<REPO_ROOT>/src/contract/relayer_endowment/...",   "module": "config.sh"},
    "WASM_DEPLOOOOR":          {"value": "<REPO_ROOT>/src/contract/deployooor/...",          "module": "config.sh"},

    # --- Internal ---
    "LOGFILE":             {"value": "/tmp/pipeline-...", "module": "config.sh"},
    "_CHECK_IMAGE_FAILED": {"value": 0,                 "module": "helpers.sh"},
    "_PHASE_FAIL_BEFORE":  {"value": 0,                 "module": "main"},
    "PHASE_START_TIME":    {"value": 0,                 "module": "main"},

    # --- Phase-output state (set by one phase, consumed by another) ---
    "BRIDGE_HELPER":       {"value": "", "set_by": "phase_prereqs",       "consumed_by": "bridge phases"},
    "WALLET_SECRET_1":     {"value": "", "set_by": "phase_wallet",        "consumed_by": "wallet_tests"},
    "WALLET_ADDRESS_1":    {"value": "", "set_by": "phase_wallet",        "consumed_by": "wallet_tests, start"},
    "WALLET_SECRET_2":     {"value": "", "set_by": "phase_wallet",        "consumed_by": "wallet_tests"},
    "WALLET_ADDRESS_2":    {"value": "", "set_by": "phase_wallet",        "consumed_by": "wallet_tests"},
    "WALLET_ADDRESS":      {"value": "", "set_by": "phase_wallet",        "consumed_by": "start"},
    "FORWARD_DESTINATION": {"value": "", "set_by": "phase_wallet",        "consumed_by": "wallet_tests"},
    "BRIDGE_ID":           {"value": "", "set_by": "bridge_deploy",       "consumed_by": "bridge phases"},
    "ENDOWMENT_ID":        {"value": "", "set_by": "bridge_deploy",       "consumed_by": "bridge phases"},
    "RELAYER_PUB":         {"value": "", "set_by": "bridge_deploy",       "consumed_by": "bridge phases"},
    "RELAYER_SECRET":      {"value": "", "set_by": "bridge_deploy",       "consumed_by": "bridge phases"},
    "DEPOSIT_COMMITMENT":  {"value": "", "set_by": "bridge_deposit",      "consumed_by": "bridge phases"},
    "WITHDRAW_NULLIFIER":  {"value": "", "set_by": "bridge_withdraw",     "consumed_by": "bridge phases"},
}


# ============================================================================
# Function Registry
# ============================================================================

@dataclass
class Function:
    """One bash function."""
    name: str
    module: str
    desc: str = ""
    reads: List[str] = field(default_factory=list)
    writes: List[str] = field(default_factory=list)
    calls: List[str] = field(default_factory=list)
    sources: List[str] = field(default_factory=list)  # sourced files
    lines: int = 0


# ============================================================================
# Module Registry
# ============================================================================

@dataclass
class Module:
    """One lib/*.sh file."""
    name: str
    filepath: str
    status: str = "todo"  # done | in_progress | todo
    desc: str = ""
    depends_on: List[str] = field(default_factory=list)
    functions: List[Function] = field(default_factory=list)
    global_defs: List[str] = field(default_factory=list)
    top_level: List[str] = field(default_factory=list)
    lines: int = 0


# ============================================================================
# MODULE DEFINITIONS — 18 files
# ============================================================================

# 0. output.sh [DONE]
module_output = Module(
    name="output", filepath="lib/output.sh", status="done",
    desc="Display functions and counters. Zero pipeline dependencies.",
    global_defs=["PASS", "FAIL"],
    functions=[
        Function("info",  "output", desc="echo [INFO] in green", lines=1),
        Function("warn",  "output", desc="echo [WARN] in yellow", lines=1),
        Function("error", "output", desc="echo [ERROR] in red, then exit 1", lines=1),
        Function("pass",  "output", desc="echo [PASS], increment PASS", writes=["PASS"], lines=1),
        Function("fail",  "output", desc="echo [FAIL], increment FAIL", writes=["FAIL"], lines=1),
        Function("check", "output", desc="pass() if arg1==0 else fail()", calls=["pass", "fail"], lines=5),
    ],
)

# 1. traps.sh [DONE]
module_traps = Module(
    name="traps", filepath="lib/traps.sh", status="done",
    desc="Error handling and cleanup. EXIT trap tears down Docker resources.",
    depends_on=["output"],
    functions=[
        Function("cleanup_on_exit", "traps",
                 desc="Stop/rm dwow containers, compose down all profiles, rm temp files",
                 reads=["COMPOSE_FILE"], lines=16),
    ],
    top_level=[
        "set -eE -o pipefail",
        "trap ERR: echo fatal + exit $rc",
        "trap INT/TERM/HUP/PIPE: echo signal + exit 1",
        "trap EXIT: cleanup_on_exit",
    ],
    lines=40,
)

# 2. config.sh
module_config = Module(
    name="config", filepath="lib/config.sh",
    desc="All configuration: flag parsing, validation, constants, DWW wrapper.",
    depends_on=["output"],
    global_defs=[
        "SCRIPT_DIR", "REPO_ROOT",
        "MODE", "BUILD_COMMIT", "NO_CACHE", "FRESH", "SKIP_BUILD", "REBUILD_BASE",
        "RESUME_FROM", "WITH_WALLET", "CONTRACT_TIER", "NATIVE_NODES",
        "FINALITY_MODE", "FINALITY_CARIBINA_ENABLED", "FINALITY_ENABLE_MONERO",
        "MONERO_MIN_CONFIRMATIONS", "MONEROD_RPC_URL",
        "NETWORK", "NODE0", "COMPOSE_FILE", "COMPOSE_PROJECT_NAME",
        "P2P_PORT", "RPC_PORT", "STRATUM_PORT", "MM_RPC_PORT",
        "FALLBACK_SEED_PORT", "CONTAINER_NAME", "FALLBACK_LILITH_NAME",
        "JOIN_TEST_DATA", "JOIN_TEST_MONERO", "JOIN_TEST_P2POOL",
        "JOIN_TEST_FALLBACK", "JOIN_TEST_PERSIST",
        "BRIDGE_CONTAINER", "BRIDGE_TEST_HELPER", "BRIDGE_TEST_HELPER_DEBUG",
        "WASM_BRIDGE", "WASM_RELAYER_ENDOWMENT", "WASM_DEPLOOOOR",
        "LOGFILE",
    ],
    functions=[
        Function("usage", "config", desc="Print --help text and exit 0. Pure display.", lines=109),
        Function("is_wallet_mode", "config", desc="test $MODE == 'wallet'", reads=["MODE"], lines=1),
        Function("DWW", "config",
                 desc="Run dwow_wallet in Docker. Generates config WITHOUT [net] "
                      "(Phase 1 only — [net] hides subcommands). Mounts at default "
                      "path /root/.config/dwow/dww_config.toml:ro. No -c flag. "
                      "Args passed through.",
                 reads=["wallet_data_pipeline"],
                 calls=["error"], lines=25),
    ],
    top_level=[
        "SCRIPT_DIR / REPO_ROOT assignment",
        "Default value assignment (MODE=, NO_CACHE=, BUILD_COMMIT=, etc.)",
        "while loop: flag parsing (--mode, --nodes, --no-cache, ...)",
        "Post-parse: mutual exclusivity (--fresh vs --skip-build)",
        "Post-parse: MODE validation",
        "Post-parse: NATIVE_NODES validation (1|2|5)",
        "Post-parse: WITH_WALLET range (0-5)",
        "Post-parse: CONTRACT_TIER range (0-4)",
        "Merge-mode defaults: FINALITY_ENABLE_MONERO, MONEROD_RPC_URL",
        "Network/compose constant assignment",
        "LOGFILE + exec > >(tee ...) 2>&1",
        "Join test data paths",
        "Bridge constants",
    ],
    lines=200,
)

# 3. helpers.sh
module_helpers = Module(
    name="helpers", filepath="lib/helpers.sh",
    desc="Shared utilities used by multiple phase modules.",
    depends_on=["output", "config"],
    global_defs=["_CHECK_IMAGE_FAILED"],
    functions=[
        Function("clean_data_dir", "helpers",
                 desc="rm -rf directories, fallback to sudo for root-owned files",
                 calls=["warn"], lines=8),
        Function("is_join_mode", "helpers",
                 desc="test $MODE in ('join-native', 'join-merge')",
                 reads=["MODE"], lines=3),
        Function("is_bridge_mode", "helpers",
                 desc="test $MODE == 'bridge'",
                 reads=["MODE"], lines=3),
        Function("check_image", "helpers",
                 desc="docker image inspect $IMAGE. First-failure latch via _CHECK_IMAGE_FAILED.",
                 reads=["_CHECK_IMAGE_FAILED", "IMAGE"],
                 writes=["_CHECK_IMAGE_FAILED"], calls=["fail"], lines=11),
        Function("check_network", "helpers",
                 desc="curl https://api.ipify.org with 5s timeout",
                 calls=["fail"], lines=7),
        Function("jsonrpc", "helpers",
                 desc="JSON-RPC over /dev/tcp via docker exec",
                 reads=["CONTAINER_NAME"], lines=28),
        Function("report", "helpers",
                 desc="Print pass/fail summary. Exit 1 if FAIL > 0. Mode-specific teardown instructions.",
                 reads=["MODE", "PASS", "FAIL", "BRIDGE_CONTAINER", "CONTAINER_NAME", "COMPOSE_FILE"],
                 calls=["is_join_mode", "is_bridge_mode"], lines=49),
    ],
    lines=200,
)

# 4-18. Phase modules — one file per dispatch phase pair
# Each file contains the local-devnet and join-mode variants for one phase number.

phase_modules: Dict[str, Module] = {
    "phase_01_clean": Module(
        name="phase_clean", filepath="lib/phase_01_clean.sh",
        desc="Phase 1: Tear down previous run — containers, volumes, temp files.",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_clean", "phase_clean",
                     desc="Phase 1: Clean",
                     reads=["REPO_ROOT", "FRESH", "JOIN_TEST_DATA", "JOIN_TEST_MONERO",
                            "JOIN_TEST_P2POOL", "JOIN_TEST_FALLBACK", "JOIN_TEST_PERSIST",
                            "COMPOSE_FILE", "CONTAINER_NAME", "FALLBACK_LILITH_NAME"],
                     calls=["is_join_mode", "clean_data_dir", "info", "pass", "fail", "warn", "error"],
                     lines=112),
        ],
        lines=112,
    ),
    "phase_02_build": Module(
        name="phase_build", filepath="lib/phase_02_build.sh",
        desc="Phase 2: Build Docker images from origin — base, testnet, wallet.",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_build", "phase_build",
                     desc="Phase 2: Build",
                     reads=["REPO_ROOT", "SKIP_BUILD", "WITH_WALLET", "BUILD_COMMIT",
                            "REBUILD_BASE", "NO_CACHE", "MODE", "SCRIPT_DIR", "COMPOSE_FILE"],
                     calls=["is_join_mode", "info", "pass", "fail", "error", "check"],
                     lines=101),
        ],
        lines=101,
    ),
    "phase_03_prereqs": Module(
        name="phase_prereqs", filepath="lib/phase_03_prereqs.sh",
        desc="Phase 3: Validate binaries, WASM files, bridge helper exist in images.",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_prereqs", "phase_prereqs",
                     desc="Phase 3: Validate prerequisites",
                     reads=["SCRIPT_DIR", "MODE", "BRIDGE_TEST_HELPER", "BRIDGE_TEST_HELPER_DEBUG",
                            "WASM_BRIDGE", "WASM_RELAYER_ENDOWMENT", "WASM_DEPLOOOOR"],
                     writes=["BRIDGE_HELPER"],
                     calls=["is_join_mode", "is_bridge_mode", "DWW", "info", "pass", "fail", "warn", "error"],
                     lines=68),
        ],
        lines=68,
    ),
    "phase_04_wallet": Module(
        name="phase_wallet", filepath="lib/phase_04_wallet.sh",
        desc="Phase 4: Generate wallet keypairs, set FORWARD_DESTINATION.",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_wallet", "phase_wallet",
                     desc="Phase 4: Generate wallet(s)",
                     reads=["WITH_WALLET", "FORWARD_DESTINATION"],
                     writes=["WALLET_SECRET_1", "WALLET_SECRET_2", "WALLET_ADDRESS_1",
                             "WALLET_ADDRESS_2", "WALLET_ADDRESS", "FORWARD_DESTINATION"],
                     calls=["DWW", "info", "pass", "fail", "error"],
                     lines=58),
        ],
        lines=58,
    ),
    "phase_05_start": Module(
        name="phase_start", filepath="lib/phase_05_start.sh",
        desc="Phase 5: Start containers (local) or static config (join).",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_start", "phase_start",
                     desc="Phase 5 local: docker compose up native profile, wallet containers",
                     reads=["MODE", "WALLET_ADDRESS", "FINALITY_MODE", "FINALITY_CARIBINA_ENABLED",
                            "FINALITY_ENABLE_MONERO", "MONERO_MIN_CONFIRMATIONS", "MONEROD_RPC_URL",
                            "NATIVE_NODES", "WITH_WALLET", "COMPOSE_PROJECT_NAME", "COMPOSE_FILE"],
                     writes=["MONERO_DATA_DIR", "MONERO_OFFLINE",
                             "MONERO_FIXED_DIFFICULTY"],
                     calls=["info", "pass", "fail", "warn", "error", "check"],
                     lines=164),
            Function("phase_join_config", "phase_start",
                     desc="Phase 5 join: generate dwowd config, create data dirs",
                     reads=["JOIN_TEST_DATA", "CONTAINER_NAME", "NETWORK", "P2P_PORT", "RPC_PORT",
                            "STRATUM_PORT", "SEED_ADDR", "MAGIC_BYTES", "FINALITY_MODE",
                            "FINALITY_CARIBINA_ENABLED", "IMAGE"],
                     calls=["check_image", "pass", "fail", "info"],
                     lines=162),
        ],
        lines=326,
    ),
    "phase_06_verify": Module(
        name="phase_verify", filepath="lib/phase_06_verify.sh",
        desc="Phase 6: Verify containers running (local) or container lifecycle (join).",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_verify", "phase_verify",
                     desc="Phase 6 local: docker ps check all expected containers",
                     reads=["MODE", "NATIVE_NODES", "WITH_WALLET"],
                     calls=["pass", "fail"], lines=32),
            Function("phase_join_lifecycle", "phase_verify",
                     desc="Phase 6 join: docker run container, wait for start, handle restart",
                     reads=["JOIN_TEST_DATA", "CONTAINER_NAME", "NETWORK", "P2P_PORT", "RPC_PORT",
                            "STRATUM_PORT", "SEED_ADDR", "MAGIC_BYTES", "FINALITY_MODE",
                            "FINALITY_CARIBINA_ENABLED", "IMAGE"],
                     calls=["check_image", "clean_data_dir", "pass", "fail", "warn", "info"],
                     lines=87),
        ],
        lines=119,
    ),
    "phase_07_rpc": Module(
        name="phase_rpc", filepath="lib/phase_07_rpc.sh",
        desc="Phase 7: RPC health check (local) or seed fallback (join).",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_rpc_health", "phase_rpc",
                     desc="Phase 7 local: JSON-RPC ping to node0",
                     reads=["NODE0", "MODE"],
                     calls=["info", "pass", "fail", "error"], lines=57),
            Function("phase_join_fallback", "phase_rpc",
                     desc="Phase 7 join: deploy fallback lilith, verify seed peer connectivity",
                     reads=["JOIN_TEST_DATA", "JOIN_TEST_FALLBACK", "CONTAINER_NAME",
                            "FALLBACK_LILITH_NAME", "NETWORK", "P2P_PORT", "RPC_PORT",
                            "STRATUM_PORT", "SEED_ADDR", "MAGIC_BYTES", "FINALITY_MODE",
                            "FINALITY_CARIBINA_ENABLED", "FALLBACK_SEED_PORT", "IMAGE"],
                     calls=["check_image", "clean_data_dir", "jsonrpc", "pass", "fail", "info"],
                     lines=199),
        ],
        lines=256,
    ),
    "phase_08_mining": Module(
        name="phase_mining", filepath="lib/phase_08_mining.sh",
        desc="Phase 8: Verify mining activity (local) or P2P connectivity (join).",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_mining_activity", "phase_mining",
                     desc="Phase 8 local: check stratum, monerod (merge), p2pool readiness",
                     reads=["MODE", "NODE0"],
                     calls=["info", "pass", "fail", "warn"], lines=134),
            Function("phase_join_p2p", "phase_mining",
                     desc="Phase 8 join: verify P2P connections, slot count via JSON-RPC",
                     reads=["CONTAINER_NAME", "RPC_PORT"],
                     calls=["check_network", "jsonrpc", "pass", "fail"], lines=52),
        ],
        lines=186,
    ),
    "phase_09_blocks": Module(
        name="phase_blocks", filepath="lib/phase_09_blocks.sh",
        desc="Phase 9: Verify block production (local) or blockchain sync (join).",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_blocks", "phase_blocks",
                     desc="Phase 9 local: verify genesis block, height increment, anchor validation",
                     reads=["MODE", "NODE0", "NATIVE_NODES", "FINALITY_CARIBINA_ENABLED",
                            "FINALITY_ENABLE_MONERO"],
                     calls=["info", "pass", "fail", "warn"], lines=244),
            Function("phase_join_sync", "phase_blocks",
                     desc="Phase 9 join: verify blockchain sync by checking height advances",
                     reads=["CONTAINER_NAME", "RPC_PORT"],
                     calls=["jsonrpc", "pass", "fail"], lines=35),
        ],
        lines=279,
    ),
    "phase_10_wallet_tests": Module(
        name="phase_wallet_tests", filepath="lib/phase_10_wallet_tests.sh",
        desc="Phases 10-11: Wallet sync/scan/balance and wallet-to-wallet transfer.",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_wallet_verify", "phase_wallet_tests",
                     desc="Phase 10: Sync, scan, check balance, address match",
                     reads=["WITH_WALLET", "FORWARD_DESTINATION", "SCRIPT_DIR"],
                     calls=["info", "pass", "fail", "warn"],
                     sources=["wallet-shell.sh"], lines=142),
            Function("phase_wallet_transfer", "phase_wallet_tests",
                     desc="Phase 11: wallet-1 sends to wallet-2, verify receiving address",
                     reads=["SCRIPT_DIR"],
                     calls=["info", "pass", "fail"],
                     sources=["wallet-shell.sh"], lines=57),
        ],
        lines=199,
    ),
    "phase_12_bridge": Module(
        name="phase_bridge", filepath="lib/phase_12_bridge.sh",
        desc="Phases 12-19: Full bridge lifecycle — deploy, init, register, deposit, "
             "withdraw, accept, execute, verify.",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_bridge_deploy", "phase_bridge",
                     desc="Deploy bridge + relayer_endowment contracts, capture IDs and relayer keypair",
                     reads=["BRIDGE_HELPER", "WASM_BRIDGE", "WASM_RELAYER_ENDOWMENT", "WASM_DEPLOOOOR"],
                     writes=["BRIDGE_DEPLOY_OUTPUT", "BRIDGE_ID", "ENDOWMENT_ID",
                             "RELAYER_KEYPAIR", "RELAYER_PUB", "RELAYER_SECRET"],
                     calls=["info", "pass", "fail"], lines=43),
            Function("phase_bridge_init", "phase_bridge",
                     desc="Initialize bridge and endowment contracts with relayer public key",
                     reads=["BRIDGE_HELPER", "RELAYER_PUB"],
                     calls=["info", "check"], lines=17),
            Function("phase_bridge_register_relayer", "phase_bridge",
                     desc="Register relayer with bridge contract",
                     reads=["BRIDGE_HELPER", "RELAYER_PUB"],
                     calls=["check", "pass"], lines=10),
            Function("phase_bridge_deposit", "phase_bridge",
                     desc="Simulate deposit with ZK proof, capture deposit commitment",
                     reads=["BRIDGE_HELPER", "RELAYER_PUB"],
                     writes=["DEPOSIT_SECRET", "DEPOSIT_AMOUNT", "DEPOSIT_RECIPIENT",
                             "DEPOSIT_OUTPUT", "DEPOSIT_COMMITMENT"],
                     calls=["info", "pass", "fail"], lines=32),
            Function("phase_bridge_withdraw", "phase_bridge",
                     desc="Create withdrawal with ZK proof, capture nullifier",
                     reads=["BRIDGE_HELPER"],
                     writes=["WITHDRAW_SECRET", "WITHDRAW_AMOUNT", "WITHDRAW_OUTPUT",
                             "WITHDRAW_NULLIFIER"],
                     calls=["info", "pass", "fail"], lines=28),
            Function("phase_bridge_accept", "phase_bridge",
                     desc="Accept withdrawal as relayer",
                     reads=["BRIDGE_HELPER", "WITHDRAW_NULLIFIER", "RELAYER_PUB"],
                     calls=["check", "pass"], lines=13),
            Function("phase_bridge_execute", "phase_bridge",
                     desc="Execute guaranteed withdrawal",
                     reads=["BRIDGE_HELPER", "WITHDRAW_NULLIFIER"],
                     calls=["check", "pass"], lines=11),
            Function("phase_bridge_verify", "phase_bridge",
                     desc="Verify bridge-node health and event logs",
                     reads=["BRIDGE_CONTAINER", "NODE0"],
                     calls=["info", "pass", "fail"], lines=38),
        ],
        lines=192,
    ),
    "phase_20_report": Module(
        name="phase_report", filepath="lib/phase_20_report.sh",
        desc="Phase 20: Mining verification (join) or final report (local/bridge).",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_join_mining", "phase_report",
                     desc="Dispatcher: routes to phase_join_native_mining or phase_join_merge_mining",
                     reads=["MODE"],
                     calls=["phase_join_merge_mining", "phase_join_native_mining"], lines=10),
            Function("phase_join_native_mining", "phase_report",
                     desc="Join native: verify mining start via RPC, validate block production",
                     reads=["CONTAINER_NAME", "RPC_PORT", "JOIN_TEST_DATA"],
                     calls=["check_image", "jsonrpc", "clean_data_dir", "pass", "fail", "info"],
                     lines=56),
            Function("phase_join_merge_mining", "phase_report",
                     desc="Join merge: docker compose up monerod + p2pool + xmrig, verify merge mining",
                     reads=["COMPOSE_FILE", "JOIN_TEST_DATA", "JOIN_TEST_MONERO", "JOIN_TEST_P2POOL",
                            "CONTAINER_NAME", "RPC_PORT", "NETWORK", "P2P_PORT", "STRATUM_PORT",
                            "MM_RPC_PORT", "SEED_ADDR", "MAGIC_BYTES", "MONERO_OFFLINE",
                            "MONERO_FIXED_DIFFICULTY", "WALLET_ADDRESS", "MONERO_WALLET_ADDRESS",
                            "FINALITY_ENABLE_MONERO", "MONERO_MIN_CONFIRMATIONS",
                            "MONEROD_RPC_URL", "REPO_ROOT"],
                     calls=["check_image", "check_network", "clean_data_dir", "info", "pass", "fail"],
                     lines=122),
        ],
        lines=188,
    ),
    "phase_21_persistence": Module(
        name="phase_persistence", filepath="lib/phase_21_persistence.sh",
        desc="Phase 21: Data persistence test — stop/start cycle, verify state survives.",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_persistence", "phase_persistence",
                     desc="Phase 21: Stop container, restart, verify persisted chain data",
                     reads=["JOIN_TEST_PERSIST", "CONTAINER_NAME", "NETWORK", "P2P_PORT",
                            "RPC_PORT", "STRATUM_PORT", "SEED_ADDR", "MAGIC_BYTES",
                            "FINALITY_MODE", "FINALITY_CARIBINA_ENABLED", "IMAGE"],
                     calls=["is_join_mode", "check_image", "clean_data_dir", "pass", "fail", "info"],
                     lines=103),
        ],
        lines=103,
    ),
    "phase_99_contract_tests": Module(
        name="phase_contract_tests", filepath="lib/phase_99_contract_tests.sh",
        desc="Post-pipeline: Contract E2E tests via test-contracts.sh.",
        depends_on=["output", "config", "helpers"],
        functions=[
            Function("phase_contract_tests", "phase_contract_tests",
                     desc="Run test-contracts.sh if CONTRACT_TIER > 0, skip for join modes",
                     reads=["CONTRACT_TIER", "MODE", "SCRIPT_DIR"],
                     calls=["is_join_mode", "info", "fail", "check"], lines=26),
        ],
        lines=26,
    ),
}

# ============================================================================
# SOURCING ORDER — must be exactly this in test_pipeline.sh
# ============================================================================

SOURCING_ORDER = [
    "lib/output.sh",
    "lib/traps.sh",
    "lib/config.sh",
    "lib/helpers.sh",
    "lib/phase_01_clean.sh",
    "lib/phase_02_build.sh",
    "lib/phase_03_prereqs.sh",
    "lib/phase_04_wallet.sh",
    "lib/phase_05_start.sh",
    "lib/phase_06_verify.sh",
    "lib/phase_07_rpc.sh",
    "lib/phase_08_mining.sh",
    "lib/phase_09_blocks.sh",
    "lib/phase_10_wallet_tests.sh",
    "lib/phase_12_bridge.sh",
    "lib/phase_20_report.sh",
    "lib/phase_21_persistence.sh",
    "lib/phase_99_contract_tests.sh",
]

# ============================================================================
# MAIN SCRIPT — thin orchestrator, ~3 functions, ~200 lines
# ============================================================================

MAIN_FUNCTIONS = [
    Function("phase_time_start", "main",
             desc="Record start time: PHASE_START_TIME=$SECONDS",
             writes=["PHASE_START_TIME"], lines=1),
    Function("phase_time_end", "main",
             desc="Print elapsed: info 'Phase X completed in Ns'",
             reads=["PHASE_START_TIME"], calls=["info"], lines=5),
    Function("phase_gate", "main",
             desc="Compare FAIL vs _PHASE_FAIL_BEFORE; exit 1 if new failures",
             reads=["FAIL", "_PHASE_FAIL_BEFORE"],
             writes=["_PHASE_FAIL_BEFORE"], calls=["error"], lines=11),
]


# ============================================================================
# VALIDATION
# ============================================================================

def validate_spec() -> bool:
    """Verify no duplicate functions, no missing call targets, valid sourcing order."""
    all_functions: Dict[str, str] = {}  # name -> module
    all_modules: Dict[str, Module] = {}
    errors: List[str] = []

    # Collect all modules
    all_modules["output"] = module_output
    all_modules["traps"] = module_traps
    all_modules["config"] = module_config
    all_modules["helpers"] = module_helpers
    for key, mod in phase_modules.items():
        all_modules[key] = mod

    # Add main functions
    for fn in MAIN_FUNCTIONS:
        if fn.name in all_functions:
            errors.append(f"DUP: {fn.name} in main and {all_functions[fn.name]}")
        all_functions[fn.name] = "main"

    # Collect all module functions
    for mod_name, mod in all_modules.items():
        for fn in mod.functions:
            if fn.name in all_functions:
                errors.append(f"DUP: {fn.name} in {mod_name} and {all_functions[fn.name]}")
            all_functions[fn.name] = mod_name

    # Verify all declared calls resolve to known functions
    all_mod_list = [module_output, module_traps, module_config, module_helpers]
    all_mod_list.extend(phase_modules.values())
    for mod in all_mod_list:
        for fn in mod.functions:
            for called in fn.calls:
                if called not in all_functions:
                    errors.append(f"MISSING: {mod.name}::{fn.name} calls '{called}' — not found")

    # Verify sourcing order satisfies all depends_on
    sourced: set = set()
    for path in SOURCING_ORDER:
        # Convert "lib/foo_bar.sh" -> "foo_bar"
        name = path.replace("lib/", "").replace(".sh", "")
        sourced.add(name)

    for mod in all_mod_list:
        for dep in mod.depends_on:
            if dep not in sourced:
                errors.append(f"DEP: {mod.name} depends_on '{dep}' — not in SOURCING_ORDER")

    # Verify module names in phase_modules dict keys match module.name fields
    for key, mod in phase_modules.items():
        if key not in sourced:
            errors.append(f"MISSING from SOURCING_ORDER: {mod.filepath}")

    # Report
    if errors:
        print("SPEC ERRORS:")
        for e in errors:
            print(f"  - {e}")
        return False

    n_funcs = len(all_functions)
    n_mods = len(SOURCING_ORDER)
    n_phase_mods = len(phase_modules)
    print(f"SPEC VALID: {n_funcs} functions across {n_mods} modules ({n_phase_mods} phase modules)")
    return True


if __name__ == "__main__":
    validate_spec()
