#!/bin/bash
# DarkFi Testnet Restart Script
# Usage: ./restart.sh [-vvv]

VERBOSE="$1"

echo "Restarting dwowd..."
./stop.sh
sleep 2
./start.sh $VERBOSE