use godot::prelude::*;

mod chunk;
mod editor_plugin;
mod gizmo;
mod grass_planter;
mod marching_squares;
mod quick_paint;
mod terrain;
mod texture_preset;

#[cfg(test)]
mod shader_validation;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
