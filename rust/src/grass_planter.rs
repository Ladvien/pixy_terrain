// Pixy Terrain — Grass planting system
//
// Original planting logic ported from Yugen's terrain toolkit:
//   https://github.com/Yukitty/Yugens-Terrain-Authoring-Toolkit
//
// Shader integration adapted from Dylearn's 3D Pixel Art Grass Demo:
//   https://github.com/DylearnDev/Dylearn-3D-Pixel-Art-Grass-Demo
use std::collections::HashMap;

use godot::classes::{ArrayMesh, Engine, Image, Mesh, MultiMesh, MultiMeshInstance3D, ShaderMaterial};
use godot::obj::InstanceId;
use godot::prelude::*;

use crate::marching_squares::{get_dominant_color, CellGeometry};
use crate::shared_params::SharedTerrainParams;

/// Cached grass configuration snapshot.
/// Passed from terrain → chunk → grass planter at init to break borrow cycles.
#[derive(Clone)]
pub struct GrassConfig {
    pub shared: SharedTerrainParams,
    pub subdivisions: i32,
    pub grass_size: Vector2,
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
            shared: SharedTerrainParams::default(),
            subdivisions: 3,
            grass_size: Vector2::new(1.0, 1.0),
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

/// Build a 3-quad cross mesh (star pattern at 0°, 60°, 120° around Y).
/// Each quad spans [-half_w, half_w] in X and [0, height] in Y.
/// UVs are standard (0,0)–(1,1) per quad.
pub fn build_cross_mesh(size: Vector2) -> Gd<ArrayMesh> {
    let half_w = size.x * 0.5;
    let height = size.y;
    let angles: [f32; 3] = [0.0, std::f32::consts::FRAC_PI_3, std::f32::consts::FRAC_PI_3 * 2.0];

    let mut verts = PackedVector3Array::new();
    let mut uvs = PackedVector2Array::new();
    let mut normals = PackedVector3Array::new();
    let mut indices = PackedInt32Array::new();

    for (i, &angle) in angles.iter().enumerate() {
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        // Quad corners: bottom-left, bottom-right, top-right, top-left
        let bl = Vector3::new(-half_w * cos_a, 0.0, -half_w * sin_a);
        let br = Vector3::new(half_w * cos_a, 0.0, half_w * sin_a);
        let tr = Vector3::new(half_w * cos_a, height, half_w * sin_a);
        let tl = Vector3::new(-half_w * cos_a, height, -half_w * sin_a);

        // Normal perpendicular to the quad plane
        let normal = Vector3::new(-sin_a, 0.0, cos_a);

        let base = (i as i32) * 4;
        verts.push(bl);
        verts.push(br);
        verts.push(tr);
        verts.push(tl);
        normals.push(normal);
        normals.push(normal);
        normals.push(normal);
        normals.push(normal);
        uvs.push(Vector2::new(0.0, 1.0));
        uvs.push(Vector2::new(1.0, 1.0));
        uvs.push(Vector2::new(1.0, 0.0));
        uvs.push(Vector2::new(0.0, 0.0));
        // Two triangles: bl-br-tr, bl-tr-tl
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 3);
    }

    let mut arrays = VarArray::new();
    arrays.resize(
        godot::classes::rendering_server::ArrayType::MAX.ord() as usize,
        &Variant::nil(),
    );
    arrays.set(
        godot::classes::rendering_server::ArrayType::VERTEX.ord() as usize,
        &verts.to_variant(),
    );
    arrays.set(
        godot::classes::rendering_server::ArrayType::NORMAL.ord() as usize,
        &normals.to_variant(),
    );
    arrays.set(
        godot::classes::rendering_server::ArrayType::TEX_UV.ord() as usize,
        &uvs.to_variant(),
    );
    arrays.set(
        godot::classes::rendering_server::ArrayType::INDEX.ord() as usize,
        &indices.to_variant(),
    );

    let mut mesh = ArrayMesh::new_gd();
    mesh.add_surface_from_arrays(
        godot::classes::mesh::PrimitiveType::TRIANGLES,
        &arrays,
    );
    mesh
}

/// Grass planter node — places MultiMesh grass blades on floor triangles.
#[derive(GodotClass)]
#[class(base=MultiMeshInstance3D, init, tool)]
pub struct PixyGrassPlanter {
    base: Base<MultiMeshInstance3D>,
    grass_config: Option<GrassConfig>,
    chunk_instance_id: Option<InstanceId>,
}

impl PixyGrassPlanter {
    /// Initialize the planter with a cached config. Called by chunk during initialization.
    pub fn setup_with_config(
        &mut self,
        chunk_id: InstanceId,
        config: GrassConfig,
        _force_rebuild: bool,
    ) {
        self.chunk_instance_id = Some(chunk_id);
        self.grass_config = Some(config);
        self.setup();
    }

    /// Create/reset the MultiMesh with the correct instance count.
    fn setup(&mut self) {
        let Some(ref config) = self.grass_config else {
            godot_error!("GrassPlanter::setup() called without config");
            return;
        };

        let dim_x = config.shared.dimensions.x - 1;
        let dim_z = config.shared.dimensions.z - 1;
        let subs = config.subdivisions;
        let instance_count = (dim_x * dim_z * subs * subs) as i32;
        let grass_material = config.grass_material.clone();

        godot_print!(
            "GrassPlanter::setup() — instances={}, has_material={}, has_grass_mesh={}, has_quad_mesh={}",
            instance_count,
            grass_material.is_some(),
            config.grass_mesh.is_some(),
            config.grass_quad_mesh.is_some()
        );

        let mut mm = MultiMesh::new_gd();
        mm.set_transform_format(godot::classes::multi_mesh::TransformFormat::TRANSFORM_3D);
        mm.set_use_custom_data(true);
        mm.set_instance_count(instance_count);

        // Use custom grass mesh if set, otherwise use the shared QuadMesh
        let in_editor = Engine::singleton().is_editor_hint();
        if let Some(ref mesh) = config.grass_mesh {
            godot_print!("GrassPlanter [editor={}]: Using grass_mesh override", in_editor);
            mm.set_mesh(mesh);
        } else if let Some(ref quad) = config.grass_quad_mesh {
            godot_print!("GrassPlanter [editor={}]: Using grass_quad_mesh (cross-mesh)", in_editor);
            mm.set_mesh(quad);
        } else {
            godot_warn!("GrassPlanter [editor={}]: No mesh set — using build_cross_mesh fallback", in_editor);
            let cross = build_cross_mesh(config.grass_size);
            mm.set_mesh(&cross);
        }

        self.base_mut().set_multimesh(&mm);
        self.base_mut().set_cast_shadows_setting(
            godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
        );

        // Apply grass ShaderMaterial for alpha-cutout rendering
        if let Some(mat) = grass_material {
            godot_print!("GrassPlanter: Applying material_override with ShaderMaterial");
            self.base_mut()
                .set_material_override(&mat.upcast::<godot::classes::Material>());
        } else {
            godot_warn!("GrassPlanter: No grass material — quads will render white!");
        }
    }

    /// Convert a color_0 / color_1 pair to a texture slot ID (1–16).
    /// Matches Yugen's _get_texture_id().
    fn get_texture_id(color_0: Color, color_1: Color) -> i32 {
        let c0 = if color_0.r > 0.9999 {
            0
        } else if color_0.g > 0.9999 {
            1
        } else if color_0.b > 0.9999 {
            2
        } else if color_0.a > 0.9999 {
            3
        } else {
            0
        };

        let c1 = if color_1.r > 0.9999 {
            0
        } else if color_1.g > 0.9999 {
            1
        } else if color_1.b > 0.9999 {
            2
        } else if color_1.a > 0.9999 {
            3
        } else {
            0
        };

        c0 * 4 + c1 + 1
    }

    /// Sample a color from the ground texture image at a world XZ position.
    /// Returns the ground_color fallback if no image is available.
    fn sample_terrain_color(
        config: &GrassConfig,
        texture_id: i32,
        world_x: f32,
        world_z: f32,
    ) -> Color {
        // texture_id 1-6 maps to ground_images[0-5]
        let img_index = (texture_id - 1).clamp(0, 5) as usize;

        let tex_scale = if img_index < config.texture_scales.len() {
            config.texture_scales[img_index]
        } else {
            1.0
        };

        if let Some(ref img) = config.ground_images[img_index] {
            let total_x = config.shared.dimensions.x as f32 * config.shared.cell_size.x;
            let total_z = config.shared.dimensions.z as f32 * config.shared.cell_size.y;

            let mut uv_x = (world_x / total_x).clamp(0.0, 1.0) * tex_scale;
            let mut uv_y = (world_z / total_z).clamp(0.0, 1.0) * tex_scale;

            uv_x = (uv_x % 1.0).abs();
            uv_y = (uv_y % 1.0).abs();

            let w = img.get_width();
            let h = img.get_height();
            let px = ((uv_x * (w - 1) as f32) as i32).clamp(0, w - 1);
            let py = ((uv_y * (h - 1) as f32) as i32).clamp(0, h - 1);

            img.get_pixel(px, py)
        } else {
            // No image — use the ground color fallback
            config.ground_colors[img_index]
        }
    }

    /// Place grass instances for a single cell using its prebuilt geometry.
    fn generate_grass_on_cell(&mut self, cell_coords: Vector2i, geo: &CellGeometry) {
        let Some(ref config) = self.grass_config else {
            return;
        };

        let Some(mm) = self.base().get_multimesh() else {
            return;
        };
        let mut mm = mm.clone();

        let subs = config.subdivisions;
        let count = (subs * subs) as i32;
        let dim_x = config.shared.dimensions.x - 1;
        let mut index = (cell_coords.y * dim_x + cell_coords.x) * count;
        let end_index = index + count;

        // Generate jittered sample points (XZ world positions)
        let mut points: Vec<Vector2> = Vec::with_capacity(count as usize);
        for z in 0..subs {
            for x in 0..subs {
                let jx = (cell_coords.x as f32 + (x as f32 + rand_f32()) / subs as f32)
                    * config.shared.cell_size.x;
                let jz = (cell_coords.y as f32 + (z as f32 + rand_f32()) / subs as f32)
                    * config.shared.cell_size.y;
                points.push(Vector2::new(jx, jz));
            }
        }

        let zero_basis = Basis::from_scale(Vector3::ZERO);
        let hide_pos = Vector3::new(9999.0, 9999.0, 9999.0);
        let hide_xform = Transform3D::new(zero_basis, hide_pos);

        // Iterate floor triangles
        let vert_count = geo.verts.len();
        let mut tri = 0;
        while tri + 2 < vert_count {
            if !geo.is_floor[tri] {
                tri += 3;
                continue;
            }

            let a = geo.verts[tri];
            let b = geo.verts[tri + 1];
            let c = geo.verts[tri + 2];

            // Precompute barycentric denominator (2D XZ projection)
            let v0 = Vector2::new(c.x - a.x, c.z - a.z);
            let v1 = Vector2::new(b.x - a.x, b.z - a.z);
            let dot00 = v0.dot(v0);
            let dot01 = v0.dot(v1);
            let dot11 = v1.dot(v1);
            let denom = dot00 * dot11 - dot01 * dot01;

            if denom.abs() < 1e-8 {
                tri += 3;
                continue; // Degenerate triangle
            }
            let inv_denom = 1.0 / denom;

            let mut pi = 0;
            while pi < points.len() {
                let pt = points[pi];
                let v2 = Vector2::new(pt.x - a.x, pt.y - a.z);
                let dot02 = v0.dot(v2);
                let dot12 = v1.dot(v2);

                let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
                if u < 0.0 {
                    pi += 1;
                    continue;
                }

                let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;
                if v < 0.0 {
                    pi += 1;
                    continue;
                }

                if u + v > 1.0 {
                    pi += 1;
                    continue;
                }

                // Point is inside triangle — remove it so it isn't claimed twice
                points.swap_remove(pi);
                let w = 1.0 - u - v;

                // Interpolate 3D position
                let p = a * w + b * u + c * v;

                // Ledge/ridge avoidance via interpolated UV
                let uv = geo.uvs[tri] * u + geo.uvs[tri + 1] * v + geo.uvs[tri + 2] * w;
                let on_ledge = uv.x > 1.0 - config.shared.ledge_threshold
                    || uv.y > 1.0 - config.shared.ridge_threshold;

                // Interpolate vertex colors → dominant channel
                let c0_interp = get_dominant_color(lerp_color3(
                    geo.colors_0[tri],
                    geo.colors_0[tri + 1],
                    geo.colors_0[tri + 2],
                    u,
                    v,
                    w,
                ));
                let c1_interp = get_dominant_color(lerp_color3(
                    geo.colors_1[tri],
                    geo.colors_1[tri + 1],
                    geo.colors_1[tri + 2],
                    u,
                    v,
                    w,
                ));

                // Grass mask: red < 1 means masked OFF, green >= 1 means force ON
                let mask = lerp_color3(
                    geo.grass_mask[tri],
                    geo.grass_mask[tri + 1],
                    geo.grass_mask[tri + 2],
                    u,
                    v,
                    w,
                );
                let is_masked = mask.r < 0.9999;
                let force_grass_on = mask.g >= 0.9999;

                let texture_id = Self::get_texture_id(c0_interp, c1_interp);

                let on_grass_tex = if force_grass_on {
                    true
                } else if texture_id == 1 {
                    true // Base grass always has grass
                } else if (2..=6).contains(&texture_id) {
                    let ti = (texture_id - 2) as usize;
                    ti < config.tex_has_grass.len() && config.tex_has_grass[ti]
                } else {
                    false
                };

                if index >= mm.get_instance_count() {
                    return;
                }

                if on_grass_tex && !on_ledge && !is_masked {
                    // Compute grass blade orientation from triangle normal
                    let edge1 = b - a;
                    let edge2 = c - a;
                    let normal = edge1.cross(edge2).normalized();

                    let right = Vector3::FORWARD.cross(normal).normalized();
                    let forward = normal.cross(right).normalized();
                    let instance_basis = Basis::from_cols(right, normal, forward);

                    mm.set_instance_transform(index, Transform3D::new(instance_basis, p));

                    // Sample terrain color for custom data
                    let mut instance_color =
                        Self::sample_terrain_color(config, texture_id, p.x, p.z);

                    // Encode texture_id in alpha for the grass shader
                    instance_color.a = match texture_id {
                        6 => 1.0,
                        5 => 0.8,
                        4 => 0.6,
                        3 => 0.4,
                        2 => 0.2,
                        _ => 0.0,
                    };

                    mm.set_instance_custom_data(index, instance_color);
                } else {
                    mm.set_instance_transform(index, hide_xform);
                }

                index += 1;
            }

            tri += 3;
        }

        // Hide remaining unused instances
        while index < end_index {
            if index >= mm.get_instance_count() {
                return;
            }
            mm.set_instance_transform(index, hide_xform);
            index += 1;
        }
    }

    /// Regenerate grass on all cells using prebuilt geometry.  
    pub fn regenerate_all_cells_with_geometry(
        &mut self,
        cell_geometry: &HashMap<[i32; 2], CellGeometry>,
    ) {
        if self.grass_config.is_none() {
            return;
        }

        // Ensure MultiMesh exists
        if self.base().get_multimesh().is_none() {
            self.setup();
        }

        let dim_x = self.grass_config.as_ref().unwrap().shared.dimensions.x - 1;
        let dim_z = self.grass_config.as_ref().unwrap().shared.dimensions.z - 1;

        for z in 0..dim_z {
            for x in 0..dim_x {
                let key = [x, z];
                if let Some(geo) = cell_geometry.get(&key) {
                    self.generate_grass_on_cell(Vector2i::new(x, z), geo);
                }
            }
        }
    }
}

/// Barycentric interpolation of three colors.
#[inline]
fn lerp_color3(a: Color, b: Color, c: Color, u: f32, v: f32, w: f32) -> Color {
    Color::from_rgba(
        a.r * w + b.r * u + c.r * v,
        a.g * w + b.g * u + c.g * v,
        a.b * w + b.b * u + c.b * v,
        a.a * w + b.a * u + c.a * v,
    )
}

/// Simple random float [0, 1) using Godot's built-in RNG.
#[inline]
fn rand_f32() -> f32 {
    godot::global::randf() as f32
}
