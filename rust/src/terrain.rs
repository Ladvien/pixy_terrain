use godot::prelude::*;
use godot::classes::{MeshInstance3D, ArrayMesh, IMeshInstance3D};

type VariantArray = Array<Variant>;

/// Main terrain editor node - displays voxel-based terrain using transvoxel meshing
#[derive(GodotClass)]
#[class(base=MeshInstance3D, init, tool)]
pub struct PixyTerrain {
    base: Base<MeshInstance3D>,

    /// Size of the terrain grid in voxels
    #[export]
    #[init(val = Vector3i::new(16, 16, 16))]
    grid_size: Vector3i,

    /// Size of each voxel in world units
    #[export]
    #[init(val = 1.0)]
    voxel_size: f32,

    /// Whether to show debug wireframe
    #[export]
    #[init(val = false)]
    debug_wireframe: bool,
}

#[godot_api]
impl IMeshInstance3D for PixyTerrain {
    fn ready(&mut self) {
        godot_print!("PixyTerrain ready! Grid size: {}", self.grid_size);
        self.generate_test_mesh();
    }
}

#[godot_api]
impl PixyTerrain {
    /// Regenerate the terrain mesh
    #[func]
    fn regenerate(&mut self) {
        godot_print!("Regenerating terrain mesh...");
        self.generate_test_mesh();
    }

    /// Clear all terrain data
    #[func]
    fn clear(&mut self) {
        self.base_mut().set_mesh(&Gd::<ArrayMesh>::default());
        godot_print!("Terrain cleared");
    }

    /// Get the current grid dimensions
    #[func]
    fn get_grid_dimensions(&self) -> Vector3i {
        self.grid_size
    }

    /// Set a voxel at the given position (stub for now)
    #[func]
    fn set_voxel(&mut self, _position: Vector3i, _value: f32) {
        // TODO: Implement voxel data storage
        godot_print!("set_voxel called - not yet implemented");
    }

    /// Generate a simple test mesh to verify the extension works
    fn generate_test_mesh(&mut self) {
        use godot::classes::mesh::PrimitiveType;
        use godot::classes::rendering_server::ArrayType;

        let mut mesh = ArrayMesh::new_gd();

        // Create a simple cube mesh for testing
        let size = self.voxel_size * self.grid_size.x as f32 * 0.5;

        // Vertices for a cube
        let vertices = PackedVector3Array::from(&[
            // Front face
            Vector3::new(-size, -size, size),
            Vector3::new(size, -size, size),
            Vector3::new(size, size, size),
            Vector3::new(-size, size, size),
            // Back face
            Vector3::new(-size, -size, -size),
            Vector3::new(-size, size, -size),
            Vector3::new(size, size, -size),
            Vector3::new(size, -size, -size),
            // Top face
            Vector3::new(-size, size, -size),
            Vector3::new(-size, size, size),
            Vector3::new(size, size, size),
            Vector3::new(size, size, -size),
            // Bottom face
            Vector3::new(-size, -size, -size),
            Vector3::new(size, -size, -size),
            Vector3::new(size, -size, size),
            Vector3::new(-size, -size, size),
            // Right face
            Vector3::new(size, -size, -size),
            Vector3::new(size, size, -size),
            Vector3::new(size, size, size),
            Vector3::new(size, -size, size),
            // Left face
            Vector3::new(-size, -size, -size),
            Vector3::new(-size, -size, size),
            Vector3::new(-size, size, size),
            Vector3::new(-size, size, -size),
        ]);

        // Normals
        let normals = PackedVector3Array::from(&[
            // Front
            Vector3::FORWARD, Vector3::FORWARD, Vector3::FORWARD, Vector3::FORWARD,
            // Back
            Vector3::BACK, Vector3::BACK, Vector3::BACK, Vector3::BACK,
            // Top
            Vector3::UP, Vector3::UP, Vector3::UP, Vector3::UP,
            // Bottom
            Vector3::DOWN, Vector3::DOWN, Vector3::DOWN, Vector3::DOWN,
            // Right
            Vector3::RIGHT, Vector3::RIGHT, Vector3::RIGHT, Vector3::RIGHT,
            // Left
            Vector3::LEFT, Vector3::LEFT, Vector3::LEFT, Vector3::LEFT,
        ]);

        // Indices for triangles
        let mut indices = PackedInt32Array::new();
        for face in 0..6 {
            let base = face * 4;
            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
            indices.push(base);
            indices.push(base + 2);
            indices.push(base + 3);
        }

        // Build the mesh arrays - need to fill all slots up to MAX
        let num_arrays = ArrayType::MAX.ord() as usize;
        let mut arrays: VariantArray = VariantArray::new();

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

        self.base_mut().set_mesh(&mesh);
        godot_print!("Test mesh generated: {} vertices", vertices.len());
    }
}
