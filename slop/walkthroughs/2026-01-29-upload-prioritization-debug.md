# Walkthrough: Upload Throttling, Distance Prioritization, and Debug Logging

**Date:** 2026-01-29
**Status:** Planning
**Checkpoint:** 5ed282a0f1add81f14be8aeb0246edfb7f6120ed

## Goal

Add configurable upload throttling, distance-based chunk prioritization, and toggle-able debug logging to improve terrain loading performance and reduce console spam.

## Acceptance Criteria

- [ ] Upload throttling: `max_uploads_per_frame` export limits how many meshes upload per frame (default 8)
- [ ] Distance prioritization: Chunks nearest to camera load first
- [ ] Debug logging: `debug_logging` export toggles all godot_print! calls (default false)
- [ ] All existing tests pass
- [ ] Console is quiet with debug_logging=false

## Technical Approach

### Architecture

The terrain system has three main components:
- **terrain.rs**: Main Godot node, owns chunk_manager and worker_pool, handles mesh uploads
- **chunk_manager.rs**: Tracks chunk state, sends requests to workers, receives results
- **mesh_worker.rs**: Parallel mesh generation with rayon

Changes flow through all three files:
1. terrain.rs adds new exports and passes debug flag down
2. chunk_manager.rs receives debug flag, sorts chunks by distance
3. mesh_worker.rs receives debug flag for conditional logging

### Key Decisions

- **Upload throttle default=8**: Conservative to maintain framerate; users can tune via Godot inspector
- **Sort by distance squared**: Avoids sqrt() cost, still produces correct ordering
- **debug_logging as bool**: Simple toggle, no log levels needed for this use case

### Dependencies

- No new external crates
- Uses existing: crossbeam, rayon, godot

### Files to Create/Modify

- `rust/src/terrain.rs`: Add exports, limit upload loop, pass debug flag
- `rust/src/chunk_manager.rs`: Accept debug flag, sort by distance, conditional logging
- `rust/src/mesh_worker.rs`: Accept debug flag, conditional logging

## Build Order

1. **mesh_worker.rs**: Add debug flag to constructor first (dependency for terrain.rs)
2. **chunk_manager.rs**: Add debug flag and distance sorting (dependency for terrain.rs)
3. **terrain.rs**: Add exports and wire everything together

## Anticipated Challenges

- **Breaking constructor signatures**: Must update all call sites when adding debug parameter
- **Sorting performance**: HashMap iteration + sort could be slow with many chunks, but view distance limits chunk count

## Steps (To Be Filled During Proof Phase)

[This section will be populated after we build and verify the implementation]

---
*Plan created: 2026-01-29*
*Implementation proven: [to be updated]*
*User implementation started: [to be updated]*
