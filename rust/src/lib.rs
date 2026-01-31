use godot::prelude::*;

mod box_geometry;
mod chunk;
mod chunk_manager;
mod editor_plugin;
mod lod;
mod mesh_extraction;
mod mesh_worker;
mod noise_field;
mod terrain;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
