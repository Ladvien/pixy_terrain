//! Shared voxel grid utilities used by both ModificationLayer and TextureLayer.
//!
//! Provides world-to-chunk coordinate conversion and sparse per-chunk data storage.

use std::collections::HashMap;

use crate::chunk::ChunkCoord;

/// Configuration for a voxel grid: resolution, voxel size, and derived chunk size.
/// Shared between ModificationLayer and TextureLayer to avoid code duplication.
#[derive(Clone, Debug)]
pub struct VoxelGridConfig {
    /// Number of voxels per axis per chunk
    pub resolution: u32,
    /// Size of each voxel in world units
    pub voxel_size: f32,
    /// Size of each chunk in world units (resolution * voxel_size)
    pub chunk_size: f32,
}

impl VoxelGridConfig {
    pub fn new(resolution: u32, voxel_size: f32) -> Self {
        Self {
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
}

/// Generic sparse per-chunk data storage.
/// Replaces both `ChunkMods` and `ChunkTextures` with a single generic type.
#[derive(Clone, Debug, Default)]
pub struct SparseChunkData<T> {
    pub data: HashMap<u32, T>,
}

impl<T> SparseChunkData<T> {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn set(&mut self, local_index: u32, value: T) {
        self.data.insert(local_index, value);
    }

    pub fn get(&self, local_index: u32) -> Option<&T> {
        self.data.get(&local_index)
    }

    pub fn remove(&mut self, local_index: u32) -> Option<T> {
        self.data.remove(&local_index)
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voxel_grid_config_new() {
        let config = VoxelGridConfig::new(32, 1.0);
        assert_eq!(config.resolution, 32);
        assert_eq!(config.voxel_size, 1.0);
        assert_eq!(config.chunk_size, 32.0);
    }

    #[test]
    fn test_world_to_chunk() {
        let config = VoxelGridConfig::new(32, 1.0);
        let chunk = config.world_to_chunk(50.0, 10.0, 70.0);
        assert_eq!(chunk, ChunkCoord::new(1, 0, 2));

        let neg = config.world_to_chunk(-10.0, -5.0, -1.0);
        assert_eq!(neg, ChunkCoord::new(-1, -1, -1));
    }

    #[test]
    fn test_world_to_local_index_in_range() {
        let config = VoxelGridConfig::new(32, 1.0);
        let index = config.world_to_local_index(1.0, 2.0, 3.0);
        // 1 + 2*32 + 3*32*32 = 1 + 64 + 3072 = 3137
        assert_eq!(index, 3137);
    }

    #[test]
    fn test_sparse_chunk_data() {
        let mut data: SparseChunkData<f32> = SparseChunkData::new();
        assert!(data.is_empty());

        data.set(42, -1.0);
        assert!(!data.is_empty());
        assert_eq!(data.len(), 1);
        assert_eq!(*data.get(42).unwrap(), -1.0);

        data.remove(42);
        assert!(data.is_empty());
    }
}
