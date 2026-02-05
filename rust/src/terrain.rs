use std::collections::HashMap;

use godot::classes::{Engine, Node3D, ResourceLoader, Shader, ShaderMaterial, Texture2D};
use godot::prelude::*;

use crate::chunk::{PixyTerrainChunk, TerrainConfig};
use crate::grass_planter::GrassConfig;
use crate::marching_squares::MergeMode;

/// Path to the terrain shader file.
const TERRAIN_SHADER_PATH: &str = "res://resources/shaders/mst_terrain.gdshader";

/// Texture uniform names in the shader (16 slots, index 0-15).
const TEXTURE_UNIFORM_NAMES: [&str; 16] = [
    "vc_tex_rr",
    "vc_tex_rg",
    "vc_tex_rb",
    "vc_tex_ra",
    "vc_tex_gr",
    "vc_tex_gg",
    "vc_tex_gb",
    "vc_tex_ga",
    "vc_tex_br",
    "vc_tex_bg",
    "vc_tex_bb",
    "vc_tex_ba",
    "vc_tex_ar",
    "vc_tex_ag",
    "vc_tex_ab",
    "vc_tex_aa",
];

/// Ground albedo uniform names in the shader (6 slots matching texture slots 1-6).
const GROUND_ALBEDO_NAMES: [&str; 6] = [
    "ground_albedo",
    "ground_albedo_2",
    "ground_albedo_3",
    "ground_albedo_4",
    "ground_albedo_5",
    "ground_albedo_6",
];

/// Texture scale uniform names (15 slots, indices 1-15).
const TEXTURE_SCALE_NAMES: [&str; 15] = [
    "texture_scale_1",
    "texture_scale_2",
    "texture_scale_3",
    "texture_scale_4",
    "texture_scale_5",
    "texture_scale_6",
    "texture_scale_7",
    "texture_scale_8",
    "texture_scale_9",
    "texture_scale_10",
    "texture_scale_11",
    "texture_scale_12",
    "texture_scale_13",
    "texture_scale_14",
    "texture_scale_15",
];

/// Main terrain manager node. Manages chunks, exports terrain settings, and syncs shader uniforms.
/// Port of Yugen's MarchingSquaresTerrain (Node3D).
#[derive(GodotClass)]
#[class(base=Node3D, init, tool)]
#[allow(clippy::approx_constant)]
pub struct PixyTerrain {
    base: Base<Node3D>,

    // ═══════════════════════════════════════════
    // Core Settings
    // ═══════════════════════════════════════════
    /// Total height values in X and Z direction, and total height range (Y).
    #[export]
    #[init(val = Vector3i::new(33, 32, 33))]
    pub dimensions: Vector3i,

    /// XZ unit size of each cell.
    #[export]
    #[init(val = Vector2::new(2.0, 2.0))]
    pub cell_size: Vector2,

    /// Blend mode: 0 = smooth, 1 = hard edge, 2 = hard with blend.
    #[export]
    #[init(val = 0)]
    pub blend_mode: i32,

    /// Height threshold that determines where walls begin on the terrain mesh.
    #[export]
    #[init(val = 0.0)]
    pub wall_threshold: f32,

    /// Noise used to generate initial heightmap. If None, terrain starts flat.
    #[export]
    pub noise_hmap: Option<Gd<godot::classes::Noise>>,

    /// Extra collision layer for terrain chunks (9-32).
    #[export]
    #[init(val = 9)]
    pub extra_collision_layer: i32,

    /// Ridge threshold for grass exclusion and ridge texture detection.
    #[export]
    #[init(val = 1.0)]
    pub ridge_threshold: f32,

    /// Ledge threshold for grass exclusion.
    #[export]
    #[init(val = 0.25)]
    pub ledge_threshold: f32,

    /// Whether ridge vertices use wall texture instead of ground texture.
    #[export]
    #[init(val = false)]
    pub use_ridge_texture: bool,

    /// Merge mode index: 0=Cubic, 1=Polyhedron, 2=RoundedPolyhedron, 3=SemiRound, 4=Spherical.
    #[export]
    #[init(val = 1)]
    pub merge_mode: i32,

    // ═══════════════════════════════════════════
    // Blending Settings
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = 5.0)]
    pub blend_sharpness: f32,

    #[export]
    #[init(val = 10.0)]
    pub blend_noise_scale: f32,

    #[export]
    #[init(val = 0.0)]
    pub blend_noise_strength: f32,

    // ═══════════════════════════════════════════
    // Texture Settings (15 texture slots)
    // ═══════════════════════════════════════════
    #[export]
    pub ground_texture: Option<Gd<Texture2D>>,
    #[export]
    pub texture_2: Option<Gd<Texture2D>>,
    #[export]
    pub texture_3: Option<Gd<Texture2D>>,
    #[export]
    pub texture_4: Option<Gd<Texture2D>>,
    #[export]
    pub texture_5: Option<Gd<Texture2D>>,
    #[export]
    pub texture_6: Option<Gd<Texture2D>>,
    #[export]
    pub texture_7: Option<Gd<Texture2D>>,
    #[export]
    pub texture_8: Option<Gd<Texture2D>>,
    #[export]
    pub texture_9: Option<Gd<Texture2D>>,
    #[export]
    pub texture_10: Option<Gd<Texture2D>>,
    #[export]
    pub texture_11: Option<Gd<Texture2D>>,
    #[export]
    pub texture_12: Option<Gd<Texture2D>>,
    #[export]
    pub texture_13: Option<Gd<Texture2D>>,
    #[export]
    pub texture_14: Option<Gd<Texture2D>>,
    #[export]
    pub texture_15: Option<Gd<Texture2D>>,

    // ═══════════════════════════════════════════
    // Per-Texture UV Scales
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_1: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_2: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_3: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_4: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_5: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_6: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_7: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_8: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_9: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_10: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_11: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_12: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_13: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_14: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_15: f32,

    // ═══════════════════════════════════════════
    // Ground Colors (6 slots matching texture slots 1-6)
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4706, 0.3176, 1.0))]
    pub ground_color: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3216, 0.4824, 0.3843, 1.0))]
    pub ground_color_2: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3725, 0.4235, 0.2941, 1.0))]
    pub ground_color_3: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4745, 0.2549, 1.0))]
    pub ground_color_4: Color,
    #[export]
    #[init(val = Color::from_rgba(0.2902, 0.4941, 0.3647, 1.0))]
    pub ground_color_5: Color,
    #[export]
    #[init(val = Color::from_rgba(0.4431, 0.4471, 0.3647, 1.0))]
    pub ground_color_6: Color,

    // ═══════════════════════════════════════════
    // Shading Settings
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = Color::from_rgba(0.0, 0.0, 0.0, 1.0))]
    pub shadow_color: Color,

    #[export]
    #[init(val = 5)]
    pub shadow_bands: i32,

    #[export]
    #[init(val = 0.0)]
    pub shadow_intensity: f32,

    // ═══════════════════════════════════════════
    // Grass Settings
    // ═══════════════════════════════════════════
    #[export]
    pub grass_sprite: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_2: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_3: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_4: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_5: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_6: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = 0)]
    pub animation_fps: i32,
    #[export]
    #[init(val = 3)]
    pub grass_subdivisions: i32,
    #[export]
    #[init(val = Vector2::new(1.0, 1.0))]
    pub grass_size: Vector2,

    #[export]
    #[init(val = true)]
    pub tex2_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex3_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex4_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex5_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex6_has_grass: bool,

    /// Default wall texture slot (0-15).
    #[export]
    #[init(val = 5)]
    pub default_wall_texture: i32,

    /// Optional grass mesh to use instead of QuadMesh.
    #[export]
    pub grass_mesh: Option<Gd<godot::classes::Mesh>>,

    /// Current texture preset for save/load workflow.
    #[export]
    pub current_texture_preset: Option<Gd<crate::texture_preset::PixyTexturePreset>>,

    // ═══════════════════════════════════════════
    // Internal State (not exported)
    // ═══════════════════════════════════════════
    pub terrain_material: Option<Gd<ShaderMaterial>>,

    pub is_batch_updating: bool,

    /// Map of chunk coordinates → chunk node.
    #[init(val = HashMap::new())]
    chunks: HashMap<[i32; 2], Gd<PixyTerrainChunk>>,
}

#[godot_api]
impl INode3D for PixyTerrain {
    fn enter_tree(&mut self) {
        if !Engine::singleton().is_editor_hint() {
            return;
        }

        // Deferred initialization to ensure tree is ready
        self.base_mut().call_deferred("_deferred_enter_tree", &[]);
    }
}

#[godot_api]
impl PixyTerrain {
    #[func]
    fn _deferred_enter_tree(&mut self) {
        if !Engine::singleton().is_editor_hint() {
            return;
        }

        // Create/load terrain material
        self.ensure_terrain_material();
        self.force_batch_update();

        // Discover existing chunk children
        self.chunks.clear();
        let children = self.base().get_children();
        for i in 0..children.len() {
            let Some(child): Option<Gd<Node>> = children.get(i) else {
                continue;
            };
            if let Ok(chunk) = child.try_cast::<PixyTerrainChunk>() {
                let coords = chunk.bind().chunk_coords;
                self.chunks.insert([coords.x, coords.y], chunk);
            }
        }

        // Create configs ONCE before iterating chunks (avoids borrow issues)
        let terrain_config = self.make_terrain_config();
        let grass_config = self.make_grass_config();
        let noise = self.noise_hmap.clone();
        let material = self.terrain_material.clone();

        // Initialize all discovered chunks with cached configs
        let chunk_keys: Vec<[i32; 2]> = self.chunks.keys().cloned().collect();
        for key in chunk_keys {
            if let Some(chunk) = self.chunks.get(&key) {
                let mut chunk = chunk.clone();
                {
                    let mut bind = chunk.bind_mut();
                    bind.set_terrain_config(terrain_config.clone());
                }
                chunk.bind_mut().initialize_terrain(
                    true,
                    noise.clone(),
                    material.clone(),
                    grass_config.clone(),
                );
            }
        }
    }

    /// Regenerate the entire terrain: clear all chunks, create a single chunk at (0,0).
    #[func]
    pub fn regenerate(&mut self) {
        godot_print!("PixyTerrain: regenerate()");
        self.ensure_terrain_material();
        self.force_batch_update();
        self.clear();
        self.add_new_chunk(0, 0);
    }

    /// Remove all chunks.
    #[func]
    pub fn clear(&mut self) {
        godot_print!("PixyTerrain: clear()");
        let keys: Vec<[i32; 2]> = self.chunks.keys().cloned().collect();
        for key in keys {
            self.remove_chunk(key[0], key[1]);
        }
    }

    /// Check if a chunk exists at the given coordinates.
    #[func]
    pub fn has_chunk(&self, x: i32, z: i32) -> bool {
        self.chunks.contains_key(&[x, z])
    }

    /// Create a new chunk at the given chunk coordinates, copying shared edges from neighbors.
    #[func]
    pub fn add_new_chunk(&mut self, chunk_x: i32, chunk_z: i32) {
        let chunk_coords = Vector2i::new(chunk_x, chunk_z);
        let mut new_chunk = Gd::<PixyTerrainChunk>::from_init_fn(PixyTerrainChunk::new_with_base);

        new_chunk.set_name(&format!("Chunk ({}, {})", chunk_x, chunk_z));
        {
            let mut chunk_bind = new_chunk.bind_mut();
            chunk_bind.chunk_coords = chunk_coords;
            chunk_bind.merge_mode = self.merge_mode;
        }

        // Add to tree and initialize
        self.add_chunk_internal(chunk_coords, new_chunk.clone(), false);

        // Copy shared edges from adjacent chunks
        let dim = self.dimensions;

        // Left neighbor: copy rightmost column → new chunk leftmost column
        if let Some(left) = self.chunks.get(&[chunk_x - 1, chunk_z]).cloned() {
            let left_bind = left.bind();
            let mut new_bind = new_chunk.bind_mut();
            for z in 0..dim.z {
                if let Some(h) = left_bind.get_height_at(dim.x - 1, z) {
                    new_bind.set_height_at(0, z, h);
                }
            }
        }

        // Right neighbor
        if let Some(right) = self.chunks.get(&[chunk_x + 1, chunk_z]).cloned() {
            let right_bind = right.bind();
            let mut new_bind = new_chunk.bind_mut();
            for z in 0..dim.z {
                if let Some(h) = right_bind.get_height_at(0, z) {
                    new_bind.set_height_at(dim.x - 1, z, h);
                }
            }
        }

        // Up neighbor: copy bottom row → new chunk top row
        if let Some(up) = self.chunks.get(&[chunk_x, chunk_z - 1]).cloned() {
            let up_bind = up.bind();
            let mut new_bind = new_chunk.bind_mut();
            for x in 0..dim.x {
                if let Some(h) = up_bind.get_height_at(x, dim.z - 1) {
                    new_bind.set_height_at(x, 0, h);
                }
            }
        }

        // Down neighbor
        if let Some(down) = self.chunks.get(&[chunk_x, chunk_z + 1]).cloned() {
            let down_bind = down.bind();
            let mut new_bind = new_chunk.bind_mut();
            for x in 0..dim.x {
                if let Some(h) = down_bind.get_height_at(x, 0) {
                    new_bind.set_height_at(x, dim.z - 1, h);
                }
            }
        }

        // Generate mesh
        new_chunk.bind_mut().regenerate_mesh();
    }

    /// Remove a chunk and free it.
    #[func]
    pub fn remove_chunk(&mut self, x: i32, z: i32) {
        if let Some(mut chunk) = self.chunks.remove(&[x, z]) {
            chunk.queue_free();
        }
    }

    /// Remove a chunk from the tree without freeing it (for undo/redo).
    #[func]
    pub fn remove_chunk_from_tree(&mut self, x: i32, z: i32) {
        if let Some(mut chunk) = self.chunks.remove(&[x, z]) {
            self.base_mut().remove_child(&chunk);
            chunk.set_owner(Gd::null_arg());
        }
    }

    /// Get a chunk handle by coordinates (returns None if chunk doesn't exist).
    #[func]
    pub fn get_chunk(&self, x: i32, z: i32) -> Option<Gd<PixyTerrainChunk>> {
        self.chunks.get(&[x, z]).cloned()
    }

    /// Get all chunk coordinate keys as a PackedVector2Array.
    #[func]
    pub fn get_chunk_keys(&self) -> PackedVector2Array {
        let mut arr = PackedVector2Array::new();
        for key in self.chunks.keys() {
            arr.push(Vector2::new(key[0] as f32, key[1] as f32));
        }
        arr
    }

    /// Get the merge threshold for the current merge mode.
    #[func]
    pub fn get_merge_threshold(&self) -> f32 {
        MergeMode::from_index(self.merge_mode).threshold()
    }

    /// Sync all shader parameters from terrain exports to the terrain material.
    #[func]
    pub fn force_batch_update(&mut self) {
        if self.terrain_material.is_none() {
            return;
        }

        self.is_batch_updating = true;

        // Collect all values before borrowing material
        let dimensions = self.dimensions;
        let cell_size = self.cell_size;
        let wall_threshold = self.wall_threshold;
        let use_hard = self.blend_mode != 0;
        let blend_mode = self.blend_mode;
        let blend_sharpness = self.blend_sharpness;
        let blend_noise_scale = self.blend_noise_scale;
        let blend_noise_strength = self.blend_noise_strength;
        let ground_colors = self.get_ground_colors();
        let scales = self.get_texture_scales();
        let textures = self.get_texture_slots();
        let shadow_color = self.shadow_color;
        let shadow_bands = self.shadow_bands;
        let shadow_intensity = self.shadow_intensity;

        let mat = self.terrain_material.as_mut().unwrap();

        // Core geometry params
        mat.set_shader_parameter("chunk_size", &dimensions.to_variant());
        mat.set_shader_parameter("cell_size", &cell_size.to_variant());
        mat.set_shader_parameter("wall_threshold", &wall_threshold.to_variant());

        // Blend settings
        mat.set_shader_parameter("use_hard_textures", &use_hard.to_variant());
        mat.set_shader_parameter("blend_mode", &blend_mode.to_variant());
        mat.set_shader_parameter("blend_sharpness", &blend_sharpness.to_variant());
        mat.set_shader_parameter("blend_noise_scale", &blend_noise_scale.to_variant());
        mat.set_shader_parameter("blend_noise_strength", &blend_noise_strength.to_variant());

        // Ground colors
        for (i, name) in GROUND_ALBEDO_NAMES.iter().enumerate() {
            mat.set_shader_parameter(*name, &ground_colors[i].to_variant());
        }

        // Texture scales
        for (i, name) in TEXTURE_SCALE_NAMES.iter().enumerate() {
            mat.set_shader_parameter(*name, &scales[i].to_variant());
        }

        // Textures (16 slots)
        for (i, name) in TEXTURE_UNIFORM_NAMES.iter().enumerate() {
            if let Some(ref tex) = textures[i] {
                mat.set_shader_parameter(*name, &tex.to_variant());
            }
        }

        // Shading
        mat.set_shader_parameter("shadow_color", &shadow_color.to_variant());
        mat.set_shader_parameter("bands", &shadow_bands.to_variant());
        mat.set_shader_parameter("shadow_intensity", &shadow_intensity.to_variant());

        self.is_batch_updating = false;
    }

    /// Save current terrain texture settings to the current_texture_preset.
    #[func]
    pub fn save_to_preset(&mut self) {
        // Get terrain Gd before borrowing preset
        let terrain_gd = self.to_gd();

        if self.current_texture_preset.is_none() {
            // Create a new preset if none exists
            let preset = Gd::<crate::texture_preset::PixyTexturePreset>::default();
            self.current_texture_preset = Some(preset);
        }

        if let Some(ref mut preset) = self.current_texture_preset {
            preset.bind_mut().save_from_terrain(terrain_gd);
            godot_print!("PixyTerrain: Saved texture settings to preset");
        }
    }

    /// Load texture settings from the current_texture_preset.
    #[func]
    pub fn load_from_preset(&mut self) {
        if let Some(ref preset) = self.current_texture_preset {
            let terrain_gd = self.to_gd();
            preset.bind().load_into_terrain(terrain_gd);
            godot_print!("PixyTerrain: Loaded texture settings from preset");
        } else {
            godot_warn!("PixyTerrain: No preset assigned to load from");
        }
    }

    /// Ensure all texture slots have sensible defaults.
    /// Called on initialization to provide defaults for new projects.
    #[func]
    pub fn ensure_textures(&mut self) {
        // This method exists to match GDScript's _ensure_textures().
        // In Rust, exports already have default values, so we just ensure
        // the shader material is synced.
        self.ensure_terrain_material();
        self.force_batch_update();
    }

    /// Regenerate grass on all chunks (call after changing grass-related settings).
    #[func]
    pub fn regenerate_all_grass(&mut self) {
        let chunk_keys: Vec<[i32; 2]> = self.chunks.keys().cloned().collect();
        for key in chunk_keys {
            if let Some(chunk) = self.chunks.get(&key) {
                let mut chunk = chunk.clone();
                chunk.bind_mut().regenerate_mesh();
            }
        }
    }
}

impl PixyTerrain {
    /// Ensure terrain material exists, creating it from shader if needed.
    pub fn ensure_terrain_material(&mut self) {
        if self.terrain_material.is_some() {
            return;
        }

        let mut loader = ResourceLoader::singleton();

        // Try loading the shader directly
        if loader.exists(TERRAIN_SHADER_PATH) {
            let resource = loader.load(TERRAIN_SHADER_PATH);
            if let Some(res) = resource {
                if let Ok(shader) = res.try_cast::<Shader>() {
                    let mut mat = ShaderMaterial::new_gd();
                    mat.set_shader(&shader);
                    mat.set_render_priority(-1);
                    self.terrain_material = Some(mat);
                    godot_print!("PixyTerrain: Created terrain material from shader");
                    return;
                }
            }
        }

        godot_warn!("PixyTerrain: Could not load terrain shader at {TERRAIN_SHADER_PATH}");
    }

    /// Collect ground colors into an array for batch update.
    fn get_ground_colors(&self) -> [Color; 6] {
        [
            self.ground_color,
            self.ground_color_2,
            self.ground_color_3,
            self.ground_color_4,
            self.ground_color_5,
            self.ground_color_6,
        ]
    }

    /// Collect texture scales into an array for batch update.
    fn get_texture_scales(&self) -> [f32; 15] {
        [
            self.texture_scale_1,
            self.texture_scale_2,
            self.texture_scale_3,
            self.texture_scale_4,
            self.texture_scale_5,
            self.texture_scale_6,
            self.texture_scale_7,
            self.texture_scale_8,
            self.texture_scale_9,
            self.texture_scale_10,
            self.texture_scale_11,
            self.texture_scale_12,
            self.texture_scale_13,
            self.texture_scale_14,
            self.texture_scale_15,
        ]
    }

    /// Collect all 16 texture slots (index 0 = ground_texture, 1-14 = texture_2..15, 15 = void/None).
    fn get_texture_slots(&self) -> [Option<Gd<Texture2D>>; 16] {
        [
            self.ground_texture.clone(),
            self.texture_2.clone(),
            self.texture_3.clone(),
            self.texture_4.clone(),
            self.texture_5.clone(),
            self.texture_6.clone(),
            self.texture_7.clone(),
            self.texture_8.clone(),
            self.texture_9.clone(),
            self.texture_10.clone(),
            self.texture_11.clone(),
            self.texture_12.clone(),
            self.texture_13.clone(),
            self.texture_14.clone(),
            self.texture_15.clone(),
            None, // Slot 15: void texture (transparent)
        ]
    }

    /// Set a single shader parameter if the material exists.
    #[allow(dead_code)]
    pub fn set_shader_param(&mut self, name: &str, value: &Variant) {
        if let Some(ref mut mat) = self.terrain_material {
            mat.set_shader_parameter(name, value);
        }
    }

    /// Internal: add a chunk to the tree and register it.
    fn add_chunk_internal(
        &mut self,
        coords: Vector2i,
        mut chunk: Gd<PixyTerrainChunk>,
        regenerate: bool,
    ) {
        // Create configs BEFORE adding chunk (avoids borrow issues - terrain is borrowed here, but that's ok)
        let terrain_config = self.make_terrain_config();
        let grass_config = self.make_grass_config();
        let noise = self.noise_hmap.clone();
        let material = self.terrain_material.clone();

        self.chunks.insert([coords.x, coords.y], chunk.clone());

        {
            let mut chunk_bind = chunk.bind_mut();
            chunk_bind.chunk_coords = coords;
            // Pass config directly - no terrain binding needed later
            chunk_bind.set_terrain_config(terrain_config);
        }

        self.base_mut().add_child(&chunk);

        // Position the chunk in world space
        let dim = self.dimensions;
        let cell = self.cell_size;
        let pos = Vector3::new(
            coords.x as f32 * ((dim.x - 1) as f32 * cell.x),
            0.0,
            coords.y as f32 * ((dim.z - 1) as f32 * cell.y),
        );
        chunk.set_position(pos);

        // Set owner for editor persistence
        if Engine::singleton().is_editor_hint() {
            if let Some(mut editor) = Engine::singleton().get_singleton("EditorInterface") {
                let scene_root = editor.call("get_edited_scene_root", &[]);
                if let Ok(root) = scene_root.try_to::<Gd<Node>>() {
                    Self::set_owner_recursive(&mut chunk.clone().upcast::<Node>(), &root);
                }
            }
        }

        // Initialize terrain with all needed data passed as parameters
        chunk
            .bind_mut()
            .initialize_terrain(regenerate, noise, material, grass_config);

        godot_print!("PixyTerrain: Added chunk at ({}, {})", coords.x, coords.y);
    }

    fn set_owner_recursive(node: &mut Gd<Node>, owner: &Gd<Node>) {
        node.set_owner(owner);
        let children = node.get_children();
        for i in 0..children.len() {
            let Some(mut child): Option<Gd<Node>> = children.get(i) else {
                continue;
            };
            Self::set_owner_recursive(&mut child, owner);
        }
    }

    /// Create a TerrainConfig from current terrain settings.
    /// This is called before chunk operations to avoid needing to bind terrain later.
    fn make_terrain_config(&self) -> TerrainConfig {
        TerrainConfig {
            dimensions: self.dimensions,
            cell_size: self.cell_size,
            blend_mode: self.blend_mode,
            use_ridge_texture: self.use_ridge_texture,
            ridge_threshold: self.ridge_threshold,
            extra_collision_layer: self.extra_collision_layer,
        }
    }

    /// Create a GrassConfig from current terrain settings.
    /// This is called before chunk operations to avoid needing to bind terrain later.
    fn make_grass_config(&self) -> GrassConfig {
        GrassConfig {
            dimensions: self.dimensions,
            subdivisions: self.grass_subdivisions,
            grass_size: self.grass_size,
            cell_size: self.cell_size,
            wall_threshold: self.wall_threshold,
            merge_mode: self.merge_mode,
            animation_fps: self.animation_fps,
            ledge_threshold: self.ledge_threshold,
            ridge_threshold: self.ridge_threshold,
            grass_sprites: [
                self.get_grass_sprite_or_default(0),
                self.get_grass_sprite_or_default(1),
                self.get_grass_sprite_or_default(2),
                self.get_grass_sprite_or_default(3),
                self.get_grass_sprite_or_default(4),
                self.get_grass_sprite_or_default(5),
            ],
            ground_colors: [
                self.ground_color,
                self.ground_color_2,
                self.ground_color_3,
                self.ground_color_4,
                self.ground_color_5,
                self.ground_color_6,
            ],
            tex_has_grass: [
                self.tex2_has_grass,
                self.tex3_has_grass,
                self.tex4_has_grass,
                self.tex5_has_grass,
                self.tex6_has_grass,
            ],
            grass_mesh: self.grass_mesh.clone(),
        }
    }

    /// Get grass sprite for a texture slot, falling back to default for base grass (slot 0).
    fn get_grass_sprite_or_default(&self, index: usize) -> Option<Gd<Texture2D>> {
        let sprite = match index {
            0 => &self.grass_sprite,
            1 => &self.grass_sprite_tex_2,
            2 => &self.grass_sprite_tex_3,
            3 => &self.grass_sprite_tex_4,
            4 => &self.grass_sprite_tex_5,
            5 => &self.grass_sprite_tex_6,
            _ => return None,
        };

        if sprite.is_some() {
            return sprite.clone();
        }

        // Load default grass sprite for all slots if no custom texture assigned
        let mut loader = ResourceLoader::singleton();
        let path = "res://resources/textures/grass_leaf_sprite.png";
        if loader.exists(path) {
            return loader
                .load(path)
                .and_then(|r| r.try_cast::<Texture2D>().ok());
        }

        None
    }
}
