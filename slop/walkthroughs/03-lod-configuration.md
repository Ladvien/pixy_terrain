# Walkthrough 03: LOD Configuration

**Series:** Transvoxel Noise Terrain
**Status:** Planning
**Prerequisites:** Walkthrough 02 (data structures)

## Goal

Implement LOD configuration with distance-based level selection. LOD levels use doubling distance thresholds (64m → 128m → 256m → 512m).

## Acceptance Criteria

- [ ] `LODConfig` struct with configurable base distance and max level
- [ ] `lod_for_distance()` returns correct LOD level
- [ ] `distance_threshold()` returns boundary for each LOD
- [ ] `voxel_size_at_lod()` calculates effective voxel size
- [ ] Unit tests verify all calculations

## LOD Distance Formula

```
LOD_level = floor(log2(distance / base_distance))
```

With base_distance = 64m:
- LOD 0: 0-64m (full detail)
- LOD 1: 64-128m (2x voxel size)
- LOD 2: 128-256m (4x voxel size)
- LOD 3: 256-512m (8x voxel size)
- LOD 4: 512-1024m (16x voxel size)

## Steps

### Step 1: Create lod.rs Module

**File:** `rust/src/lod.rs` (new file)

```rust
// Full path: rust/src/lod.rs

//! LOD (Level of Detail) configuration for terrain chunks.
//!
//! Distance thresholds double at each LOD level, matching the
//! transvoxel algorithm's 2:1 resolution ratio at boundaries.

/// LOD configuration with distance-based level selection
#[derive(Clone, Debug)]
pub struct LODConfig {
    /// Distance threshold for LOD 0 (highest detail)
    pub base_distance: f32,
    /// Maximum LOD level (lower detail, further distance)
    pub max_lod: u8,
    /// Chunk subdivisions (typically 32)
    pub chunk_subdivisions: u32,
}

impl LODConfig {
    /// Create new LOD configuration
    ///
    /// # Arguments
    /// * `base_distance` - Distance at which LOD 0 ends (typically 64.0)
    /// * `max_lod` - Maximum LOD level (typically 4)
    pub fn new(base_distance: f32, max_lod: u8) -> Self {
        Self {
            base_distance,
            max_lod,
            chunk_subdivisions: 32,
        }
    }

    /// Calculate LOD level for a given distance
    ///
    /// Formula: LOD = floor(log2(distance / base_distance))
    /// Clamped to [0, max_lod]
    pub fn lod_for_distance(&self, distance: f32) -> u8 {
        if distance <= 0.0 || distance <= self.base_distance {
            return 0;
        }

        let level = (distance / self.base_distance).log2().floor() as i32;
        (level.max(0) as u8).min(self.max_lod)
    }

    /// Get distance threshold where a LOD level begins
    ///
    /// LOD 0: 0, LOD 1: base_distance, LOD 2: base_distance * 2, etc.
    pub fn distance_threshold(&self, lod: u8) -> f32 {
        if lod == 0 {
            0.0
        } else {
            self.base_distance * (1 << (lod - 1)) as f32
        }
    }

    /// Get distance threshold where a LOD level ends
    pub fn distance_threshold_end(&self, lod: u8) -> f32 {
        self.base_distance * (1 << lod) as f32
    }

    /// Maximum view distance (where max_lod ends)
    pub fn max_view_distance(&self) -> f32 {
        self.distance_threshold_end(self.max_lod)
    }

    /// Effective voxel size at a given LOD level
    ///
    /// Voxel size doubles at each LOD level
    pub fn voxel_size_at_lod(&self, base_voxel_size: f32, lod: u8) -> f32 {
        base_voxel_size * (1 << lod) as f32
    }

    /// Chunk world size (same at all LOD levels)
    pub fn chunk_world_size(&self, base_voxel_size: f32) -> f32 {
        base_voxel_size * self.chunk_subdivisions as f32
    }
}

impl Default for LODConfig {
    fn default() -> Self {
        Self::new(64.0, 4)
    }
}
```

### Step 2: Add Unit Tests

```rust
// Full path: rust/src/lod.rs (append)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lod_for_distance_at_origin() {
        let config = LODConfig::new(64.0, 4);
        assert_eq!(config.lod_for_distance(0.0), 0);
        assert_eq!(config.lod_for_distance(1.0), 0);
    }

    #[test]
    fn test_lod_for_distance_boundaries() {
        let config = LODConfig::new(64.0, 4);

        // LOD 0: 0-64m
        assert_eq!(config.lod_for_distance(32.0), 0);
        assert_eq!(config.lod_for_distance(64.0), 0);

        // LOD 1: 64-128m
        assert_eq!(config.lod_for_distance(65.0), 1);
        assert_eq!(config.lod_for_distance(100.0), 1);
        assert_eq!(config.lod_for_distance(128.0), 1);

        // LOD 2: 128-256m
        assert_eq!(config.lod_for_distance(129.0), 2);
        assert_eq!(config.lod_for_distance(256.0), 2);

        // LOD 3: 256-512m
        assert_eq!(config.lod_for_distance(300.0), 3);
        assert_eq!(config.lod_for_distance(512.0), 3);

        // LOD 4: 512-1024m
        assert_eq!(config.lod_for_distance(600.0), 4);
        assert_eq!(config.lod_for_distance(1024.0), 4);
    }

    #[test]
    fn test_lod_clamped_to_max() {
        let config = LODConfig::new(64.0, 4);
        // Beyond max LOD distance should clamp to max_lod
        assert_eq!(config.lod_for_distance(5000.0), 4);
    }

    #[test]
    fn test_distance_thresholds() {
        let config = LODConfig::new(64.0, 4);

        assert_eq!(config.distance_threshold(0), 0.0);
        assert_eq!(config.distance_threshold(1), 64.0);
        assert_eq!(config.distance_threshold(2), 128.0);
        assert_eq!(config.distance_threshold(3), 256.0);
        assert_eq!(config.distance_threshold(4), 512.0);
    }

    #[test]
    fn test_distance_threshold_end() {
        let config = LODConfig::new(64.0, 4);

        assert_eq!(config.distance_threshold_end(0), 64.0);
        assert_eq!(config.distance_threshold_end(1), 128.0);
        assert_eq!(config.distance_threshold_end(2), 256.0);
        assert_eq!(config.distance_threshold_end(3), 512.0);
        assert_eq!(config.distance_threshold_end(4), 1024.0);
    }

    #[test]
    fn test_max_view_distance() {
        let config = LODConfig::new(64.0, 4);
        assert_eq!(config.max_view_distance(), 1024.0);
    }

    #[test]
    fn test_voxel_size_at_lod() {
        let config = LODConfig::new(64.0, 4);
        let base_voxel = 1.0;

        assert_eq!(config.voxel_size_at_lod(base_voxel, 0), 1.0);
        assert_eq!(config.voxel_size_at_lod(base_voxel, 1), 2.0);
        assert_eq!(config.voxel_size_at_lod(base_voxel, 2), 4.0);
        assert_eq!(config.voxel_size_at_lod(base_voxel, 3), 8.0);
        assert_eq!(config.voxel_size_at_lod(base_voxel, 4), 16.0);
    }

    #[test]
    fn test_chunk_world_size() {
        let config = LODConfig::new(64.0, 4);
        assert_eq!(config.chunk_world_size(1.0), 32.0);
        assert_eq!(config.chunk_world_size(2.0), 64.0);
    }
}
```

### Step 3: Register Module in lib.rs

```rust
// Full path: rust/src/lib.rs

use godot::prelude::*;

mod chunk;
mod lod;
mod terrain;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
```

### Step 4: Verify

```bash
cd rust && cargo test lod
```

Expected: All LOD tests pass.

## Verification Checklist

- [ ] `lod_for_distance(32.0)` returns 0
- [ ] `lod_for_distance(100.0)` returns 1
- [ ] `lod_for_distance(5000.0)` returns max_lod (clamped)
- [ ] Distance thresholds double correctly
- [ ] Voxel sizes double at each LOD level

## Key Insight: Why Doubling?

Transvoxel transition cells are designed for 2:1 resolution ratios. Adjacent chunks can differ by at most one LOD level. The doubling distance formula naturally creates this relationship - as you move twice as far, you get half the resolution, which is exactly what transvoxel's transition cells expect.

## What's Next

Walkthrough 04 creates the noise field that produces SDF values for terrain generation.
