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

/// Immutable configuration that stays constant across all cells in a generation pass.
#[derive(Clone, Debug, Default)]
pub struct CellConfig {
    pub dimensions: Vector3i,
    pub cell_size: Vector2,
    pub merge_threshold: f32,
    pub higher_poly_floors: bool,
    pub blend_mode: BlendMode,
    pub use_ridge_texture: bool,
    pub ridge_threshold: f32,
    pub is_new_chunk: bool,
    pub chunk_position: Vector3,
}

#[derive(Clone, Debug, Default)]
pub struct CellContext {
    // Per-cell mutable state
    pub heights: [f32; 4],
    pub edges: [bool; 4],
    pub profiles: [BoundaryProfile; 4],
    pub rotation: usize,
    pub cell_coords: Vector2i,
    pub color_state: CellColorState,
    pub floor_mode: bool,

    // Shared across all cells in a generation pass
    pub config: CellConfig,
    pub color_maps: ColorMaps,
}

#[cfg(test)]
impl CellContext {
    /// Shared test helper for creating a default CellContext with standard test parameters.
    pub fn test_default(dim_x: i32, dim_z: i32) -> Self {
        let total = (dim_x * dim_z) as usize;
        Self {
            config: CellConfig {
                dimensions: Vector3i::new(dim_x, 32, dim_z),
                cell_size: Vector2::new(2.0, 2.0),
                merge_threshold: 1.3,
                higher_poly_floors: true,
                ..Default::default()
            },
            color_maps: ColorMaps::new_default(total),
            ..Default::default()
        }
    }
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
    pub fn corner_heights(&self) -> (f32, f32, f32, f32) {
        (self.ay(), self.by(), self.cy(), self.dy())
    }
    pub fn rotate_cell(&mut self, rotations: i32) {
        self.rotation = ((self.rotation as i32 + 4 + rotations) % 4) as usize
    }
    pub fn is_higher(&self, a: f32, b: f32) -> bool {
        a - b > self.config.merge_threshold
    }
    pub fn is_lower(&self, a: f32, b: f32) -> bool {
        a - b < -self.config.merge_threshold
    }
    pub fn is_merged(&self, a: f32, b: f32) -> bool {
        (a - b).abs() < self.config.merge_threshold
    }

    /// Compute boundary profiles from canonical (unrotated) heights.
    /// Must be called after edges are computed but before case matching.
    /// profiles[0] = AB, profiles[1] = BD, profiles[2] = CD, profiles[3] = AC
    pub fn compute_profiles(&mut self) {
        let (a, b, d, c) = (
            self.heights[0],
            self.heights[1],
            self.heights[2],
            self.heights[3],
        );
        self.profiles[0] = compute_boundary_profile(a, b, self.config.merge_threshold); // AB
        self.profiles[1] = compute_boundary_profile(b, d, self.config.merge_threshold); // BD
        self.profiles[2] = compute_boundary_profile(c, d, self.config.merge_threshold); // CD
        self.profiles[3] = compute_boundary_profile(a, c, self.config.merge_threshold);
        // AC
    }

    /// Get the boundary height along the AB edge (top edge in rotated frame).
    /// t=0 is the A corner, t=1 is the B corner.
    ///
    /// Profiles 2 (CD) and 3 (AC) have their h1→h2 direction reversed relative
    /// to the circular rotation order, so t must be flipped when using them.
    pub fn ab_height(&self, t: f32, is_upper: bool) -> f32 {
        let p = self.rotation;
        let t_adj = if p >= 2 { 1.0 - t } else { t };
        self.profiles[p].height_at(t_adj, is_upper)
    }

    /// Get the boundary height along the BD edge (right edge in rotated frame).
    /// t=0 is the B corner, t=1 is the D corner.
    pub fn bd_height(&self, t: f32, is_upper: bool) -> f32 {
        let p = (self.rotation + 1) % 4;
        let t_adj = if p >= 2 { 1.0 - t } else { t };
        self.profiles[p].height_at(t_adj, is_upper)
    }

    /// Get the boundary height along the CD edge (bottom edge in rotated frame).
    /// t=0 is the C corner, t=1 is the D corner.
    ///
    /// CD traverses reverse circular order. Flip t when the profile direction
    /// agrees with circular order (profiles 0, 1) — the two reversals cancel
    /// for profiles 2, 3.
    pub fn cd_height(&self, t: f32, is_upper: bool) -> f32 {
        let p = (self.rotation + 2) % 4;
        let t_adj = if p < 2 { 1.0 - t } else { t };
        self.profiles[p].height_at(t_adj, is_upper)
    }

    /// Get the boundary height along the AC edge (left edge in rotated frame).
    /// t=0 is the A corner, t=1 is the C corner.
    pub fn ac_height(&self, t: f32, is_upper: bool) -> f32 {
        let p = (self.rotation + 3) % 4;
        let t_adj = if p < 2 { 1.0 - t } else { t };
        self.profiles[p].height_at(t_adj, is_upper)
    }

    pub fn start_floor(&mut self) {
        self.floor_mode = true;
    }
    pub fn start_wall(&mut self) {
        self.floor_mode = false;
    }
    pub(super) fn corner_indices(&self) -> [usize; 4] {
        let cc = self.cell_coords;
        let dim_x = self.config.dimensions.x;
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
        self.color_state.floor_lower_color_0 = self.color_maps.color_0[corners[min_idx]];
        self.color_state.floor_upper_color_0 = self.color_maps.color_0[corners[max_idx]];
        self.color_state.floor_lower_color_1 = self.color_maps.color_1[corners[min_idx]];
        self.color_state.floor_upper_color_1 = self.color_maps.color_1[corners[max_idx]];

        // Wall boundary colors
        self.color_state.wall_lower_color_0 = self.color_maps.wall_color_0[corners[min_idx]];
        self.color_state.wall_upper_color_0 = self.color_maps.wall_color_0[corners[max_idx]];

        self.color_state.wall_lower_color_1 = self.color_maps.wall_color_1[corners[min_idx]];
        self.color_state.wall_upper_color_1 = self.color_maps.wall_color_1[corners[max_idx]];
    }

    pub(super) fn calculate_cell_material_pair(&mut self) {
        let corners = self.corner_indices();
        let [texture_a, texture_b, texture_c, texture_d] =
            corners.map(|i| self.color_maps.texture_at(i));

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
