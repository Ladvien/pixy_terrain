//! Sparse voxel modification layer for terrain editing.
//!
//! This module provides a modification layer that overlays the procedural noise field,
//! allowing brush-based terrain sculpting without regenerating the entire terrain.

use std::collections::HashMap;

use crate::chunk::ChunkCoord;
use crate::voxel_grid::{SparseChunkData, VoxelGridConfig};

/// A single voxel modification.
/// Stored sparsely - only modified voxels are tracked.
///
/// Uses absolute SDF mode: `desired_sdf` is the target SDF value at this voxel,
/// and `blend` controls how much the final SDF uses this desired value vs the
/// base noise. Combined as: `noise * (1 - blend) + desired_sdf * blend`.
#[derive(Clone, Copy, Debug)]
pub struct VoxelMod {
    /// The target SDF value at this voxel (absolute, not a delta)
    pub desired_sdf: f32,
    /// Blend factor: 0.0 = use noise only, 1.0 = fully use desired_sdf
    pub blend: f32,
}

impl Default for VoxelMod {
    fn default() -> Self {
        Self {
            desired_sdf: 0.0,
            blend: 0.0,
        }
    }
}

impl VoxelMod {
    pub fn new(desired_sdf: f32, blend: f32) -> Self {
        Self { desired_sdf, blend }
    }
}

/// Sparse storage of modifications within a single chunk.
pub type ChunkMods = SparseChunkData<VoxelMod>;

/// Layer of terrain modifications spanning all chunks.
/// Thread-safe for concurrent read access during mesh generation.
#[derive(Clone, Debug)]
pub struct ModificationLayer {
    /// Sparse storage: only chunks with modifications are stored
    chunks: HashMap<ChunkCoord, ChunkMods>,
    /// Shared grid configuration
    grid: VoxelGridConfig,
}

impl ModificationLayer {
    pub fn new(resolution: u32, voxel_size: f32) -> Self {
        Self {
            chunks: HashMap::new(),
            grid: VoxelGridConfig::new(resolution, voxel_size),
        }
    }

    /// Convert world position to chunk coordinate
    pub fn world_to_chunk(&self, x: f32, y: f32, z: f32) -> ChunkCoord {
        self.grid.world_to_chunk(x, y, z)
    }

    /// Convert world position to local cell index within a chunk
    pub fn world_to_local_index(&self, x: f32, y: f32, z: f32) -> u32 {
        self.grid.world_to_local_index(x, y, z)
    }

    /// Set a modification at a world position
    pub fn set_at_world(&mut self, x: f32, y: f32, z: f32, modification: VoxelMod) {
        let chunk = self.grid.world_to_chunk(x, y, z);
        let local_index = self.grid.world_to_local_index(x, y, z);

        self.chunks
            .entry(chunk)
            .or_default()
            .set(local_index, modification);
    }

    /// Get modification at a world position
    pub fn get_at_world(&self, x: f32, y: f32, z: f32) -> Option<&VoxelMod> {
        let chunk = self.grid.world_to_chunk(x, y, z);
        let local_index = self.grid.world_to_local_index(x, y, z);

        self.chunks
            .get(&chunk)
            .and_then(|mods| mods.get(local_index))
    }

    /// Remove modification at a world position
    pub fn remove_at_world(&mut self, x: f32, y: f32, z: f32) -> Option<VoxelMod> {
        let chunk = self.grid.world_to_chunk(x, y, z);
        let local_index = self.grid.world_to_local_index(x, y, z);

        if let Some(chunk_mods) = self.chunks.get_mut(&chunk) {
            let result = chunk_mods.remove(local_index);
            // Clean up empty chunks
            if chunk_mods.is_empty() {
                self.chunks.remove(&chunk);
            }
            result
        } else {
            None
        }
    }

    /// Get all chunks that have modifications
    pub fn modified_chunks(&self) -> impl Iterator<Item = &ChunkCoord> {
        self.chunks.keys()
    }

    /// Get total number of modifications across all chunks
    pub fn total_modifications(&self) -> usize {
        self.chunks.values().map(|c| c.len()).sum()
    }

    /// Sample the modification layer at a world position using trilinear interpolation.
    /// Returns `(desired_total, blend_total)` for absolute SDF blending:
    ///   `combined_sdf = noise * (1 - blend_total) + desired_total`
    ///
    /// Corners without modifications contribute (0, 0) — i.e. "use noise".
    pub fn sample_absolute(&self, x: f32, y: f32, z: f32) -> (f32, f32) {
        let voxel_size = self.grid.voxel_size;
        let vx = (x / voxel_size).floor();
        let vy = (y / voxel_size).floor();
        let vz = (z / voxel_size).floor();

        let fx = x / voxel_size - vx;
        let fy = y / voxel_size - vy;
        let fz = z / voxel_size - vz;

        let mut desired_total = 0.0f32;
        let mut blend_total = 0.0f32;

        for dz in 0..2 {
            for dy in 0..2 {
                for dx in 0..2 {
                    let wx = vx + dx as f32;
                    let wy = vy + dy as f32;
                    let wz = vz + dz as f32;

                    let world_x = wx * voxel_size;
                    let world_y = wy * voxel_size;
                    let world_z = wz * voxel_size;

                    if let Some(modification) = self.get_at_world(world_x, world_y, world_z) {
                        let weight_x = if dx == 0 { 1.0 - fx } else { fx };
                        let weight_y = if dy == 0 { 1.0 - fy } else { fy };
                        let weight_z = if dz == 0 { 1.0 - fz } else { fz };
                        let weight = weight_x * weight_y * weight_z;

                        desired_total += modification.desired_sdf * modification.blend * weight;
                        blend_total += modification.blend * weight;
                    }
                }
            }
        }

        (desired_total, blend_total)
    }

    pub fn chunk_size(&self) -> f32 {
        self.grid.chunk_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voxel_mod_new() {
        let m = VoxelMod::new(3.0, 0.5);
        assert_eq!(m.desired_sdf, 3.0);
        assert_eq!(m.blend, 0.5);

        let d = VoxelMod::default();
        assert_eq!(d.desired_sdf, 0.0);
        assert_eq!(d.blend, 0.0);
    }

    #[test]
    fn test_chunk_mods() {
        let mut chunk_mods = ChunkMods::new();

        assert!(chunk_mods.is_empty());

        chunk_mods.set(42, VoxelMod::new(-1.0, 1.0));
        assert!(!chunk_mods.is_empty());
        assert_eq!(chunk_mods.len(), 1);

        let m = chunk_mods.get(42).unwrap();
        assert_eq!(m.desired_sdf, -1.0);
        assert_eq!(m.blend, 1.0);

        chunk_mods.remove(42);
        assert!(chunk_mods.is_empty());
    }

    #[test]
    fn test_modification_layer_world_coords() {
        let layer = ModificationLayer::new(32, 1.0);

        // Test chunk coordinate calculation
        let chunk = layer.world_to_chunk(50.0, 10.0, 70.0);
        assert_eq!(chunk, ChunkCoord::new(1, 0, 2)); // 50/32=1, 10/32=0, 70/32=2

        // Test negative coordinates
        let neg_chunk = layer.world_to_chunk(-10.0, -5.0, -1.0);
        assert_eq!(neg_chunk, ChunkCoord::new(-1, -1, -1));
    }

    #[test]
    fn test_modification_layer_set_get() {
        let mut layer = ModificationLayer::new(32, 1.0);

        layer.set_at_world(16.0, 8.0, 24.0, VoxelMod::new(-5.0, 1.0));

        let modification = layer.get_at_world(16.0, 8.0, 24.0).unwrap();
        assert_eq!(modification.desired_sdf, -5.0);
        assert_eq!(modification.blend, 1.0);

        layer.remove_at_world(16.0, 8.0, 24.0);
        assert!(layer.get_at_world(16.0, 8.0, 24.0).is_none());
    }

    #[test]
    fn test_modification_layer_sample_absolute() {
        let mut layer = ModificationLayer::new(32, 1.0);

        // Set a modification with desired_sdf = -4.0, blend = 1.0
        layer.set_at_world(10.0, 10.0, 10.0, VoxelMod::new(-4.0, 1.0));

        // Sample near the modification (offset by 0.5 into the cell)
        let (desired, blend) = layer.sample_absolute(10.5, 10.5, 10.5);
        // Only one corner has the modification, weight = 0.5^3 = 0.125
        assert!(blend > 0.0, "Should have non-zero blend near modification");
        assert!(
            desired < 0.0,
            "Should have negative desired_sdf for this modification"
        );

        // Sample far away should return (0, 0)
        let (far_desired, far_blend) = layer.sample_absolute(100.0, 100.0, 100.0);
        assert_eq!(far_desired, 0.0);
        assert_eq!(far_blend, 0.0);
    }

    #[test]
    fn test_sample_absolute_full_blend_at_grid_point() {
        let mut layer = ModificationLayer::new(32, 1.0);

        // Set all 8 corners of a cell to the same desired_sdf with blend=1.0
        let desired = -3.0;
        for dz in 0..2 {
            for dy in 0..2 {
                for dx in 0..2 {
                    layer.set_at_world(
                        10.0 + dx as f32,
                        10.0 + dy as f32,
                        10.0 + dz as f32,
                        VoxelMod::new(desired, 1.0),
                    );
                }
            }
        }

        // Sample at the center of the cell — all 8 corners contribute
        let (d, b) = layer.sample_absolute(10.5, 10.5, 10.5);
        assert!((b - 1.0).abs() < 0.001, "Blend should be ~1.0, got {b}");
        assert!(
            (d - desired).abs() < 0.001,
            "Desired should be ~{desired}, got {d}"
        );
    }
}
