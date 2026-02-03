//! Sparse voxel modification layer for terrain editing.
//!
//! This module provides a modification layer that overlays the procedural noise field,
//! allowing brush-based terrain sculpting without regenerating the entire terrain.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::chunk::ChunkCoord;

/// A single voxel modification.
/// Stored sparsely - only modified voxels are tracked.
#[derive(Clone, Copy, Debug)]
pub struct VoxelMod {
    /// SDF delta: negative = add material (raise terrain), positive = remove material (lower terrain)
    pub sdf_delta: f32,
    /// Blend factor: 0.0 = ignore modification, 1.0 = fully apply
    pub blend: f32,
}

impl Default for VoxelMod {
    fn default() -> Self {
        Self {
            sdf_delta: 0.0,
            blend: 1.0,
        }
    }
}

impl VoxelMod {
    pub fn new(sdf_delta: f32, blend: f32) -> Self {
        Self { sdf_delta, blend }
    }

    /// Create a modification to raise terrain (add material)
    pub fn raise(amount: f32) -> Self {
        Self {
            sdf_delta: -amount,
            blend: 1.0,
        }
    }

    /// Create a modification to lower terrain (remove material)
    pub fn lower(amount: f32) -> Self {
        Self {
            sdf_delta: amount,
            blend: 1.0,
        }
    }

    /// Apply this modification to an existing SDF value
    #[inline]
    pub fn apply(&self, base_sdf: f32) -> f32 {
        if self.blend <= 0.0 {
            return base_sdf;
        }
        // Blend the modification with the base SDF
        base_sdf + self.sdf_delta * self.blend
    }
}

/// Sparse storage of modifications within a single chunk.
/// Uses local cell index (flattened from 3D grid position) as key.
#[derive(Clone, Debug, Default)]
pub struct ChunkMods {
    /// Map from local cell index to modification
    pub mods: HashMap<u32, VoxelMod>,
}

impl ChunkMods {
    pub fn new() -> Self {
        Self {
            mods: HashMap::new(),
        }
    }

    /// Set a modification at a local cell index
    pub fn set(&mut self, local_index: u32, modification: VoxelMod) {
        self.mods.insert(local_index, modification);
    }

    /// Get a modification at a local cell index
    pub fn get(&self, local_index: u32) -> Option<&VoxelMod> {
        self.mods.get(&local_index)
    }

    /// Remove a modification at a local cell index
    pub fn remove(&mut self, local_index: u32) -> Option<VoxelMod> {
        self.mods.remove(&local_index)
    }

    /// Check if this chunk has any modifications
    pub fn is_empty(&self) -> bool {
        self.mods.is_empty()
    }

    /// Get the number of modifications in this chunk
    pub fn len(&self) -> usize {
        self.mods.len()
    }

    /// Clear all modifications in this chunk
    pub fn clear(&mut self) {
        self.mods.clear();
    }

    /// Iterate over all modifications
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &VoxelMod)> {
        self.mods.iter()
    }
}

/// Layer of terrain modifications spanning all chunks.
/// Thread-safe for concurrent read access during mesh generation.
#[derive(Clone, Debug)]
pub struct ModificationLayer {
    /// Sparse storage: only chunks with modifications are stored
    chunks: HashMap<ChunkCoord, ChunkMods>,
    /// Resolution of each chunk (voxels per axis)
    resolution: u32,
    /// Size of each voxel in world units
    voxel_size: f32,
    /// Size of each chunk in world units
    chunk_size: f32,
}

impl ModificationLayer {
    pub fn new(resolution: u32, voxel_size: f32) -> Self {
        Self {
            chunks: HashMap::new(),
            resolution,
            voxel_size,
            chunk_size: resolution as f32 * voxel_size,
        }
    }

    /// Convert world position to chunk coordinate
    pub fn world_to_chunk(&self, x: f32, y: f32, z: f32) -> ChunkCoord {
        ChunkCoord::new(
            (x / self.chunk_size).floor() as i32,
            (y / self.chunk_size).floor() as i32,
            (z / self.chunk_size).floor() as i32,
        )
    }

    /// Convert world position to local cell index within a chunk
    pub fn world_to_local_index(&self, x: f32, y: f32, z: f32) -> u32 {
        let chunk_x = (x / self.chunk_size).floor() * self.chunk_size;
        let chunk_y = (y / self.chunk_size).floor() * self.chunk_size;
        let chunk_z = (z / self.chunk_size).floor() * self.chunk_size;

        let local_x = ((x - chunk_x) / self.voxel_size).floor() as u32;
        let local_y = ((y - chunk_y) / self.voxel_size).floor() as u32;
        let local_z = ((z - chunk_z) / self.voxel_size).floor() as u32;

        let lx = local_x.min(self.resolution - 1);
        let ly = local_y.min(self.resolution - 1);
        let lz = local_z.min(self.resolution - 1);

        lx + ly * self.resolution + lz * self.resolution * self.resolution
    }

    /// Convert local cell index to local 3D coordinates
    pub fn local_index_to_local_pos(&self, index: u32) -> (u32, u32, u32) {
        let z = index / (self.resolution * self.resolution);
        let remainder = index % (self.resolution * self.resolution);
        let y = remainder / self.resolution;
        let x = remainder % self.resolution;
        (x, y, z)
    }

    /// Convert local cell position and chunk to world position (cell center)
    pub fn local_to_world(&self, chunk: ChunkCoord, local_x: u32, local_y: u32, local_z: u32) -> (f32, f32, f32) {
        let world_x = chunk.x as f32 * self.chunk_size + (local_x as f32 + 0.5) * self.voxel_size;
        let world_y = chunk.y as f32 * self.chunk_size + (local_y as f32 + 0.5) * self.voxel_size;
        let world_z = chunk.z as f32 * self.chunk_size + (local_z as f32 + 0.5) * self.voxel_size;
        (world_x, world_y, world_z)
    }

    /// Set a modification at a world position
    pub fn set_at_world(&mut self, x: f32, y: f32, z: f32, modification: VoxelMod) {
        let chunk = self.world_to_chunk(x, y, z);
        let local_index = self.world_to_local_index(x, y, z);

        self.chunks
            .entry(chunk)
            .or_insert_with(ChunkMods::new)
            .set(local_index, modification);
    }

    /// Get modification at a world position
    pub fn get_at_world(&self, x: f32, y: f32, z: f32) -> Option<&VoxelMod> {
        let chunk = self.world_to_chunk(x, y, z);
        let local_index = self.world_to_local_index(x, y, z);

        self.chunks.get(&chunk).and_then(|mods| mods.get(local_index))
    }

    /// Remove modification at a world position
    pub fn remove_at_world(&mut self, x: f32, y: f32, z: f32) -> Option<VoxelMod> {
        let chunk = self.world_to_chunk(x, y, z);
        let local_index = self.world_to_local_index(x, y, z);

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

    /// Get modifications for a specific chunk
    pub fn get_chunk_mods(&self, coord: &ChunkCoord) -> Option<&ChunkMods> {
        self.chunks.get(coord)
    }

    /// Check if a chunk has any modifications
    pub fn chunk_has_mods(&self, coord: &ChunkCoord) -> bool {
        self.chunks.get(coord).map_or(false, |m| !m.is_empty())
    }

    /// Get all chunks that have modifications
    pub fn modified_chunks(&self) -> impl Iterator<Item = &ChunkCoord> {
        self.chunks.keys()
    }

    /// Clear all modifications
    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    /// Get total number of modifications across all chunks
    pub fn total_modifications(&self) -> usize {
        self.chunks.values().map(|c| c.len()).sum()
    }

    /// Sample the modification delta at a world position using trilinear interpolation
    /// Returns the SDF delta to add to the base noise value
    pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        // Find the voxel cell that contains this point
        let vx = (x / self.voxel_size).floor();
        let vy = (y / self.voxel_size).floor();
        let vz = (z / self.voxel_size).floor();

        // Fractional position within the cell (0-1)
        let fx = x / self.voxel_size - vx;
        let fy = y / self.voxel_size - vy;
        let fz = z / self.voxel_size - vz;

        // Sample 8 corners for trilinear interpolation
        let mut total = 0.0f32;

        for dz in 0..2 {
            for dy in 0..2 {
                for dx in 0..2 {
                    let wx = vx + dx as f32;
                    let wy = vy + dy as f32;
                    let wz = vz + dz as f32;

                    let world_x = wx * self.voxel_size;
                    let world_y = wy * self.voxel_size;
                    let world_z = wz * self.voxel_size;

                    if let Some(modification) = self.get_at_world(world_x, world_y, world_z) {
                        // Trilinear weight
                        let weight_x = if dx == 0 { 1.0 - fx } else { fx };
                        let weight_y = if dy == 0 { 1.0 - fy } else { fy };
                        let weight_z = if dz == 0 { 1.0 - fz } else { fz };
                        let weight = weight_x * weight_y * weight_z;

                        total += modification.sdf_delta * modification.blend * weight;
                    }
                }
            }
        }

        total
    }

    pub fn resolution(&self) -> u32 {
        self.resolution
    }

    pub fn voxel_size(&self) -> f32 {
        self.voxel_size
    }

    pub fn chunk_size(&self) -> f32 {
        self.chunk_size
    }
}

/// Thread-safe shared modification layer for parallel mesh generation
pub type SharedModificationLayer = Arc<RwLock<ModificationLayer>>;

/// Create a new shared modification layer
pub fn new_shared_modification_layer(resolution: u32, voxel_size: f32) -> SharedModificationLayer {
    Arc::new(RwLock::new(ModificationLayer::new(resolution, voxel_size)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voxel_mod_apply() {
        let base_sdf = 5.0;

        // Raising terrain (negative delta)
        let raise_mod = VoxelMod::raise(3.0);
        assert_eq!(raise_mod.apply(base_sdf), 2.0); // 5.0 + (-3.0) = 2.0

        // Lowering terrain (positive delta)
        let lower_mod = VoxelMod::lower(2.0);
        assert_eq!(lower_mod.apply(base_sdf), 7.0); // 5.0 + 2.0 = 7.0

        // Partial blend
        let partial_mod = VoxelMod { sdf_delta: -10.0, blend: 0.5 };
        assert_eq!(partial_mod.apply(base_sdf), 0.0); // 5.0 + (-10.0 * 0.5) = 0.0

        // Zero blend (no effect)
        let no_effect = VoxelMod { sdf_delta: -10.0, blend: 0.0 };
        assert_eq!(no_effect.apply(base_sdf), 5.0);
    }

    #[test]
    fn test_chunk_mods() {
        let mut chunk_mods = ChunkMods::new();

        assert!(chunk_mods.is_empty());

        chunk_mods.set(42, VoxelMod::raise(1.0));
        assert!(!chunk_mods.is_empty());
        assert_eq!(chunk_mods.len(), 1);

        let m = chunk_mods.get(42).unwrap();
        assert_eq!(m.sdf_delta, -1.0);

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

        layer.set_at_world(16.0, 8.0, 24.0, VoxelMod::raise(5.0));

        let modification = layer.get_at_world(16.0, 8.0, 24.0).unwrap();
        assert_eq!(modification.sdf_delta, -5.0);

        layer.remove_at_world(16.0, 8.0, 24.0);
        assert!(layer.get_at_world(16.0, 8.0, 24.0).is_none());
    }

    #[test]
    fn test_modification_layer_sample() {
        let mut layer = ModificationLayer::new(32, 1.0);

        // Set a modification
        layer.set_at_world(10.0, 10.0, 10.0, VoxelMod::raise(4.0));

        // Sample at the exact position should return the modification
        let sample = layer.sample(10.5, 10.5, 10.5);
        assert!(sample < 0.0, "Should have negative delta for raised terrain");

        // Sample far away should return 0
        let far_sample = layer.sample(100.0, 100.0, 100.0);
        assert_eq!(far_sample, 0.0);
    }

    #[test]
    fn test_shared_modification_layer() {
        let shared = new_shared_modification_layer(32, 1.0);

        {
            let mut layer = shared.write().unwrap();
            layer.set_at_world(5.0, 5.0, 5.0, VoxelMod::raise(2.0));
        }

        {
            let layer = shared.read().unwrap();
            let modification = layer.get_at_world(5.0, 5.0, 5.0).unwrap();
            assert_eq!(modification.sdf_delta, -2.0);
        }
    }
}
