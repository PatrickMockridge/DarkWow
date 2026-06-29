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
    pkill -9 -f "cargo build.*${REPO_ROOT}" 2>/dev/null || true
    pkill -9 -f "rustc.*${REPO_ROOT}" 2>/dev/null || true
    pkill -9 -f "target/.*/dwowd.*${REPO_ROOT}" 2>/dev/null || true
    pkill -9 -f "target/.*/lilith.*${REPO_ROOT}" 2>/dev/null || true

    # Remove stale wallet secrets.
    for sf in "${SCRIPT_DIR}/.secrets"/dwow_mining_secret_*; do
        [ -e "$sf" ] || continue
        rm -f "$sf" 2>/dev/null || true
    done

    # Remove dwow_wallet wallet state.
    clean_data_dir ~/.local/share/dwow/dww

    cd "$SCRIPT_DIR"

    if is_join_mode; then
        # ... (unchanged join-mode branch)
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
            for img in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep "^darkwow-testnet" || true); do
                docker rmi -f "$img" 2>/dev/null || true
            done
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

    # ── Step 1: Force-remove ALL known container names BEFORE compose down ──
    # Explicit named removal catches containers that Docker's filter misses.
    for c in dwow-observer dwow-node0 dwow-node1 dwow-node2 dwow-monerod dwow-p2pool \
             dwow-wallet-1 dwow-wallet-2 dwow-wallet-3 dwow-wallet-4 dwow-wallet-5 \
             dwow-bridge-node dwow-xmrig; do
        docker rm -f "$c" 2>/dev/null || true
    done
    # Filter-based removal as fallback for any missed containers
    STALE=$(docker ps -a -q --filter name=dwow 2>/dev/null)
    if [ -n "$STALE" ]; then
        warn "Removing $(echo "$STALE" | wc -w) additional stale dwow container(s)..."
        echo "$STALE" | xargs -r docker rm -f 2>/dev/null || true
    fi

    # ── Step 2: Compose down (now safe — no containers to conflict) ──
    docker compose --profile native --remove-orphans down 2>/dev/null || true
    docker compose --profile merge --remove-orphans down 2>/dev/null || true
    docker compose --profile bridge --remove-orphans down 2>/dev/null || true
    docker compose --profile wallet --remove-orphans down 2>/dev/null || true

    # ── Step 3: Remove any containers compose down missed ──
    for i in $(seq 1 5); do
        docker rm -f "dwow-wallet-$i" 2>/dev/null || true
    done
    STALE=$(docker ps -a -q --filter name=dwow 2>/dev/null)
    if [ -n "$STALE" ]; then
        echo "$STALE" | xargs -r docker rm -f 2>/dev/null || true
    fi

    # ── Step 4: Remove volumes ──
    for vol in $(docker volume ls -q --filter name=darkwow-testnet 2>/dev/null); do
        docker volume rm -f "$vol" 2>/dev/null || true
    done
    for i in $(seq 1 5); do
        docker volume rm "wallet_data_$i" 2>/dev/null || true
    done
    docker volume rm wallet_data_pipeline 2>/dev/null || true

    # ── Step 5: Prune (always, not just FRESH) ──
    docker container prune -f --filter "name=dwow" 2>/dev/null || true
    docker network prune -f --filter "name=dwow" 2>/dev/null || true

    if [ "$FRESH" = "true" ]; then
        for img in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep "^darkwow-testnet-" || true); do
            docker rmi -f "$img" 2>/dev/null || true
        done
        for img in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep "darkwow-wallet" || true); do
            docker rmi -f "$img" 2>/dev/null || true
        done
        for b in $(docker buildx ls --format '{{.Name}}' 2>/dev/null | grep -v '^default$' || true); do
            docker buildx prune -a -f --builder "$b" 2>/dev/null || true
        done
    fi

    # Verify no dwow containers remain
    STALE=$(docker ps -a -q --filter name=dwow 2>/dev/null)
    if [ -n "$STALE" ]; then
        fail "clean: $(echo "$STALE" | wc -w) dwow containers still present after cleanup"
    else
        pass "clean"
    fi
}
