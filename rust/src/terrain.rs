use std::collections::HashMap;

use godot::classes::{
    Engine, Image, Node3D, QuadMesh, ResourceLoader, Shader, ShaderMaterial, Texture2D,
};
use godot::prelude::*;

use crate::chunk::{PixyTerrainChunk, TerrainConfig};
use crate::grass_planter::GrassConfig;
use crate::marching_squares::{BlendMode, MergeMode};

/// Path to the terrain shader file.
const TERRAIN_SHADER_PATH: &str = "res://resources/shaders/mst_terrain.gdshader";

/// Path to the default ground noise texture.
const DEFAULT_GROUND_TEXTURE_PATH: &str = "res://resources/textures/default_ground_noise.tres";

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

#[derive(GodotClass)]
#[class(base=Node3D, init, tool)]
#[allow(clippy::approx_constant)]
pub struct PixyTerrain {
    base: Base<Node3D>,

    // ═══════════════════════════════════════════
    // Core Settings
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = Vector3i::new(33, 32, 33))]
    pub dimensions: Vector3i,

    #[export]
    #[init(val = Vector2::new(2.0, 2.0))]
    pub cell_size: Vector2,

    #[export]
    #[init(val = 0)]
    pub blend_mode: i32,

    #[export]
    #[init(val = 0.0)]
    pub wall_threshold: f32,

    #[export]
    pub noise_hmap: Option<Gd<godot::classes::Noise>>,

    #[export]
    #[init(val = 9)]
    pub extra_collision_layer: i32,

    #[export]
    #[init(val = 1.0)]
    pub ridge_threshold: f32,

    #[export]
    #[init(val = 0.25)]
    pub ledge_threshold: f32,

    #[export]
    #[init(val = false)]
    pub use_ridge_texture: bool,

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

    #[export]
    #[init(val = Vector2::new(1.0, 1.0))]
    pub wind_direction: Vector2,

    #[export]
    #[init(val = 0.02)]
    pub wind_scale: f32,

    #[export]
    #[init(val = 0.14)]
    pub wind_speed: f32,

    #[export]
    #[init(val = 5)]
    pub default_wall_texture: i32,

    #[export]
    pub grass_mesh: Option<Gd<godot::classes::Mesh>>,

    #[export]
    pub current_texture_preset: Option<Gd<crate::texture_preset::PixyTexturePreset>>,

    // ═══════════════════════════════════════════
    // Internal State (not exported)
    // ═══════════════════════════════════════════
    pub terrain_material: Option<Gd<ShaderMaterial>>,
    pub grass_material: Option<Gd<ShaderMaterial>>,
    pub grass_quad_mesh: Option<Gd<QuadMesh>>,
    pub is_batch_updating: bool,

    #[init(val = HashMap::new())]
    chunks: HashMap<[i32; 2], Gd<PixyTerrainChunk>>,
}

#[godot_api]
impl INode3D for PixyTerrain {
    fn enter_tree(&mut self) {
        if !Engine::singleton().is_editor_hint() {
            return;
        }

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

        // Create/load terrain material and shared grass material
        self.ensure_terrain_material();
        self.ensure_grass_material();
        self.force_batch_update();
        self.force_grass_material_update();

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

        // Create configs ONCE before iteraing chunks (Avoid borrow issues)
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

    /// Ensure terrain material exists, creating it from shader if needed.
    pub fn ensure_terrain_material(&mut self) {
        if self.terrain_material.is_some() {
            return;
        }

        let mut loader = ResourceLoader::singleton();

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

    /// Ensure shared grass material and QuadMesh exist.
    pub fn ensure_grass_material(&mut self) {
        if self.grass_material.is_some() {
            return;
        }

        let mut loader = ResourceLoader::singleton();

        let shader_path = "res://resources/shaders/mst_grass.gdshader";
        if !loader.exists(shader_path) {
            godot_warn!("PixyTerrain: Grass shader not found at {}", shader_path);
            return;
        }

        let Some(res) = loader.load(shader_path) else {
            godot_warn!("PixyTerrain: Failed to load grass shader");
            return;
        };

        let Ok(shader) = res.try_cast::<Shader>() else {
            godot_warn!("PixyTerrain: Resource is not a Shader");
            return;
        };

        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);
        self.grass_material = Some(mat.clone());

        // Create shared QuadMesh with the material applied
        let mut quad = QuadMesh::new_gd();
        quad.set_size(self.grass_size);
        quad.set_center_offset(Vector3::new(0.0, self.grass_size.y / 2.0, 0.0));
        quad.set_material(&mat);
        quad.surface_set_material(0, &mat);
        self.grass_quad_mesh = Some(quad);

        godot_print!("PixyTerrain: Created shared grass material and mesh");
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

    /// Sync all grass shader parameters from terrain fields to the shared grass material.
    pub fn force_grass_material_update(&mut self) {
        if self.grass_material.is_none() {
            return;
        }

        // Collect ALL values BEFORE borrowing grass_material mutably
        let is_merge_round = matches!(
            MergeMode::from_index(self.merge_mode),
            MergeMode::RoundedPolyhedron | MergeMode::SemiRound | MergeMode::Spherical
        );
        let wall_threshold = self.wall_threshold;
        let animation_fps = self.animation_fps as f32;

        let sprites = [
            self.get_grass_sprite_or_default(0),
            self.get_grass_sprite_or_default(1),
            self.get_grass_sprite_or_default(2),
            self.get_grass_sprite_or_default(3),
            self.get_grass_sprite_or_default(4),
            self.get_grass_sprite_or_default(5),
        ];

        let ground_colors = [
            self.ground_color,
            self.ground_color_2,
            self.ground_color_3,
            self.ground_color_4,
            self.ground_color_5,
            self.ground_color_6,
        ];

        let use_base_color = [
            self.ground_texture.is_none(),
            self.texture_2.is_none(),
            self.texture_3.is_none(),
            self.texture_4.is_none(),
            self.texture_5.is_none(),
            self.texture_6.is_none(),
        ];

        let tex_has_grass = [
            self.tex2_has_grass,
            self.tex3_has_grass,
            self.tex4_has_grass,
            self.tex5_has_grass,
            self.tex6_has_grass,
        ];

        let shadow_color = self.shadow_color;
        let shadow_bands = self.shadow_bands;
        let shadow_intensity = self.shadow_intensity;
        let grass_size = self.grass_size;
        let wind_direction = self.wind_direction;
        let wind_scale = self.wind_scale;
        let wind_speed = self.wind_speed;

        let mat = self.grass_material.as_mut().unwrap();

        mat.set_shader_parameter("is_merge_round", &is_merge_round.to_variant());
        mat.set_shader_parameter("wall_threshold", &wall_threshold.to_variant());
        mat.set_shader_parameter("fps", &animation_fps.to_variant());

        let texture_names = [
            "grass_texture",
            "grass_texture_2",
            "grass_texture_3",
            "grass_texture_4",
            "grass_texture_5",
            "grass_texture_6",
        ];
        for (i, name) in texture_names.iter().enumerate() {
            if let Some(ref tex) = sprites[i] {
                mat.set_shader_parameter(*name, &tex.to_variant());
            }
        }

        mat.set_shader_parameter("grass_base_color", &ground_colors[0].to_variant());
        let color_names = [
            "grass_color_2",
            "grass_color_3",
            "grass_color_4",
            "grass_color_5",
            "grass_color_6",
        ];
        for (i, name) in color_names.iter().enumerate() {
            mat.set_shader_parameter(*name, &ground_colors[i + 1].to_variant());
        }

        mat.set_shader_parameter("use_base_color", &use_base_color[0].to_variant());
        mat.set_shader_parameter("use_base_color_2", &use_base_color[1].to_variant());
        mat.set_shader_parameter("use_base_color_3", &use_base_color[2].to_variant());
        mat.set_shader_parameter("use_base_color_4", &use_base_color[3].to_variant());
        mat.set_shader_parameter("use_base_color_5", &use_base_color[4].to_variant());
        mat.set_shader_parameter("use_base_color_6", &use_base_color[5].to_variant());

        mat.set_shader_parameter("use_grass_tex_2", &tex_has_grass[0].to_variant());
        mat.set_shader_parameter("use_grass_tex_3", &tex_has_grass[1].to_variant());
        mat.set_shader_parameter("use_grass_tex_4", &tex_has_grass[2].to_variant());
        mat.set_shader_parameter("use_grass_tex_5", &tex_has_grass[3].to_variant());
        mat.set_shader_parameter("use_grass_tex_6", &tex_has_grass[4].to_variant());

        mat.set_shader_parameter("wind_direction", &wind_direction.to_variant());
        mat.set_shader_parameter("wind_scale", &wind_scale.to_variant());
        mat.set_shader_parameter("wind_speed", &wind_speed.to_variant());
        mat.set_shader_parameter("animate_active", &true.to_variant());

        let mut loader = ResourceLoader::singleton();
        let wind_path = "res://resources/textures/wind_noise_texture.tres";
        if loader.exists(wind_path) {
            if let Some(wind_tex) = loader.load(wind_path) {
                mat.set_shader_parameter("wind_texture", &wind_tex.to_variant());
            }
        }

        mat.set_shader_parameter("shadow_color", &shadow_color.to_variant());
        mat.set_shader_parameter("bands", &shadow_bands.to_variant());
        mat.set_shader_parameter("shadow_intensity", &shadow_intensity.to_variant());

        if let Some(ref mut quad) = self.grass_quad_mesh {
            quad.set_size(grass_size);
            quad.set_center_offset(Vector3::new(0.0, grass_size.y / 2.0, 0.0));
        }
    }

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

    fn get_texture_slots(&self) -> [Option<Gd<Texture2D>>; 16] {
        [
            self.get_ground_texture_or_default(0),
            self.get_ground_texture_or_default(1),
            self.get_ground_texture_or_default(2),
            self.get_ground_texture_or_default(3),
            self.get_ground_texture_or_default(4),
            self.get_ground_texture_or_default(5),
            self.get_ground_texture_or_default(6),
            self.get_ground_texture_or_default(7),
            self.get_ground_texture_or_default(8),
            self.get_ground_texture_or_default(9),
            self.get_ground_texture_or_default(10),
            self.get_ground_texture_or_default(11),
            self.get_ground_texture_or_default(12),
            self.get_ground_texture_or_default(13),
            self.get_ground_texture_or_default(14),
            None, // Slot 15: void texture (transparent)
        ]
    }

    fn get_ground_texture_or_default(&self, index: usize) -> Option<Gd<Texture2D>> {
        let texture = match index {
            0 => &self.ground_texture,
            1 => &self.texture_2,
            2 => &self.texture_3,
            3 => &self.texture_4,
            4 => &self.texture_5,
            5 => &self.texture_6,
            6 => &self.texture_7,
            7 => &self.texture_8,
            8 => &self.texture_9,
            9 => &self.texture_10,
            10 => &self.texture_11,
            11 => &self.texture_12,
            12 => &self.texture_13,
            13 => &self.texture_14,
            14 => &self.texture_15,
            _ => return None,
        };

        if texture.is_some() {
            return texture.clone();
        }

        // Load default ground noise texture
        let mut loader = ResourceLoader::singleton();
        if loader.exists(DEFAULT_GROUND_TEXTURE_PATH) {
            let result = loader
                .load(DEFAULT_GROUND_TEXTURE_PATH)
                .and_then(|r| r.try_cast::<Texture2D>().ok());
            if result.is_none() {
                godot_warn!(
                    "Failed to cast default ground texture at {}",
                    DEFAULT_GROUND_TEXTURE_PATH
                );
            }
            return result;
        } else {
            godot_warn!(
                "Default ground texture not found at {}",
                DEFAULT_GROUND_TEXTURE_PATH
            );
        }

        None
    }

    #[allow(dead_code)]
    pub fn set_shader_param(&mut self, name: &str, value: &Variant) {
        if let Some(ref mut mat) = self.terrain_material {
            mat.set_shader_parameter(name, value);
        }
    }

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

        let mut loader = ResourceLoader::singleton();
        let path = "res://resources/textures/grass_leaf_sprite.png";
        if loader.exists(path) {
            return loader
                .load(path)
                .and_then(|r| r.try_cast::<Texture2D>().ok());
        }

        None
    }

    fn extract_ground_image(&self, index: usize) -> Option<Gd<Image>> {
        let tex = self.get_ground_texture_or_default(index)?;
        let mut img = tex.get_image()?;
        img.decompress();
        Some(img)
    }

    fn make_terrain_config(&self) -> TerrainConfig {
        TerrainConfig {
            dimensions: self.dimensions,
            cell_size: self.cell_size,
            blend_mode: if self.blend_mode == 0 {
                BlendMode::Interpolated
            } else {
                BlendMode::Direct
            },
            use_ridge_texture: self.use_ridge_texture,
            ridge_threshold: self.ridge_threshold,
            extra_collision_layer: self.extra_collision_layer,
        }
    }

    fn make_grass_config(&self) -> GrassConfig {
        GrassConfig {
            dimensions: self.dimensions,
            subdivisions: self.grass_subdivisions,
            grass_size: self.grass_size,
            cell_size: self.cell_size,
            wall_threshold: self.wall_threshold,
            merge_mode: MergeMode::from_index(self.merge_mode),
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
            grass_material: self.grass_material.clone(),
            grass_quad_mesh: self
                .grass_quad_mesh
                .as_ref()
                .map(|q| q.clone().upcast::<godot::classes::Mesh>()),
            ground_images: [
                self.extract_ground_image(0),
                self.extract_ground_image(1),
                self.extract_ground_image(2),
                self.extract_ground_image(3),
                self.extract_ground_image(4),
                self.extract_ground_image(5),
            ],
            texture_scales: [
                self.texture_scale_1,
                self.texture_scale_2,
                self.texture_scale_3,
                self.texture_scale_4,
                self.texture_scale_5,
                self.texture_scale_6,
            ],
        }
    }
}
