use crate::noise_field::NoiseField;

const WATERTIGHT_EPSILON: f32 = 0.001;

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
    pub fn generate_with_terrain(
        min: [f32; 3],
        max: [f32; 3],
        floor_y: f32,
        noise: &NoiseField,
        segments: usize,
    ) -> Self {
        let mut mesh = Self {
            vertices: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
        };

        // Four walls: -X, +X, -Z, +Z
        let walls = [
            WallParams {
                // -X wall
                fixed_coord: min[0],
                vary_start: min[2],
                vary_end: max[2],
                along_z: true,
                normal: [-1.0, 0.0, 0.0],
            },
            WallParams {
                // +X wall
                fixed_coord: max[0],
                vary_start: max[2],
                vary_end: min[2],
                along_z: true,
                normal: [1.0, 0.0, 0.0],
            },
            WallParams {
                // -Z wall
                fixed_coord: min[2],
                vary_start: max[0],
                vary_end: min[0],
                along_z: false,
                normal: [0.0, 0.0, -1.0],
            },
            WallParams {
                // +Z wall
                fixed_coord: max[2],
                vary_start: min[0],
                vary_end: max[0],
                along_z: false,
                normal: [0.0, 0.0, 1.0],
            },
        ];

        for wall in &walls {
            mesh.add_wall_strip(wall, floor_y, noise, segments);
        }
        mesh
    }

    fn add_wall_strip(
        &mut self,
        wall: &WallParams,
        floor_y: f32,
        noise: &NoiseField,
        segments: usize,
    ) {
        let segments = segments.max(1);
        let step = (wall.vary_end - wall.vary_start) / segments as f32;

        for i in 0..segments {
            let t0 = wall.vary_start + step * i as f32;
            let t1 = wall.vary_start + step * (i + 1) as f32;

            let (x0, z0, x1, z1) = if wall.along_z {
                (wall.fixed_coord, t0, wall.fixed_coord, t1)
            } else {
                (t0, wall.fixed_coord, t1, wall.fixed_coord)
            };

            let y0_top = Self::find_terrain_height(noise, x0, z0, floor_y) + WATERTIGHT_EPSILON;
            let y1_top = Self::find_terrain_height(noise, x1, z1, floor_y) + WATERTIGHT_EPSILON;

            self.add_quad(
                [x0, 0.0, z0],
                [x1, 0.0, z1],
                [x1, y1_top, z1],
                [x0, y0_top, z0],
                wall.normal,
            );
        }
    }

    fn find_terrain_height(noise: &NoiseField, x: f32, z: f32, floor_y: f32) -> f32 {
        let amplitude = noise.get_amplitude();
        let search_max = floor_y + amplitude * 2.0 + 50.0;

        let mut low = 0.0_f32;
        let mut high = search_max;

        for _ in 0..23 {
            let mid = (low + high) * 0.5;
            let sdf = noise.sample(x, mid, z);

            if sdf < 0.0 {
                low = mid;
            } else {
                high = mid
            }

            if (high - low) < 0.001 {
                break;
            }
        }

        (low + high) * 0.5
    }

    fn add_quad(
        &mut self,
        v0: [f32; 3],
        v1: [f32; 3],
        v2: [f32; 3],
        v3: [f32; 3],
        normal: [f32; 3],
    ) {
        let base = self.vertices.len() as i32;
        self.vertices.extend([v0, v1, v2, v3]);
        self.normals.extend([normal; 4]);
        self.indices
            .extend([base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}
