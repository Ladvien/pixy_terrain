use godot::prelude::*;
use godot::classes::{Mesh, MeshInstance3D, IMeshInstance3D};

use crate::tile_data::{TileData, TileGrid};
use crate::mesh_builder::MeshBuilder;

#[derive(GodotClass)]
#[class(tool, base=MeshInstance3D, init)]
pub struct PixyTerrain {
    base: Base<MeshInstance3D>,

    /// Size of each tile/voxel in world units
    #[export]
    #[init(val = 1.0)]
    pub voxel_size: f32,

    /// Show debug grid wireframe
    #[export]
    #[init(val = false)]
    pub debug_show_grid: bool,

    /// The sparse tile grid storage (uses Default::default())
    tile_grid: TileGrid,

    /// Flag to track if mesh needs regeneration (uses Default::default())
    dirty: bool,
}

#[godot_api]
impl PixyTerrain {
    /// Set a tile at the given grid position
    /// tile_id of 0 removes the tile (empty/air)
    #[func]
    pub fn set_tile(&mut self, x: i32, y: i32, z: i32, tile_id: i32) {
        self.set_tile_full(x, y, z, tile_id, 0, 0);
    }

    /// Set a tile with full parameters (rotation and flip flags)
    #[func]
    pub fn set_tile_full(&mut self, x: i32, y: i32, z: i32, tile_id: i32, rotation: i32, flip_flags: i32) {
        let tile = TileData {
            tile_id: tile_id.max(0) as u16,
            rotation: (rotation.max(0) as u8) % 4,
            flip_flags: (flip_flags.max(0) as u8) & 0b111,
        };

        self.tile_grid.set_tile(x, y, z, tile);
        self.dirty = true;

        // Auto-regenerate mesh
        self.regenerate();
    }

    /// Get the tile_id at the given position (0 if empty)
    #[func]
    pub fn get_tile(&self, x: i32, y: i32, z: i32) -> i32 {
        self.tile_grid
            .get_tile(x, y, z)
            .map(|t| t.tile_id as i32)
            .unwrap_or(0)
    }

    /// Check if a tile exists at the given position
    #[func]
    pub fn has_tile(&self, x: i32, y: i32, z: i32) -> bool {
        self.tile_grid.has_tile(x, y, z)
    }

    /// Remove a tile at the given position
    #[func]
    pub fn remove_tile(&mut self, x: i32, y: i32, z: i32) {
        self.tile_grid.remove_tile(x, y, z);
        self.dirty = true;
        self.regenerate();
    }

    /// Clear all tiles
    #[func]
    pub fn clear_all(&mut self) {
        self.tile_grid.clear();
        self.dirty = true;
        self.regenerate();
    }

    /// Get the total number of tiles
    #[func]
    pub fn get_tile_count(&self) -> i32 {
        self.tile_grid.tile_count() as i32
    }

    /// Regenerate the mesh from tile data
    #[func]
    pub fn regenerate(&mut self) {
        let builder = MeshBuilder::new(self.voxel_size);

        if let Some(mesh) = builder.build_mesh(&self.tile_grid) {
            // Set material if mesh doesn't have one
            if mesh.surface_get_material(0).is_none() {
                let material = MeshBuilder::create_default_material();
                self.base_mut().set_surface_override_material(0, &material);
            }
            self.base_mut().set_mesh(&mesh);
        } else {
            // No tiles - clear mesh
            self.base_mut().set_mesh(Gd::<Mesh>::null_arg());
        }

        self.dirty = false;
        godot_print!("PixyTerrain: regenerated mesh with {} tiles", self.tile_grid.tile_count());
    }

    /// Get the bounding box of all tiles as a Dictionary
    /// Returns empty dict if no tiles exist
    #[func]
    pub fn get_bounds(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();

        if let Some(((min_x, min_y, min_z), (max_x, max_y, max_z))) = self.tile_grid.bounds() {
            dict.set("min_x", min_x);
            dict.set("min_y", min_y);
            dict.set("min_z", min_z);
            dict.set("max_x", max_x);
            dict.set("max_y", max_y);
            dict.set("max_z", max_z);
        }

        dict
    }

    /// Fill a region with tiles (useful for testing)
    #[func]
    pub fn fill_region(&mut self, min_x: i32, min_y: i32, min_z: i32, max_x: i32, max_y: i32, max_z: i32, tile_id: i32) {
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                for z in min_z..=max_z {
                    let tile = TileData::new(tile_id.max(0) as u16);
                    self.tile_grid.set_tile(x, y, z, tile);
                }
            }
        }
        self.dirty = true;
        self.regenerate();
    }

    // Deprecated methods for backward compatibility
    #[func]
    fn clear(&mut self) {
        self.clear_all();
    }

    /// Create a test floor (7x7 red tiles at y=0)
    #[func]
    pub fn create_test_floor(&mut self) {
        godot_print!("PixyTerrain: Creating test floor");
        // Create a 7x7 floor at y=0 with tile_id 1 (red)
        for x in -3..=3 {
            for z in -3..=3 {
                let tile = TileData::new(1); // Red tile
                self.tile_grid.set_tile(x, 0, z, tile);
            }
        }
        self.dirty = true;
        self.regenerate();
    }

    /// Create a test tower with varying colors
    #[func]
    pub fn create_test_tower(&mut self) {
        godot_print!("PixyTerrain: Creating test tower");
        // Create a colorful tower at origin
        for y in 0..5 {
            let tile_id = (y as u16) + 1; // Different color per level
            let tile = TileData::new(tile_id);
            self.tile_grid.set_tile(0, y, 0, tile);
        }
        // Add some surrounding tiles at different heights
        for i in 0..4 {
            let offsets = [(1, 0), (0, 1), (-1, 0), (0, -1)];
            let (dx, dz) = offsets[i];
            let tile = TileData::new((i as u16) + 2);
            self.tile_grid.set_tile(dx, i as i32, dz, tile);
        }
        self.dirty = true;
        self.regenerate();
    }
}

#[godot_api]
impl IMeshInstance3D for PixyTerrain {
    fn ready(&mut self) {
        godot_print!("PixyTerrain ready - voxel_size: {}", self.voxel_size);
    }
}
