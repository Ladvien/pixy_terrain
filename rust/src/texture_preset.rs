use godot::classes::{Resource, Texture2D};
use godot::prelude::*;

use crate::terrain::PixyTerrain;

/// Stores per-texture configuration data: textures, scales, grass sprites, grass colors, grass toggles.
/// Port of Yugen's MarchingSquaresTextureList.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyTextureList {
    base: Base<Resource>,

    // 15 terrain textures (slots 1-15)
    #[export]
    pub texture_1: Option<Gd<Texture2D>>,
    #[export]
    pub texture_2: Option<Gd<Texture2D>>,
    #[export]
    pub texture_3: Option<Gd<Texture2D>>,
    #[export]
    pub texture_4: Option<Gd<Texture2D>>,
    #[export]
    pub texture_5: Option<Gd<Texture2D>>,
    #[export]
    pub texture_6: Option<Gd<Texture2D>>,
    #[export]
    pub texture_7: Option<Gd<Texture2D>>,
    #[export]
    pub texture_8: Option<Gd<Texture2D>>,
    #[export]
    pub texture_9: Option<Gd<Texture2D>>,
    #[export]
    pub texture_10: Option<Gd<Texture2D>>,
    #[export]
    pub texture_11: Option<Gd<Texture2D>>,
    #[export]
    pub texture_12: Option<Gd<Texture2D>>,
    #[export]
    pub texture_13: Option<Gd<Texture2D>>,
    #[export]
    pub texture_14: Option<Gd<Texture2D>>,
    #[export]
    pub texture_15: Option<Gd<Texture2D>>,

    // 15 texture scales
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

    // 6 grass sprites (slots 1-6)
    #[export]
    pub grass_sprite_1: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_2: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_3: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_4: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_5: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_6: Option<Gd<Texture2D>>,

    // 6 grass/ground colors (slots 1-6)
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4706, 0.3176, 1.0))]
    pub grass_color_1: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3216, 0.4824, 0.3843, 1.0))]
    pub grass_color_2: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3725, 0.4235, 0.2941, 1.0))]
    pub grass_color_3: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4745, 0.2549, 1.0))]
    pub grass_color_4: Color,
    #[export]
    #[init(val = Color::from_rgba(0.2902, 0.4941, 0.3647, 1.0))]
    pub grass_color_5: Color,
    #[export]
    #[init(val = Color::from_rgba(0.4431, 0.4471, 0.3647, 1.0))]
    pub grass_color_6: Color,

    // 5 has_grass toggles (slots 2-6; slot 1 always has grass)
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
/// Port of Yugen's MarchingSquaresTexturePreset.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyTexturePreset {
    base: Base<Resource>,

    #[export]
    #[init(val = GString::from("New Preset"))]
    pub preset_name: GString,

    #[export]
    pub textures: Option<Gd<PixyTextureList>>,
}

#[godot_api]
impl PixyTexturePreset {
    /// Save the current terrain texture settings into this preset.
    #[func]
    pub fn save_from_terrain(&mut self, terrain: Gd<PixyTerrain>) {
        let t = terrain.bind();

        let mut list = if let Some(ref existing) = self.textures {
            existing.clone()
        } else {
            Gd::<PixyTextureList>::default()
        };

        {
            let mut l = list.bind_mut();

            // Textures
            l.texture_1 = t.ground_texture.clone();
            l.texture_2 = t.texture_2.clone();
            l.texture_3 = t.texture_3.clone();
            l.texture_4 = t.texture_4.clone();
            l.texture_5 = t.texture_5.clone();
            l.texture_6 = t.texture_6.clone();
            l.texture_7 = t.texture_7.clone();
            l.texture_8 = t.texture_8.clone();
            l.texture_9 = t.texture_9.clone();
            l.texture_10 = t.texture_10.clone();
            l.texture_11 = t.texture_11.clone();
            l.texture_12 = t.texture_12.clone();
            l.texture_13 = t.texture_13.clone();
            l.texture_14 = t.texture_14.clone();
            l.texture_15 = t.texture_15.clone();

            // Scales
            l.scale_1 = t.texture_scale_1;
            l.scale_2 = t.texture_scale_2;
            l.scale_3 = t.texture_scale_3;
            l.scale_4 = t.texture_scale_4;
            l.scale_5 = t.texture_scale_5;
            l.scale_6 = t.texture_scale_6;
            l.scale_7 = t.texture_scale_7;
            l.scale_8 = t.texture_scale_8;
            l.scale_9 = t.texture_scale_9;
            l.scale_10 = t.texture_scale_10;
            l.scale_11 = t.texture_scale_11;
            l.scale_12 = t.texture_scale_12;
            l.scale_13 = t.texture_scale_13;
            l.scale_14 = t.texture_scale_14;
            l.scale_15 = t.texture_scale_15;

            // Grass sprites
            l.grass_sprite_1 = t.grass_sprite.clone();
            l.grass_sprite_2 = t.grass_sprite_tex_2.clone();
            l.grass_sprite_3 = t.grass_sprite_tex_3.clone();
            l.grass_sprite_4 = t.grass_sprite_tex_4.clone();
            l.grass_sprite_5 = t.grass_sprite_tex_5.clone();
            l.grass_sprite_6 = t.grass_sprite_tex_6.clone();

            // Ground colors
            l.grass_color_1 = t.ground_color;
            l.grass_color_2 = t.ground_color_2;
            l.grass_color_3 = t.ground_color_3;
            l.grass_color_4 = t.ground_color_4;
            l.grass_color_5 = t.ground_color_5;
            l.grass_color_6 = t.ground_color_6;

            // Has grass
            l.has_grass_2 = t.tex2_has_grass;
            l.has_grass_3 = t.tex3_has_grass;
            l.has_grass_4 = t.tex4_has_grass;
            l.has_grass_5 = t.tex5_has_grass;
            l.has_grass_6 = t.tex6_has_grass;
        }

        self.textures = Some(list);
    }

    /// Load this preset's texture settings into the terrain.
    #[func]
    pub fn load_into_terrain(&self, terrain: Gd<PixyTerrain>) {
        let Some(ref list_gd) = self.textures else {
            godot_warn!("PixyTexturePreset: no texture list to load");
            return;
        };
        let l = list_gd.bind();
        let mut terrain = terrain;
        let mut t = terrain.bind_mut();

        // Textures
        t.ground_texture = l.texture_1.clone();
        t.texture_2 = l.texture_2.clone();
        t.texture_3 = l.texture_3.clone();
        t.texture_4 = l.texture_4.clone();
        t.texture_5 = l.texture_5.clone();
        t.texture_6 = l.texture_6.clone();
        t.texture_7 = l.texture_7.clone();
        t.texture_8 = l.texture_8.clone();
        t.texture_9 = l.texture_9.clone();
        t.texture_10 = l.texture_10.clone();
        t.texture_11 = l.texture_11.clone();
        t.texture_12 = l.texture_12.clone();
        t.texture_13 = l.texture_13.clone();
        t.texture_14 = l.texture_14.clone();
        t.texture_15 = l.texture_15.clone();

        // Scales
        t.texture_scale_1 = l.scale_1;
        t.texture_scale_2 = l.scale_2;
        t.texture_scale_3 = l.scale_3;
        t.texture_scale_4 = l.scale_4;
        t.texture_scale_5 = l.scale_5;
        t.texture_scale_6 = l.scale_6;
        t.texture_scale_7 = l.scale_7;
        t.texture_scale_8 = l.scale_8;
        t.texture_scale_9 = l.scale_9;
        t.texture_scale_10 = l.scale_10;
        t.texture_scale_11 = l.scale_11;
        t.texture_scale_12 = l.scale_12;
        t.texture_scale_13 = l.scale_13;
        t.texture_scale_14 = l.scale_14;
        t.texture_scale_15 = l.scale_15;

        // Grass sprites
        t.grass_sprite = l.grass_sprite_1.clone();
        t.grass_sprite_tex_2 = l.grass_sprite_2.clone();
        t.grass_sprite_tex_3 = l.grass_sprite_3.clone();
        t.grass_sprite_tex_4 = l.grass_sprite_4.clone();
        t.grass_sprite_tex_5 = l.grass_sprite_5.clone();
        t.grass_sprite_tex_6 = l.grass_sprite_6.clone();

        // Ground colors
        t.ground_color = l.grass_color_1;
        t.ground_color_2 = l.grass_color_2;
        t.ground_color_3 = l.grass_color_3;
        t.ground_color_4 = l.grass_color_4;
        t.ground_color_5 = l.grass_color_5;
        t.ground_color_6 = l.grass_color_6;

        // Has grass
        t.tex2_has_grass = l.has_grass_2;
        t.tex3_has_grass = l.has_grass_3;
        t.tex4_has_grass = l.has_grass_4;
        t.tex5_has_grass = l.has_grass_5;
        t.tex6_has_grass = l.has_grass_6;

        // Sync shader
        t.force_batch_update();
    }
}
