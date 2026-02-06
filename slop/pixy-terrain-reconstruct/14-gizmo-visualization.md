# Part 14 — Gizmo Visualization

**Series:** Reconstructing Pixy Terrain
**Part:** 14 of 18
**Previous:** 2026-02-06-editor-plugin-ui-panels-13.md
**Status:** Complete

## What We're Building

A gizmo plugin that draws three layers of editor-only wireframe overlays on top of the terrain: chunk management lines (red X for existing chunks, green + for placeable neighbors), a draw-pattern preview that shows where the brush will affect cells and at what height, and a brush cursor that conforms to the terrain surface as either a circle or a square. This is Godot's `EditorNode3DGizmoPlugin` system — the same system that draws transform handles, collision shapes, and light cones in the 3D viewport.

## What You'll Have After This

When the editor plugin is active, the viewport shows a white circle (or square) following the mouse cursor over the terrain. The circle drapes over hills and dips into valleys instead of floating at a fixed Y. When painting, small white squares appear under the brush showing exactly which cells will be affected and how strongly. When in chunk management mode, existing chunks display red X overlays and empty neighbor slots display green + symbols. All of this renders on top of geometry (depth draw disabled) with alpha transparency.

## Prerequisites

- Part 13 completed (editor plugin with `PixyTerrainPlugin`, tool modes, brush state, `get_gizmo_state()`)
- Part 10 completed (`PixyTerrain` with `get_chunk()`, `get_chunk_keys()`, `dimensions`, `cell_size`)
- Part 07 completed (`PixyTerrainChunk` with `get_height_at()`)

## Why a Gizmo Plugin Instead of Drawing Directly

In Godot, the 3D editor viewport is controlled by the engine, not by your plugin. You cannot simply draw lines in `_process()` and have them appear in the editor. Godot provides the `EditorNode3DGizmoPlugin` system specifically for this: you register a gizmo plugin, tell Godot which node types it applies to, and implement a `redraw()` method that Godot calls whenever the viewport needs updating. This keeps your drawing code inside Godot's rendering pipeline where it belongs, with proper depth sorting, material overrides, and cleanup.

The alternative — creating `MeshInstance3D` children with `ImmediateMesh` — would clutter the scene tree, cause serialization headaches, and fight the editor's own rendering cycle. Gizmo plugins are the correct tool for editor-only visualization.

## Steps

### Step 1: Define the `GizmoState` snapshot struct

**Why:** The gizmo's `redraw()` method needs to read brush state from the editor plugin: mouse position, brush size, tool mode, draw pattern, flatten toggle, and several other flags. But `redraw()` is called on the gizmo plugin object (`&mut self`), while the brush state lives on a completely different object (the editor plugin). If we tried to hold a mutable borrow of the editor plugin inside `redraw()`, we would immediately hit Rust's aliasing rules — the gizmo already borrows its own fields, and the editor plugin might be mid-mutation. The solution is a snapshot: the gizmo asks the editor plugin for a plain `struct` containing copies of all the state it needs, then works exclusively from that snapshot. No borrow survives across the boundary.

**File:** `rust/src/gizmo.rs`

Start the file with the imports and the snapshot struct:

```rust
use std::collections::HashMap;

use godot::classes::base_material_3d::{DepthDrawMode, ShadingMode, Transparency};
use godot::classes::{
    EditorNode3DGizmo, EditorNode3DGizmoPlugin, IEditorNode3DGizmoPlugin, StandardMaterial3D,
};
use godot::prelude::*;

use crate::editor_plugin::{BrushType, PixyTerrainPlugin, TerrainToolMode};
use crate::terrain::PixyTerrain;

/// State snapshot passed from editor plugin to gizmo plugin.
#[allow(dead_code)]
pub struct GizmoState {
    pub mode: TerrainToolMode,
    pub brush_type: BrushType,
    pub brush_position: Vector3,
    pub brush_size: f32,
    pub terrain_hovered: bool,
    pub flatten: bool,
    pub draw_height: f32,
    pub draw_pattern: HashMap<[i32; 2], HashMap<[i32; 2], f32>>,
    /// Whether the plugin is in setting mode (first click done, waiting for drag/release).
    pub is_setting: bool,
    /// Whether draw_height has been captured for this setting session.
    pub draw_height_set: bool,
    /// Whether the plugin is in active drawing mode.
    pub is_drawing: bool,
}
```

**What's happening:**

`GizmoState` is not a Godot class — it has no `#[derive(GodotClass)]`, no `Base<>`, no registration with the engine. It is a pure Rust struct that exists only to shuttle data from point A (editor plugin) to point B (gizmo redraw). This is intentional. Godot doesn't need to know about this struct. It lives entirely in Rust-land.

The `draw_pattern` field deserves attention: `HashMap<[i32; 2], HashMap<[i32; 2], f32>>`. The outer key is a chunk coordinate `[chunk_x, chunk_z]`. The inner key is a cell coordinate `[cell_x, cell_z]` within that chunk. The `f32` value is the brush sample strength at that cell — 1.0 at the center, tapering to 0.0 at the edges via smoothstep falloff. This nested HashMap structure matches the editor plugin's `current_draw_pattern` field exactly. The `.clone()` in `get_gizmo_state()` deep-copies the entire pattern so the gizmo can iterate it freely.

The `#[allow(dead_code)]` suppresses warnings for fields that are read only during gizmo drawing — the Rust compiler sees no function that reads `is_drawing` directly, but it is present in the struct for completeness and future use.

The `is_setting` and `draw_height_set` flags control height preview behavior. When the user first clicks (entering "setting" mode), the gizmo switches from showing the terrain's current height to showing a preview of what the height *will be* after the brush applies. This requires knowing whether a draw height has been captured (`draw_height_set`) and what that height is (`draw_height`).

### Step 2: Define the `PixyTerrainGizmoPlugin` struct

**Why:** Godot's gizmo system requires a class that extends `EditorNode3DGizmoPlugin`. This class is where you implement the virtual methods that Godot calls: `has_gizmo()` to check if a node needs a gizmo, `redraw()` to actually draw it, and `get_gizmo_name()` for identification. The class also stores a reference to the editor plugin, which is how it accesses the brush state snapshot.

**File:** `rust/src/gizmo.rs` (add before the `GizmoState` struct)

```rust
/// Gizmo plugin for PixyTerrain: brush preview, chunk grid overlay, draw pattern visualization.
/// Port of Yugen's MarchingSquaresTerrainGizmoPlugin + MarchingSquaresTerrainGizmo.
#[derive(GodotClass)]
#[class(base=EditorNode3DGizmoPlugin, init, tool)]
pub struct PixyTerrainGizmoPlugin {
    base: Base<EditorNode3DGizmoPlugin>,

    /// Cached reference to the editor plugin for reading brush state.
    pub plugin_ref: Option<Gd<PixyTerrainPlugin>>,
}
```

**What's happening:**

This is the most important gdext note in the entire file: **`EditorNode3DGizmoPlugin` extends `Resource`, which extends `RefCounted`**. It is NOT a `Node`. You do NOT add it to the scene tree. You do not call `new_alloc()` on it. You create it with `Gd::<PixyTerrainGizmoPlugin>::default()` — the standard constructor for reference-counted Godot objects in gdext.

This trips up many gdext developers. In Godot's class hierarchy:
- `Node`, `Node3D`, `MeshInstance3D` — these are manually managed (`new_alloc()`, must call `free()` or add to tree).
- `Resource`, `RefCounted`, `EditorNode3DGizmoPlugin` — these are reference-counted (`default()`, freed automatically when the last reference drops).

Using `new_alloc()` on a `RefCounted` type would compile but behave incorrectly — the object would be double-freed. Using `default()` on a `Node` type would panic at runtime. The base class determines which constructor you use.

The `plugin_ref` field stores an `Option<Gd<PixyTerrainPlugin>>`. This is set by the editor plugin during `enter_tree()`:

```rust
// In editor_plugin.rs enter_tree():
let mut gizmo_plugin = Gd::<PixyTerrainGizmoPlugin>::default();
gizmo::init_gizmo_plugin(&mut gizmo_plugin);
gizmo_plugin.bind_mut().plugin_ref = Some(self.to_gd());
self.base_mut().add_node_3d_gizmo_plugin(&gizmo_plugin);
self.gizmo_plugin = Some(gizmo_plugin);
```

Notice the ownership chain: the editor plugin stores a `Gd` handle to the gizmo, and the gizmo stores a `Gd` handle back to the editor plugin. This is a reference cycle — both objects hold strong references to each other. In pure Rust, this would leak. But Godot's `EditorPlugin` is manually managed (freed when the editor unloads the addon), which breaks the cycle. In `exit_tree()`, the editor plugin calls `remove_node_3d_gizmo_plugin()` and drops its handle, allowing the gizmo to be reclaimed.

### Step 3: Implement the `IEditorNode3DGizmoPlugin` virtual methods

**Why:** Godot calls these four methods to integrate your gizmo into the 3D editor. `get_gizmo_name()` identifies it. `has_gizmo()` tells Godot which nodes get this gizmo. `redraw()` does the actual drawing. `get_priority()` resolves conflicts when multiple gizmo plugins claim the same node.

**File:** `rust/src/gizmo.rs`

```rust
#[godot_api]
impl IEditorNode3DGizmoPlugin for PixyTerrainGizmoPlugin {
    fn get_gizmo_name(&self) -> GString {
        "PixyTerrain".into()
    }

    fn has_gizmo(&self, node: Option<Gd<godot::classes::Node3D>>) -> bool {
        node.is_some_and(|n| n.is_class("PixyTerrain"))
    }

    fn redraw(&mut self, gizmo: Option<Gd<EditorNode3DGizmo>>) {
        // ... (covered in Steps 4-7)
    }

    fn get_priority(&self) -> i32 {
        -1
    }
}
```

**What's happening:**

**`get_gizmo_name()`** returns a string that Godot uses to identify this gizmo plugin in its internal registry. It does not need to match the Rust struct name — it is a display name. We use `"PixyTerrain"` for clarity.

**`has_gizmo()`** receives an `Option<Gd<Node3D>>`. Godot calls this for every `Node3D` in the scene to ask "does your gizmo plugin want to draw on this node?" The check uses `is_class("PixyTerrain")` — a Godot runtime class check that walks the inheritance chain. This works because `PixyTerrain` is registered as a GDExtension class with that exact name (from the `#[derive(GodotClass)]` on the terrain struct). We use `is_some_and()` to handle the `Option` wrapper — if `node` is `None`, return `false`.

**`get_priority()`** returns `-1`, which is below Godot's default gizmo priority of `0`. This means if another gizmo plugin (like the built-in transform gizmo) also claims `PixyTerrain`, the other plugin wins the "primary gizmo" slot. Our gizmo still draws — it just doesn't steal handle interactions from other gizmos.

All four virtual methods receive their parameters as `Option<Gd<...>>`. This is a gdext convention for virtual method overrides — the engine may pass null, and gdext wraps that possibility as `Option`. Every virtual method should handle `None` gracefully.

### Step 4: Implement `redraw()` — setup and chunk management lines

**Why:** The `redraw()` method is the heart of the gizmo. It is called by Godot every frame that the viewport needs updating (roughly every frame the editor repaints). It must clear the previous frame's lines, gather state, and draw all three visualization layers. We start with the boilerplate and the chunk management layer.

**File:** `rust/src/gizmo.rs` (fill in the `redraw` body)

```rust
    fn redraw(&mut self, gizmo: Option<Gd<EditorNode3DGizmo>>) {
        let Some(mut gizmo) = gizmo else {
            return;
        };
        gizmo.clear();

        let Some(node) = gizmo.get_node_3d() else {
            return;
        };

        let Ok(terrain) = node.clone().try_cast::<PixyTerrain>() else {
            return;
        };

        let Some(ref plugin) = self.plugin_ref else {
            return;
        };

        if !plugin.is_instance_valid() {
            return;
        }

        let plugin_bind = plugin.bind();
        let state = plugin_bind.get_gizmo_state();
        drop(plugin_bind);

        let t = terrain.bind();
        let dim = t.dimensions;
        let cell_size = t.cell_size;
        let chunk_keys = t.get_chunk_keys();

        let chunk_width = (dim.x - 1) as f32 * cell_size.x;
        let chunk_depth = (dim.z - 1) as f32 * cell_size.y;

        // Collect chunk existence for closure use (avoid holding terrain borrow)
        let mut existing_chunks: Vec<[i32; 2]> = Vec::new();
        for i in 0..chunk_keys.len() {
            let ck = chunk_keys[i];
            existing_chunks.push([ck.x as i32, ck.y as i32]);
        }

        let has_chunk_fn = |x: i32, z: i32| -> bool { existing_chunks.contains(&[x, z]) };

        // ── Chunk management lines ──
        let addchunk_mat = self.base_mut().get_material("addchunk");
        let removechunk_mat = self.base_mut().get_material("removechunk");

        if state.mode == TerrainToolMode::ChunkManagement {
            for &[cx, cz] in &existing_chunks {
                // Draw borders for existing chunk (red X)
                draw_chunk_lines(
                    &mut gizmo,
                    cx,
                    cz,
                    chunk_width,
                    chunk_depth,
                    &has_chunk_fn,
                    &removechunk_mat,
                    true,
                );

                // Draw borders for adjacent empty chunks (green +)
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let ax = cx + dx;
                    let az = cz + dz;
                    if !has_chunk_fn(ax, az) {
                        draw_chunk_lines(
                            &mut gizmo,
                            ax,
                            az,
                            chunk_width,
                            chunk_depth,
                            &has_chunk_fn,
                            &addchunk_mat,
                            false,
                        );
                    }
                }
            }
        }

        // ... (draw pattern and brush cursor in Steps 5-6)
```

**What's happening:**

The method begins with a chain of early returns. This is defensive programming for editor code — any of these can legitimately be `None`:
- The gizmo handle itself (Godot passed null).
- The node the gizmo is attached to (the node was freed).
- The terrain cast (the gizmo was mistakenly attached to a non-terrain node).
- The editor plugin reference (the gizmo outlived the plugin).
- The instance validity check (the plugin Godot object was freed but the Rust `Gd` still holds a stale handle).

The `gizmo.clear()` call is critical. Gizmo drawing is not incremental — you must clear all previous lines and redraw everything from scratch each frame. Without `clear()`, lines from previous frames would accumulate until the viewport becomes an unreadable mess.

**The snapshot pattern** (`get_gizmo_state()` + `drop(plugin_bind)`) is the key borrow management technique. `plugin.bind()` creates an immutable borrow into the editor plugin's `Gd` handle. `get_gizmo_state()` copies all relevant fields into a `GizmoState` struct. Then `drop(plugin_bind)` explicitly releases the borrow. After the drop, the `state` variable owns all the data it needs, and no borrow on the plugin exists. This frees us to call `self.base_mut()` later (which requires `&mut self`) without conflicting with a live borrow on `self.plugin_ref`.

**Chunk dimension math**: `chunk_width = (dim.x - 1) * cell_size.x`. The terrain's `dimensions` field stores the number of *height values* per axis (default 33). A grid of 33 height values has 32 cells between them. So the chunk width in world units is 32 cells times the cell size (default 2.0), yielding 64.0 world units per chunk. The `- 1` accounts for the fencepost difference between vertices and cells.

**Existing chunks collection**: We copy chunk keys from a `PackedVector2Array` (Godot's return type) into a `Vec<[i32; 2]>` (Rust's preferred type). This avoids holding the terrain borrow while iterating — the `has_chunk_fn` closure captures the `Vec` by reference, not the terrain.

**The chunk management loop** has two halves:
1. For each existing chunk, draw it with `removechunk_mat` (red) and `is_remove=true` (draws an X symbol inside the chunk boundaries).
2. For each of the 4 cardinal neighbors of each existing chunk, if that neighbor doesn't exist, draw it with `addchunk_mat` (green) and `is_remove=false` (draws a + symbol). This creates an expanding frontier of addable chunks around the existing terrain.

### Step 5: Implement `redraw()` — draw pattern visualization with height preview

**Why:** When the user is painting height, the gizmo needs to show exactly which cells will be modified and what their new heights will be. This is the draw pattern visualization — small squares at each affected cell, positioned at the predicted final height. The preview has three modes depending on whether the user is in setting mode, flatten mode, or default mode.

**File:** `rust/src/gizmo.rs` (continue inside `redraw()`)

```rust
        // ── Draw pattern visualization with height preview ──
        let pattern_mat = self.base_mut().get_material("brush_pattern");

        if !state.draw_pattern.is_empty() {
            let mut lines = PackedVector3Array::new();

            // Calculate height difference for setting mode preview
            let height_diff = if state.is_setting && state.draw_height_set {
                state.brush_position.y - state.draw_height
            } else {
                0.0
            };

            for (chunk_key, cells) in &state.draw_pattern {
                for (cell_key, sample) in cells {
                    let world_x = (chunk_key[0] * (dim.x - 1) + cell_key[0]) as f32 * cell_size.x;
                    let world_z = (chunk_key[1] * (dim.z - 1) + cell_key[1]) as f32 * cell_size.y;

                    // Get base height from terrain (safe access avoids OOB crash)
                    let base_y = if let Some(chunk) = t.get_chunk(chunk_key[0], chunk_key[1]) {
                        chunk
                            .bind()
                            .get_height_at(cell_key[0], cell_key[1])
                            .unwrap_or(0.0)
                    } else {
                        0.0
                    };

                    // Calculate preview height based on mode
                    let preview_y = if state.is_setting && state.draw_height_set {
                        // Setting mode: show height preview at predicted final position
                        if state.flatten {
                            // Flatten mode: lerp towards brush_position.y
                            let t = *sample;
                            base_y + (state.brush_position.y - base_y) * t
                        } else {
                            // Non-flatten: add height delta scaled by sample
                            base_y + height_diff * *sample
                        }
                    } else if state.flatten {
                        // Flatten mode (not in setting): show at draw_height
                        state.draw_height
                    } else {
                        // Default: show at terrain height
                        base_y
                    };

                    let half = *sample * cell_size.x * 0.4;
                    let center = Vector3::new(world_x, preview_y + 0.2, world_z);

                    // Draw a small square for each pattern cell
                    lines.push(center + Vector3::new(-half, 0.0, -half));
                    lines.push(center + Vector3::new(half, 0.0, -half));
                    lines.push(center + Vector3::new(half, 0.0, -half));
                    lines.push(center + Vector3::new(half, 0.0, half));
                    lines.push(center + Vector3::new(half, 0.0, half));
                    lines.push(center + Vector3::new(-half, 0.0, half));
                    lines.push(center + Vector3::new(-half, 0.0, half));
                    lines.push(center + Vector3::new(-half, 0.0, -half));
                }
            }

            if !lines.is_empty() {
                if let Some(ref mat) = pattern_mat {
                    gizmo.add_lines(&lines, &mat.clone().upcast::<godot::classes::Material>());
                }
            }
        }
```

**What's happening:**

**World position calculation**: The expression `(chunk_key[0] * (dim.x - 1) + cell_key[0]) as f32 * cell_size.x` converts a chunk+cell coordinate pair into a world X position. `chunk_key[0] * (dim.x - 1)` gives the cell offset of the chunk's origin (chunk 0 starts at cell 0, chunk 1 starts at cell 32, chunk 2 starts at cell 64). Adding `cell_key[0]` gives the absolute cell index. Multiplying by `cell_size.x` converts from cell coordinates to world coordinates.

**Base height lookup**: For each cell in the draw pattern, we ask the terrain for the current height at that cell. The chain `t.get_chunk() -> chunk.bind().get_height_at()` navigates the two-level data structure: terrain owns chunks (keyed by chunk coordinate), chunks own height maps (keyed by cell coordinate). The `unwrap_or(0.0)` handles cells that are out of bounds — this can happen when the brush extends past the edge of a chunk's height map.

**Height preview has three branches:**

1. **Setting mode + height captured** (`is_setting && draw_height_set`): The user clicked once and is now dragging. The gizmo shows where the height *will go*. In flatten mode, it lerps from the current height toward the brush Y position, scaled by the brush sample strength. In non-flatten mode, it adds a height delta (brush Y minus the captured draw height) scaled by sample strength. This lets the artist see the terrain deformation before releasing the mouse.

2. **Flatten mode without setting** (`state.flatten` alone): The user hasn't clicked yet. The gizmo shows all cells at the captured `draw_height` — a flat plane preview.

3. **Default**: The gizmo shows cells at their current terrain height. No deformation preview.

**Square size encodes brush strength**: `let half = *sample * cell_size.x * 0.4`. The `sample` value (0.0 to 1.0) scales the square size. Cells at the brush center (sample=1.0) get large squares. Cells at the brush edge (sample approaching 0.0) get tiny squares. The `0.4` factor ensures even a full-strength cell's square doesn't quite fill the entire cell, leaving visual gaps between neighbors.

**The +0.2 Y offset** (`preview_y + 0.2`) lifts the pattern squares slightly above the terrain surface. Without this, Z-fighting between the gizmo lines and the terrain mesh would cause flickering.

**Line pairs**: Each square is drawn as 4 line segments (8 vertices): top edge, right edge, bottom edge, left edge. Godot's `add_lines()` interprets the array as consecutive pairs — vertices 0-1 form one line, 2-3 form another, and so on. The 8 vertices produce 4 lines that form a closed square.

### Step 6: Implement `redraw()` — brush circle and square cursor

**Why:** The artist needs a visual indicator of the brush's position and size. A circle for round brushes, a square for square brushes. Unlike a flat overlay, the cursor conforms to the terrain's surface by sampling height at each vertex — so the circle drapes over hills and dips into valleys, giving an accurate preview of what the brush covers.

**File:** `rust/src/gizmo.rs` (continue inside `redraw()`)

```rust
        // ── Brush circle/square visualization ──
        let brush_mat = self.base_mut().get_material("brush");

        if state.terrain_hovered {
            let pos = state.brush_position;
            let half = state.brush_size / 2.0;
            let mut brush_lines = PackedVector3Array::new();

            let gizmo_offset = 0.3;

            match state.brush_type {
                BrushType::Round => {
                    let segments = 32;
                    for i in 0..segments {
                        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
                        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
                        let x0 = pos.x + half * a0.cos();
                        let z0 = pos.z + half * a0.sin();
                        let x1 = pos.x + half * a1.cos();
                        let z1 = pos.z + half * a1.sin();
                        let y0 = sample_terrain_height(&t, x0, z0, dim, cell_size, pos.y, gizmo_offset);
                        let y1 = sample_terrain_height(&t, x1, z1, dim, cell_size, pos.y, gizmo_offset);
                        brush_lines.push(Vector3::new(x0, y0, z0));
                        brush_lines.push(Vector3::new(x1, y1, z1));
                    }
                }
                BrushType::Square => {
                    // Subdivide each side into segments so the square conforms to terrain
                    let subdivisions = 8;
                    let corners = [
                        Vector2::new(pos.x - half, pos.z - half),
                        Vector2::new(pos.x + half, pos.z - half),
                        Vector2::new(pos.x + half, pos.z + half),
                        Vector2::new(pos.x - half, pos.z + half),
                    ];
                    for side in 0..4 {
                        let c0 = corners[side];
                        let c1 = corners[(side + 1) % 4];
                        for s in 0..subdivisions {
                            let t0 = s as f32 / subdivisions as f32;
                            let t1 = (s + 1) as f32 / subdivisions as f32;
                            let x0 = c0.x + (c1.x - c0.x) * t0;
                            let z0 = c0.y + (c1.y - c0.y) * t0;
                            let x1 = c0.x + (c1.x - c0.x) * t1;
                            let z1 = c0.y + (c1.y - c0.y) * t1;
                            let y0 = sample_terrain_height(&t, x0, z0, dim, cell_size, pos.y, gizmo_offset);
                            let y1 = sample_terrain_height(&t, x1, z1, dim, cell_size, pos.y, gizmo_offset);
                            brush_lines.push(Vector3::new(x0, y0, z0));
                            brush_lines.push(Vector3::new(x1, y1, z1));
                        }
                    }
                }
            }

            if !brush_lines.is_empty() {
                if let Some(ref mat) = brush_mat {
                    gizmo.add_lines(
                        &brush_lines,
                        &mat.clone().upcast::<godot::classes::Material>(),
                    );
                }
            }
        }

        drop(t);
    }
```

**What's happening:**

**Round brush**: A circle is approximated by 32 line segments. For each segment, we compute two points on the circle using standard trigonometry (`half * cos(angle)`, `half * sin(angle)`). The angle sweeps from 0 to TAU (2*pi) in 32 equal steps. At each point, we call `sample_terrain_height()` to get the Y coordinate from the actual terrain data. The result is a circle that conforms to the terrain surface — if the brush is half on a cliff and half on flat ground, the circle visually drops down the cliff face.

32 segments is a balance between smoothness and performance. At typical terrain zoom levels, 32 segments is visually indistinguishable from a true circle. Doubling to 64 would not improve the visual but would double the vertex count submitted every frame.

**Square brush**: Each of the 4 sides is subdivided into 8 segments (32 segments total, matching the round brush's vertex budget). Without subdivision, each side would be a single straight line — it would float above valleys and clip through hills. By subdividing and sampling the height at each sub-vertex, the square conforms to the terrain just like the circle does.

The `corners` array stores the 4 corners of the square as `Vector2` (XZ only). The `(side + 1) % 4` modular indexing wraps the last side back to the first corner, closing the square.

**The `gizmo_offset` of 0.3** lifts the brush cursor above the terrain surface. This is slightly larger than the 0.2 offset used for draw pattern squares, ensuring the brush cursor always renders on top of both the terrain mesh and the pattern squares.

**`drop(t)`** explicitly releases the terrain borrow at the end of `redraw()`. This is not strictly necessary — the borrow would be dropped when the function returns — but it documents the programmer's intent: "we are done reading terrain data." In editor code that runs every frame, being explicit about borrow lifetimes prevents surprises when future code is added after this block.

**The `add_lines()` API**: Godot's `EditorNode3DGizmo::add_lines()` takes two arguments — a `PackedVector3Array` of vertex pairs and a `Material` reference. The material must be a `Material` (the base class), but our stored materials are `StandardMaterial3D`. The `.upcast::<godot::classes::Material>()` call converts the concrete type to the base type. The `.clone()` is necessary because `upcast()` consumes the `Gd` handle, and we don't want to move the material out of the `Option`.

### Step 7: Implement `create_materials()`

**Why:** Gizmo materials must be registered with the gizmo plugin before `redraw()` can retrieve them by name. Godot provides two ways to create gizmo materials: `create_material()` for simple colored materials, and manual construction for materials that need custom properties. The brush cursor needs depth draw disabled (so it renders on top of everything) and alpha transparency. The chunk management markers are simpler — they sit at Y=0 and don't need special depth handling.

**File:** `rust/src/gizmo.rs` (add after the `#[godot_api]` block)

```rust
impl PixyTerrainGizmoPlugin {
    pub fn create_materials(&mut self) {
        // Create brush material with depth draw disabled so it renders on top of grass/terrain
        let mut brush_mat = StandardMaterial3D::new_gd();
        brush_mat.set_depth_draw_mode(DepthDrawMode::DISABLED);
        brush_mat.set_shading_mode(ShadingMode::UNSHADED);
        brush_mat.set_transparency(Transparency::ALPHA);
        brush_mat.set_albedo(Color::from_rgba(1.0, 1.0, 1.0, 0.7));
        self.base_mut().add_material("brush", &brush_mat);

        // Brush pattern material (draw pattern visualization)
        let mut pattern_mat = StandardMaterial3D::new_gd();
        pattern_mat.set_depth_draw_mode(DepthDrawMode::DISABLED);
        pattern_mat.set_shading_mode(ShadingMode::UNSHADED);
        pattern_mat.set_transparency(Transparency::ALPHA);
        pattern_mat.set_albedo(Color::from_rgba(0.7, 0.7, 0.7, 0.6));
        self.base_mut().add_material("brush_pattern", &pattern_mat);

        // Chunk management materials (these can use basic create_material since they're at Y=0)
        self.base_mut()
            .create_material("removechunk", Color::from_rgba(1.0, 0.0, 0.0, 0.5));
        self.base_mut()
            .create_material("addchunk", Color::from_rgba(0.0, 1.0, 0.0, 0.5));
        self.base_mut().create_handle_material("handles");
    }
}
```

**What's happening:**

There are two different APIs for creating gizmo materials, and the choice between them matters:

**`add_material(name, &material)`**: You construct the `StandardMaterial3D` yourself, configure it however you want, and register it under a name. This gives full control. The brush and pattern materials use this path because they need `DepthDrawMode::DISABLED` — without it, the brush cursor would be hidden behind terrain geometry. The depth draw disable ensures the gizmo renders in front of everything, regardless of Z-buffer state.

**`create_material(name, color)`**: Godot creates a basic material for you with just a color. Simpler, less control. The chunk management materials (`removechunk` and `addchunk`) use this path because they draw flat lines at Y=0 — there is no terrain geometry to occlude them, so depth draw behavior doesn't matter.

The material properties for brush/pattern:
- `DepthDrawMode::DISABLED` — ignores the Z-buffer entirely. Lines always render on top.
- `ShadingMode::UNSHADED` — no lighting calculations. The color appears exactly as specified regardless of scene lights.
- `Transparency::ALPHA` — enables alpha blending. The 0.7/0.6 alpha values make the lines semi-transparent.
- `set_albedo(Color::from_rgba(...))` — sets the base color. White at 70% opacity for the brush, light gray at 60% for the pattern.

**`create_handle_material("handles")`** creates a default handle material. Handles are the small interactive spheres you see on gizmos (like the light range handle). We don't currently use handles on the terrain gizmo, but Godot expects the material to exist if `add_handles()` is ever called. Registering it during init is cheap insurance.

Note the gdext API asymmetry: `create_material()` takes `Color` **by value** (not by reference). `get_material()` takes just a name (**1 argument**). `add_lines()` takes lines + material (**2 arguments**). These signatures differ from what you might expect if you're used to Godot's GDScript API where everything is passed uniformly.

### Step 8: Implement the `sample_terrain_height()` free function

**Why:** Both the brush circle and the brush square need to look up the terrain height at arbitrary world XZ positions. This function converts a world position to chunk+cell coordinates, looks up the height from the chunk's height map, and returns it with an offset. It is a free function (not a method on any struct) to keep the borrow checker happy — calling it doesn't require `&self` or `&mut self`, so it can be called while the terrain is already borrowed immutably.

**File:** `rust/src/gizmo.rs` (add after the impl blocks)

```rust
/// Sample terrain height at a world XZ position by looking up the chunk and cell.
/// Returns the height + offset, or fallback_y + offset if out of bounds.
fn sample_terrain_height(
    terrain: &PixyTerrain,
    world_x: f32,
    world_z: f32,
    dim: Vector3i,
    cell_size: Vector2,
    fallback_y: f32,
    offset: f32,
) -> f32 {
    let chunk_width = (dim.x - 1) as f32 * cell_size.x;
    let chunk_depth = (dim.z - 1) as f32 * cell_size.y;

    let chunk_x = (world_x / chunk_width).floor() as i32;
    let chunk_z = (world_z / chunk_depth).floor() as i32;

    let local_x = ((world_x - chunk_x as f32 * chunk_width) / cell_size.x).round() as i32;
    let local_z = ((world_z - chunk_z as f32 * chunk_depth) / cell_size.y).round() as i32;

    if let Some(chunk) = terrain.get_chunk(chunk_x, chunk_z) {
        if let Some(h) = chunk.bind().get_height_at(local_x, local_z) {
            return h + offset;
        }
    }
    fallback_y + offset
}
```

**What's happening:**

The coordinate conversion has two stages:

**Stage 1 — World to chunk**: Divide world position by chunk size, then `floor()`. This maps any world position to the chunk that contains it. Negative coordinates work correctly because `floor()` rounds toward negative infinity: `-0.5.floor() = -1`, placing negative-side points in chunk -1.

**Stage 2 — World to local cell**: Subtract the chunk's world-space origin (`chunk_x * chunk_width`), then divide by cell size. `.round()` snaps to the nearest cell center. This rounds rather than floors because cell coordinates should map to the closest height value, not the cell that contains the point — a subtle but important distinction when the brush cursor sits on a cell boundary.

**Fallback behavior**: If the chunk doesn't exist or the cell is out of bounds, the function returns `fallback_y + offset`. The fallback is typically `pos.y` — the brush position's Y. This means the brush cursor floats at its raycast hit height when it extends past the edge of existing terrain. Without a sensible fallback, the circle would have jagged gaps wherever it crosses chunk boundaries.

**Why a free function, not a method?** Inside `redraw()`, we already hold `&t` (an immutable borrow of the terrain via `terrain.bind()`). If `sample_terrain_height` were a method on `PixyTerrainGizmoPlugin`, it couldn't accept `&PixyTerrain` as a parameter without `self` being involved in the borrow chain. As a free function, it takes a plain `&PixyTerrain` reference — which is exactly what `t` (the bind guard's `Deref` target) provides. No borrow conflicts.

### Step 9: Implement the `draw_chunk_lines()` free function

**Why:** Each chunk needs boundary lines (on edges not shared with a neighbor) and either an X symbol (existing chunk, removable) or a + symbol (empty slot, addable). This is a free function for the same borrow reason as `sample_terrain_height()` — it needs `&mut Gd<EditorNode3DGizmo>`, and passing `self` through would tangle borrows.

**File:** `rust/src/gizmo.rs` (add after `sample_terrain_height`)

```rust
/// Draw chunk border lines (free function to avoid borrow issues with self).
#[allow(clippy::too_many_arguments)]
fn draw_chunk_lines(
    gizmo: &mut Gd<EditorNode3DGizmo>,
    chunk_x: i32,
    chunk_z: i32,
    chunk_width: f32,
    chunk_depth: f32,
    has_chunk: &dyn Fn(i32, i32) -> bool,
    material: &Option<Gd<StandardMaterial3D>>,
    is_remove: bool,
) {
    let x0 = chunk_x as f32 * chunk_width;
    let z0 = chunk_z as f32 * chunk_depth;
    let x1 = x0 + chunk_width;
    let z1 = z0 + chunk_depth;

    let mut lines = PackedVector3Array::new();

    if !has_chunk(chunk_x, chunk_z - 1) {
        lines.push(Vector3::new(x0, 0.0, z0));
        lines.push(Vector3::new(x1, 0.0, z0));
    }
    if !has_chunk(chunk_x + 1, chunk_z) {
        lines.push(Vector3::new(x1, 0.0, z0));
        lines.push(Vector3::new(x1, 0.0, z1));
    }
    if !has_chunk(chunk_x, chunk_z + 1) {
        lines.push(Vector3::new(x1, 0.0, z1));
        lines.push(Vector3::new(x0, 0.0, z1));
    }
    if !has_chunk(chunk_x - 1, chunk_z) {
        lines.push(Vector3::new(x0, 0.0, z1));
        lines.push(Vector3::new(x0, 0.0, z0));
    }

    if is_remove {
        lines.push(Vector3::new(x0, 0.0, z0));
        lines.push(Vector3::new(x1, 0.0, z1));
        lines.push(Vector3::new(x1, 0.0, z0));
        lines.push(Vector3::new(x0, 0.0, z1));
    } else {
        let mx = (x0 + x1) / 2.0;
        let mz = (z0 + z1) / 2.0;
        let qw = chunk_width * 0.25;
        let qd = chunk_depth * 0.25;
        lines.push(Vector3::new(mx - qw, 0.0, mz));
        lines.push(Vector3::new(mx + qw, 0.0, mz));
        lines.push(Vector3::new(mx, 0.0, mz - qd));
        lines.push(Vector3::new(mx, 0.0, mz + qd));
    }

    if !lines.is_empty() {
        if let Some(ref mat) = material {
            gizmo.add_lines(&lines, &mat.clone().upcast::<godot::classes::Material>());
        }
    }
}
```

**What's happening:**

**Boundary detection**: Each chunk has 4 edges (north, east, south, west). An edge is only drawn if the neighboring chunk in that direction does not exist. This prevents double-drawing at shared boundaries — if chunk [0,0] and chunk [1,0] are both present, neither draws the east/west edge between them. The `has_chunk` closure performs the lookup.

The boundary edges trace the outline of the terrain. For a single chunk, all 4 edges draw. For a 2x2 block of chunks, only the outer perimeter draws. This creates a clean grid visualization without interior clutter.

**Symbol drawing**: The `is_remove` flag switches between two symbols:

- **X (remove)**: Two diagonal lines corner-to-corner. `(x0,z0)->(x1,z1)` and `(x1,z0)->(x0,z1)`. This communicates "click here to remove this chunk."

- **+ (add)**: Two perpendicular lines through the center, each spanning half the chunk dimension (using quarter-offsets `qw` and `qd` from center). The + is intentionally smaller than the X — the add targets are less prominent than the remove targets, guiding the artist's attention toward existing terrain.

All lines are at Y=0.0 because chunk management operates on the ground plane. Heights are irrelevant for chunk creation/deletion.

**`#[allow(clippy::too_many_arguments)]`**: Clippy warns when a function takes more than 7 arguments. This function takes 8. The alternative would be bundling parameters into a struct, but for a private helper called from exactly one place, the struct would add complexity without improving readability. The `#[allow]` is the pragmatic choice.

**`&dyn Fn(i32, i32) -> bool`**: The `has_chunk` parameter is a trait object — a dynamic dispatch closure. We use `&dyn Fn` rather than a generic `impl Fn` because the function is called from a loop where different closures might be passed (in practice it's always the same closure, but `&dyn Fn` keeps the function signature simple and avoids monomorphization).

### Step 10: Implement the `init_gizmo_plugin()` public function

**Why:** The gizmo plugin's materials must be created after construction but before the first `redraw()`. Godot's `EditorNode3DGizmoPlugin` has no `_ready()` virtual — it's a Resource, not a Node. The editor plugin needs a way to trigger material creation from the outside. This free function provides that entry point.

**File:** `rust/src/gizmo.rs` (add at the end of the file)

```rust
/// Initialize gizmo materials. Must be called after construction.
pub fn init_gizmo_plugin(plugin: &mut Gd<PixyTerrainGizmoPlugin>) {
    plugin.bind_mut().create_materials();
}
```

**What's happening:**

This function exists because of gdext's separation between construction and initialization. When the editor plugin creates the gizmo with `Gd::<PixyTerrainGizmoPlugin>::default()`, the `init` attribute generates a default constructor that sets `plugin_ref` to `None` and leaves the base `EditorNode3DGizmoPlugin` empty (no materials registered). Materials cannot be created in the constructor because `self.base_mut()` is not safely callable during `init` — the Godot object is not yet fully constructed.

The editor plugin calls this function in `enter_tree()`:

```rust
let mut gizmo_plugin = Gd::<PixyTerrainGizmoPlugin>::default();
gizmo::init_gizmo_plugin(&mut gizmo_plugin);       // materials created here
gizmo_plugin.bind_mut().plugin_ref = Some(self.to_gd()); // plugin ref set here
self.base_mut().add_node_3d_gizmo_plugin(&gizmo_plugin); // registered with Godot here
```

The three-step sequence is: construct, initialize materials, wire up cross-references. Each step must happen in order — `add_node_3d_gizmo_plugin()` expects materials to already exist, and `redraw()` expects `plugin_ref` to be set.

## The Complete File

Here is `rust/src/gizmo.rs` in full — 391 lines:

```rust
use std::collections::HashMap;

use godot::classes::base_material_3d::{DepthDrawMode, ShadingMode, Transparency};
use godot::classes::{
    EditorNode3DGizmo, EditorNode3DGizmoPlugin, IEditorNode3DGizmoPlugin, StandardMaterial3D,
};
use godot::prelude::*;

use crate::editor_plugin::{BrushType, PixyTerrainPlugin, TerrainToolMode};
use crate::terrain::PixyTerrain;

/// Gizmo plugin for PixyTerrain: brush preview, chunk grid overlay, draw pattern visualization.
/// Port of Yugen's MarchingSquaresTerrainGizmoPlugin + MarchingSquaresTerrainGizmo.
#[derive(GodotClass)]
#[class(base=EditorNode3DGizmoPlugin, init, tool)]
pub struct PixyTerrainGizmoPlugin {
    base: Base<EditorNode3DGizmoPlugin>,

    /// Cached reference to the editor plugin for reading brush state.
    pub plugin_ref: Option<Gd<PixyTerrainPlugin>>,
}

#[godot_api]
impl IEditorNode3DGizmoPlugin for PixyTerrainGizmoPlugin {
    fn get_gizmo_name(&self) -> GString {
        "PixyTerrain".into()
    }

    fn has_gizmo(&self, node: Option<Gd<godot::classes::Node3D>>) -> bool {
        node.is_some_and(|n| n.is_class("PixyTerrain"))
    }

    fn redraw(&mut self, gizmo: Option<Gd<EditorNode3DGizmo>>) {
        let Some(mut gizmo) = gizmo else {
            return;
        };
        gizmo.clear();

        let Some(node) = gizmo.get_node_3d() else {
            return;
        };

        let Ok(terrain) = node.clone().try_cast::<PixyTerrain>() else {
            return;
        };

        let Some(ref plugin) = self.plugin_ref else {
            return;
        };

        if !plugin.is_instance_valid() {
            return;
        }

        let plugin_bind = plugin.bind();
        let state = plugin_bind.get_gizmo_state();
        drop(plugin_bind);

        let t = terrain.bind();
        let dim = t.dimensions;
        let cell_size = t.cell_size;
        let chunk_keys = t.get_chunk_keys();

        let chunk_width = (dim.x - 1) as f32 * cell_size.x;
        let chunk_depth = (dim.z - 1) as f32 * cell_size.y;

        // Collect chunk existence for closure use (avoid holding terrain borrow)
        let mut existing_chunks: Vec<[i32; 2]> = Vec::new();
        for i in 0..chunk_keys.len() {
            let ck = chunk_keys[i];
            existing_chunks.push([ck.x as i32, ck.y as i32]);
        }

        let has_chunk_fn = |x: i32, z: i32| -> bool { existing_chunks.contains(&[x, z]) };

        // ── Chunk management lines ──
        let addchunk_mat = self.base_mut().get_material("addchunk");
        let removechunk_mat = self.base_mut().get_material("removechunk");

        if state.mode == TerrainToolMode::ChunkManagement {
            for &[cx, cz] in &existing_chunks {
                // Draw borders for existing chunk (red X)
                draw_chunk_lines(
                    &mut gizmo,
                    cx,
                    cz,
                    chunk_width,
                    chunk_depth,
                    &has_chunk_fn,
                    &removechunk_mat,
                    true,
                );

                // Draw borders for adjacent empty chunks (green +)
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let ax = cx + dx;
                    let az = cz + dz;
                    if !has_chunk_fn(ax, az) {
                        draw_chunk_lines(
                            &mut gizmo,
                            ax,
                            az,
                            chunk_width,
                            chunk_depth,
                            &has_chunk_fn,
                            &addchunk_mat,
                            false,
                        );
                    }
                }
            }
        }

        // ── Draw pattern visualization with height preview ──
        let pattern_mat = self.base_mut().get_material("brush_pattern");

        if !state.draw_pattern.is_empty() {
            let mut lines = PackedVector3Array::new();

            // Calculate height difference for setting mode preview
            let height_diff = if state.is_setting && state.draw_height_set {
                state.brush_position.y - state.draw_height
            } else {
                0.0
            };

            for (chunk_key, cells) in &state.draw_pattern {
                for (cell_key, sample) in cells {
                    let world_x = (chunk_key[0] * (dim.x - 1) + cell_key[0]) as f32 * cell_size.x;
                    let world_z = (chunk_key[1] * (dim.z - 1) + cell_key[1]) as f32 * cell_size.y;

                    // Get base height from terrain (safe access avoids OOB crash)
                    let base_y = if let Some(chunk) = t.get_chunk(chunk_key[0], chunk_key[1]) {
                        chunk
                            .bind()
                            .get_height_at(cell_key[0], cell_key[1])
                            .unwrap_or(0.0)
                    } else {
                        0.0
                    };

                    // Calculate preview height based on mode
                    let preview_y = if state.is_setting && state.draw_height_set {
                        // Setting mode: show height preview at predicted final position
                        if state.flatten {
                            // Flatten mode: lerp towards brush_position.y
                            let t = *sample;
                            base_y + (state.brush_position.y - base_y) * t
                        } else {
                            // Non-flatten: add height delta scaled by sample
                            base_y + height_diff * *sample
                        }
                    } else if state.flatten {
                        // Flatten mode (not in setting): show at draw_height
                        state.draw_height
                    } else {
                        // Default: show at terrain height
                        base_y
                    };

                    let half = *sample * cell_size.x * 0.4;
                    let center = Vector3::new(world_x, preview_y + 0.2, world_z);

                    // Draw a small square for each pattern cell
                    lines.push(center + Vector3::new(-half, 0.0, -half));
                    lines.push(center + Vector3::new(half, 0.0, -half));
                    lines.push(center + Vector3::new(half, 0.0, -half));
                    lines.push(center + Vector3::new(half, 0.0, half));
                    lines.push(center + Vector3::new(half, 0.0, half));
                    lines.push(center + Vector3::new(-half, 0.0, half));
                    lines.push(center + Vector3::new(-half, 0.0, half));
                    lines.push(center + Vector3::new(-half, 0.0, -half));
                }
            }

            if !lines.is_empty() {
                if let Some(ref mat) = pattern_mat {
                    gizmo.add_lines(&lines, &mat.clone().upcast::<godot::classes::Material>());
                }
            }
        }

        // ── Brush circle/square visualization ──
        let brush_mat = self.base_mut().get_material("brush");

        if state.terrain_hovered {
            let pos = state.brush_position;
            let half = state.brush_size / 2.0;
            let mut brush_lines = PackedVector3Array::new();

            let gizmo_offset = 0.3;

            match state.brush_type {
                BrushType::Round => {
                    let segments = 32;
                    for i in 0..segments {
                        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
                        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
                        let x0 = pos.x + half * a0.cos();
                        let z0 = pos.z + half * a0.sin();
                        let x1 = pos.x + half * a1.cos();
                        let z1 = pos.z + half * a1.sin();
                        let y0 = sample_terrain_height(&t, x0, z0, dim, cell_size, pos.y, gizmo_offset);
                        let y1 = sample_terrain_height(&t, x1, z1, dim, cell_size, pos.y, gizmo_offset);
                        brush_lines.push(Vector3::new(x0, y0, z0));
                        brush_lines.push(Vector3::new(x1, y1, z1));
                    }
                }
                BrushType::Square => {
                    // Subdivide each side into segments so the square conforms to terrain
                    let subdivisions = 8;
                    let corners = [
                        Vector2::new(pos.x - half, pos.z - half),
                        Vector2::new(pos.x + half, pos.z - half),
                        Vector2::new(pos.x + half, pos.z + half),
                        Vector2::new(pos.x - half, pos.z + half),
                    ];
                    for side in 0..4 {
                        let c0 = corners[side];
                        let c1 = corners[(side + 1) % 4];
                        for s in 0..subdivisions {
                            let t0 = s as f32 / subdivisions as f32;
                            let t1 = (s + 1) as f32 / subdivisions as f32;
                            let x0 = c0.x + (c1.x - c0.x) * t0;
                            let z0 = c0.y + (c1.y - c0.y) * t0;
                            let x1 = c0.x + (c1.x - c0.x) * t1;
                            let z1 = c0.y + (c1.y - c0.y) * t1;
                            let y0 = sample_terrain_height(&t, x0, z0, dim, cell_size, pos.y, gizmo_offset);
                            let y1 = sample_terrain_height(&t, x1, z1, dim, cell_size, pos.y, gizmo_offset);
                            brush_lines.push(Vector3::new(x0, y0, z0));
                            brush_lines.push(Vector3::new(x1, y1, z1));
                        }
                    }
                }
            }

            if !brush_lines.is_empty() {
                if let Some(ref mat) = brush_mat {
                    gizmo.add_lines(
                        &brush_lines,
                        &mat.clone().upcast::<godot::classes::Material>(),
                    );
                }
            }
        }

        drop(t);
    }

    fn get_priority(&self) -> i32 {
        -1
    }
}

impl PixyTerrainGizmoPlugin {
    pub fn create_materials(&mut self) {
        // Create brush material with depth draw disabled so it renders on top of grass/terrain
        let mut brush_mat = StandardMaterial3D::new_gd();
        brush_mat.set_depth_draw_mode(DepthDrawMode::DISABLED);
        brush_mat.set_shading_mode(ShadingMode::UNSHADED);
        brush_mat.set_transparency(Transparency::ALPHA);
        brush_mat.set_albedo(Color::from_rgba(1.0, 1.0, 1.0, 0.7));
        self.base_mut().add_material("brush", &brush_mat);

        // Brush pattern material (draw pattern visualization)
        let mut pattern_mat = StandardMaterial3D::new_gd();
        pattern_mat.set_depth_draw_mode(DepthDrawMode::DISABLED);
        pattern_mat.set_shading_mode(ShadingMode::UNSHADED);
        pattern_mat.set_transparency(Transparency::ALPHA);
        pattern_mat.set_albedo(Color::from_rgba(0.7, 0.7, 0.7, 0.6));
        self.base_mut().add_material("brush_pattern", &pattern_mat);

        // Chunk management materials (these can use basic create_material since they're at Y=0)
        self.base_mut()
            .create_material("removechunk", Color::from_rgba(1.0, 0.0, 0.0, 0.5));
        self.base_mut()
            .create_material("addchunk", Color::from_rgba(0.0, 1.0, 0.0, 0.5));
        self.base_mut().create_handle_material("handles");
    }
}

/// Sample terrain height at a world XZ position by looking up the chunk and cell.
/// Returns the height + offset, or fallback_y + offset if out of bounds.
fn sample_terrain_height(
    terrain: &PixyTerrain,
    world_x: f32,
    world_z: f32,
    dim: Vector3i,
    cell_size: Vector2,
    fallback_y: f32,
    offset: f32,
) -> f32 {
    let chunk_width = (dim.x - 1) as f32 * cell_size.x;
    let chunk_depth = (dim.z - 1) as f32 * cell_size.y;

    let chunk_x = (world_x / chunk_width).floor() as i32;
    let chunk_z = (world_z / chunk_depth).floor() as i32;

    let local_x = ((world_x - chunk_x as f32 * chunk_width) / cell_size.x).round() as i32;
    let local_z = ((world_z - chunk_z as f32 * chunk_depth) / cell_size.y).round() as i32;

    if let Some(chunk) = terrain.get_chunk(chunk_x, chunk_z) {
        if let Some(h) = chunk.bind().get_height_at(local_x, local_z) {
            return h + offset;
        }
    }
    fallback_y + offset
}

/// Draw chunk border lines (free function to avoid borrow issues with self).
#[allow(clippy::too_many_arguments)]
fn draw_chunk_lines(
    gizmo: &mut Gd<EditorNode3DGizmo>,
    chunk_x: i32,
    chunk_z: i32,
    chunk_width: f32,
    chunk_depth: f32,
    has_chunk: &dyn Fn(i32, i32) -> bool,
    material: &Option<Gd<StandardMaterial3D>>,
    is_remove: bool,
) {
    let x0 = chunk_x as f32 * chunk_width;
    let z0 = chunk_z as f32 * chunk_depth;
    let x1 = x0 + chunk_width;
    let z1 = z0 + chunk_depth;

    let mut lines = PackedVector3Array::new();

    if !has_chunk(chunk_x, chunk_z - 1) {
        lines.push(Vector3::new(x0, 0.0, z0));
        lines.push(Vector3::new(x1, 0.0, z0));
    }
    if !has_chunk(chunk_x + 1, chunk_z) {
        lines.push(Vector3::new(x1, 0.0, z0));
        lines.push(Vector3::new(x1, 0.0, z1));
    }
    if !has_chunk(chunk_x, chunk_z + 1) {
        lines.push(Vector3::new(x1, 0.0, z1));
        lines.push(Vector3::new(x0, 0.0, z1));
    }
    if !has_chunk(chunk_x - 1, chunk_z) {
        lines.push(Vector3::new(x0, 0.0, z1));
        lines.push(Vector3::new(x0, 0.0, z0));
    }

    if is_remove {
        lines.push(Vector3::new(x0, 0.0, z0));
        lines.push(Vector3::new(x1, 0.0, z1));
        lines.push(Vector3::new(x1, 0.0, z0));
        lines.push(Vector3::new(x0, 0.0, z1));
    } else {
        let mx = (x0 + x1) / 2.0;
        let mz = (z0 + z1) / 2.0;
        let qw = chunk_width * 0.25;
        let qd = chunk_depth * 0.25;
        lines.push(Vector3::new(mx - qw, 0.0, mz));
        lines.push(Vector3::new(mx + qw, 0.0, mz));
        lines.push(Vector3::new(mx, 0.0, mz - qd));
        lines.push(Vector3::new(mx, 0.0, mz + qd));
    }

    if !lines.is_empty() {
        if let Some(ref mat) = material {
            gizmo.add_lines(&lines, &mat.clone().upcast::<godot::classes::Material>());
        }
    }
}

/// State snapshot passed from editor plugin to gizmo plugin.
#[allow(dead_code)]
pub struct GizmoState {
    pub mode: TerrainToolMode,
    pub brush_type: BrushType,
    pub brush_position: Vector3,
    pub brush_size: f32,
    pub terrain_hovered: bool,
    pub flatten: bool,
    pub draw_height: f32,
    pub draw_pattern: HashMap<[i32; 2], HashMap<[i32; 2], f32>>,
    /// Whether the plugin is in setting mode (first click done, waiting for drag/release).
    pub is_setting: bool,
    /// Whether draw_height has been captured for this setting session.
    pub draw_height_set: bool,
    /// Whether the plugin is in active drawing mode.
    pub is_drawing: bool,
}

/// Initialize gizmo materials. Must be called after construction.
pub fn init_gizmo_plugin(plugin: &mut Gd<PixyTerrainGizmoPlugin>) {
    plugin.bind_mut().create_materials();
}
```

## Verify

```bash
cd rust && cargo build
```

The gizmo module compiles as part of the full library. To verify it works visually:

1. Open the Godot editor with the project.
2. Select the `PixyTerrain` node in the scene.
3. The editor plugin should be active (toolbar visible).
4. Move the mouse over the terrain — a white circle should follow the cursor, conforming to terrain height.
5. Click and drag to paint — small gray squares should appear showing the affected cells.
6. Switch to Chunk Management mode — red X overlays should appear on existing chunks, green + symbols on empty neighbors.

## What You Learned

- **EditorNode3DGizmoPlugin extends Resource (RefCounted)**: Create with `Gd::<T>::default()`, NOT `new_alloc()`. This is the most common gdext mistake for gizmo plugins. Resource-based types are reference-counted and automatically freed. Node-based types are manually managed and must be freed explicitly or added to the scene tree.

- **The snapshot pattern for cross-object data access**: When object A needs to read state from object B during a method that requires `&mut self` on A, copy B's state into a plain struct and drop the borrow on B before proceeding. This sidesteps Rust's aliasing rules entirely — no overlapping borrows, no `RefCell`, no `unsafe`.

- **Free functions dodge borrow conflicts**: `sample_terrain_height()` and `draw_chunk_lines()` are free functions (not methods) specifically because calling a method on `self` while `self.plugin_ref` is borrowed would violate aliasing rules. Free functions take their dependencies as explicit parameters, keeping the borrow checker happy.

- **`add_lines()` works with vertex pairs**: Every two consecutive vertices in the `PackedVector3Array` form one line segment. 8 vertices = 4 lines = 1 square. 64 vertices = 32 lines = 1 circle. There is no "line strip" mode — each segment is independent.

- **Terrain-conforming cursors via height sampling**: Rather than drawing the brush at a fixed Y, each vertex of the circle/square queries the terrain's height map. This creates cursors that drape over the 3D terrain surface, giving artists accurate visual feedback about brush coverage.

- **`create_material()` vs `add_material()`**: Use `create_material(name, color)` for simple colored gizmo materials. Use manual `StandardMaterial3D` construction + `add_material(name, &mat)` when you need depth draw control, transparency, or other advanced properties.

- **`upcast::<Material>()`**: Godot's `add_lines()` expects a `Material` reference, but stored gizmo materials are `StandardMaterial3D`. The `.upcast::<godot::classes::Material>()` call converts the concrete subclass to the expected base type. This is a runtime-free type cast — no actual conversion happens, just a pointer reinterpretation.

## Stubs Introduced

- None

## Stubs Resolved

- [x] `gizmo` module (empty) — introduced in Part 01, now full `PixyTerrainGizmoPlugin` with brush cursor, draw pattern, and chunk management visualization
