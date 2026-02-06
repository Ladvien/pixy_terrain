# Pixy Terrain — Part 10: Terrain Manager — Chunk Operations

**Series:** Reconstructing Pixy Terrain
**Part:** 10 of 18
**Previous:** 2026-02-06-terrain-manager-materials-settings-09.md
**Status:** Complete

## What We're Building

The chunk management system — how the terrain creates, removes, and maintains chunks. Also: the `apply_composite_pattern()` method that the editor's undo/redo system uses to replay terrain modifications, and the preset save/load workflow.

## What You'll Have After This

A fully functional `PixyTerrain` that can add chunks at any grid coordinate, copy shared edge heights from neighbors, remove chunks, and apply batched undo/redo operations. The terrain manager is now complete — combined with the material pipeline from Part 09, it can create and render terrain.

## Prerequisites

- Part 09 completed (terrain material pipeline, `PixyTerrain` struct with all exports)

## Steps

### Step 1: Add basic chunk operations

**Why:** The terrain needs methods to add, remove, and query chunks. These are `#[func]` methods callable from both Rust and GDScript.

**File:** `rust/src/terrain.rs` (add to the `#[godot_api] impl PixyTerrain` block, after `_deferred_enter_tree`)

```rust
    /// Regenerate the entire terrain: clear all chunks, create a single chunk at (0,0).
    #[func]
    pub fn regenerate(&mut self) {
        godot_print!("PixyTerrain: regenerate()");
        self.ensure_terrain_material();
        self.ensure_grass_material();
        self.force_batch_update();
        self.force_grass_material_update();
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

    /// Get a chunk handle by coordinates (returns None if chunk doesn't exist).
    #[func]
    pub fn get_chunk(&self, x: i32, z: i32) -> Option<Gd<PixyTerrainChunk>> {
        self.chunks.get(&[x, z]).cloned()
    }

    /// Get all chunk coordinate keys as a PackedVector2Array.
    #[func]
    pub fn get_chunk_keys(&self) -> PackedVector2Array {
        let mut arr = PackedVector2Array::new();
        for key in self.chunks.keys() {
            arr.push(Vector2::new(key[0] as f32, key[1] as f32));
        }
        arr
    }

    /// Get the merge threshold for the current merge mode.
    #[func]
    pub fn get_merge_threshold(&self) -> f32 {
        MergeMode::from_index(self.merge_mode).threshold()
    }
```

**What's happening:**
- `clear()` collects keys first, then removes — you can't iterate a HashMap and mutate it simultaneously.
- `remove_chunk()` calls `queue_free()` — the node is deleted next frame. For immediate operations.
- `remove_chunk_from_tree()` removes the node from the tree but doesn't free it. This is for undo/redo: the node stays in memory so it can be re-added. `set_owner(Gd::null_arg())` clears the owner reference — in gdext, `Gd::null_arg()` creates a null Gd value for functions that expect "no owner".
- `get_chunk_keys()` returns `PackedVector2Array` because Godot can't directly serialize Rust HashMaps. The editor plugin reads this to know which chunks exist.

### Step 2: Implement `add_new_chunk()` with edge copying

**Why:** When adding a chunk adjacent to existing chunks, the shared edge (border row/column of height values) must be copied from the neighbor. Without this, adjacent chunks would have height discontinuities at their borders, creating visible seams.

**File:** `rust/src/terrain.rs` (continue in the `#[godot_api]` block)

```rust
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
```

**What's happening:**

Edge copying ensures seamless terrain across chunk boundaries:

| Direction | Neighbor column/row | New chunk column/row |
|---|---|---|
| Left | rightmost column (x = dim.x - 1) | leftmost column (x = 0) |
| Right | leftmost column (x = 0) | rightmost column (x = dim.x - 1) |
| Up | bottom row (z = dim.z - 1) | top row (z = 0) |
| Down | top row (z = 0) | bottom row (z = dim.z - 1) |

The `.cloned()` on `self.chunks.get()` clones the `Gd` smart pointer (not the underlying chunk data). This allows us to `bind()` the neighbor chunk and `bind_mut()` the new chunk simultaneously — you can't hold two borrows into the same HashMap entry, but you can hold borrows into different entries after cloning the Gd handle.

After copying edges, `regenerate_mesh()` generates the mesh with correct boundary heights.

### Step 3: Implement `add_chunk_internal()`

**Why:** The internal helper handles the mechanics of adding a chunk to the scene tree, positioning it, setting the owner for editor persistence, and initializing it with terrain configs.

**File:** `rust/src/terrain.rs` (add to the `impl PixyTerrain` block — non-godot_api)

```rust
    fn add_chunk_internal(
        &mut self,
        coords: Vector2i,
        mut chunk: Gd<PixyTerrainChunk>,
        regenerate: bool,
    ) {
        let terrain_config = self.make_terrain_config();
        let grass_config = self.make_grass_config();
        let noise = self.noise_hmap.clone();
        let material = self.terrain_material.clone();

        self.chunks.insert([coords.x, coords.y], chunk.clone());

        {
            let mut chunk_bind = chunk.bind_mut();
            chunk_bind.chunk_coords = coords;
            chunk_bind.set_terrain_config(terrain_config);
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

        chunk
            .bind_mut()
            .initialize_terrain(regenerate, noise, material, grass_config);

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
```

**What's happening:**

Chunk positioning: each chunk is offset by `(dim - 1) * cell_size` in world space. With `dim.x = 33` and `cell_size.x = 2.0`, a chunk is `32 * 2.0 = 64` units wide. Chunk (1, 0) is at X = 64, chunk (2, 0) at X = 128.

The owner setup is critical for Godot editor persistence:
1. `Engine::get_singleton("EditorInterface")` gets the editor API.
2. `editor.call("get_edited_scene_root", &[])` gets the root of the currently edited scene.
3. `set_owner_recursive()` sets the owner on the chunk AND all its children (like the GrassPlanter). Without setting owner, saved scenes would lose chunk nodes on reload.
4. This uses `call()` with a string method name because `EditorInterface` isn't directly available as a typed class in gdext — it's accessed through the singleton registry.

### Step 4: Implement `apply_composite_pattern()`

**Why:** The editor's undo/redo system stores terrain modifications as `VarDictionary` patterns. When undo or redo triggers, this method replays the pattern — applying height, color, and mask changes to the appropriate chunks and regenerating their meshes.

**File:** `rust/src/terrain.rs` (add to the `#[godot_api] impl PixyTerrain` block)

```rust
    /// Apply a composite pattern action. Called by undo/redo.
    /// `patterns` is a VarDictionary with keys: "height", "color_0", "color_1",
    /// "wall_color_0", "wall_color_1", "grass_mask".
    /// Each value is Dict<Vector2i(chunk), Dict<Vector2i(cell), value>>.
    #[func]
    pub fn apply_composite_pattern(&mut self, patterns: VarDictionary) {
        let mut affected_chunks: HashMap<[i32; 2], Gd<PixyTerrainChunk>> = HashMap::new();

        let keys_in_order = [
            "wall_color_0",
            "wall_color_1",
            "height",
            "grass_mask",
            "color_0",
            "color_1",
        ];

        for &key in &keys_in_order {
            let Some(outer_variant) = patterns.get(key) else {
                continue;
            };
            let outer_dict: VarDictionary = outer_variant.to();

            let chunk_entries: Vec<(Vector2i, VarDictionary)> = outer_dict
                .iter_shared()
                .map(|(k, v)| (k.to::<Vector2i>(), v.to::<VarDictionary>()))
                .collect();

            for (chunk_coords, cell_dict) in chunk_entries {
                let Some(mut chunk) = self.get_chunk(chunk_coords.x, chunk_coords.y) else {
                    continue;
                };

                affected_chunks
                    .entry([chunk_coords.x, chunk_coords.y])
                    .or_insert_with(|| chunk.clone());

                let cell_entries: Vec<(Vector2i, Variant)> = cell_dict
                    .iter_shared()
                    .map(|(k, v)| (k.to::<Vector2i>(), v.clone()))
                    .collect();

                for (cell, cell_value) in cell_entries {
                    let mut c = chunk.bind_mut();
                    match key {
                        "height" => {
                            let h: f32 = cell_value.to();
                            c.draw_height(cell.x, cell.y, h);
                        }
                        "color_0" => {
                            let color: Color = cell_value.to();
                            c.draw_color_0(cell.x, cell.y, color);
                        }
                        "color_1" => {
                            let color: Color = cell_value.to();
                            c.draw_color_1(cell.x, cell.y, color);
                        }
                        "wall_color_0" => {
                            let color: Color = cell_value.to();
                            c.draw_wall_color_0(cell.x, cell.y, color);
                        }
                        "wall_color_1" => {
                            let color: Color = cell_value.to();
                            c.draw_wall_color_1(cell.x, cell.y, color);
                        }
                        "grass_mask" => {
                            let mask: Color = cell_value.to();
                            c.draw_grass_mask(cell.x, cell.y, mask);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Regenerate mesh once per affected chunk
        for (_, mut chunk) in affected_chunks {
            chunk.bind_mut().regenerate_mesh();
        }
    }
```

**What's happening:**

The composite pattern structure is:
```
{
  "height": {
    Vector2i(chunk_x, chunk_z): {
      Vector2i(cell_x, cell_z): f32_height,
      ...
    },
    ...
  },
  "color_0": {
    Vector2i(chunk): {
      Vector2i(cell): Color,
      ...
    },
  },
  ...
}
```

Two-phase iteration:
1. **Collect snapshot** from `iter_shared()` into a Vec. This is necessary because `VarDictionary::iter_shared()` returns references that would conflict with the mutable `bind_mut()` calls in the inner loop.
2. **Apply changes** by dispatching to the appropriate `draw_*` method based on the key name.

Apply order matters: wall colors are set before height, and ground colors after. This ensures that when height changes cause new wall geometry, the wall colors are already in place.

The `affected_chunks` HashMap tracks which chunks were modified. After all changes are applied, each affected chunk regenerates its mesh exactly once — even if changes span multiple data types in the same chunk.

### Step 5: Add preset save/load and utility methods

**Why:** The texture preset system lets artists save and restore all texture settings as a resource. The remaining utility methods complete the terrain's public API.

**File:** `rust/src/terrain.rs` (add to the `#[godot_api] impl PixyTerrain` block)

```rust
    /// Save current terrain texture settings to the current_texture_preset.
    #[func]
    pub fn save_to_preset(&mut self) {
        if self.current_texture_preset.is_none() {
            let preset = Gd::<crate::texture_preset::PixyTexturePreset>::default();
            self.current_texture_preset = Some(preset);
        }

        if let Some(ref mut preset) = self.current_texture_preset {
            let mut list_gd = {
                let p = preset.bind();
                if let Some(ref existing) = p.textures {
                    existing.clone()
                } else {
                    Gd::<crate::texture_preset::PixyTextureList>::default()
                }
            };

            {
                let mut l = list_gd.bind_mut();
                l.texture_1 = self.ground_texture.clone();
                l.texture_2 = self.texture_2.clone();
                l.texture_3 = self.texture_3.clone();
                l.texture_4 = self.texture_4.clone();
                l.texture_5 = self.texture_5.clone();
                l.texture_6 = self.texture_6.clone();
                l.texture_7 = self.texture_7.clone();
                l.texture_8 = self.texture_8.clone();
                l.texture_9 = self.texture_9.clone();
                l.texture_10 = self.texture_10.clone();
                l.texture_11 = self.texture_11.clone();
                l.texture_12 = self.texture_12.clone();
                l.texture_13 = self.texture_13.clone();
                l.texture_14 = self.texture_14.clone();
                l.texture_15 = self.texture_15.clone();
                l.scale_1 = self.texture_scale_1;
                l.scale_2 = self.texture_scale_2;
                l.scale_3 = self.texture_scale_3;
                l.scale_4 = self.texture_scale_4;
                l.scale_5 = self.texture_scale_5;
                l.scale_6 = self.texture_scale_6;
                l.scale_7 = self.texture_scale_7;
                l.scale_8 = self.texture_scale_8;
                l.scale_9 = self.texture_scale_9;
                l.scale_10 = self.texture_scale_10;
                l.scale_11 = self.texture_scale_11;
                l.scale_12 = self.texture_scale_12;
                l.scale_13 = self.texture_scale_13;
                l.scale_14 = self.texture_scale_14;
                l.scale_15 = self.texture_scale_15;
                l.grass_sprite_1 = self.grass_sprite.clone();
                l.grass_sprite_2 = self.grass_sprite_tex_2.clone();
                l.grass_sprite_3 = self.grass_sprite_tex_3.clone();
                l.grass_sprite_4 = self.grass_sprite_tex_4.clone();
                l.grass_sprite_5 = self.grass_sprite_tex_5.clone();
                l.grass_sprite_6 = self.grass_sprite_tex_6.clone();
                l.grass_color_1 = self.ground_color;
                l.grass_color_2 = self.ground_color_2;
                l.grass_color_3 = self.ground_color_3;
                l.grass_color_4 = self.ground_color_4;
                l.grass_color_5 = self.ground_color_5;
                l.grass_color_6 = self.ground_color_6;
                l.has_grass_2 = self.tex2_has_grass;
                l.has_grass_3 = self.tex3_has_grass;
                l.has_grass_4 = self.tex4_has_grass;
                l.has_grass_5 = self.tex5_has_grass;
                l.has_grass_6 = self.tex6_has_grass;
            }

            preset.bind_mut().textures = Some(list_gd);
            godot_print!("PixyTerrain: Saved texture settings to preset");
        }
    }

    /// Load texture settings from the current_texture_preset.
    #[func]
    pub fn load_from_preset(&mut self) {
        let Some(ref preset) = self.current_texture_preset else {
            godot_warn!("PixyTerrain: No preset assigned to load from");
            return;
        };

        let p = preset.bind();
        let Some(ref list_gd) = p.textures else {
            godot_warn!("PixyTerrain: Preset has no texture list to load");
            return;
        };
        let l = list_gd.bind();

        self.ground_texture = l.texture_1.clone();
        self.texture_2 = l.texture_2.clone();
        self.texture_3 = l.texture_3.clone();
        self.texture_4 = l.texture_4.clone();
        self.texture_5 = l.texture_5.clone();
        self.texture_6 = l.texture_6.clone();
        self.texture_7 = l.texture_7.clone();
        self.texture_8 = l.texture_8.clone();
        self.texture_9 = l.texture_9.clone();
        self.texture_10 = l.texture_10.clone();
        self.texture_11 = l.texture_11.clone();
        self.texture_12 = l.texture_12.clone();
        self.texture_13 = l.texture_13.clone();
        self.texture_14 = l.texture_14.clone();
        self.texture_15 = l.texture_15.clone();
        self.texture_scale_1 = l.scale_1;
        self.texture_scale_2 = l.scale_2;
        self.texture_scale_3 = l.scale_3;
        self.texture_scale_4 = l.scale_4;
        self.texture_scale_5 = l.scale_5;
        self.texture_scale_6 = l.scale_6;
        self.texture_scale_7 = l.scale_7;
        self.texture_scale_8 = l.scale_8;
        self.texture_scale_9 = l.scale_9;
        self.texture_scale_10 = l.scale_10;
        self.texture_scale_11 = l.scale_11;
        self.texture_scale_12 = l.scale_12;
        self.texture_scale_13 = l.scale_13;
        self.texture_scale_14 = l.scale_14;
        self.texture_scale_15 = l.scale_15;
        self.grass_sprite = l.grass_sprite_1.clone();
        self.grass_sprite_tex_2 = l.grass_sprite_2.clone();
        self.grass_sprite_tex_3 = l.grass_sprite_3.clone();
        self.grass_sprite_tex_4 = l.grass_sprite_4.clone();
        self.grass_sprite_tex_5 = l.grass_sprite_5.clone();
        self.grass_sprite_tex_6 = l.grass_sprite_6.clone();
        self.ground_color = l.grass_color_1;
        self.ground_color_2 = l.grass_color_2;
        self.ground_color_3 = l.grass_color_3;
        self.ground_color_4 = l.grass_color_4;
        self.ground_color_5 = l.grass_color_5;
        self.ground_color_6 = l.grass_color_6;
        self.tex2_has_grass = l.has_grass_2;
        self.tex3_has_grass = l.has_grass_3;
        self.tex4_has_grass = l.has_grass_4;
        self.tex5_has_grass = l.has_grass_5;
        self.tex6_has_grass = l.has_grass_6;

        // Drop borrows before calling methods on self
        drop(l);
        drop(p);

        self.force_batch_update();
        self.force_grass_material_update();
        godot_print!("PixyTerrain: Loaded texture settings from preset");
    }

    /// Ensure all texture slots have sensible defaults.
    #[func]
    pub fn ensure_textures(&mut self) {
        self.ensure_terrain_material();
        self.force_batch_update();
    }

    /// Regenerate grass on all chunks.
    #[func]
    pub fn regenerate_all_grass(&mut self) {
        let chunk_keys: Vec<[i32; 2]> = self.chunks.keys().cloned().collect();
        for key in chunk_keys {
            if let Some(chunk) = self.chunks.get(&key) {
                let mut chunk = chunk.clone();
                chunk.bind_mut().regenerate_mesh();
            }
        }
    }
```

**What's happening:**

**Preset save** follows a borrow-dance pattern:
1. Bind the preset to check if it has a texture list → clone the list Gd handle.
2. Drop the preset bind.
3. Bind the texture list mutably to write all field values.
4. Drop the list bind.
5. Bind the preset again to set the updated list.

This dance avoids Rust's restriction on holding multiple borrows into nested Gd objects.

**Preset load** has a similar pattern, but with an explicit `drop(l); drop(p)` before calling `self.force_batch_update()`. Without the drops, the borrows on `l` (texture list) and `p` (preset) would conflict with the `&mut self` needed by `force_batch_update()`.

## Verify

```bash
cd rust && cargo build
```

The terrain manager is now feature-complete. You can:
1. Add a `PixyTerrain` node to a Godot scene
2. Call `regenerate()` to create a chunk at (0,0) with the default or noise-generated heightmap
3. Add more chunks with `add_new_chunk()` — edges sync automatically
4. Modify terrain data through chunk `draw_*` methods
5. Save/load texture presets

## What You Learned

- **Edge copying**: Adjacent chunks share border height values. When adding a new chunk, copy the shared edge from each existing neighbor.
- **Composite pattern dictionary**: Undo/redo stores terrain changes as nested `VarDictionary<chunk → Dictionary<cell → value>>`. Snapshot iteration into Vecs before modifying data.
- **Borrow dance for nested Gd**: Bind outer → read inner → clone inner handle → drop outer → bind inner → modify → drop inner → rebind outer to set.
- **Explicit `drop()` for borrow release**: When you need to call `&mut self` methods after borrowing through exported `Gd` handles, explicit drops release the borrows.
- **`Gd::null_arg()`**: Creates a null Gd for functions like `set_owner()` that accept an optional owner.

## Stubs Introduced

- None

## Stubs Resolved

- [x] Chunk management methods — `add_new_chunk`, `remove_chunk`, `apply_composite_pattern` all implemented
