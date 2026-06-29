# DarkWow Testnet Pipeline — Phase 2: Build
#
# Build Docker images from origin — base, testnet, wallet.
# Dependencies: output.sh (info, pass, fail, error, check),
#               config.sh (REPO_ROOT, SKIP_BUILD, WITH_WALLET, BUILD_COMMIT,
#                          REBUILD_BASE, NO_CACHE, MODE, SCRIPT_DIR, COMPOSE_FILE),
#               helpers.sh (is_join_mode)
#
# Sourced by test_pipeline.sh after phase_01_clean.sh.

phase_build() {
    info "Phase 2: Building images..."

    # Pre-flight: disk space check. WASM builds + release compilation
    # consume 5-10GB. Fail early if space is tight.
    local free_kb
    free_kb=$(df -k "$REPO_ROOT" | awk 'NR==2 {print $4}')
    if [ "$free_kb" -lt 5000000 ]; then
        error "Low disk space: $(($free_kb / 1024))MB free. Need at least 5GB for builds."
    fi

    # --skip-build: use cached images, verify they exist
    if [ "$SKIP_BUILD" = "true" ]; then
        info "  Skipping build (--skip-build), verifying cached images..."
        docker image inspect darkwow-testnet:latest >/dev/null 2>&1 || {
            error "darkwow-testnet:latest not found — run without --skip-build first"
        }
        if [ "$WITH_WALLET" -gt 0 ] && ! is_join_mode; then
            docker image inspect darkwow-wallet:latest >/dev/null 2>&1 || {
                error "darkwow-wallet:latest not found — run without --skip-build first"
            }
        fi
        pass "cached images verified"
        return
    fi

    # Pre-flight: verify BUILD_COMMIT exists on origin/linear-master.
    # Skip when BUILD_LOCAL=true — the source comes from the working tree.
    if [ "$BUILD_LOCAL" != "true" ] && ! git ls-remote --heads origin linear-master 2>/dev/null | grep -q "$BUILD_COMMIT"; then
        error "BUILD_COMMIT ${BUILD_COMMIT} not found on origin/linear-master"
        error "The pipeline clones from origin/linear-master and tests code on that branch."
        error "Your commit may be on a different branch, or hasn't been pushed."
        error ""
        error "Options:"
        error "  1. Push your changes to origin/linear-master"
        error "  2. Set BUILD_COMMIT to a commit that exists on origin/linear-master"
        error "     BUILD_COMMIT=<sha> ./test_pipeline.sh --mode native"
        error "  3. Use --skip-build to test with the last built image"
        exit 1
    fi
    info "  BUILD_COMMIT ${BUILD_COMMIT:0:10}... verified on origin/linear-master"

    # Build base image if missing or --rebuild-base specified.
    # Cached by default for fast dev iterations. Use --rebuild-base when
    # Dockerfile.base changes (toolchain, system deps).
    if [ "$REBUILD_BASE" = "true" ] || ! docker image inspect darkwow-base:24.04 >/dev/null 2>&1; then
        if [ "$REBUILD_BASE" = "true" ]; then
            info "  Rebuilding base image darkwow-base:24.04 (--rebuild-base)..."
            docker build --no-cache -t darkwow-base:24.04 -f "$SCRIPT_DIR/Dockerfile.base" "$REPO_ROOT" 2>&1
        else
            info "  Base image not found, building darkwow-base:24.04..."
            docker build -t darkwow-base:24.04 -f "$SCRIPT_DIR/Dockerfile.base" "$REPO_ROOT" 2>&1
        fi
        pass "base image built"
    else
        info "  Using cached darkwow-base:24.04 (--rebuild-base to force)"
    fi

    BUILD_ARGS=""
    if [ "$NO_CACHE" = "true" ]; then
        BUILD_ARGS="--no-cache"
    fi
    BUILD_ARGS="$BUILD_ARGS --build-arg BUILD_LOCAL=\"${BUILD_LOCAL:-false}\""

    # Forward host resource-control env vars into the Docker build.
    # The Dockerfile converts these ARGs to ENVs, and all cargo build commands
    # use -j ${CARGO_BUILD_JOBS} so the override takes full effect.
    local cargo_jobs="${CARGO_BUILD_JOBS:-1}"
    local rayon_threads="${RAYON_NUM_THREADS:-2}"
    BUILD_ARGS="$BUILD_ARGS --build-arg CARGO_BUILD_JOBS=${cargo_jobs}"
    BUILD_ARGS="$BUILD_ARGS --build-arg RAYON_NUM_THREADS=${rayon_threads}"
    info "  Build parallelism: JOBS=${cargo_jobs}, RAYON=${rayon_threads}"

    # Build the main testnet image ONCE. Previously docker compose --profile native
    # triggered 6 separate docker build invocations (one per service: observer,
    # node0-4) even though all 6 share darkwow-testnet:latest from the same
    # Dockerfile. Direct docker build guarantees exactly one compilation.
    # Defense-in-depth: one build = one compilation. No service-count multiplier.
    if [ "$MODE" = "merge" ]; then
        info "  Building darkwow-testnet:latest..."
        docker build \
            $BUILD_ARGS \
            --build-arg BUILD_COMMIT="${BUILD_COMMIT}" \
            -t darkwow-testnet:latest \
            -f "$SCRIPT_DIR/Dockerfile" \
            "$REPO_ROOT" 2>&1
        check $? "docker build testnet"
        # Merge-mining sidecars (monerod, p2pool) — no Rust compilation
        info "  Building merge sidecars..."
        docker compose --profile merge build $BUILD_ARGS 2>&1
        check $? "docker build (merge sidecars)"
    elif [ "$MODE" = "bridge" ]; then
        info "  Building darkwow-testnet:latest..."
        docker build \
            $BUILD_ARGS \
            --build-arg BUILD_COMMIT="${BUILD_COMMIT}" \
            -t darkwow-testnet:latest \
            -f "$SCRIPT_DIR/Dockerfile" \
            "$REPO_ROOT" 2>&1
        check $? "docker build testnet"
        info "  Building bridge-node..."
        docker build \
            $BUILD_ARGS \
            -t darkwow-bridge-node:latest \
            -f "$REPO_ROOT/contrib/docker/bridge-node/Dockerfile" \
            "$REPO_ROOT" 2>&1
        check $? "docker build bridge-node"
    elif [ "$MODE" = "join-merge" ]; then
        # Join-merge sidecars (monerod/p2pool — download binaries, no Rust)
        docker compose --profile join-merge build $BUILD_ARGS \
            --build-arg BUILD_COMMIT="${BUILD_COMMIT}" 2>&1
        check $? "docker build (join-merge profile)"
        # Build testnet image once for observer
        docker build \
            $BUILD_ARGS \
            --build-arg BUILD_COMMIT="${BUILD_COMMIT}" \
            -t darkwow-testnet:latest \
            -f "$SCRIPT_DIR/Dockerfile" \
            "$REPO_ROOT" 2>&1
        check $? "docker build testnet"
    elif [ "$MODE" = "join-native" ]; then
        # Build testnet image once for observer
        docker build \
            $BUILD_ARGS \
            --build-arg BUILD_COMMIT="${BUILD_COMMIT}" \
            -t darkwow-testnet:latest \
            -f "$SCRIPT_DIR/Dockerfile" \
            "$REPO_ROOT" 2>&1
        check $? "docker build testnet"
    else
        # Native mode (default): build testnet image once for all 6 services
        info "  Building darkwow-testnet:latest..."
        docker build \
            $BUILD_ARGS \
            --build-arg BUILD_COMMIT="${BUILD_COMMIT}" \
            -t darkwow-testnet:latest \
            -f "$SCRIPT_DIR/Dockerfile" \
            "$REPO_ROOT" 2>&1
        check $? "docker build testnet"
    fi

    if [ "$WITH_WALLET" -gt 0 ] && ! is_join_mode; then
        info "  Building wallet container..."
        docker build \
            $BUILD_ARGS \
            --build-arg BUILD_COMMIT="${BUILD_COMMIT}" \
            --build-arg CARGO_PACKAGE="${CARGO_PACKAGE:-dwow_wallet}" \
            -t darkwow-wallet:latest \
            -f "$SCRIPT_DIR/Dockerfile.wallet" \
            "$REPO_ROOT" 2>&1
        check $? "docker build wallet"
    fi

    pass "build complete"
}
