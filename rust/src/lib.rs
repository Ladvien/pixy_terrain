use godot::prelude::*;

mod chunk;
mod editor_plugin;
mod marching_squares;
mod terrain;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
