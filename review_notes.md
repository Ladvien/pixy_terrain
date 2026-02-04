# Pixy Terrain Code Review
Last Updated: 2026-02-01 12:00 CST

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
**Location:** `rust/src/chunk_manager.rs:200`
**Severity:** LOW
**Status:** OPEN

Variable `neigbor_coord` should be `neighbor_coord` for consistency and readability.

---

### BUG 2: Magic Numbers in FBM Configuration
**Location:** `rust/src/noise_field.rs:55-56`
**Severity:** LOW
**Status:** OPEN

`set_lacunarity(2.0)` and `set_persistence(0.5)` are hardcoded. Consider extracting to constants:
```rust
const DEFAULT_LACUNARITY: f64 = 2.0;
const DEFAULT_PERSISTENCE: f64 = 0.5;
```

---

### BUG 3: No Unit Tests for mesh_postprocess.rs
**Location:** `rust/src/mesh_postprocess.rs`
**Severity:** MEDIUM
**Status:** OPEN

No `#[cfg(test)]` section. Other modules have tests but this critical module lacks coverage.

Tests needed for:
- `merge_chunks` - empty input, single chunk, multiple chunks
- `weld_vertices` - coincident vertices, near-miss vertices
- `recompute_normals` - flat surface, ridge detection
- `decimate` - triangle count reduction
- `remove_unused_vertices` - orphan cleanup

---

### BUG 4: dylib Self-Referential Dependency (macOS)
**Location:** Compiled library
**Severity:** MEDIUM
**Status:** OPEN

The library has an absolute self-referential path that won't work on other machines.

**Fix needed:** Add to Cargo.toml:
```toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-Wl,-install_name,@rpath/libpixy_terrain.dylib"]
```

---

### BUG 5: Missing macOS x86_64 Entries in .gdextension
**Location:** `godot/pixy_terrain.gdextension`
**Severity:** LOW
**Status:** OPEN

Missing x86_64 entries for Intel Mac support.

---

### ARCHITECTURAL: Normal Artifacts at Sharp Ridges
**Location:** `rust/src/mesh_postprocess.rs:220-270`
**Severity:** MEDIUM
**Status:** OPEN (Partially Mitigated)

The `recompute_normals()` function lacks proper smooth groups for ridge detection. At ridges where faces from both sides meet, normals are averaged together incorrectly.

**Current mitigation:** The `smooth_max` CSG intersection reduces artifacts at terrain-wall junctions.

**Remaining issue:** True ridge lines within terrain (mountain peaks) still show artifacts. Fix requires:
1. Implement smooth groups via transitive face smoothness
2. Split vertices at ridge lines
3. Area-weight normal contributions

---

### ARCHITECTURAL: Real-Time Seam Visibility
**Location:** `rust/src/terrain.rs:373-439`
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
| noise_field.rs | PASS | LOW (2 items) | GOOD | 8/10 |
| chunk_manager.rs | PASS | GOOD | GOOD | 8/10 |
| mesh_postprocess.rs | MISSING | MEDIUM | MEDIUM | 6/10 |
| mesh_extraction.rs | PASS | GOOD | GOOD | 9/10 |
| editor_plugin.rs | N/A | GOOD | MEDIUM | 8/10 |

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

All 13 tests pass after recent fixes:
- chunk_manager: 7 tests pass
- mesh_worker: 6 tests pass

```bash
cd rust && cargo test
# Running unittests src/lib.rs
# test result: ok. 13 passed; 0 failed; 0 ignored
```

---

## Architecture Compliance Checklist

| Check | Status | Notes |
|-------|--------|-------|
| No magic numbers | FAIL | ~75 values across 10 files (see 2026-02-04 review) |
| Functions single-purpose | PASS | |
| No dead code | FAIL | ~60 items across 12 files (see 2026-02-04 review) |
| DRY / No WET code | FAIL | 11 duplication patterns found (see 2026-02-04 review) |
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

### DEAD CODE (~60 items across 12 files)

#### DC-1: Write-Only Debug Instrumentation (mesh_worker.rs)
- **Lines 19-25:** `TASKS_SPAWNED`, `TASKS_COMPLETED`, `THREADS_USED` statics are written to but never read
- **Line 43:** `TaskPool` field `pool` creates idle thread pool; rayon does actual work. Replace with `thread_count: usize`
- **Lines 7-8:** Unused imports `TrySelectError`, `godot_print`

#### DC-2: Superseded Brush Methods (brush.rs)
- **Line 131:** `BrushFootprint::contains` -- never called; `cells` is pub
- **Line 141:** `BrushFootprint::len` -- only tests; use `cells.len()`
- **Line 162:** `BrushFootprint::compute_centroid` -- superseded by `compute_stroke_falloff` (line 704)
- **Line 652:** `Brush::compute_falloff` -- superseded by `compute_stroke_falloff`

#### DC-3: terrain_modifications.rs (11 items)
- **Lines 81, 86:** `ChunkMods::clear`, `iter` -- never called
- **Lines 142, 151, 203, 208, 218, 275, 279:** `ModificationLayer` methods `local_index_to_local_pos`, `local_to_world`, `get_chunk_mods`, `chunk_has_mods`, `clear`, `resolution`, `voxel_size` -- never called
- **Lines 289, 292:** `SharedModificationLayer` alias + constructor -- uses `RwLock` but production uses bare `Arc`

#### DC-4: texture_layer.rs (14+ items)
- **Lines 55, 62, 85:** `TextureWeights::custom`, `normalize`, `lerp` -- never called
- **Lines 126, 131, 141, 146:** `ChunkTextures::remove`, `is_empty`, `clear`, `iter` -- never called
- **Lines 213, 228, 233, 238, 243, 310, 314:** `TextureLayer` methods `paint_texture`, `get_chunk_textures`, `chunk_has_textures`, `textured_chunks`, `clear`, `resolution`, `voxel_size` -- never called
- **Lines 324, 327:** `SharedTextureLayer` alias + constructor -- dead pattern

#### DC-5: chunk_manager.rs (6 items)
- **Line 7:** Unused `godot_print` import
- **Line 76:** `requests_sent` variable assigned but never read
- **Line 57:** `update` method superseded by `update_with_layers`
- **Line 286:** `request_chunk_regeneration` -- never called; `mark_chunks_dirty` used instead
- **Lines 313, 317:** `chunk_count`, `active_chunk_count` -- never called

#### DC-6: Other Files
- **chunk.rs:51** `MeshResult::transition_sides` -- written but never read
- **chunk.rs:61,65** `MeshResult::triangle_count`, `vertex_count` -- never called
- **chunk.rs:82** `Chunk::coord` -- redundant with HashMap key
- **noise_field.rs:86** `set_box_bounds` -- never called
- **noise_field.rs:152,156** `get_csg_blend_width`, `set_csg_blend_width` -- never called
- **noise_field.rs:207** `SharedNoiseField` alias -- never used
- **mesh_postprocess.rs:36** `weld_epsilon` field -- stored but never read
- **mesh_postprocess.rs:168** `repair_manifold` -- called but is an empty stub (TODO)
- **brush_preview.rs:272** `set_colors` -- never called
- **lod.rs:28** `voxel_size_at_lod` -- never called; computed inline elsewhere

---

### WET CODE (11 duplication patterns)

#### WET-1 [CRITICAL]: Five Identical Commit Methods (terrain.rs:1617-1845)
`commit_geometry_modification`, `commit_flatten_modification`, `commit_plateau_modification`, `commit_smooth_modification`, `commit_slope_modification` differ only in which `brush.apply_*` is called and the log label (~40 lines each, 5x).

**Fix:** Extract `commit_sdf_modification(label, apply_fn)` taking a closure.

#### WET-2 [CRITICAL]: Alpha-Compositing + Direction Filter (brush.rs)
~45-line block copy-pasted in `apply_flatten` (766-822), `apply_plateau` (875-928), `apply_slope` (1163-1217).

**Fix:** Extract `composite_and_write(wx, y, wz, desired_sdf, new_blend, noise, existing, new_mods, direction_filter)`.

#### WET-3 [MAJOR]: Brush Settings Sync (terrain.rs:610-636 and 1199-1225)
Identical 25-line block setting mode, shape, size, strength, texture, step_size, feather, direction, bounds.

**Fix:** Extract `sync_brush_settings(&self, brush: &mut Brush)`.

#### WET-4 [MAJOR]: ArrayMesh Construction (terrain.rs + brush_preview.rs, 5 instances)
VariantArray construction pattern (vertex/normal/color/index + nils, `add_surface_from_arrays`) repeated 5 times.

**Fix:** Extract `build_array_mesh(vertices, normals, indices, colors: Option) -> Gd<ArrayMesh>`.

#### WET-5 [MAJOR]: Cross-Section Shader Parameters (terrain.rs:406-494)
Same 5 `set_shader_parameter` calls for stencil_write, terrain, and cap materials (3x).

**Fix:** Extract `set_cross_section_params(&self, mat: &mut Gd<ShaderMaterial>)`.

#### WET-6 [MODERATE]: Y-Range Setup (brush.rs, 4+ instances)
Lines 745-758, 848-861, 954-967, 1101-1112 compute terrain peak/trough/padding/y_range identically.

**Fix:** Extract `compute_y_scan_range(&self, noise, reference_y) -> Option<(f32, f32)>`.

#### WET-7 [MODERATE]: world_to_chunk / world_to_local_index (terrain_modifications.rs + texture_layer.rs)
Identical implementations in `ModificationLayer` and `TextureLayer`.

**Fix:** Extract shared `VoxelGridConfig` struct; both layers delegate to it.

#### WET-8 [MODERATE]: ChunkMods / ChunkTextures Wrappers
Both are identical `HashMap<u32, T>` wrappers with the same methods.

**Fix:** Create `SparseChunkData<T>` generic; type alias both.

#### WET-9 [MODERATE]: Undo/Redo (terrain.rs:1396-1451)
Structurally identical except `history.undo(c)` vs `history.redo(c)` and log string.

**Fix:** Extract `apply_history_action(action_fn, label) -> bool`.

#### WET-10 [MODERATE]: Trilinear Interpolation (terrain_modifications.rs:247-270 + texture_layer.rs:269-294)
Same 8-corner trilinear interpolation loop with identical weighting.

**Fix:** Extract generic trilinear sampler via `VoxelGridConfig`.

#### WET-11 [MINOR]: Node Cleanup (terrain.rs:924-946, 1470-1476)
Chunk node draining pattern repeated.

**Fix:** Extract `free_all_chunk_nodes(&mut self)`.

---

### MAGIC NUMBERS (~75 values across 10 files)

#### MN-1 [HIGH]: Repeated Epsilons/Thresholds (brush.rs)

| Value | Occurrences | Suggested Constant |
|-------|-------------|--------------------|
| `0.0001` | ~12 lines | `MIN_BLEND_THRESHOLD` / `CHANGE_EPSILON` |
| `0.001` | 2 lines (472, 503) | `MIN_HEIGHT_DELTA` / `CURVATURE_EPSILON` |
| `4.0` (Y padding) | 6 lines (487, 750, 853, 959, 970, 1105) | `Y_PADDING_VOXELS` |
| `1e-12` | 2 lines (666, 1067) | `DEGENERATE_SEGMENT_THRESHOLD` |
| `1e-6` | 1 line (1053) | `MIN_PATH_LENGTH` |

#### MN-2 [HIGH]: FBM Persistence Duplication (noise_field.rs)
`0.5` on lines 59 and 139 -- if one changes, `get_effective_amplitude()` breaks.

**Fix:** `const FBM_PERSISTENCE: f32 = 0.5;`

#### MN-3 [HIGH]: Default Texture Color (2 files)
`[1.0, 0.0, 0.0, 0.0]` in terrain.rs:1507 and mesh_extraction.rs:90.

**Fix:** Shared `const DEFAULT_TEXTURE_COLOR: [f32; 4] = [1.0, 0.0, 0.0, 0.0];`

#### MN-4 [MEDIUM]: UI/Editor Constants (editor_plugin.rs)

| Value | Lines | Suggested Constant |
|-------|-------|--------------------|
| `100.0, 28.0` (button size) | 103-107, 127+ | `BUTTON_MIN_SIZE` |
| `140.0` (toolbar width) | 83 | `TOOLBAR_MIN_WIDTH` |
| `8` (margins) | 84-87 | `TOOLBAR_MARGIN` |
| `4` (separation) | 92 | `TOOLBAR_SEPARATION` |
| `8.0` (separator height) | 116 | `SEPARATOR_HEIGHT` |
| `50.0` (brush size max) | 195, 942 | `BRUSH_SIZE_MAX` |
| `0.05` (slider steps) | 183, 225 | `SLIDER_FINE_STEP` |
| `10000.0` (raycast) | 1131, 1209, 1212 | `RAYCAST_MAX_DISTANCE` |
| `0.0001` (ray epsilon) | 1169, 1197 | `RAY_HORIZONTAL_EPSILON` |

#### MN-5 [MEDIUM]: Brush Defaults (brush.rs)

| Value | Line | Suggested Constant |
|-------|------|--------------------|
| `5.0` | 280 | `DEFAULT_BRUSH_SIZE` |
| `0.1` | 286 | `DEFAULT_HEIGHT_SENSITIVITY` |
| `4.0` | 288 | `DEFAULT_STEP_SIZE` |
| `0.5` | 414 | `CURVATURE_SENSITIVITY_FACTOR` |
| `0.1` | 623 | `MIN_STEP_SIZE` |
| `8` | 1261 | `SURFACE_BISECTION_ITERATIONS` |

#### MN-6 [MEDIUM]: Worker/Mesh Constants

| Value | File | Line | Suggested Constant |
|-------|------|------|--------------------|
| `3/4` ratio, min `2` | mesh_worker.rs | 54 | `THREAD_CPU_FRACTION` / `MIN_WORKER_THREADS` |
| `16` (batch size) | mesh_worker.rs | 86 | `MIN_BATCH_SIZE` |
| `256` (channel cap) | mesh_worker.rs | 145 | `DEFAULT_CHANNEL_CAPACITY` |
| `64` (max results) | chunk_manager.rs | 46 | `DEFAULT_MAX_RESULTS_PER_UPDATE` |
| `1e-2` (decimation err) | mesh_postprocess.rs | 331 | `DECIMATION_TARGET_ERROR` |

#### MN-7 [LOW]: Preview/Visual Constants (brush_preview.rs)

| Value | Line | Suggested Constant |
|-------|------|--------------------|
| `0.45` | 209 | `PREVIEW_QUAD_SCALE` |
| `16` | 360 | `CURVED_PLANE_SUBDIVISIONS` |
| `0.25` | 288, 290 | `HEIGHT_PLANE_ALPHA` |
| `0.0001` | 435 | `NORMAL_EPSILON` |
| Colors (lines 84-91) | 84-91 | Per CLAUDE.md: prefer `#[export]` properties |

#### MN-8 [LOW]: Terrain Stability (terrain_stability.rs)

| Value | Line | Suggested Constant |
|-------|------|--------------------|
| `64` | 246 | `FALLBACK_Y_RANGE_PADDING` |
| `2, 8, 32` | 254 | `SCAN_MARGIN_MULTIPLIER/MIN/MAX` |
| `4` | 318 | `FLOOR_SEED_ROWS` |
| `0.01` | 554 | `MIN_DROP_BLEND` |

#### MN-9 [LOW]: Render Priority (terrain.rs)

| Value | Line | Suggested Constant |
|-------|------|--------------------|
| `-1` | 531 | `STENCIL_WRITE_RENDER_PRIORITY` |
| `0` | 532 | `TERRAIN_RENDER_PRIORITY` |
| `1` | 533 | `STENCIL_CAP_RENDER_PRIORITY` |
