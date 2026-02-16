// Pixy Terrain — Flower planting system
//
// Scatter-plants 3D flower meshes on terrain surfaces using the same
// barycentric surface sampling as the grass planter. Each flower instance
// gets random Y rotation and slight scale variation.
//
// Flower shader adapted from Eduardo Schildt's 3D Pixel Art Vegetation (MIT License).

use std::collections::HashMap;

use godot::classes::{Mesh, MultiMesh, MultiMeshInstance3D, Shader, ShaderMaterial, Texture2D};
use godot::obj::InstanceId;
use godot::prelude::*;

use crate::marching_squares::CellGeometry;
use crate::shared_params::SharedTerrainParams;

/// Cached flower configuration snapshot.
/// Passed from terrain -> chunk -> flower planter at init.
#[derive(Clone)]
pub struct FlowerConfig {
    pub shared: SharedTerrainParams,
    /// Subdivisions per cell (flowers per cell = subdivisions^2)
    pub subdivisions: i32,
    /// The 3D flower mesh (from flower.glb)
    pub flower_mesh: Option<Gd<Mesh>>,
    /// The flower sprite texture
    pub flower_texture: Option<Gd<Texture2D>>,
    /// Tint color for flowers
    pub flower_color: Color,
    /// Base scale for flower instances
    pub flower_size: f32,
    /// Wind bendiness
    pub flower_bendiness: f32,
    /// Cel-shading light quantization steps
    pub light_steps: f32,
    /// Whether flowers are enabled
    pub enabled: bool,
}

impl Default for FlowerConfig {
    fn default() -> Self {
        Self {
            shared: SharedTerrainParams::default(),
            subdivisions: 2,
            flower_mesh: None,
            flower_texture: None,
            flower_color: Color::WHITE,
            flower_size: 1.0,
            flower_bendiness: 1.0,
            light_steps: 3.0,
            enabled: false,
        }
    }
}

#[derive(GodotClass)]
#[class(base=MultiMeshInstance3D, init, tool)]
pub struct PixyFlowerPlanter {
    base: Base<MultiMeshInstance3D>,
    flower_config: Option<FlowerConfig>,
    chunk_instance_id: Option<InstanceId>,
}

#[godot_api]
impl PixyFlowerPlanter {
    pub fn setup_with_config(
        &mut self,
        chunk_id: InstanceId,
        config: FlowerConfig,
        _force_rebuild: bool,
    ) {
        self.chunk_instance_id = Some(chunk_id);
        self.flower_config = Some(config);
        self.setup();
    }

    /// Create/reset the MultiMesh with the correct instance count.
    fn setup(&mut self) {
        // Clone config values to avoid borrow checker conflicts
        let config = match self.flower_config.clone() {
            Some(c) => c,
            None => {
                godot_error!("FlowerPlanter::setup() called without config");
                return;
            }
        };

        if !config.enabled {
            return;
        }

        let dim_x = config.shared.dimensions.x - 1;
        let dim_z = config.shared.dimensions.z - 1;
        let subs = config.subdivisions;
        let instance_count = (dim_x * dim_z * subs * subs) as i32;

        let mut mm = MultiMesh::new_gd();
        mm.set_transform_format(godot::classes::multi_mesh::TransformFormat::TRANSFORM_3D);
        mm.set_use_custom_data(false);
        mm.set_instance_count(instance_count);

        // Use the 3D flower mesh
        if let Some(ref mesh) = config.flower_mesh {
            mm.set_mesh(mesh);
        } else {
            godot_warn!("FlowerPlanter: No flower mesh — flowers won't render!");
            return;
        }

        self.base_mut().set_multimesh(&mm);
        self.base_mut().set_cast_shadows_setting(
            godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
        );

        // Create and apply the flower ShaderMaterial
        let material = Self::create_flower_material(&config);
        self.base_mut()
            .set_material_override(&material.upcast::<godot::classes::Material>());
    }

    fn create_flower_material(config: &FlowerConfig) -> Gd<ShaderMaterial> {
        let shader: Gd<Shader> =
            load("res://addons/pixy_terrain/resources/shaders/mst_flower.gdshader");
        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);

        if let Some(ref tex) = config.flower_texture {
            mat.set_shader_parameter("flower_texture", &tex.to_variant());
        }

        mat.set_shader_parameter("flower_bendiness", &config.flower_bendiness.to_variant());
        mat.set_shader_parameter("light_steps", &config.light_steps.to_variant());

        mat
    }

    /// Place flowers on a single terrain cell.
    fn generate_flowers_on_cell(&mut self, cell_coords: Vector2i, geo: &CellGeometry) {
        let Some(ref config) = self.flower_config else {
            return;
        };
        if !config.enabled {
            return;
        }

        let Some(mm_gd) = self.base().get_multimesh() else {
            return;
        };
        let mut mm = mm_gd;

        let subs = config.subdivisions;
        let dim_x = config.shared.dimensions.x - 1;
        let base_index = ((cell_coords.y * dim_x + cell_coords.x) * subs * subs) as i32;

        // Generate jittered candidate points within the cell
        let count = (subs * subs) as usize;
        let mut points: Vec<Vector2> = Vec::with_capacity(count);
        for z in 0..subs {
            for x in 0..subs {
                let jx = (cell_coords.x as f32 + (x as f32 + rand_f32()) / subs as f32)
                    * config.shared.cell_size.x;
                let jz = (cell_coords.y as f32 + (z as f32 + rand_f32()) / subs as f32)
                    * config.shared.cell_size.y;
                points.push(Vector2::new(jx, jz));
            }
        }

        // Hide all instances in this cell first (zero scale)
        let zero_xform = Transform3D::new(Basis::IDENTITY.scaled(Vector3::ZERO), Vector3::ZERO);
        for i in 0..count {
            let idx = base_index + i as i32;
            if idx < mm.get_instance_count() {
                mm.set_instance_transform(idx, zero_xform);
            }
        }

        let mut placed = 0i32;
        let flower_size = config.flower_size;

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

            // Barycentric denominator (2D XZ projection)
            let v0 = Vector2::new(c.x - a.x, c.z - a.z);
            let v1 = Vector2::new(b.x - a.x, b.z - a.z);
            let dot00 = v0.dot(v0);
            let dot01 = v0.dot(v1);
            let dot11 = v1.dot(v1);
            let denom = dot00 * dot11 - dot01 * dot01;

            if denom.abs() < 1e-8 {
                tri += 3;
                continue;
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

                // Point is inside triangle — claim it
                points.swap_remove(pi);

                let w = 1.0 - u - v;
                let p = a * w + b * u + c * v;

                // Ledge avoidance via interpolated UV
                let uv = geo.uvs[tri] * u + geo.uvs[tri + 1] * v + geo.uvs[tri + 2] * w;
                let on_ledge = uv.x > 1.0 - config.shared.ledge_threshold
                    || uv.y > 1.0 - config.shared.ridge_threshold;

                if on_ledge {
                    continue;
                }

                // Respect grass mask: flowers only where grass is allowed
                let mask = lerp_color3(
                    geo.grass_mask[tri],
                    geo.grass_mask[tri + 1],
                    geo.grass_mask[tri + 2],
                    u,
                    v,
                    w,
                );
                let is_masked = mask.r < 0.9999;
                if is_masked {
                    continue;
                }

                let index = base_index + placed;
                if index >= mm.get_instance_count() {
                    break;
                }

                // Random Y rotation for variety
                let y_angle = rand_f32() * std::f32::consts::TAU;
                // Slight scale variation (0.8 - 1.2x)
                let scale_var = 0.8 + rand_f32() * 0.4;
                let final_scale = flower_size * scale_var;

                let basis = Basis::from_axis_angle(Vector3::UP, y_angle)
                    .scaled(Vector3::new(final_scale, final_scale, final_scale));

                mm.set_instance_transform(index, Transform3D::new(basis, p));
                placed += 1;
            }

            tri += 3;
        }
    }

    /// Regenerate flowers for all cells using stored geometry.
    pub fn regenerate_all_cells_with_geometry(
        &mut self,
        cell_geometry: &HashMap<[i32; 2], CellGeometry>,
    ) {
        if self.flower_config.is_none() {
            return;
        }

        if !self.flower_config.as_ref().unwrap().enabled {
            return;
        }

        // Ensure MultiMesh exists
        if self.base().get_multimesh().is_none() {
            self.setup();
        }

        let dim_x = self.flower_config.as_ref().unwrap().shared.dimensions.x - 1;
        let dim_z = self.flower_config.as_ref().unwrap().shared.dimensions.z - 1;

        for z in 0..dim_z {
            for x in 0..dim_x {
                let key = [x, z];
                if let Some(geo) = cell_geometry.get(&key) {
                    self.generate_flowers_on_cell(Vector2i::new(x, z), geo);
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
