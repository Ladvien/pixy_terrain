use std::collections::HashMap;

use godot::classes::{Engine, Node3D, ShaderMaterial, Texture2D};
use godot::prelude::*;

use crate::chunk::PixyTerrainChunk;
use crate::marching_squares::MergeMode;

/// Main terrain manager node. Manages chunks, exports terrain settings, and syncs shader uniforms.
/// Port of Yugen's MarchingSquaresTerrain (Node3D).
#[derive(GodotClass)]
#[class(base=Node3D, init, tool)]
#[allow(clippy::approx_constant)]
pub struct PixyTerrain {
    base: Base<Node3D>,

    // ═══════════════════════════════════════════
    // Core Settings
    // ═══════════════════════════════════════════
    /// Total height values in X and Z direction, and total height range (Y).
    #[export]
    #[init(val = Vector3i::new(33, 32, 33))]
    pub dimensions: Vector3i,

    /// XZ unit size of each cell.
    #[export]
    #[init(val = Vector2::new(2.0, 2.0))]
    pub cell_size: Vector2,

    /// Blend mode: 0 = smooth, 1 = hard edge, 2 = hard with blend.
    #[export]
    #[init(val = 0)]
    pub blend_mode: i32,

    /// Height threshold that determines where walls begin on the terrain mesh.
    #[export]
    #[init(val = 0.0)]
    pub wall_threshold: f32,

    /// Noise used to generate initial heightmap. If None, terrain starts flat.
    #[export]
    pub noise_hmap: Option<Gd<godot::classes::Noise>>,

    /// Extra collision layer for terrain chunks (9-32).
    #[export]
    #[init(val = 9)]
    pub extra_collision_layer: i32,

    /// Ridge threshold for grass exclusion and ridge texture detection.
    #[export]
    #[init(val = 1.0)]
    pub ridge_threshold: f32,

    /// Ledge threshold for grass exclusion.
    #[export]
    #[init(val = 0.25)]
    pub ledge_threshold: f32,

    /// Whether ridge vertices use wall texture instead of ground texture.
    #[export]
    #[init(val = false)]
    pub use_ridge_texture: bool,

    /// Merge mode index: 0=Cubic, 1=Polyhedron, 2=RoundedPolyhedron, 3=SemiRound, 4=Spherical.
    #[export]
    #[init(val = 1)]
    pub merge_mode: i32,

    // ═══════════════════════════════════════════
    // Texture Settings (15 texture slots)
    // ═══════════════════════════════════════════
    #[export]
    pub ground_texture: Option<Gd<Texture2D>>,
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

    // ═══════════════════════════════════════════
    // Per-Texture UV Scales
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_1: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_2: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_3: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_4: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_5: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_6: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_7: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_8: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_9: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_10: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_11: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_12: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_13: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_14: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_15: f32,

    // ═══════════════════════════════════════════
    // Ground Colors (6 slots matching texture slots 1-6)
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4706, 0.3176, 1.0))]
    pub ground_color: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3216, 0.4824, 0.3843, 1.0))]
    pub ground_color_2: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3725, 0.4235, 0.2941, 1.0))]
    pub ground_color_3: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4745, 0.2549, 1.0))]
    pub ground_color_4: Color,
    #[export]
    #[init(val = Color::from_rgba(0.2902, 0.4941, 0.3647, 1.0))]
    pub ground_color_5: Color,
    #[export]
    #[init(val = Color::from_rgba(0.4431, 0.4471, 0.3647, 1.0))]
    pub ground_color_6: Color,

    // ═══════════════════════════════════════════
    // Grass Settings
    // ═══════════════════════════════════════════
    #[export]
    pub grass_sprite: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_2: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_3: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_4: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_5: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_6: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = 0)]
    pub animation_fps: i32,
    #[export]
    #[init(val = 3)]
    pub grass_subdivisions: i32,
    #[export]
    #[init(val = Vector2::new(1.0, 1.0))]
    pub grass_size: Vector2,

    #[export]
    #[init(val = true)]
    pub tex2_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex3_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex4_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex5_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex6_has_grass: bool,

    /// Default wall texture slot (0-15).
    #[export]
    #[init(val = 5)]
    pub default_wall_texture: i32,

    // ═══════════════════════════════════════════
    // Internal State (not exported)
    // ═══════════════════════════════════════════
    pub terrain_material: Option<Gd<ShaderMaterial>>,

    pub is_batch_updating: bool,

    /// Map of chunk coordinates → chunk node.
    #[init(val = HashMap::new())]
    chunks: HashMap<[i32; 2], Gd<PixyTerrainChunk>>,
}

#[godot_api]
impl INode3D for PixyTerrain {
    fn enter_tree(&mut self) {
        if !Engine::singleton().is_editor_hint() {
            return;
        }

        // Deferred initialization to ensure tree is ready
        self.base_mut().call_deferred("_deferred_enter_tree", &[]);
    }
}

#[godot_api]
impl PixyTerrain {
    #[func]
    fn _deferred_enter_tree(&mut self) {
        if !Engine::singleton().is_editor_hint() {
            return;
        }

        // Discover existing chunk children
        self.chunks.clear();
        let children = self.base().get_children();
        for i in 0..children.len() {
            let Some(child): Option<Gd<Node>> = children.get(i) else {
                continue;
            };
            if let Ok(chunk) = child.try_cast::<PixyTerrainChunk>() {
                let coords = chunk.bind().chunk_coords;
                self.chunks.insert([coords.x, coords.y], chunk);
            }
        }

        // Initialize all discovered chunks
        let chunk_keys: Vec<[i32; 2]> = self.chunks.keys().cloned().collect();
        for key in chunk_keys {
            if let Some(chunk) = self.chunks.get(&key) {
                let mut chunk = chunk.clone();
                let terrain_ref = self.to_gd();
                chunk.bind_mut().set_terrain_ref(terrain_ref);
                chunk.bind_mut().initialize_terrain(true);
            }
        }
    }

    /// Regenerate the entire terrain: clear all chunks, create a single chunk at (0,0).
    #[func]
    pub fn regenerate(&mut self) {
        godot_print!("PixyTerrain: regenerate()");
        self.clear();
        self.add_new_chunk(0, 0);
    }

    /// Remove all chunks.
    #[func]
    pub fn clear(&mut self) {
        godot_print!("PixyTerrain: clear()");
        let keys: Vec<[i32; 2]> = self.chunks.keys().cloned().collect();
        for key in keys {
            self.remove_chunk(key[0], key[1]);
        }
    }

    /// Check if a chunk exists at the given coordinates.
    #[func]
    pub fn has_chunk(&self, x: i32, z: i32) -> bool {
        self.chunks.contains_key(&[x, z])
    }

    /// Create a new chunk at the given chunk coordinates, copying shared edges from neighbors.
    #[func]
    pub fn add_new_chunk(&mut self, chunk_x: i32, chunk_z: i32) {
        let chunk_coords = Vector2i::new(chunk_x, chunk_z);
        let mut new_chunk = Gd::<PixyTerrainChunk>::from_init_fn(PixyTerrainChunk::new_with_base);

        new_chunk.set_name(&format!("Chunk ({}, {})", chunk_x, chunk_z));
        {
            let mut chunk_bind = new_chunk.bind_mut();
            chunk_bind.chunk_coords = chunk_coords;
            chunk_bind.merge_mode = self.merge_mode;
        }

        // Add to tree and initialize
        self.add_chunk_internal(chunk_coords, new_chunk.clone(), false);

        // Copy shared edges from adjacent chunks
        let dim = self.dimensions;

        // Left neighbor: copy rightmost column → new chunk leftmost column
        if let Some(left) = self.chunks.get(&[chunk_x - 1, chunk_z]).cloned() {
            let left_bind = left.bind();
            let mut new_bind = new_chunk.bind_mut();
            for z in 0..dim.z {
                if let Some(h) = left_bind.get_height_at(dim.x - 1, z) {
                    new_bind.set_height_at(0, z, h);
                }
            }
        }

        // Right neighbor
        if let Some(right) = self.chunks.get(&[chunk_x + 1, chunk_z]).cloned() {
            let right_bind = right.bind();
            let mut new_bind = new_chunk.bind_mut();
            for z in 0..dim.z {
                if let Some(h) = right_bind.get_height_at(0, z) {
                    new_bind.set_height_at(dim.x - 1, z, h);
                }
            }
        }

        // Up neighbor: copy bottom row → new chunk top row
        if let Some(up) = self.chunks.get(&[chunk_x, chunk_z - 1]).cloned() {
            let up_bind = up.bind();
            let mut new_bind = new_chunk.bind_mut();
            for x in 0..dim.x {
                if let Some(h) = up_bind.get_height_at(x, dim.z - 1) {
                    new_bind.set_height_at(x, 0, h);
                }
            }
        }

        // Down neighbor
        if let Some(down) = self.chunks.get(&[chunk_x, chunk_z + 1]).cloned() {
            let down_bind = down.bind();
            let mut new_bind = new_chunk.bind_mut();
            for x in 0..dim.x {
                if let Some(h) = down_bind.get_height_at(x, 0) {
                    new_bind.set_height_at(x, dim.z - 1, h);
                }
            }
        }

        // Generate mesh
        new_chunk.bind_mut().regenerate_mesh();
    }

    /// Remove a chunk and free it.
    #[func]
    pub fn remove_chunk(&mut self, x: i32, z: i32) {
        if let Some(mut chunk) = self.chunks.remove(&[x, z]) {
            chunk.queue_free();
        }
    }

    /// Remove a chunk from the tree without freeing it (for undo/redo).
    #[func]
    pub fn remove_chunk_from_tree(&mut self, x: i32, z: i32) {
        if let Some(mut chunk) = self.chunks.remove(&[x, z]) {
            self.base_mut().remove_child(&chunk);
            chunk.set_owner(Gd::null_arg());
        }
    }

    /// Get the merge threshold for the current merge mode.
    #[func]
    pub fn get_merge_threshold(&self) -> f32 {
        MergeMode::from_index(self.merge_mode).threshold()
    }
}

impl PixyTerrain {
    /// Internal: add a chunk to the tree and register it.
    fn add_chunk_internal(
        &mut self,
        coords: Vector2i,
        mut chunk: Gd<PixyTerrainChunk>,
        regenerate: bool,
    ) {
        self.chunks.insert([coords.x, coords.y], chunk.clone());

        {
            let mut chunk_bind = chunk.bind_mut();
            chunk_bind.chunk_coords = coords;
        }

        self.base_mut().add_child(&chunk);

        // Position the chunk in world space
        let dim = self.dimensions;
        let cell = self.cell_size;
        let pos = Vector3::new(
            coords.x as f32 * ((dim.x - 1) as f32 * cell.x),
            0.0,
            coords.y as f32 * ((dim.z - 1) as f32 * cell.y),
        );
        chunk.set_position(pos);

        // Set owner for editor persistence
        if Engine::singleton().is_editor_hint() {
            if let Some(mut editor) = Engine::singleton().get_singleton("EditorInterface") {
                let scene_root = editor.call("get_edited_scene_root", &[]);
                if let Ok(root) = scene_root.try_to::<Gd<Node>>() {
                    Self::set_owner_recursive(&mut chunk.clone().upcast::<Node>(), &root);
                }
            }
        }

        // Set terrain reference and initialize
        let terrain_ref = self.to_gd();
        chunk.bind_mut().set_terrain_ref(terrain_ref);
        chunk.bind_mut().initialize_terrain(regenerate);

        godot_print!("PixyTerrain: Added chunk at ({}, {})", coords.x, coords.y);
    }

    fn set_owner_recursive(node: &mut Gd<Node>, owner: &Gd<Node>) {
        node.set_owner(owner);
        let children = node.get_children();
        for i in 0..children.len() {
            let Some(mut child): Option<Gd<Node>> = children.get(i) else {
                continue;
            };
            Self::set_owner_recursive(&mut child, owner);
        }
    }
}
