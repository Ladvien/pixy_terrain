//! Per-voxel texture layer for multi-texture terrain blending.
//!
//! This module provides texture weight storage that is passed to the shader
//! via vertex colors for blending multiple terrain textures.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

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

    /// Create custom weights (will be normalized)
    pub fn custom(weights: [f32; MAX_TEXTURE_LAYERS]) -> Self {
        let mut result = Self { weights };
        result.normalize();
        result
    }

    /// Normalize weights so they sum to 1.0
    pub fn normalize(&mut self) {
        let sum: f32 = self.weights.iter().sum();
        if sum > 0.0 {
            for w in &mut self.weights {
                *w /= sum;
            }
        } else {
            // Fallback to texture 0 if all zeros
            self.weights[0] = 1.0;
        }
    }

    /// Get the dominant texture index (highest weight)
    pub fn dominant_texture(&self) -> usize {
        self.weights
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Blend this weight with another using a factor
    pub fn lerp(&self, other: &TextureWeights, factor: f32) -> TextureWeights {
        let factor = factor.clamp(0.0, 1.0);
        let mut result = [0.0; MAX_TEXTURE_LAYERS];
        for i in 0..MAX_TEXTURE_LAYERS {
            result[i] = self.weights[i] * (1.0 - factor) + other.weights[i] * factor;
        }
        TextureWeights { weights: result }
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

    /// Remove texture weights at a local cell index
    pub fn remove(&mut self, local_index: u32) -> Option<TextureWeights> {
        self.textures.remove(&local_index)
    }

    /// Check if this chunk has any texture data
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Get the number of texture entries in this chunk
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Clear all texture data in this chunk
    pub fn clear(&mut self) {
        self.textures.clear();
    }

    /// Iterate over all texture entries
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &TextureWeights)> {
        self.textures.iter()
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

    /// Paint a single texture at a world position
    pub fn paint_texture(&mut self, x: f32, y: f32, z: f32, texture_index: usize) {
        self.set_at_world(x, y, z, TextureWeights::single(texture_index));
    }

    /// Get texture weights at a world position
    pub fn get_at_world(&self, x: f32, y: f32, z: f32) -> Option<&TextureWeights> {
        let chunk = self.world_to_chunk(x, y, z);
        let local_index = self.world_to_local_index(x, y, z);

        self.chunks
            .get(&chunk)
            .and_then(|textures| textures.get(local_index))
    }

    /// Get texture weights for a chunk
    pub fn get_chunk_textures(&self, coord: &ChunkCoord) -> Option<&ChunkTextures> {
        self.chunks.get(coord)
    }

    /// Check if a chunk has any texture data
    pub fn chunk_has_textures(&self, coord: &ChunkCoord) -> bool {
        self.chunks.get(coord).map_or(false, |t| !t.is_empty())
    }

    /// Get all chunks that have texture data
    pub fn textured_chunks(&self) -> impl Iterator<Item = &ChunkCoord> {
        self.chunks.keys()
    }

    /// Clear all texture data
    pub fn clear(&mut self) {
        self.chunks.clear();
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

/// Thread-safe shared texture layer for parallel mesh generation
pub type SharedTextureLayer = Arc<RwLock<TextureLayer>>;

/// Create a new shared texture layer
pub fn new_shared_texture_layer(resolution: u32, voxel_size: f32) -> SharedTextureLayer {
    Arc::new(RwLock::new(TextureLayer::new(resolution, voxel_size)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_weights_single() {
        let weights = TextureWeights::single(2);
        assert_eq!(weights.weights, [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(weights.dominant_texture(), 2);
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
    fn test_texture_weights_normalize() {
        let mut weights = TextureWeights {
            weights: [2.0, 2.0, 0.0, 0.0],
        };
        weights.normalize();
        assert!((weights.weights[0] - 0.5).abs() < 0.001);
        assert!((weights.weights[1] - 0.5).abs() < 0.001);
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

        layer.paint_texture(16.0, 8.0, 24.0, 2);

        let weights = layer.get_at_world(16.0, 8.0, 24.0).unwrap();
        assert_eq!(weights.dominant_texture(), 2);
    }

    #[test]
    fn test_texture_layer_sample_default() {
        let layer = TextureLayer::new(32, 1.0);

        // Sample where no texture is painted should return default
        let weights = layer.sample(50.0, 50.0, 50.0);
        assert_eq!(weights.dominant_texture(), 0);
    }

    #[test]
    fn test_shared_texture_layer() {
        let shared = new_shared_texture_layer(32, 1.0);

        {
            let mut layer = shared.write().unwrap();
            layer.paint_texture(5.0, 5.0, 5.0, 3);
        }

        {
            let layer = shared.read().unwrap();
            let weights = layer.get_at_world(5.0, 5.0, 5.0).unwrap();
            assert_eq!(weights.dominant_texture(), 3);
        }
    }
}
