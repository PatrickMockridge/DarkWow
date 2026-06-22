# Build Resource HAZOP

This is a HAZOP (Hazard and Operability) analysis of the Docker build pipeline's
resource consumption. It identifies every valve that controls CPU and memory usage,
how they interact, which safety barriers are degraded, and how small loosenings
cascade into multiplicative bloat.

## Valve Inventory

Eight valves control resource consumption during a Docker build. Each valve limits
a different dimension of parallelism. They interact **multiplicatively** — loosening
two valves doubles the effect of each.

| # | Valve | Location | Mechanism | Default | Overridable? | Scope |
|---|-------|----------|-----------|---------|-------------|-------|
| V1 | `CARGO_BUILD_JOBS` | Dockerfile `ARG`/`ENV` | Limits concurrent `rustc` processes per `cargo` invocation | 1 | `--build-arg` or host env | Build |
| V2 | `-j N` on cargo CLI | Dockerfile `RUN` lines | Explicit job count — **overrides V1** | `${CARGO_BUILD_JOBS}` | Indirectly via V1 | Build |
| V3 | `RAYON_NUM_THREADS` | Dockerfile `ARG`/`ENV` | Limits Rayon thread pool inside each `rustc` | 2 | `--build-arg` or host env | Build + Runtime |
| V4 | Docker Compose parallelism | `docker compose build` | Builds multiple services concurrently | all services | `COMPOSE_PARALLEL_LIMIT` | Build |
| V5 | Pipeline → Docker forwarding | `phase_02_build.sh` | Passes host env vars as `--build-arg` | forwards V1,V3 | Env vars on host | Build |
| V6 | `deploy.resources.limits` | `docker-compose.yml` | CPU/RAM caps on containers | 4 GB / 2 CPUs | compose override | **Runtime only** |
| V7 | Cargo profiles (`lto`, `codegen-units`) | workspace `Cargo.toml` | LLVM codegen memory per crate | thin LTO, cgu=16 | Cargo.toml edit | Build |
| V8 | Linker (`mold` vs `ld`) | system default | Final binary link memory | GNU `ld` | `RUSTFLAGS` or `.cargo/config.toml` | Build |

## Interaction Matrix

The valves do not operate in isolation. Peak memory is the product of their
interactions:

```
Peak_RAM ≈ (V1 × 1.2 GB) + (V3 × 0.6 GB) + V8_link_overhead + 0.5 GB
```

| V1=1 | V1=2 | V1=4 | V1=8 |
|------|------|------|------|
| **V3=2**: 2.9 GB | **V3=2**: 4.1 GB | **V3=2**: 6.5 GB | **V3=2**: 11.3 GB |
| **V3=4**: 4.1 GB | **V3=4**: 5.3 GB | **V3=4**: 7.7 GB | **V3=4**: 12.5 GB |
| **V3=8**: 6.5 GB | **V3=8**: 7.7 GB | **V3=8**: 10.1 GB | **V3=8**: 14.9 GB |
| **V3=16**: 11.3 GB | **V3=16**: 12.5 GB | **V3=16**: 14.9 GB | **V3=16**: 19.7 GB |

The safe zone for a 16 GB machine (accounting for OS + Docker overhead) is
roughly the top-left 2×2 cells. Beyond that, you risk OOM.

V8 (linker) adds 2–6 GB during the final native daemon link step. With GNU `ld`
and LTO, the link step alone can reach 4 GB per job. With `mold`, it drops to
~500 MB.

## Cascade Analysis: How Small Loosenings Multiply

The user's observation — "small loosenings cause big bloat" — is mechanically
precise. Each valve loosening compounds:

### Scenario A: Tight (JOBS=1, RAYON=2)
```
Peak: 1.2 + 1.2 + 0.5 = 2.9 GB    Fits in 4 GB ✓
```

### Scenario B: "Just one more job" (JOBS=2, RAYON=2)
```
Peak: 2.4 + 1.2 + 0.5 = 4.1 GB    +41% from "just one more"
```

### Scenario C: "And two more rayon threads" (JOBS=2, RAYON=4)
```
Peak: 2.4 + 2.4 + 0.5 = 5.3 GB    +83% from A, +29% from B
```

### Scenario D: "Reasonable dev settings" (JOBS=4, RAYON=8)
```
Peak: 4.8 + 4.8 + 0.5 = 10.1 GB   +248% from A
```

### Scenario E: "What upstream used" (JOBS=20, RAYON=10)
```
Peak: 24 + 6 + 0.5 = 30.5 GB      OOM on anything less than 64 GB
```

Each step feels reasonable in isolation. Nobody sets JOBS=4 thinking "I'm using
3.5× the minimum memory." They think "4 is conservative, my CPU has 8 cores."
The interaction with RAYON is invisible until the OOM kill.

## Bow-Tie: OOM Kill Event

```
                      PREVENTIVE BARRIERS              │           MITIGATIVE BARRIERS
                      (stop OOM before it happens)      │      (limit damage after OOM starts)
                                                       │
    [Resource demand]  ──┬── B1: CARGO_BUILD_JOBS      │      B5: Kernel OOM killer ──→ [SIGKILL]
                         │    Status: INTACT            │      Status: LAST RESORT
                         │    Limits cargo -j           │      Kills largest process
                         │                              │
                         ├── B2: RAYON_NUM_THREADS      │      B6: Pipeline exit check
                         │    Status: INTACT            │      Status: INTACT
                         │    Limits codegen threads    │      Detects exit code 137
                         │                              │
                         ├── B3: ARG-based overrides     │      B7: Docker build --memory
                         │    Status: INTACT            │      Status: NOT CONFIGURED
                         │    Propagates host settings  │      Would enforce hard cap
                         │                              │
                         └── B4: COMPOSE_PARALLEL_LIMIT │
                              Status: INTACT            │
                              Prevents N× builds        │
                                                       │
    [Threat realized] ──────────────────────────────────────→ [OOM kill — exit 137]
```

### Barrier Status After Fix

| Barrier | Before | After | What changed |
|---------|--------|-------|-------------|
| B1: CARGO_BUILD_JOBS | **DEGRADED** — bypassed by hardcoded `-j 2` | **INTACT** — `-j ${CARGO_BUILD_JOBS}` respects env var | Replaced hardcoded `-j 2` with variable in 5 Dockerfiles |
| B2: RAYON_NUM_THREADS | INTACT | INTACT | No bypass existed |
| B3: ARG overrides | **DEGRADED** — not forwarded from host | **INTACT** — phase_02_build.sh forwards to docker compose | Added `--build-arg CARGO_BUILD_JOBS` + `RAYON_NUM_THREADS` forwarding |
| B4: COMPOSE_PARALLEL_LIMIT | **MISSING** — no explicit limit | **INTACT** — `COMPOSE_PARALLEL_LIMIT=1` | Added defense-in-depth limit |
| B5: Kernel OOM killer | LAST RESORT | LAST RESORT | Cannot be upgraded — this is the kernel |
| B6: Exit code check | INTACT | INTACT | Correctly detects 137 |
| B7: Docker --memory | **NOT CONFIGURED** | **NOT CONFIGURED** | Future enhancement — requires BuildKit |

## The `-j` Flag Precedence Problem (Historical)

**Root cause of the recurring OOM kills:** The `-j N` flag on `cargo build` takes
precedence over the `CARGO_BUILD_JOBS` environment variable. Every Dockerfile had
hardcoded `-j 2` on its final `cargo build` commands. This meant:

1. The Dockerfile set `CARGO_BUILD_JOBS=1` (via ARG default) — looks correct
2. But `cargo build -j 2` ignores it — the flag wins
3. The LTO link step runs with 2 jobs, each using ~4 GB → 8 GB peak
4. Even setting `--build-arg CARGO_BUILD_JOBS=1` had no effect on the final build

The fix replaces every `-j N` with `-j ${CARGO_BUILD_JOBS}`, making the ARG-based
override system authoritative. The `-j` flag now **defers to** the env var rather
than **overriding** it.

## Per-Tier Valve Settings

### Minimum (4 GB, 2–4 cores) — committed defaults

| Valve | Setting | Rationale |
|-------|---------|-----------|
| V1: CARGO_BUILD_JOBS | 1 | One rustc at a time |
| V2: -j flag | `${CARGO_BUILD_JOBS}` | Defers to V1 |
| V3: RAYON_NUM_THREADS | 2 | Minimum codegen parallelism |
| V4: COMPOSE_PARALLEL_LIMIT | 1 | Single image build |
| V8: Linker | GNU `ld` (default) | No configuration needed |
| **Peak RAM** | **~2.9 GB** | Leaves 1.1 GB for OS |

### Recommended (16 GB, 8–12 cores)

| Valve | Setting | Rationale |
|-------|---------|-----------|
| V1: CARGO_BUILD_JOBS | 4 | Four parallel rustc processes |
| V3: RAYON_NUM_THREADS | 4 | Moderate codegen parallelism |
| V4: COMPOSE_PARALLEL_LIMIT | 1 | Single image build |
| V8: Linker | `mold` if available | Cuts link memory 8× |
| **Peak RAM** | **~7.7 GB** | Well within 16 GB |

### Pro/Server (64 GB+, 16–64 cores)

| Valve | Setting | Rationale |
|-------|---------|-----------|
| V1: CARGO_BUILD_JOBS | 8 | Practical cap (diminishing returns beyond 8) |
| V3: RAYON_NUM_THREADS | physical cores | Full codegen parallelism |
| V4: COMPOSE_PARALLEL_LIMIT | 1 | Single image build |
| V8: Linker | `mold` | Cuts link memory 8× |
| **Peak RAM** | **~29.5 GB** with GNU ld, **~9.9 GB** with mold | |

## How to Read This

When adjusting build resources, consult this document in order:

1. **Identify your hardware tier** from the table above
2. **Set the valves** by exporting env vars before running the pipeline
3. **Check the interaction matrix** — if you loosen one valve, check what it
   multiplies with
4. **Watch the bow-tie** — every valve is a barrier. If you bypass one (e.g.,
   by editing a Dockerfile directly), you're operating with a degraded barrier

### Quick override

```bash
# Recommended tier (16 GB machine)
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 ./test_pipeline.sh --mode native --fresh

# Minimum tier (tight defaults, works everywhere)
./test_pipeline.sh --mode native --fresh

# Pro tier (64 GB+ machine)
CARGO_BUILD_JOBS=8 RAYON_NUM_THREADS=16 ./test_pipeline.sh --mode native --fresh
```

The pipeline (`phase_02_build.sh`) forwards `CARGO_BUILD_JOBS` and `RAYON_NUM_THREADS`
from the host environment into the Docker build via `--build-arg`. The Dockerfile
converts these ARGs to ENVs, and all `cargo build` commands use `-j ${CARGO_BUILD_JOBS}`
so the override takes full effect.

## Related Documents

- [Build Resource Tuning](build-resource-tuning.md) — practical guide, memory model, overrides
- [Testing Overview](overview.md) — test levels and pipeline architecture
- [Dockerfile](../../../contrib/docker/darkwow-testnet/Dockerfile) — ARG/ENV definitions
- [phase_02_build.sh](../../../contrib/docker/darkwow-testnet/lib/phase_02_build.sh) — pipeline forwarding
