use godot::prelude::*;

mod chunk;
mod chunk_manager;
mod debug_log;
mod editor_plugin;
mod lod;
mod mesh_extraction;
mod mesh_postprocess;
mod mesh_worker;
mod noise_field;
mod terrain;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
