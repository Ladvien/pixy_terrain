use godot::classes::Resource;
use godot::prelude::*;

/// Quick paint preset: applies ground texture + wall texture + grass toggle in a single stroke.
/// Port of Yugen's MarchingSquaresQuickPaint.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyQuickPaint {
    base: Base<Resource>,

    /// User-visible name for this quick paint preset.
    #[export]
    #[init(val = GString::from("New Paint"))]
    pub paint_name: GString,

    /// Ground texture slot (0-15, maps to color encoding).
    #[export]
    #[init(val = 0)]
    pub ground_texture_slot: i32,

    /// Wall texture slot (0-15, maps to color encoding).
    #[export]
    #[init(val = 0)]
    pub wall_texture_slot: i32,

    /// Whether grass is enabled for this paint preset.
    #[export]
    #[init(val = true)]
    pub has_grass: bool,
}

#[godot_api]
impl PixyQuickPaint {
    /// Get the vertex color pair for the ground texture slot.
    #[func]
    pub fn get_ground_colors(&self) -> Array<Color> {
        let (c0, c1) = crate::marching_squares::texture_index_to_colors(self.ground_texture_slot);
        let mut arr = Array::new();
        arr.push(c0);
        arr.push(c1);
        arr
    }

    /// Get the vertex color pair for the wall texture slot.
    #[func]
    pub fn get_wall_colors(&self) -> Array<Color> {
        let (c0, c1) = crate::marching_squares::texture_index_to_colors(self.wall_texture_slot);
        let mut arr = Array::new();
        arr.push(c0);
        arr.push(c1);
        arr
    }
}
