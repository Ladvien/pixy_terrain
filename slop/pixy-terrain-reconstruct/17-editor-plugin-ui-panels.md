# Part 17 — Editor Plugin — UI Panels

**Series:** Reconstructing Pixy Terrain
**Part:** 17 of 18
**Previous:** 2026-02-06-editor-plugin-input-handling-16.md
**Status:** Complete

## What We're Building

The entire editor UI system for the Pixy Terrain plugin: a left-side toolbar with tool mode buttons, a bottom attributes bar that rebuilds dynamically per mode, a right-side texture settings panel with resource pickers and color pickers, and the callback wiring that connects every slider, checkbox, dropdown, and picker to the plugin's internal state. This is roughly lines 833-2282 of `editor_plugin.rs` — the `#[func]` callback methods, the `rebuild_attributes_impl()` dispatcher, six UI helper methods, `apply_terrain_setting()`, and `rebuild_texture_panel_impl()`.

## What You'll Have After This

A fully interactive editor sidebar. Switching tool modes (Height, Level, Smooth, Bridge, Grass Mask, Vertex Paint, Debug, Chunks, Settings) rebuilds the bottom attributes bar with the correct controls. Changing any control immediately updates the plugin's brush state or the terrain's shader uniforms. The right-side panel exposes all 15 texture slots with resource pickers, UV scale sliders, ground color pickers, grass sprite pickers, and per-texture grass toggles. Every change flows through a single `on_attribute_changed` or `on_texture_resource_changed` callback, keeping the signal routing centralized and predictable.

## Prerequisites

- Part 16 completed (the plugin struct, `IEditorPlugin` trait, `enter_tree()` toolbar construction, and `forward_3d_gui_input()` mouse handling)
- Part 09-10 completed (`PixyTerrain` with `force_batch_update()`, all 50+ export fields, chunk operations)
- Part 11 completed (`PixyQuickPaint` resource type)
- Part 03 completed (`texture_index_to_colors()` in `marching_squares.rs`)

## Why `call_deferred` Is Everywhere

Before diving into the code, we need to understand a pervasive pattern. Godot's signal dispatch system invokes your callback while the engine still holds internal locks on the scene tree. In gdext specifically, when a signal fires during `forward_3d_gui_input()`, the `PixyTerrainPlugin` is already mutably borrowed by the engine. If the callback tries to call `self.to_gd()` — which requires a shared borrow — you get a Rust panic from the borrow checker at runtime.

The solution is `call_deferred`. Instead of rebuilding the attributes panel inside the signal handler, we ask Godot to call `_rebuild_attributes_deferred()` at the end of the current frame, after all borrows are released. The deferred method can safely call `self.to_gd()` because no other borrow is active.

This pattern appears in three places:
1. `on_tool_button_toggled()` defers the attributes rebuild
2. `set_ui_visible()` defers both attributes and texture panel rebuilds
3. `on_attribute_changed("chunk_select")` defers the attributes rebuild after updating the selected chunk

If you ever see a `call_deferred` in this codebase, the reason is almost always "a signal handler cannot safely borrow self while the engine already holds a borrow."

## Steps

### Step 1: Simple `#[func]` button callbacks

**Why:** The Generate and Clear buttons, the Show Colliders toggle, and the deferred rebuild trampolines all need `#[func]` methods so Godot's signal system can call them. These are trivially short — they delegate to private helpers — but they must be `#[func]` because Godot signals connect via string method names.

Add this `#[godot_api]` impl block after the `IEditorPlugin` impl closes (after line 833):

```rust
// ═══════════════════════════════════════════
// #[func] Methods (callable from GDScript / undo-redo)
// ═══════════════════════════════════════════

#[godot_api]
impl PixyTerrainPlugin {
    #[func]
    fn on_generate_pressed(&mut self) {
        self.do_generate();
    }

    #[func]
    fn on_clear_pressed(&mut self) {
        self.do_clear();
    }

    #[func]
    fn on_collision_toggle_changed(&mut self, pressed: bool) {
        self.show_collision_wireframes = pressed;
        self.apply_collision_visibility_to_all_chunks();
    }

    #[func]
    fn apply_collision_visibility_deferred(&self) {
        self.apply_collision_visibility_to_all_chunks();
    }

    /// Deferred rebuild of attributes panel - safe to call to_gd() here.
    #[func]
    fn _rebuild_attributes_deferred(&mut self) {
        let plugin_ref = self.to_gd();
        self.rebuild_attributes_impl(plugin_ref);
    }

    /// Deferred rebuild of texture panel - safe to call to_gd() here.
    #[func]
    fn _rebuild_texture_panel_deferred(&mut self) {
        let plugin_ref = self.to_gd();
        self.rebuild_texture_panel_impl(plugin_ref);
    }
```

**Design note on `_rebuild_attributes_deferred` and `_rebuild_texture_panel_deferred`:** These are the "trampoline" methods. They exist solely to be called by `call_deferred`. The leading underscore signals "internal, not for external callers." Each one captures `self.to_gd()` — a `Gd<PixyTerrainPlugin>` smart pointer — and passes it to the real implementation. The implementation methods need this pointer so they can construct `Callable::from_object_method(&plugin_ref, ...)` for each new control's signal connection.

### Step 2: `on_tool_button_toggled` — mode mapping with deferred rebuild

**Why:** When the user clicks a tool button in the toolbar, Godot fires the `toggled` signal with `(pressed: bool)`. Our `bindv` call in `enter_tree()` appended `tool_index: i32`, so the callback receives both. We map the index to a `TerrainToolMode` enum variant and schedule a deferred rebuild of the attributes panel.

Still inside the same `#[godot_api]` impl block:

```rust
    /// Called when a tool mode toggle button is pressed.
    /// Godot passes signal args first (pressed: bool), then bound args (tool_index: i32).
    #[func]
    fn on_tool_button_toggled(&mut self, pressed: bool, tool_index: i32) {
        if !pressed {
            return;
        }
        self.mode = match tool_index {
            0 => TerrainToolMode::Height,
            1 => TerrainToolMode::Level,
            2 => TerrainToolMode::Smooth,
            3 => TerrainToolMode::Bridge,
            4 => TerrainToolMode::GrassMask,
            5 => TerrainToolMode::VertexPaint,
            6 => TerrainToolMode::DebugBrush,
            7 => TerrainToolMode::ChunkManagement,
            8 => TerrainToolMode::TerrainSettings,
            _ => TerrainToolMode::Height,
        };
        // Use call_deferred to avoid borrow conflict from signal dispatch
        self.base_mut()
            .call_deferred("_rebuild_attributes_deferred", &[]);
    }
```

**Key gdext pattern — signal argument ordering:** When you connect a signal with `Callable::from_object_method(...).bindv(&varray![extra])`, Godot appends the bound arguments *after* the signal's own arguments. The `toggled` signal emits `(pressed: bool)`, so the callback signature is `(pressed: bool, tool_index: i32)` — signal arg first, bound arg second. Getting this order wrong is a common source of silent bugs where the tool index is always 0 or 1.

### Step 3: `on_attribute_changed` — the giant routing match

**Why:** Every slider, checkbox, dropdown, and spinbox in the attributes panel connects its value-changed signal to this single method. Centralizing the routing means we only need one `#[func]` callback for all controls — we distinguish them by the `setting_name` string bound via `bindv`.

The method receives a `Variant` (the new value) and a `GString` (the setting name). A large `match` dispatches to the appropriate field update:

```rust
    /// Called when an attribute control value changes.
    /// Godot passes signal args first (value: Variant), then bound args (setting_name: GString).
    #[func]
    fn on_attribute_changed(&mut self, value: Variant, setting_name: GString) {
        match setting_name.to_string().as_str() {
            "brush_type" => {
                let idx: i64 = value.to();
                self.brush_type = if idx == 0 {
                    BrushType::Round
                } else {
                    BrushType::Square
                };
            }
            "size" => {
                let v = value.to::<f64>();
                self.brush_size = v as f32;
                if let Some(ref hbox) = self.attributes_hbox {
                    Self::update_slider_label(hbox, "size", "Size", v);
                }
            }
            "strength" => {
                let v = value.to::<f64>();
                self.strength = v as f32;
                if let Some(ref hbox) = self.attributes_hbox {
                    Self::update_slider_label(hbox, "strength", "Strength", v);
                }
            }
            "height" => {
                let v = value.to::<f64>();
                self.height = v as f32;
                if let Some(ref hbox) = self.attributes_hbox {
                    Self::update_slider_label(hbox, "height", "Height", v);
                }
            }
            "flatten" => {
                self.flatten = value.to();
            }
            "falloff" => {
                self.falloff = value.to();
            }
            "ease_value" => {
                let v = value.to::<f64>();
                self.ease_value = v as f32;
                if let Some(ref hbox) = self.attributes_hbox {
                    Self::update_slider_label(hbox, "ease_value", "Ease", v);
                }
            }
            "mask_mode" => {
                self.should_mask_grass = value.to();
            }
            "material" => {
                let idx: i64 = value.to();
                self.set_vertex_colors(idx as i32);
            }
            "paint_walls" => {
                self.paint_walls_mode = value.to();
            }
            "quick_paint" => {
                let idx: i64 = value.to();
                if idx == 0 {
                    // "None" option
                    self.current_quick_paint = None;
                } else {
                    let preset_idx = (idx - 1) as usize;
                    self.current_quick_paint = self.quick_paint_presets.get(preset_idx).cloned();
                }
            }
            "chunk_select" => {
                // Update selected chunk from dropdown index
                if let Some(ref terrain) = self.current_terrain {
                    if terrain.is_instance_valid() {
                        let t: Gd<PixyTerrain> = terrain.clone().cast();
                        let keys = t.bind().get_chunk_keys();
                        let idx = value.to::<i64>() as usize;
                        if idx < keys.len() {
                            let k = keys[idx];
                            self.selected_chunk_coords =
                                Some(Vector2i::new(k.x as i32, k.y as i32));
                            // Rebuild to update merge mode display (deferred to avoid borrow conflict)
                            self.base_mut()
                                .call_deferred("_rebuild_attributes_deferred", &[]);
                        }
                    }
                }
            }
            "chunk_merge_mode" => {
                // Set merge mode on selected chunk
                if let Some(ref terrain) = self.current_terrain {
                    if terrain.is_instance_valid() {
                        let t: Gd<PixyTerrain> = terrain.clone().cast();
                        if let Some(sel) = self.selected_chunk_coords {
                            if let Some(mut chunk) = t.bind().get_chunk(sel.x, sel.y) {
                                chunk.bind_mut().merge_mode = value.to::<i64>() as i32;
                                chunk.bind_mut().regenerate_mesh();
                            }
                        }
                    }
                }
            }
```

**Design note on `chunk_select`:** When the user picks a different chunk from the dropdown, we store its coordinates in `selected_chunk_coords` and then schedule a deferred rebuild. The rebuild reads the selected chunk's current merge mode and pre-selects the merge mode dropdown to match. This is another instance of the deferred pattern — changing the dropdown triggers a signal, and we cannot rebuild the UI while that signal is still dispatching.

**Design note on `chunk_merge_mode`:** This one does *not* defer. It reaches directly into the chunk and modifies its `merge_mode` field, then calls `regenerate_mesh()`. This is safe because we are not rebuilding any UI — just mutating terrain data and triggering a mesh rebuild.

### Step 4: Terrain settings dispatch in `on_attribute_changed`

**Why:** The Terrain Settings mode exposes 20+ parameters (dimensions, cell size, blend mode, thresholds, grass settings, etc.). Rather than adding 20 match arms that each write to a terrain field, we delegate to `apply_terrain_setting()`. The `on_attribute_changed` match arm just needs to recognize the setting names and update any dynamic slider labels:

```rust
            // ── Terrain Settings ──
            "dim_x"
            | "dim_z"
            | "dim_y"
            | "cell_size_x"
            | "cell_size_z"
            | "blend_mode"
            | "wall_threshold"
            | "ridge_threshold"
            | "ledge_threshold"
            | "merge_mode"
            | "grass_subdivisions"
            | "grass_size_x"
            | "grass_size_y"
            | "default_wall_texture"
            | "blend_sharpness"
            | "blend_noise_scale"
            | "blend_noise_strength"
            | "animation_fps"
            | "use_ridge_texture"
            | "extra_collision_layer" => {
                self.apply_terrain_setting(setting_name.to_string().as_str(), &value);
                let label_text = match setting_name.to_string().as_str() {
                    "cell_size_x" => Some("Cell X"),
                    "cell_size_z" => Some("Cell Z"),
                    "wall_threshold" => Some("Wall Thresh"),
                    "ridge_threshold" => Some("Ridge Thresh"),
                    "ledge_threshold" => Some("Ledge Thresh"),
                    "grass_size_x" => Some("Grass W"),
                    "grass_size_y" => Some("Grass H"),
                    "blend_sharpness" => Some("Blend Sharp"),
                    "blend_noise_scale" => Some("Noise Scale"),
                    "blend_noise_strength" => Some("Noise Str"),
                    _ => None,
                };
                if let (Some(label), Some(ref hbox)) = (label_text, &self.attributes_hbox) {
                    Self::update_slider_label(hbox, setting_name.to_string().as_str(), label, value.to::<f64>());
                }
            }
            // ── Texture Panel Settings ──
            name if name.starts_with("tex_scale_")
                || name.starts_with("tex_has_grass_")
                || name.starts_with("ground_color_") =>
            {
                if name.starts_with("tex_scale_") {
                    if let Some(ref panel) = self.texture_panel {
                        Self::update_slider_label(panel, name, "Scale", value.to::<f64>());
                    }
                }
                self.apply_terrain_setting(name, &value);
            }
            _ => {}
        }
    }
```

**Design note on the texture panel settings arm:** These names are dynamically generated (`tex_scale_1`, `tex_has_grass_3`, `ground_color_5`, etc.) so we use guard patterns (`name if name.starts_with(...)`) instead of literal match arms. The scale slider labels live in the texture panel (right side), not the attributes hbox (bottom), so we search the correct container.

### Step 5: `on_texture_resource_changed` — resource picker callback

**Why:** The `EditorResourcePicker` widget fires a `resource_changed` signal with the new resource (or null if cleared). We need to map the setting name back to the correct field on `PixyTerrain`, assign the texture, and sync shader uniforms.

```rust
    /// Called when a texture resource is changed via EditorResourcePicker.
    /// Godot passes signal args first (resource), then bound args (setting_name).
    #[func]
    fn on_texture_resource_changed(&mut self, resource: Variant, setting_name: GString) {
        let Some(ref terrain_node) = self.current_terrain else {
            return;
        };
        if !terrain_node.is_instance_valid() {
            return;
        }
        let mut terrain: Gd<PixyTerrain> = terrain_node.clone().cast();

        let name = setting_name.to_string();
        let tex: Option<Gd<godot::classes::Texture2D>> = if resource.is_nil() {
            None
        } else {
            Some(resource.to())
        };

        {
            let mut t = terrain.bind_mut();

            if let Some(slot_str) = name.strip_prefix("ground_tex_") {
                let slot: i32 = slot_str.parse().unwrap_or(1);
                match slot {
                    1 => t.ground_texture = tex,
                    2 => t.texture_2 = tex,
                    3 => t.texture_3 = tex,
                    4 => t.texture_4 = tex,
                    5 => t.texture_5 = tex,
                    6 => t.texture_6 = tex,
                    7 => t.texture_7 = tex,
                    8 => t.texture_8 = tex,
                    9 => t.texture_9 = tex,
                    10 => t.texture_10 = tex,
                    11 => t.texture_11 = tex,
                    12 => t.texture_12 = tex,
                    13 => t.texture_13 = tex,
                    14 => t.texture_14 = tex,
                    15 => t.texture_15 = tex,
                    _ => {}
                }
            } else if let Some(slot_str) = name.strip_prefix("grass_sprite_") {
                let slot: i32 = slot_str.parse().unwrap_or(1);
                match slot {
                    1 => t.grass_sprite = tex,
                    2 => t.grass_sprite_tex_2 = tex,
                    3 => t.grass_sprite_tex_3 = tex,
                    4 => t.grass_sprite_tex_4 = tex,
                    5 => t.grass_sprite_tex_5 = tex,
                    6 => t.grass_sprite_tex_6 = tex,
                    _ => {}
                }
            }
        }

        // Sync shader uniforms
        terrain.bind_mut().force_batch_update();
    }
}
```

**Design note:** The `bind_mut()` scope is explicitly closed with a block before calling `force_batch_update()`. This is necessary because `force_batch_update()` also calls `bind_mut()` on the terrain. If the first borrow was still active, Rust would panic. The block-scoped drop pattern is a recurring theme throughout the codebase.

**Design note on the slot matching:** Yes, 15 match arms for ground textures and 6 for grass sprites is verbose. The alternative would be a `Vec` or array of `Option<Gd<Texture2D>>` on `PixyTerrain`, but Godot's `#[export]` attribute does not support `Vec<Option<Gd<T>>>` in gdext, so we use individual named fields and pay the match-arm tax.

### Step 6: `set_ui_visible()` and collision visibility helpers

**Why:** When the user selects or deselects a `PixyTerrain` node, the editor calls `edit()` / `make_visible()`. We need to show or hide all three panels and trigger deferred rebuilds when making them visible.

Add these to the private `impl PixyTerrainPlugin` block:

```rust
impl PixyTerrainPlugin {
    fn apply_collision_visibility_to_all_chunks(&self) {
        let Some(ref terrain_node) = self.current_terrain else {
            return;
        };
        if !terrain_node.is_instance_valid() {
            return;
        }
        let terrain: Gd<PixyTerrain> = terrain_node.clone().cast();
        let t = terrain.bind();
        let keys = t.get_chunk_keys();
        for i in 0..keys.len() {
            let k = keys[i];
            if let Some(chunk) = t.get_chunk(k.x as i32, k.y as i32) {
                Self::set_chunk_collision_visible(
                    &chunk.upcast::<Node>(),
                    self.show_collision_wireframes,
                );
            }
        }
    }

    fn set_chunk_collision_visible(chunk_node: &Gd<Node>, visible: bool) {
        let children = chunk_node.get_children();
        for i in 0..children.len() {
            if let Some(child) = children.get(i) {
                if let Ok(mut body) = child.try_cast::<StaticBody3D>() {
                    body.set_visible(visible);
                }
            }
        }
    }

    fn set_ui_visible(&mut self, visible: bool) {
        if let Some(ref mut margin) = self.margin_container {
            margin.set_visible(visible);
        }
        if let Some(ref mut scroll) = self.attributes_container {
            scroll.set_visible(visible);
        }
        if let Some(ref mut tex_panel) = self.texture_panel {
            tex_panel.set_visible(visible);
        }
        if visible {
            // Use call_deferred to avoid borrow conflict from Godot dispatch
            self.base_mut()
                .call_deferred("_rebuild_attributes_deferred", &[]);
            self.base_mut()
                .call_deferred("_rebuild_texture_panel_deferred", &[]);
        }
    }
```

**Design note:** `set_ui_visible` controls three separate containers because the editor plugin registers them in three different Godot panel slots: `SPATIAL_EDITOR_SIDE_LEFT` for the toolbar, `SPATIAL_EDITOR_BOTTOM` for the attributes bar, and `SPATIAL_EDITOR_SIDE_RIGHT` for the texture panel. Each must be shown/hidden independently.

### Step 7: `rebuild_attributes_impl()` — the main mode dispatcher

**Why:** This is the heart of the dynamic UI. Every time the user switches tool modes, we tear down all existing controls in the bottom attributes bar and rebuild new ones specific to that mode. Height mode gets brush type, size, flatten, and falloff controls. Vertex Paint mode gets a 15-option material dropdown. Terrain Settings mode gets 20+ spinboxes, sliders, and dropdowns.

```rust
    /// Rebuild the bottom attributes panel controls based on the current tool mode.
    /// This is the internal implementation - call via _rebuild_attributes_deferred.
    fn rebuild_attributes_impl(&mut self, plugin_ref: Gd<PixyTerrainPlugin>) {
        // Clear existing children
        if let Some(ref mut hbox) = self.attributes_hbox {
            // Remove all children
            let count = hbox.get_child_count();
            for i in (0..count).rev() {
                if let Some(mut child) = hbox.get_child(i) {
                    hbox.remove_child(&child);
                    child.queue_free();
                }
            }
        }

        match self.mode {
            TerrainToolMode::Height => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_checkbox_attribute("flatten", "Flatten", self.flatten, &plugin_ref);
                self.add_checkbox_attribute("falloff", "Falloff", self.falloff, &plugin_ref);
                self.add_quick_paint_dropdown(&plugin_ref);
            }
            TerrainToolMode::Level => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "height",
                    "Height",
                    -50.0,
                    50.0,
                    0.1,
                    self.height as f64,
                    &plugin_ref,
                );
                self.add_checkbox_attribute("falloff", "Falloff", self.falloff, &plugin_ref);
                self.add_quick_paint_dropdown(&plugin_ref);
            }
            TerrainToolMode::Smooth => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "strength",
                    "Strength",
                    0.1,
                    10.0,
                    0.1,
                    self.strength as f64,
                    &plugin_ref,
                );
                self.add_quick_paint_dropdown(&plugin_ref);
            }
            TerrainToolMode::Bridge => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "ease_value",
                    "Ease",
                    -5.0,
                    5.0,
                    0.1,
                    self.ease_value as f64,
                    &plugin_ref,
                );
                self.add_quick_paint_dropdown(&plugin_ref);
            }
            TerrainToolMode::GrassMask => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_checkbox_attribute(
                    "mask_mode",
                    "Mask",
                    self.should_mask_grass,
                    &plugin_ref,
                );
            }
            TerrainToolMode::VertexPaint => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                let mat_options: Vec<&str> = (0..15)
                    .map(|i| match i {
                        0 => "Tex 0",
                        1 => "Tex 1",
                        2 => "Tex 2",
                        3 => "Tex 3",
                        4 => "Tex 4",
                        5 => "Tex 5",
                        6 => "Tex 6",
                        7 => "Tex 7",
                        8 => "Tex 8",
                        9 => "Tex 9",
                        10 => "Tex 10",
                        11 => "Tex 11",
                        12 => "Tex 12",
                        13 => "Tex 13",
                        14 => "Tex 14",
                        _ => "Wall",
                    })
                    .collect();
                self.add_option_attribute(
                    "material",
                    "Material",
                    &mat_options,
                    self.vertex_color_idx as i64,
                    &plugin_ref,
                );
                self.add_checkbox_attribute(
                    "paint_walls",
                    "Paint Walls",
                    self.paint_walls_mode,
                    &plugin_ref,
                );
            }
            TerrainToolMode::DebugBrush => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
            }
            TerrainToolMode::ChunkManagement => {
                // Chunk selector and merge mode selector
                if let Some(ref terrain) = self.current_terrain {
                    if terrain.is_instance_valid() {
                        let t: Gd<PixyTerrain> = terrain.clone().cast();
                        let keys = t.bind().get_chunk_keys();

                        if !keys.is_empty() {
                            // Build chunk selection options
                            let mut chunk_options: Vec<String> = Vec::new();
                            for i in 0..keys.len() {
                                let k = keys[i];
                                chunk_options
                                    .push(format!("Chunk ({}, {})", k.x as i32, k.y as i32));
                            }
                            let chunk_refs: Vec<&str> =
                                chunk_options.iter().map(|s| s.as_str()).collect();

                            // Determine current selection
                            let current_idx = if let Some(sel) = self.selected_chunk_coords {
                                (0..keys.len())
                                    .find(|&i| {
                                        let k = keys[i];
                                        k.x as i32 == sel.x && k.y as i32 == sel.y
                                    })
                                    .unwrap_or(0)
                            } else {
                                0
                            };

                            self.add_option_attribute(
                                "chunk_select",
                                "Chunk",
                                &chunk_refs,
                                current_idx as i64,
                                &plugin_ref,
                            );

                            // Get selected chunk's merge mode
                            let sel_coords = if let Some(sel) = self.selected_chunk_coords {
                                sel
                            } else if !keys.is_empty() {
                                let k = keys[0];
                                Vector2i::new(k.x as i32, k.y as i32)
                            } else {
                                Vector2i::ZERO
                            };

                            let merge_mode = if let Some(chunk) =
                                t.bind().get_chunk(sel_coords.x, sel_coords.y)
                            {
                                chunk.bind().merge_mode
                            } else {
                                1 // Default to Polyhedron
                            };

                            self.add_option_attribute(
                                "chunk_merge_mode",
                                "Merge",
                                &[
                                    "Cubic",
                                    "Polyhedron",
                                    "RoundedPoly",
                                    "SemiRound",
                                    "Spherical",
                                ],
                                merge_mode as i64,
                                &plugin_ref,
                            );
                        }
                    }
                }
            }
```

**Design note on chunk management:** The chunk selector reads the terrain's current chunk list and builds a dropdown. When you select a chunk, the merge mode dropdown updates to show that chunk's current mode. This is the only mode where the attributes panel reads *runtime* terrain state (chunk list, per-chunk merge mode) instead of just reflecting the plugin's own brush settings.

### Step 8: Terrain Settings mode — 20+ controls

**Why:** The Terrain Settings mode provides a one-stop panel for every terrain-wide parameter: grid dimensions, cell size, blend mode, merge mode, thresholds, grass settings, animation FPS, collision layer, and shader blend parameters. The pattern is consistent — read the current value from the terrain, create the appropriate control type, and let the signal flow through `on_attribute_changed` back to `apply_terrain_setting`.

Still inside the `match self.mode { ... }` in `rebuild_attributes_impl`:

```rust
            TerrainToolMode::TerrainSettings => {
                // Read current terrain values for display
                let (
                    dims,
                    cell_sz,
                    blend,
                    wall_th,
                    ridge_th,
                    ledge_th,
                    merge,
                    grass_sub,
                    grass_sz,
                    def_wall,
                    blend_sharp,
                    blend_ns,
                    blend_nstr,
                    anim_fps,
                    use_ridge_tex,
                    extra_coll,
                ) = if let Some(ref terrain) = self.current_terrain {
                    if terrain.is_instance_valid() {
                        let t: Gd<PixyTerrain> = terrain.clone().cast();
                        let tb = t.bind();
                        (
                            tb.dimensions,
                            tb.cell_size,
                            tb.blend_mode,
                            tb.wall_threshold,
                            tb.ridge_threshold,
                            tb.ledge_threshold,
                            tb.merge_mode,
                            tb.grass_subdivisions,
                            tb.grass_size,
                            tb.default_wall_texture,
                            tb.blend_sharpness,
                            tb.blend_noise_scale,
                            tb.blend_noise_strength,
                            tb.animation_fps,
                            tb.use_ridge_texture,
                            tb.extra_collision_layer,
                        )
                    } else {
                        return;
                    }
                } else {
                    return;
                };

                self.add_spinbox_attribute(
                    "dim_x", "Dim X", 3.0, 129.0, 1.0, dims.x as f64, &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "dim_z", "Dim Z", 3.0, 129.0, 1.0, dims.z as f64, &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "dim_y", "Height", 1.0, 256.0, 1.0, dims.y as f64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "cell_size_x", "Cell X", 0.1, 10.0, 0.1, cell_sz.x as f64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "cell_size_z", "Cell Z", 0.1, 10.0, 0.1, cell_sz.y as f64, &plugin_ref,
                );
                self.add_option_attribute(
                    "blend_mode", "Blend", &["Smooth", "Hard", "Hard Blend"],
                    blend as i64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "wall_threshold", "Wall Thresh", 0.0, 0.5, 0.01, wall_th as f64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "ridge_threshold", "Ridge Thresh", 0.0, 1.0, 0.01, ridge_th as f64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "ledge_threshold", "Ledge Thresh", 0.0, 1.0, 0.01, ledge_th as f64, &plugin_ref,
                );
                self.add_option_attribute(
                    "merge_mode", "Merge",
                    &["Cubic", "Polyhedron", "RoundedPoly", "SemiRound", "Spherical"],
                    merge as i64, &plugin_ref,
                );
                self.add_checkbox_attribute(
                    "use_ridge_texture", "Ridge Tex", use_ridge_tex, &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "grass_subdivisions", "Grass Subs", 1.0, 10.0, 1.0, grass_sub as f64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "grass_size_x", "Grass W", 0.1, 5.0, 0.1, grass_sz.x as f64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "grass_size_y", "Grass H", 0.1, 5.0, 0.1, grass_sz.y as f64, &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "animation_fps", "Anim FPS", 0.0, 60.0, 1.0, anim_fps as f64, &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "default_wall_texture", "Wall Tex", 0.0, 15.0, 1.0, def_wall as f64, &plugin_ref,
                );
                // Extra collision layer: options 9-32 (stored as absolute layer number)
                let coll_options: Vec<&str> = (9..=32)
                    .map(|i| match i {
                        9 => "Layer 9",
                        10 => "Layer 10",
                        11 => "Layer 11",
                        12 => "Layer 12",
                        13 => "Layer 13",
                        14 => "Layer 14",
                        15 => "Layer 15",
                        16 => "Layer 16",
                        17 => "Layer 17",
                        18 => "Layer 18",
                        19 => "Layer 19",
                        20 => "Layer 20",
                        21 => "Layer 21",
                        22 => "Layer 22",
                        23 => "Layer 23",
                        24 => "Layer 24",
                        25 => "Layer 25",
                        26 => "Layer 26",
                        27 => "Layer 27",
                        28 => "Layer 28",
                        29 => "Layer 29",
                        30 => "Layer 30",
                        31 => "Layer 31",
                        32 => "Layer 32",
                        _ => "Layer 9",
                    })
                    .collect();
                self.add_option_attribute(
                    "extra_collision_layer", "Coll Layer", &coll_options,
                    (extra_coll - 9).max(0) as i64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "blend_sharpness", "Blend Sharp", 0.0, 20.0, 0.1, blend_sharp as f64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "blend_noise_scale", "Noise Scale", 0.0, 50.0, 0.1, blend_ns as f64, &plugin_ref,
                );
                self.add_slider_attribute(
                    "blend_noise_strength", "Noise Str", 0.0, 5.0, 0.01, blend_nstr as f64, &plugin_ref,
                );
            }
        }
    }
```

**Design note on the 16-field destructuring:** We read all terrain values in a single `bind()` scope and destructure them into local variables. This is deliberate — we cannot hold a `bind()` borrow across the `add_*_attribute` calls because those calls borrow `self.attributes_hbox` mutably. By extracting all values first and dropping the bind, we avoid any borrow conflicts.

**Design note on `extra_collision_layer`:** The terrain stores the collision layer as an absolute number (9-32), but the dropdown index runs 0-23. We convert between them with `(extra_coll - 9).max(0)` on display and `value + 9` on store (in `apply_terrain_setting`).

### Step 9: UI helper methods — `add_slider_attribute`

**Why:** Every slider in the attributes panel follows the same pattern: CenterContainer wrapper, VBoxContainer for vertical stacking, Label showing "Name: value", HSlider with min/max/step/current, signal connected via `Callable::from_object_method().bindv()`. Extracting this into a helper eliminates hundreds of lines of duplication.

```rust
    /// Add an HSlider attribute control to the bottom attributes panel.
    #[allow(clippy::too_many_arguments)]
    fn add_slider_attribute(
        &mut self,
        name: &str,
        label_text: &str,
        min: f64,
        max: f64,
        step: f64,
        current: f64,
        plugin_ref: &Gd<PixyTerrainPlugin>,
    ) {
        let Some(ref mut hbox) = self.attributes_hbox else {
            return;
        };

        let mut center = CenterContainer::new_alloc();
        center.set_custom_minimum_size(Vector2::new(160.0, 36.0));

        let mut vbox = VBoxContainer::new_alloc();
        vbox.add_theme_constant_override("separation", 0);

        let mut label = Label::new_alloc();
        label.set_text(&format!("{label_text}: {current:.1}"));
        label.set_name(&format!("{name}_label"));

        let mut slider = HSlider::new_alloc();
        slider.set_name(&format!("{name}_slider"));
        slider.set_min(min);
        slider.set_max(max);
        slider.set_step(step);
        slider.set_value(current);
        slider.set_custom_minimum_size(Vector2::new(140.0, 0.0));

        let callable = Callable::from_object_method(plugin_ref, "on_attribute_changed")
            .bindv(&varray![name.to_variant()]);
        slider.connect("value_changed", &callable);

        vbox.add_child(&label);
        vbox.add_child(&slider);
        center.add_child(&vbox);
        hbox.add_child(&center);
    }
```

**Key gdext pattern — `Callable::from_object_method().bindv()`:** This is how you pass extra context through a Godot signal. The `HSlider`'s `value_changed` signal emits a single `f64`. By binding `name.to_variant()`, the callback receives `(value: f64, setting_name: GString)`. The `on_attribute_changed` method then matches on the name to know which field to update.

**Design note on naming conventions:** The label is named `{name}_label` and the slider is named `{name}_slider`. This naming convention is critical — `sync_brush_size_slider()` and `update_slider_label()` use `find_child_ex` to locate these widgets by name. If the naming convention breaks, scroll wheel brush size adjustment silently stops working.

### Step 10: `sync_brush_size_slider` and `update_slider_label`

**Why:** When the user changes brush size via Shift+scroll wheel (handled in `forward_3d_gui_input`), the slider widget in the attributes panel needs to update to match. `sync_brush_size_slider` finds the slider by name and calls `set_value`, which triggers the `value_changed` signal, which calls `on_attribute_changed("size")`, which updates the label. This round-trip through the signal system ensures the label always stays in sync.

`update_slider_label` is a static helper that finds a label by name inside any container and updates its text. It is used both from `on_attribute_changed` (for immediate feedback) and could be called from any code that needs to update a label without triggering a signal.

```rust
    /// Sync the "size" slider widget + label to match `self.brush_size` (e.g. after scroll wheel).
    fn sync_brush_size_slider(&self) {
        if let Some(ref hbox) = self.attributes_hbox {
            let slider_name = GString::from("size_slider" as &str);
            if let Some(node) = hbox.upcast_ref::<Node>().find_child_ex(&slider_name).recursive(true).owned(false).done() {
                let mut slider: Gd<HSlider> = node.cast();
                // set_value triggers value_changed signal -> on_attribute_changed -> label update
                slider.set_value(self.brush_size as f64);
            }
        }
    }

    /// Update a slider label's displayed value. Searches `container` for a child named `{name}_label`.
    fn update_slider_label(container: &Gd<impl Inherits<Node>>, name: &str, label_text: &str, value: f64) {
        let label_name_str = format!("{name}_label");
        let label_name = GString::from(label_name_str.as_str());
        if let Some(node) = container.upcast_ref::<Node>().find_child_ex(&label_name).recursive(true).owned(false).done() {
            let mut label: Gd<Label> = node.cast();
            label.set_text(&format!("{label_text}: {value:.1}"));
        }
    }
```

**Design note on `find_child_ex`:** Godot's `find_child` has builder-pattern options in gdext. The `.recursive(true)` flag searches the entire subtree (necessary because controls are nested inside CenterContainer and VBoxContainer wrappers). The `.owned(false)` flag means "search all children, not just directly owned ones."

**Design note on `update_slider_label` being a static method:** It takes a container reference instead of `&self` so it can search either the `attributes_hbox` (bottom panel) or the `texture_panel` (right panel). Texture scale sliders live in the texture panel, not the attributes panel, so the caller passes the correct container.

### Step 11: `add_checkbox_attribute`, `add_option_attribute`, `add_spinbox_attribute`

**Why:** These follow the same pattern as `add_slider_attribute` — wrap in CenterContainer, add label and control, connect signal with `bindv`. Each control type has slightly different signal semantics:

- **CheckBox** emits `toggled(bool)` — no label update needed, the checkbox text is static
- **OptionButton** emits `item_selected(i64)` — the value is an index, not a display string
- **SpinBox** emits `value_changed(f64)` — like HSlider but with typed input

```rust
    /// Add a CheckBox attribute control to the bottom attributes panel.
    fn add_checkbox_attribute(
        &mut self,
        name: &str,
        label_text: &str,
        current: bool,
        plugin_ref: &Gd<PixyTerrainPlugin>,
    ) {
        let Some(ref mut hbox) = self.attributes_hbox else {
            return;
        };

        let mut center = CenterContainer::new_alloc();
        center.set_custom_minimum_size(Vector2::new(100.0, 36.0));

        let mut checkbox = CheckBox::new_alloc();
        checkbox.set_text(label_text);
        checkbox.set_pressed(current);

        let callable = Callable::from_object_method(plugin_ref, "on_attribute_changed")
            .bindv(&varray![name.to_variant()]);
        checkbox.connect("toggled", &callable);

        center.add_child(&checkbox);
        hbox.add_child(&center);
    }

    /// Add an OptionButton attribute control to the bottom attributes panel.
    fn add_option_attribute(
        &mut self,
        name: &str,
        label_text: &str,
        options: &[&str],
        current_index: i64,
        plugin_ref: &Gd<PixyTerrainPlugin>,
    ) {
        let Some(ref mut hbox) = self.attributes_hbox else {
            return;
        };

        let mut center = CenterContainer::new_alloc();
        center.set_custom_minimum_size(Vector2::new(120.0, 36.0));

        let mut vbox = VBoxContainer::new_alloc();
        vbox.add_theme_constant_override("separation", 0);

        let mut label = Label::new_alloc();
        label.set_text(label_text);

        let mut option_btn = OptionButton::new_alloc();
        for opt in options {
            option_btn.add_item(*opt);
        }
        option_btn.select(current_index as i32);

        let callable = Callable::from_object_method(plugin_ref, "on_attribute_changed")
            .bindv(&varray![name.to_variant()]);
        option_btn.connect("item_selected", &callable);

        vbox.add_child(&label);
        vbox.add_child(&option_btn);
        center.add_child(&vbox);
        hbox.add_child(&center);
    }

    /// Add a SpinBox attribute control to the bottom attributes panel.
    #[allow(clippy::too_many_arguments)]
    fn add_spinbox_attribute(
        &mut self,
        name: &str,
        label_text: &str,
        min: f64,
        max: f64,
        step: f64,
        current: f64,
        plugin_ref: &Gd<PixyTerrainPlugin>,
    ) {
        let Some(ref mut hbox) = self.attributes_hbox else {
            return;
        };

        let mut center = CenterContainer::new_alloc();
        center.set_custom_minimum_size(Vector2::new(120.0, 36.0));

        let mut vbox = VBoxContainer::new_alloc();
        vbox.add_theme_constant_override("separation", 0);

        let mut label = Label::new_alloc();
        label.set_text(label_text);

        let mut spin = SpinBox::new_alloc();
        spin.set_min(min);
        spin.set_max(max);
        spin.set_step(step);
        spin.set_value(current);
        spin.set_custom_minimum_size(Vector2::new(80.0, 0.0));

        let callable = Callable::from_object_method(plugin_ref, "on_attribute_changed")
            .bindv(&varray![name.to_variant()]);
        spin.connect("value_changed", &callable);

        vbox.add_child(&label);
        vbox.add_child(&spin);
        center.add_child(&vbox);
        hbox.add_child(&center);
    }
```

**Design note — SpinBox vs HSlider:** We use SpinBox for discrete/integer values (dimensions, subdivisions, FPS, texture slot) and HSlider for continuous values (brush size, thresholds, blend parameters). SpinBox provides typed input which is better for exact values; HSlider provides visual scrubbing which is better for "feel it out" parameters.

### Step 12: `add_quick_paint_dropdown`

**Why:** The Height, Level, Smooth, and Bridge modes all support QuickPaint presets. Rather than duplicating the dropdown construction in four match arms, we extract it into a helper. The dropdown shows "None" as the first option, followed by the names of all loaded `PixyQuickPaint` presets. Selecting an entry sets `self.current_quick_paint` so the draw pattern can apply texture/grass changes alongside height changes.

```rust
    /// Add a QuickPaint dropdown to the attributes panel.
    fn add_quick_paint_dropdown(&mut self, plugin_ref: &Gd<PixyTerrainPlugin>) {
        // Build options: "None" + names of all presets
        let mut options: Vec<&str> = vec!["None"];
        let preset_names: Vec<String> = self
            .quick_paint_presets
            .iter()
            .map(|p| p.bind().paint_name.to_string())
            .collect();
        let preset_refs: Vec<&str> = preset_names.iter().map(|s| s.as_str()).collect();
        options.extend(preset_refs);

        let current_idx = if self.current_quick_paint.is_some() {
            // Find which preset is active
            if let Some(ref active) = self.current_quick_paint {
                self.quick_paint_presets
                    .iter()
                    .position(|p| p.instance_id() == active.instance_id())
                    .map(|i| (i + 1) as i64)
                    .unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };

        self.add_option_attribute(
            "quick_paint",
            "QuickPaint",
            &options,
            current_idx,
            plugin_ref,
        );
    }
```

**Design note on the index offset:** The "None" entry is at index 0, so preset indices are offset by 1. When the user selects index 3, the actual preset is `quick_paint_presets[2]`. The `on_attribute_changed("quick_paint")` handler subtracts 1 to compensate. This is a simple but error-prone convention — if you add options before the presets, every index shifts.

**Design note on lifetime gymnastics:** We collect `preset_names` as `Vec<String>`, then create `preset_refs` as `Vec<&str>` referencing those strings. The `options` vec extends with `preset_refs`. This two-step process is necessary because `add_option_attribute` takes `&[&str]`, and we cannot borrow from a temporary `String` that does not live long enough.

### Step 13: `apply_terrain_setting()` — field writer with shader sync

**Why:** When any terrain setting changes — from the Settings mode panel or from the texture panel — we need to (a) write the new value into the correct field on `PixyTerrain`, and (b) sync all shader uniforms so the change is visible immediately. This method handles both steps.

```rust
    /// Apply a terrain setting change to the current terrain and update the shader.
    fn apply_terrain_setting(&mut self, name: &str, value: &Variant) {
        let Some(ref terrain_node) = self.current_terrain else {
            return;
        };
        if !terrain_node.is_instance_valid() {
            return;
        }
        let mut terrain: Gd<PixyTerrain> = terrain_node.clone().cast();

        {
            let mut t = terrain.bind_mut();
            match name {
                "dim_x" => {
                    let v = value.to::<f64>() as i32;
                    t.dimensions = Vector3i::new(v, t.dimensions.y, t.dimensions.z);
                }
                "dim_z" => {
                    let v = value.to::<f64>() as i32;
                    t.dimensions = Vector3i::new(t.dimensions.x, t.dimensions.y, v);
                }
                "dim_y" => {
                    let v = value.to::<f64>() as i32;
                    t.dimensions = Vector3i::new(t.dimensions.x, v, t.dimensions.z);
                }
                "cell_size_x" => {
                    let v = value.to::<f64>() as f32;
                    t.cell_size = Vector2::new(v, t.cell_size.y);
                }
                "cell_size_z" => {
                    let v = value.to::<f64>() as f32;
                    t.cell_size = Vector2::new(t.cell_size.x, v);
                }
                "blend_mode" => {
                    t.blend_mode = value.to::<i64>() as i32;
                }
                "wall_threshold" => {
                    t.wall_threshold = value.to::<f64>() as f32;
                }
                "ridge_threshold" => {
                    t.ridge_threshold = value.to::<f64>() as f32;
                }
                "ledge_threshold" => {
                    t.ledge_threshold = value.to::<f64>() as f32;
                }
                "merge_mode" => {
                    t.merge_mode = value.to::<i64>() as i32;
                }
                "grass_subdivisions" => {
                    t.grass_subdivisions = value.to::<f64>() as i32;
                }
                "grass_size_x" => {
                    let v = value.to::<f64>() as f32;
                    t.grass_size = Vector2::new(v, t.grass_size.y);
                }
                "grass_size_y" => {
                    let v = value.to::<f64>() as f32;
                    t.grass_size = Vector2::new(t.grass_size.x, v);
                }
                "default_wall_texture" => {
                    t.default_wall_texture = value.to::<f64>() as i32;
                }
                "blend_sharpness" => {
                    t.blend_sharpness = value.to::<f64>() as f32;
                }
                "blend_noise_scale" => {
                    t.blend_noise_scale = value.to::<f64>() as f32;
                }
                "blend_noise_strength" => {
                    t.blend_noise_strength = value.to::<f64>() as f32;
                }
                "animation_fps" => {
                    t.animation_fps = value.to::<f64>() as i32;
                }
                "use_ridge_texture" => {
                    t.use_ridge_texture = value.to();
                }
                "extra_collision_layer" => {
                    // Value is index 0-23, convert to layer 9-32
                    t.extra_collision_layer = value.to::<i64>() as i32 + 9;
                }
                _ if name.starts_with("tex_scale_") => {
                    let slot: usize = name["tex_scale_".len()..].parse().unwrap_or(1);
                    let v = value.to::<f64>() as f32;
                    match slot {
                        1 => t.texture_scale_1 = v,
                        2 => t.texture_scale_2 = v,
                        3 => t.texture_scale_3 = v,
                        4 => t.texture_scale_4 = v,
                        5 => t.texture_scale_5 = v,
                        6 => t.texture_scale_6 = v,
                        7 => t.texture_scale_7 = v,
                        8 => t.texture_scale_8 = v,
                        9 => t.texture_scale_9 = v,
                        10 => t.texture_scale_10 = v,
                        11 => t.texture_scale_11 = v,
                        12 => t.texture_scale_12 = v,
                        13 => t.texture_scale_13 = v,
                        14 => t.texture_scale_14 = v,
                        15 => t.texture_scale_15 = v,
                        _ => {}
                    }
                }
                _ if name.starts_with("tex_has_grass_") => {
                    let slot: usize = name["tex_has_grass_".len()..].parse().unwrap_or(2);
                    let v: bool = value.to();
                    match slot {
                        2 => t.tex2_has_grass = v,
                        3 => t.tex3_has_grass = v,
                        4 => t.tex4_has_grass = v,
                        5 => t.tex5_has_grass = v,
                        6 => t.tex6_has_grass = v,
                        _ => {}
                    }
                }
                _ if name.starts_with("ground_color_") => {
                    let slot: usize = name["ground_color_".len()..].parse().unwrap_or(1);
                    let v: Color = value.to();
                    match slot {
                        1 => t.ground_color = v,
                        2 => t.ground_color_2 = v,
                        3 => t.ground_color_3 = v,
                        4 => t.ground_color_4 = v,
                        5 => t.ground_color_5 = v,
                        6 => t.ground_color_6 = v,
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Sync shader uniforms after changing any terrain setting
        terrain.bind_mut().force_batch_update();
    }
```

**Design note on the block scope:** The `{ let mut t = terrain.bind_mut(); ... }` block is critical. `force_batch_update()` at the bottom also calls `bind_mut()`, which would panic if the first borrow was still alive. The explicit block drops `t` before the second borrow begins.

**Design note on `extra_collision_layer`:** The dropdown index starts at 0 (for layer 9) and goes to 23 (for layer 32). We add 9 to convert back to an absolute layer number. This offset arithmetic matches the reverse conversion in `rebuild_attributes_impl` where we subtract 9 for display.

**Design note on the prefix matching arms:** The `_ if name.starts_with(...)` pattern at the end of the match handles dynamically-named settings from the texture panel. These cannot be literal match arms because the slot number is embedded in the string. We parse the slot number with `name[prefix.len()..].parse()` and dispatch to the correct field.

### Step 14: `rebuild_texture_panel_impl()` — the split-borrow pattern

**Why:** The right-side texture settings panel shows 15 texture slots, each with up to 5 controls. Rebuilding this panel requires reading 40+ fields from the terrain, building a large VBox with nested controls, and adding it to the scroll container. The challenge is that the scroll container is stored as `self.texture_panel`, and we also need `self.current_terrain` — this creates a borrow conflict if we try to access both simultaneously.

The solution is the "split-borrow" pattern: borrow `self.texture_panel` to clear its children, drop the borrow, read terrain data into local variables, build the VBox as a free-standing node, then re-borrow `self.texture_panel` to add the completed VBox.

```rust
    /// Rebuild the right-side texture settings panel content.
    /// This is the internal implementation - call via _rebuild_texture_panel_deferred.
    fn rebuild_texture_panel_impl(&mut self, plugin_ref: Gd<PixyTerrainPlugin>) {
        // Clear existing children (borrow scroll, then release)
        if let Some(ref mut scroll) = self.texture_panel {
            let count = scroll.get_child_count();
            for i in (0..count).rev() {
                if let Some(mut child) = scroll.get_child(i) {
                    scroll.remove_child(&child);
                    child.queue_free();
                }
            }
        } else {
            return;
        }

        // Only populate when a terrain is selected
        let terrain_node = match self.current_terrain {
            Some(ref t) if t.is_instance_valid() => t.clone(),
            _ => return,
        };

        let terrain: Gd<PixyTerrain> = terrain_node.cast();
        let t = terrain.bind();

        // Read current values
        let scales = [
            t.texture_scale_1,
            t.texture_scale_2,
            t.texture_scale_3,
            t.texture_scale_4,
            t.texture_scale_5,
            t.texture_scale_6,
            t.texture_scale_7,
            t.texture_scale_8,
            t.texture_scale_9,
            t.texture_scale_10,
            t.texture_scale_11,
            t.texture_scale_12,
            t.texture_scale_13,
            t.texture_scale_14,
            t.texture_scale_15,
        ];
        let has_grass = [
            true, // tex1 always has grass
            t.tex2_has_grass,
            t.tex3_has_grass,
            t.tex4_has_grass,
            t.tex5_has_grass,
            t.tex6_has_grass,
        ];
        let ground_colors = [
            t.ground_color,
            t.ground_color_2,
            t.ground_color_3,
            t.ground_color_4,
            t.ground_color_5,
            t.ground_color_6,
        ];
        // Collect ground textures (15 slots)
        let ground_textures: [Option<Gd<godot::classes::Texture2D>>; 15] = [
            t.ground_texture.clone(),
            t.texture_2.clone(),
            t.texture_3.clone(),
            t.texture_4.clone(),
            t.texture_5.clone(),
            t.texture_6.clone(),
            t.texture_7.clone(),
            t.texture_8.clone(),
            t.texture_9.clone(),
            t.texture_10.clone(),
            t.texture_11.clone(),
            t.texture_12.clone(),
            t.texture_13.clone(),
            t.texture_14.clone(),
            t.texture_15.clone(),
        ];
        // Collect grass sprites (6 slots)
        let grass_sprites: [Option<Gd<godot::classes::Texture2D>>; 6] = [
            t.grass_sprite.clone(),
            t.grass_sprite_tex_2.clone(),
            t.grass_sprite_tex_3.clone(),
            t.grass_sprite_tex_4.clone(),
            t.grass_sprite_tex_5.clone(),
            t.grass_sprite_tex_6.clone(),
        ];
        drop(t);
```

**Design note on explicit `drop(t)`:** The `t = terrain.bind()` borrows the terrain immutably. We need to release this borrow before the loop below calls `Callable::from_object_method(&plugin_ref, ...)`, which may indirectly trigger operations that need the terrain. The explicit `drop(t)` makes the borrow lifetime crystal clear.

Now build the VBox with all 15 texture slots:

```rust
        let mut vbox = VBoxContainer::new_alloc();
        vbox.set_name("TextureSettingsVBox");
        vbox.add_theme_constant_override("separation", 6);

        let mut header = Label::new_alloc();
        header.set_text("Texture Settings");
        vbox.add_child(&header);

        // 15 texture slots
        for slot in 1..=15i32 {
            let sep = HSeparator::new_alloc();
            vbox.add_child(&sep);

            let mut slot_label = Label::new_alloc();
            slot_label.set_text(&format!("Texture {slot}"));
            vbox.add_child(&slot_label);

            // Ground texture picker
            let tex_name = format!("ground_tex_{slot}");
            let mut tex_label = Label::new_alloc();
            tex_label.set_text("Ground Texture");

            let mut tex_picker = EditorResourcePicker::new_alloc();
            tex_picker.set_base_type("Texture2D");
            if let Some(ref tex) = ground_textures[(slot - 1) as usize] {
                tex_picker.set_edited_resource(tex);
            }
            tex_picker.set_custom_minimum_size(Vector2::new(180.0, 28.0));

            let callable = Callable::from_object_method(&plugin_ref, "on_texture_resource_changed")
                .bindv(&varray![tex_name.to_variant()]);
            tex_picker.connect("resource_changed", &callable);

            vbox.add_child(&tex_label);
            vbox.add_child(&tex_picker);

            // UV scale slider
            let scale_name = format!("tex_scale_{slot}");
            let mut scale_label = Label::new_alloc();
            scale_label.set_text(&format!("Scale: {:.1}", scales[(slot - 1) as usize]));
            scale_label.set_name(&format!("{scale_name}_label"));

            let mut scale_slider = HSlider::new_alloc();
            scale_slider.set_min(0.1);
            scale_slider.set_max(40.0);
            scale_slider.set_step(0.1);
            scale_slider.set_value(scales[(slot - 1) as usize] as f64);
            scale_slider.set_custom_minimum_size(Vector2::new(180.0, 0.0));

            let callable = Callable::from_object_method(&plugin_ref, "on_attribute_changed")
                .bindv(&varray![scale_name.to_variant()]);
            scale_slider.connect("value_changed", &callable);

            vbox.add_child(&scale_label);
            vbox.add_child(&scale_slider);

            // Ground color picker (slots 1-6 only)
            if slot <= 6 {
                let color_name = format!("ground_color_{slot}");
                let mut color_label = Label::new_alloc();
                color_label.set_text("Ground Color");

                let mut color_picker = ColorPickerButton::new_alloc();
                color_picker.set_pick_color(ground_colors[(slot - 1) as usize]);
                color_picker.set_custom_minimum_size(Vector2::new(180.0, 28.0));

                let callable = Callable::from_object_method(&plugin_ref, "on_attribute_changed")
                    .bindv(&varray![color_name.to_variant()]);
                color_picker.connect("color_changed", &callable);

                vbox.add_child(&color_label);
                vbox.add_child(&color_picker);

                // Grass sprite picker
                let sprite_name = format!("grass_sprite_{slot}");
                let mut sprite_label = Label::new_alloc();
                sprite_label.set_text("Grass Sprite");

                let mut sprite_picker = EditorResourcePicker::new_alloc();
                sprite_picker.set_base_type("Texture2D");
                if let Some(ref tex) = grass_sprites[(slot - 1) as usize] {
                    sprite_picker.set_edited_resource(tex);
                }
                sprite_picker.set_custom_minimum_size(Vector2::new(180.0, 28.0));

                let callable =
                    Callable::from_object_method(&plugin_ref, "on_texture_resource_changed")
                        .bindv(&varray![sprite_name.to_variant()]);
                sprite_picker.connect("resource_changed", &callable);

                vbox.add_child(&sprite_label);
                vbox.add_child(&sprite_picker);
            }

            // Has grass checkbox (slots 2-6 only, slot 1 always has grass)
            if (2..=6).contains(&slot) {
                let grass_name = format!("tex_has_grass_{slot}");
                let mut grass_cb = CheckBox::new_alloc();
                grass_cb.set_text("Has Grass");
                grass_cb.set_pressed(has_grass[(slot - 1) as usize]);

                let callable = Callable::from_object_method(&plugin_ref, "on_attribute_changed")
                    .bindv(&varray![grass_name.to_variant()]);
                grass_cb.connect("toggled", &callable);

                vbox.add_child(&grass_cb);
            }
        }

        // Re-borrow scroll to add the completed vbox
        if let Some(ref mut scroll) = self.texture_panel {
            scroll.add_child(&vbox);
        }
    }
```

**Design note on the slot layout:**

| Slot | Ground Texture | UV Scale | Ground Color | Grass Sprite | Has Grass |
|------|---------------|----------|--------------|--------------|-----------|
| 1    | Yes           | Yes      | Yes          | Yes          | No (always true) |
| 2-6  | Yes           | Yes      | Yes          | Yes          | Yes       |
| 7-15 | Yes           | Yes      | No           | No           | No        |

Slots 7-15 are "extended" texture slots that only support a ground texture and UV scale. The first 6 slots correspond to the terrain's color-blend system and grass planting, which is why they get the extra controls.

**Design note on `EditorResourcePicker`:** This is an editor-only widget that shows a thumbnail preview and provides drag-and-drop support for Godot resources. We set `set_base_type("Texture2D")` so it only accepts textures. The `resource_changed` signal fires when the user assigns or clears a texture, routing through `on_texture_resource_changed`.

**Design note on the re-borrow at the end:** We build the entire VBox as a detached node tree, then add it to the scroll container in a single `add_child` call at the end. This is the completion of the split-borrow pattern — the scroll container is only borrowed twice: once to clear children, once to add the new VBox. Between those two borrows, we hold no reference to `self.texture_panel`, so we are free to borrow `self.current_terrain` and other fields.

## Summary of gdext Patterns in This Part

1. **`Callable::from_object_method(&plugin_ref, "method_name").bindv(&varray![extra])`** — The fundamental signal-routing pattern. The signal's own arguments come first in the callback, bound arguments come after.

2. **`call_deferred("_rebuild_*_deferred", &[])`** — Delays execution to end of frame to avoid borrow conflicts during signal dispatch. Used anywhere a signal handler needs to rebuild UI.

3. **Split-borrow pattern** — Borrow a field, modify it, drop the borrow, then borrow a different field. Critical for `rebuild_texture_panel_impl` where we need both `self.texture_panel` and `self.current_terrain`.

4. **Block-scoped `bind_mut()`** — Wrap `terrain.bind_mut()` in `{ }` blocks so the borrow drops before subsequent `bind_mut()` or `force_batch_update()` calls.

5. **`find_child_ex().recursive(true).owned(false).done()`** — Godot's builder-pattern child search, used to locate named controls for programmatic updates (slider sync, label text updates).

6. **Signal type differences** — `HSlider`/`SpinBox` emit `f64`, `OptionButton` emits `i64`, `CheckBox` emits `bool`, `ColorPickerButton` emits `Color`, `EditorResourcePicker` emits `Variant` (resource or nil). All funnel into `on_attribute_changed(Variant, GString)` where the `Variant` is type-erased and decoded per setting.

## File Summary

All code in this part lives in a single file:

- **`/Users/ladvien/pixy_terrain/rust/src/editor_plugin.rs`** (lines ~833-2282) — `#[func]` callbacks, `rebuild_attributes_impl`, UI helpers, `apply_terrain_setting`, `rebuild_texture_panel_impl`
