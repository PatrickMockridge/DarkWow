#!/bin/bash
# DarkFi Testnet Stop Script
# Usage: ./stop.sh

DWOW_HOME="${DWOW_HOME:-$HOME/.local/share/dwow}"
DARKFID_HOME="$DWOW_HOME/darkfid"
PID_FILE="$DARKFID_HOME/testnet.pid"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

if [ ! -f "$PID_FILE" ]; then
    # Try to find by pgrep
    PID=$(pgrep -f "darkfid.*testnet" 2>/dev/null | head -1)
    if [ -n "$PID" ]; then
        echo "Found dwowd running (PID: $PID), stopping..."
        kill "$PID"
        sleep 2
        if kill -0 "$PID" 2>/dev/null; then
            echo "Graceful stop failed, forcing..."
            kill -9 "$PID" 2>/dev/null || true
        fi
        echo -e "${GREEN}darkfid stopped${NC}"
    else
        echo -e "${RED}darkfid is not running (no PID file found)${NC}"
    fi
    exit 0
fi

PID=$(cat "$PID_FILE")

if ! kill -0 "$PID" 2>/dev/null; then
    echo "Stale PID file, removing..."
    rm -f "$PID_FILE"
    exit 0
fi

echo "Stopping dwowd (PID: $PID)..."
kill "$PID"

# Wait up to 10 seconds for graceful shutdown
for i in $(seq 1 10); do
    if ! kill -0 "$PID" 2>/dev/null; then
        echo -e "${GREEN}darkfid stopped gracefully${NC}"
        rm -f "$PID_FILE"
        exit 0
    fi
    sleep 1
done

# Force kill if still running
echo "Forcing shutdown..."
kill -9 "$PID" 2>/dev/null || true
rm -f "$PID_FILE"
echo -e "${GREEN}darkfid stopped${NC}"