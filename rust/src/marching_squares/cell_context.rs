use godot::prelude::*;

use super::types::*;

#[derive(Clone, Debug, Default)]
pub struct CellColorState {
    // Boundaries
    pub min_height: f32,
    pub max_height: f32,
    pub is_boundary: bool,

    // Floor and celing colors
    pub floor_lower_color_0: Color,
    pub floor_upper_color_0: Color,
    pub floor_lower_color_1: Color,
    pub floor_upper_color_1: Color,
    pub wall_lower_color_0: Color,
    pub wall_upper_color_0: Color,
    pub wall_lower_color_1: Color,
    pub wall_upper_color_1: Color,

    // Textures
    pub material_a: TextureIndex,
    pub material_b: TextureIndex,
    pub material_c: TextureIndex,
}

impl CellColorState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Debug, Default)]
pub struct CellContext {
    // Grid
    pub heights: [f32; 4],
    pub edges: [bool; 4],
    pub rotation: usize,
    pub cell_coords: Vector2i,
    pub dimensions: Vector3i,
    pub cell_size: Vector2,
    pub merge_threshold: f32,
    pub higher_poly_floors: bool,

    // Colors
    pub color_map_0: Vec<Color>,
    pub color_map_1: Vec<Color>,
    pub wall_color_map_0: Vec<Color>,

    pub wall_color_map_1: Vec<Color>,
    pub grass_mask_map: Vec<Color>,

    pub color_state: CellColorState,

    // Blend mode from terrain system
    pub blend_mode: BlendMode,
    pub use_ridge_texture: bool,
    pub ridge_threshold: f32,

    // Whether this is a new (freshly created) chunk
    pub is_new_chunk: bool,

    // Floor mode toggle: true = floor geometry, false = wall geometry
    pub floor_mode: bool,

    // Blend thresholds
    pub lower_threshold: f32,
    pub upper_threshold: f32,

    // Chunk world position for wall UV2 offset
    pub chunk_position: Vector3,
}

// ================================
// ===== CellContext Impl  ========
// ================================

impl CellContext {
    pub fn ay(&self) -> f32 {
        self.heights[self.rotation]
    }
    pub fn by(&self) -> f32 {
        self.heights[(self.rotation + 1) % 4]
    }
    pub fn dy(&self) -> f32 {
        self.heights[(self.rotation + 2) % 4]
    }
    pub fn cy(&self) -> f32 {
        self.heights[(self.rotation + 3) % 4]
    }
    pub fn ab(&self) -> bool {
        self.edges[self.rotation]
    }
    pub fn bd(&self) -> bool {
        self.edges[(self.rotation + 1) % 4]
    }
    pub fn cd(&self) -> bool {
        self.edges[(self.rotation + 2) % 4]
    }
    pub fn ac(&self) -> bool {
        self.edges[(self.rotation + 3) % 4]
    }
    pub fn rotate_cell(&mut self, rotations: i32) {
        self.rotation = ((self.rotation as i32 + 4 + rotations) % 4) as usize
    }
    pub fn is_higher(&self, a: f32, b: f32) -> bool {
        a - b > self.merge_threshold
    }
    pub fn is_lower(&self, a: f32, b: f32) -> bool {
        a - b < -self.merge_threshold
    }
    pub fn is_merged(&self, a: f32, b: f32) -> bool {
        (a - b).abs() < self.merge_threshold
    }
    pub fn start_floor(&mut self) {
        self.floor_mode = true;
    }
    pub fn start_wall(&mut self) {
        self.floor_mode = false;
    }
    pub fn color_index(&self, x: i32, z: i32) -> usize {
        (z * self.dimensions.x + x) as usize
    }
    pub(super) fn corner_indices(&self) -> [usize; 4] {
        let cc = self.cell_coords;
        let dim_x = self.dimensions.x;
        [
            (cc.y * dim_x + cc.x) as usize,           // A
            (cc.y * dim_x + cc.x + 1) as usize,       // B
            ((cc.y + 1) * dim_x + cc.x) as usize,     // C
            ((cc.y + 1) * dim_x + cc.x + 1) as usize, // D
        ]
    }
    pub(super) fn calculate_boundary_colors(&mut self) {
        let corners = self.corner_indices();
        let corner_heights = [
            self.heights[0], // A
            self.heights[1], // B
            self.heights[3], // C
            self.heights[2], // D
        ];

        let mut min_idx = 0;
        let mut max_idx = 0;
        for i in 1..4 {
            if corner_heights[i] < corner_heights[min_idx] {
                min_idx = i;
            }
            if corner_heights[i] > corner_heights[max_idx] {
                max_idx = i;
            }
        }

        // Floor boundary colors
        self.color_state.floor_lower_color_0 = self.color_map_0[corners[min_idx]];
        self.color_state.floor_upper_color_0 = self.color_map_0[corners[max_idx]];
        self.color_state.floor_lower_color_1 = self.color_map_1[corners[min_idx]];
        self.color_state.floor_upper_color_1 = self.color_map_1[corners[max_idx]];

        // Wall boundary colors
        self.color_state.wall_lower_color_0 = self.wall_color_map_0[corners[min_idx]];
        self.color_state.wall_upper_color_0 = self.wall_color_map_0[corners[max_idx]];

        self.color_state.wall_lower_color_1 = self.wall_color_map_1[corners[min_idx]];
        self.color_state.wall_upper_color_1 = self.wall_color_map_1[corners[max_idx]];
    }

    fn calculate_cell_material_pair(&mut self) {
        let corners = self.corner_indices();
        let texture_a = TextureIndex::from_color_pair(
            self.color_map_0[corners[0]],
            self.color_map_1[corners[0]],
        );

        let texture_b = TextureIndex::from_color_pair(
            self.color_map_0[corners[1]],
            self.color_map_1[corners[1]],
        );

        let texture_c = TextureIndex::from_color_pair(
            self.color_map_0[corners[2]],
            self.color_map_1[corners[2]],
        );

        let texture_d = TextureIndex::from_color_pair(
            self.color_map_0[corners[3]],
            self.color_map_1[corners[3]],
        );

        let mut counts = [0u8; 16];
        for t in [texture_a, texture_b, texture_c, texture_d] {
            counts[t.0 as usize] += 1;
        }

        // Find top 3 by linear scan
        let mut first = (0u8, 0u8); // (index, count)
        let mut second = (0u8, 0u8);
        let mut third = (0u8, 0u8);
        for (i, &count) in counts.iter().enumerate() {
            if count > first.1 {
                third = second;
                second = first;
                first = (i as u8, count);
            } else if count > second.1 {
                third = second;
                second = (i as u8, count);
            } else if count > third.1 {
                third = (i as u8, count);
            }
        }

        self.color_state.material_a = TextureIndex(first.0);
        self.color_state.material_b = if second.1 > 0 {
            TextureIndex(second.0)
        } else {
            self.color_state.material_a
        };
        self.color_state.material_c = if third.1 > 0 {
            TextureIndex(third.0)
        } else {
            self.color_state.material_b
        };
    }

    pub(super) fn calculate_material_blend_data(
        &self,
        vertex_x: f32,
        vertex_z: f32,
        source_map_0: &[Color],
        source_map_1: &[Color],
    ) -> Color {
        let corners = self.corner_indices();
        let texture_a =
            TextureIndex::from_color_pair(source_map_0[corners[0]], source_map_1[corners[0]]);
        let texture_b =
            TextureIndex::from_color_pair(source_map_0[corners[1]], source_map_1[corners[1]]);
        let texture_c =
            TextureIndex::from_color_pair(source_map_0[corners[2]], source_map_1[corners[2]]);
        let texture_d =
            TextureIndex::from_color_pair(source_map_0[corners[3]], source_map_1[corners[3]]);

        // Bilinear interpolation weights
        let weight_a = (1.0 - vertex_x) * (1.0 - vertex_z);
        let weight_b = vertex_x * (1.0 - vertex_z);
        let weight_c = (1.0 - vertex_x) * vertex_z;
        let weight_d = vertex_x * vertex_z;

        let mut weight_material_a = 0.0f32;
        let mut weight_material_b = 0.0f32;
        let mut weight_material_c = 0.0f32;

        for (texture, weight) in [
            (texture_a, weight_a),
            (texture_b, weight_b),
            (texture_c, weight_c),
            (texture_d, weight_d),
        ] {
            if texture == self.color_state.material_a {
                weight_material_a += weight;
            } else if texture == self.color_state.material_b {
                weight_material_b += weight;
            } else if texture == self.color_state.material_c {
                weight_material_c += weight;
            }
        }

        let total_weight = weight_material_a + weight_material_b + weight_material_c;
        if total_weight > MIN_WEIGHT_THRESHOLD {
            weight_material_a /= total_weight;
            weight_material_b /= total_weight;
        };

        let packed_materials = (self.color_state.material_a.as_f32()
            + self.color_state.material_b.as_f32() * MATERIAL_PACK_SCALE)
            / MATERIAL_PACK_NORMALIZE;

        Color::from_rgba(
            packed_materials,
            self.color_state.material_c.as_f32() / MATERIAL_INDEX_SCALE,
            weight_material_a,
            weight_material_b,
        )
    }
}
