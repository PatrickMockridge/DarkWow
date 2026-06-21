# DarkWow Testnet Pipeline — configuration
#
# All configuration: flag parsing, validation, constants, DWW wrapper,
# LOGFILE generation, and exec redirect.
#
# Sourced by test_pipeline.sh after output.sh and traps.sh.
# SCRIPT_DIR and REPO_ROOT must be set by the main script before sourcing.

# --- Help ---
usage() {
    cat <<'EOF'
DarkWow Testnet Full Pipeline

Usage:
  ./test_pipeline.sh --mode <mode>

Modes:
  native         6-node local devnet (seed + 5 mining full nodes, --nodes controls count)
  merge          5-node local devnet (seed + 3 full nodes with p2pool/xmrig sidecars + monerod)
  bridge         5-node local devnet (seed + 3 full nodes + bridge relay node)
  join-native    Single node joining public testnet, native mining
  join-merge     Single node joining public testnet, merge mining
  wallet         Build wallet Docker image + generate keypair, then exit

Phases (native, merge):
  1.  Clean                Tear down previous containers, images, volumes
  2.  Build                Build Docker images via compose
  3.  Validate prereqs     Check required files and wallet image exist
  4.  Generate wallet      Create DarkWow keypair via dwow_wallet
  5.  Start                Launch containers (6 native, 5 merge)
  6.  Verify containers    Check all expected containers are running
  7.  RPC health           Wait for JSON-RPC endpoints to respond
  8.  Mining activity      Verify in-container mining (RPC or xmrig sidecar)
  9.  Block production     Wait for blocks to be mined (no timeout — PoW pace)
  10. Wallet verify        Sync, scan, balance, address check (with --with-wallet)
  11. Wallet transfer      Wallet-to-wallet transfer test (with --with-wallet >= 2)
  12. Report               Print pass/fail summary

Phases (bridge — seed + 3 full nodes + bridge relay):
  1-9. Shared with native mode (clean through block production)
  10. Bridge Deploy        Deploy bridge + relayer_endowment contracts via RPC
  10b. Bridge Initialize    Init bridge + endowment accounts
  11. Register Relayer     Register test relayer with bridge contract
  12. Simulate Deposit     Generate ZK deposit proof, submit DepositV1
  13. Create Withdrawal    Generate ZK withdraw proof, submit WithdrawV1
  14. Accept Withdrawal    Relayer accepts pending withdrawal
  15. Execute Withdrawal   Execute guaranteed withdrawal
  16. Verify Bridge        Check container health, relayer logs, block height

Phases (join-native, join-merge):
  1.  Clean                Tear down previous join containers + fallback lilith
  2.  Build                Build Docker image via compose
  3.  Validate prereqs     Check join-testnet.sh and required files exist
  4.  Generate wallet      Create DarkWow keypair via dwow_wallet
  5.  Static config        Extract generated dwowd_config.toml and validate keys
  6.  Container lifecycle  Start container, verify startup log messages
  7.  Seed fallback        Test local lilith fallback when public seeds unreachable
  8.  P2P connectivity     Wait for peer connections via p2p.info
  9.  Blockchain sync      Wait for block_height > 0 via blockchain.get_height
  10. Mining verification  Wait for block production or merge stack health
  11. Persistence          Stop container, verify data survives, restart
  12. Report               Print pass/fail summary

Sequential determinism:
  Every phase runs to completion before the next begins. No background tasks,
  no parallel operations. One machine, one thing at a time. This guarantees
  reproducible results across different machines.

Environment:
  RAYON_NUM_THREADS         Cargo build parallelism (default: 10)
  FORWARD_DESTINATION       Redirect coinbase rewards to this address (wallet testing)
  NATIVE_NODES              Number of native mining nodes: 1, 2, or 5 (env form of --nodes)
  CONTRACT_TIER             Contract E2E test tier: 1-4 (env form of --contract-tier)
  MONERO_WALLET_ADDRESS     Monero testnet wallet for merge mining rewards
  MONERO_OFFLINE            Skip online bootstrap (default: true for devnet)
  MONERO_FIXED_DIFFICULTY   Fixed difficulty for offline mode (default: 1000)
  FINALITY_MODE             Finality enforcement mode: always (default), native, signaled
  FINALITY_CARIBINA_ENABLED   Set to "false" to disable Caribina Arweave anchoring (default: true)
  FINALITY_ENABLE_MONERO    Set "true" to enable Monero anchor verification
  MONERO_MIN_CONFIRMATIONS  Minimum Monero block confirmations (default: 3)
  MONEROD_RPC_URL           monerod JSON-RPC URL for anchor verification

Options:
  --nodes N                   Native mining nodes: 1, 2, or 5 (default: 2, native mode only)
  --rebuild-base              Force --no-cache rebuild of darkwow-base:24.04 image
  --skip-build                Skip Docker build phase — use cached images
  --resume-from N             Resume from phase N (skip phases 1 through N-1)
                                Safe from phase 6+ (verification only, no side effects)
  --no-cache                  Pass --no-cache to docker compose build
  --fresh                     Aggressive clean: system prune, image rm, volume prune
  --with-wallet N             Number of wallet containers (0-5, default: 0, recommended: 2)
  --contract-tier N           Run contract E2E tests after pipeline (1-4, default: 0 = skip)
  --finality-mode MODE        Finality mode: "always" (default), "native", or "signaled"
  --finality-disable-caribina Disable Caribina Arweave anchoring entirely
  --finality-enable-monero    Enable Monero p2pool anchor verification
  --monero-min-confirmations N  Monero minimum confirmations (default: 3)
  --monerod-rpc-url URL       monerod JSON-RPC URL for anchor verification

Examples:
  ./test_pipeline.sh                         # local devnet, native mining, 2 nodes
  ./test_pipeline.sh --nodes 5               # local devnet, 5-node native mining
  ./test_pipeline.sh --mode merge            # local devnet, merge mining
  ./test_pipeline.sh --mode bridge           # local devnet, full bridge lifecycle
  ./test_pipeline.sh --mode wallet           # build wallet image + keygen, then exit
  ./test_pipeline.sh --mode join-native      # join public testnet, solo mining
  ./test_pipeline.sh --mode join-merge       # join public testnet, merge mining
  ./test_pipeline.sh --with-wallet 2         # local devnet + 2 wallet containers for docker exec
  ./test_pipeline.sh --with-wallet 3 --contract-tier 2  # 3-wallet devnet + contract deploy + invocations
  ./test_pipeline.sh --skip-build            # skip Docker build, use cached images
  FORWARD_DESTINATION="<addr>" ./test_pipeline.sh  # mine coinbase rewards to external wallet

After pipeline passes:
  ./test-contracts.sh --mode native          # contract deploy + transfer test
  ./test-contracts.sh --mode merge           # merge mode contract test
  ./contract-tests/run-all.sh                # run all per-contract wallet tests
EOF
    exit 0
}

# --- Parse flags ---
MODE="native"
FINALITY_MODE="${FINALITY_MODE:-always}"
FINALITY_CARIBINA_ENABLED="${FINALITY_CARIBINA_ENABLED:-true}"
FINALITY_ENABLE_MONERO="${FINALITY_ENABLE_MONERO:-false}"
MONERO_MIN_CONFIRMATIONS="${MONERO_MIN_CONFIRMATIONS:-3}"
MONEROD_RPC_URL="${MONEROD_RPC_URL:-}"
NO_CACHE="${NO_CACHE:-false}"
BUILD_COMMIT="${BUILD_COMMIT:-$(git rev-parse HEAD)}"
REBUILD_BASE="${REBUILD_BASE:-false}"
FRESH="${FRESH:-false}"
SKIP_BUILD="${SKIP_BUILD:-false}"
BUILD_LOCAL="${BUILD_LOCAL:-false}"
RESUME_FROM="${RESUME_FROM:-0}"
STOP_AFTER="${STOP_AFTER:-99}"
WITH_WALLET="${WITH_WALLET:-0}"
CONTRACT_TIER="${CONTRACT_TIER:-0}"
while [ $# -gt 0 ]; do
    case "$1" in
        --mode) MODE="$2"; shift 2 ;;
        --mode=*) MODE="${1#*=}"; shift ;;
        --nodes) NATIVE_NODES="$2"; shift 2 ;;
        --nodes=*) NATIVE_NODES="${1#*=}"; shift ;;
        --finality-mode) FINALITY_MODE="$2"; shift 2 ;;
        --finality-mode=*) FINALITY_MODE="${1#*=}"; shift ;;
        --finality-disable-caribina) FINALITY_CARIBINA_ENABLED="false"; shift ;;
        --finality-enable-monero) FINALITY_ENABLE_MONERO="true"; shift ;;
        --monero-min-confirmations) MONERO_MIN_CONFIRMATIONS="$2"; shift 2 ;;
        --monero-min-confirmations=*) MONERO_MIN_CONFIRMATIONS="${1#*=}"; shift ;;
        --monerod-rpc-url) MONEROD_RPC_URL="$2"; shift 2 ;;
        --monerod-rpc-url=*) MONEROD_RPC_URL="${1#*=}"; shift ;;
        --no-cache) NO_CACHE="true"; shift ;;
        --rebuild-base) REBUILD_BASE="true"; shift ;;
        --fresh) FRESH="true"; NO_CACHE="true"; REBUILD_BASE="true"; shift ;;
        --skip-build) SKIP_BUILD="true"; shift ;;
        --build-local) BUILD_LOCAL="true"; shift ;;
        --resume-from) RESUME_FROM="$2"; shift 2 ;;
        --stop-after) STOP_AFTER="$2"; shift 2 ;;
        --with-wallet) WITH_WALLET="$2"; shift 2 ;;
        --contract-tier) CONTRACT_TIER="$2"; shift 2 ;;
        --help|-h) usage ;;
        *)
            echo "Unknown flag: $1"
            echo "Usage: $0 --mode native|merge|bridge|join-native|join-merge|wallet"
            echo "       $0 --help"
            exit 1 ;;
    esac
done

# Validate mutual exclusivity of --fresh and --skip-build.
# --fresh purges Docker images; --skip-build relies on cached images.
# Using both would destroy the images then fail to find them.
if [ "$FRESH" = "true" ] && [ "$SKIP_BUILD" = "true" ]; then
    echo "Error: --fresh and --skip-build are mutually exclusive."
    echo "  --fresh      purges cached images and rebuilds from scratch"
    echo "  --skip-build uses cached images without rebuilding"
    exit 1
fi

VALID_MODES="native merge bridge join-native join-merge wallet"
if ! echo "$VALID_MODES" | grep -qw "$MODE"; then
    echo "Invalid mode: $MODE"
    echo "Valid modes: $VALID_MODES"
    echo "Run '$0 --help' for full documentation."
    exit 1
fi

# Wallet-only mode: build wallet image, generate keypair, exit. No mining nodes.
is_wallet_mode() { [ "$MODE" = "wallet" ]; }

# Native mode node count: 1=solo, 2=dual (default), 5=consensus
NATIVE_NODES="${NATIVE_NODES:-2}"
if [ "$MODE" = "native" ]; then
    case "$NATIVE_NODES" in
        1|2|5) ;;
        *) echo "Invalid --nodes value: $NATIVE_NODES (valid: 1, 2, 5)"; exit 1 ;;
    esac
fi

# Validate wallet count
if ! [ "$WITH_WALLET" -ge 0 ] 2>/dev/null || ! [ "$WITH_WALLET" -le 5 ] 2>/dev/null; then
    echo "Invalid wallet count: $WITH_WALLET"
    echo "WITH_WALLET must be an integer between 0 and 5."
    exit 1
fi

# Validate contract tier
if ! [ "$CONTRACT_TIER" -ge 0 ] 2>/dev/null || ! [ "$CONTRACT_TIER" -le 4 ] 2>/dev/null; then
    echo "Invalid contract tier: $CONTRACT_TIER"
    echo "CONTRACT_TIER must be an integer between 0 and 4."
    exit 1
fi

# In merge mining modes, enable Monero finality by default since merge
# mining is the only source of Monero anchors in this pipeline.
if [ "$MODE" = "merge" ] || [ "$MODE" = "join-merge" ]; then
    FINALITY_ENABLE_MONERO="${FINALITY_ENABLE_MONERO:-true}"
    MONEROD_RPC_URL="${MONEROD_RPC_URL:-http://monerod:28081/json_rpc}"
fi

# Wallet binary via Docker. Image builds from origin (pipeline-determinism).
# No hardcoded config template — the binary is the source of truth for its
# own schema. Pre-create a minimal config (just the network name) so the
# binary doesn't hit the two-run bootstrap error (it writes defaults then
# returns ConfigInvalid on first run). The binary fills in all other fields
# from its embedded default template.
DWW() {
    if ! docker image inspect darkwow-wallet:latest >/dev/null 2>&1; then
        error "darkwow-wallet:latest not found — phase_build must run before DWW()"
    fi
    # Pre-create minimal config — network name only. Binary owns everything else.
    docker run --rm \
        --entrypoint /bin/sh \
        -v wallet_data_pipeline:/root/.local/share/dwow/dww \
        darkwow-wallet:latest \
        -c "mkdir -p /root/.config/dwow && printf 'network = \"darkwow-testnet\"\n' > /root/.config/dwow/dww_config.toml" 2>&1
    docker run --rm \
        --entrypoint /app/dwow_wallet \
        -v wallet_data_pipeline:/root/.local/share/dwow/dww \
        -e RAYON_NUM_THREADS=2 \
        darkwow-wallet:latest -n darkwow-testnet "$@"
}

NETWORK="darkwow-testnet"
NODE0="dwow-node0"
IMAGE="darkwow-testnet-lilith:latest"

# Public testnet constants (join modes)
MAGIC_BYTES="${MAGIC_BYTES:-68,82,75,87}"
SEED_ADDR="${SEED_ADDR:-lilith0.dark.fi:31340,lilith1.dark.fi:31340}"
P2P_PORT=31342
RPC_PORT=31345
STRATUM_PORT=31347
MM_RPC_PORT=31348
FALLBACK_SEED_PORT="${FALLBACK_SEED_PORT:-31341}"
CONTAINER_NAME="dwow-test-node"
FALLBACK_LILITH_NAME="dwow-fallback-lilith"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
export COMPOSE_PROJECT_NAME="darkwow-testnet"

# --- Log capture (activated early to capture all pipeline output) ---
LOG_DIR="${LOG_DIR:-/tmp}"
mkdir -p "$LOG_DIR" 2>/dev/null || LOG_DIR=/tmp
LOGFILE="${LOG_DIR}/pipeline-$(date +%Y%m%d-%H%M%S).log"
echo "=== DarkWow Testnet Full Pipeline ==="
echo "  Mode: $MODE"
echo "  Logging to $LOGFILE"
echo ""
# Tee all output to log file for post-mortem analysis
exec > >(tee -a "$LOGFILE") 2>&1

# Test data paths (join modes)
JOIN_TEST_DATA="$(pwd)/test-data"
JOIN_TEST_MONERO="$(pwd)/test-monero-data"
JOIN_TEST_P2POOL="$(pwd)/test-p2pool-data"
JOIN_TEST_FALLBACK="$(pwd)/test-fallback-data"
JOIN_TEST_PERSIST="$(pwd)/test-persist-data"

MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-}"

# Bridge-specific constants
BRIDGE_CONTAINER="dwow-bridge-node"
BRIDGE_TEST_HELPER="${REPO_ROOT}/target/release/bridge_test_helper"
BRIDGE_TEST_HELPER_DEBUG="${REPO_ROOT}/target/debug/bridge_test_helper"
WASM_BRIDGE="${REPO_ROOT}/src/contract/bridge/darkfi_bridge_contract.wasm"
WASM_RELAYER_ENDOWMENT="${REPO_ROOT}/src/contract/relayer_endowment/darkfi_relayer_endowment_contract.wasm"
WASM_DEPLOOOOR="${REPO_ROOT}/src/contract/deployooor/dwow_deployooor_contract.wasm"
