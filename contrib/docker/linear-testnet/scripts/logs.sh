#!/bin/bash
# View logs from the 5-node linear-testnet Docker stack
# Usage: ./logs.sh [node0|node1|node2|node3|node4|all|xmrig0|xmrig1|...]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

if [ -z "$1" ] || [ "$1" = "all" ]; then
    echo "=== Viewing all logs ==="
    docker-compose logs -f
elif [ "$1" = "xmrig" ]; then
    echo "=== Viewing xmrig logs ==="
    docker-compose logs -f xmrig0 xmrig1 xmrig2 xmrig3 xmrig4
elif [[ "$1" =~ ^node[0-4]$ ]]; then
    echo "=== Viewing $1 logs ==="
    docker-compose logs -f "$1"
elif [[ "$1" =~ ^xmrig[0-4]$ ]]; then
    echo "=== Viewing $1 logs ==="
    docker-compose logs -f "$1"
else
    echo "Usage: $0 [node0|node1|node2|node3|node4|xmrig0|xmrig1|xmrig2|xmrig3|xmrig4|all]"
    echo ""
    echo "Examples:"
    echo "  $0 all        # View all logs"
    echo "  $0 node0      # View node0 logs"
    echo "  $0 xmrig0     # View xmrig0 logs"
    echo "  $0 xmrig      # View all xmrig logs"
    exit 1
fi