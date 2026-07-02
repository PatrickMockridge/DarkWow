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

    # ── Step 1: Compose down WITH volumes FIRST ──
    # Must run before any manual docker rm — compose needs its state tracking
    # intact to identify and remove named volumes (observer_data, node0_data, etc).
    # Running docker rm -f first DESTROYS compose's project tracking, so -v
    # can't find the volumes. Order matters.
    docker compose --profile native --remove-orphans down -v 2>/dev/null || true
    docker compose --profile merge --remove-orphans down -v 2>/dev/null || true
    docker compose --profile bridge --remove-orphans down -v 2>/dev/null || true
    docker compose --profile wallet --remove-orphans down -v 2>/dev/null || true

    # ── Step 2: Force-remove any containers compose down missed ──
    for c in dwow-observer dwow-node0 dwow-node1 dwow-node2 dwow-monerod dwow-p2pool \
             dwow-wallet-1 dwow-wallet-2 dwow-wallet-3 dwow-wallet-4 dwow-wallet-5 \
             dwow-bridge-node dwow-xmrig; do
        docker rm -f "$c" 2>/dev/null || true
    done
    STALE=$(docker ps -a -q --filter name=dwow 2>/dev/null)
    if [ -n "$STALE" ]; then
        echo "$STALE" | xargs -r docker rm -f 2>/dev/null || true
    fi

    # ── Step 3: Belt-and-suspenders volume removal ──
    for vol in $(docker volume ls -q --filter name=darkwow-testnet 2>/dev/null); do
        docker volume rm -f "$vol" 2>/dev/null || true
    done
    for i in $(seq 1 5); do
        docker volume rm "wallet_data_$i" 2>/dev/null || true
    done
    docker volume rm wallet_data_pipeline 2>/dev/null || true

    # ── Step 4: Prune (always, not just FRESH) ──
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
    fi

    # Verify no dwow volumes remain — the observer_data volume surviving
    # between runs is the #1 cause of stale-chain contamination.
    STALE_VOLS=$(docker volume ls -q --filter name=darkwow-testnet 2>/dev/null)
    if [ -n "$STALE_VOLS" ]; then
        fail "clean: $(echo "$STALE_VOLS" | wc -w) dwow volumes still present after cleanup: $(echo "$STALE_VOLS" | tr '\n' ' ')"
    fi

    if [ -z "$STALE" ] && [ -z "$STALE_VOLS" ]; then
        pass "clean"
    fi
}
