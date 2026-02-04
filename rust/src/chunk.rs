use std::collections::HashMap;

use godot::classes::mesh::PrimitiveType;
use godot::classes::surface_tool::CustomFormat;
use godot::classes::{ArrayMesh, Engine, IMeshInstance3D, MeshInstance3D, Noise, SurfaceTool};
use godot::prelude::*;

use crate::marching_squares::{self, CellContext, CellGeometry, MergeMode};
use crate::terrain::PixyTerrain;

/// Per-chunk mesh instance that holds heightmap data and generates geometry.
/// Port of Yugen's MarchingSquaresTerrainChunk (MeshInstance3D).
#[derive(GodotClass)]
#[class(base=MeshInstance3D, init, tool)]
pub struct PixyTerrainChunk {
    base: Base<MeshInstance3D>,

    /// Reference to the parent terrain system.
    #[export]
    pub chunk_coords: Vector2i,

    /// Merge mode index (mirrors terrain setting, stored per-chunk for serialization).
    #[export]
    #[init(val = 1)]
    pub merge_mode: i32,

    // ═══════════════════════════════════════════
    // Terrain Data Maps
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

    /// Reference to parent terrain (set by terrain when adding chunk).
    terrain_ref: Option<Gd<PixyTerrain>>,
}

impl PixyTerrainChunk {
    pub fn new_with_base(base: Base<MeshInstance3D>) -> Self {
        Self {
            base,
            chunk_coords: Vector2i::ZERO,
            merge_mode: 1,
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
            terrain_ref: None,
        }
    }
}

#[godot_api]
impl IMeshInstance3D for PixyTerrainChunk {
    fn exit_tree(&mut self) {
        // In Yugen, this saves the mesh and cleans up the chunk from the terrain's map.
        // For now, just clean up the terrain reference.
        if let Some(terrain) = self.terrain_ref.as_ref() {
            if terrain.is_instance_valid() {
                // The terrain's remove_chunk handles map cleanup
            }
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
        self.height_map[coords.y as usize][coords.x as usize]
    }

    /// Draw (set) height at a grid point and mark surrounding cells dirty.
    #[func]
    pub fn draw_height(&mut self, x: i32, z: i32, y: f32) {
        self.height_map[z as usize][x as usize] = y;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw ground color channel 0 at a grid point.
    #[func]
    pub fn draw_color_0(&mut self, x: i32, z: i32, color: Color) {
        let dim_x = self.get_dimensions_xz().0;
        self.color_map_0[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw ground color channel 1 at a grid point.
    #[func]
    pub fn draw_color_1(&mut self, x: i32, z: i32, color: Color) {
        let dim_x = self.get_dimensions_xz().0;
        self.color_map_1[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw wall color channel 0.
    #[func]
    pub fn draw_wall_color_0(&mut self, x: i32, z: i32, color: Color) {
        let dim_x = self.get_dimensions_xz().0;
        self.wall_color_map_0[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw wall color channel 1.
    #[func]
    pub fn draw_wall_color_1(&mut self, x: i32, z: i32, color: Color) {
        let dim_x = self.get_dimensions_xz().0;
        self.wall_color_map_1[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw grass mask.
    #[func]
    pub fn draw_grass_mask(&mut self, x: i32, z: i32, masked: Color) {
        let dim_x = self.get_dimensions_xz().0;
        self.grass_mask_map[(z * dim_x + x) as usize] = masked;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }
}

impl PixyTerrainChunk {
    /// Set the reference to the parent terrain node.
    pub fn set_terrain_ref(&mut self, terrain: Gd<PixyTerrain>) {
        self.terrain_ref = Some(terrain);
    }

    /// Get dimensions from the terrain parent.
    fn get_dimensions_xz(&self) -> (i32, i32) {
        if let Some(ref terrain) = self.terrain_ref {
            let t = terrain.bind();
            (t.dimensions.x, t.dimensions.z)
        } else {
            (33, 33) // fallback defaults
        }
    }

    fn get_terrain_dimensions(&self) -> Vector3i {
        if let Some(ref terrain) = self.terrain_ref {
            terrain.bind().dimensions
        } else {
            Vector3i::new(33, 32, 33)
        }
    }

    fn get_cell_size(&self) -> Vector2 {
        if let Some(ref terrain) = self.terrain_ref {
            terrain.bind().cell_size
        } else {
            Vector2::new(2.0, 2.0)
        }
    }

    fn get_blend_mode(&self) -> i32 {
        if let Some(ref terrain) = self.terrain_ref {
            terrain.bind().blend_mode
        } else {
            0
        }
    }

    fn get_use_ridge_texture(&self) -> bool {
        if let Some(ref terrain) = self.terrain_ref {
            terrain.bind().use_ridge_texture
        } else {
            false
        }
    }

    fn get_ridge_threshold(&self) -> f32 {
        if let Some(ref terrain) = self.terrain_ref {
            terrain.bind().ridge_threshold
        } else {
            1.0
        }
    }

    fn get_noise(&self) -> Option<Gd<Noise>> {
        if let Some(ref terrain) = self.terrain_ref {
            terrain.bind().noise_hmap.clone()
        } else {
            None
        }
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
    pub fn set_height_at(&mut self, x: i32, z: i32, h: f32) {
        let z_idx = z as usize;
        let x_idx = x as usize;
        if z_idx < self.height_map.len() && x_idx < self.height_map[z_idx].len() {
            self.height_map[z_idx][x_idx] = h;
        }
    }

    /// Initialize terrain data (called by terrain parent after adding chunk to tree).
    pub fn initialize_terrain(&mut self, should_regenerate_mesh: bool) {
        if !Engine::singleton().is_editor_hint() {
            godot_error!(
                "PixyTerrainChunk: Trying to initialize terrain during runtime (NOT SUPPORTED)"
            );
            return;
        }

        let dim = self.get_terrain_dimensions();

        // Initialize needs_update grid
        self.needs_update = Vec::with_capacity((dim.z - 1) as usize);
        for _ in 0..(dim.z - 1) {
            self.needs_update.push(vec![true; (dim.x - 1) as usize]);
        }

        // Generate data maps if they don't exist yet
        if self.height_map.is_empty() {
            self.generate_height_map();
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

        if should_regenerate_mesh && self.base().get_mesh().is_none() {
            self.regenerate_mesh();
        }
    }

    /// Generate a flat height map, optionally seeded by noise.
    pub fn generate_height_map(&mut self) {
        let dim = self.get_terrain_dimensions();
        let dim_x = dim.x as usize;
        let dim_z = dim.z as usize;

        self.height_map = vec![vec![0.0; dim_x]; dim_z];

        if let Some(noise) = self.get_noise() {
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

    /// Generate default ground color maps (all zeros → texture slot 0).
    pub fn generate_color_maps(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        // Default to texture slot 0: Color(1,0,0,0) for both channels
        // Actually Yugen initializes to Color(0,0,0,0) which also maps to slot 0
        // since get_dominant_color treats 0,0,0,0 as R channel (index 0).
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
    pub fn regenerate_mesh(&mut self) {
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
            // Apply terrain material if available
            if let Some(ref terrain) = self.terrain_ref {
                let t = terrain.bind();
                if let Some(ref mat) = t.terrain_material {
                    mesh.clone()
                        .cast::<ArrayMesh>()
                        .surface_set_material(0, mat);
                }
            }
            self.base_mut().set_mesh(&mesh);
        }

        // Create trimesh collision
        self.base_mut().create_trimesh_collision();

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

        for z in 0..(dim.z - 1) {
            for x in 0..(dim.x - 1) {
                let cell_coords = Vector2i::new(x, z);
                let key = [x, z];

                // If geometry didn't change, replay cached geometry
                if !self.needs_update[z as usize][x as usize] {
                    if let Some(geo) = self.cell_geometry.get(&key) {
                        replay_geometry(st, geo);
                        continue;
                    }
                }

                // Mark cell as updated
                self.needs_update[z as usize][x as usize] = false;

                // Get corner heights: A(top-left), B(top-right), C(bottom-left), D(bottom-right)
                // Note: Yugen uses heights[A,B,D,C] in its rotation array
                let ay = self.height_map[z as usize][x as usize];
                let by = self.height_map[z as usize][(x + 1) as usize];
                let cy = self.height_map[(z + 1) as usize][x as usize];
                let dy = self.height_map[(z + 1) as usize][(x + 1) as usize];

                // Build context for this cell
                let mut ctx = CellContext {
                    // Heights in rotation order: [A, B, D, C]
                    heights: [ay, by, dy, cy],
                    edges: [true; 4], // Will be computed in generate_cell
                    rotation: 0,
                    cell_coords,
                    dimensions: dim,
                    cell_size,
                    merge_threshold,
                    higher_poly_floors: self.higher_poly_floors,
                    color_map_0: self.color_map_0.clone(),
                    color_map_1: self.color_map_1.clone(),
                    wall_color_map_0: self.wall_color_map_0.clone(),
                    wall_color_map_1: self.wall_color_map_1.clone(),
                    grass_mask_map: self.grass_mask_map.clone(),
                    cell_min_height: 0.0,
                    cell_max_height: 0.0,
                    cell_is_boundary: false,
                    cell_floor_lower_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                    cell_floor_upper_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                    cell_floor_lower_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                    cell_floor_upper_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                    cell_wall_lower_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                    cell_wall_upper_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                    cell_wall_lower_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                    cell_wall_upper_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
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
                };

                let mut geo = CellGeometry::default();

                // Generate geometry for this cell
                marching_squares::generate_cell(&mut ctx, &mut geo);

                // Commit geometry to SurfaceTool
                replay_geometry(st, &geo);

                // Cache the geometry
                self.cell_geometry.insert(key, geo);
            }
        }

        if self.is_new_chunk {
            self.is_new_chunk = false;
        }
    }

    /// Mark a cell as needing update.
    pub fn notify_needs_update(&mut self, z: i32, x: i32) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if z < 0 || z >= dim_z - 1 || x < 0 || x >= dim_x - 1 {
            return;
        }
        self.needs_update[z as usize][x as usize] = true;
    }
}

/// Replay cached geometry into a SurfaceTool.
fn replay_geometry(st: &mut Gd<SurfaceTool>, geo: &CellGeometry) {
    for i in 0..geo.verts.len() {
        // Smooth group: 0 for floor, -1 for wall
        let smooth_group = if geo.is_floor[i] { 0 } else { u32::MAX };
        st.set_smooth_group(smooth_group);
        st.set_uv(geo.uvs[i]);
        st.set_uv2(geo.uv2s[i]);
        st.set_color(geo.colors_0[i]);
        st.set_custom(0, geo.colors_1[i]);
        st.set_custom(1, geo.grass_mask[i]);
        st.set_custom(2, geo.mat_blend[i]);
        st.add_vertex(geo.verts[i]);
    }
}
