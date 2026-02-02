use godot::prelude::*;

mod editor_plugin;
mod mesh_builder;
mod terrain;
mod tile_data;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
