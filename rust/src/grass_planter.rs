// Grass planting system — implemented in Part 12
use std::collections::HashMap;

use godot::classes::{
    IMultiMeshInstance3D, Image, Mesh, MultiMesh, MultiMeshInstance3D, QuadMesh, ResourceLoader,
    Shader, ShaderMaterial, Texture, Texture2D,
};
use godot::obj::InstanceId;
use godot::prelude::*;

use crate::marching_squares::{CellGeometry, MergeMode};

/// Cached grass configuration (avoid needing to bind terrain during operations).
/// Passed from terrain to chunk to grass planter at initialization.
#[derive(Clone)]
pub struct GrassConfig {
    pub dimensions: Vector3i,
    pub subdivisions: i32,
    pub grass_size: Vector2,
    pub cell_size: Vector2,
    pub wall_threshold: f32,
    pub merge_mode: MergeMode,
    pub animation_fps: i32,
    pub ledge_threshold: f32,
    pub ridge_threshold: f32,
    pub grass_sprites: [Option<Gd<Texture2D>>; 6],
    pub ground_colors: [Color; 6],
    pub tex_has_grass: [bool; 5],
    pub grass_mesh: Option<Gd<Mesh>>,
    pub grass_material: Option<Gd<ShaderMaterial>>,
    pub grass_quad_mesh: Option<Gd<Mesh>>,
    pub ground_images: [Option<Gd<Image>>; 6],
    pub texture_scales: [f32; 6],
}

impl Default for GrassConfig {
    fn default() -> Self {
        Self {
            dimensions: Vector3i::new(33, 32, 33),
            subdivisions: 3,
            grass_size: Vector2::new(1.0, 1.0),
            cell_size: Vector2::new(2.0, 2.0),
            wall_threshold: 0.0,
            merge_mode: MergeMode::Polyhedron,
            animation_fps: 0,
            ledge_threshold: 0.25,
            ridge_threshold: 1.0,
            grass_sprites: [None, None, None, None, None, None],
            ground_colors: [Color::from_rgba(0.4, 0.5, 0.3, 1.0); 6],
            tex_has_grass: [true; 5],
            grass_mesh: None,
            grass_material: None,
            grass_quad_mesh: None,
            ground_images: [None, None, None, None, None, None],
            texture_scales: [1.0; 6],
        }
    }
}

/// Gras planter node -- places MultiMesh grass blades on floor trianges.
#[derive(GodotClass)]
#[class(base=MultiMeshInstance3D, init, tool)]
pub struct PixyGrassPlanter {
    base: Base<MultiMeshInstance3D>,
    grass_config: GrassConfig,
    parent_chunk_id: Option<InstanceId>,
}

impl PixyGrassPlanter {
    /// Initialize the planter with a cached config. Called by chunk during initialization
    pub fn setup_with_config(
        &mut self,
        chunk_id: InstanceId,
        config: GrassConfig,
        _force_rebuild: bool,
    ) {
        self.parent_chunk_id = Some(chunk_id);
        self.grass_config = config;
    }

    /// Regenerate grass on all cells using prebuilt geometry.
    pub fn regenerate_all_cells_with_geometry(
        &mut self,
        _cell_geometry: &HashMap<[i32; 2], CellGeometry>,
    ) {
    }
}
