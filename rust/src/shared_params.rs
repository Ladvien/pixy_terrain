/// Shared terrain parameters used by both TerrainConfig and GrassConfig.
/// Adding a new shared field here automatically propagates to both config structs.
use godot::prelude::*;

use crate::marching_squares::{BlendMode, MergeMode};

#[derive(Clone, Debug)]
pub struct SharedTerrainParams {
    pub dimensions: Vector3i,
    pub cell_size: Vector2,
    pub merge_mode: MergeMode,
    pub blend_mode: BlendMode,
    pub wall_threshold: f32,
    pub ridge_threshold: f32,
    pub ledge_threshold: f32,
    pub use_ridge_texture: bool,
}

impl Default for SharedTerrainParams {
    fn default() -> Self {
        Self {
            dimensions: Vector3i::new(33, 32, 33),
            cell_size: Vector2::new(2.0, 2.0),
            merge_mode: MergeMode::Polyhedron,
            blend_mode: BlendMode::Direct,
            wall_threshold: 0.0,
            ridge_threshold: 1.0,
            ledge_threshold: 0.25,
            use_ridge_texture: false,
        }
    }
}
