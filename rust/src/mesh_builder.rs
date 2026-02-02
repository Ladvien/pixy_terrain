use godot::prelude::*;
use godot::classes::{ArrayMesh, Material, StandardMaterial3D, SurfaceTool};
use godot::classes::mesh::PrimitiveType;

use crate::tile_data::TileGrid;

/// Generates placeholder colors for tile IDs (for debugging/prototyping)
fn tile_id_to_color(tile_id: u16) -> Color {
    // Use a simple hash to generate distinct colors for each tile ID
    match tile_id % 8 {
        0 => Color::from_rgba(0.8, 0.2, 0.2, 1.0), // Red
        1 => Color::from_rgba(0.2, 0.8, 0.2, 1.0), // Green
        2 => Color::from_rgba(0.2, 0.2, 0.8, 1.0), // Blue
        3 => Color::from_rgba(0.8, 0.8, 0.2, 1.0), // Yellow
        4 => Color::from_rgba(0.8, 0.2, 0.8, 1.0), // Magenta
        5 => Color::from_rgba(0.2, 0.8, 0.8, 1.0), // Cyan
        6 => Color::from_rgba(0.8, 0.5, 0.2, 1.0), // Orange
        7 => Color::from_rgba(0.5, 0.2, 0.8, 1.0), // Purple
        _ => Color::from_rgba(0.5, 0.5, 0.5, 1.0), // Gray
    }
}

/// Cube face data: vertices and normal for each face
/// Vertices are in counter-clockwise order for front-face culling
struct CubeFace {
    vertices: [Vector3; 4],
    normal: Vector3,
    uvs: [Vector2; 4],
}

/// Get the 6 faces of a unit cube centered at origin
fn get_cube_faces() -> [CubeFace; 6] {
    [
        // +X face (right)
        CubeFace {
            vertices: [
                Vector3::new(0.5, -0.5, 0.5),
                Vector3::new(0.5, 0.5, 0.5),
                Vector3::new(0.5, 0.5, -0.5),
                Vector3::new(0.5, -0.5, -0.5),
            ],
            normal: Vector3::new(1.0, 0.0, 0.0),
            uvs: [
                Vector2::new(0.0, 1.0),
                Vector2::new(0.0, 0.0),
                Vector2::new(1.0, 0.0),
                Vector2::new(1.0, 1.0),
            ],
        },
        // -X face (left)
        CubeFace {
            vertices: [
                Vector3::new(-0.5, -0.5, -0.5),
                Vector3::new(-0.5, 0.5, -0.5),
                Vector3::new(-0.5, 0.5, 0.5),
                Vector3::new(-0.5, -0.5, 0.5),
            ],
            normal: Vector3::new(-1.0, 0.0, 0.0),
            uvs: [
                Vector2::new(0.0, 1.0),
                Vector2::new(0.0, 0.0),
                Vector2::new(1.0, 0.0),
                Vector2::new(1.0, 1.0),
            ],
        },
        // +Y face (top)
        CubeFace {
            vertices: [
                Vector3::new(-0.5, 0.5, -0.5),
                Vector3::new(0.5, 0.5, -0.5),
                Vector3::new(0.5, 0.5, 0.5),
                Vector3::new(-0.5, 0.5, 0.5),
            ],
            normal: Vector3::new(0.0, 1.0, 0.0),
            uvs: [
                Vector2::new(0.0, 0.0),
                Vector2::new(1.0, 0.0),
                Vector2::new(1.0, 1.0),
                Vector2::new(0.0, 1.0),
            ],
        },
        // -Y face (bottom)
        CubeFace {
            vertices: [
                Vector3::new(-0.5, -0.5, 0.5),
                Vector3::new(0.5, -0.5, 0.5),
                Vector3::new(0.5, -0.5, -0.5),
                Vector3::new(-0.5, -0.5, -0.5),
            ],
            normal: Vector3::new(0.0, -1.0, 0.0),
            uvs: [
                Vector2::new(0.0, 1.0),
                Vector2::new(1.0, 1.0),
                Vector2::new(1.0, 0.0),
                Vector2::new(0.0, 0.0),
            ],
        },
        // +Z face (front)
        CubeFace {
            vertices: [
                Vector3::new(0.5, -0.5, 0.5),
                Vector3::new(-0.5, -0.5, 0.5),
                Vector3::new(-0.5, 0.5, 0.5),
                Vector3::new(0.5, 0.5, 0.5),
            ],
            normal: Vector3::new(0.0, 0.0, 1.0),
            uvs: [
                Vector2::new(0.0, 1.0),
                Vector2::new(1.0, 1.0),
                Vector2::new(1.0, 0.0),
                Vector2::new(0.0, 0.0),
            ],
        },
        // -Z face (back)
        CubeFace {
            vertices: [
                Vector3::new(-0.5, -0.5, -0.5),
                Vector3::new(0.5, -0.5, -0.5),
                Vector3::new(0.5, 0.5, -0.5),
                Vector3::new(-0.5, 0.5, -0.5),
            ],
            normal: Vector3::new(0.0, 0.0, -1.0),
            uvs: [
                Vector2::new(0.0, 1.0),
                Vector2::new(1.0, 1.0),
                Vector2::new(1.0, 0.0),
                Vector2::new(0.0, 0.0),
            ],
        },
    ]
}

/// Neighbor offsets for each face (same order as get_cube_faces)
const NEIGHBOR_OFFSETS: [(i32, i32, i32); 6] = [
    (1, 0, 0),   // +X
    (-1, 0, 0),  // -X
    (0, 1, 0),   // +Y
    (0, -1, 0),  // -Y
    (0, 0, 1),   // +Z
    (0, 0, -1),  // -Z
];

/// Builds an ArrayMesh from a TileGrid
pub struct MeshBuilder {
    voxel_size: f32,
}

impl MeshBuilder {
    pub fn new(voxel_size: f32) -> Self {
        Self { voxel_size }
    }

    /// Build a mesh from the tile grid using SurfaceTool
    /// Returns None if the grid is empty
    pub fn build_mesh(&self, grid: &TileGrid) -> Option<Gd<ArrayMesh>> {
        if grid.tile_count() == 0 {
            return None;
        }

        let mut st = SurfaceTool::new_gd();
        st.begin(PrimitiveType::TRIANGLES);

        let cube_faces = get_cube_faces();
        let mut has_geometry = false;

        // Build geometry for each tile
        for (&(x, y, z), tile) in grid.iter() {
            let center = Vector3::new(
                x as f32 * self.voxel_size,
                y as f32 * self.voxel_size,
                z as f32 * self.voxel_size,
            );
            let color = tile_id_to_color(tile.tile_id);

            // Check each face - only add if neighbor is empty
            for (face_idx, face) in cube_faces.iter().enumerate() {
                let (dx, dy, dz) = NEIGHBOR_OFFSETS[face_idx];
                let neighbor_pos = (x + dx, y + dy, z + dz);

                // Skip face if neighbor exists (face culling optimization)
                if grid.has_tile(neighbor_pos.0, neighbor_pos.1, neighbor_pos.2) {
                    continue;
                }

                has_geometry = true;

                // Add two triangles for this quad face
                // Triangle 1: vertices 0, 1, 2
                // Triangle 2: vertices 0, 2, 3
                let indices = [0, 1, 2, 0, 2, 3];

                for &idx in &indices {
                    let vert = face.vertices[idx] * self.voxel_size + center;
                    st.set_normal(face.normal);
                    st.set_color(color);
                    st.set_uv(face.uvs[idx]);
                    st.add_vertex(vert);
                }
            }
        }

        if !has_geometry {
            return None;
        }

        // Generate the mesh
        st.commit()
    }

    /// Create a simple material that uses vertex colors
    pub fn create_default_material() -> Gd<Material> {
        let mut material = StandardMaterial3D::new_gd();
        material.set_flag(godot::classes::base_material_3d::Flags::ALBEDO_FROM_VERTEX_COLOR, true);
        material.upcast()
    }
}

/// Builds a wireframe mesh showing grid bounds (for debugging)
pub fn build_grid_wireframe(
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    min_z: i32,
    max_z: i32,
    voxel_size: f32,
) -> Gd<ArrayMesh> {
    let mut st = SurfaceTool::new_gd();
    st.begin(PrimitiveType::LINES);

    let min_pos = Vector3::new(
        min_x as f32 * voxel_size - voxel_size * 0.5,
        min_y as f32 * voxel_size - voxel_size * 0.5,
        min_z as f32 * voxel_size - voxel_size * 0.5,
    );
    let max_pos = Vector3::new(
        (max_x + 1) as f32 * voxel_size - voxel_size * 0.5,
        (max_y + 1) as f32 * voxel_size - voxel_size * 0.5,
        (max_z + 1) as f32 * voxel_size - voxel_size * 0.5,
    );

    let white = Color::from_rgba(1.0, 1.0, 1.0, 0.5);

    // Helper closure to add a line
    let add_line = |st: &mut Gd<SurfaceTool>, p1: Vector3, p2: Vector3| {
        st.set_color(white);
        st.add_vertex(p1);
        st.set_color(white);
        st.add_vertex(p2);
    };

    // Draw grid lines along X
    for y in min_y..=max_y + 1 {
        for z in min_z..=max_z + 1 {
            let y_pos = y as f32 * voxel_size - voxel_size * 0.5;
            let z_pos = z as f32 * voxel_size - voxel_size * 0.5;
            add_line(
                &mut st,
                Vector3::new(min_pos.x, y_pos, z_pos),
                Vector3::new(max_pos.x, y_pos, z_pos),
            );
        }
    }

    // Draw grid lines along Y
    for x in min_x..=max_x + 1 {
        for z in min_z..=max_z + 1 {
            let x_pos = x as f32 * voxel_size - voxel_size * 0.5;
            let z_pos = z as f32 * voxel_size - voxel_size * 0.5;
            add_line(
                &mut st,
                Vector3::new(x_pos, min_pos.y, z_pos),
                Vector3::new(x_pos, max_pos.y, z_pos),
            );
        }
    }

    // Draw grid lines along Z
    for x in min_x..=max_x + 1 {
        for y in min_y..=max_y + 1 {
            let x_pos = x as f32 * voxel_size - voxel_size * 0.5;
            let y_pos = y as f32 * voxel_size - voxel_size * 0.5;
            add_line(
                &mut st,
                Vector3::new(x_pos, y_pos, min_pos.z),
                Vector3::new(x_pos, y_pos, max_pos.z),
            );
        }
    }

    st.commit().expect("Failed to commit wireframe mesh")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_id_colors_distinct() {
        // Ensure different tile IDs produce different colors
        let colors: Vec<Color> = (0..8).map(|id| tile_id_to_color(id)).collect();
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j], "Colors {} and {} should be distinct", i, j);
            }
        }
    }
}
