use godot::prelude::*;
// =====================
// ===== Constants =====
// =====================

pub(super) const BLEND_EDGE_SENSITIVITY: f32 = 1.25;
pub(super) const DEFAULT_TEXTURE_COLOR: Color = Color::from_rgba(1.0, 0.0, 0.0, 0.0);
pub(super) const DOMINANT_CHANNEL_THRESHOLD: f32 = 0.99;
pub(super) const MIN_WEIGHT_THRESHOLD: f32 = 0.001;
pub(super) const MIN_HEIGHT_RANGE: f32 = 0.001;
pub(super) const MATERIAL_PACK_SCALE: f32 = 16.0;
pub(super) const MATERIAL_PACK_NORMALIZE: f32 = 255.0;
pub(super) const MATERIAL_INDEX_SCALE: f32 = 15.0;
pub(super) const COLOR_1_LOWER_THRESHOLD: f32 = 0.3;
pub(super) const COLOR_1_UPPER_THRESHOLD: f32 = 0.7;
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
    pub fn to_index(self) -> i32 {
        match self {
            MergeMode::Cubic => 0,
            MergeMode::Polyhedron => 1,
            MergeMode::RoundedPolyhedron => 2,
            MergeMode::SemiRound => 3,
            MergeMode::Spherical => 4,
        }
    }
    pub fn is_round(self) -> bool {
        matches!(self, MergeMode::SemiRound | MergeMode::Spherical)
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
