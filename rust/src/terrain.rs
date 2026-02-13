// Pixy Terrain — Terrain manager
//
// Original architecture ported from Yugen's marching_squares_terrain.gd:
//   https://github.com/Yukitty/Yugens-Terrain-Authoring-Toolkit
//
// Grass/cloud shader integration adapted from Dylearn's 3D Pixel Art Grass Demo:
//   https://github.com/DylearnDev/Dylearn-3D-Pixel-Art-Grass-Demo

use std::collections::HashMap;

use godot::classes::{
    rendering_server::GlobalShaderParameterType, Engine, Image, ImageTexture, Node3D, QuadMesh,
    RenderingServer, ResourceLoader, Shader, ShaderMaterial, Texture2D,
};
use godot::prelude::*;

use crate::chunk::{PixyTerrainChunk, TerrainConfig};
use crate::grass_planter::GrassConfig;
use crate::marching_squares::{BlendMode, MergeMode};
use crate::shared_params::SharedTerrainParams;

/// Path to the terrain shader file.
const TERRAIN_SHADER_PATH: &str =
    "res://addons/pixy_terrain/resources/shaders/mst_terrain.gdshader";

/// Path to the default ground noise texture.
const DEFAULT_GROUND_TEXTURE_PATH: &str =
    "res://addons/pixy_terrain/resources/textures/default_ground_noise.tres";

#[derive(GodotClass)]
#[class(base=Node3D, init, tool)]
#[allow(clippy::approx_constant)]
pub struct PixyTerrain {
    base: Base<Node3D>,

    // ═══════════════════════════════════════════
    // Core Settings
    // ═══════════════════════════════════════════
    #[export_group(name = "Core")]
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
    #[export_group(name = "Blending")]
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
    // Texture Settings (array exports)
    // ═══════════════════════════════════════════
    #[export_group(name = "Textures")]
    #[export]
    pub textures: VarArray,

    #[export]
    pub texture_scales: PackedFloat32Array,

    #[export]
    pub ground_colors: PackedColorArray,

    // ═══════════════════════════════════════════
    // Shading Settings
    // ═══════════════════════════════════════════
    #[export_group(name = "Shading")]
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
    #[export_group(name = "Grass")]
    #[export]
    pub grass_sprites: VarArray,

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
    #[init(val = 5)]
    pub default_wall_texture: i32,

    #[export]
    pub grass_mesh: Option<Gd<godot::classes::Mesh>>,

    #[export]
    pub current_texture_preset: Option<Gd<crate::texture_preset::PixyTexturePreset>>,

    // ═══════════════════════════════════════════
    // Grass Animation (Dylearn-based)
    // ═══════════════════════════════════════════
    #[export_subgroup(name = "Animation")]
    #[export]
    #[init(val = 5.0)]
    pub grass_framerate: f32,

    #[export]
    #[init(val = true)]
    pub grass_quantised: bool,

    #[export]
    #[init(val = true)]
    pub world_space_sway: bool,

    #[export]
    #[init(val = 60.0)]
    pub world_sway_angle: f32,

    #[export]
    #[init(val = 0.3)]
    pub fake_perspective_scale: f32,

    #[export_subgroup(name = "View Sway")]
    #[export]
    #[init(val = true)]
    pub view_space_sway: bool,

    #[export]
    #[init(val = 0.1)]
    pub view_sway_speed: f32,

    #[export]
    #[init(val = 10.0)]
    pub view_sway_angle: f32,

    #[export_subgroup(name = "Displacement")]
    #[export]
    #[init(val = true)]
    pub character_displacement_enabled: bool,

    #[export]
    #[init(val = 45.0)]
    pub player_displacement_angle_z: f32,

    #[export]
    #[init(val = 45.0)]
    pub player_displacement_angle_x: f32,

    #[export]
    #[init(val = 1.0)]
    pub radius_exponent: f32,

    #[export]
    #[init(val = 0.7)]
    pub displacement_radius: f32,

    #[export]
    #[init(val = GString::from("pixy_characters"))]
    pub character_group_name: GString,

    // ═══════════════════════════════════════════
    // Cross-Section (Y-clip)
    // ═══════════════════════════════════════════
    #[export_group(name = "Cross-Section")]
    #[export]
    #[init(val = false)]
    pub cross_section_enabled: bool,

    #[export]
    #[init(val = 3.0)]
    pub cross_section_y_offset: f32,

    // ═══════════════════════════════════════════
    // Grass Toon Lighting (Dylearn-based)
    // ═══════════════════════════════════════════
    #[export_group(name = "Toon Lighting")]
    #[export]
    #[init(val = 3)]
    pub grass_toon_cuts: i32,

    #[export]
    #[init(val = 0.0)]
    pub grass_toon_wrap: f32,

    #[export]
    #[init(val = 1.0)]
    pub grass_toon_steepness: f32,

    #[export]
    #[init(val = 0.2)]
    pub grass_threshold_gradient_size: f32,

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
        self.base_mut().call_deferred("_deferred_enter_tree", &[]);
    }

    fn process(&mut self, _delta: f64) {
        // Character tracking: collect positions from group, push to grass material
        if self.character_displacement_enabled {
            if let Some(mut tree) = self.base().get_tree() {
                let group_name = StringName::from(&self.character_group_name);
                let nodes = tree.get_nodes_in_group(&group_name);
                let mut positions: Vec<Vector4> = Vec::with_capacity(64);
                for i in 0..nodes.len().min(64) {
                    if let Some(node) = nodes.get(i) {
                        if let Ok(node3d) = node.try_cast::<Node3D>() {
                            let pos = node3d.get_global_position();
                            // w = displacement radius
                            positions.push(Vector4::new(
                                pos.x,
                                pos.y,
                                pos.z,
                                self.displacement_radius,
                            ));
                        }
                    }
                }
                // Pad to 64 entries
                while positions.len() < 64 {
                    positions.push(Vector4::new(0.0, 0.0, 0.0, 0.001));
                }
                // Set on grass material
                if let Some(ref mut mat) = self.grass_material {
                    let arr = VarArray::from_iter(positions.iter().map(|v| v.to_variant()));
                    mat.set_shader_parameter("character_positions", &arr.to_variant());
                }
            }
        }

        // Cross-section: clip terrain above the player from the camera's perspective
        if self.cross_section_enabled {
            if let Some(mut tree) = self.base().get_tree() {
                let group_name = StringName::from(&self.character_group_name);
                let nodes = tree.get_nodes_in_group(&group_name);
                if let Some(node) = nodes.get(0) {
                    if let Ok(node3d) = node.try_cast::<Node3D>() {
                        let player_pos = node3d.get_global_position();
                        let clip_origin = Vector3::new(
                            player_pos.x,
                            player_pos.y + self.cross_section_y_offset,
                            player_pos.z,
                        );
                        if let Some(ref mut mat) = self.terrain_material {
                            mat.set_shader_parameter("cross_section_enabled", &true.to_variant());
                            mat.set_shader_parameter("clip_origin", &clip_origin.to_variant());
                        }
                        if let Some(ref mut mat) = self.grass_material {
                            mat.set_shader_parameter("cross_section_enabled", &true.to_variant());
                            mat.set_shader_parameter("clip_origin", &clip_origin.to_variant());
                        }
                    }
                }
            }
        }
    }
}

#[godot_api]
impl PixyTerrain {
    #[func]
    fn _deferred_enter_tree(&mut self) {
        // Register fallback global shader parameters (no-ops if already present)
        Self::ensure_environment_globals();

        // Pad arrays to correct lengths on scene load
        self.ensure_array_sizes();

        // Create/load terrain material and shared grass material
        self.ensure_materials_and_sync();

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

    /// Register fallback global shader parameters for wind/cloud systems.
    ///
    /// Checks whether globals are already registered (e.g. from project.godot
    /// `[shader_globals]`) and skips registration to avoid duplicate-add errors
    /// in Godot 4.6+. Zero-effect defaults ensure grass renders without
    /// wind/cloud artifacts when pixy_environment is not installed.
    fn ensure_environment_globals() {
        let mut rs = RenderingServer::singleton();

        // Skip if globals already registered (e.g. from project.godot [shader_globals])
        let existing = rs.global_shader_parameter_get_list();
        if existing.contains(&StringName::from("cloud_noise")) {
            godot_print!("PixyTerrain: Registered fallback environment globals");
            return;
        }

        // Create a 1×1 white fallback texture for sampler2D defaults.
        // Guarantees the sampler returns 1.0 (white) on all GPU drivers,
        // which the existing threshold math converts to zero wind / full brightness.
        let mut img = Image::create(1, 1, false, godot::classes::image::Format::RGBA8).unwrap();
        img.set_pixel(0, 0, Color::WHITE);
        let tex = ImageTexture::create_from_image(&img).unwrap();

        // Cloud system defaults (zero-effect: no shadow visible)
        rs.global_shader_parameter_add(
            "cloud_noise",
            GlobalShaderParameterType::SAMPLER2D,
            &tex.to_variant(),
        );
        rs.global_shader_parameter_add(
            "cloud_scale",
            GlobalShaderParameterType::FLOAT,
            &100.0_f32.to_variant(),
        );
        rs.global_shader_parameter_add(
            "cloud_world_y",
            GlobalShaderParameterType::FLOAT,
            &50.0_f32.to_variant(),
        );
        rs.global_shader_parameter_add(
            "cloud_speed",
            GlobalShaderParameterType::FLOAT,
            &0.02_f32.to_variant(),
        );
        rs.global_shader_parameter_add(
            "cloud_contrast",
            GlobalShaderParameterType::FLOAT,
            &0.0_f32.to_variant(), // zero contrast → flat mid-gray → no shadow pattern
        );
        rs.global_shader_parameter_add(
            "cloud_threshold",
            GlobalShaderParameterType::FLOAT,
            &0.0_f32.to_variant(),
        );
        rs.global_shader_parameter_add(
            "cloud_direction",
            GlobalShaderParameterType::VEC2,
            &Vector2::new(1.0, 0.0).to_variant(),
        );
        rs.global_shader_parameter_add(
            "light_direction",
            GlobalShaderParameterType::VEC3,
            &Vector3::new(0.0, -1.0, 0.0).to_variant(),
        );
        rs.global_shader_parameter_add(
            "cloud_shadow_min",
            GlobalShaderParameterType::FLOAT,
            &1.0_f32.to_variant(), // min=1.0 → clamp always returns 1.0 → full brightness
        );
        rs.global_shader_parameter_add(
            "cloud_diverge_angle",
            GlobalShaderParameterType::FLOAT,
            &10.0_f32.to_variant(),
        );

        // Wind system defaults (zero-effect: no wind displacement)
        rs.global_shader_parameter_add(
            "wind_noise",
            GlobalShaderParameterType::SAMPLER2D,
            &tex.to_variant(),
        );
        rs.global_shader_parameter_add(
            "wind_noise_threshold",
            GlobalShaderParameterType::FLOAT,
            &(-0.5_f32).to_variant(), // white tex → combined=1.0 → clamp(1.0-0.5)=0.5 → (0.5-0.5)*2=0
        );
        rs.global_shader_parameter_add(
            "wind_noise_scale",
            GlobalShaderParameterType::FLOAT,
            &0.071_f32.to_variant(),
        );
        rs.global_shader_parameter_add(
            "wind_noise_speed",
            GlobalShaderParameterType::FLOAT,
            &0.025_f32.to_variant(),
        );
        rs.global_shader_parameter_add(
            "wind_noise_direction",
            GlobalShaderParameterType::VEC2,
            &Vector2::new(0.0, 1.0).to_variant(),
        );
        rs.global_shader_parameter_add(
            "wind_diverge_angle",
            GlobalShaderParameterType::FLOAT,
            &10.0_f32.to_variant(),
        );

        godot_print!("PixyTerrain: Registered fallback environment globals");
    }

    /// Ensure terrain material exists, creating it from shader if needed.
    fn ensure_materials_and_sync(&mut self) {
        self.ensure_terrain_material();
        self.ensure_grass_material();
        self.force_batch_update();
        self.force_grass_material_update();
    }

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
            godot_print!("PixyTerrain: Grass material already exists, skipping");
            return;
        }

        godot_print!("PixyTerrain: ensure_grass_material() — creating grass material...");

        let mut loader = ResourceLoader::singleton();

        let shader_path = "res://addons/pixy_terrain/resources/shaders/mst_grass.gdshader";
        let exists = loader.exists(shader_path);
        godot_print!(
            "PixyTerrain: Grass shader exists at {}: {}",
            shader_path,
            exists
        );
        if !exists {
            godot_warn!("PixyTerrain: Grass shader not found at {}", shader_path);
            return;
        }

        let Some(res) = loader.load(shader_path) else {
            godot_warn!(
                "PixyTerrain: Failed to load grass shader from {}",
                shader_path
            );
            return;
        };
        godot_print!(
            "PixyTerrain: Loaded grass shader resource: {}",
            res.get_class()
        );

        let Ok(shader) = res.try_cast::<Shader>() else {
            godot_warn!("PixyTerrain: Resource is not a Shader");
            return;
        };
        godot_print!(
            "PixyTerrain: Shader cast OK, code length = {}",
            shader.get_code().len()
        );

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

        godot_print!(
            "PixyTerrain: Created shared grass material and QuadMesh (size={})",
            self.grass_size
        );
    }

    /// Sync all shader parameters from terrain exports to the terrain material.
    #[func]
    pub fn force_batch_update(&mut self) {
        use crate::shader_sync::*;

        if self.terrain_material.is_none() {
            return;
        }

        self.is_batch_updating = true;

        // Ensure arrays are sized before reading
        self.ensure_array_sizes();

        // Collect all values before borrowing material
        let dimensions = self.dimensions;
        let cell_size = self.cell_size;
        let wall_threshold = self.wall_threshold;
        let use_hard = self.blend_mode != 0;
        let blend_mode = self.blend_mode;
        let blend_sharpness = self.blend_sharpness;
        let blend_noise_scale = self.blend_noise_scale;
        let blend_noise_strength = self.blend_noise_strength;
        let ground_colors: Vec<Color> = (0..6)
            .map(|i| self.ground_colors[i])
            .collect();
        let scales: Vec<f32> = (0..15)
            .map(|i| self.texture_scales[i])
            .collect();
        let textures = self.get_texture_slots();
        let shadow_color = self.shadow_color;
        let shadow_bands = self.shadow_bands;
        let shadow_intensity = self.shadow_intensity;
        let cross_section_enabled = self.cross_section_enabled;

        let mat = self.terrain_material.as_mut().unwrap();

        // Scalar params
        sync_shader_params!(mat, [
            "chunk_size"           => dimensions,
            "cell_size"            => cell_size,
            "wall_threshold"       => wall_threshold,
            "use_hard_textures"    => use_hard,
            "blend_mode"           => blend_mode,
            "blend_sharpness"      => blend_sharpness,
            "blend_noise_scale"    => blend_noise_scale,
            "blend_noise_strength" => blend_noise_strength,
            "shadow_color"         => shadow_color,
            "bands"                => shadow_bands,
            "shadow_intensity"     => shadow_intensity,
            "cross_section_enabled" => cross_section_enabled,
        ]);

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

        self.is_batch_updating = false;
    }

    /// Sync all grass shader parameters from terrain fields to the shared grass material.
    pub fn force_grass_material_update(&mut self) {
        use crate::shader_sync::*;

        if self.grass_material.is_none() {
            return;
        }

        // Collect ALL values BEFORE borrowing grass_material mutably
        let is_merge_round = matches!(
            MergeMode::from_index(self.merge_mode),
            MergeMode::RoundedPolyhedron | MergeMode::SemiRound | MergeMode::Spherical
        );
        let wall_threshold = self.wall_threshold;

        self.ensure_array_sizes();

        let sprites: Vec<Option<Gd<Texture2D>>> =
            (0..6).map(|i| self.get_grass_sprite_or_default(i)).collect();

        let ground_colors: Vec<Color> = (0..6)
            .map(|i| self.ground_colors[i])
            .collect();
        let use_base_color: [bool; 6] = std::array::from_fn(|i| {
            get_variant_texture(&self.textures, i).is_none()
        });
        let tex_has_grass = self.tex_has_grass_array();

        let shadow_color = self.shadow_color;
        let grass_size = self.grass_size;
        let grass_framerate = self.grass_framerate;
        let grass_quantised = self.grass_quantised;
        let world_space_sway = self.world_space_sway;
        let world_sway_angle = self.world_sway_angle;
        let fake_perspective_scale = self.fake_perspective_scale;
        let view_space_sway = self.view_space_sway;
        let view_sway_speed = self.view_sway_speed;
        let view_sway_angle = self.view_sway_angle;
        let character_displacement = self.character_displacement_enabled;
        let player_displacement_angle_z = self.player_displacement_angle_z;
        let player_displacement_angle_x = self.player_displacement_angle_x;
        let radius_exponent = self.radius_exponent;
        let grass_toon_cuts = self.grass_toon_cuts;
        let grass_toon_wrap = self.grass_toon_wrap;
        let grass_toon_steepness = self.grass_toon_steepness;
        let grass_threshold_gradient_size = self.grass_threshold_gradient_size;

        let mat = self.grass_material.as_mut().unwrap();

        // Core settings
        sync_shader_params!(mat, [
            "is_merge_round"    => is_merge_round,
            "wall_threshold"    => wall_threshold,
        ]);

        // Grass textures
        for (i, name) in GRASS_TEXTURE_NAMES.iter().enumerate() {
            if let Some(ref tex) = sprites[i] {
                mat.set_shader_parameter(*name, &tex.to_variant());
            }
        }

        // Ground colors
        for (i, name) in GRASS_COLOR_NAMES.iter().enumerate() {
            mat.set_shader_parameter(*name, &ground_colors[i].to_variant());
        }

        // use_base_color flags
        for (i, name) in USE_BASE_COLOR_NAMES.iter().enumerate() {
            mat.set_shader_parameter(*name, &use_base_color[i].to_variant());
        }

        // use_grass_tex flags
        for (i, name) in USE_GRASS_TEX_NAMES.iter().enumerate() {
            mat.set_shader_parameter(*name, &tex_has_grass[i].to_variant());
        }

        // Dylearn animation + toon lighting scalars
        sync_shader_params!(mat, [
            "framerate"                  => grass_framerate,
            "quantised"                  => grass_quantised,
            "world_space_sway"           => world_space_sway,
            "world_sway_angle"           => world_sway_angle,
            "fake_perspective_scale"     => fake_perspective_scale,
            "view_space_sway"            => view_space_sway,
            "view_sway_speed"            => view_sway_speed,
            "view_sway_angle"            => view_sway_angle,
            "character_displacement"     => character_displacement,
            "player_displacement_angle_z" => player_displacement_angle_z,
            "player_displacement_angle_x" => player_displacement_angle_x,
            "radius_exponent"            => radius_exponent,
            "cuts"                       => grass_toon_cuts,
            "wrap"                       => grass_toon_wrap,
            "steepness"                  => grass_toon_steepness,
            "threshold_gradient_size"    => grass_threshold_gradient_size,
            "shadow_color"               => shadow_color,
        ]);

        // Update QuadMesh size
        if let Some(ref mut quad) = self.grass_quad_mesh {
            quad.set_size(grass_size);
            quad.set_center_offset(Vector3::new(0.0, grass_size.y / 2.0, 0.0));
        }
    }

    /// Pad arrays to correct lengths on scene load or first use.
    fn ensure_array_sizes(&mut self) {
        let nil = Variant::nil();
        while self.textures.len() < 15 {
            self.textures.push(&nil);
        }
        while self.texture_scales.len() < 15 {
            self.texture_scales.push(1.0);
        }
        let default_colors = [
            Color::from_rgba(0.3922, 0.4706, 0.3176, 1.0),
            Color::from_rgba(0.3216, 0.4824, 0.3843, 1.0),
            Color::from_rgba(0.3725, 0.4235, 0.2941, 1.0),
            Color::from_rgba(0.3922, 0.4745, 0.2549, 1.0),
            Color::from_rgba(0.2902, 0.4941, 0.3647, 1.0),
            Color::from_rgba(0.4431, 0.4471, 0.3647, 1.0),
        ];
        while self.ground_colors.len() < 6 {
            self.ground_colors
                .push(default_colors[self.ground_colors.len()]);
        }
        while self.grass_sprites.len() < 6 {
            self.grass_sprites.push(&nil);
        }
    }

    fn get_texture_slots(&self) -> [Option<Gd<Texture2D>>; 16] {
        let mut slots: [Option<Gd<Texture2D>>; 16] = Default::default();
        for (i, slot) in slots.iter_mut().enumerate().take(15) {
            *slot = self.get_ground_texture_or_default(i);
        }
        // Slot 15: void texture (transparent)
        slots
    }

    fn get_ground_texture_or_default(&self, index: usize) -> Option<Gd<Texture2D>> {
        if let Some(tex) = get_variant_texture(&self.textures, index) {
            return Some(tex);
        }
        load_default_texture(DEFAULT_GROUND_TEXTURE_PATH)
    }

    fn get_grass_sprite_or_default(&self, index: usize) -> Option<Gd<Texture2D>> {
        if let Some(tex) = get_variant_texture(&self.grass_sprites, index) {
            return Some(tex);
        }
        load_default_texture("res://addons/pixy_terrain/resources/textures/grass_leaf_sprite.png")
    }

    fn extract_ground_image(&self, index: usize) -> Option<Gd<Image>> {
        let tex = self.get_ground_texture_or_default(index)?;
        let mut img = tex.get_image()?;
        img.decompress();
        Some(img)
    }

    pub fn tex_has_grass_array(&self) -> [bool; 5] {
        [
            self.tex2_has_grass,
            self.tex3_has_grass,
            self.tex4_has_grass,
            self.tex5_has_grass,
            self.tex6_has_grass,
        ]
    }

    fn make_shared_params(&self) -> SharedTerrainParams {
        SharedTerrainParams {
            dimensions: self.dimensions,
            cell_size: self.cell_size,
            merge_mode: MergeMode::from_index(self.merge_mode),
            blend_mode: if self.blend_mode == 0 {
                BlendMode::Interpolated
            } else {
                BlendMode::Direct
            },
            wall_threshold: self.wall_threshold,
            ridge_threshold: self.ridge_threshold,
            ledge_threshold: self.ledge_threshold,
            use_ridge_texture: self.use_ridge_texture,
        }
    }

    fn make_terrain_config(&self) -> TerrainConfig {
        TerrainConfig {
            shared: self.make_shared_params(),
            extra_collision_layer: self.extra_collision_layer,
        }
    }

    fn make_grass_config(&self) -> GrassConfig {
        GrassConfig {
            shared: self.make_shared_params(),
            subdivisions: self.grass_subdivisions,
            grass_size: self.grass_size,
            grass_sprites: std::array::from_fn(|i| self.get_grass_sprite_or_default(i)),
            ground_colors: std::array::from_fn(|i| self.ground_colors[i]),
            tex_has_grass: self.tex_has_grass_array(),
            grass_mesh: self.grass_mesh.clone(),
            grass_material: self.grass_material.clone(),
            grass_quad_mesh: self
                .grass_quad_mesh
                .as_ref()
                .map(|q| q.clone().upcast::<godot::classes::Mesh>()),
            ground_images: std::array::from_fn(|i| self.extract_ground_image(i)),
            texture_scales: std::array::from_fn(|i| self.texture_scales[i]),
        }
    }

    /// Regenerate the entire terrain: clear all chunks, create a single chunk at (0,0).
    #[func]
    pub fn regenerate(&mut self) {
        godot_print!("PixyTerrain: regenerate()");
        self.ensure_materials_and_sync();
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

    /// Get a chunk handle by coordinates.
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

    /// Create a new chunk at the given chunk coordinates, copying shared edges from neighbors.
    #[func]
    pub fn add_new_chunk(&mut self, chunk_x: i32, chunk_z: i32) {
        // Ensure material exists (may not yet if called before _deferred_enter_tree)
        self.ensure_terrain_material();
        self.ensure_grass_material();
        self.force_batch_update();

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

    fn add_chunk_internal(
        &mut self,
        coords: Vector2i,
        mut chunk: Gd<PixyTerrainChunk>,
        regenerate: bool,
    ) {
        let terrain_config = self.make_terrain_config();
        let grass_config = self.make_grass_config();
        let noise = self.noise_hmap.clone();
        let material = self.terrain_material.clone();

        self.chunks.insert([coords.x, coords.y], chunk.clone());

        {
            let mut chunk_bind = chunk.bind_mut();
            chunk_bind.chunk_coords = coords;
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

    /// Apply a composite pattern action. Called by undo/redo.
    /// `patterns` is a VarDictionary with keys: "height", "color_0", "color_1",
    /// "wall_color_0", "wall_color_1", "grass_mask".
    /// Each value is Dict<Vector2i(chunk), Dict<Vector2i(cell), value>>.
    #[func]
    pub fn apply_composite_pattern(&mut self, patterns: VarDictionary) {
        let mut affected_chunks: HashMap<[i32; 2], Gd<PixyTerrainChunk>> = HashMap::new();

        let keys_in_order = [
            "wall_color_0",
            "wall_color_1",
            "height",
            "grass_mask",
            "color_0",
            "color_1",
        ];

        for &key in &keys_in_order {
            let Some(outer_variant) = patterns.get(key) else {
                continue;
            };
            let outer_dict: VarDictionary = outer_variant.to();

            let chunk_entries: Vec<(Vector2i, VarDictionary)> = outer_dict
                .iter_shared()
                .map(|(k, v)| (k.to::<Vector2i>(), v.to::<VarDictionary>()))
                .collect();

            for (chunk_coords, cell_dict) in chunk_entries {
                let Some(mut chunk) = self.get_chunk(chunk_coords.x, chunk_coords.y) else {
                    continue;
                };

                affected_chunks
                    .entry([chunk_coords.x, chunk_coords.y])
                    .or_insert_with(|| chunk.clone());

                let cell_entries: Vec<(Vector2i, Variant)> = cell_dict
                    .iter_shared()
                    .map(|(k, v)| (k.to::<Vector2i>(), v.clone()))
                    .collect();

                for (cell, cell_value) in cell_entries {
                    let mut c = chunk.bind_mut();
                    match key {
                        "height" => {
                            let h: f32 = cell_value.to();
                            c.draw_height(cell.x, cell.y, h);
                        }
                        "color_0" => {
                            let color: Color = cell_value.to();
                            c.draw_color_0(cell.x, cell.y, color);
                        }
                        "color_1" => {
                            let color: Color = cell_value.to();
                            c.draw_color_1(cell.x, cell.y, color);
                        }
                        "wall_color_0" => {
                            let color: Color = cell_value.to();
                            c.draw_wall_color_0(cell.x, cell.y, color);
                        }
                        "wall_color_1" => {
                            let color: Color = cell_value.to();
                            c.draw_wall_color_1(cell.x, cell.y, color);
                        }
                        "grass_mask" => {
                            let mask: Color = cell_value.to();
                            c.draw_grass_mask(cell.x, cell.y, mask);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Regenerate mesh once per affected chunk
        for (_, mut chunk) in affected_chunks {
            chunk.bind_mut().regenerate_mesh();
        }
    }

    #[func]
    pub fn save_to_preset(&mut self) {
        if self.current_texture_preset.is_none() {
            let preset = Gd::<crate::texture_preset::PixyTexturePreset>::default();
            self.current_texture_preset = Some(preset);
        }

        if let Some(ref mut preset) = self.current_texture_preset {
            let mut list_gd = {
                let p = preset.bind();
                if let Some(ref existing) = p.textures {
                    existing.clone()
                } else {
                    Gd::<crate::texture_preset::PixyTextureList>::default()
                }
            };

            {
                let mut l = list_gd.bind_mut();
                l.textures = self.textures.clone();
                l.texture_scales = self.texture_scales.clone();
                l.ground_colors = self.ground_colors.clone();
                l.grass_sprites = self.grass_sprites.clone();
                l.has_grass_2 = self.tex2_has_grass;
                l.has_grass_3 = self.tex3_has_grass;
                l.has_grass_4 = self.tex4_has_grass;
                l.has_grass_5 = self.tex5_has_grass;
                l.has_grass_6 = self.tex6_has_grass;
            }
            preset.bind_mut().textures = Some(list_gd);
            godot_print!("PixyTerrain: Saved texture settings to preset");
        }
    }

    /// Load texture settings from the current_texture_preset.
    #[func]
    pub fn load_from_preset(&mut self) {
        let Some(ref preset) = self.current_texture_preset else {
            godot_warn!("PixyTerrain: No preset assigned to load from");
            return;
        };

        let p = preset.bind();
        let Some(ref list_gd) = p.textures else {
            godot_warn!("PixyTerrain: Preset has no texture list to load");
            return;
        };
        let l = list_gd.bind();
        self.textures = l.textures.clone();
        self.texture_scales = l.texture_scales.clone();
        self.ground_colors = l.ground_colors.clone();
        self.grass_sprites = l.grass_sprites.clone();
        self.tex2_has_grass = l.has_grass_2;
        self.tex3_has_grass = l.has_grass_3;
        self.tex4_has_grass = l.has_grass_4;
        self.tex5_has_grass = l.has_grass_5;
        self.tex6_has_grass = l.has_grass_6;
        drop(l);
        drop(p);

        self.force_batch_update();
        self.force_grass_material_update();
        godot_print!("PixyTerrain: Loaded texture settings from preset");
    }

    /// Ensure all texture slots have sensible defaults.
    #[func]
    pub fn ensure_textures(&mut self) {
        self.ensure_terrain_material();
        self.force_batch_update();
    }

    /// Regenerate grass on all chunks.
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

/// Extract a `Gd<Texture2D>` from a VarArray slot, returning None for nil/out-of-bounds.
pub fn get_variant_texture(arr: &VarArray, i: usize) -> Option<Gd<Texture2D>> {
    if i >= arr.len() {
        return None;
    }
    let v = arr.at(i);
    if v.is_nil() {
        return None;
    }
    v.try_to::<Gd<Texture2D>>().ok()
}

/// Set a texture into a VarArray slot (nil for None).
pub fn set_variant_texture(arr: &mut VarArray, i: usize, tex: Option<Gd<Texture2D>>) {
    if i < arr.len() {
        let v = match tex {
            Some(t) => t.to_variant(),
            None => Variant::nil(),
        };
        arr.set(i, &v);
    }
}

/// Load a default texture from a resource path, returning None on failure.
fn load_default_texture(path: &str) -> Option<Gd<Texture2D>> {
    let mut loader = ResourceLoader::singleton();
    if loader.exists(path) {
        loader
            .load(path)
            .and_then(|r| r.try_cast::<Texture2D>().ok())
    } else {
        None
    }
}
