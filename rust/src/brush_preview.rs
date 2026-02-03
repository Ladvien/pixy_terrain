//! Brush preview visualization for terrain painting.
//!
//! This module generates a quad mesh that projects the brush footprint
//! onto the terrain surface for visual feedback.

use godot::classes::mesh::PrimitiveType;
use godot::classes::rendering_server::ArrayType;
use godot::classes::{ArrayMesh, Shader, ShaderMaterial};
use godot::prelude::*;

use crate::brush::{Brush, BrushMode, BrushPhase, BrushShape};

/// Shader source for brush preview visualization
const BRUSH_PREVIEW_SHADER: &str = r#"
shader_type spatial;
render_mode blend_add, depth_draw_never, depth_test_disabled, cull_disabled, unshaded;

uniform vec4 brush_color : source_color = vec4(0.0, 0.8, 1.0, 0.3);
uniform vec4 height_positive_color : source_color = vec4(0.0, 1.0, 0.0, 0.3);
uniform vec4 height_negative_color : source_color = vec4(1.0, 0.0, 0.0, 0.3);
uniform float height_delta = 0.0;
uniform bool show_height_preview = false;

void fragment() {
    vec4 color = brush_color;

    if (show_height_preview) {
        if (height_delta > 0.001) {
            // Raising terrain - green
            float intensity = clamp(height_delta * 0.1, 0.0, 1.0);
            color = mix(brush_color, height_positive_color, intensity);
        } else if (height_delta < -0.001) {
            // Lowering terrain - red
            float intensity = clamp(-height_delta * 0.1, 0.0, 1.0);
            color = mix(brush_color, height_negative_color, intensity);
        }
    }

    ALBEDO = color.rgb;
    ALPHA = color.a;
}
"#;

/// Shader source for the height plane visualization (transparent quad at target height)
const HEIGHT_PLANE_SHADER: &str = r#"
shader_type spatial;
render_mode blend_add, depth_draw_never, depth_test_disabled, cull_disabled, unshaded;

uniform vec4 plane_color : source_color = vec4(0.0, 1.0, 0.0, 0.25);

void fragment() {
    // Pulsing alpha for visual feedback
    float pulse = 0.5 + 0.5 * sin(TIME * 3.0);
    float alpha = plane_color.a * (0.6 + 0.4 * pulse);
    ALBEDO = plane_color.rgb;
    ALPHA = alpha;
}
"#;

/// Colors for different brush modes and phases
#[derive(Clone, Copy, Debug)]
pub struct BrushColors {
    /// Color for elevation mode brush
    pub elevation_color: [f32; 4],
    /// Color for texture mode brush
    pub texture_color: [f32; 4],
    /// Color for flatten mode brush
    pub flatten_color: [f32; 4],
    /// Color for plateau mode brush
    pub plateau_color: [f32; 4],
    /// Color for smooth mode brush
    pub smooth_color: [f32; 4],
    /// Color for positive height delta (raising terrain)
    pub height_positive: [f32; 4],
    /// Color for negative height delta (lowering terrain)
    pub height_negative: [f32; 4],
}

impl Default for BrushColors {
    fn default() -> Self {
        Self {
            elevation_color: [0.0, 0.8, 1.0, 0.3], // Cyan
            texture_color: [1.0, 0.5, 0.0, 0.3],   // Orange
            flatten_color: [0.8, 0.8, 0.0, 0.3],   // Yellow
            plateau_color: [0.8, 0.0, 0.8, 0.3],   // Purple
            smooth_color: [0.0, 0.8, 0.4, 0.3],    // Green-teal
            height_positive: [0.0, 1.0, 0.0, 0.3], // Green
            height_negative: [1.0, 0.0, 0.0, 0.3], // Red
        }
    }
}

/// Brush preview mesh generator
pub struct BrushPreview {
    /// Cached preview material
    material: Option<Gd<ShaderMaterial>>,
    /// Colors for different modes
    colors: BrushColors,
}

impl Default for BrushPreview {
    fn default() -> Self {
        Self::new()
    }
}

impl BrushPreview {
    pub fn new() -> Self {
        Self {
            material: None,
            colors: BrushColors::default(),
        }
    }

    /// Initialize the shader material
    fn ensure_material(&mut self) {
        if self.material.is_some() {
            return;
        }

        let mut shader = Shader::new_gd();
        shader.set_code(BRUSH_PREVIEW_SHADER);

        let mut material = ShaderMaterial::new_gd();
        material.set_shader(&shader);

        // Set default colors
        material.set_shader_parameter(
            "brush_color",
            &Color::from_rgba(
                self.colors.elevation_color[0],
                self.colors.elevation_color[1],
                self.colors.elevation_color[2],
                self.colors.elevation_color[3],
            )
            .to_variant(),
        );
        material.set_shader_parameter(
            "height_positive_color",
            &Color::from_rgba(
                self.colors.height_positive[0],
                self.colors.height_positive[1],
                self.colors.height_positive[2],
                self.colors.height_positive[3],
            )
            .to_variant(),
        );
        material.set_shader_parameter(
            "height_negative_color",
            &Color::from_rgba(
                self.colors.height_negative[0],
                self.colors.height_negative[1],
                self.colors.height_negative[2],
                self.colors.height_negative[3],
            )
            .to_variant(),
        );

        self.material = Some(material);
    }

    /// Update the material based on brush state
    pub fn update_material(&mut self, brush: &Brush) {
        self.ensure_material();

        let Some(ref mut material) = self.material else {
            return;
        };

        // Set color based on mode
        let color = match brush.mode {
            BrushMode::Elevation => self.colors.elevation_color,
            BrushMode::Texture => self.colors.texture_color,
            BrushMode::Flatten => self.colors.flatten_color,
            BrushMode::Plateau => self.colors.plateau_color,
            BrushMode::Smooth => self.colors.smooth_color,
        };
        material.set_shader_parameter(
            "brush_color",
            &Color::from_rgba(color[0], color[1], color[2], color[3]).to_variant(),
        );

        // Set height preview parameters
        let show_height = brush.phase == BrushPhase::AdjustingHeight
            || brush.phase == BrushPhase::AdjustingCurvature;
        material.set_shader_parameter("show_height_preview", &show_height.to_variant());
        material.set_shader_parameter("height_delta", &brush.footprint.height_delta.to_variant());
    }

    /// Generate a preview mesh for the brush footprint
    pub fn generate_mesh(&mut self, brush: &Brush) -> Option<Gd<ArrayMesh>> {
        if brush.footprint.is_empty() {
            return None;
        }

        self.ensure_material();

        let positions = brush.get_preview_positions(brush.footprint.base_y);
        if positions.is_empty() {
            return None;
        }

        // Generate a quad for each cell
        let voxel_size = brush.voxel_size;
        let half_size = voxel_size * 0.45; // Slightly smaller than full cell

        let mut vertices: Vec<Vector3> = Vec::new();
        let mut normals: Vec<Vector3> = Vec::new();
        let mut indices: Vec<i32> = Vec::new();

        for (x, y, z) in &positions {
            let base_idx = vertices.len() as i32;

            // Add quad vertices (facing up)
            vertices.push(Vector3::new(x - half_size, *y, z - half_size));
            vertices.push(Vector3::new(x + half_size, *y, z - half_size));
            vertices.push(Vector3::new(x + half_size, *y, z + half_size));
            vertices.push(Vector3::new(x - half_size, *y, z + half_size));

            // Normals pointing up
            for _ in 0..4 {
                normals.push(Vector3::new(0.0, 1.0, 0.0));
            }

            // Two triangles per quad
            indices.push(base_idx);
            indices.push(base_idx + 1);
            indices.push(base_idx + 2);

            indices.push(base_idx);
            indices.push(base_idx + 2);
            indices.push(base_idx + 3);
        }

        // Create the mesh
        let packed_vertices = PackedVector3Array::from(&vertices[..]);
        let packed_normals = PackedVector3Array::from(&normals[..]);
        let packed_indices = PackedInt32Array::from(&indices[..]);

        let mut mesh = ArrayMesh::new_gd();
        let num_arrays = ArrayType::MAX.ord() as usize;
        let mut arrays: Array<Variant> = Array::new();

        for i in 0..num_arrays {
            if i == ArrayType::VERTEX.ord() as usize {
                arrays.push(&packed_vertices.to_variant());
            } else if i == ArrayType::NORMAL.ord() as usize {
                arrays.push(&packed_normals.to_variant());
            } else if i == ArrayType::INDEX.ord() as usize {
                arrays.push(&packed_indices.to_variant());
            } else {
                arrays.push(&Variant::nil());
            }
        }

        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);

        Some(mesh)
    }

    /// Get the preview material (for applying to MeshInstance3D)
    pub fn get_material(&mut self) -> Option<Gd<ShaderMaterial>> {
        self.ensure_material();
        self.material.clone()
    }

    /// Set custom colors
    pub fn set_colors(&mut self, colors: BrushColors) {
        self.colors = colors;
        // Clear material to force recreation with new colors
        self.material = None;
    }

    /// Create a shader material for the height plane indicator.
    /// Green when raising terrain, red when lowering.
    pub fn create_height_plane_material(raising: bool) -> Gd<ShaderMaterial> {
        let mut shader = Shader::new_gd();
        shader.set_code(HEIGHT_PLANE_SHADER);

        let mut material = ShaderMaterial::new_gd();
        material.set_shader(&shader);

        let color = if raising {
            Color::from_rgba(0.0, 1.0, 0.0, 0.25) // Green
        } else {
            Color::from_rgba(1.0, 0.0, 0.0, 0.25) // Red
        };
        material.set_shader_parameter("plane_color", &color.to_variant());

        material
    }

    /// Generate a single quad mesh at the given Y height covering the given XZ bounds.
    pub fn generate_height_plane_mesh(
        min_x: f32,
        max_x: f32,
        min_z: f32,
        max_z: f32,
        y: f32,
    ) -> Gd<ArrayMesh> {
        let vertices = PackedVector3Array::from(
            &[
                Vector3::new(min_x, y, min_z),
                Vector3::new(max_x, y, min_z),
                Vector3::new(max_x, y, max_z),
                Vector3::new(min_x, y, max_z),
            ][..],
        );

        let normals = PackedVector3Array::from(
            &[
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
            ][..],
        );

        let indices = PackedInt32Array::from(&[0, 1, 2, 0, 2, 3][..]);

        let mut mesh = ArrayMesh::new_gd();
        let num_arrays = ArrayType::MAX.ord() as usize;
        let mut arrays: Array<Variant> = Array::new();

        for i in 0..num_arrays {
            if i == ArrayType::VERTEX.ord() as usize {
                arrays.push(&vertices.to_variant());
            } else if i == ArrayType::NORMAL.ord() as usize {
                arrays.push(&normals.to_variant());
            } else if i == ArrayType::INDEX.ord() as usize {
                arrays.push(&indices.to_variant());
            } else {
                arrays.push(&Variant::nil());
            }
        }

        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        mesh
    }
    /// Generate a tessellated quad mesh with curvature applied to vertex heights.
    ///
    /// curvature > 0: dome (center higher than edges)
    /// curvature < 0: bowl (center lower than edges)
    /// curvature == 0: flat plane
    pub fn generate_curved_height_plane_mesh(
        min_x: f32,
        max_x: f32,
        min_z: f32,
        max_z: f32,
        base_y: f32,
        height_delta: f32,
        curvature: f32,
        brush_size: f32,
        brush_shape: BrushShape,
    ) -> Gd<ArrayMesh> {
        let subdivisions: usize = 16;
        let cx = (min_x + max_x) * 0.5;
        let cz = (min_z + max_z) * 0.5;
        let radius = brush_size;

        let num_verts = (subdivisions + 1) * (subdivisions + 1);
        let mut vertices = Vec::with_capacity(num_verts);
        let mut normals = Vec::with_capacity(num_verts);
        let mut indices = Vec::with_capacity(subdivisions * subdivisions * 6);

        let dx = (max_x - min_x) / subdivisions as f32;
        let dz = (max_z - min_z) / subdivisions as f32;

        // Generate vertices
        for iz in 0..=subdivisions {
            for ix in 0..=subdivisions {
                let x = min_x + ix as f32 * dx;
                let z = min_z + iz as f32 * dz;

                let dist = match brush_shape {
                    BrushShape::Round => ((x - cx).powi(2) + (z - cz).powi(2)).sqrt(),
                    BrushShape::Square => (x - cx).abs().max((z - cz).abs()),
                };
                let ratio = (dist / radius).clamp(0.0, 1.0);

                let curvature_factor = if curvature.abs() > 0.001 {
                    let t = Brush::smootherstep(ratio);
                    if curvature > 0.0 {
                        1.0 - curvature * t
                    } else {
                        1.0 + curvature * (1.0 - t)
                    }
                } else {
                    1.0
                };

                let y = base_y + height_delta * curvature_factor;
                vertices.push(Vector3::new(x, y, z));
                // Normals will be computed per-face below
                normals.push(Vector3::new(0.0, 1.0, 0.0));
            }
        }

        // Generate indices
        let row = (subdivisions + 1) as i32;
        for iz in 0..subdivisions {
            for ix in 0..subdivisions {
                let i = (iz * (subdivisions + 1) + ix) as i32;
                // Triangle 1
                indices.push(i);
                indices.push(i + row);
                indices.push(i + 1);
                // Triangle 2
                indices.push(i + 1);
                indices.push(i + row);
                indices.push(i + row + 1);
            }
        }

        // Compute per-vertex normals from adjacent faces
        let mut normal_accum = vec![Vector3::ZERO; vertices.len()];
        for tri in indices.chunks(3) {
            let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let v0 = vertices[i0];
            let v1 = vertices[i1];
            let v2 = vertices[i2];
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            let face_normal = edge1.cross(edge2);
            normal_accum[i0] += face_normal;
            normal_accum[i1] += face_normal;
            normal_accum[i2] += face_normal;
        }
        for (i, n) in normal_accum.iter().enumerate() {
            let len = n.length();
            normals[i] = if len > 0.0001 { *n / len } else { Vector3::UP };
        }

        // Build mesh
        let packed_vertices = PackedVector3Array::from(&vertices[..]);
        let packed_normals = PackedVector3Array::from(&normals[..]);
        let packed_indices = PackedInt32Array::from(&indices[..]);

        let mut mesh = ArrayMesh::new_gd();
        let num_arrays = ArrayType::MAX.ord() as usize;
        let mut arrays: Array<Variant> = Array::new();

        for i in 0..num_arrays {
            if i == ArrayType::VERTEX.ord() as usize {
                arrays.push(&packed_vertices.to_variant());
            } else if i == ArrayType::NORMAL.ord() as usize {
                arrays.push(&packed_normals.to_variant());
            } else if i == ArrayType::INDEX.ord() as usize {
                arrays.push(&packed_indices.to_variant());
            } else {
                arrays.push(&Variant::nil());
            }
        }

        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brush_colors_default() {
        let colors = BrushColors::default();
        assert!(colors.elevation_color[3] > 0.0); // Has alpha
        assert!(colors.texture_color[3] > 0.0);
    }

    #[test]
    fn test_brush_preview_new() {
        let preview = BrushPreview::new();
        assert!(preview.material.is_none()); // Lazy initialization
    }
}
