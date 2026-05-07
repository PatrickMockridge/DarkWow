#!/bin/sh
set -e

# Start a tmux session with a dwowd node in linear-testnet mode
# Mining is done via RPC: miner.mine_linear

# Path to dwowd binary
DARKFID="LOG_TARGETS='!net,!runtime,!sled' ../../../dwowd -c dwowd.toml"

session=darkfid-single-node

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session -n $session
tmux send-keys -t $session "$DARKFID $verbose" Enter
tmux attach -t $session
