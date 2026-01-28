# Walkthrough 01: Dependencies and Project Setup

**Series:** Transvoxel Noise Terrain
**Status:** Planning
**Prerequisites:** None

## Goal

Configure Cargo.toml with all dependencies needed for the transvoxel terrain system.

## Acceptance Criteria

- [ ] All dependencies added to Cargo.toml
- [ ] `cargo check` passes without errors
- [ ] `cargo build` compiles the extension

## Dependencies Overview

| Crate | Version | Purpose |
|-------|---------|---------|
| `transvoxel` | 0.5 | Isosurface mesh extraction with LOD transitions |
| `noise` | 0.9 | 3D Perlin/Fbm noise for terrain SDF |
| `bevy_tasks` | 0.15 | Parallel task pool with low idle CPU |
| `crossbeam` | 0.8 | Thread-safe channels for worker communication |
| `num_cpus` | 1.16 | Detect CPU cores for thread pool sizing |

## Why bevy_tasks over rayon?

Rayon uses busy-loop polling (`thread::yield_now()`) while waiting for work, causing constant CPU usage even when idle. bevy_tasks uses `async-executor` with proper thread parking - threads truly sleep when no work is available.

Benchmark results from Bevy's migration:
- **Debug mode:** 900% → 110% CPU usage
- **Release mode:** 450% → 40% CPU usage

## Steps

### Step 1: Open Cargo.toml

**File:** `rust/Cargo.toml`

Current state:
```toml
[dependencies]
godot = { git = "https://github.com/godot-rust/gdext", branch = "master" }

# For transvoxel algorithm
# transvoxel = "0.5"  # Uncomment when ready to implement
```

### Step 2: Add All Dependencies

```toml
# Full path: rust/Cargo.toml

[dependencies]
godot = { git = "https://github.com/godot-rust/gdext", branch = "master" }

# Mesh generation - transvoxel algorithm for smooth isosurfaces with LOD
transvoxel = "0.5"

# Noise generation - Fbm/Perlin for terrain density field
noise = "0.9"

# Parallelization - low idle CPU via proper thread parking (not rayon!)
bevy_tasks = { version = "0.15", default-features = true }

# Thread-safe channels for worker-to-main-thread communication
crossbeam = "0.8"

# CPU detection for thread pool sizing
num_cpus = "1.16"
```

### Step 3: Verify Compilation

```bash
cd rust && cargo check
```

Expected output: No errors, dependencies download and resolve.

```bash
cargo build
```

Expected output: Library compiles successfully.

## Verification Checklist

- [ ] `cargo check` completes without errors
- [ ] `cargo build` produces `libpixy_terrain.dylib`
- [ ] No version conflicts between dependencies

## What's Next

With dependencies in place, we'll create the core data structures in walkthrough 02.

## Notes

The `bevy_tasks` crate pulls in only `bevy_platform` for platform abstractions - NOT the full Bevy engine. Total transitive dependencies ~10 crates, comparable to crossbeam alone.
