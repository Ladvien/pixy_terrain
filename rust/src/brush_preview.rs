//! Brush preview visualization for terrain painting.
//!
//! This module generates a quad mesh that projects the brush footprint
//! onto the terrain surface for visual feedback.

use godot::classes::mesh::PrimitiveType;
use godot::classes::rendering_server::ArrayType;
use godot::classes::{ArrayMesh, Shader, ShaderMaterial};
use godot::prelude::*;

use crate::brush::{Brush, BrushMode, BrushPhase};

/// Shader source for brush preview visualization
const BRUSH_PREVIEW_SHADER: &str = r#"
shader_type spatial;
render_mode blend_add, depth_draw_never, cull_disabled, unshaded;

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

/// Colors for different brush modes and phases
#[derive(Clone, Copy, Debug)]
pub struct BrushColors {
    /// Color for geometry mode brush
    pub geometry_color: [f32; 4],
    /// Color for texture mode brush
    pub texture_color: [f32; 4],
    /// Color for positive height delta (raising terrain)
    pub height_positive: [f32; 4],
    /// Color for negative height delta (lowering terrain)
    pub height_negative: [f32; 4],
}

impl Default for BrushColors {
    fn default() -> Self {
        Self {
            geometry_color: [0.0, 0.8, 1.0, 0.3],   // Cyan
            texture_color: [1.0, 0.5, 0.0, 0.3],    // Orange
            height_positive: [0.0, 1.0, 0.0, 0.3],  // Green
            height_negative: [1.0, 0.0, 0.0, 0.3],  // Red
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
                self.colors.geometry_color[0],
                self.colors.geometry_color[1],
                self.colors.geometry_color[2],
                self.colors.geometry_color[3],
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
            BrushMode::Geometry => self.colors.geometry_color,
            BrushMode::Texture => self.colors.texture_color,
        };
        material.set_shader_parameter(
            "brush_color",
            &Color::from_rgba(color[0], color[1], color[2], color[3]).to_variant(),
        );

        // Set height preview parameters
        let show_height = brush.phase == BrushPhase::AdjustingHeight;
        material.set_shader_parameter("show_height_preview", &show_height.to_variant());
        material.set_shader_parameter("height_delta", &brush.footprint.height_delta.to_variant());
    }

    /// Generate a preview mesh for the brush footprint
    pub fn generate_mesh(&mut self, brush: &Brush) -> Option<Gd<ArrayMesh>> {
        if brush.footprint.is_empty() {
            return None;
        }

        self.ensure_material();

        let positions = brush.get_preview_positions(brush.footprint.base_y + 0.1);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brush_colors_default() {
        let colors = BrushColors::default();
        assert!(colors.geometry_color[3] > 0.0); // Has alpha
        assert!(colors.texture_color[3] > 0.0);
    }

    #[test]
    fn test_brush_preview_new() {
        let preview = BrushPreview::new();
        assert!(preview.material.is_none()); // Lazy initialization
    }
}
