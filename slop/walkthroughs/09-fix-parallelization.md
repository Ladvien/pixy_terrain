# Walkthrough 09: Fix Parallelization Performance

**Series:** Transvoxel Noise Terrain
**Status:** Planning
**Checkpoint:** 77068a8
**Prerequisites:** Walkthrough 08 (Godot integration)

## Goal

Fix the ~2 FPS performance issue caused by `bevy_tasks::scope()` blocking the main thread until ALL mesh generation tasks complete.

## Acceptance Criteria

- [ ] Replace blocking `scope()` with fire-and-forget `spawn()`
- [ ] Increase `max_results_per_update` to handle async result flow
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes
- [ ] Godot runs at ~60 FPS during chunk generation

## Technical Approach

### Architecture

The current flow:
1. `ChunkManager::update()` sends mesh requests via channel
2. `MeshWorkerPool::process_requests()` receives requests and spawns tasks
3. **PROBLEM**: `scope()` blocks until ALL tasks complete
4. Results sent back via channel to main thread

The fix changes step 3 to use non-blocking `spawn()`.

### Key Decisions

- **spawn() over scope()**: `spawn()` returns immediately, letting the main thread continue rendering while tasks run in background
- **Increase results cap**: With async spawning, results arrive continuously - need to process more per frame

### Files to Modify

- `rust/src/mesh_worker.rs`: Replace `scope()` with `spawn()`
- `rust/src/chunk_manager.rs`: Increase `max_results_per_update` from 8 to 16

## The Problem Explained

Current code in `mesh_worker.rs`:

```rust
self.pool.scope(|scope| {
    for request in requests {
        let tx = result_tx.clone();
        scope.spawn(async move {
            let mesh = generate_mesh_for_request(&request);
            let _ = tx.try_send(mesh);
        })
    }
});  // <-- BLOCKS HERE until ALL tasks finish!
```

If 50 chunks need meshes and each takes 50ms, the main thread stalls for 2.5 seconds.

## Steps

### Step 1: Understand scope() vs spawn()

| Aspect | `scope()` | `spawn()` |
|--------|-----------|-----------|
| Returns | After ALL tasks done | Immediately |
| Main thread | Blocked | Free to render |
| Results | All at once | Streamed via channel |
| FPS impact | Drops to ~2 | Stays at ~60 |

### Step 2: Modify process_requests()

**File:** `rust/src/mesh_worker.rs`

Change the `process_requests` method to use `spawn()` instead of `scope()`.

### Step 3: Increase Results Cap

**File:** `rust/src/chunk_manager.rs`

Change `max_results_per_update` from 8 to 16.

## Verification

1. `cd rust && cargo build`
2. `cd rust && cargo test`
3. Run in Godot - should maintain ~60 FPS

---
*Plan created: 2026-01-29*
