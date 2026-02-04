# 2026-02-04: Review Notes Cleanup

**Branch:** `fix/review-notes-cleanup` (based off `transvoxel_v0_2`)
**Commit:** `27fdc79`

## What Was Done

Resolved all remaining items from `review_notes.md` that a previous cleanup agent left unfinished.

### Phase 1: Trivial Fixes
- Fixed typo `neigbor_coord` → `neighbor_coord` in chunk_manager.rs
- Removed unused `bevy_tasks` dependency entirely (TaskPool/TaskPoolBuilder imports were dead)
- Removed `repair_manifold` empty stub and its call site in mesh_postprocess.rs
- Moved `DEFAULT_TEXTURE_COLOR` to chunk.rs as shared constant (was duplicated in terrain.rs and mesh_extraction.rs)
- Fixed pre-existing borrow checker error in terrain.rs using `self.brush.take()` pattern

### Phase 2: Infrastructure
- Created `rust/.cargo/config.toml` with macOS `@rpath` install_name for aarch64 and x86_64
- Added `macos.debug.x86_64` and `macos.release.x86_64` entries to `.gdextension`

### Phase 3: WET Refactors in brush.rs
- Extracted `compute_y_scan_range()` — replaces 3 identical Y-range setup blocks in apply_flatten, apply_plateau, apply_smooth
- Extracted `composite_and_write()` — replaces 3 identical alpha-compositing + direction filter blocks in apply_flatten, apply_plateau, apply_slope

### Phase 4: Shared VoxelGrid Module
- Created `rust/src/voxel_grid.rs` with:
  - `VoxelGridConfig` — shared world_to_chunk / world_to_local_index (was duplicated in ModificationLayer and TextureLayer)
  - `SparseChunkData<T>` — generic HashMap<u32, T> wrapper (was duplicated as ChunkMods and ChunkTextures)
- Rewrote `terrain_modifications.rs` and `texture_layer.rs` to use shared types
- 4 new tests for voxel_grid.rs

### Phase 5: mesh_postprocess.rs Tests
- Added 13 unit tests covering merge_chunks, weld_vertices, recompute_normals, remove_unused_vertices, decimate, and full process pipeline

### Phase 6: Updated review_notes.md
- Marked all BUG 1-5 as RESOLVED
- Marked ARCHITECTURAL normal artifacts as DEFERRED
- Updated Dead Code, WET Code, Magic Numbers sections with resolution status
- Updated Code Quality Summary, Test Results, Architecture Compliance Checklist

## Issues Encountered

1. **Borrow checker in terrain.rs:** `self.sync_brush_settings(brush)` while brush was mutably borrowed from `self.brush`. Fixed with `self.brush.take()` to temporarily take ownership.

2. **Branch divergence:** `main` had moved to a tile-based system with many deleted files. Could not merge `transvoxel_v0_2` into main. Created feature branch off checkpoint instead.

3. **Orphaned code after partial edit:** After replacing alpha-compositing block with `composite_and_write` in apply_slope, leftover `if changed { ... }` lines remained. Had to manually clean up.

4. **WET-10 trilinear interpolation:** Decided to DEFER — the two trilinear loops share structure but differ in accumulation types (`(f32, f32)` vs `[f32; 4]` with normalization). A generic extractor would require a trait and add more complexity than it removes.

## Final State
- 103 tests pass (86 original + 4 voxel_grid + 13 mesh_postprocess)
- 32 clippy warnings (all pre-existing, none introduced)
- 15 files changed, 628 insertions, 619 deletions
