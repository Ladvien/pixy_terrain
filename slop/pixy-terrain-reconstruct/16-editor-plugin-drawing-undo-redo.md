# Part 16 -- Editor Plugin -- Drawing & Undo/Redo

**Series:** Reconstructing Pixy Terrain
**Part:** 16 of 18
**Previous:** 2026-02-06-editor-plugin-input-routing-15.md (Editor Plugin -- UI & Input)
**Status:** Complete

## What We're Building

The drawing subsystem of the editor plugin -- the code that converts mouse actions into terrain modifications. This covers the full pipeline from "brush position calculated, mouse button pressed" through to "undo/redo action committed to Godot's EditorUndoRedoManager." The drawing system handles seven distinct paint modes (Height, Level, Smooth, Bridge, GrassMask, VertexPaint, DebugBrush) plus chunk add/remove operations, all routed through a unified composite pattern dictionary that supports full undo/redo.

This is the largest single section of `editor_plugin.rs` (lines ~2283-3484) and the heart of the editor experience. Every other part of the plugin -- input routing, UI panels, gizmo visualization -- exists to feed data into or display results from this drawing pipeline.

## What You'll Have After This

A fully operational drawing system with:
- A gizmo state snapshot that the gizmo renderer reads without holding borrows on the plugin
- Cross-chunk brush patterns with distance-based falloff (smoothstep for round, Chebyshev for square)
- A 4-phase composite pattern pipeline that computes do/undo dictionaries for 6 data channels (height, color_0, color_1, wall_color_0, wall_color_1, grass_mask)
- Cross-chunk edge propagation with blend factors for seamless chunk borders
- Wall color expansion for height-altering modes
- QuickPaint integration that applies wall + ground + grass alongside height changes
- Full undo/redo via Godot's EditorUndoRedoManager with callbacks on the terrain node (not the plugin)
- Chunk management undo/redo for add/remove operations

## Prerequisites

- Part 15 completed (editor plugin with UI panels, input routing, `forward_3d_gui_input`)
- Part 10 completed (`PixyTerrain` with `apply_composite_pattern()`, `add_new_chunk()`, `remove_chunk()`, `remove_chunk_from_tree()`)
- Part 07 completed (`PixyTerrainChunk` with `draw_height()`, `draw_color_0()`, etc.)
- Part 03 completed (`texture_index_to_colors()` in `marching_squares.rs`)
- Part 11 completed (`PixyQuickPaint` resource type)
- Part 14 completed (`GizmoState` struct in `gizmo.rs`)

## Steps

### Step 1: `get_gizmo_state()` -- Snapshot brush state for gizmo rendering

**Why:** The gizmo plugin needs to know the current brush position, size, mode, and draw pattern to render the brush preview circle, the pattern overlay, and the height preview. But the gizmo's `redraw()` method is called by Godot at an arbitrary time -- we cannot hold a mutable borrow on the plugin while the gizmo is rendering. The solution is a snapshot: `get_gizmo_state()` copies all relevant state into a plain struct that the gizmo can read without any borrow conflicts.

Add this method to the `impl PixyTerrainPlugin` block (the private, non-`#[godot_api]` impl):

```rust
/// Return a snapshot of the current brush/drawing state for gizmo rendering.
pub fn get_gizmo_state(&self) -> GizmoState {
    GizmoState {
        mode: self.mode,
        brush_type: self.brush_type,
        brush_position: self.brush_position,
        brush_size: self.brush_size,
        terrain_hovered: self.terrain_hovered,
        flatten: self.flatten,
        draw_height: self.draw_height,
        draw_pattern: self.current_draw_pattern.clone(),
        is_setting: self.is_setting,
        draw_height_set: self.draw_height_set,
        is_drawing: self.is_drawing,
    }
}
```

**What's happening:**

The `GizmoState` struct (defined in `gizmo.rs`, covered in Part 14) is a plain data container:

```rust
pub struct GizmoState {
    pub mode: TerrainToolMode,
    pub brush_type: BrushType,
    pub brush_position: Vector3,
    pub brush_size: f32,
    pub terrain_hovered: bool,
    pub flatten: bool,
    pub draw_height: f32,
    pub draw_pattern: HashMap<[i32; 2], HashMap<[i32; 2], f32>>,
    pub is_setting: bool,
    pub draw_height_set: bool,
    pub is_drawing: bool,
}
```

The `draw_pattern` field is a clone of the plugin's `current_draw_pattern`. This is intentional -- the gizmo uses it to draw colored squares over each cell in the brush footprint. The clone is cheap during editing (typical patterns have a few dozen cells) and completely avoids borrow aliasing between the plugin and gizmo.

The `is_setting` and `draw_height_set` booleans tell the gizmo which phase of the two-click workflow the user is in. In phase 1 (paint), the gizmo draws a flat pattern overlay. In phase 2 (height adjustment), the gizmo draws the same pattern but elevated to `brush_position.y`, showing the user where the terrain will land when they click to confirm.

### Step 2: `update_gizmos()` -- Trigger gizmo redraw

**Why:** Godot does not automatically redraw gizmos every frame. The gizmo system is demand-driven: you must call `Node3D::update_gizmos()` on the terrain node to tell Godot to invoke the gizmo plugin's `redraw()` method. We call this at the end of every `forward_3d_gui_input` cycle so the brush preview tracks the mouse.

```rust
/// Trigger a gizmo redraw on the terrain node.
fn update_gizmos(&self) {
    if let Some(ref terrain) = self.current_terrain {
        if terrain.is_instance_valid() {
            let mut terrain_3d: Gd<godot::classes::Node3D> = terrain.clone().cast();
            terrain_3d.update_gizmos();
        }
    }
}
```

**What's happening:**

The plugin stores `current_terrain` as `Option<Gd<Node>>` (not `Gd<Node3D>`) because `edit()` receives a generic `Object`. We cast to `Node3D` here because `update_gizmos()` is defined on `Node3D`. The `clone()` before `cast()` is required by gdext -- `cast()` consumes the `Gd<T>`, and we need to keep the original reference alive in `self.current_terrain`.

The `is_instance_valid()` check guards against the terrain being freed between frames. This can happen during scene reloads or when the user deletes the terrain node while the plugin is active.

### Step 3: `call_terrain_method()`, `do_generate()`, `do_clear()` -- Terrain method helpers

**Why:** The plugin needs to call methods on the terrain node for generate and clear operations. These methods are exposed as `#[func]` on `PixyTerrain` and can be called dynamically via Godot's `Object::call()`. The `is_modifying` flag prevents the plugin from being hidden during a regeneration (Godot might call `make_visible(false)` if the terrain's class changes during the operation).

```rust
fn call_terrain_method(&mut self, method_name: &str) {
    if let Some(ref terrain) = self.current_terrain {
        if terrain.is_instance_valid() {
            let mut terrain_clone = terrain.clone();
            if terrain_clone.has_method(method_name) {
                self.is_modifying = true;
                terrain_clone.call(method_name, &[]);
                self.is_modifying = false;
            }
        }
    }
}

fn do_generate(&mut self) {
    self.call_terrain_method("regenerate");
    self.base_mut()
        .call_deferred("apply_collision_visibility_deferred", &[]);
}

fn do_clear(&mut self) {
    self.call_terrain_method("clear");
}
```

**What's happening:**

`call_terrain_method` uses Godot's dynamic dispatch (`Object::call()`) rather than casting to `PixyTerrain` and calling the method directly. This avoids a `bind_mut()` on the terrain, which would conflict with any existing borrows. The `has_method` guard prevents runtime errors if the method name is misspelled or doesn't exist.

`do_generate` also defers a collision visibility update. After regeneration, new chunks may have been created with `StaticBody3D` children that need their visibility synced to the editor's "Show Colliders" toggle. The deferred call ensures Godot has finished processing the regeneration before we iterate child nodes.

### Step 4: `set_vertex_colors()` -- Convert texture index to color pair

**Why:** The artist selects a texture slot (0-14) from a dropdown. The mesh system encodes textures as pairs of vertex colors (covered in Part 03). This method bridges the two representations.

```rust
/// Set vertex colors from a texture index (0-15).
fn set_vertex_colors(&mut self, idx: i32) {
    let (c0, c1) = marching_squares::texture_index_to_colors(idx);
    self.vertex_color_0 = c0;
    self.vertex_color_1 = c1;
    self.vertex_color_idx = idx;
}
```

**What's happening:**

`texture_index_to_colors()` (from Part 03 in `marching_squares.rs`) divides the index by 4 and takes the remainder to produce two `Color` values, each with exactly one non-zero RGBA channel:

```rust
pub fn texture_index_to_colors(idx: i32) -> (Color, Color) {
    let c0_channel = idx / 4;
    let c1_channel = idx % 4;

    let c0 = match c0_channel {
        0 => Color::from_rgba(1.0, 0.0, 0.0, 0.0),
        1 => Color::from_rgba(0.0, 1.0, 0.0, 0.0),
        2 => Color::from_rgba(0.0, 0.0, 1.0, 0.0),
        3 => Color::from_rgba(0.0, 0.0, 0.0, 1.0),
        _ => Color::from_rgba(1.0, 0.0, 0.0, 0.0),
    };
    let c1 = match c1_channel {
        0 => Color::from_rgba(1.0, 0.0, 0.0, 0.0),
        1 => Color::from_rgba(0.0, 1.0, 0.0, 0.0),
        2 => Color::from_rgba(0.0, 0.0, 1.0, 0.0),
        3 => Color::from_rgba(0.0, 0.0, 0.0, 1.0),
        _ => Color::from_rgba(1.0, 0.0, 0.0, 0.0),
    };
    (c0, c1)
}
```

The two colors are stored in CUSTOM0 and CUSTOM1 vertex attributes. The terrain shader uses the combination of these two 4-channel colors to encode up to 4x4 = 16 texture indices. Texture 0 is `(R, R)` = `(1,0,0,0), (1,0,0,0)`. Texture 5 is `(G, B)` = `(0,1,0,0), (0,0,1,0)`. This encoding was chosen by Yugen because it packs cleanly into two vertex color channels with no precision loss.

### Step 5: `initialize_draw_state()` -- The two-click workflow state machine

**Why:** Height editing uses a two-click workflow that differs from most paint tools. Understanding this workflow is critical to understanding the entire drawing pipeline.

The workflow has three phases:
1. **Paint phase** (`is_setting=true`, `draw_height_set=false`): The user clicks and optionally drags to build a brush pattern. Each mouse motion during the drag expands the pattern.
2. **Height phase** (`is_setting=true`, `draw_height_set=true`): The user releases the mouse button. The pattern locks. Mouse movement now adjusts the height preview via a vertical plane raycast (handled in `forward_3d_gui_input`).
3. **Apply phase**: The user clicks again. The pattern is applied with the chosen height, then everything resets.

This two-click workflow gives the artist precise control -- they paint an area first, then dial in the exact height by dragging up or down. Modes like Level, Smooth, GrassMask, and VertexPaint skip this workflow and use simpler click-drag-release semantics.

```rust
/// Initialize draw_height_set state (two-click workflow).
///
/// Two-click workflow:
/// 1. First click: `is_setting=true`, builds pattern, stays in paint phase (`draw_height_set=false`)
/// 2. Drag/motion: Pattern expands as user drags brush over terrain
/// 3. Release: Enter height adjustment mode (`draw_height_set=true`), pattern locked
/// 4. Mouse movement: Adjust height preview via vertical plane raycast
/// 5. Second click: Apply pattern and reset for next stroke
///
/// Special cases:
/// - Clicking on existing pattern: go directly to height mode (skip paint phase)
/// - ALT+click on pattern: isolate clicked cell for height adjustment
fn initialize_draw_state(
    &mut self,
    terrain: &Gd<PixyTerrain>,
    dim: Vector3i,
    cell_size: Vector2,
) {
    // Two-click workflow initialization:
    // Phase 1 (paint): is_setting=true, draw_height_set=false
    //   - Build pattern on initial click and during drag
    // Phase 2 (height): is_setting=true, draw_height_set=true (set on release)
    //   - Pattern locked, mouse movement adjusts height preview
    // Phase 3 (apply): second click applies and resets
    if self.is_setting && !self.draw_height_set {
        // DON'T set draw_height_set = true here!
        // This is the paint phase - we stay in this phase until mouse release.

        // Check if clicked cell is in the current pattern
        let pos = self.brush_position;
        let chunk_width = (dim.x - 1) as f32 * cell_size.x;
        let chunk_depth = (dim.z - 1) as f32 * cell_size.y;
        let cursor_chunk_x = (pos.x / chunk_width).floor() as i32;
        let cursor_chunk_z = (pos.z / chunk_depth).floor() as i32;

        let cursor_cell_x = ((pos.x + cell_size.x / 2.0) / cell_size.x
            - cursor_chunk_x as f32 * (dim.x - 1) as f32)
            .floor() as i32;
        let cursor_cell_z = ((pos.z + cell_size.y / 2.0) / cell_size.y
            - cursor_chunk_z as f32 * (dim.z - 1) as f32)
            .floor() as i32;

        let in_pattern = self
            .current_draw_pattern
            .get(&[cursor_chunk_x, cursor_chunk_z])
            .and_then(|cells| cells.get(&[cursor_cell_x, cursor_cell_z]))
            .is_some();

        let alt_held = Input::singleton().is_key_pressed(godot::global::Key::ALT);

        if !in_pattern && !alt_held {
            // Not on existing pattern -> clear pattern, build new one, stay in paint phase
            self.current_draw_pattern.clear();
            self.draw_height = pos.y;
            self.setting_start_position = pos;
            self.base_position = pos;
            // Build the pattern immediately at click position
            self.build_draw_pattern(terrain, dim, cell_size);
        } else {
            // On existing pattern -> go directly to height mode on this click
            // (second click on existing pattern)
            self.draw_height_set = true;
            if alt_held {
                // ALT: only drag the clicked cell
                let chunk_key = [cursor_chunk_x, cursor_chunk_z];
                let cell_key = [cursor_cell_x, cursor_cell_z];
                self.current_draw_pattern.clear();

                if let Some(chunk) = terrain.bind().get_chunk(cursor_chunk_x, cursor_chunk_z) {
                    let h = chunk
                        .bind()
                        .get_height(Vector2i::new(cursor_cell_x, cursor_cell_z));
                    let mut cells = HashMap::new();
                    cells.insert(cell_key, h as f64 as f32);
                    self.current_draw_pattern.insert(chunk_key, cells);
                }
                self.draw_height = pos.y;
            }
            self.setting_start_position = pos;
            self.base_position = pos;
        }
    }

    if self.is_drawing && !self.draw_height_set {
        self.draw_height_set = true;
        self.draw_height = self.brush_position.y;
    }
}
```

**What's happening:**

This function is called once at the start of a drawing interaction (from the mouse button press handler in `forward_3d_gui_input`). It decides how to initialize the draw state based on the current context.

**Cell coordinate calculation.** The cursor's world position is converted to chunk coordinates and cell-within-chunk coordinates. The `+ cell_size / 2.0` offset centers the calculation on the cell (rather than the cell's corner), matching Yugen's original behavior. The math is:
- `chunk_x = floor(pos.x / chunk_width)` where `chunk_width = (dim.x - 1) * cell_size.x`
- `cell_x = floor((pos.x + half_cell) / cell_size.x - chunk_x * (dim.x - 1))`

The `dim.x - 1` stride (not `dim.x`) reflects how marching squares work: a grid of `dim.x` vertices produces `dim.x - 1` cells. Adjacent chunks share their edge vertices, so the stride between chunk origins is `dim.x - 1`, not `dim.x`.

**Pattern-aware click detection.** If the user clicks on an existing pattern cell, the system skips the paint phase and goes directly to height adjustment. This lets artists paint a pattern, preview it, and then click on it again to adjust its height without rebuilding the pattern.

**ALT+click isolation.** If ALT is held while clicking on a pattern, the system isolates just the clicked cell. The entire pattern is cleared and replaced with a single-cell pattern containing the clicked cell's current height. This gives the artist per-cell height control.

**Drawing mode fallback.** The final `if self.is_drawing` block handles the simpler shift+click drawing mode used by Level, Smooth, GrassMask, and VertexPaint. These modes skip the two-click workflow and immediately start accumulating a pattern with `draw_height_set = true`.

### Step 6: `build_draw_pattern()` -- Brush footprint calculation

**Why:** Every drawing operation starts by computing which cells the brush covers and how strongly each cell is affected. The result is stored in `self.current_draw_pattern: HashMap<[i32;2], HashMap<[i32;2], f32>>` -- a nested map from `[chunk_x, chunk_z]` to `[cell_x, cell_z]` to falloff sample value (0.0 to 1.0).

```rust
/// Build the draw pattern based on current brush position and size.
fn build_draw_pattern(
    &mut self,
    terrain: &Gd<PixyTerrain>,
    dim: Vector3i,
    cell_size: Vector2,
) {
    let pos = self.brush_position;

    // Compute brush bounding box (with cell_size offset matching Yugen)
    let pos_tl = Vector2::new(
        pos.x + cell_size.x - self.brush_size / 2.0,
        pos.z + cell_size.y - self.brush_size / 2.0,
    );
    let pos_br = Vector2::new(
        pos.x + cell_size.x + self.brush_size / 2.0,
        pos.z + cell_size.y + self.brush_size / 2.0,
    );

    let chunk_width = (dim.x - 1) as f32 * cell_size.x;
    let chunk_depth = (dim.z - 1) as f32 * cell_size.y;

    let chunk_tl_x = (pos_tl.x / chunk_width).floor() as i32;
    let chunk_tl_z = (pos_tl.y / chunk_depth).floor() as i32;
    let chunk_br_x = (pos_br.x / chunk_width).floor() as i32;
    let chunk_br_z = (pos_br.y / chunk_depth).floor() as i32;

    let x_tl = (pos_tl.x / cell_size.x
        - chunk_tl_x as f32 * (dim.x - 1) as f32)
        .floor() as i32;
    let z_tl = (pos_tl.y / cell_size.y
        - chunk_tl_z as f32 * (dim.z - 1) as f32)
        .floor() as i32;
    let x_br = (pos_br.x / cell_size.x
        - chunk_br_x as f32 * (dim.x - 1) as f32)
        .floor() as i32;
    let z_br = (pos_br.y / cell_size.y
        - chunk_br_z as f32 * (dim.z - 1) as f32)
        .floor() as i32;

    // Max distance for brush type
    let half = self.brush_size / 2.0;
    let max_distance = match self.brush_type {
        BrushType::Round => half * half,
        BrushType::Square => half * half * 2.0,
    };

    for chunk_z in chunk_tl_z..=chunk_br_z {
        for chunk_x in chunk_tl_x..=chunk_br_x {
            if !terrain.bind().has_chunk(chunk_x, chunk_z) {
                continue;
            }

            let x_min = if chunk_x == chunk_tl_x { x_tl } else { 0 };
            let x_max = if chunk_x == chunk_br_x { x_br } else { dim.x };
            let z_min = if chunk_z == chunk_tl_z { z_tl } else { 0 };
            let z_max = if chunk_z == chunk_br_z { z_br } else { dim.z };

            for z in z_min..z_max {
                for x in x_min..x_max {
                    let world_x =
                        (chunk_x * (dim.x - 1) + x) as f32 * cell_size.x;
                    let world_z =
                        (chunk_z * (dim.z - 1) + z) as f32 * cell_size.y;

                    let dist_sq = (pos.x - world_x) * (pos.x - world_x)
                        + (pos.z - world_z) * (pos.z - world_z);

                    if dist_sq > max_distance {
                        continue;
                    }

                    let sample = if self.falloff {
                        let t = match self.brush_type {
                            BrushType::Round => {
                                ((max_distance - dist_sq) / max_distance)
                                    .clamp(0.0, 1.0)
                            }
                            BrushType::Square => {
                                let local_x = world_x - pos.x;
                                let local_z = world_z - pos.z;
                                let uv_x = local_x / (self.brush_size * 0.5);
                                let uv_z = local_z / (self.brush_size * 0.5);
                                let d = uv_x.abs().max(uv_z.abs());
                                1.0 - d.clamp(0.2, 1.0)
                            }
                        };
                        let t = t.clamp(0.001, 0.999);
                        // Smoothstep falloff
                        t * t * (3.0 - 2.0 * t)
                    } else {
                        1.0
                    };

                    // Accumulate: keep max sample per cell
                    let chunk_key = [chunk_x, chunk_z];
                    let cell_key = [x, z];
                    let cell_entry = self
                        .current_draw_pattern
                        .entry(chunk_key)
                        .or_default()
                        .entry(cell_key)
                        .or_insert(0.0);
                    if sample > *cell_entry {
                        *cell_entry = sample;
                    }
                }
            }
        }
    }
}
```

**What's happening:**

This function has four distinct stages.

**Stage 1: Bounding box calculation.** The brush center (`self.brush_position`) is expanded by `brush_size / 2.0` in all four directions to get a world-space bounding box. There is a deliberate `cell_size` offset added to the center before computing the bounds -- this matches Yugen's original GDScript and ensures the brush is centered on cell corners rather than cell centers. The bounding box corners are then converted to chunk coordinates (which chunk contains that corner) and cell coordinates (which cell within that chunk).

**Stage 2: Cross-chunk iteration.** The brush might span multiple chunks. We iterate over all chunks from `chunk_tl` to `chunk_br`. Within each chunk, we iterate over the cell range that falls within the bounding box. For edge chunks, this is the partial range from the bounding box edge to the chunk boundary. For interior chunks (if the brush is very large), this is the full chunk dimensions.

**Stage 3: Distance check and falloff.** For each candidate cell, we compute the squared distance from the brush center to the cell's world position. The distance metric differs by brush type:
- **Round**: Euclidean squared distance. `max_distance = half * half`. The test `dist_sq > max_distance` defines a circular brush.
- **Square**: The `max_distance` is doubled (`half * half * 2.0`) to ensure the diagonal corners of the square are included. The actual falloff uses Chebyshev distance (max of |dx|, |dz|) to create a square-shaped gradient.

If falloff is enabled, the raw distance ratio is passed through a smoothstep function: `t * t * (3.0 - 2.0 * t)`. This approximates the Curve resource Yugen used in GDScript. The result is clamped to `[0.001, 0.999]` to avoid exact zero or one, which would cause edge cases in the lerp calculations downstream.

If falloff is disabled (GrassMask, DebugBrush modes), every cell within the brush gets `sample = 1.0`.

**Stage 4: Max-sample accumulation.** The HashMap `entry` API is used to accumulate samples across multiple calls to `build_draw_pattern`. When the user drags the brush, the function is called repeatedly, and each call potentially adds new cells or updates existing ones. We keep the maximum sample per cell -- this ensures that a cell visited by the brush center (sample = 1.0) is not overwritten by a later pass where the brush edge barely touches it (sample = 0.1).

The `entry().or_default().entry().or_insert(0.0)` chain is idiomatic Rust for nested HashMaps: get-or-create the chunk map, then get-or-create the cell entry with a default of 0.0.

### Step 7: `draw_pattern()` -- The 4-phase composite pattern system

**Why:** This is the core drawing function. It converts the accumulated brush pattern into terrain modifications, wrapped in undo/redo dictionaries. The function is called when the user finishes a drawing action (second click in two-click mode, mouse release for continuous modes).

The function operates in four phases:
1. **Phase 1:** Compute per-cell do/undo values for each paint mode
2. **Phase 1.5:** QuickPaint integration (overlay wall + ground + grass)
3. **Phase 2:** Propagate values to adjacent chunk edges
4. **Phase 3:** Expand wall colors around height-modified cells
5. **Phase 4:** Bundle into composite dictionaries and register undo/redo

```rust
/// Apply the current draw pattern to the terrain.
#[allow(clippy::type_complexity)]
fn draw_pattern(
    &mut self,
    terrain: &Gd<PixyTerrain>,
    dim: Vector3i,
    cell_size: Vector2,
) {
    if self.current_draw_pattern.is_empty() {
        return;
    }

    // Snapshot the pattern (avoid borrow issues)
    let pattern_snapshot: Vec<([i32; 2], Vec<([i32; 2], f32)>)> = self
        .current_draw_pattern
        .iter()
        .map(|(k, v)| (*k, v.iter().map(|(ck, cv)| (*ck, *cv)).collect()))
        .collect();

    // Phase 1: Compute do/undo values per cell
    let mut do_height = VarDictionary::new();
    let mut undo_height = VarDictionary::new();
    let mut do_color_0 = VarDictionary::new();
    let mut undo_color_0 = VarDictionary::new();
    let mut do_color_1 = VarDictionary::new();
    let mut undo_color_1 = VarDictionary::new();
    let mut do_wall_color_0 = VarDictionary::new();
    let mut undo_wall_color_0 = VarDictionary::new();
    let mut do_wall_color_1 = VarDictionary::new();
    let mut undo_wall_color_1 = VarDictionary::new();
    let mut do_grass_mask = VarDictionary::new();
    let mut undo_grass_mask = VarDictionary::new();

    let mut first_chunk: Option<[i32; 2]> = None;

    // Compute global average for smooth mode (across ALL chunks in brush)
    let global_avg_height = if self.mode == TerrainToolMode::Smooth {
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for (chunk_key, cells) in &pattern_snapshot {
            if let Some(chunk) = terrain.bind().get_chunk(chunk_key[0], chunk_key[1]) {
                let c = chunk.bind();
                for &(cell_key, _) in cells {
                    sum += c.get_height(Vector2i::new(cell_key[0], cell_key[1]));
                    count += 1;
                }
            }
        }
        sum / count.max(1) as f32
    } else {
        0.0
    };

    for (chunk_key, cells) in &pattern_snapshot {
        if first_chunk.is_none() {
            first_chunk = Some(*chunk_key);
        }

        let chunk_coords = Vector2i::new(chunk_key[0], chunk_key[1]);
        let chunk = terrain.bind().get_chunk(chunk_key[0], chunk_key[1]);
        let Some(chunk) = chunk else { continue };

        match self.mode {
            TerrainToolMode::Smooth => {
                let mut do_chunk = VarDictionary::new();
                let mut undo_chunk = VarDictionary::new();

                for &(cell_key, sample) in cells {
                    let sample = sample.clamp(0.001, 0.999);
                    let cell_coords = Vector2i::new(cell_key[0], cell_key[1]);
                    let old_h = chunk.bind().get_height(cell_coords);
                    let f = sample * self.strength;
                    let new_h = lerp_f32(old_h, global_avg_height, f);
                    do_chunk.set(cell_coords, new_h);
                    undo_chunk.set(cell_coords, old_h);
                }

                do_height.set(chunk_coords, do_chunk);
                undo_height.set(chunk_coords, undo_chunk);
            }

            TerrainToolMode::DebugBrush => {
                for &(cell_key, _) in cells {
                    let c = chunk.bind();
                    let cell_coords = Vector2i::new(cell_key[0], cell_key[1]);
                    let h = c.get_height(cell_coords);
                    let col0 = c.get_color_0(cell_key[0], cell_key[1]);
                    let col1 = c.get_color_1(cell_key[0], cell_key[1]);
                    godot_print!(
                        "DEBUG: chunk ({},{}), cell ({},{}), h={:.3}, c0={:?}, c1={:?}",
                        chunk_key[0], chunk_key[1],
                        cell_key[0], cell_key[1],
                        h, col0, col1
                    );
                }
                continue; // Debug mode doesn't apply changes
            }

            _ => {
                let mut do_chunk = VarDictionary::new();
                let mut undo_chunk = VarDictionary::new();
                let mut do_chunk_cc = VarDictionary::new();
                let mut undo_chunk_cc = VarDictionary::new();

                for &(cell_key, sample) in cells {
                    let sample = sample.clamp(0.001, 0.999);
                    let cell_coords = Vector2i::new(cell_key[0], cell_key[1]);

                    match self.mode {
                        TerrainToolMode::GrassMask => {
                            let old = chunk
                                .bind()
                                .get_grass_mask_at(cell_key[0], cell_key[1]);
                            let new_mask = if self.should_mask_grass {
                                Color::from_rgba(0.0, 0.0, 0.0, 0.0)
                            } else {
                                Color::from_rgba(1.0, 0.0, 0.0, 0.0)
                            };
                            do_chunk.set(cell_coords, new_mask);
                            undo_chunk.set(cell_coords, old);
                        }

                        TerrainToolMode::Level => {
                            let old_h = chunk.bind().get_height(cell_coords);
                            let new_h = lerp_f32(old_h, self.height, sample);
                            do_chunk.set(cell_coords, new_h);
                            undo_chunk.set(cell_coords, old_h);
                        }

                        TerrainToolMode::Bridge => {
                            let b_end = Vector2::new(
                                self.brush_position.x,
                                self.brush_position.z,
                            );
                            let b_start = Vector2::new(
                                self.bridge_start_pos.x,
                                self.bridge_start_pos.z,
                            );
                            let bridge_length = b_end.distance_to(b_start);

                            if bridge_length < 0.5 || cells.len() < 3 {
                                continue;
                            }

                            // Cell to world-space
                            let mut global_x =
                                (chunk_key[0] * (dim.x - 1) + cell_key[0]) as f32
                                    * cell_size.x;
                            let global_z =
                                (chunk_key[1] * (dim.z - 1) + cell_key[1]) as f32
                                    * cell_size.y;

                            // Cross-chunk offset correction
                            if chunk_key[0] != self.bridge_start_chunk.x {
                                global_x +=
                                    (self.bridge_start_chunk.x - chunk_key[0]) as f32
                                        * 2.0
                                        * cell_size.x;
                            }

                            let global_cell = Vector2::new(global_x, global_z);
                            let bridge_dir = (b_end - b_start) / bridge_length;
                            let cell_vec = global_cell - b_start;
                            let linear_offset = cell_vec.dot(bridge_dir);
                            let mut progress =
                                (linear_offset / bridge_length).clamp(0.0, 1.0);

                            if self.ease_value != -1.0 {
                                progress = godot_ease(progress, self.ease_value);
                            }

                            let bridge_height = lerp_f32(
                                self.bridge_start_pos.y,
                                self.brush_position.y,
                                progress,
                            );

                            let old_h = chunk.bind().get_height(cell_coords);
                            do_chunk.set(cell_coords, bridge_height);
                            undo_chunk.set(cell_coords, old_h);
                        }

                        TerrainToolMode::VertexPaint => {
                            if self.paint_walls_mode {
                                let old_c0 = chunk
                                    .bind()
                                    .get_wall_color_0(cell_key[0], cell_key[1]);
                                let old_c1 = chunk
                                    .bind()
                                    .get_wall_color_1(cell_key[0], cell_key[1]);
                                do_chunk.set(cell_coords, self.vertex_color_0);
                                undo_chunk.set(cell_coords, old_c0);
                                do_chunk_cc.set(cell_coords, self.vertex_color_1);
                                undo_chunk_cc.set(cell_coords, old_c1);
                            } else {
                                let old_c0 = chunk
                                    .bind()
                                    .get_color_0(cell_key[0], cell_key[1]);
                                let old_c1 = chunk
                                    .bind()
                                    .get_color_1(cell_key[0], cell_key[1]);
                                do_chunk.set(cell_coords, self.vertex_color_0);
                                undo_chunk.set(cell_coords, old_c0);
                                do_chunk_cc.set(cell_coords, self.vertex_color_1);
                                undo_chunk_cc.set(cell_coords, old_c1);
                            }
                        }

                        // Height tool (default)
                        _ => {
                            let old_h = chunk.bind().get_height(cell_coords);
                            let new_h = if self.flatten {
                                lerp_f32(old_h, self.brush_position.y, sample)
                            } else {
                                let height_diff =
                                    self.brush_position.y - self.draw_height;
                                old_h + height_diff * sample
                            };
                            do_chunk.set(cell_coords, new_h);
                            undo_chunk.set(cell_coords, old_h);
                        }
                    }
                }

                // Store in appropriate dictionaries
                match self.mode {
                    TerrainToolMode::GrassMask => {
                        do_grass_mask.set(chunk_coords, do_chunk);
                        undo_grass_mask.set(chunk_coords, undo_chunk);
                    }
                    TerrainToolMode::VertexPaint => {
                        if self.paint_walls_mode {
                            do_wall_color_0.set(chunk_coords, do_chunk);
                            undo_wall_color_0.set(chunk_coords, undo_chunk);
                            do_wall_color_1.set(chunk_coords, do_chunk_cc);
                            undo_wall_color_1.set(chunk_coords, undo_chunk_cc);
                        } else {
                            do_color_0.set(chunk_coords, do_chunk);
                            undo_color_0.set(chunk_coords, undo_chunk);
                            do_color_1.set(chunk_coords, do_chunk_cc);
                            undo_color_1.set(chunk_coords, undo_chunk_cc);
                        }
                    }
                    _ => {
                        // Height modes: Brush, Level, Bridge
                        do_height.set(chunk_coords, do_chunk);
                        undo_height.set(chunk_coords, undo_chunk);
                    }
                }
            }
        }
    }
```

**What's happening in Phase 1:**

The function starts by snapshotting `self.current_draw_pattern` into a Vec of tuples. This snapshot is necessary because the function borrows `terrain` (to read chunk data) and also needs to reference the pattern, which lives on `self`. Snapshotting once avoids repeated borrow conflicts throughout the function body.

There are 12 `VarDictionary` instances -- a do/undo pair for each of the 6 data channels. `VarDictionary` is Godot's `Dictionary` type (recently renamed from the deprecated `Dictionary` alias). Using Godot dictionaries (rather than Rust HashMaps) is deliberate: these dictionaries will be passed to `apply_composite_pattern()` via the undo/redo system, which uses Godot's Variant-based method calls. The dictionaries are nested: the outer key is `Vector2i` (chunk coords), the inner key is `Vector2i` (cell coords), and the value depends on the channel (f32 for height, Color for colors/grass mask).

Each mode computes values differently:

- **Smooth**: Computes a global average height across ALL cells in the pattern (not per-chunk). Each cell lerps toward this average weighted by `sample * strength`. This flattens bumps while preserving the overall terrain level. The global average is pre-computed before the loop.

- **DebugBrush**: Prints cell data to the Godot console and continues without modifying any dictionaries. No undo/redo is registered.

- **GrassMask**: Sets each cell to either `(0,0,0,0)` (masked/no grass) or `(1,0,0,0)` (unmasked/has grass). The sample value is ignored -- grass mask is binary.

- **Level**: Lerps each cell from its current height toward `self.height` (the target level) weighted by the sample. Cells at the brush center (sample near 1.0) reach the target exactly; cells at the edge approach it partially.

- **Bridge**: Projects each cell onto the line from `bridge_start_pos` to `brush_position`, computing a progress value from 0.0 to 1.0. The height is then linearly interpolated between the start and end heights. If `ease_value != -1.0`, the progress is passed through `godot_ease()` to create curved slopes. The cross-chunk offset correction adjusts the cell's X coordinate when the bridge spans multiple chunks.

- **VertexPaint**: Stores `self.vertex_color_0` and `self.vertex_color_1` as the "do" values. Uses separate dictionaries for wall vs. floor painting based on `self.paint_walls_mode`. The `do_chunk_cc` ("cc" = complementary channel) stores the second color.

- **Height (default)**: Two sub-modes. In "flatten" mode, each cell lerps toward `brush_position.y` weighted by sample. In non-flatten mode, the height *delta* (`brush_position.y - draw_height`) is added to each cell, scaled by sample. This is the difference between "set terrain to this height" and "raise/lower terrain by this amount."

**Phase 1.5 -- QuickPaint integration:**

```rust
    // Phase 1.5: QuickPaint -- apply wall, ground, and grass patterns
    if let Some(ref qp) = self.current_quick_paint {
        let qp_bind = qp.bind();
        let wall_slot = qp_bind.wall_texture_slot;
        let ground_slot = qp_bind.ground_texture_slot;
        let has_grass = qp_bind.has_grass;
        drop(qp_bind);

        let (wall_c0, wall_c1) =
            marching_squares::texture_index_to_colors(wall_slot);
        let (ground_c0, ground_c1) =
            marching_squares::texture_index_to_colors(ground_slot);

        for (chunk_key, cells) in &pattern_snapshot {
            let chunk_coords = Vector2i::new(chunk_key[0], chunk_key[1]);
            let chunk = terrain.bind().get_chunk(chunk_key[0], chunk_key[1]);
            let Some(chunk) = chunk else { continue };

            let mut do_wc0_chunk = VarDictionary::new();
            let mut undo_wc0_chunk = VarDictionary::new();
            let mut do_wc1_chunk = VarDictionary::new();
            let mut undo_wc1_chunk = VarDictionary::new();
            let mut do_gc0_chunk = VarDictionary::new();
            let mut undo_gc0_chunk = VarDictionary::new();
            let mut do_gc1_chunk = VarDictionary::new();
            let mut undo_gc1_chunk = VarDictionary::new();
            let mut do_gm_chunk = VarDictionary::new();
            let mut undo_gm_chunk = VarDictionary::new();

            for &(cell_key, _) in cells {
                let cell = Vector2i::new(cell_key[0], cell_key[1]);
                let c = chunk.bind();

                // Wall colors
                undo_wc0_chunk.set(
                    cell,
                    c.get_wall_color_0(cell_key[0], cell_key[1]),
                );
                undo_wc1_chunk.set(
                    cell,
                    c.get_wall_color_1(cell_key[0], cell_key[1]),
                );
                do_wc0_chunk.set(cell, wall_c0);
                do_wc1_chunk.set(cell, wall_c1);

                // Ground colors
                undo_gc0_chunk.set(
                    cell,
                    c.get_color_0(cell_key[0], cell_key[1]),
                );
                undo_gc1_chunk.set(
                    cell,
                    c.get_color_1(cell_key[0], cell_key[1]),
                );
                do_gc0_chunk.set(cell, ground_c0);
                do_gc1_chunk.set(cell, ground_c1);

                // Grass mask
                undo_gm_chunk.set(
                    cell,
                    c.get_grass_mask_at(cell_key[0], cell_key[1]),
                );
                if has_grass {
                    do_gm_chunk.set(
                        cell,
                        Color::from_rgba(1.0, 1.0, 0.0, 0.0),
                    );
                } else {
                    do_gm_chunk.set(
                        cell,
                        Color::from_rgba(0.0, 0.0, 0.0, 0.0),
                    );
                }
            }

            // Merge into existing dicts
            do_wall_color_0.set(chunk_coords, do_wc0_chunk);
            undo_wall_color_0.set(chunk_coords, undo_wc0_chunk);
            do_wall_color_1.set(chunk_coords, do_wc1_chunk);
            undo_wall_color_1.set(chunk_coords, undo_wc1_chunk);
            do_color_0.set(chunk_coords, do_gc0_chunk);
            undo_color_0.set(chunk_coords, undo_gc0_chunk);
            do_color_1.set(chunk_coords, do_gc1_chunk);
            undo_color_1.set(chunk_coords, undo_gc1_chunk);
            do_grass_mask.set(chunk_coords, do_gm_chunk);
            undo_grass_mask.set(chunk_coords, undo_gm_chunk);
        }
```

**What's happening in Phase 1.5:**

When a `PixyQuickPaint` preset is active, every height-mode stroke also applies wall texture, ground texture, and grass mask alongside the height change. This saves the artist from switching between Height and VertexPaint modes for every stroke.

The QuickPaint preset's `wall_texture_slot` and `ground_texture_slot` integers are converted to color pairs. Then, for every cell in the pattern, the current wall colors, ground colors, and grass mask are captured for undo, and the preset's values are written for do. The sample value is ignored here -- QuickPaint is all-or-nothing per cell.

QuickPaint also runs its own wall color expansion loop (the nested `for dx in -1..=1 / for dz in -1..=1` block that follows in the source). This ensures that adjacent cells around the painted area get the QuickPaint's wall texture, matching the behavior of Phase 3's `expand_wall_colors()`. The code checks `do_wall_color_0.get(adj_chunk)` to avoid overwriting cells that are already in the wall pattern, then uses `get_or_create_dict()` to safely build the nested dictionary structure.

**Phase 2, 3, and 4 (completing draw_pattern):**

```rust
    // Phase 2: Cross-chunk edge propagation
    self.propagate_cross_chunk_edges(
        terrain,
        &pattern_snapshot,
        dim,
        &mut do_height,
        &mut undo_height,
        &mut do_color_0,
        &mut undo_color_0,
        &mut do_color_1,
        &mut undo_color_1,
        &mut do_wall_color_0,
        &mut undo_wall_color_0,
        &mut do_wall_color_1,
        &mut undo_wall_color_1,
        &mut do_grass_mask,
        &mut undo_grass_mask,
    );

    // Phase 3: Wall color expansion for height modes
    if self.current_quick_paint.is_none()
        && matches!(
            self.mode,
            TerrainToolMode::Height
                | TerrainToolMode::Level
                | TerrainToolMode::Smooth
                | TerrainToolMode::Bridge
        )
    {
        self.expand_wall_colors(
            terrain,
            dim,
            &do_height,
            &mut do_wall_color_0,
            &mut undo_wall_color_0,
            &mut do_wall_color_1,
            &mut undo_wall_color_1,
        );
    }

    // Phase 4: Build composite dictionaries and register undo/redo
    let mut do_patterns = VarDictionary::new();
    let mut undo_patterns = VarDictionary::new();

    if !do_height.is_empty() {
        do_patterns.set("height", do_height);
        undo_patterns.set("height", undo_height);
    }
    if !do_wall_color_0.is_empty() {
        do_patterns.set("wall_color_0", do_wall_color_0);
        undo_patterns.set("wall_color_0", undo_wall_color_0);
    }
    if !do_wall_color_1.is_empty() {
        do_patterns.set("wall_color_1", do_wall_color_1);
        undo_patterns.set("wall_color_1", undo_wall_color_1);
    }
    if !do_grass_mask.is_empty() {
        do_patterns.set("grass_mask", do_grass_mask);
        undo_patterns.set("grass_mask", undo_grass_mask);
    }
    if !do_color_0.is_empty() {
        do_patterns.set("color_0", do_color_0);
        undo_patterns.set("color_0", undo_color_0);
    }
    if !do_color_1.is_empty() {
        do_patterns.set("color_1", do_color_1);
        undo_patterns.set("color_1", undo_color_1);
    }

    if do_patterns.is_empty() {
        return;
    }

    let action_name = match self.mode {
        TerrainToolMode::Height => "terrain height",
        TerrainToolMode::Level => "terrain level",
        TerrainToolMode::Smooth => "terrain smooth",
        TerrainToolMode::Bridge => "terrain bridge",
        TerrainToolMode::GrassMask => "terrain grass mask",
        TerrainToolMode::VertexPaint => {
            if self.paint_walls_mode {
                "terrain wall paint"
            } else {
                "terrain vertex paint"
            }
        }
        _ => "terrain draw",
    };

    let terrain_node: Gd<Node> = terrain.clone().upcast();
    self.register_undo_redo(
        action_name,
        &terrain_node,
        do_patterns,
        undo_patterns,
    );
}
```

**What's happening in Phases 2-4:**

Phase 2 calls `propagate_cross_chunk_edges()` (covered in Step 8 below). Phase 3 calls `expand_wall_colors()` (covered in Step 9). Phase 3 is skipped when QuickPaint is active because QuickPaint handles its own wall color expansion in Phase 1.5.

Phase 4 bundles all non-empty channel dictionaries into a single composite `do_patterns` dictionary and a matching `undo_patterns` dictionary. The composite structure is:

```
{
    "height": { Vector2i(chunk) -> { Vector2i(cell) -> f32 } },
    "color_0": { Vector2i(chunk) -> { Vector2i(cell) -> Color } },
    "color_1": { Vector2i(chunk) -> { Vector2i(cell) -> Color } },
    "wall_color_0": { Vector2i(chunk) -> { Vector2i(cell) -> Color } },
    "wall_color_1": { Vector2i(chunk) -> { Vector2i(cell) -> Color } },
    "grass_mask": { Vector2i(chunk) -> { Vector2i(cell) -> Color } },
}
```

Only non-empty channels are included. This means a pure height operation produces a composite with just `"height"`, while a VertexPaint with QuickPaint produces a composite with up to 6 channels. The composite is passed to `register_undo_redo()` (Step 11), which commits it to Godot's undo/redo system.

### Step 8: `propagate_cross_chunk_edges()` -- Seamless chunk borders

**Why:** When the brush modifies cells at a chunk boundary, the adjacent chunk's shared edge must receive the same values. Without propagation, you get visible seams at chunk borders. This function copies (or blends) values from boundary cells to their counterparts on the neighboring chunk.

```rust
/// Propagate draw values to adjacent chunk edges for seamless borders.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn propagate_cross_chunk_edges(
    &self,
    terrain: &Gd<PixyTerrain>,
    pattern_snapshot: &[([i32; 2], Vec<([i32; 2], f32)>)],
    dim: Vector3i,
    do_height: &mut VarDictionary,
    undo_height: &mut VarDictionary,
    do_color_0: &mut VarDictionary,
    undo_color_0: &mut VarDictionary,
    do_color_1: &mut VarDictionary,
    undo_color_1: &mut VarDictionary,
    do_wall_color_0: &mut VarDictionary,
    undo_wall_color_0: &mut VarDictionary,
    do_wall_color_1: &mut VarDictionary,
    undo_wall_color_1: &mut VarDictionary,
    do_grass_mask: &mut VarDictionary,
    undo_grass_mask: &mut VarDictionary,
) {
    struct EdgeEntry {
        src_chunk: Vector2i,
        src_cell: Vector2i,
        adj_chunk: Vector2i,
        adj_cell: Vector2i,
        blend: f32,
    }

    let mut edges: Vec<EdgeEntry> = Vec::new();

    for (chunk_key, cells) in pattern_snapshot {
        for &(cell_key, sample) in cells {
            let sample = sample.clamp(0.001, 0.999);

            for cx in -1i32..=1 {
                for cz in -1i32..=1 {
                    if cx == 0 && cz == 0 {
                        continue;
                    }

                    let adj_chunk =
                        [chunk_key[0] + cx, chunk_key[1] + cz];
                    if !terrain
                        .bind()
                        .has_chunk(adj_chunk[0], adj_chunk[1])
                    {
                        continue;
                    }

                    let mut x = cell_key[0];
                    let mut z = cell_key[1];

                    if cx == -1 {
                        if x == 0 {
                            x = dim.x - 1;
                        } else {
                            continue;
                        }
                    } else if cx == 1 {
                        if x == dim.x - 1 {
                            x = 0;
                        } else {
                            continue;
                        }
                    }
                    if cz == -1 {
                        if z == 0 {
                            z = dim.z - 1;
                        } else {
                            continue;
                        }
                    } else if cz == 1 {
                        if z == dim.z - 1 {
                            z = 0;
                        } else {
                            continue;
                        }
                    }

                    let existing_higher = self
                        .current_draw_pattern
                        .get(&adj_chunk)
                        .and_then(|cells| cells.get(&[x, z]))
                        .is_some_and(|&s| s > sample);

                    if existing_higher {
                        continue;
                    }

                    edges.push(EdgeEntry {
                        src_chunk: Vector2i::new(
                            chunk_key[0],
                            chunk_key[1],
                        ),
                        src_cell: Vector2i::new(
                            cell_key[0],
                            cell_key[1],
                        ),
                        adj_chunk: Vector2i::new(
                            adj_chunk[0],
                            adj_chunk[1],
                        ),
                        adj_cell: Vector2i::new(x, z),
                        blend: 1.0,
                    });

                    // Inner-cell blend for height modes
                    if matches!(
                        self.mode,
                        TerrainToolMode::Height
                            | TerrainToolMode::Level
                            | TerrainToolMode::Smooth
                            | TerrainToolMode::Bridge
                    ) {
                        let inner_x = if cx == -1 {
                            x - 1
                        } else if cx == 1 {
                            x + 1
                        } else {
                            x
                        };
                        let inner_z = if cz == -1 {
                            z - 1
                        } else if cz == 1 {
                            z + 1
                        } else {
                            z
                        };

                        if inner_x >= 0
                            && inner_x < dim.x
                            && inner_z >= 0
                            && inner_z < dim.z
                        {
                            let already_in_pattern = self
                                .current_draw_pattern
                                .get(&[adj_chunk[0], adj_chunk[1]])
                                .and_then(|cells| {
                                    cells.get(&[inner_x, inner_z])
                                })
                                .is_some();
                            if !already_in_pattern {
                                edges.push(EdgeEntry {
                                    src_chunk: Vector2i::new(
                                        chunk_key[0],
                                        chunk_key[1],
                                    ),
                                    src_cell: Vector2i::new(
                                        cell_key[0],
                                        cell_key[1],
                                    ),
                                    adj_chunk: Vector2i::new(
                                        adj_chunk[0],
                                        adj_chunk[1],
                                    ),
                                    adj_cell: Vector2i::new(
                                        inner_x, inner_z,
                                    ),
                                    blend: 0.5,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Apply collected edges
    for edge in &edges {
        let adj_chunk_gd = terrain
            .bind()
            .get_chunk(edge.adj_chunk.x, edge.adj_chunk.y);

        match self.mode {
            TerrainToolMode::GrassMask => {
                Self::copy_dict_entry(
                    do_grass_mask,
                    edge.src_chunk,
                    edge.src_cell,
                    edge.adj_chunk,
                    edge.adj_cell,
                );
                if let Some(adj) = &adj_chunk_gd {
                    let restore = adj
                        .bind()
                        .get_grass_mask_at(
                            edge.adj_cell.x,
                            edge.adj_cell.y,
                        );
                    Self::set_nested_dict(
                        undo_grass_mask,
                        edge.adj_chunk,
                        edge.adj_cell,
                        restore.to_variant(),
                    );
                }
            }
            TerrainToolMode::VertexPaint if self.paint_walls_mode => {
                Self::copy_dict_entry(
                    do_wall_color_0,
                    edge.src_chunk,
                    edge.src_cell,
                    edge.adj_chunk,
                    edge.adj_cell,
                );
                Self::copy_dict_entry(
                    do_wall_color_1,
                    edge.src_chunk,
                    edge.src_cell,
                    edge.adj_chunk,
                    edge.adj_cell,
                );
                if let Some(adj) = &adj_chunk_gd {
                    Self::set_nested_dict(
                        undo_wall_color_0,
                        edge.adj_chunk,
                        edge.adj_cell,
                        adj.bind()
                            .get_wall_color_0(
                                edge.adj_cell.x,
                                edge.adj_cell.y,
                            )
                            .to_variant(),
                    );
                    Self::set_nested_dict(
                        undo_wall_color_1,
                        edge.adj_chunk,
                        edge.adj_cell,
                        adj.bind()
                            .get_wall_color_1(
                                edge.adj_cell.x,
                                edge.adj_cell.y,
                            )
                            .to_variant(),
                    );
                }
            }
            TerrainToolMode::VertexPaint => {
                Self::copy_dict_entry(
                    do_color_0,
                    edge.src_chunk,
                    edge.src_cell,
                    edge.adj_chunk,
                    edge.adj_cell,
                );
                Self::copy_dict_entry(
                    do_color_1,
                    edge.src_chunk,
                    edge.src_cell,
                    edge.adj_chunk,
                    edge.adj_cell,
                );
                if let Some(adj) = &adj_chunk_gd {
                    Self::set_nested_dict(
                        undo_color_0,
                        edge.adj_chunk,
                        edge.adj_cell,
                        adj.bind()
                            .get_color_0(
                                edge.adj_cell.x,
                                edge.adj_cell.y,
                            )
                            .to_variant(),
                    );
                    Self::set_nested_dict(
                        undo_color_1,
                        edge.adj_chunk,
                        edge.adj_cell,
                        adj.bind()
                            .get_color_1(
                                edge.adj_cell.x,
                                edge.adj_cell.y,
                            )
                            .to_variant(),
                    );
                }
            }
            _ => {
                // Height modes with blend factor
                if edge.blend >= 1.0 {
                    Self::copy_dict_entry(
                        do_height,
                        edge.src_chunk,
                        edge.src_cell,
                        edge.adj_chunk,
                        edge.adj_cell,
                    );
                } else if let Some(src_outer) =
                    do_height.get(edge.src_chunk)
                {
                    let src_dict: VarDictionary = src_outer.to();
                    if let Some(val) = src_dict.get(edge.src_cell) {
                        let src_h: f32 = val.to();
                        if let Some(adj) = &adj_chunk_gd {
                            let existing_h =
                                adj.bind().get_height(edge.adj_cell);
                            let blended =
                                lerp_f32(existing_h, src_h, edge.blend);
                            Self::set_nested_dict(
                                do_height,
                                edge.adj_chunk,
                                edge.adj_cell,
                                blended.to_variant(),
                            );
                        }
                    }
                }
                if let Some(adj) = &adj_chunk_gd {
                    let restore =
                        adj.bind().get_height(edge.adj_cell);
                    Self::set_nested_dict(
                        undo_height,
                        edge.adj_chunk,
                        edge.adj_cell,
                        restore.to_variant(),
                    );
                }
            }
        }
    }
}
```

**What's happening:**

This function uses a two-pass architecture: collect, then apply. The "collect" pass builds a `Vec<EdgeEntry>`, and the "apply" pass writes to the dictionaries. This pattern is forced by Rust's borrow checker -- we cannot iterate over the pattern (which borrows `self`) while also writing to dictionaries (which also borrows `self`). Collecting into a temporary vec breaks the borrow chain.

**The EdgeEntry struct** captures everything needed to propagate one edge:
- `src_chunk` / `src_cell`: where the value comes from
- `adj_chunk` / `adj_cell`: where the value goes
- `blend`: 1.0 for exact edge copies, 0.5 for inner transition cells

**Boundary detection.** For each cell in the pattern, we check all 8 neighbors (including diagonals). The coordinate remapping logic is:
- If `cx == -1` (checking left neighbor chunk) and `x == 0` (cell is at left edge), the corresponding cell on the adjacent chunk is `x = dim.x - 1` (its right edge). If `x != 0`, this cell is not at the boundary, so we skip it (`continue`).
- Same logic for `cx == 1` (right edge: `x = dim.x - 1` maps to `x = 0`), `cz == -1` (top edge), and `cz == 1` (bottom edge).

**Conflict resolution.** If the adjacent cell is already in the draw pattern with a higher sample value, we skip propagation. This prevents an edge propagation from overwriting a directly painted cell with a weaker value.

**Inner transition cells.** For height modes, we also push a half-blend entry for the cell one step further into the adjacent chunk (`inner_x = x + 1` for right-side propagation). This creates a 2-cell transition zone: the edge cell gets an exact copy, and the next cell gets a 50/50 blend between its current value and the source. This prevents hard height steps at chunk borders.

**Application pass.** Each edge is applied differently based on mode:
- **GrassMask**: Direct copy of the source cell's value to the adjacent cell
- **VertexPaint**: Copies both color_0 and color_1 (or wall_color_0 and wall_color_1)
- **Height modes**: Uses the blend factor. `blend >= 1.0` is an exact copy via `copy_dict_entry`. `blend < 1.0` lerps between the adjacent cell's existing height and the source height. The undo value is always the adjacent cell's current value.

### Step 9: `expand_wall_colors()` -- Uniform walls around height changes

**Why:** When you raise or lower terrain, new walls appear at the height transitions. These walls need a texture. Without explicit wall color expansion, they would inherit whatever default color the chunk was initialized with, creating visible mismatches. This function "paints" the default wall texture onto all cells adjacent to height-modified cells.

```rust
/// Expand wall colors to adjacent cells for height modification modes.
#[allow(clippy::too_many_arguments)]
fn expand_wall_colors(
    &mut self,
    terrain: &Gd<PixyTerrain>,
    dim: Vector3i,
    height_pattern: &VarDictionary,
    do_wall_0: &mut VarDictionary,
    undo_wall_0: &mut VarDictionary,
    do_wall_1: &mut VarDictionary,
    undo_wall_1: &mut VarDictionary,
) {
    let default_wall_tex = terrain.bind().default_wall_texture;
    let (vc0, vc1) =
        marching_squares::texture_index_to_colors(default_wall_tex);

    // Collect all cells in the height pattern
    let mut cells_to_process: Vec<(Vector2i, Vector2i)> = Vec::new();

    for (chunk_key, chunk_value) in height_pattern.iter_shared() {
        let chunk_coords: Vector2i = chunk_key.to();
        let cell_dict: VarDictionary = chunk_value.to();
        for (cell_key, _) in cell_dict.iter_shared() {
            let cell_coords: Vector2i = cell_key.to();
            cells_to_process.push((chunk_coords, cell_coords));
        }
    }

    for (chunk_coords, cell_coords) in &cells_to_process {
        for dx in -1i32..=1 {
            for dz in -1i32..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }

                let mut adj_x = cell_coords.x + dx;
                let mut adj_z = cell_coords.y + dz;
                let mut adj_chunk = *chunk_coords;

                if adj_x < 0 {
                    adj_chunk.x -= 1;
                    adj_x = dim.x - 1;
                } else if adj_x >= dim.x {
                    adj_chunk.x += 1;
                    adj_x = 0;
                }
                if adj_z < 0 {
                    adj_chunk.y -= 1;
                    adj_z = dim.z - 1;
                } else if adj_z >= dim.z {
                    adj_chunk.y += 1;
                    adj_z = 0;
                }

                if !terrain.bind().has_chunk(adj_chunk.x, adj_chunk.y) {
                    continue;
                }

                let adj_cell = Vector2i::new(adj_x, adj_z);

                // Skip if already in wall pattern
                if let Some(existing) = do_wall_0.get(adj_chunk) {
                    let d: VarDictionary = existing.to();
                    if d.contains_key(adj_cell) {
                        continue;
                    }
                }

                let adj_chunk_gd = terrain
                    .bind()
                    .get_chunk(adj_chunk.x, adj_chunk.y);
                let Some(adj_chunk_gd) = adj_chunk_gd else {
                    continue;
                };

                let old_wc0 =
                    adj_chunk_gd.bind().get_wall_color_0(adj_x, adj_z);
                let old_wc1 =
                    adj_chunk_gd.bind().get_wall_color_1(adj_x, adj_z);

                let mut do_chunk_0: VarDictionary =
                    Self::get_or_create_dict(do_wall_0, adj_chunk);
                do_chunk_0.set(adj_cell, vc0);
                do_wall_0.set(adj_chunk, do_chunk_0);

                let mut undo_chunk_0: VarDictionary =
                    Self::get_or_create_dict(undo_wall_0, adj_chunk);
                undo_chunk_0.set(adj_cell, old_wc0);
                undo_wall_0.set(adj_chunk, undo_chunk_0);

                let mut do_chunk_1: VarDictionary =
                    Self::get_or_create_dict(do_wall_1, adj_chunk);
                do_chunk_1.set(adj_cell, vc1);
                do_wall_1.set(adj_chunk, do_chunk_1);

                let mut undo_chunk_1: VarDictionary =
                    Self::get_or_create_dict(undo_wall_1, adj_chunk);
                undo_chunk_1.set(adj_cell, old_wc1);
                undo_wall_1.set(adj_chunk, undo_chunk_1);
            }
        }
    }
}
```

**What's happening:**

The function reads `default_wall_texture` from the terrain (an integer slot index, configurable in Terrain Settings) and converts it to a color pair. Then it iterates through every cell in the height pattern, and for each cell's 8 neighbors, sets the wall color to the default if the neighbor is not already in the wall pattern.

The "skip if already in wall pattern" check (`do_wall_0.get(adj_chunk)` then `d.contains_key(adj_cell)`) is important. If the artist used VertexPaint to set a specific wall texture on a cell, that cell's wall color should not be overwritten by the default. Similarly, if QuickPaint already handled wall colors in Phase 1.5, those take priority.

The chunk boundary crossing logic is identical to `propagate_cross_chunk_edges`: if `adj_x < 0`, wrap to `adj_chunk.x - 1` and `adj_x = dim.x - 1`.

The `get_or_create_dict` pattern deserves attention. Because `VarDictionary` is a Godot reference type (not a Rust value type), getting a sub-dictionary from the outer dictionary, modifying it, and putting it back is the safe way to update nested structures:

```rust
let mut do_chunk_0: VarDictionary =
    Self::get_or_create_dict(do_wall_0, adj_chunk);
do_chunk_0.set(adj_cell, vc0);
do_wall_0.set(adj_chunk, do_chunk_0);
```

### Step 10: Helper statics -- Dictionary manipulation utilities

**Why:** The 4-phase system constantly creates, reads, and updates nested `VarDictionary` structures. Three small helper functions eliminate the boilerplate.

```rust
/// Safely get or create a VarDictionary from a nested dictionary.
fn get_or_create_dict(
    dict: &VarDictionary,
    key: Vector2i,
) -> VarDictionary {
    dict.get(key)
        .and_then(|v| v.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// Copy a value from src nested dict entry to adj nested dict entry.
fn copy_dict_entry(
    dict: &mut VarDictionary,
    src_chunk: Vector2i,
    src_cell: Vector2i,
    adj_chunk: Vector2i,
    adj_cell: Vector2i,
) {
    if let Some(src_outer) = dict.get(src_chunk) {
        let src_dict: VarDictionary = src_outer.to();
        if let Some(val) = src_dict.get(src_cell) {
            let mut adj_dict: VarDictionary =
                Self::get_or_create_dict(dict, adj_chunk);
            adj_dict.set(adj_cell, val);
            dict.set(adj_chunk, adj_dict);
        }
    }
}

/// Set a value in a nested VarDictionary.
fn set_nested_dict(
    dict: &mut VarDictionary,
    chunk: Vector2i,
    cell: Vector2i,
    value: Variant,
) {
    let mut inner: VarDictionary =
        Self::get_or_create_dict(dict, chunk);
    inner.set(cell, value);
    dict.set(chunk, inner);
}
```

**What's happening:**

`get_or_create_dict` attempts to retrieve a `VarDictionary` from the outer dictionary at the given key. If the key doesn't exist or the value is not a dictionary, it returns a new empty `VarDictionary`. The `try_to::<VarDictionary>()` is the safe conversion -- it returns `Ok` if the Variant is actually a Dictionary, `Err` otherwise.

`copy_dict_entry` looks up a value at `[src_chunk][src_cell]` in the dictionary and writes it to `[adj_chunk][adj_cell]`. This is the core operation for edge propagation: take the value painted on a boundary cell and copy it to the corresponding cell on the adjacent chunk.

`set_nested_dict` is the simplest: write `value` into `dict[chunk][cell]`, creating the inner dictionary if needed. It is used extensively in `propagate_cross_chunk_edges` to write undo values.

All three are `fn` (associated functions on `PixyTerrainPlugin`) rather than free functions because they reference `Self::get_or_create_dict`. They could be free functions, but keeping them as associated functions groups them with the data structures they operate on.

### Step 11: `register_undo_redo()` -- Committing to Godot's undo system

**Why:** Godot's `EditorUndoRedoManager` provides Ctrl+Z/Ctrl+Y support. We register each drawing action as a reversible operation by providing "do" and "undo" method calls with the appropriate data.

```rust
/// Register an undo/redo action for a composite pattern operation.
fn register_undo_redo(
    &mut self,
    action_name: &str,
    terrain_node: &Gd<Node>,
    do_patterns: VarDictionary,
    undo_patterns: VarDictionary,
) {
    let Some(mut undo_redo) = self.base_mut().get_undo_redo() else {
        godot_warn!("No EditorUndoRedoManager available");
        return;
    };

    undo_redo.create_action(action_name);
    undo_redo.add_do_method(
        terrain_node,
        "apply_composite_pattern",
        &[do_patterns.to_variant()],
    );
    undo_redo.add_undo_method(
        terrain_node,
        "apply_composite_pattern",
        &[undo_patterns.to_variant()],
    );
    undo_redo.commit_action();
    self.base_mut()
        .call_deferred("apply_collision_visibility_deferred", &[]);
}
```

**What's happening:**

`self.base_mut().get_undo_redo()` retrieves the `EditorUndoRedoManager` from the `EditorPlugin` base class. This returns `Option` because the manager may not be available in non-editor builds.

`create_action(action_name)` starts a new undo/redo action. The `action_name` appears in the Edit menu as "Undo terrain height" or "Redo terrain vertex paint."

`add_do_method` and `add_undo_method` register callbacks on the *terrain node*, not the plugin. This is a critical design choice. When Godot fires undo/redo, the plugin may already be borrowed (e.g., during a deferred call or another signal handler). By targeting the terrain node, we avoid borrow conflicts entirely. The `"apply_composite_pattern"` method is a `#[func]` on `PixyTerrain` that knows how to apply a composite dictionary:

```rust
// In terrain.rs:
#[func]
pub fn apply_composite_pattern(&mut self, patterns: VarDictionary) {
    // Apply order: wall_color_0, wall_color_1, height, grass_mask, color_0, color_1
    let keys_in_order = [
        "wall_color_0", "wall_color_1", "height",
        "grass_mask", "color_0", "color_1",
    ];
    for &key in &keys_in_order {
        // ... iterate chunks and cells, call draw_height/draw_color_0/etc.
    }
    // Regenerate affected chunk meshes
}
```

The apply order matters: wall colors are applied before height, so that when the height change triggers mesh regeneration, the wall geometry already has the correct colors.

`commit_action()` finalizes the action and immediately executes the "do" callbacks. This is why `draw_pattern()` does not apply the changes itself -- the undo/redo system applies them via `apply_composite_pattern`.

The deferred `apply_collision_visibility_deferred` call at the end ensures collision wireframe visibility stays in sync after the terrain changes.

### Step 12: `register_chunk_undo_redo()` -- Chunk add/remove operations

**Why:** Adding and removing chunks must also be undoable. The undo/redo for chunks is simpler than for drawing: it calls specific terrain methods directly rather than using a composite pattern.

```rust
/// Register undo/redo for chunk add/remove operations.
fn register_chunk_undo_redo(
    &mut self,
    terrain_node: &Gd<Node>,
    chunk_x: i32,
    chunk_z: i32,
    action_name: &str,
    is_remove: bool,
) {
    let Some(mut undo_redo) = self.base_mut().get_undo_redo() else {
        godot_warn!("No EditorUndoRedoManager available");
        return;
    };

    let terrain_clone = terrain_node.clone();

    if is_remove {
        undo_redo.create_action(action_name);
        undo_redo.add_do_method(
            &terrain_clone,
            "remove_chunk_from_tree",
            &[chunk_x.to_variant(), chunk_z.to_variant()],
        );
        undo_redo.add_undo_method(
            &terrain_clone,
            "add_new_chunk",
            &[chunk_x.to_variant(), chunk_z.to_variant()],
        );
        undo_redo.commit_action();
    } else {
        undo_redo.create_action(action_name);
        undo_redo.add_do_method(
            &terrain_clone,
            "add_new_chunk",
            &[chunk_x.to_variant(), chunk_z.to_variant()],
        );
        undo_redo.add_undo_method(
            &terrain_clone,
            "remove_chunk",
            &[chunk_x.to_variant(), chunk_z.to_variant()],
        );
        undo_redo.commit_action();
    }
    self.base_mut()
        .call_deferred("apply_collision_visibility_deferred", &[]);
}
```

**What's happening:**

The three terrain methods used are:
- `add_new_chunk(x, z)`: Creates a new chunk at the given coordinates, adds it to the scene tree, and initializes it with default data
- `remove_chunk(x, z)`: Removes the chunk and frees it entirely (`queue_free`)
- `remove_chunk_from_tree(x, z)`: Removes the chunk from the scene tree without freeing it (sets owner to null)

The asymmetry between `remove_chunk_from_tree` (for do/remove) and `remove_chunk` (for undo/add) is deliberate:
- When removing a chunk (do action), we use `remove_chunk_from_tree` which detaches it from the scene tree but keeps the node alive. This preserves the chunk's data so that if the user undoes the removal, the chunk can be re-added with its original state.
- When undoing an add (undo of "add chunk"), we use `remove_chunk` which fully frees the chunk. Since the chunk was just created by the "do" action and has no user data worth preserving, full cleanup is appropriate.

## Key Design Decisions

**Why VarDictionary instead of Rust types?** The composite pattern must cross the Rust-Godot boundary via undo/redo callbacks. Godot's `EditorUndoRedoManager` serializes method arguments as `Variant` arrays. `VarDictionary` is a Godot-native type that serializes cleanly. Using Rust HashMaps would require manual conversion at the boundary.

**Why snapshot the pattern?** Rust's borrow checker does not allow simultaneous immutable borrows of `self.current_draw_pattern` and mutable borrows of other `self` fields. Snapshotting the pattern into a local Vec gives the function an owned copy that doesn't conflict with other borrows.

**Why collect-then-apply for edge propagation?** Same borrow checker constraint. We cannot iterate over the draw pattern (reads `self`) while pushing into dictionaries (also reads `self` for `get_or_create_dict`). The `EdgeEntry` vec breaks this dependency.

**Why register callbacks on terrain, not plugin?** The plugin is often borrowed when Godot fires undo/redo. If the callback targeted the plugin, it would attempt a second mutable borrow and panic. Targeting the terrain node avoids this entirely.

**Why 12 dictionaries?** Six data channels (height, color_0, color_1, wall_color_0, wall_color_1, grass_mask) times two (do and undo). Each channel needs its own pair because different modes populate different subsets. Height mode only fills `do_height`/`undo_height` (plus wall colors from Phase 3). VertexPaint only fills `do_color_0`/`undo_color_0`/`do_color_1`/`undo_color_1`. The composite dictionary only includes non-empty pairs.

## Utility Functions Referenced

Two utility functions are used throughout the drawing system but defined at the top of `editor_plugin.rs`:

```rust
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Replicates Godot's @GlobalScope.ease() function.
fn godot_ease(x: f32, curve: f32) -> f32 {
    if x < 0.0 {
        return 0.0;
    }
    if x > 1.0 {
        return 1.0;
    }
    if curve > 0.0 {
        if curve < 1.0 {
            1.0 - (1.0 - x).powf(1.0 / curve)
        } else {
            x.powf(curve)
        }
    } else if curve < 0.0 {
        if x < 0.5 {
            (2.0 * x).powf(-curve) * 0.5
        } else {
            (1.0 - (2.0 * (1.0 - x)).powf(-curve)) * 0.5 + 0.5
        }
    } else {
        0.0
    }
}
```

`lerp_f32` is a standard linear interpolation. `godot_ease` replicates Godot's built-in `ease()` function in pure Rust, matching the three curve regions (positive < 1, positive >= 1, negative) documented in [Godot's @GlobalScope reference](https://docs.godotengine.org/en/stable/classes/class_%40globalscope.html#class-globalscope-method-ease). The Bridge mode uses this to create S-curves and ease-in/ease-out slopes between the start and end points.

## What's Next

Part 17 will cover the gizmo plugin (`gizmo.rs`) -- the visual feedback system that renders brush circles, pattern overlays, chunk grids, and height previews in the 3D viewport. The gizmo consumes the `GizmoState` snapshot we built in Step 1 and draws everything using Godot's `EditorNode3DGizmoPlugin` API.
