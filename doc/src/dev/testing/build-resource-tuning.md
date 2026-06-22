# Build Resource Tuning

The Docker pipeline compiles 31 WASM contracts plus a native daemon inside a container.
A `--fresh` build compiles everything from scratch. Without resource controls, this can
exceed available RAM and trigger an OOM kill (exit code 137).

This document explains how to tune the build for your hardware — from a 4 GB Raspberry Pi
to a 64-core server.

## Quick Reference

| Tier | Hardware | RAM | Cores | `CARGO_BUILD_JOBS` | `RAYON_NUM_THREADS` | Peak RAM | Fresh Build |
|------|----------|-----|-------|---------------------|----------------------|----------|-------------|
| **Minimum** | Low-end VPS, RPi | 4 GB | 2–4 | 1 | 2 | 1.2 GB | 50–70 min |
| **Recommended** | Dev laptop/desktop | 16 GB | 8–12 | 4 | 4 | ~4.8 GB | 30–45 min |
| **Pro/Server** | Threadripper, CI runner | 64 GB+ | 16–64 | 8 | 8 | ~9.6 GB | 15–25 min |

The committed defaults (`JOBS=1`, `RAYON=2`) match the Minimum tier and work universally.

## The Memory Model

Each parallel cargo job consumes approximately 1.2 GB for WASM contract compilation.
Each Rayon codegen thread consumes approximately 600 MB. The LTO linking step for the
native daemon uses approximately 4 GB per job.

```
Peak_RAM ≈ (CARGO_BUILD_JOBS × 1.2 GB) + (RAYON_NUM_THREADS × 0.6 GB) + 0.5 GB
```

This must stay below available RAM or Docker will OOM-kill the build. Running `docker stats`
in another terminal during a build will show live memory usage.

## The Formula

**CARGO_BUILD_JOBS**: `floor(available_RAM_GB / 1.2)`, capped at 8 (diminishing returns beyond 8 due to filesystem contention).

**RAYON_NUM_THREADS**: `min(floor(available_RAM_GB / 0.6), physical_cores)`.

**Final native `-j` flag**: `min(floor(available_RAM_GB / 4), CARGO_BUILD_JOBS)`. The native
daemon build uses LTO linking which is significantly more memory-intensive per job.

### Worked Example — 16 GB, 8-core machine

```
CARGO_BUILD_JOBS      = floor(16 / 1.2) = 4
RAYON_NUM_THREADS     = min(floor(16 / 0.6), 8) = 4
Final -j              = min(floor(16 / 4), 4) = 2
Peak_RAM              = (4 × 1.2) + (4 × 0.6) + 0.5 = ~7.7 GB  ✓
```

### Common Configurations

| Cores | RAM | JOBS | RAYON | Final -j | Peak RAM |
|-------|-----|------|-------|----------|----------|
| 2 | 4 GB | 1 | 2 | 1 | 1.2 GB |
| 4 | 8 GB | 3 | 4 | 1 | 3.6 GB |
| 8 | 16 GB | 4 | 8 | 2 | 4.8 GB |
| 16 | 32 GB | 8 | 16 | 4 | 9.6 GB |
| 32 | 64 GB | 8 | 32 | 4 | 9.6 GB |
| 64 | 128 GB | 8 | 64 | 4 | 9.6 GB |

Note the cap at JOBS=8 — beyond this, the target directory lock becomes the bottleneck,
not CPU or RAM.

## Hardware Tiers

### Minimum (4 GB, 2–4 cores)

Use the committed defaults. No override needed. These values are baked into the Dockerfile
and work on any machine with 2 GB or more of free RAM. Builds are slow but reliable.

### Recommended (16 GB, 8–12 cores)

This is the sweet spot for daily development. Override to JOBS=4, RAYON=4. Fresh builds
complete in 30–45 minutes. Cached builds in 10–15 minutes.

### Pro/Server (64 GB+, 16–64 cores)

Push JOBS to 8 (the practical cap) and RAYON to match core count. Fresh builds complete
in 15–25 minutes. Beyond JOBS=8 there are diminishing returns from filesystem contention
on the `target/` directory — an NVMe RAID can help here but the benefit is marginal.

## How to Override

The Dockerfile uses `ARG` with defaults, so you can override at build time without
editing the file.

### Method 1: `--build-arg` (one-off)

```bash
docker build \
  --build-arg CARGO_BUILD_JOBS=4 \
  --build-arg RAYON_NUM_THREADS=4 \
  -t darkwow-testnet . \
  -f contrib/docker/darkwow-testnet/Dockerfile
```

### Method 2: `docker-compose.override.yml` (permanent)

Create `contrib/docker/darkwow-testnet/docker-compose.override.yml`:

```yaml
services:
  lilith:
    build:
      args:
        CARGO_BUILD_JOBS: "4"
        RAYON_NUM_THREADS: "4"
  node0:
    build:
      args:
        CARGO_BUILD_JOBS: "4"
        RAYON_NUM_THREADS: "4"
  node1:
    build:
      args:
        CARGO_BUILD_JOBS: "4"
        RAYON_NUM_THREADS: "4"
```

Every service that builds from the Dockerfile needs the override — Docker Compose
does not have a global build-arg injection.

### Method 3: Environment variables

```bash
export CARGO_BUILD_JOBS=4
export RAYON_NUM_THREADS=4
./test_pipeline.sh --mode native --fresh
```

The pipeline (`phase_02_build.sh`) forwards these env vars into the Docker build
via `--build-arg`. Without this forwarding (added after HAZOP analysis), setting
env vars on the host had zero effect on the Docker build. This is now the
recommended method for persistent overrides.

**Important:** `-j` on the `cargo build` command line takes precedence over the
`CARGO_BUILD_JOBS` environment variable. All Dockerfiles now use `-j ${CARGO_BUILD_JOBS}`
(not a hardcoded number) so the env var controls the actual job count. If you ever
add a new `cargo build` to a Dockerfile, use `-j ${CARGO_BUILD_JOBS}`, never a
hardcoded `-j N`.

For a deeper analysis of how these valves interact and why small loosenings cascade,
see [Build Resource HAZOP](build-resource-hazop.md).

### Method 4: Edit the Dockerfile directly

The `ARG` defaults are at the top of the builder stage. Change them and rebuild.

## Verifying Your Settings

Check that the overrides took effect:

```bash
# In the build output, look for the env var values
grep -i "CARGO_BUILD_JOBS\|RAYON" build.log

# Monitor live memory during build
docker stats

# After build, verify no OOM kill
echo $?  # should be 0, not 137
```

## Troubleshooting OOM Kills

**Symptom**: Build output ends with `Killed` or exit code `137`.

1. **Drop to Minimum settings**: `CARGO_BUILD_JOBS=1`, `RAYON_NUM_THREADS=2`. This resolves 90% of OOM issues.
2. **Still OOM?** Reduce `RAYON_NUM_THREADS` to 1 first — it has the biggest RAM impact per unit.
3. **Still OOM?** Reduce both to 1. If this still OOMs on a 4 GB machine, free RAM may be below 2 GB — close other applications.
4. **Check Docker memory**: `docker info | grep -i memory`. Docker Desktop on macOS/Win may have a low RAM allocation in Preferences → Resources.
5. **Check system swap**: `free -h`. If swap is exhausted, the kernel will OOM-kill regardless of Docker settings.

## Runtime vs Build-Time Limits

The `deploy.resources.limits.memory` and `cpus` settings in `docker-compose.yml` apply
to **running containers** (runtime), not to the **build stage**. They do not affect
compilation memory usage. To limit build-stage RAM, use Docker BuildKit:

```bash
DOCKER_BUILDKIT=1 docker build --memory=8g --memory-swap=8g ...
```

## Current Defaults (committed in Dockerfile)

```
ARG CARGO_BUILD_JOBS=1
ARG RAYON_NUM_THREADS=2
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
ENV RAYON_NUM_THREADS=${RAYON_NUM_THREADS}
ENV RUST_MIN_STACK=67108864
```

These values are safe for any machine with 2 GB or more of free RAM. They were chosen
after a HAZOP analysis of OOM kills traced to compounding resource demands from
independent "optimizations" that drifted upward over time. The defaults are intentionally
conservative — developers with more hardware should override upward per this guide.
