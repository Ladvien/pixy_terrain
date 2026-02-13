//! Centralized shader uniform name constants and sync helpers.
//!
//! All shader parameter string literals live here so they can be
//! cross-validated against .gdshader files in tests.

/// Texture uniform names in the terrain shader (16 slots, index 0-15).
pub const TEXTURE_UNIFORM_NAMES: [&str; 16] = [
    "vc_tex_rr",
    "vc_tex_rg",
    "vc_tex_rb",
    "vc_tex_ra",
    "vc_tex_gr",
    "vc_tex_gg",
    "vc_tex_gb",
    "vc_tex_ga",
    "vc_tex_br",
    "vc_tex_bg",
    "vc_tex_bb",
    "vc_tex_ba",
    "vc_tex_ar",
    "vc_tex_ag",
    "vc_tex_ab",
    "vc_tex_aa",
];

/// Ground albedo uniform names in the terrain shader (6 slots matching texture slots 1-6).
pub const GROUND_ALBEDO_NAMES: [&str; 6] = [
    "ground_albedo",
    "ground_albedo_2",
    "ground_albedo_3",
    "ground_albedo_4",
    "ground_albedo_5",
    "ground_albedo_6",
];

/// Texture scale uniform names in the terrain shader (15 slots, indices 1-15).
pub const TEXTURE_SCALE_NAMES: [&str; 15] = [
    "texture_scale_1",
    "texture_scale_2",
    "texture_scale_3",
    "texture_scale_4",
    "texture_scale_5",
    "texture_scale_6",
    "texture_scale_7",
    "texture_scale_8",
    "texture_scale_9",
    "texture_scale_10",
    "texture_scale_11",
    "texture_scale_12",
    "texture_scale_13",
    "texture_scale_14",
    "texture_scale_15",
];

/// Grass texture uniform names in the grass shader (6 slots).
pub const GRASS_TEXTURE_NAMES: [&str; 6] = [
    "grass_texture",
    "grass_texture_2",
    "grass_texture_3",
    "grass_texture_4",
    "grass_texture_5",
    "grass_texture_6",
];

/// Grass ground color uniform names in the grass shader (6 slots).
/// Slot 0 uses "grass_base_color"; slots 1-5 use "grass_color_N".
pub const GRASS_COLOR_NAMES: [&str; 6] = [
    "grass_base_color",
    "grass_color_2",
    "grass_color_3",
    "grass_color_4",
    "grass_color_5",
    "grass_color_6",
];

/// use_base_color flags in the grass shader (6 slots).
pub const USE_BASE_COLOR_NAMES: [&str; 6] = [
    "use_base_color",
    "use_base_color_2",
    "use_base_color_3",
    "use_base_color_4",
    "use_base_color_5",
    "use_base_color_6",
];

/// use_grass_tex flags in the grass shader (5 slots, indices 2-6).
pub const USE_GRASS_TEX_NAMES: [&str; 5] = [
    "use_grass_tex_2",
    "use_grass_tex_3",
    "use_grass_tex_4",
    "use_grass_tex_5",
    "use_grass_tex_6",
];

/// Declarative macro for setting multiple scalar shader parameters at once.
macro_rules! sync_shader_params {
    ($mat:expr, [ $( $uniform:literal => $value:expr ),* $(,)? ]) => {
        $( $mat.set_shader_parameter($uniform, &($value).to_variant()); )*
    };
}

pub(crate) use sync_shader_params;
