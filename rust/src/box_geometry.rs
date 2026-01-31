/// Parameters for generating a single wall strip
struct WallParams {
    fixed_coord: f32,
    vary_start: f32,
    vary_end: f32,
    along_z: bool,
    normal: [f32; 3],
}
pub struct BoxMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<i32>,
}

impl BoxMesh {
    fn add_skirt_quad(&mut self, wall: &WallParams, height: f32) {
        let (x0, z0, x1, z1) = if wall.along_z {
            (
                wall.fixed_coord,
                wall.vary_start,
                wall.fixed_coord,
                wall.vary_end,
            )
        } else {
            (
                wall.vary_start,
                wall.fixed_coord,
                wall.vary_end,
                wall.fixed_coord,
            )
        };

        Self::add_quad(
            &mut self.vertices,
            &mut self.normals,
            &mut self.indices,
            [x0, 0.0, z0],
            [x1, 0.0, z1],
            [x1, height, z1],
            [x0, height, z0],
            wall.normal,
        );
    }

    pub fn generate_skirt(min: [f32; 3], max: [f32; 3]) -> Self {
        let mut mesh = Self {
            vertices: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
        };

        let skirt_height = 2.0;

        let walls = [
            WallParams {
                fixed_coord: min[0],
                vary_start: min[2],
                vary_end: max[2],
                along_z: true,
                normal: [-1.0, 0.0, 0.0],
            },
            WallParams {
                fixed_coord: max[0],
                vary_start: max[2],
                vary_end: min[2],
                along_z: true,
                normal: [1.0, 0.0, 0.0],
            },
            WallParams {
                fixed_coord: min[2],
                vary_start: max[0],
                vary_end: min[0],
                along_z: false,
                normal: [0.0, 0.0, -1.0],
            },
            WallParams {
                fixed_coord: max[2],
                vary_start: min[0],
                vary_end: max[0],
                along_z: false,
                normal: [0.0, 0.0, 1.0],
            },
        ];

        for wall in &walls {
            mesh.add_skirt_quad(wall, skirt_height);
        }

        mesh
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    fn add_quad(
        vertices: &mut Vec<[f32; 3]>,
        normals: &mut Vec<[f32; 3]>,
        indices: &mut Vec<i32>,
        v0: [f32; 3],
        v1: [f32; 3],
        v2: [f32; 3],
        v3: [f32; 3],
        normal: [f32; 3],
    ) {
        let base = vertices.len() as i32;
        vertices.extend([v0, v1, v2, v3]);
        normals.extend([normal; 4]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
