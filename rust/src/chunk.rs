// Pixy Terrain — Chunk mesh generation
//
// Original chunk algorithm ported from Yugen's marching_squares_terrain_chunk.gd:
//   https://github.com/Yukitty/Yugens-Terrain-Authoring-Toolkit

use std::collections::HashMap;

use godot::classes::mesh::PrimitiveType;
use godot::classes::surface_tool::CustomFormat;
use godot::classes::{
    CollisionShape3D, ConcavePolygonShape3D, IMeshInstance3D, MeshInstance3D, Noise,
    ShaderMaterial, StaticBody3D, SurfaceTool,
};
use godot::prelude::*;

use crate::grass_planter::{GrassConfig, PixyGrassPlanter};
use crate::marching_squares::{
    self, validate_cell_watertight, BlendMode, CellContext, CellGeometry, MergeMode,
};

/// Cached terrain configuration (avoids needing to bind terrain during chunk operations)
/// Passed from terrain to chunk at initialization time to break the borrow cycle
#[derive(Clone, Debug)]
pub struct TerrainConfig {
    pub dimensions: Vector3i,
    pub cell_size: Vector2,
    pub blend_mode: BlendMode,
    pub use_ridge_texture: bool,
    pub ridge_threshold: f32,
    pub extra_collision_layer: i32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            dimensions: Vector3i::new(33, 32, 33),
            cell_size: Vector2::new(2.0, 2.0),
            blend_mode: BlendMode::Direct,
            use_ridge_texture: false,
            ridge_threshold: 1.0,
            extra_collision_layer: 9,
        }
    }
}

/// Per-chunk mesh instance that holds heightmap data and generates geometry.
#[derive(GodotClass)]
#[class(base=MeshInstance3D, init, tool)]
pub struct PixyTerrainChunk {
    base: Base<MeshInstance3D>,

    #[export]
    pub chunk_coords: Vector2i,

    #[export]
    #[init(val = 1)]
    pub merge_mode: i32,

    // ═══════════════════════════════════════════
    // Persisted Terrain Data (Godot PackedArrays)
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = PackedFloat32Array::new())]
    pub saved_height_map: PackedFloat32Array,

    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_color_map_0: PackedColorArray,

    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_color_map_1: PackedColorArray,

    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_wall_color_map_0: PackedColorArray,

    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_wall_color_map_1: PackedColorArray,

    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_grass_mask_map: PackedColorArray,

    // ═══════════════════════════════════════════
    // Runtime Terrain Data Maps (working copies)
    // ═══════════════════════════════════════════
    pub height_map: Vec<Vec<f32>>,
    pub color_map_0: Vec<Color>,
    pub color_map_1: Vec<Color>,
    pub wall_color_map_0: Vec<Color>,
    pub wall_color_map_1: Vec<Color>,
    pub grass_mask_map: Vec<Color>,
    pub needs_update: Vec<Vec<bool>>,
    pub cell_geometry: HashMap<[i32; 2], CellGeometry>,

    #[init(val = true)]
    pub higher_poly_floors: bool,

    pub is_new_chunk: bool,

    #[export]
    #[init(val = false)]
    pub skip_save_on_exit: bool,

    terrain_config: TerrainConfig,
    terrain_material: Option<Gd<ShaderMaterial>>,
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
        if !self.skip_save_on_exit {
            self.sync_to_packed();
        }
    }
}

#[godot_api]
impl PixyTerrainChunk {
    #[func]
    pub fn regenerate_all_cells(&mut self) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        for z in 0..(dim_z - 1) {
            for x in 0..(dim_x - 1) {
                self.needs_update[z as usize][x as usize] = true
            }
        }
        self.regenerate_mesh();
    }

    #[func]
    pub fn get_height(&self, coords: Vector2i) -> f32 {
        if !self.is_in_bounds(coords.x, coords.y) {
            return 0.0;
        }
        self.height_map[coords.y as usize][coords.x as usize]
    }

    #[func]
    pub fn draw_height(&mut self, x: i32, z: i32, y: f32) {
        if !self.is_in_bounds(x, z) {
            return;
        }
        self.height_map[z as usize][x as usize] = y;
        self.notify_neighbors(x, z);
    }

    #[func]
    pub fn draw_color_0(&mut self, x: i32, z: i32, color: Color) {
        if !self.is_in_bounds(x, z) {
            return;
        }

        let dim_x = self.terrain_config.dimensions.x;
        self.color_map_0[(z * dim_x + x) as usize] = color;
        self.notify_neighbors(x, z);
    }

    #[func]
    pub fn draw_color_1(&mut self, x: i32, z: i32, color: Color) {
        if !self.is_in_bounds(x, z) {
            return;
        }

        let dim_x = self.terrain_config.dimensions.x;
        self.color_map_1[(z * dim_x + x) as usize] = color;
        self.notify_neighbors(x, z);
    }

    #[func]
    pub fn draw_wall_color_0(&mut self, x: i32, z: i32, color: Color) {
        if !self.is_in_bounds(x, z) {
            return;
        }

        let dim_x = self.terrain_config.dimensions.x;
        self.wall_color_map_0[(z * dim_x + x) as usize] = color;
        self.notify_neighbors(x, z);
    }

    #[func]
    pub fn draw_wall_color_1(&mut self, x: i32, z: i32, color: Color) {
        if !self.is_in_bounds(x, z) {
            return;
        }

        let dim_x = self.terrain_config.dimensions.x;
        self.wall_color_map_1[(z * dim_x + x) as usize] = color;
        self.notify_neighbors(x, z);
    }

    #[func]
    pub fn draw_grass_mask(&mut self, x: i32, z: i32, masked: Color) {
        if !self.is_in_bounds(x, z) {
            return;
        }

        let dim_x = self.terrain_config.dimensions.x;
        self.grass_mask_map[(z * dim_x + x) as usize] = masked;
        self.notify_neighbors(x, z);
    }

    #[func]
    pub fn get_color_0(&self, x: i32, z: i32) -> Color {
        if !self.is_in_bounds(x, z) {
            return Color::default();
        }
        let dim_x = self.terrain_config.dimensions.x;
        self.color_map_0[(z * dim_x + x) as usize]
    }

    #[func]
    pub fn get_color_1(&self, x: i32, z: i32) -> Color {
        if !self.is_in_bounds(x, z) {
            return Color::default();
        }
        let dim_x = self.terrain_config.dimensions.x;
        self.color_map_1[(z * dim_x + x) as usize]
    }

    #[func]
    pub fn get_wall_color_0(&self, x: i32, z: i32) -> Color {
        if !self.is_in_bounds(x, z) {
            return Color::default();
        }
        let dim_x = self.terrain_config.dimensions.x;
        self.wall_color_map_0[(z * dim_x + x) as usize]
    }

    #[func]
    pub fn get_wall_color_1(&self, x: i32, z: i32) -> Color {
        if !self.is_in_bounds(x, z) {
            return Color::default();
        }
        let dim_x = self.terrain_config.dimensions.x;
        self.wall_color_map_1[(z * dim_x + x) as usize]
    }

    #[func]
    pub fn get_grass_mask_at(&self, x: i32, z: i32) -> Color {
        if !self.is_in_bounds(x, z) {
            return Color::default();
        }
        let dim_x = self.terrain_config.dimensions.x;
        self.grass_mask_map[(z * dim_x + x) as usize]
    }

    #[func]
    pub fn validate_mesh_gaps(&self) -> i32 {
        let cell_size = self.terrain_config.cell_size;
        let mut total_gaps = 0i32;
        for (key, geo) in &self.cell_geometry {
            let cell_x = key[0];
            let cell_z = key[1];
            let result = validate_cell_watertight(geo, cell_x, cell_z, cell_size);
            if !result.is_watertight {
                let gap_count = result.open_edges.len();
                total_gaps += gap_count as i32;
                for (a, b) in &result.open_edges {
                    godot_warn!(
                        "Mesh gap in cell ({},{}): ({:.3},{:.3},{:.3}) - ({:.3},{:.3},{:.3})",
                        cell_x,
                        cell_z,
                        a.x,
                        a.y,
                        a.z,
                        b.x,
                        b.y,
                        b.z
                    );
                }
            }
        }
        if total_gaps == 0 {
            godot_print!("Mesh validation passed: no gaps found.");
        } else {
            godot_warn!("Mesh validation found {} total gaps.", total_gaps);
        }
        total_gaps
    }
}

impl PixyTerrainChunk {
    pub fn set_terrain_config(&mut self, config: TerrainConfig) {
        self.terrain_config = config;
    }

    #[allow(dead_code)]
    pub fn get_terrain_config(&self) -> &TerrainConfig {
        &self.terrain_config
    }

    fn get_cell_size(&self) -> Vector2 {
        self.terrain_config.cell_size
    }

    fn get_blend_mode(&self) -> BlendMode {
        self.terrain_config.blend_mode
    }

    fn get_use_ridge_texture(&self) -> bool {
        self.terrain_config.use_ridge_texture
    }
    fn get_ridge_threshold(&self) -> f32 {
        self.terrain_config.ridge_threshold
    }

    pub fn get_height_at(&self, x: i32, z: i32) -> Option<f32> {
        self.height_map
            .get(z as usize)
            .and_then(|row| row.get(x as usize))
            .copied()
    }

    pub fn set_height_at(&mut self, x: i32, z: i32, h: f32) {
        let z_idx = z as usize;
        let x_idx = x as usize;
        if z_idx < self.height_map.len() && x_idx < self.height_map[z_idx].len() {
            let safe_h = if h.is_finite() {
                h
            } else {
                godot_warn!(
                    "NaN/Inf height detected at ({}, {}),
  using 0.0",
                    x,
                    z
                );
                0.0
            };
            self.height_map[z_idx][x_idx] = safe_h;
        }
    }

    fn get_dimensions_xz(&self) -> (i32, i32) {
        (
            self.terrain_config.dimensions.x,
            self.terrain_config.dimensions.z,
        )
    }

    fn get_terrain_dimensions(&self) -> Vector3i {
        self.terrain_config.dimensions
    }

    pub fn notify_needs_update(&mut self, z: i32, x: i32) {
        self.mark_cell_needs_update(x, z);
    }

    fn mark_cell_needs_update(&mut self, x: i32, z: i32) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        for dz in -1..=1 {
            for dx in -1..=1 {
                let nx = x + dx;
                let nz = z + dz;
                if nx >= 0 && nx < dim_x - 1 && nz >= 0 && nz < dim_z - 1 {
                    self.needs_update[nz as usize][nx as usize] = true;
                    self.cell_geometry.remove(&[nx, nz]);
                }
            }
        }
    }

    fn is_in_bounds(&self, x: i32, z: i32) -> bool {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        x >= 0 && z >= 0 && x < dim_x && z < dim_z
    }

    /// Mark the 4 cells that shar grid point (x, z) as dirty
    fn notify_neighbors(&mut self, x: i32, z: i32) {
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    // ================================
    // === Packed Array Persistence ===
    // ================================

    pub fn sync_to_packed(&mut self) {
        let dim = self.get_terrain_dimensions();
        let dim_x = dim.x as usize;
        let dim_z = dim.z as usize;
        let expected_total = dim_x * dim_z;

        // Guard
        if !self.height_map.is_empty() {
            if self.height_map.len() != dim_z {
                godot_warn!(
                    "sync_to_packed height_map row mismatch: {} vs
  {}",
                    self.height_map.len(),
                    dim_z
                );
                return;
            }
            for (z, row) in self.height_map.iter().enumerate() {
                if row.len() != dim_x {
                    godot_warn!(
                        "sync_to_packed height_map[{}] col mismatch:
   {} vs {}",
                        z,
                        row.len(),
                        dim_x
                    );
                    return;
                }
            }
        }

        if !self.color_map_0.is_empty() && self.color_map_0.len() != expected_total {
            godot_warn!(
                "sync_to_packed color_map_0 size mismatch: {} vs
  {}",
                self.color_map_0.len(),
                expected_total
            );
            return;
        }

        // Guard complete
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

    fn restore_from_packed(&mut self) -> bool {
        let dim = self.get_terrain_dimensions();
        let dim_x = dim.x as usize;
        let dim_z = dim.z as usize;
        let expected_total = dim_x * dim_z;

        if self.saved_height_map.len() != expected_total {
            return false;
        }

        self.height_map = Vec::with_capacity(dim_z);
        for z in 0..dim_z {
            let mut row = Vec::with_capacity(dim_x);
            for x in 0..dim_x {
                row.push(self.saved_height_map[z * dim_x + x])
            }
            self.height_map.push(row);
        }

        self.color_map_0 = Self::packed_to_vec_color(&self.saved_color_map_0, expected_total);
        self.color_map_1 = Self::packed_to_vec_color(&self.saved_color_map_1, expected_total);
        self.wall_color_map_0 =
            Self::packed_to_vec_color(&self.saved_wall_color_map_0, expected_total);
        self.wall_color_map_1 =
            Self::packed_to_vec_color(&self.saved_wall_color_map_1, expected_total);
        self.grass_mask_map = Self::packed_to_vec_color(&self.saved_grass_mask_map, expected_total);

        godot_print!(
            "Restored data from saved arrays for chunk ({}, {})",
            self.chunk_coords.x,
            self.chunk_coords.y
        );
        true
    }

    fn vec_color_to_packed(colors: &[Color]) -> PackedColorArray {
        let mut packed = PackedColorArray::new();
        packed.resize(colors.len());
        for (i, color) in colors.iter().enumerate() {
            packed[i] = *color;
        }
        packed
    }

    fn packed_to_vec_color(packed: &PackedColorArray, expected: usize) -> Vec<Color> {
        if packed.len() == expected {
            (0..expected).map(|i| packed[i]).collect()
        } else {
            vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); expected]
        }
    }

    pub fn initialize_terrain(
        &mut self,
        should_regenerate_mesh: bool,
        noise: Option<Gd<Noise>>,
        terrain_material: Option<Gd<ShaderMaterial>>,
        grass_config: GrassConfig,
    ) {
        self.terrain_material = terrain_material.clone();
        let dim = self.get_terrain_dimensions();

        // Initialize needs_updated grid
        self.needs_update = Vec::with_capacity((dim.z - 1) as usize);
        for _ in 0..(dim.z - 1) {
            self.needs_update.push(vec![true; (dim.x - 1) as usize]);
        }

        // Try to restore from saved packed array first (scene reload)
        let restored = self.restore_from_packed();

        if !restored {
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

        // Reuse existing grass planter from scene save
        if self.grass_planter.is_none() {
            let name = GString::from("GrassPlanter");
            if let Some(child) = self
                .base()
                .find_child_ex(&name)
                .recursive(false)
                .owned(false)
                .done()
            {
                if let Ok(planter) = child.try_cast::<PixyGrassPlanter>() {
                    self.grass_planter = Some(planter);
                }
            }
        }

        // Create new grass planter if none found
        if self.grass_planter.is_none() {
            let mut planter = PixyGrassPlanter::new_alloc();
            planter.set_name("GrassPlanter");
            self.base_mut().add_child(&planter);
            self.grass_planter = Some(planter);
        }

        // Initialize the grass planter with config
        let chunk_id = self.base().instance_id();
        if let Some(ref mut planter) = self.grass_planter {
            planter
                .bind_mut()
                .setup_with_config(chunk_id, grass_config, true);
        }

        // Regenerate mesh + grass on scene reload (rebuilds cell_geometry from restored maps)
        if should_regenerate_mesh {
            self.regenerate_mesh();
        }
    }

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

    pub fn generate_color_maps(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        self.color_map_0 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
        self.color_map_1 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
    }

    pub fn generate_wall_color_maps(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        self.wall_color_map_0 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
        self.wall_color_map_1 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
    }

    pub fn generate_grass_mask_map(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        self.grass_mask_map = vec![Color::from_rgba(1.0, 1.0, 1.0, 1.0); total];
    }

    pub fn regenerate_mesh(&mut self) {
        let material = self.terrain_material.clone();
        self.regenerate_mesh_with_material(material);
    }

    pub fn regenerate_mesh_with_material(&mut self, _terrain_material: Option<Gd<ShaderMaterial>>) {
        self.cell_geometry.clear();

        let (dim_x, dim_z) = self.get_dimensions_xz();
        for z in 0..(dim_z - 1) {
            for x in 0..(dim_x - 1) {
                self.needs_update[z as usize][x as usize] = true
            }
        }

        let mut st = SurfaceTool::new_gd();
        st.begin(PrimitiveType::TRIANGLES);
        st.set_custom_format(0, CustomFormat::RGBA_FLOAT);
        st.set_custom_format(1, CustomFormat::RGBA_FLOAT);
        st.set_custom_format(2, CustomFormat::RGBA_FLOAT);

        self.generate_terrain_cells(&mut st);

        st.generate_normals();

        if let Some(mesh) = st.commit() {
            self.base_mut().set_mesh(&mesh);

            // Apply terrain material to surface 0
            let mat_clone = self.terrain_material.clone();
            if let Some(mat) = mat_clone {
                self.base_mut()
                    .set_surface_override_material(0, &mat.upcast::<godot::classes::Material>());
            }

            // Remove any existing StaticBody3D children (cleanup for re-generation)
            let children = self.base().get_children_ex().include_internal(true).done();
            for i in (0..children.len()).rev() {
                if let Some(child) = children.get(i) {
                    if child.is_class("StaticBody3D") {
                        let mut child = child;
                        self.base_mut().remove_child(&child);
                        child.queue_free();
                    }
                }
            }

            // Create collision via Godot's built-in method, then configure it.
            self.base_mut().create_trimesh_collision();
            self.configure_collision();
        }

        // Regenerate grass after mesh geometry is built
        if let Some(ref mut planter) = self.grass_planter {
            planter
                .bind_mut()
                .regenerate_all_cells_with_geometry(&self.cell_geometry);
        }

        // Keep packed arrays in sync so Ctrl+S captures current state
        self.sync_to_packed();
    }

    fn generate_terrain_cells(&mut self, st: &mut Gd<SurfaceTool>) {
        let dim = self.get_terrain_dimensions();
        let cell_size = self.get_cell_size();
        let merge_threshold = MergeMode::from_index(self.merge_mode).threshold();
        let blend_mode = self.get_blend_mode();
        let use_ridge_texture = self.get_use_ridge_texture();
        let ridge_threshold = self.get_ridge_threshold();

        let chunk_position = if self.base().is_inside_tree() {
            self.base().get_global_position()
        } else {
            self.base().get_position()
        };

        let mut ctx = CellContext {
            heights: [0.0; 4],
            edges: [true; 4],
            profiles: Default::default(),
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
            color_state: marching_squares::CellColorState::default(),
            blend_mode,
            use_ridge_texture,
            ridge_threshold,
            is_new_chunk: self.is_new_chunk,
            floor_mode: true,
            lower_threshold: 0.3,
            upper_threshold: 0.7,
            chunk_position,
        };

        for z in 0..(dim.z - 1) {
            for x in 0..(dim.x - 1) {
                let key = [x, z];

                if !self.needs_update[z as usize][x as usize] {
                    if let Some(geo) = self.cell_geometry.get(&key) {
                        let _ = replay_geometry(st, geo);
                        continue;
                    }
                }

                self.needs_update[z as usize][x as usize] = false;
                let ay = self.height_map[z as usize][x as usize];
                let by = self.height_map[z as usize][(x + 1) as usize];
                let cy = self.height_map[(z + 1) as usize][x as usize];
                let dy = self.height_map[(z + 1) as usize][(x + 1) as usize];

                ctx.heights = [ay, by, dy, cy];
                ctx.edges = [true; 4];
                ctx.rotation = 0;
                ctx.cell_coords = Vector2i::new(x, z);
                ctx.color_state = marching_squares::CellColorState::default();
                ctx.floor_mode = true;

                let mut geo = CellGeometry::default();
                marching_squares::generate_cell(&mut ctx, &mut geo);

                if geo.verts.len() % 3 != 0 {
                    godot_error!(
                        "Cell ({}, {}) invalid geometry: {} verts.
  Replacing with flat floor.",
                        x,
                        z,
                        geo.verts.len()
                    );
                    geo = CellGeometry::default();
                    ctx.rotation = 0;
                    marching_squares::add_full_floor(&mut ctx, &mut geo);
                }

                let _ = replay_geometry(st, &geo);
                self.cell_geometry.insert(key, geo);
            }
        }

        // Move color maps back
        self.color_map_0 = ctx.color_map_0;
        self.color_map_1 = ctx.color_map_1;
        self.wall_color_map_0 = ctx.wall_color_map_0;
        self.wall_color_map_1 = ctx.wall_color_map_1;
        self.grass_mask_map = ctx.grass_mask_map;

        if self.is_new_chunk {
            self.is_new_chunk = false;
        }
    }

    fn configure_collision(&mut self) {
        let children = self.base().get_children_ex().include_internal(true).done();
        for i in 0..children.len() {
            let Some(child) = children.get(i) else {
                continue;
            };
            if let Ok(mut body) = child.try_cast::<StaticBody3D>() {
                body.set_visible(false);
                body.set_collision_layer(1 << 16);

                let extra = self.terrain_config.extra_collision_layer;
                if (1..=32).contains(&extra) {
                    body.set_collision_layer_value(extra, true);
                }

                // Enable backface collision on the ConcavePolygonShape3D
                let body_children = body.get_children_ex().include_internal(true).done();
                for j in 0..body_children.len() {
                    if let Some(shape_node) = body_children.get(j) {
                        if let Ok(shape_node) = shape_node.try_cast::<CollisionShape3D>() {
                            if let Some(shape) = shape_node.get_shape() {
                                if let Ok(mut concave) = shape.try_cast::<ConcavePolygonShape3D>() {
                                    concave.set_backface_collision_enabled(true);
                                }
                            }
                        }
                    }
                }
                return;
            }
        }
    }
}

fn replay_geometry(st: &mut Gd<SurfaceTool>, geo: &CellGeometry) -> bool {
    if geo.verts.len() % 3 != 0 {
        godot_warn!(
            "Skipping cell with invalid vertex count: {} (not
  divisible by 3)",
            geo.verts.len()
        );
        return false;
    }

    for i in 0..geo.verts.len() {
        let vert = geo.verts[i];
        if !vert.is_finite() || !vert.y.is_finite() {
            godot_warn!(
                "Skipping vertex with NaN/Inf: ({}, {}, {})",
                vert.x,
                vert.y,
                vert.z
            );
            return false;
        }
        let smooth_group = if geo.is_floor[i] { 0 } else { u32::MAX };

        st.set_smooth_group(smooth_group);
        st.set_uv(geo.uvs[i]);
        st.set_uv2(geo.uv2s[i]);
        st.set_color(geo.colors_0[i]);
        st.set_custom(0, geo.colors_1[i]);
        st.set_custom(1, geo.grass_mask[i]);
        st.set_custom(2, geo.material_blend[i]);
        st.add_vertex(vert);
    }
    true
}
