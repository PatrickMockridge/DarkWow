#!/bin/bash
# DarkFi Testnet Start Script
# Usage: ./start.sh [-vvv]

VERBOSE=""
if [ "$1" = "-vvv" ]; then
    VERBOSE="-vvv"
elif [ "$1" = "-vv" ]; then
    VERBOSE="-vv"
elif [ "$1" = "-v" ]; then
    VERBOSE="-v"
fi

DWOW_HOME="${DWOW_HOME:-$HOME/.local/share/darkfi}"
DARKFID_HOME="$DWOW_HOME/darkfid"
CONFIG_FILE="$DARKFID_HOME/darkfid_config.toml"
PID_FILE="$DARKFID_HOME/testnet.pid"
LOG_FILE="$DARKFID_HOME/testnet.log"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

# Check if already running
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if kill -0 "$PID" 2>/dev/null; then
        echo -e "${RED}Error: dwowd is already running (PID: $PID)${NC}"
        echo "Use './restart.sh' to restart or './stop.sh' to stop first."
        exit 1
    else
        echo "Stale PID file found, removing..."
        rm -f "$PID_FILE"
    fi
fi

# Check if dwowd is running without PID file ( orphaned)
if pgrep -f "darkfid.*testnet" > /dev/null 2>&1; then
    echo -e "${RED}Error: dwowd appears to be running (found via pgrep)${NC}"
    echo "Use './restart.sh' to restart or './stop.sh' to stop first."
    exit 1
fi

# Ensure config exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo "Config not found. Running setup first..."
    ./setup.sh
fi

echo "Starting dwowd on testnet..."
echo "Log file: $LOG_FILE"
echo "PID file: $PID_FILE"

# Start dwowd in background
nohup dwowd --config "$CONFIG_FILE" --network testnet $VERBOSE >> "$LOG_FILE" 2>&1 &
PID=$!

# Save PID
echo $PID > "$PID_FILE"

# Wait a moment for startup
sleep 2

# Check if still running
if kill -0 "$PID" 2>/dev/null; then
    echo -e "${GREEN}darkfid started successfully (PID: $PID)${NC}"
    echo ""
    echo "Useful commands:"
    echo "  ./logs.sh     - View logs"
    echo "  ./status.sh   - Check sync status"
    echo "  ./stop.sh     - Stop the daemon"
else
    echo -e "${RED}Error: dwowd failed to start${NC}"
    echo "Check log file for errors: $LOG_FILE"
    rm -f "$PID_FILE"
    exit 1
fi