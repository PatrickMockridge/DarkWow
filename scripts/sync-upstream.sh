#!/bin/bash
# Sync upstream DarkFi changes into the upstream-sync branch
# Usage: ./scripts/sync-upstream.sh [commit-message]

set -e

cd "$(dirname "$0")/.."

BRANCH="upstream-sync"
UPSTREAM_REMOTE="upstream"
UPSTREAM_BRANCH="$UPSTREAM_REMOTE/master"

echo "=== Fetching from upstream ==="
git fetch "$UPSTREAM_REMOTE"

echo "=== Checking divergence from last sync ==="
LAST_SYNC=$(git rev-parse "$BRANCH")
UPSTREAM_HEAD=$(git rev-parse "$UPSTREAM_BRANCH")

if [ "$LAST_SYNC" = "$UPSTREAM_HEAD" ]; then
    echo "Upstream is already at same commit as $BRANCH. Nothing to do."
    exit 0
fi

echo "Last sync: $LAST_SYNC"
echo "Upstream:  $UPSTREAM_HEAD"

echo "=== Merging upstream changes into $BRANCH ==="
git checkout "$BRANCH"
git merge "$UPSTREAM_BRANCH" --no-edit || {
    echo "Merge conflict detected. Resolve manually and commit."
    exit 1
}

echo "=== upstream-sync branch updated (local only) ==="
echo "To merge into master: git checkout master && git merge upstream-sync"
echo "Then push: git push origin master"