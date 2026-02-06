use std::collections::HashMap;

use godot::classes::mesh::PrimitiveType;
use godot::classes::surface_tool::CustomFormat;
use godot::classes::{
    ArrayMesh, Engine, IMeshInstance3D, MeshInstance3D, Noise, ShaderMaterial, StaticBody3D,
    SurfaceTool,
};
use godot::prelude::*;

use crate::grass_planter::{GrassConfig, PixyGrassPlanter};
use crate::marching_squares::{self, CellContext, CellGeometry, MergeMode};

/// Cached terrain configuration (avoids needing to bind terrain during chunk operations).
/// Passed from terrain to chunk at initialization time to break the borrow cycle.
#[derive(Clone, Debug)]
pub struct TerrainConfig {
    pub dimensions: Vector3i,
    pub cell_size: Vector2,
    pub blend_mode: i32,
    pub use_ridge_texture: bool,
    pub ridge_threshold: f32,
    pub extra_collision_layer: i32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            dimensions: Vector3i::new(33, 32, 33),
            cell_size: Vector2::new(2.0, 2.0),
            blend_mode: 0,
            use_ridge_texture: false,
            ridge_threshold: 1.0,
            extra_collision_layer: 9,
        }
    }
}

/// Per-chunk mesh instance that holds heightmap data and generates geometry.
/// Port of Yugen's MarchingSquaresTerrainChunk (MeshInstance3D).
#[derive(GodotClass)]
#[class(base=MeshInstance3D, init, tool)]
pub struct PixyTerrainChunk {
    base: Base<MeshInstance3D>,

    /// Chunk coordinates in the terrain grid.
    #[export]
    pub chunk_coords: Vector2i,

    /// Merge mode index (mirrors terrain setting, stored per-chunk for serialization).
    #[export]
    #[init(val = 1)]
    pub merge_mode: i32,

    // ═══════════════════════════════════════════
    // Persisted Terrain Data (Godot PackedArrays for scene serialization)
    // ═══════════════════════════════════════════
    /// Flat height data for serialization: row-major, dim_z rows of dim_x values.
    #[export]
    #[init(val = PackedFloat32Array::new())]
    pub saved_height_map: PackedFloat32Array,

    /// Persisted ground color channel 0.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_color_map_0: PackedColorArray,

    /// Persisted ground color channel 1.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_color_map_1: PackedColorArray,

    /// Persisted wall color channel 0.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_wall_color_map_0: PackedColorArray,

    /// Persisted wall color channel 1.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_wall_color_map_1: PackedColorArray,

    /// Persisted grass mask map.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_grass_mask_map: PackedColorArray,

    // ═══════════════════════════════════════════
    // Runtime Terrain Data Maps (working copies)
    // ═══════════════════════════════════════════
    /// 2D height array: height_map[z][x] = f32
    pub height_map: Vec<Vec<f32>>,

    /// Ground vertex color channel 0 (flat array: z * dim_x + x).
    pub color_map_0: Vec<Color>,

    /// Ground vertex color channel 1.
    pub color_map_1: Vec<Color>,

    /// Wall vertex color channel 0.
    pub wall_color_map_0: Vec<Color>,

    /// Wall vertex color channel 1.
    pub wall_color_map_1: Vec<Color>,

    /// Grass mask per vertex (R=mask, G=ridge flag).
    pub grass_mask_map: Vec<Color>,

    /// Dirty flags per cell: needs_update[z][x] = bool.
    pub needs_update: Vec<Vec<bool>>,

    /// Cached geometry per cell for incremental updates.
    pub cell_geometry: HashMap<[i32; 2], CellGeometry>,

    /// Whether to use higher-poly floors (4 triangles vs 2).
    #[init(val = true)]
    pub higher_poly_floors: bool,

    /// Whether this chunk was just created (affects initial color assignment).
    pub is_new_chunk: bool,

    /// Skip saving data to packed arrays on exit_tree (e.g., during undo operations).
    #[export]
    #[init(val = false)]
    pub skip_save_on_exit: bool,

    /// Cached terrain configuration (set once at initialization, avoids needing to bind terrain).
    terrain_config: TerrainConfig,

    /// Terrain material reference for mesh regeneration (stored so regenerate_mesh() can use it).
    terrain_material: Option<Gd<ShaderMaterial>>,

    /// Grass planter child node.
    grass_planter: Option<Gd<PixyGrassPlanter>>,
}

impl PixyTerrainChunk {
    pub fn new_with_base(base: Base<MeshInstance3D>) -> Self {
        Self {
            base,
            chunk_coords: Vector2i::ZERO,
            merge_mode: 1,
            saved_height_map: PackedFloat32Array::new(),
            saved_color_map_0: PackedColorArray::new(),
            saved_color_map_1: PackedColorArray::new(),
            saved_wall_color_map_0: PackedColorArray::new(),
            saved_wall_color_map_1: PackedColorArray::new(),
            saved_grass_mask_map: PackedColorArray::new(),
            height_map: Vec::new(),
            color_map_0: Vec::new(),
            color_map_1: Vec::new(),
            wall_color_map_0: Vec::new(),
            wall_color_map_1: Vec::new(),
            grass_mask_map: Vec::new(),
            needs_update: Vec::new(),
            cell_geometry: HashMap::new(),
            higher_poly_floors: true,
            is_new_chunk: false,
            skip_save_on_exit: false,
            terrain_config: TerrainConfig::default(),
            terrain_material: None,
            grass_planter: None,
        }
    }
}

#[godot_api]
impl IMeshInstance3D for PixyTerrainChunk {
    fn exit_tree(&mut self) {
        // Sync runtime data to packed arrays before leaving tree (scene save)
        if !self.skip_save_on_exit {
            self.sync_to_packed();
        }
    }
}

#[godot_api]
impl PixyTerrainChunk {
    /// Regenerate all cells (mark all dirty and rebuild mesh).
    #[func]
    pub fn regenerate_all_cells(&mut self) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        for z in 0..(dim_z - 1) {
            for x in 0..(dim_x - 1) {
                self.needs_update[z as usize][x as usize] = true;
            }
        }
        self.regenerate_mesh();
    }

    /// Get height at a grid point.
    #[func]
    pub fn get_height(&self, coords: Vector2i) -> f32 {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if coords.x < 0 || coords.y < 0 || coords.x >= dim_x || coords.y >= dim_z {
            return 0.0;
        }
        self.height_map[coords.y as usize][coords.x as usize]
    }

    /// Draw (set) height at a grid point and mark surrounding cells dirty.
    #[func]
    pub fn draw_height(&mut self, x: i32, z: i32, y: f32) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.height_map[z as usize][x as usize] = y;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw ground color channel 0 at a grid point.
    #[func]
    pub fn draw_color_0(&mut self, x: i32, z: i32, color: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.color_map_0[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw ground color channel 1 at a grid point.
    #[func]
    pub fn draw_color_1(&mut self, x: i32, z: i32, color: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.color_map_1[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw wall color channel 0.
    #[func]
    pub fn draw_wall_color_0(&mut self, x: i32, z: i32, color: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.wall_color_map_0[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw wall color channel 1.
    #[func]
    pub fn draw_wall_color_1(&mut self, x: i32, z: i32, color: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.wall_color_map_1[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw grass mask.
    #[func]
    pub fn draw_grass_mask(&mut self, x: i32, z: i32, masked: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.grass_mask_map[(z * dim_x + x) as usize] = masked;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Get ground color channel 0 at a grid point.
    #[func]
    pub fn get_color_0(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.color_map_0[(z * dim_x + x) as usize]
    }

    /// Get ground color channel 1 at a grid point.
    #[func]
    pub fn get_color_1(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.color_map_1[(z * dim_x + x) as usize]
    }

    /// Get wall color channel 0 at a grid point.
    #[func]
    pub fn get_wall_color_0(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.wall_color_map_0[(z * dim_x + x) as usize]
    }

    /// Get wall color channel 1 at a grid point.
    #[func]
    pub fn get_wall_color_1(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.wall_color_map_1[(z * dim_x + x) as usize]
    }

    /// Get grass mask at a grid point.
    #[func]
    pub fn get_grass_mask_at(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.grass_mask_map[(z * dim_x + x) as usize]
    }
}

impl PixyTerrainChunk {
    /// Set terrain configuration (called by terrain when adding/initializing chunk).
    /// This caches all needed terrain data so we don't need to bind terrain later.
    pub fn set_terrain_config(&mut self, config: TerrainConfig) {
        self.terrain_config = config;
    }

    /// Get the cached terrain config (for external callers).
    #[allow(dead_code)]
    pub fn get_terrain_config(&self) -> &TerrainConfig {
        &self.terrain_config
    }

    /// Get dimensions from cached config.
    fn get_dimensions_xz(&self) -> (i32, i32) {
        (
            self.terrain_config.dimensions.x,
            self.terrain_config.dimensions.z,
        )
    }

    fn get_terrain_dimensions(&self) -> Vector3i {
        self.terrain_config.dimensions
    }

    fn get_cell_size(&self) -> Vector2 {
        self.terrain_config.cell_size
    }

    fn get_blend_mode(&self) -> i32 {
        self.terrain_config.blend_mode
    }

    fn get_use_ridge_texture(&self) -> bool {
        self.terrain_config.use_ridge_texture
    }

    fn get_ridge_threshold(&self) -> f32 {
        self.terrain_config.ridge_threshold
    }

    /// Get height at a specific grid coordinate, returning None if out of bounds.
    pub fn get_height_at(&self, x: i32, z: i32) -> Option<f32> {
        let z_idx = z as usize;
        let x_idx = x as usize;
        self.height_map
            .get(z_idx)
            .and_then(|row| row.get(x_idx))
            .copied()
    }

    /// Set height at a specific grid coordinate.
    /// Guards against NaN/Inf values to prevent mesh corruption.
    pub fn set_height_at(&mut self, x: i32, z: i32, h: f32) {
        let z_idx = z as usize;
        let x_idx = x as usize;
        if z_idx < self.height_map.len() && x_idx < self.height_map[z_idx].len() {
            // Guard against NaN/Inf - use 0.0 as fallback
            let safe_h = if h.is_finite() {
                h
            } else {
                godot_warn!("NaN/Inf height detected at ({}, {}), using 0.0", x, z);
                0.0
            };
            self.height_map[z_idx][x_idx] = safe_h;
        }
    }

    // ═══════════════════════════════════════════
    // Data Persistence: Packed Array Conversion
    // ═══════════════════════════════════════════

    /// Sync runtime data to packed arrays for scene serialization.
    pub fn sync_to_packed(&mut self) {
        let dim = self.get_terrain_dimensions();
        let dim_x = dim.x as usize;
        let dim_z = dim.z as usize;
        let expected_total = dim_x * dim_z;

        // Validate height map dimensions before packing
        if !self.height_map.is_empty() {
            if self.height_map.len() != dim_z {
                godot_warn!(
                    "PixyTerrainChunk: sync_to_packed height_map row count mismatch: {} vs expected {}",
                    self.height_map.len(),
                    dim_z
                );
                return;
            }
            for (z, row) in self.height_map.iter().enumerate() {
                if row.len() != dim_x {
                    godot_warn!(
                        "PixyTerrainChunk: sync_to_packed height_map[{}] col count mismatch: {} vs expected {}",
                        z,
                        row.len(),
                        dim_x
                    );
                    return;
                }
            }
        }

        // Validate color map lengths before packing
        if !self.color_map_0.is_empty() && self.color_map_0.len() != expected_total {
            godot_warn!(
                "PixyTerrainChunk: sync_to_packed color_map_0 size mismatch: {} vs expected {}",
                self.color_map_0.len(),
                expected_total
            );
            return;
        }

        // Height map: flatten 2D → 1D (row-major: z * dim_x + x)
        if !self.height_map.is_empty() {
            let mut packed = PackedFloat32Array::new();
            packed.resize(dim_x * dim_z);
            for z in 0..dim_z {
                for x in 0..dim_x {
                    packed[z * dim_x + x] = self.height_map[z][x];
                }
            }
            self.saved_height_map = packed;
        }

        // Color maps
        self.saved_color_map_0 = Self::vec_color_to_packed(&self.color_map_0);
        self.saved_color_map_1 = Self::vec_color_to_packed(&self.color_map_1);
        self.saved_wall_color_map_0 = Self::vec_color_to_packed(&self.wall_color_map_0);
        self.saved_wall_color_map_1 = Self::vec_color_to_packed(&self.wall_color_map_1);
        self.saved_grass_mask_map = Self::vec_color_to_packed(&self.grass_mask_map);
    }

    /// Restore runtime data from packed arrays (after scene load).
    /// Returns true if data was restored, false if packed arrays were empty.
    fn restore_from_packed(&mut self) -> bool {
        let dim = self.get_terrain_dimensions();
        let dim_x = dim.x as usize;
        let dim_z = dim.z as usize;
        let expected_total = dim_x * dim_z;

        // Check if height data was saved
        if self.saved_height_map.len() != expected_total {
            return false;
        }

        // Restore height map from flat packed array → 2D Vec
        self.height_map = Vec::with_capacity(dim_z);
        for z in 0..dim_z {
            let mut row = Vec::with_capacity(dim_x);
            for x in 0..dim_x {
                row.push(self.saved_height_map[z * dim_x + x]);
            }
            self.height_map.push(row);
        }

        // Restore color maps
        self.color_map_0 = Self::packed_to_vec_color(&self.saved_color_map_0, expected_total);
        self.color_map_1 = Self::packed_to_vec_color(&self.saved_color_map_1, expected_total);
        self.wall_color_map_0 =
            Self::packed_to_vec_color(&self.saved_wall_color_map_0, expected_total);
        self.wall_color_map_1 =
            Self::packed_to_vec_color(&self.saved_wall_color_map_1, expected_total);
        self.grass_mask_map = Self::packed_to_vec_color(&self.saved_grass_mask_map, expected_total);

        godot_print!(
            "PixyTerrainChunk: Restored data from saved arrays for chunk ({}, {})",
            self.chunk_coords.x,
            self.chunk_coords.y
        );
        true
    }

    /// Convert Vec<Color> to PackedColorArray.
    fn vec_color_to_packed(colors: &[Color]) -> PackedColorArray {
        let mut packed = PackedColorArray::new();
        packed.resize(colors.len());
        for (i, color) in colors.iter().enumerate() {
            packed[i] = *color;
        }
        packed
    }

    /// Convert PackedColorArray to Vec<Color>, with fallback if wrong size.
    fn packed_to_vec_color(packed: &PackedColorArray, expected: usize) -> Vec<Color> {
        if packed.len() == expected {
            (0..expected).map(|i| packed[i]).collect()
        } else {
            vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); expected]
        }
    }

    // ═══════════════════════════════════════════
    // Initialization
    // ═══════════════════════════════════════════

    /// Initialize terrain data (called by terrain parent after adding chunk to tree).
    /// All needed data is passed as parameters to avoid needing to bind terrain.
    pub fn initialize_terrain(
        &mut self,
        should_regenerate_mesh: bool,
        noise: Option<Gd<Noise>>,
        terrain_material: Option<Gd<ShaderMaterial>>,
        grass_config: GrassConfig,
    ) {
        if !Engine::singleton().is_editor_hint() {
            godot_error!(
                "PixyTerrainChunk: Trying to initialize terrain during runtime (NOT SUPPORTED)"
            );
            return;
        }

        // Store terrain material reference for use by regenerate_mesh()
        self.terrain_material = terrain_material.clone();

        let dim = self.get_terrain_dimensions();

        // Initialize needs_update grid
        self.needs_update = Vec::with_capacity((dim.z - 1) as usize);
        for _ in 0..(dim.z - 1) {
            self.needs_update.push(vec![true; (dim.x - 1) as usize]);
        }

        // Try to restore from saved packed arrays first (scene reload)
        let restored = self.restore_from_packed();

        if !restored {
            // Generate fresh data maps
            if self.height_map.is_empty() {
                self.generate_height_map_with_noise(noise);
            }
            if self.color_map_0.is_empty() || self.color_map_1.is_empty() {
                self.generate_color_maps();
            }
            if self.wall_color_map_0.is_empty() || self.wall_color_map_1.is_empty() {
                self.generate_wall_color_maps();
            }
            if self.grass_mask_map.is_empty() {
                self.generate_grass_mask_map();
            }
        }

        // Reuse existing grass planter from scene save, or create new one
        if self.grass_planter.is_none() {
            let name = GString::from("GrassPlanter");
            if let Some(child) = self
                .base()
                .find_child_ex(&name)
                .recursive(false)
                .owned(false)
                .done()
            {
                if let Ok(mut planter) = child.try_cast::<PixyGrassPlanter>() {
                    let chunk_id = self.base().instance_id();
                    planter
                        .bind_mut()
                        .setup_with_config(chunk_id, grass_config.clone(), true);
                    self.grass_planter = Some(planter);
                }
            }
        }

        if self.grass_planter.is_none() {
            // Create new planter (for genuinely new chunks)
            let mut planter = PixyGrassPlanter::new_alloc();
            planter.set_name("GrassPlanter");

            let chunk_id = self.base().instance_id();
            planter
                .bind_mut()
                .setup_with_config(chunk_id, grass_config, true);

            self.base_mut().add_child(&planter);

            // Set owner for editor persistence
            if let Some(owner) = self.base().get_owner() {
                planter.set_owner(&owner);
            }

            self.grass_planter = Some(planter);
        }

        if should_regenerate_mesh && self.base().get_mesh().is_none() {
            self.regenerate_mesh_with_material(terrain_material);
        }
    }

    /// Generate a flat height map, optionally seeded by noise (passed as parameter).
    pub fn generate_height_map_with_noise(&mut self, noise: Option<Gd<Noise>>) {
        let dim = self.get_terrain_dimensions();
        let dim_x = dim.x as usize;
        let dim_z = dim.z as usize;

        self.height_map = vec![vec![0.0; dim_x]; dim_z];

        if let Some(noise) = noise {
            for z in 0..dim_z {
                for x in 0..dim_x {
                    let noise_x = (self.chunk_coords.x * (dim.x - 1)) + x as i32;
                    let noise_z = (self.chunk_coords.y * (dim.z - 1)) + z as i32;
                    let sample = noise.get_noise_2d(noise_x as f32, noise_z as f32);
                    self.height_map[z][x] = sample * dim.y as f32;
                }
            }
        }
    }

    /// Generate default ground color maps (texture slot 0).
    pub fn generate_color_maps(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        self.color_map_0 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
        self.color_map_1 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
    }

    /// Generate default wall color maps (texture slot 0).
    pub fn generate_wall_color_maps(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        self.wall_color_map_0 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
        self.wall_color_map_1 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
    }

    /// Generate default grass mask map (all enabled).
    pub fn generate_grass_mask_map(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        self.grass_mask_map = vec![Color::from_rgba(1.0, 1.0, 1.0, 1.0); total];
    }

    /// Rebuild the mesh using SurfaceTool with CUSTOM0-2 format.
    /// Uses the stored terrain_material reference (set during initialize_terrain).
    pub fn regenerate_mesh(&mut self) {
        // Use stored material reference instead of None
        let material = self.terrain_material.clone();
        self.regenerate_mesh_with_material(material);
    }

    /// Rebuild the mesh using SurfaceTool with CUSTOM0-2 format.
    /// Material is passed as parameter to avoid needing to bind terrain.
    pub fn regenerate_mesh_with_material(&mut self, terrain_material: Option<Gd<ShaderMaterial>>) {
        // Clear geometry cache to force full regeneration (debug: isolate caching issues)
        self.cell_geometry.clear();

        // Mark all cells as needing update
        let (dim_x, dim_z) = self.get_dimensions_xz();
        for z in 0..(dim_z - 1) {
            for x in 0..(dim_x - 1) {
                self.needs_update[z as usize][x as usize] = true;
            }
        }

        let mut st = SurfaceTool::new_gd();
        st.begin(PrimitiveType::TRIANGLES);
        st.set_custom_format(0, CustomFormat::RGBA_FLOAT);
        st.set_custom_format(1, CustomFormat::RGBA_FLOAT);
        st.set_custom_format(2, CustomFormat::RGBA_FLOAT);

        self.generate_terrain_cells(&mut st);

        st.generate_normals();
        st.index();

        let mesh = st.commit();
        if let Some(mesh) = mesh {
            // Apply terrain material if available (passed as parameter)
            if let Some(ref mat) = terrain_material {
                mesh.clone()
                    .cast::<ArrayMesh>()
                    .surface_set_material(0, mat);
            }
            self.base_mut().set_mesh(&mesh);
        }

        // Remove old collision body before creating a new one (prevents leaking StaticBody3D children)
        let children = self.base().get_children();
        for i in (0..children.len()).rev() {
            if let Some(child) = children.get(i) {
                if child.is_class("StaticBody3D") {
                    let mut child = child;
                    self.base_mut().remove_child(&child);
                    child.queue_free();
                }
            }
        }

        // Create trimesh collision
        self.base_mut().create_trimesh_collision();

        // Configure collision layer on the generated StaticBody3D
        self.configure_collision_layer();

        // Sync data to packed arrays for persistence
        self.sync_to_packed();

        // Regenerate grass on top of the new mesh
        // Pass cell_geometry directly to avoid grass planter needing to bind chunk
        if let Some(ref mut planter) = self.grass_planter {
            planter
                .bind_mut()
                .regenerate_all_cells_with_geometry(&self.cell_geometry);
        }

        godot_print!(
            "PixyTerrainChunk: Mesh regenerated for chunk ({}, {})",
            self.chunk_coords.x,
            self.chunk_coords.y
        );
    }

    /// Generate terrain cells, using cached geometry for unchanged cells.
    fn generate_terrain_cells(&mut self, st: &mut Gd<SurfaceTool>) {
        let dim = self.get_terrain_dimensions();
        let cell_size = self.get_cell_size();
        let merge_threshold = MergeMode::from_index(self.merge_mode).threshold();
        let blend_mode = self.get_blend_mode();
        let use_ridge_texture = self.get_use_ridge_texture();
        let ridge_threshold = self.get_ridge_threshold();

        // Get chunk position once for wall UV2 offset
        let chunk_position = if self.base().is_inside_tree() {
            self.base().get_global_position()
        } else {
            self.base().get_position()
        };

        // Create CellContext once, moving color maps in (avoids cloning per cell).
        // Maps are moved back after the loop.
        let default_color = Color::from_rgba(1.0, 0.0, 0.0, 0.0);
        let mut ctx = CellContext {
            heights: [0.0; 4],
            edges: [true; 4],
            rotation: 0,
            cell_coords: Vector2i::ZERO,
            dimensions: dim,
            cell_size,
            merge_threshold,
            higher_poly_floors: self.higher_poly_floors,
            color_map_0: std::mem::take(&mut self.color_map_0),
            color_map_1: std::mem::take(&mut self.color_map_1),
            wall_color_map_0: std::mem::take(&mut self.wall_color_map_0),
            wall_color_map_1: std::mem::take(&mut self.wall_color_map_1),
            grass_mask_map: std::mem::take(&mut self.grass_mask_map),
            cell_min_height: 0.0,
            cell_max_height: 0.0,
            cell_is_boundary: false,
            cell_floor_lower_color_0: default_color,
            cell_floor_upper_color_0: default_color,
            cell_floor_lower_color_1: default_color,
            cell_floor_upper_color_1: default_color,
            cell_wall_lower_color_0: default_color,
            cell_wall_upper_color_0: default_color,
            cell_wall_lower_color_1: default_color,
            cell_wall_upper_color_1: default_color,
            cell_mat_a: 0,
            cell_mat_b: 0,
            cell_mat_c: 0,
            blend_mode,
            use_ridge_texture,
            ridge_threshold,
            is_new_chunk: self.is_new_chunk,
            floor_mode: true,
            lower_thresh: 0.3,
            upper_thresh: 0.7,
            chunk_position,
        };

        for z in 0..(dim.z - 1) {
            for x in 0..(dim.x - 1) {
                let key = [x, z];

                // If geometry didn't change, replay cached geometry
                if !self.needs_update[z as usize][x as usize] {
                    if let Some(geo) = self.cell_geometry.get(&key) {
                        let _ = replay_geometry(st, geo);
                        continue;
                    }
                }

                // Mark cell as updated
                self.needs_update[z as usize][x as usize] = false;

                // Get corner heights: A(top-left), B(top-right), C(bottom-left), D(bottom-right)
                let ay = self.height_map[z as usize][x as usize];
                let by = self.height_map[z as usize][(x + 1) as usize];
                let cy = self.height_map[(z + 1) as usize][x as usize];
                let dy = self.height_map[(z + 1) as usize][(x + 1) as usize];

                // Update per-cell context fields (reuse shared ctx)
                // CRITICAL: Reset ALL per-cell state to avoid corruption from previous cells
                ctx.heights = [ay, by, dy, cy];
                ctx.edges = [true; 4];
                ctx.rotation = 0;
                ctx.cell_coords = Vector2i::new(x, z);
                ctx.cell_min_height = 0.0;
                ctx.cell_max_height = 0.0;
                ctx.cell_is_boundary = false;
                ctx.floor_mode = true;

                // Reset color/material state that persists from previous cell
                ctx.cell_floor_lower_color_0 = default_color;
                ctx.cell_floor_upper_color_0 = default_color;
                ctx.cell_floor_lower_color_1 = default_color;
                ctx.cell_floor_upper_color_1 = default_color;
                ctx.cell_wall_lower_color_0 = default_color;
                ctx.cell_wall_upper_color_0 = default_color;
                ctx.cell_wall_lower_color_1 = default_color;
                ctx.cell_wall_upper_color_1 = default_color;
                ctx.cell_mat_a = 0;
                ctx.cell_mat_b = 0;
                ctx.cell_mat_c = 0;

                let mut geo = CellGeometry::default();

                // Generate geometry for this cell
                marching_squares::generate_cell(&mut ctx, &mut geo);

                // Validate geometry before commit - must have complete triangles
                if geo.verts.len() % 3 != 0 {
                    godot_error!(
                        "Cell ({}, {}) generated invalid geometry: {} verts (not divisible by 3). Heights: [{:.2}, {:.2}, {:.2}, {:.2}]. Replacing with flat floor.",
                        x, z, geo.verts.len(),
                        ctx.heights[0], ctx.heights[1], ctx.heights[2], ctx.heights[3]
                    );
                    // Reset and generate safe fallback (flat floor)
                    geo = CellGeometry::default();
                    ctx.rotation = 0;
                    marching_squares::add_full_floor(&mut ctx, &mut geo);
                }

                // Commit geometry to SurfaceTool
                let _ = replay_geometry(st, &geo);

                // Cache the geometry
                self.cell_geometry.insert(key, geo);
            }
        }

        // Move color maps back (may have been mutated by new_chunk source map writes)
        self.color_map_0 = ctx.color_map_0;
        self.color_map_1 = ctx.color_map_1;
        self.wall_color_map_0 = ctx.wall_color_map_0;
        self.wall_color_map_1 = ctx.wall_color_map_1;
        self.grass_mask_map = ctx.grass_mask_map;

        if self.is_new_chunk {
            self.is_new_chunk = false;
        }
    }

    /// Mark a cell as needing update.
    /// Also marks adjacent cells (8 neighbors) because their geometry depends on edge connectivity.
    pub fn notify_needs_update(&mut self, z: i32, x: i32) {
        self.mark_cell_needs_update(x, z);
    }

    /// Mark a cell and its 8 neighbors as needing update.
    /// Adjacent cells must be invalidated because marching squares geometry depends on
    /// edge connectivity with neighbors - a height change in one cell affects how
    /// adjacent cells render their shared edges.
    fn mark_cell_needs_update(&mut self, x: i32, z: i32) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        for dz in -1..=1 {
            for dx in -1..=1 {
                let nx = x + dx;
                let nz = z + dz;
                if nx >= 0 && nx < dim_x - 1 && nz >= 0 && nz < dim_z - 1 {
                    self.needs_update[nz as usize][nx as usize] = true;
                    // Also invalidate cached geometry for this cell
                    self.cell_geometry.remove(&[nx, nz]);
                }
            }
        }
    }

    /// Configure collision layer on the StaticBody3D created by create_trimesh_collision().
    /// Sets layer 17 (bit 16) as the base terrain collision layer, plus any extra layer
    /// specified by the terrain's extra_collision_layer setting.
    fn configure_collision_layer(&mut self) {
        // Look for the collision body child created by create_trimesh_collision()
        // It will be named something like "ChunkName_col"
        let children = self.base().get_children();
        for i in 0..children.len() {
            let Some(child) = children.get(i) else {
                continue;
            };
            if let Ok(mut body) = child.try_cast::<StaticBody3D>() {
                // Set layer 17 (bit 16) as base terrain layer
                body.set_collision_layer(1 << 16);

                // Add extra collision layer from cached terrain config
                let extra = self.terrain_config.extra_collision_layer;
                if (1..=32).contains(&extra) {
                    body.set_collision_layer_value(extra, true);
                }
                return;
            }
        }
    }
}

/// Replay cached geometry into a SurfaceTool.
/// Returns true if geometry was valid and added, false if skipped due to invalid vertex count.
fn replay_geometry(st: &mut Gd<SurfaceTool>, geo: &CellGeometry) -> bool {
    // CRITICAL: Skip cells with incomplete triangles to prevent index errors
    if geo.verts.len() % 3 != 0 {
        godot_warn!(
            "Skipping cell with invalid vertex count: {} (not divisible by 3)",
            geo.verts.len()
        );
        return false;
    }

    for i in 0..geo.verts.len() {
        // Additional NaN guard on vertex position
        let vert = geo.verts[i];
        if !vert.x.is_finite() || !vert.y.is_finite() || !vert.z.is_finite() {
            godot_warn!(
                "Skipping vertex with NaN/Inf coordinates: ({}, {}, {})",
                vert.x,
                vert.y,
                vert.z
            );
            // Skip the entire cell since we can't have incomplete triangles
            return false;
        }

        let smooth_group = if geo.is_floor[i] { 0 } else { u32::MAX };
        st.set_smooth_group(smooth_group);
        st.set_uv(geo.uvs[i]);
        st.set_uv2(geo.uv2s[i]);
        st.set_color(geo.colors_0[i]);
        st.set_custom(0, geo.colors_1[i]);
        st.set_custom(1, geo.grass_mask[i]);
        st.set_custom(2, geo.mat_blend[i]);
        st.add_vertex(vert);
    }
    true
}
