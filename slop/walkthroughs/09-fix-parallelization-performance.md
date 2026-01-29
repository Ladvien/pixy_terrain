# Walkthrough 09: Fix Parallelization Performance

**Date:** 2026-01-29
**Status:** Planning
**Checkpoint:** 95fde15
**Prerequisites:** Walkthrough 08 (Godot integration)

## Goal

Fix terrain generation bottlenecks so chunks generate in parallel at full speed instead of "one at a time" over 10 minutes.

## Acceptance Criteria

- [ ] Channel buffers increased from 64 to 256
- [ ] Thread count uses 3/4 of CPUs instead of 1/2
- [ ] Results per frame increased from 16 to 64
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes
- [ ] Terrain fills in significantly faster in Godot

## Technical Approach

### Architecture

The mesh generation pipeline:
1. `ChunkManager::update()` sends `MeshRequest` via channel
2. `MeshWorkerPool::process_requests()` receives requests, spawns tasks on thread pool
3. Worker threads run `extract_chunk_mesh()` (CPU-intensive)
4. Results sent back via channel
5. `ChunkManager::update()` receives results (capped per frame)
6. Main thread uploads meshes to Godot

### Bottlenecks Identified

| Bottleneck | Current | Problem |
|------------|---------|---------|
| Channel size | 64 | Only 64 requests/results can queue |
| Thread count | `num_cpus/2` | Could be 1-2 threads |
| Results/frame | 16 | Limits upload speed even if meshes ready |

### Files to Modify

- `rust/src/mesh_worker.rs`: Channel sizes + thread count
- `rust/src/chunk_manager.rs`: Results per frame cap

## Build Order

1. **Increase channel sizes**: Allows more requests to queue
2. **Increase thread count**: More parallel mesh generation
3. **Increase results cap**: Faster mesh uploads to Godot

## Steps

### Step 1: Increase Channel Buffers

**File:** `rust/src/mesh_worker.rs` (lines 46-47)

**Current:**
```rust
let (request_tx, request_rx) = bounded(64);
let (result_tx, result_rx) = bounded(64);
```

**Change to:**
```rust
let (request_tx, request_rx) = bounded(256);
let (result_tx, result_rx) = bounded(256);
```

### Step 2: Increase Thread Count

**File:** `rust/src/mesh_worker.rs` (lines 35-39)

**Current:**
```rust
let threads = if num_threads == 0 {
    (num_cpus::get() / 2).max(1)
} else {
    num_threads
};
```

**Change to:**
```rust
let threads = if num_threads == 0 {
    ((num_cpus::get() * 3) / 4).max(2)
} else {
    num_threads
};
```

### Step 3: Increase Results Per Frame

**File:** `rust/src/chunk_manager.rs` (line 37)

**Current:**
```rust
max_results_per_update: 16,
```

**Change to:**
```rust
max_results_per_update: 64,
```

## Verification

1. `cd rust && cargo build && cargo test`
2. Open Godot TestScene
3. Observe terrain filling in faster
4. Check FPS stays near 60

---
*Plan created: 2026-01-29*
