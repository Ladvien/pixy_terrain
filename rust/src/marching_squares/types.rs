use godot::prelude::*;
// =====================
// ===== Constants =====
// =====================

pub(super) const BLEND_EDGE_SENSITIVITY: f32 = 1.25;
pub const DEFAULT_TEXTURE_COLOR: Color = Color::from_rgba(1.0, 0.0, 0.0, 0.0);
pub(super) const DOMINANT_CHANNEL_THRESHOLD: f32 = 0.99;
pub(super) const MIN_WEIGHT_THRESHOLD: f32 = 0.001;
pub(super) const MIN_HEIGHT_RANGE: f32 = 0.001;
/// Must match `MAT_PACK_STRIDE` in mst_terrain.gdshader.
/// Material B is packed at stride 16 within CUSTOM2.r.
pub(super) const MATERIAL_PACK_SCALE: f32 = 16.0;
pub(super) const MATERIAL_PACK_NORMALIZE: f32 = 255.0;
/// Must match `MAT_INDEX_SCALE` in mst_terrain.gdshader.
/// Material indices are normalized to 0..1 as index/15.
pub(super) const MATERIAL_INDEX_SCALE: f32 = 15.0;
pub(super) const COLOR_1_LOWER_THRESHOLD: f32 = 0.3;
pub(super) const COLOR_1_UPPER_THRESHOLD: f32 = 0.7;
/// Written to CUSTOM2.a to signal vertex-color blending mode.
/// Must be greater than `VERTEX_COLOR_FLAG` (1.5) in mst_terrain.gdshader.
pub(super) const WALL_BLEND_SENTINEL: f32 = 2.0;

// =====================
// ===== Types  ========
// =====================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    Cubic,
    Polyhedron,
    RoundedPolyhedron,
    SemiRound,
    Spherical,
}

impl MergeMode {
    pub fn threshold(self) -> f32 {
        match self {
            MergeMode::Cubic => 0.6,
            MergeMode::Polyhedron => 1.3,
            MergeMode::RoundedPolyhedron => 2.1,
            MergeMode::SemiRound => 5.0,
            MergeMode::Spherical => 20.0,
        }
    }
    pub fn from_index(idx: i32) -> Self {
        match idx {
            0 => MergeMode::Cubic,
            1 => MergeMode::Polyhedron,
            2 => MergeMode::RoundedPolyhedron,
            3 => MergeMode::SemiRound,
            4 => MergeMode::Spherical,
            _ => MergeMode::Polyhedron,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    #[default]
    Interpolated, // 0 - bilinear interpolation across corners
    Direct, // 1 - use corner A's color directly
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChannel {
    Red = 0,
    Green = 1,
    Blue = 2,
    Alpha = 3,
}

impl ColorChannel {
    #[must_use]
    pub fn dominant(c: Color) -> Self {
        let mut max_val = c.r;
        let mut channel = ColorChannel::Red;
        if c.g > max_val {
            max_val = c.g;
            channel = ColorChannel::Green;
        }
        if c.b > max_val {
            max_val = c.b;
            channel = ColorChannel::Blue;
        }
        if c.a > max_val {
            channel = ColorChannel::Alpha
        }
        channel
    }

    #[must_use]
    pub fn dominant_index(c: Color) -> u8 {
        Self::dominant(c) as u8
    }

    #[must_use]
    pub fn from_index(idx: u8) -> Self {
        match idx {
            0 => ColorChannel::Red,
            1 => ColorChannel::Green,
            2 => ColorChannel::Blue,
            3 => ColorChannel::Alpha,
            _ => ColorChannel::Red,
        }
    }

    #[must_use]
    pub fn to_one_hot(self) -> Color {
        match self {
            ColorChannel::Red => Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            ColorChannel::Green => Color::from_rgba(0.0, 1.0, 0.0, 0.0),
            ColorChannel::Blue => Color::from_rgba(0.0, 0.0, 1.0, 0.0),
            ColorChannel::Alpha => Color::from_rgba(0.0, 0.0, 0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct TextureIndex(pub u8); // 0-15

impl TextureIndex {
    #[must_use]
    pub fn from_color_pair(c0: Color, c1: Color) -> Self {
        Self(ColorChannel::dominant_index(c0) * 4 + ColorChannel::dominant_index(c1))
    }

    #[must_use]
    pub fn to_color_pair(self) -> (Color, Color) {
        let c0 = ColorChannel::from_index(self.0 / 4).to_one_hot();
        let c1 = ColorChannel::from_index(self.0 % 4).to_one_hot();
        (c0, c1)
    }

    #[must_use]
    pub fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

#[derive(Clone, Debug, Default)]
pub struct CellGeometry {
    pub verts: Vec<Vector3>,
    pub uvs: Vec<Vector2>,
    pub uv2s: Vec<Vector2>,
    pub colors_0: Vec<Color>,
    pub colors_1: Vec<Color>,
    pub grass_mask: Vec<Color>,
    pub material_blend: Vec<Color>,
    pub is_floor: Vec<bool>,
}

#[derive(Clone, Debug, Default)]
pub struct ColorMaps {
    pub color_0: Vec<Color>,
    pub color_1: Vec<Color>,
    pub wall_color_0: Vec<Color>,
    pub wall_color_1: Vec<Color>,
    pub grass_mask: Vec<Color>,
}

impl ColorMaps {
    pub fn new_default(total: usize) -> Self {
        Self {
            color_0: vec![DEFAULT_TEXTURE_COLOR; total],
            color_1: vec![DEFAULT_TEXTURE_COLOR; total],
            wall_color_0: vec![DEFAULT_TEXTURE_COLOR; total],
            wall_color_1: vec![DEFAULT_TEXTURE_COLOR; total],
            grass_mask: vec![Color::from_rgba(1.0, 1.0, 1.0, 1.0); total],
        }
    }

    /// Get TextureIndex for a corner by map index
    pub fn texture_at(&self, idx: usize) -> TextureIndex {
        TextureIndex::from_color_pair(self.color_0[idx], self.color_1[idx])
    }
}

/// Convert a texture index (0-15) to a pair of vertex colors.
/// Each color has exactly one non-zero RGBA channel.
/// The terrain shader uses the combination to encode 4x4 = 16 textures.
pub fn texture_index_to_colors(idx: i32) -> (Color, Color) {
    TextureIndex(idx as u8).to_color_pair()
}

// ================================
// ===== Boundary Profiles ========
// ================================

/// Describes the canonical vertex layout along a shared cell boundary.
/// Computed from only the two shared corner heights + merge_threshold,
/// so both adjacent cells produce identical boundary geometry.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoundaryProfile {
    pub h1: f32,
    pub h2: f32,
    pub is_merged: bool,
}

/// Compute the canonical boundary profile for an edge defined by two corner heights.
pub fn compute_boundary_profile(h1: f32, h2: f32, merge_threshold: f32) -> BoundaryProfile {
    let is_merged = (h1 - h2).abs() < merge_threshold;
    BoundaryProfile { h1, h2, is_merged }
}

impl BoundaryProfile {
    /// Get the height at parameter t along this boundary.
    /// `is_upper`: true = top of wall (max height), false = bottom of wall (min height).
    /// At t=0 returns h1, at t=1 returns h2.
    /// At t=0.5 (midpoint): if merged, linearly interpolate; if walled, return upper or lower height.
    pub fn height_at(&self, t: f32, is_upper: bool) -> f32 {
        if self.is_merged {
            // Smooth slope: linear interpolation
            self.h1 + (self.h2 - self.h1) * t
        } else if is_upper {
            self.h1.max(self.h2)
        } else {
            self.h1.min(self.h2)
        }
    }
}

#[cfg(test)]
mod boundary_profile_tests {
    use super::*;

    #[test]
    fn test_merged_profile_interpolates() {
        let p = compute_boundary_profile(5.0, 5.5, 1.3);
        assert!(p.is_merged);
        assert_eq!(p.height_at(0.0, true), 5.0);
        assert_eq!(p.height_at(1.0, true), 5.5);
        assert_eq!(p.height_at(0.5, true), 5.25);
    }

    #[test]
    fn test_walled_profile_returns_extremes() {
        let p = compute_boundary_profile(3.0, 8.0, 1.3);
        assert!(!p.is_merged);
        assert_eq!(p.height_at(0.5, true), 8.0);
        assert_eq!(p.height_at(0.5, false), 3.0);
    }

    #[test]
    fn test_endpoints_always_return_corner_heights() {
        let p = compute_boundary_profile(3.0, 8.0, 1.3);
        assert_eq!(p.height_at(0.0, true), 8.0);
        assert_eq!(p.height_at(0.0, false), 3.0);
        assert_eq!(p.height_at(1.0, true), 8.0);
        assert_eq!(p.height_at(1.0, false), 3.0);
    }

    #[test]
    fn test_merged_endpoints() {
        let p = compute_boundary_profile(5.0, 6.0, 1.3);
        assert!(p.is_merged);
        assert_eq!(p.height_at(0.0, true), 5.0);
        assert_eq!(p.height_at(1.0, false), 6.0);
    }
}
