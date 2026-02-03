use godot::prelude::*;

mod brush;
mod brush_preview;
mod chunk;
mod chunk_manager;
mod editor_plugin;
mod lod;
mod mesh_extraction;
mod mesh_postprocess;
mod mesh_worker;
mod noise_field;
mod terrain;
mod terrain_modifications;
mod texture_layer;
mod undo;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
