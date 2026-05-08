#!/bin/bash
# DarkFi Testnet Logs Script
# Usage: ./logs.sh [lines]

DWOW_HOME="${DWOW_HOME:-$HOME/.local/share/dwow}"
DARKFID_HOME="$DWOW_HOME/darkfid"
LOG_FILE="$DARKFID_HOME/testnet.log"

LINES="${1:-50}"

if [ ! -f "$LOG_FILE" ]; then
    echo "Log file not found: $LOG_FILE"
    echo ""
    echo "The daemon may not have started yet or logs are going elsewhere."
    echo "Try starting with: ./start.sh"
    exit 1
fi

echo "Showing last $LINES lines of $LOG_FILE"
echo "(Use Ctrl+C to exit, -n to specify different line count: ./logs.sh 100)"
echo "=========================================="
tail -n "$LINES" -f "$LOG_FILE"