//! Per-voxel texture layer for multi-texture terrain blending.
//!
//! This module provides texture weight storage that is passed to the shader
//! via vertex colors for blending multiple terrain textures.

use std::collections::HashMap;

use crate::chunk::ChunkCoord;

/// Maximum number of texture layers supported
pub const MAX_TEXTURE_LAYERS: usize = 4;

/// Texture blend weights for a single voxel.
/// Weights should sum to 1.0 for proper blending.
#[derive(Clone, Copy, Debug)]
pub struct TextureWeights {
    /// Blend weights for up to 4 textures (RGBA in vertex color)
    pub weights: [f32; MAX_TEXTURE_LAYERS],
}

impl Default for TextureWeights {
    fn default() -> Self {
        // Default: 100% texture 0
        Self {
            weights: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

impl TextureWeights {
    /// Create weights with a single texture at full strength
    pub fn single(texture_index: usize) -> Self {
        let mut weights = [0.0; MAX_TEXTURE_LAYERS];
        if texture_index < MAX_TEXTURE_LAYERS {
            weights[texture_index] = 1.0;
        }
        Self { weights }
    }

    /// Create weights with a blend between two textures
    pub fn blend(index_a: usize, index_b: usize, factor: f32) -> Self {
        let mut weights = [0.0; MAX_TEXTURE_LAYERS];
        let factor = factor.clamp(0.0, 1.0);
        if index_a < MAX_TEXTURE_LAYERS {
            weights[index_a] = 1.0 - factor;
        }
        if index_b < MAX_TEXTURE_LAYERS {
            weights[index_b] = factor;
        }
        Self { weights }
    }

    /// Convert to vertex color (RGBA)
    pub fn to_color(&self) -> [f32; 4] {
        self.weights
    }
}

/// Sparse storage of texture weights within a single chunk.
/// Uses local cell index (flattened from 3D grid position) as key.
#[derive(Clone, Debug, Default)]
pub struct ChunkTextures {
    /// Map from local cell index to texture weights
    pub textures: HashMap<u32, TextureWeights>,
}

impl ChunkTextures {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
        }
    }

    /// Set texture weights at a local cell index
    pub fn set(&mut self, local_index: u32, weights: TextureWeights) {
        self.textures.insert(local_index, weights);
    }

    /// Get texture weights at a local cell index
    pub fn get(&self, local_index: u32) -> Option<&TextureWeights> {
        self.textures.get(&local_index)
    }

    /// Get the number of texture entries in this chunk
    pub fn len(&self) -> usize {
        self.textures.len()
    }

}

/// Layer of texture weights spanning all chunks.
/// Thread-safe for concurrent read access during mesh generation.
#[derive(Clone, Debug)]
pub struct TextureLayer {
    /// Sparse storage: only chunks with painted textures are stored
    chunks: HashMap<ChunkCoord, ChunkTextures>,
    /// Resolution of each chunk (voxels per axis)
    resolution: u32,
    /// Size of each voxel in world units
    voxel_size: f32,
    /// Size of each chunk in world units
    chunk_size: f32,
}

impl TextureLayer {
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

    /// Set texture weights at a world position
    pub fn set_at_world(&mut self, x: f32, y: f32, z: f32, weights: TextureWeights) {
        let chunk = self.world_to_chunk(x, y, z);
        let local_index = self.world_to_local_index(x, y, z);

        self.chunks
            .entry(chunk)
            .or_insert_with(ChunkTextures::new)
            .set(local_index, weights);
    }

    /// Get texture weights at a world position
    pub fn get_at_world(&self, x: f32, y: f32, z: f32) -> Option<&TextureWeights> {
        let chunk = self.world_to_chunk(x, y, z);
        let local_index = self.world_to_local_index(x, y, z);

        self.chunks
            .get(&chunk)
            .and_then(|textures| textures.get(local_index))
    }

    /// Get total number of textured voxels across all chunks
    pub fn total_textured(&self) -> usize {
        self.chunks.values().map(|c| c.len()).sum()
    }

    /// Sample texture weights at a world position using trilinear interpolation
    /// Returns interpolated weights or default if no texture data nearby
    pub fn sample(&self, x: f32, y: f32, z: f32) -> TextureWeights {
        // Find the voxel cell that contains this point
        let vx = (x / self.voxel_size).floor();
        let vy = (y / self.voxel_size).floor();
        let vz = (z / self.voxel_size).floor();

        // Fractional position within the cell (0-1)
        let fx = x / self.voxel_size - vx;
        let fy = y / self.voxel_size - vy;
        let fz = z / self.voxel_size - vz;

        // Sample 8 corners for trilinear interpolation
        let mut total_weights = [0.0f32; MAX_TEXTURE_LAYERS];
        let mut total_influence = 0.0f32;

        for dz in 0..2 {
            for dy in 0..2 {
                for dx in 0..2 {
                    let wx = vx + dx as f32;
                    let wy = vy + dy as f32;
                    let wz = vz + dz as f32;

                    let world_x = wx * self.voxel_size;
                    let world_y = wy * self.voxel_size;
                    let world_z = wz * self.voxel_size;

                    if let Some(weights) = self.get_at_world(world_x, world_y, world_z) {
                        // Trilinear weight
                        let weight_x = if dx == 0 { 1.0 - fx } else { fx };
                        let weight_y = if dy == 0 { 1.0 - fy } else { fy };
                        let weight_z = if dz == 0 { 1.0 - fz } else { fz };
                        let weight = weight_x * weight_y * weight_z;

                        for i in 0..MAX_TEXTURE_LAYERS {
                            total_weights[i] += weights.weights[i] * weight;
                        }
                        total_influence += weight;
                    }
                }
            }
        }

        if total_influence > 0.0 {
            // Normalize the interpolated weights
            for w in &mut total_weights {
                *w /= total_influence;
            }
            TextureWeights {
                weights: total_weights,
            }
        } else {
            // No texture data - return default
            TextureWeights::default()
        }
    }

    pub fn chunk_size(&self) -> f32 {
        self.chunk_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_weights_single() {
        let weights = TextureWeights::single(2);
        assert_eq!(weights.weights, [0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn test_texture_weights_blend() {
        let weights = TextureWeights::blend(0, 1, 0.5);
        assert!((weights.weights[0] - 0.5).abs() < 0.001);
        assert!((weights.weights[1] - 0.5).abs() < 0.001);
        assert_eq!(weights.weights[2], 0.0);
        assert_eq!(weights.weights[3], 0.0);
    }

    #[test]
    fn test_texture_weights_to_color() {
        let weights = TextureWeights::single(1);
        let color = weights.to_color();
        assert_eq!(color, [0.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn test_texture_layer_paint() {
        let mut layer = TextureLayer::new(32, 1.0);

        layer.set_at_world(16.0, 8.0, 24.0, TextureWeights::single(2));

        let weights = layer.get_at_world(16.0, 8.0, 24.0).unwrap();
        // Texture 2 should have the highest weight
        assert_eq!(weights.weights[2], 1.0);
    }

    #[test]
    fn test_texture_layer_sample_default() {
        let layer = TextureLayer::new(32, 1.0);

        // Sample where no texture is painted should return default
        let weights = layer.sample(50.0, 50.0, 50.0);
        // Default is texture 0
        assert_eq!(weights.weights[0], 1.0);
    }
}
