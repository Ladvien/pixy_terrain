use godot::prelude::*;

mod chunk;
mod chunk_manager;
mod lod;
mod mesh_extraction;
mod mesh_worker;
mod noise_field;
mod terrain;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
