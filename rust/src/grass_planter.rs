use std::collections::HashMap;

use godot::classes::{
    IMultiMeshInstance3D, Mesh, MultiMesh, MultiMeshInstance3D, QuadMesh, ResourceLoader, Shader,
    ShaderMaterial, Texture2D,
};
use godot::obj::InstanceId;
use godot::prelude::*;

use crate::chunk::PixyTerrainChunk;
use crate::marching_squares::{get_dominant_color, CellGeometry, MergeMode};

/// Cached grass configuration (avoids needing to bind terrain during grass operations).
/// Passed from terrain to chunk to grass planter at initialization time.
#[derive(Clone)]
pub struct GrassConfig {
    pub dimensions: Vector3i,
    pub subdivisions: i32,
    pub grass_size: Vector2,
    pub cell_size: Vector2,
    pub wall_threshold: f32,
    pub merge_mode: i32,
    pub animation_fps: i32,
    pub ledge_threshold: f32,
    pub ridge_threshold: f32,
    pub grass_sprites: [Option<Gd<Texture2D>>; 6],
    pub ground_colors: [Color; 6],
    pub tex_has_grass: [bool; 5],
    pub grass_mesh: Option<Gd<Mesh>>,
}

impl Default for GrassConfig {
    fn default() -> Self {
        Self {
            dimensions: Vector3i::new(33, 32, 33),
            subdivisions: 3,
            grass_size: Vector2::new(1.0, 1.0),
            cell_size: Vector2::new(2.0, 2.0),
            wall_threshold: 0.0,
            merge_mode: 1,
            animation_fps: 0,
            ledge_threshold: 0.25,
            ridge_threshold: 1.0,
            grass_sprites: [None, None, None, None, None, None],
            ground_colors: [
                Color::from_rgba(0.3922, 0.4706, 0.3176, 1.0),
                Color::from_rgba(0.3216, 0.4824, 0.3843, 1.0),
                Color::from_rgba(0.3725, 0.4235, 0.2941, 1.0),
                Color::from_rgba(0.3922, 0.4745, 0.2549, 1.0),
                Color::from_rgba(0.2902, 0.4941, 0.3647, 1.0),
                Color::from_rgba(0.4431, 0.4471, 0.3647, 1.0),
            ],
            tex_has_grass: [true, true, true, true, true],
            grass_mesh: None,
        }
    }
}

/// Path to the grass shader file.
const GRASS_SHADER_PATH: &str = "res://resources/shaders/mst_grass.gdshader";
/// Path to the wind noise texture.
const WIND_NOISE_TEXTURE_PATH: &str = "res://resources/textures/wind_noise_texture.tres";

/// MultiMeshInstance3D grass placement system.
/// Port of Yugen's MarchingSquaresGrassPlanter.
#[derive(GodotClass)]
#[class(base=MultiMeshInstance3D, init, tool)]
pub struct PixyGrassPlanter {
    base: Base<MultiMeshInstance3D>,

    /// Instance ID of the parent chunk (avoids borrow issues with Gd storage).
    chunk_instance_id: Option<InstanceId>,

    /// Cached grass configuration (avoids needing to bind terrain).
    grass_config: Option<GrassConfig>,
}

#[godot_api]
impl IMultiMeshInstance3D for PixyGrassPlanter {}

#[godot_api]
impl PixyGrassPlanter {
    /// Initialize the MultiMesh using cached config (avoids borrow issues).
    pub fn setup_with_config(
        &mut self,
        chunk_id: InstanceId,
        config: GrassConfig,
        regenerate: bool,
    ) {
        self.chunk_instance_id = Some(chunk_id);
        self.grass_config = Some(config.clone());

        let dim = config.dimensions;
        let subdivisions = config.subdivisions.max(1);
        let grass_size = config.grass_size;

        let instance_count = (dim.x - 1) * (dim.z - 1) * subdivisions * subdivisions;

        let existing = self.base().get_multimesh();
        if (regenerate && existing.is_some()) || existing.is_none() {
            let mut mm = MultiMesh::new_gd();
            mm.set_instance_count(0);
            mm.set_transform_format(godot::classes::multi_mesh::TransformFormat::TRANSFORM_3D);
            mm.set_use_custom_data(true);
            mm.set_instance_count(instance_count);

            // Use custom grass mesh if provided, otherwise default to QuadMesh
            if let Some(ref mesh) = config.grass_mesh {
                mm.set_mesh(mesh);
            } else {
                let mut quad = QuadMesh::new_gd();
                quad.set_size(grass_size);
                // Set center_offset.y to half height so grass stands upright from ground
                quad.set_center_offset(Vector3::new(0.0, grass_size.y / 2.0, 0.0));
                mm.set_mesh(&quad);
            }

            self.base_mut().set_multimesh(&mm);
        }

        self.base_mut().set_cast_shadows_setting(
            godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
        );

        // Set up grass material using cached config
        self.setup_grass_material(
            config.wall_threshold,
            config.merge_mode,
            config.animation_fps,
            &config.grass_sprites,
            &config.ground_colors,
            &config.tex_has_grass,
        );
    }

    /// Initialize the MultiMesh with proper instance count and mesh (legacy method).
    /// Note: This requires the chunk to have a terrain_config set.
    #[func]
    pub fn setup(&mut self, chunk: Gd<PixyTerrainChunk>, regenerate: bool) {
        let chunk_id = chunk.instance_id();
        // Use default grass config since we can't get terrain config from chunk in legacy path
        let config = GrassConfig::default();
        self.setup_with_config(chunk_id, config, regenerate);
    }

    /// Regenerate grass for all cells in the chunk.
    /// Takes cell_geometry as parameter to avoid needing to bind the chunk.
    pub fn regenerate_all_cells_with_geometry(
        &mut self,
        cell_geometry: &HashMap<[i32; 2], CellGeometry>,
    ) {
        let Some(config) = self.grass_config.as_ref() else {
            godot_error!("PixyGrassPlanter: regenerate_all_cells — no grass config");
            return;
        };

        let dim = config.dimensions;

        if self.base().get_multimesh().is_none() {
            // Re-setup with current config
            if let (Some(c_id), Some(cfg)) = (self.chunk_instance_id, self.grass_config.clone()) {
                self.setup_with_config(c_id, cfg, true);
            }
        }

        for z in 0..(dim.z - 1) {
            for x in 0..(dim.x - 1) {
                self.generate_grass_on_cell_with_geometry(Vector2i::new(x, z), cell_geometry);
            }
        }
    }

    /// Legacy method for backward compatibility - warns and does nothing.
    #[func]
    pub fn regenerate_all_cells(&mut self) {
        godot_warn!("PixyGrassPlanter: regenerate_all_cells() called without geometry - skipping");
    }

    /// Generate grass instances for a single cell using provided geometry.
    fn generate_grass_on_cell_with_geometry(
        &mut self,
        cell_coords: Vector2i,
        cell_geometry: &HashMap<[i32; 2], CellGeometry>,
    ) {
        let Some(config) = self.grass_config.as_ref() else {
            return;
        };

        // Get geometry from passed parameter instead of chunk lookup
        let geo = match cell_geometry.get(&[cell_coords.x, cell_coords.y]) {
            Some(g) => g.clone(),
            None => return,
        };

        // Use cell_size from config instead of chunk lookup
        let cell_size = config.cell_size;

        let dim = config.dimensions;
        let subdivisions = config.subdivisions.max(1);
        let ledge_threshold = config.ledge_threshold;
        let ridge_threshold = config.ridge_threshold;
        let tex_has_grass = [
            true, // tex1 (base grass) always has grass
            config.tex_has_grass[0],
            config.tex_has_grass[1],
            config.tex_has_grass[2],
            config.tex_has_grass[3],
            config.tex_has_grass[4],
        ];

        let count = (subdivisions * subdivisions) as usize;

        // Generate random sample points within this cell
        let mut points: Vec<Vector2> = Vec::with_capacity(count);
        for z in 0..subdivisions {
            for x in 0..subdivisions {
                let rx: f32 = rand_f32();
                let rz: f32 = rand_f32();
                points.push(Vector2::new(
                    (cell_coords.x as f32 + (x as f32 + rx) / subdivisions as f32) * cell_size.x,
                    (cell_coords.y as f32 + (z as f32 + rz) / subdivisions as f32) * cell_size.y,
                ));
            }
        }

        let base_index = (cell_coords.y * (dim.x - 1) + cell_coords.x) as usize * count;
        let end_index = base_index + count;
        let mut index = base_index;

        let Some(mm) = self.base().get_multimesh() else {
            return;
        };
        let mut mm = mm.clone();
        let total_instances = mm.get_instance_count() as usize;

        // Process each floor triangle
        self.place_grass_on_triangles(
            &geo,
            &mut points,
            &mut index,
            end_index.min(total_instances),
            &mut mm,
            cell_size,
            ledge_threshold,
            ridge_threshold,
            &tex_has_grass,
        );

        // Hide remaining unused instances
        let hidden_transform = Transform3D::new(
            Basis::from_scale(Vector3::ZERO),
            Vector3::new(9999.0, 9999.0, 9999.0),
        );
        while index < end_index && index < total_instances {
            mm.set_instance_transform(index as i32, hidden_transform);
            index += 1;
        }
    }

    /// Legacy method for backward compatibility - warns and does nothing.
    #[func]
    pub fn generate_grass_on_cell(&mut self, _cell_coords: Vector2i) {
        godot_warn!(
            "PixyGrassPlanter: generate_grass_on_cell() called without geometry - skipping"
        );
    }
}

impl PixyGrassPlanter {
    /// Set up or update the grass ShaderMaterial with proper parameters.
    #[allow(clippy::too_many_arguments)]
    fn setup_grass_material(
        &mut self,
        wall_threshold: f32,
        merge_mode: i32,
        animation_fps: i32,
        grass_sprites: &[Option<Gd<godot::classes::Texture2D>>; 6],
        ground_colors: &[Color; 6],
        tex_has_grass: &[bool; 5],
    ) {
        let mut loader = ResourceLoader::singleton();

        // Try loading the grass shader
        if !loader.exists(GRASS_SHADER_PATH) {
            godot_warn!("PixyGrassPlanter: Grass shader not found at {GRASS_SHADER_PATH}");
            return;
        }

        let resource = loader.load(GRASS_SHADER_PATH);
        let Some(res) = resource else {
            godot_warn!("PixyGrassPlanter: Failed to load grass shader");
            return;
        };

        let Ok(shader) = res.try_cast::<Shader>() else {
            godot_warn!("PixyGrassPlanter: Resource is not a Shader");
            return;
        };

        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);

        // Core parameters
        let is_merge_round = matches!(
            MergeMode::from_index(merge_mode),
            MergeMode::RoundedPolyhedron | MergeMode::SemiRound | MergeMode::Spherical
        );
        mat.set_shader_parameter("is_merge_round", &is_merge_round.to_variant());
        mat.set_shader_parameter("wall_threshold", &wall_threshold.to_variant());
        mat.set_shader_parameter("fps", &(animation_fps as f32).to_variant());

        // Grass textures (grass_texture, grass_texture_2, ..., grass_texture_6)
        let texture_names = [
            "grass_texture",
            "grass_texture_2",
            "grass_texture_3",
            "grass_texture_4",
            "grass_texture_5",
            "grass_texture_6",
        ];
        for (i, name) in texture_names.iter().enumerate() {
            if let Some(ref tex) = grass_sprites[i] {
                mat.set_shader_parameter(*name, &tex.to_variant());
            }
        }

        // Base grass color (grass_base_color) and per-texture colors
        mat.set_shader_parameter(
            "grass_base_color",
            &Vector3::new(ground_colors[0].r, ground_colors[0].g, ground_colors[0].b).to_variant(),
        );
        let color_names = [
            "grass_color_2",
            "grass_color_3",
            "grass_color_4",
            "grass_color_5",
            "grass_color_6",
        ];
        for (i, name) in color_names.iter().enumerate() {
            let c = ground_colors[i + 1];
            mat.set_shader_parameter(*name, &Vector3::new(c.r, c.g, c.b).to_variant());
        }

        // use_base_color flags (true when no texture is assigned)
        mat.set_shader_parameter("use_base_color", &grass_sprites[0].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_2", &grass_sprites[1].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_3", &grass_sprites[2].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_4", &grass_sprites[3].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_5", &grass_sprites[4].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_6", &grass_sprites[5].is_none().to_variant());

        // use_grass_tex_* flags (tex2-6 has_grass toggles)
        mat.set_shader_parameter("use_grass_tex_2", &tex_has_grass[0].to_variant());
        mat.set_shader_parameter("use_grass_tex_3", &tex_has_grass[1].to_variant());
        mat.set_shader_parameter("use_grass_tex_4", &tex_has_grass[2].to_variant());
        mat.set_shader_parameter("use_grass_tex_5", &tex_has_grass[3].to_variant());
        mat.set_shader_parameter("use_grass_tex_6", &tex_has_grass[4].to_variant());

        // Wind animation parameters (mst_grass.gdshader lines 39-46)
        // Values from Yugen's mst_grass_mesh.tres: wind_scale=0.02, wind_speed=0.14
        mat.set_shader_parameter("wind_direction", &Vector2::new(1.0, 1.0).to_variant());
        mat.set_shader_parameter("wind_scale", &0.02_f32.to_variant());
        mat.set_shader_parameter("wind_speed", &0.14_f32.to_variant());
        mat.set_shader_parameter("animate_active", &true.to_variant());

        // Load wind noise texture for wind animation
        if loader.exists(WIND_NOISE_TEXTURE_PATH) {
            if let Some(wind_tex) = loader.load(WIND_NOISE_TEXTURE_PATH) {
                mat.set_shader_parameter("wind_texture", &wind_tex.to_variant());
            }
        }

        // Shading parameters (mst_grass.gdshader lines 48-52)
        mat.set_shader_parameter(
            "shadow_color",
            &Color::from_rgba(0.0, 0.0, 0.0, 1.0).to_variant(),
        );
        mat.set_shader_parameter("bands", &5_i32.to_variant());
        mat.set_shader_parameter("shadow_intensity", &0.0_f32.to_variant());

        self.base_mut().set_material_override(&mat);
        godot_print!("PixyGrassPlanter: Grass material set up successfully");
    }

    /// Place grass instances on floor triangles using barycentric testing.
    #[allow(clippy::too_many_arguments)]
    fn place_grass_on_triangles(
        &self,
        geo: &CellGeometry,
        points: &mut Vec<Vector2>,
        index: &mut usize,
        end_index: usize,
        mm: &mut Gd<MultiMesh>,
        _cell_size: Vector2,
        ledge_threshold: f32,
        ridge_threshold: f32,
        tex_has_grass: &[bool; 6],
    ) {
        let hidden_transform = Transform3D::new(
            Basis::from_scale(Vector3::ZERO),
            Vector3::new(9999.0, 9999.0, 9999.0),
        );

        let num_verts = geo.verts.len();
        let mut tri_idx = 0;

        while tri_idx + 2 < num_verts {
            // Only place grass on floor triangles
            if !geo.is_floor[tri_idx] {
                tri_idx += 3;
                continue;
            }

            let a = geo.verts[tri_idx];
            let b = geo.verts[tri_idx + 1];
            let c = geo.verts[tri_idx + 2];

            // Precompute barycentric denominator (2D projection on XZ)
            let v0 = Vector2::new(c.x - a.x, c.z - a.z);
            let v1 = Vector2::new(b.x - a.x, b.z - a.z);
            let dot00 = v0.dot(v0);
            let dot01 = v0.dot(v1);
            let dot11 = v1.dot(v1);
            let denom = dot00 * dot11 - dot01 * dot01;

            if denom.abs() < 1e-10 {
                tri_idx += 3;
                continue;
            }
            let inv_denom = 1.0 / denom;

            let mut pt_idx = 0;
            while pt_idx < points.len() {
                if *index >= end_index {
                    return;
                }

                let v2 = Vector2::new(points[pt_idx].x - a.x, points[pt_idx].y - a.z);
                let dot02 = v0.dot(v2);
                let dot12 = v1.dot(v2);

                let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
                if u < 0.0 {
                    pt_idx += 1;
                    continue;
                }

                let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;
                if v < 0.0 {
                    pt_idx += 1;
                    continue;
                }

                if u + v <= 1.0 {
                    // Point is inside this triangle — remove it from candidates
                    points.remove(pt_idx);

                    // Interpolate 3D position using barycentric weights
                    // GDScript: p = a*(1-u-v) + b*u + c*v  =>  a*w + b*u + c*v
                    // where w = 1-u-v, but vertex order in GDScript is [i]*u + [i+1]*v + [i+2]*w
                    // So: a*u + b*v + c*w
                    let w = 1.0 - u - v;
                    let p = Vector3::new(
                        a.x * u + b.x * v + c.x * w,
                        a.y * u + b.y * v + c.y * w,
                        a.z * u + b.z * v + c.z * w,
                    );

                    // Ledge/ridge avoidance (same barycentric order)
                    let uv = Vector2::new(
                        geo.uvs[tri_idx].x * u
                            + geo.uvs[tri_idx + 1].x * v
                            + geo.uvs[tri_idx + 2].x * w,
                        geo.uvs[tri_idx].y * u
                            + geo.uvs[tri_idx + 1].y * v
                            + geo.uvs[tri_idx + 2].y * w,
                    );
                    let on_ledge = uv.x > 1.0 - ledge_threshold || uv.y > 1.0 - ridge_threshold;

                    // Interpolate vertex colors to determine texture
                    let col0 = interpolate_color(
                        geo.colors_0[tri_idx],
                        geo.colors_0[tri_idx + 1],
                        geo.colors_0[tri_idx + 2],
                        u,
                        v,
                        w,
                    );
                    let col1 = interpolate_color(
                        geo.colors_1[tri_idx],
                        geo.colors_1[tri_idx + 1],
                        geo.colors_1[tri_idx + 2],
                        u,
                        v,
                        w,
                    );
                    let col0 = get_dominant_color(col0);
                    let col1 = get_dominant_color(col1);

                    // Grass mask check
                    let mask = interpolate_color(
                        geo.grass_mask[tri_idx],
                        geo.grass_mask[tri_idx + 1],
                        geo.grass_mask[tri_idx + 2],
                        u,
                        v,
                        w,
                    );
                    let is_masked = mask.r < 0.9999;
                    let force_grass_on = mask.g >= 0.9999;

                    // Texture grass check
                    let texture_id = get_texture_id(col0, col1);
                    let on_grass_tex = if force_grass_on {
                        true
                    } else if (1..=6).contains(&texture_id) {
                        tex_has_grass[(texture_id - 1) as usize]
                    } else {
                        false
                    };

                    if on_grass_tex && !on_ledge && !is_masked {
                        // Compute billboard basis from triangle normal
                        let edge1 = b - a;
                        let edge2 = c - a;
                        let normal = edge1.cross(edge2).normalized();

                        let right = Vector3::FORWARD.cross(normal).normalized();
                        let forward = normal.cross(Vector3::RIGHT).normalized();
                        let basis = Basis::from_cols(right, forward, -normal);

                        mm.set_instance_transform(*index as i32, Transform3D::new(basis, p));

                        // Set custom data: RGB = grass color (from terrain ground color),
                        // A = sprite ID encoding (0.0 = tex1, 0.2 = tex2, ... 1.0 = tex6)
                        let alpha = match texture_id {
                            6 => 1.0,
                            5 => 0.8,
                            4 => 0.6,
                            3 => 0.4,
                            2 => 0.2,
                            _ => 0.0, // base grass
                        };

                        // Set instance_color to WHITE so shader multiplication produces correct brightness.
                        // The shader computes: instance_color * grass_base_color
                        // Using WHITE (1.0) means: 1.0 * grass_base_color = grass_base_color (unchanged)
                        // Alpha channel encodes the texture slot ID for sprite selection.
                        let instance_color = Color::from_rgba(1.0, 1.0, 1.0, alpha);
                        mm.set_instance_custom_data(*index as i32, instance_color);
                    } else {
                        mm.set_instance_transform(*index as i32, hidden_transform);
                    }

                    *index += 1;
                } else {
                    pt_idx += 1;
                }
            }

            tri_idx += 3;
        }
    }
}

/// Interpolate three colors using barycentric weights.
/// GDScript order: colors_0[i]*u + colors_0[i+1]*v + colors_0[i+2]*(1-u-v)
/// So: a*u + b*v + c*w where w = 1-u-v
fn interpolate_color(a: Color, b: Color, c: Color, u: f32, v: f32, w: f32) -> Color {
    Color::from_rgba(
        a.r * u + b.r * v + c.r * w,
        a.g * u + b.g * v + c.g * w,
        a.b * u + b.b * v + c.b * w,
        a.a * u + b.a * v + c.a * w,
    )
}

/// Map vertex color pair to 1-based texture ID (1-16).
/// Matches Yugen's _get_texture_id() encoding.
fn get_texture_id(col0: Color, col1: Color) -> i32 {
    let c0 = if col0.r > 0.9999 {
        0
    } else if col0.g > 0.9999 {
        1
    } else if col0.b > 0.9999 {
        2
    } else if col0.a > 0.9999 {
        3
    } else {
        0
    };

    let c1 = if col1.r > 0.9999 {
        0
    } else if col1.g > 0.9999 {
        1
    } else if col1.b > 0.9999 {
        2
    } else if col1.a > 0.9999 {
        3
    } else {
        0
    };

    c0 * 4 + c1 + 1 // 1-based to match Yugen
}

/// Simple random float [0, 1) using Godot's random.
fn rand_f32() -> f32 {
    // Use a simple hash-based random since we don't need crypto quality
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEED: AtomicU32 = AtomicU32::new(12345);
    let mut s = SEED.load(Ordering::Relaxed);
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    SEED.store(s, Ordering::Relaxed);
    (s as f32) / (u32::MAX as f32)
}
