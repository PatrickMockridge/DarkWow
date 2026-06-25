# DarkWow Testnet Pipeline — Phase 1: Clean
#
# Tear down previous run — containers, volumes, temp files, orphan processes.
# Dependencies: output.sh, config.sh (REPO_ROOT, MODE, FRESH, SCRIPT_DIR,
#               CONTAINER_NAME, FALLBACK_LILITH_NAME, COMPOSE_FILE,
#               JOIN_TEST_DATA, JOIN_TEST_MONERO, JOIN_TEST_P2POOL,
#               JOIN_TEST_FALLBACK, JOIN_TEST_PERSIST),
#               helpers.sh (is_join_mode, clean_data_dir)
#
# Sourced by test_pipeline.sh after helpers.sh.

phase_clean() {
    info "Phase 1: Clean — tearing down previous state..."

    # Kill orphan build processes from prior interrupted runs.
    # These hold file locks on target/ and Cargo.lock, causing
    # the next build to fail or deadlock.
    # Scoped to this repo only — will not kill cargo builds in other projects.
    pkill -9 -f "cargo build.*${REPO_ROOT}" 2>/dev/null || true
    pkill -9 -f "rustc.*${REPO_ROOT}" 2>/dev/null || true

    # Kill orphan dwowd/lilith processes on the host. These connect
    # to the Docker bridge network and appear as phantom P2P peers
    # (172.18.0.1), crashing the sync task with a stack overflow.
    # Scoped to this repo's build artifacts only.
    pkill -9 -f "target/.*/dwowd.*${REPO_ROOT}" 2>/dev/null || true
    pkill -9 -f "target/.*/lilith.*${REPO_ROOT}" 2>/dev/null || true

    # Remove stale wallet secret with 3-tier fallback. Mount /tmp (parent)
    # not the file itself — if the file doesn't exist, -v auto-creates a
    # directory at the mount point, making the problem worse.
    rm -rf /tmp/dwow_mining_secret 2>/dev/null || \
        sudo rm -rf /tmp/dwow_mining_secret 2>/dev/null || \
        { warn "Could not remove /tmp/dwow_mining_secret (may be root-owned)"; }

    # Remove dwow_wallet wallet state so each run generates a fresh keypair.
    clean_data_dir ~/.local/share/dwow/dww

    cd "$SCRIPT_DIR"

    if is_join_mode; then
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        docker stop "$FALLBACK_LILITH_NAME" 2>/dev/null || true
        docker rm "$FALLBACK_LILITH_NAME" 2>/dev/null || true
        if docker compose -f "$COMPOSE_FILE" --profile native ps 2>/dev/null | grep -q .; then
            docker compose -f "$COMPOSE_FILE" --profile native --remove-orphans down --rmi all -v 2>&1 || \
                warn "Failed to stop native profile (containers may need manual cleanup)"
        fi
        docker compose -f "$COMPOSE_FILE" --profile merge --remove-orphans down --rmi all -v 2>/dev/null || true
        docker compose -f "$COMPOSE_FILE" --profile bridge --remove-orphans down --rmi all -v 2>/dev/null || true
        docker compose -f "$COMPOSE_FILE" --profile join-merge --remove-orphans down --rmi all -v 2>/dev/null || true
        # Remove stale join containers and ALL dwow-* containers
        for c in dwow-node0-join dwow-node0 dwow-monerod; do
            docker stop "$c" 2>/dev/null || true
            docker rm "$c" 2>/dev/null || true
        done
        STALE=$(docker ps -a --format '{{.Names}}' 2>/dev/null | grep "^dwow-" || true)
        if [ -n "$STALE" ]; then
            warn "Removing stale containers..."
            echo "$STALE" | xargs -r docker rm -f 2>/dev/null || true
        fi
        if [ "$FRESH" = "true" ]; then
            # Remove old images first — builder prune skips layers still
            # referenced by existing images, so stale COPY caches survive.
            for img in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep "^darkwow-testnet" || true); do
                docker rmi -f "$img" 2>/dev/null || true
            done
            # Prune only dwow-related build cache — not system-wide
            docker container prune -f --filter "name=dwow" 2>/dev/null || true
            for b in $(docker buildx ls --format '{{.Name}}' 2>/dev/null | grep -v '^default$' || true); do
                docker buildx prune -a -f --builder "$b" 2>/dev/null || true
            done
        fi
        clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_MONERO" "$JOIN_TEST_P2POOL" \
               "$JOIN_TEST_FALLBACK" "$JOIN_TEST_PERSIST"
        pass "clean (join mode)"
        return
    fi

    # --- Step 1: Stop ALL containers BEFORE touching volumes ---
    # Docker refuses to remove in-use volumes. Order matters.
    docker compose --profile native --remove-orphans down 2>/dev/null || true
    docker compose --profile merge --remove-orphans down 2>/dev/null || true
    docker compose --profile bridge --remove-orphans down 2>/dev/null || true
    docker compose --profile wallet --remove-orphans down 2>/dev/null || true

    for i in $(seq 1 5); do
        docker rm -f "dwow-wallet-$i" 2>/dev/null || true
    done

    STALE=$(docker ps -a -q --filter name=dwow 2>/dev/null)
    if [ -n "$STALE" ]; then
        echo "$STALE" | xargs -r docker rm -f 2>/dev/null || true
    fi

    # --- Step 2: Remove volumes (containers stopped, safe to remove) ---
    for vol in $(docker volume ls -q --filter name=darkwow-testnet 2>/dev/null); do
        docker volume rm -f "$vol" 2>/dev/null || true
    done
    for i in $(seq 1 5); do
        docker volume rm "wallet_data_$i" 2>/dev/null || true
    done
    docker volume rm wallet_data_pipeline 2>/dev/null || true
    if [ "$FRESH" = "true" ]; then
        # Remove darkwow testnet images explicitly (docker compose --rmi misses
        # images that were built with different profile combinations)
        for img in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep "^darkwow-testnet-" || true); do
            docker rmi -f "$img" 2>/dev/null || true
        done
        for img in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep "darkwow-wallet" || true); do
            docker rmi -f "$img" 2>/dev/null || true
        done

        # Prune only dwow-related containers and build cache — not system-wide
        docker container prune -f --filter "name=dwow" 2>/dev/null || true
        for b in $(docker buildx ls --format '{{.Name}}' 2>/dev/null | grep -v '^default$' || true); do
            docker buildx prune -a -f --builder "$b" 2>/dev/null || true
        done
    fi
    pass "clean"
}
