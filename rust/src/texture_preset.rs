
use godot::prelude::*;

/// A list of terrain textures, scales, grass sprites, and colors.
/// Full implementation in Part 11.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyTextureList {
    base: Base<Resource>,

    #[export]
    pub texture_1: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_2: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_3: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_4: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_5: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_6: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_7: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_8: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_9: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_10: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_11: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_12: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_13: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_14: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub texture_15: Option<Gd<godot::classes::Texture2D>>,

    #[export]
    #[init(val = 1.0)]
    pub scale_1: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_2: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_3: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_4: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_5: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_6: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_7: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_8: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_9: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_10: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_11: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_12: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_13: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_14: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_15: f32,

    #[export]
    pub grass_sprite_1: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub grass_sprite_2: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub grass_sprite_3: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub grass_sprite_4: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub grass_sprite_5: Option<Gd<godot::classes::Texture2D>>,
    #[export]
    pub grass_sprite_6: Option<Gd<godot::classes::Texture2D>>,

    #[export]
    #[init(val = Color::from_rgba(0.4, 0.5, 0.3, 1.0))]
    pub grass_color_1: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3, 0.5, 0.4, 1.0))]
    pub grass_color_2: Color,
    #[export]
    #[init(val = Color::from_rgba(0.4, 0.4, 0.3, 1.0))]
    pub grass_color_3: Color,
    #[export]
    #[init(val = Color::from_rgba(0.4, 0.5, 0.3, 1.0))]
    pub grass_color_4: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3, 0.5, 0.4, 1.0))]
    pub grass_color_5: Color,
    #[export]
    #[init(val = Color::from_rgba(0.4, 0.4, 0.4, 1.0))]
    pub grass_color_6: Color,

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

/// Texture preset resource for save/load workflow.
/// Full implementation in Part 11.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyTexturePreset {
    base: Base<Resource>,

    #[export]
    pub textures: Option<Gd<PixyTextureList>>,
}
