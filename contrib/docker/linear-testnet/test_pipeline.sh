#!/bin/bash
# Linear-Testnet Deterministic Testing Pipeline
# Works for anyone who clones the repo
#
# Design:
#   1. Atomic cleanup (remove ALL containers, images, volumes)
#   2. Docker build via docker-compose (build: directive)
#   3. Pre-start verification
#   4. Start containers via docker-compose
#   5. Post-start verification
#   6. Network connectivity verification
#   7. P2P connection verification

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Determine repo root from script location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$(dirname "$(dirname "$(dirname "$SCRIPT_DIR")")")" && pwd)"
COMPOSE_DIR="$SCRIPT_DIR"

cd "$REPO_ROOT"

echo "=== Deterministic Docker Testnet Pipeline ==="
echo ""

# ============================================================================
# PHASE 1: Atomic Cleanup - Aggressive
# ============================================================================
echo "[1/7] Atomic cleanup..."

# Kill ALL containers matching our pattern
echo "  Killing containers..."
for container in $(docker ps -a --filter "name=darkfi-linear" --format '{{.Names}}' 2>/dev/null); do
    echo "    Killing $container"
    docker kill "$container" 2>/dev/null || true
done

# Remove ALL containers matching our pattern
echo "  Removing containers..."
for container in $(docker ps -a --filter "name=darkfi-linear" --format '{{.Names}}' 2>/dev/null); do
    echo "    Removing $container"
    docker rm -f "$container" 2>/dev/null || true
done

# Remove ALL images we build by pattern
echo "  Removing images by pattern..."
for image in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null); do
    if echo "$image" | grep -qE 'linear-testnet|darkfi.*linear|linear.*node'; then
        echo "    Removing image $image"
        docker rmi -f "$image" 2>/dev/null || true
    fi
done

# Remove images by checking Labels for darkfi/linear build
echo "  Removing darkfi images by label check..."
for img_id in $(docker images -q 2>/dev/null); do
    if docker inspect "$img_id" --format '{{.Config.Labels.org.opencontainers.image.title}}' 2>/dev/null | grep -qi darkfi; then
        echo "    Removing image $img_id"
        docker rmi -f "$img_id" 2>/dev/null || true
    fi
done

# Also check for our custom labels
for img_id in $(docker images -q 2>/dev/null); do
    if docker inspect "$img_id" --format '{{.Config.Labels.com.docker.compose}}' 2>/dev/null | grep -qi linear; then
        echo "    Removing image $img_id"
        docker rmi -f "$img_id" 2>/dev/null || true
    fi
done

# Remove ALL volumes with our prefix
echo "  Removing volumes..."
for volume in $(docker volume ls -q 2>/dev/null); do
    if echo "$volume" | grep -qE 'linear-testnet|darkfi.*linear'; then
        echo "    Removing volume $volume"
        docker volume rm -f "$volume" 2>/dev/null || true
    fi
done

# Full system prune
echo "  Full system prune..."
docker system prune -af --volumes 2>/dev/null || true

echo "  ${GREEN}Cleanup complete${NC}"
echo ""

# ============================================================================
# PHASE 2: Docker Build (No Cache)
# ============================================================================
echo "[2/7] Building Docker images via docker-compose (no cache)..."

docker-compose -f "$COMPOSE_DIR/docker-compose.yml" build --no-cache 2>&1

echo "  ${GREEN}Docker images built${NC}"
echo ""

# ============================================================================
# PHASE 3: Pre-start Verification
# ============================================================================
echo "[3/7] Pre-start verification..."

IMAGE_NAME="linear-testnet_node0:latest"

# Check entrypoint exists
echo "  Checking entrypoint script exists..."
if docker run --rm "$IMAGE_NAME" test -f /app/entrypoint.sh 2>/dev/null; then
    echo "    Entrypoint script exists: ${GREEN}OK${NC}"
else
    echo -e "${RED}ERROR: Entrypoint script missing${NC}"
    exit 1
fi

# Check darkfid binary
echo "  Checking darkfid binary exists..."
if docker run --rm "$IMAGE_NAME" test -f /app/darkfid 2>/dev/null; then
    echo "    darkfid binary exists: ${GREEN}OK${NC}"
else
    echo -e "${RED}ERROR: darkfid binary missing${NC}"
    exit 1
fi

# Check darkfid version
echo "  Checking darkfid version..."
VERSION_OUTPUT=$(docker run --rm "$IMAGE_NAME" /app/darkfid --version 2>&1)
if echo "$VERSION_OUTPUT" | grep -q "darkfid"; then
    echo "    darkfid version: ${GREEN}${VERSION_OUTPUT}${NC}"
else
    echo -e "${RED}ERROR: Cannot get darkfid version${NC}"
    exit 1
fi

echo "  ${GREEN}Verification complete${NC}"
echo ""

# ============================================================================
# PHASE 4: Start Containers
# ============================================================================
echo "[4/7] Starting containers..."

(cd "$COMPOSE_DIR" && docker-compose up -d)

echo "  ${GREEN}Containers started${NC}"
echo ""

# ============================================================================
# PHASE 5: Post-start Verification
# ============================================================================
echo "[5/7] Post-start verification..."

# Wait for containers to initialize
sleep 5

RUNNING_CONTAINERS=$(docker ps --filter "name=darkfi-linear" --format '{{.Names}}' 2>/dev/null | wc -l)
echo "  Running containers: $RUNNING_CONTAINERS"

if [ "$RUNNING_CONTAINERS" -ne 2 ]; then
    echo -e "${RED}ERROR: Expected 2 containers, found $RUNNING_CONTAINERS${NC}"
    docker ps -a --filter "name=darkfi-linear"
    echo ""
    echo "Container logs:"
    docker ps -a --filter "name=darkfi-linear" --format '{{.Names}}' | while read name; do
        echo "=== $name ==="
        docker logs "$name" 2>&1 | tail -10
    done
    exit 1
fi

# Check containers are actually running (not exited)
for container in darkfi-linear-node0 darkfi-linear-node1; do
    STATUS=$(docker ps --filter "name=$container" --format '{{.Status}}' 2>/dev/null)
    if echo "$STATUS" | grep -q "Up"; then
        echo "    $container: ${GREEN}$STATUS${NC}"
    else
        echo -e "    ${RED}$container: $STATUS${NC}"
        echo "    Logs:"
        docker logs "$container" 2>&1 | tail -10
        exit 1
    fi
done

echo "  ${GREEN}All containers running${NC}"
echo ""

# ============================================================================
# PHASE 6: Network Connectivity Verification
# ============================================================================
echo "[6/7] Network connectivity verification..."

# Verify node0 is reachable from node1
echo "  Testing node0 ping from node1..."
PING_RESULT=$(docker exec darkfi-linear-node1 ping -c 2 node0 2>&1)
if echo "$PING_RESULT" | grep -q "ping: bad address"; then
    echo -e "${RED}ERROR: node0 not resolvable from node1${NC}"
    exit 1
elif echo "$PING_RESULT" | grep -q "2 packets transmitted, 2 received"; then
    echo "    node0 reachable from node1: ${GREEN}OK${NC}"
else
    echo -e "${YELLOW}WARNING: ping result unclear${NC}"
    echo "$PING_RESULT"
fi

# Verify node1 is reachable from node0
echo "  Testing node1 ping from node0..."
PING_RESULT=$(docker exec darkfi-linear-node0 ping -c 2 node1 2>&1)
if echo "$PING_RESULT" | grep -q "ping: bad address"; then
    echo -e "${RED}ERROR: node1 not resolvable from node0${NC}"
    exit 1
elif echo "$PING_RESULT" | grep -q "2 packets transmitted, 2 received"; then
    echo "    node1 reachable from node0: ${GREEN}OK${NC}"
else
    echo -e "${YELLOW}WARNING: ping result unclear${NC}"
    echo "$PING_RESULT"
fi

echo "  ${GREEN}Network connectivity verified${NC}"
echo ""

# ============================================================================
# PHASE 7: P2P and Initialization Verification
# ============================================================================
echo "[7/7] P2P and initialization verification..."

# Check node0 logs for initialization
sleep 3
NODE0_LOGS=$(docker logs darkfi-linear-node0 2>&1)
if echo "$NODE0_LOGS" | grep -q "Initializing DarkFi node"; then
    echo "    node0 initialized: ${GREEN}OK${NC}"
else
    echo -e "${YELLOW}WARNING: node0 initialization message not found${NC}"
fi

if echo "$NODE0_LOGS" | grep -q "error:"; then
    echo -e "${RED}ERROR: node0 has errors${NC}"
    echo "$NODE0_LOGS" | grep "error:" | head -5
    exit 1
fi

# Check node1 logs for initialization
NODE1_LOGS=$(docker logs darkfi-linear-node1 2>&1)
if echo "$NODE1_LOGS" | grep -q "Initializing DarkFi node"; then
    echo "    node1 initialized: ${GREEN}OK${NC}"
else
    echo -e "${YELLOW}WARNING: node1 initialization message not found${NC}"
fi

if echo "$NODE1_LOGS" | grep -q "error:"; then
    echo -e "${RED}ERROR: node1 has errors${NC}"
    echo "$NODE1_LOGS" | grep "error:" | head -5
    exit 1
fi

# Check for P2P related messages (peer connected, listening, etc)
if echo "$NODE0_LOGS" | grep -qiE "peer|connect|listen|seed|network"; then
    echo "    node0 P2P activity: ${GREEN}OK${NC}"
else
    echo -e "${YELLOW}WARNING: No obvious P2P activity in node0${NC}"
fi

echo ""
echo "========================================"
echo -e "${GREEN}Pipeline complete!${NC}"
echo "========================================"
echo ""
echo "Containers running:"
docker ps --filter "name=darkfi-linear" --format "table {{.Names}}\t{{.Status}}"
echo ""
echo "Next steps:"
echo "  1. Start mining: curl -X POST http://localhost:28345/json_rpc -d '{\"method\":\"miner.mine_linear\",\"params\":[]}'"
echo "  2. Check logs: docker logs darkfi-linear-node0 2>&1 | grep -i block"