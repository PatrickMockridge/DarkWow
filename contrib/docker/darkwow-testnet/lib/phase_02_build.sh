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

    # Build base image if it doesn't exist. All services FROM this image.
    # --no-cache ensures the RUN git clone step always fetches the latest
    # code from origin. Docker's RUN cache is keyed by instruction text,
    # not by remote state, so stale layers persist even after builder prune.
    if [ "$MODE" = "merge" ]; then
        docker compose --profile merge build $BUILD_ARGS --build-arg BUILD_COMMIT="${BUILD_COMMIT}" 2>&1
        check $? "docker build (merge profile)"
    elif [ "$MODE" = "bridge" ]; then
        docker compose --profile native build $BUILD_ARGS --build-arg BUILD_COMMIT="${BUILD_COMMIT}" 2>&1
        check $? "docker build (native profile)"
        docker compose --profile bridge build $BUILD_ARGS 2>&1
        check $? "docker build (bridge profile)"
    elif [ "$MODE" = "join-merge" ]; then
        docker compose --profile join-merge build $BUILD_ARGS --build-arg BUILD_COMMIT="${BUILD_COMMIT}" 2>&1
        check $? "docker build (join-merge profile)"
        docker compose --profile native build $BUILD_ARGS --build-arg BUILD_COMMIT="${BUILD_COMMIT}" lilith 2>&1
        check $? "docker build (lilith image for join phases)"
    elif [ "$MODE" = "join-native" ]; then
        docker compose --profile native build $BUILD_ARGS --build-arg BUILD_COMMIT="${BUILD_COMMIT}" lilith 2>&1
        check $? "docker build (lilith image for join phases)"
    else
        docker compose --profile native build $BUILD_ARGS --build-arg BUILD_COMMIT="${BUILD_COMMIT}" 2>&1
        check $? "docker build"
    fi

    if [ "$WITH_WALLET" -gt 0 ] && ! is_join_mode; then
        info "  Building wallet container..."
        docker compose --profile wallet build $BUILD_ARGS --build-arg BUILD_COMMIT="${BUILD_COMMIT}" 2>&1
        check $? "docker build (wallet profile)"
    fi

    pass "build complete"
}
