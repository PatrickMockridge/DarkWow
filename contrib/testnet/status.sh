#!/bin/bash
# DarkFi Testnet Status Script
# Usage: ./status.sh

DARKFI_HOME="${DARKFI_HOME:-$HOME/.local/share/darkfi}"
DARKFID_HOME="$DARKFI_HOME/darkfid"
PID_FILE="$DARKFID_HOME/testnet.pid"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=========================================="
echo "  DarkFi Testnet Node Status"
echo "=========================================="
echo ""

# Check PID file
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if kill -0 "$PID" 2>/dev/null; then
        echo -e "Daemon:     ${GREEN}Running${NC} (PID: $PID)"
    else
        echo -e "Daemon:     ${RED}Not running${NC} (stale PID file)"
        exit 1
    fi
else
    # Check if running without PID file
    PID=$(pgrep -f "darkfid.*testnet" 2>/dev/null | head -1)
    if [ -n "$PID" ]; then
        echo -e "Daemon:     ${GREEN}Running${NC} (PID: $PID, no PID file)"
    else
        echo -e "Daemon:     ${RED}Not running${NC}"
        exit 1
    fi
fi

# Try to get sync status via RPC
echo ""
echo "Getting sync status via RPC..."

# Check if RPC is responding
RPC_RESPONSE=$(curl -s -X POST http://127.0.0.1:18345/json_rpc \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"blockchain.get_info","id":1}' 2>/dev/null || echo "")

if [ -n "$RPC_RESPONSE" ]; then
    HEIGHT=$(echo "$RPC_RESPONSE" | grep -o '"sync_height":[0-9]*' | cut -d':' -f2)
    LOCAL_HEIGHT=$(echo "$RPC_RESPONSE" | grep -o '"local_height":[0-9]*' | cut -d':' -f2)
    HEAD_HASH=$(echo "$RPC_RESPONSE" | grep -o '"head":[^,]*' | cut -d'"' -f4)

    if [ -n "$HEIGHT" ] && [ -n "$LOCAL_HEIGHT" ]; then
        echo "Local Height:  $LOCAL_HEIGHT"
        echo "Sync Height:   $HEIGHT"

        if [ "$LOCAL_HEIGHT" -ge "$HEIGHT" ]; then
            echo -e "Sync Status:   ${GREEN}In sync${NC}"
        else
            echo -e "Sync Status:   ${YELLOW}Syncing ($((HEIGHT - LOCAL_HEIGHT)) blocks behind)${NC}"
        fi
    fi

    if [ -n "$HEAD_HASH" ]; then
        echo "Head Hash:     ${HEAD_HASH:0:16}..."
    fi
else
    echo -e "${YELLOW}RPC not responding or not available${NC}"
    echo "Check logs: ./logs.sh"
fi

echo ""
echo "Network:     testnet"
echo "RPC URL:     http://127.0.0.1:18345"
echo "Mgmt RPC:    http://127.0.0.1:18346"