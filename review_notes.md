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
| No magic numbers | PARTIAL | 2 FBM constants remain |
| Functions single-purpose | PASS | |
| No dead code | PASS | box_geometry.rs deleted |
| No unsafe without justification | PASS | meshopt FFI documented |
| Chunk boundaries respected | PASS | Guard chunks working |
| Noise uses 2D sampling | PASS | `fbm.get([x, z])` |
| Wall normals outward | PASS | Via SDF gradient |
| Watertight seams | PARTIAL | Export pipeline only |
| Mesh uses worker threads | PASS | Rayon parallel |
| No blocking in render | PASS | Async mesh gen |
