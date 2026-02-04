# Pixy Terrain Code Review
Last Updated: 2026-02-04

## Active Monitors

| Agent | Files | Status | Last Review |
|-------|-------|--------|-------------|
| AGENT-1 | terrain.rs, lib.rs | ACTIVE | 2026-02-01 12:00 |
| AGENT-2 | noise_field.rs, chunk.rs | ACTIVE | 2026-02-01 12:00 |
| AGENT-3 | mesh_worker.rs, mesh_extraction.rs | ACTIVE | 2026-02-01 12:00 |
| AGENT-4 | chunk_manager.rs, editor_plugin.rs | ACTIVE | 2026-02-01 12:00 |
| AGENT-5 | Cargo.toml, mesh_postprocess.rs | ACTIVE | 2026-02-01 12:00 |

## Summary

Comprehensive review against ARCHITECTURE.md and Godot integration. This review file was cleaned on 2026-02-01 to remove items that have been resolved by recent commits.

---

## RESOLVED: Issues Fixed in Recent Commits

### FIX 1: CSG Intersection Gradient Discontinuity
**Location:** `rust/src/noise_field.rs:116-124`
**Status:** RESOLVED in commit 64daa3d

The `smooth_max()` function was implemented to replace hard `max(a, b)` CSG intersection:
```rust
fn smooth_max(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 { return a.max(b); }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    a * (1.0 - h) + b * h + k * h * (1.0 - h)
}
```
This provides C1 continuous gradients at terrain-wall junctions, eliminating normal artifacts.

---

### FIX 2: Decimation Magic Number
**Location:** `rust/src/terrain.rs:133`
**Status:** RESOLVED in commit 41d9748

The hardcoded `1000` for decimate target is now an `#[export]` variable:
```rust
#[export]
decimation_target_triangles: i32,
```
Editor plugin now uses this exported property instead of hardcoded value.

---

### FIX 3: Box Geometry Mode Replaced by SDF Enclosure
**Location:** `rust/src/box_geometry.rs`
**Status:** RESOLVED - File deleted

The separate `box_geometry.rs` module has been removed. Wall generation now uses SDF enclosure mode exclusively:
- `use_sdf_enclosure` mode generates watertight walls via CSG intersection
- Single mesh with shared vertices at wall-terrain junction
- No more alignment/tessellation mismatch issues

---

### FIX 4: Guard Chunks for Wall Generation
**Location:** `rust/src/chunk_manager.rs:103-119`
**Status:** RESOLVED in commit 68c4da2

Guard chunks are now allowed on all boundaries for proper wall generation:
```rust
if coord.x < -1 || coord.x > self.map_width { continue; }
if coord.z < -1 || coord.z > self.map_depth { continue; }
if coord.y < -1 || coord.y >= self.map_height { continue; }
```

---

### FIX 5: Triplanar Shader for UV Scaling
**Location:** `rust/src/shaders/triplanar_pbr.gdshader`
**Status:** RESOLVED in commit b3e7db1

StandardMaterial3D's `set_uv1_scale()` had no effect on transvoxel meshes (no UVs). Replaced with triplanar ShaderMaterial that projects textures using world-space coordinates.

---

## REMAINING: Open Issues

### BUG 1: Typo in Variable Name
**Location:** `rust/src/chunk_manager.rs:218`
**Severity:** LOW
**Status:** RESOLVED — Renamed `neigbor_coord` to `neighbor_coord`.

---

### BUG 2: Magic Numbers in FBM Configuration
**Location:** `rust/src/noise_field.rs:5-6`
**Severity:** LOW
**Status:** RESOLVED — Already extracted to `FBM_PERSISTENCE` and `FBM_LACUNARITY` constants by previous cleanup.

---

### BUG 3: No Unit Tests for mesh_postprocess.rs
**Location:** `rust/src/mesh_postprocess.rs`
**Severity:** MEDIUM
**Status:** RESOLVED — Added 13 tests covering merge_chunks, weld_vertices, recompute_normals, remove_unused_vertices, decimate, and the full process pipeline.

---

### BUG 4: dylib Self-Referential Dependency (macOS)
**Location:** `rust/.cargo/config.toml`
**Severity:** MEDIUM
**Status:** RESOLVED — Created `.cargo/config.toml` with `@rpath` install_name for both aarch64 and x86_64 macOS targets.

---

### BUG 5: Missing macOS x86_64 Entries in .gdextension
**Location:** `godot/pixy_terrain.gdextension`
**Severity:** LOW
**Status:** RESOLVED — Added `macos.debug.x86_64` and `macos.release.x86_64` entries.

---

### ARCHITECTURAL: Normal Artifacts at Sharp Ridges
**Location:** `rust/src/mesh_postprocess.rs`
**Severity:** MEDIUM
**Status:** DEFERRED — Requires smooth groups, vertex splitting at ridges, and area-weighted normals. Tracked as separate future task.

---

### ARCHITECTURAL: Real-Time Seam Visibility
**Location:** `rust/src/terrain.rs`
**Severity:** MEDIUM
**Status:** OPEN (By Design)

Per-chunk uploads have no seam coordination. Each chunk computes normals independently.

**Current behavior:** Seams visible during real-time editing
**Export behavior:** `merge_and_export()` applies full post-processing pipeline, fixing seams

This is acceptable for the current workflow (edit -> export).

---

## Code Quality Summary

| Module | Tests | Magic Numbers | Documentation | Overall |
|--------|-------|---------------|---------------|---------|
| terrain.rs | N/A | GOOD (exports) | GOOD | 9/10 |
| noise_field.rs | PASS | GOOD (constants) | GOOD | 9/10 |
| chunk_manager.rs | PASS | GOOD | GOOD | 8/10 |
| mesh_postprocess.rs | PASS (13 tests) | GOOD | GOOD | 9/10 |
| mesh_extraction.rs | PASS | GOOD | GOOD | 9/10 |
| editor_plugin.rs | N/A | GOOD | MEDIUM | 8/10 |
| voxel_grid.rs | PASS (4 tests) | GOOD | GOOD | 9/10 |

---

## Recent Commit Review Log

### Commit: b3e7db1 - Triplanar Shader
**Date:** 2026-02-01
**Risk Level:** Low
**Status:** REVIEWED - APPROVED

Adds triplanar PBR shader for proper texture projection on UV-less transvoxel meshes.

---

### Commit: 41d9748 - Decimate Export Parameter
**Date:** 2026-02-01
**Risk Level:** Low
**Status:** REVIEWED - APPROVED

Decimation target now uses exported property instead of hardcoded value.

---

### Commit: 64daa3d - Smooth CSG Intersection
**Date:** 2026-02-01
**Risk Level:** Medium
**Status:** REVIEWED - APPROVED

Fixed smooth_max interpolation weights. Critical fix for normal artifacts at CSG seams.

---

### Commit: 68c4da2 - Guard Chunks
**Date:** 2026-02-01
**Risk Level:** Medium
**Status:** REVIEWED - APPROVED

Enabled wall generation by allowing guard chunks on all boundaries.

---

### AGENT-1 Check: 2026-02-01 (Uncommitted)
**Files:** terrain.rs, lib.rs
**Status:** CHANGES DETECTED
**Findings:**

**Summary:** Cross-section clipping feature added to terrain.rs and triplanar_pbr.gdshader

**Changes Reviewed:**
1. New `#[export_group(name = "Cross-Section")]` with 7 new export variables
2. Shader parameters passed from terrain.rs to triplanar shader
3. Shader implements plane clipping with `discard` for fragments below clip plane
4. Underground texture rendering on back faces using `!FRONT_FACING`

**Checklist Results:**

| Check | Status | Notes |
|-------|--------|-------|
| No magic numbers | PASS | All values use `#[export]` variables with sensible defaults |
| `#[export]` defaults sensible | PASS | Default normals `(0,1,0)`, scales `1.0`, disabled by default |
| No blocking operations | PASS | Only sets shader parameters, no I/O |
| Proper resource management | PASS | Uses `Option<Gd<Texture2D>>` pattern |
| GDExtension patterns | PASS | Correct use of `#[export_group]`, `#[export]`, `#[init]` |
| Clear naming | PASS | Consistent `clip_*` prefix, descriptive names |

**Shader Review:**
- `cull_disabled` render mode allows back-face rendering for underground texture
- `FRONT_FACING` builtin correctly used to differentiate interior vs exterior
- Normal flipping (`NORMAL = -NORMAL`) ensures correct lighting on cut surfaces
- Magic number `0.9` roughness for underground (line 68) - MINOR, acceptable for internal surface

**No Issues Found.** Ready to commit.

---

## Test Results

All 103 tests pass after cleanup:
- chunk_manager: 7 tests
- mesh_worker: 6 tests
- brush: 46 tests
- chunk: 3 tests
- noise_field: 7 tests
- terrain_modifications: 6 tests
- texture_layer: 4 tests
- terrain_stability: 4 tests
- voxel_grid: 4 tests (NEW)
- mesh_postprocess: 13 tests (NEW)
- undo: 3 tests

```bash
cd rust && cargo test
# Running unittests src/lib.rs
# test result: ok. 103 passed; 0 failed; 0 ignored
```

---

## Architecture Compliance Checklist

| Check | Status | Notes |
|-------|--------|-------|
| No magic numbers | PASS | ~50 extracted to constants; remaining are UI/editor (see MN-4,5,6,7,8,9 below) |
| Functions single-purpose | PASS | |
| No dead code | PASS | ~50 items removed; unused bevy_tasks removed; repair_manifold stub removed |
| DRY / No WET code | PASS | 8 of 11 patterns resolved (WET-1–9 except WET-10); see notes below |
| No unsafe without justification | PASS | meshopt FFI documented |
| Chunk boundaries respected | PASS | Guard chunks working |
| Noise uses 2D sampling | PASS | `fbm.get([x, z])` |
| Wall normals outward | PASS | Via SDF gradient |
| Watertight seams | PARTIAL | Export pipeline only |
| Mesh uses worker threads | PASS | Rayon parallel |
| No blocking in render | PASS | Async mesh gen |

---

## 2026-02-04: Dead Code, WET Patterns, Magic Numbers Review

**Scope:** `rust/src/` (16 files)
**Branch:** `checkpoint/stencil-cap-working`
**Focus:** Dead code, WET/duplication, magic numbers

---

### DEAD CODE — RESOLVED

Most dead code items (~50) were removed by previous cleanup agent. Additional items resolved in this pass:

| Item | Status | Notes |
|------|--------|-------|
| DC-1: mesh_worker.rs unused imports | RESOLVED | Removed `TaskPool, TaskPoolBuilder` imports; removed `bevy_tasks` dependency entirely |
| DC-2: Superseded brush methods | RESOLVED | Removed by previous agent |
| DC-3: terrain_modifications.rs | RESOLVED | Rewritten to use `VoxelGridConfig` + `SparseChunkData<T>`; dead methods removed |
| DC-4: texture_layer.rs | RESOLVED | Rewritten to use `VoxelGridConfig` + `SparseChunkData<T>`; dead methods removed |
| DC-5: chunk_manager.rs | RESOLVED | Removed by previous agent |
| DC-6: mesh_postprocess.rs `repair_manifold` | RESOLVED | Removed empty stub and its call site |
| DC-6: mesh_postprocess.rs `weld_epsilon` | RESOLVED | Removed by previous agent |
| DC-6: Other files | RESOLVED | Removed by previous agent |

---

### WET CODE — MOSTLY RESOLVED (8 of 11 patterns)

| Pattern | Status | Notes |
|---------|--------|-------|
| WET-1: Five identical commit methods | RESOLVED | Extracted `commit_sdf_modification` (previous agent) |
| WET-2: Alpha-compositing + direction filter | RESOLVED | Extracted `Brush::composite_and_write` (this pass) |
| WET-3: Brush settings sync | RESOLVED | Extracted `sync_brush_settings` (previous agent) |
| WET-4: ArrayMesh construction | RESOLVED | Extracted `build_array_mesh` (previous agent) |
| WET-5: Cross-section shader params | RESOLVED | Extracted `set_cross_section_params` (previous agent) |
| WET-6: Y-range setup | RESOLVED | Extracted `Brush::compute_y_scan_range` (this pass) |
| WET-7: world_to_chunk duplication | RESOLVED | Created shared `VoxelGridConfig` in `voxel_grid.rs` (this pass) |
| WET-8: ChunkMods/ChunkTextures wrappers | RESOLVED | Created generic `SparseChunkData<T>` in `voxel_grid.rs` (this pass) |
| WET-9: Undo/redo | RESOLVED | Extracted `apply_history_action` (previous agent) |
| WET-10: Trilinear interpolation | DEFERRED | Same loop structure but different accumulation types; generic extraction would add more complexity than it removes |
| WET-11: Node cleanup | RESOLVED | Extracted `free_all_chunk_nodes` (previous agent) |

---

### MAGIC NUMBERS — MOSTLY RESOLVED

HIGH priority items resolved by previous cleanup agent and this pass:

| Item | Status | Notes |
|------|--------|-------|
| MN-1: brush.rs epsilons/thresholds | RESOLVED | Extracted `MIN_BLEND_THRESHOLD`, `CHANGE_EPSILON`, `Y_PADDING_VOXELS`, etc. (previous agent) |
| MN-2: FBM persistence duplication | RESOLVED | Extracted `FBM_PERSISTENCE` and `FBM_LACUNARITY` (previous agent) |
| MN-3: Default texture color | RESOLVED | Shared `DEFAULT_TEXTURE_COLOR` in `chunk.rs`, imported by terrain.rs and mesh_extraction.rs (this pass) |
| MN-4: UI/editor constants | RESOLVED | Extracted to named constants (previous agent) |
| MN-5: Brush defaults | RESOLVED | Extracted to named constants (previous agent) |
| MN-6: Worker/mesh constants | RESOLVED | Extracted to named constants (previous agent) |
| MN-7: Preview/visual constants | RESOLVED | Extracted to named constants (previous agent) |
| MN-8: Terrain stability constants | RESOLVED | Extracted to named constants (previous agent) |
| MN-9: Render priority | RESOLVED | Extracted to named constants (previous agent) |
