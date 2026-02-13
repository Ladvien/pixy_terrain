use godot::classes::Resource;
use godot::prelude::*;

/// Stores per-texture configuration data: textures, scales, grass sprites, grass colors, grass toggles.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyTextureList {
    base: Base<Resource>,

    #[export]
    pub textures: VarArray,

    #[export]
    pub texture_scales: PackedFloat32Array,

    #[export]
    pub ground_colors: PackedColorArray,

    #[export]
    pub grass_sprites: VarArray,

    #[export]
    #[init(val = true)]
    pub has_grass_2: bool,
    #[export]
    #[init(val = true)]
    pub has_grass_3: bool,
    #[export]
    #[init(val = true)]
    pub has_grass_4: bool,
    #[export]
    #[init(val = true)]
    pub has_grass_5: bool,
    #[export]
    #[init(val = true)]
    pub has_grass_6: bool,
}

/// Container resource for saving/loading a complete texture preset.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
#[allow(clippy::approx_constant)]
pub struct PixyTexturePreset {
    base: Base<Resource>,

    #[export]
    #[init(val = GString::from("New Preset"))]
    pub preset_name: GString,

    #[export]
    pub textures: Option<Gd<PixyTextureList>>,
}
