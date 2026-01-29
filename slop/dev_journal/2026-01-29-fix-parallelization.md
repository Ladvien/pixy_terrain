# Dev Journal: 2026-01-29 - Fix Parallelization Performance

**Session Duration:** ~2 hours
**Walkthrough:** `slop/walkthroughs/09-fix-parallelization-performance.md`

## What We Did

Investigated and fixed multiple parallelization issues in the terrain mesh generation system. The terrain was generating "one piece at a time" taking ~10 minutes instead of using multiple CPU cores.

## Bugs & Challenges

### Bug 1: bevy_tasks Only Creating 1 Thread

**Symptom:** Debug output showed `TaskPool created with 1 threads` despite requesting 7.

**Initial Hypothesis:** Something wrong with our thread count calculation.

**Investigation:**
- Added debug output showing CPU detection, requested threads, and actual threads
- Created a test that directly called `TaskPoolBuilder` - still got 1 thread
- Checked bevy_tasks Cargo.toml features

**Root Cause:** The `multi_threaded` feature is NOT included in bevy_tasks default features!

**Solution:** Changed Cargo.toml from:
```toml
bevy_tasks = { version = "0.18", default-features = true }
```
to:
```toml
bevy_tasks = { version = "0.18", features = ["multi_threaded"] }
```

**Lesson:** Always check feature flags for threading crates. "default-features = true" doesn't mean all features are enabled.

### Bug 2: spawn().detach() Tasks Never Running

**Symptom:** Tasks spawned with `spawn().detach()` showed `completed: 0` forever.

**Initial Hypothesis:** The tasks were being cancelled.

**Investigation:**
- Added atomic counters for spawned/completed tasks
- Found that tasks were spawned but never completed
- Researched bevy_tasks executor model

**Root Cause:** `spawn().detach()` creates detached tasks, but without Bevy's full runtime, there's no executor polling them. Tasks need `scope()` to actually run.

**Solution:** Switched back to `scope()` but with batching to avoid blocking too long per frame.

### Bug 3: Requests Lost When Batching

**Symptom:** Only first batch of chunks generated, rest stuck in "pending" forever.

**Root Cause:** We drained ALL requests from the channel but only processed `batch_size` of them. The rest were lost.

**Solution:** Only take `batch_size` from the channel, leaving rest for next frame:
```rust
while batch.len() < batch_size {
    match self.request_rx.try_recv() {
        Ok(req) => batch.push(req),
        Err(_) => break,
    }
}
```

### Bug 4: bevy_tasks scope() Not Parallelizing (htop showed 1 core)

**Symptom:** Even with multi_threaded feature, htop showed only 1 CPU core active.

**Investigation:**
- Added thread ID tracking inside tasks
- bevy_tasks reported 7 threads but work wasn't distributed

**Solution:** Switched from bevy_tasks to **rayon** for parallel processing:
```rust
rayon::scope(|scope| {
    for request in batch {
        let tx = result_tx.clone();
        scope.spawn(move |_| {
            let mesh = generate_mesh_for_request(&request);
            let _ = tx.try_send(mesh);
        });
    }
});
```

**Lesson:** bevy_tasks is designed for Bevy's async runtime. For standalone CPU-bound parallel work, rayon is more reliable.

## Code Changes Summary

- `Cargo.toml`: Added `multi_threaded` feature to bevy_tasks, added rayon dependency
- `rust/src/mesh_worker.rs`:
  - Added `channel_capacity` parameter to `new()`
  - Changed from `spawn().detach()` to `scope()` with batching
  - Switched from bevy_tasks scope to rayon scope
  - Added debug output for thread usage tracking
- `rust/src/chunk_manager.rs`: Increased `max_results_per_update` from 8 to 64
- `rust/src/terrain.rs`: Added `channel_capacity` export variable

## Patterns Learned

- **Feature Flag Gotcha**: Threading crates often have threading behind feature flags, not defaults
- **Async vs Parallel**: bevy_tasks is async (cooperative), rayon is truly parallel (work-stealing)
- **Channel Batching**: When batching from channels, only take what you'll process - don't drain and discard
- **Thread Tracking**: Use `std::thread::current().id()` with a HashSet to verify actual thread distribution

## Metrics

Before fixes:
- 1 thread, ~10 minutes for full terrain

After fixes:
- 7-10 threads actively used per batch
- ~944 meshes generated per second
- ~2.5 minutes estimated for 137k chunks

## Open Questions

- Why does htop show low CPU even when 10 threads are verified in use? (Likely: fast tasks complete before htop samples)
- Upload bottleneck: 898 ready but only 46 active after 1 second - investigate Godot mesh upload path
- Should we add distance-based prioritization to generate closest chunks first?

## Next Session

1. Investigate upload bottleneck (ready >> active)
2. Consider distance prioritization for better perceived performance
3. Clean up debug output before committing final version
