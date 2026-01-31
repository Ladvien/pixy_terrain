# Pixy Terrain Code Review
Last Updated: 2026-01-30

## Summary

Comprehensive review against ARCHITECTURE.md and Godot integration.

---

## RESOLVED: Architecture Fixes Applied

### BUG 1: +1000 Outside Walls Creates Transvoxel Wall Geometry
**Location:** `rust/src/noise_field.rs:83-102`
**Status:** RESOLVED

**Fix:** Changed from returning `+1000` outside walls to clamping XZ coordinates to wall boundaries. This eliminates zero-crossings so transvoxel generates no wall geometry.

```rust
// Now clamps instead of discontinuous return
let clamped_x = x.clamp(self.box_min[0], self.box_max[0]);
let clamped_z = z.clamp(self.box_min[2], self.box_max[2]);
self.sample_terrain_only(clamped_x, y, clamped_z)
```

---

### BUG 2: Wall Normal Directions Are Inverted
**Location:** `rust/src/box_geometry.rs:62, 77, 92, 107`
**Status:** RESOLVED

**Fix:** Flipped all wall normals to point outward from the box:
- Wall -X: `[-1, 0, 0]` (was `[1, 0, 0]`)
- Wall +X: `[1, 0, 0]` (was `[-1, 0, 0]`)
- Wall -Z: `[0, 0, -1]` (was `[0, 0, 1]`)
- Wall +Z: `[0, 0, 1]` (was `[0, 0, -1]`)

---

### BUG 3: 3D Noise Makes Wall Height Search Unstable
**Location:** `rust/src/noise_field.rs:132-141`
**Status:** RESOLVED

**Fix:** Changed from 3D noise `fbm.get([x, y, z])` to 2D noise `fbm.get([x, z])`. Terrain height at (x,z) is now constant regardless of Y sample position, ensuring binary search converges correctly.

---

### BUG 4: WATERTIGHT_EPSILON Applied in Wrong Direction
**Location:** `rust/src/box_geometry.rs:155-158`
**Status:** RESOLVED

**Fix:** Changed from `y_top - WATERTIGHT_EPSILON` to `y_top + WATERTIGHT_EPSILON`. Wall tops now overlap INTO terrain mesh for watertight seam.

---

### BUG 5: Wall Tessellation Resolution Mismatch
**Location:** `rust/src/terrain.rs:490-495`
**Status:** RESOLVED

**Fix:** Wall segments now scale with map dimensions:
```rust
let segments_x = self.chunk_subdivisions as usize * self.map_width_x.max(1) as usize;
let segments_z = self.chunk_subdivisions as usize * self.map_depth_z.max(1) as usize;
let wall_segments = segments_x.max(segments_z).max(8);
```

---

### BUG 7: Floor/Wall Bottom Y Mismatch
**Location:** `rust/src/box_geometry.rs:60, 75, 90, 105`
**Status:** RESOLVED

**Fix:** Wall bottoms now use `floor_y_adjusted` (same as floor surface) instead of raw `floor_y`, ensuring watertight wall-floor junction.

---

## REMAINING: Not Yet Fixed

### BUG 6: Transvoxel Gradient Sampling Exceeds Boundary Offset
**Location:** Transvoxel library + `rust/src/terrain.rs:231`
**Severity:** MEDIUM
**Status:** MITIGATED by BUG 1 fix

The coordinate clamping in BUG 1 fix handles samples outside walls gracefully, so this is no longer causing visual issues. May still want to increase `boundary_offset` for cleaner behavior.

---

### BUG 8: dylib Self-Referential Dependency (macOS)
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

### BUG 9: Missing macOS x86_64 Entries in .gdextension
**Location:** `godot/pixy_terrain.gdextension`
**Severity:** LOW
**Status:** OPEN

Missing x86_64 entries for Intel Mac support.

---

## INVALID: External Agent False Positives

### NOT A BUG: "Missing Class Registration"
**Status:** INVALID

In modern gdext (Godot 4), `#[derive(GodotClass)]` automatically registers classes. No explicit registration call needed.

---

## Test Results

All 13 tests pass after fixes:
- chunk_manager: 7 tests pass
- mesh_worker: 6 tests pass
