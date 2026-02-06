# Part 15 -- Editor Plugin -- Foundation & Input

**Series:** Reconstructing Pixy Terrain
**Part:** 15 of 18
**Previous:** 2026-02-06-resource-types-texture-presets-11.md
**Status:** Complete

## What We're Building

The editor plugin that turns Pixy Terrain from a runtime library into an interactive authoring tool. This part covers the skeleton: the struct that holds all editor state, the lifecycle hooks that build and tear down the UI, and the massive `forward_3d_gui_input()` method that routes every mouse click, drag, and keyboard shortcut to the correct terrain operation.

Godot's `EditorPlugin` class is the gateway to the 3D viewport. By overriding `handles()`, `edit()`, and `forward_3d_gui_input()`, we intercept all user input when a PixyTerrain node is selected. This part wires up the plumbing -- Parts 16-17 will fill in the actual terrain modification logic that this plumbing calls.

## What You'll Have After This

A plugin that compiles, registers with Godot, shows a left-side toolbar with 9 tool mode buttons, a bottom attributes bar, and a right-side texture panel. Clicking in the viewport performs raycasts, captures brush positions, and enters the correct state machine phase (paint, setting, height adjustment, drawing). The plugin intercepts keyboard shortcuts (G for generate, C for clear) and scroll wheel events (shift+scroll for brush resize). No terrain modification happens yet -- the methods `build_draw_pattern()`, `draw_pattern()`, and `rebuild_attributes()` are called but will be implemented in later parts.

## Prerequisites

- Part 11 completed (`PixyQuickPaint`, `PixyTexturePreset`, `PixyTextureList` resources)
- Parts 12-14 completed (gizmo plugin, grass planter -- covered in separate walkthroughs)
- Part 10 completed (`PixyTerrain` with `regenerate()`, `clear()`, `has_chunk()`, `add_new_chunk()`, `remove_chunk()`)

## Steps

### Step 1: Imports and constants

**Why:** The editor plugin touches nearly every part of the Godot editor API: buttons, sliders, containers, input events, physics raycasting, and the undo/redo system. We also need our own crate modules for the gizmo, marching squares color encoding, quick paint presets, and the terrain node itself.

The constants at the top define the toolbar's visual layout. Centralizing these avoids magic numbers scattered through `enter_tree()` and makes it easy to tweak the look later.

**File:** `rust/src/editor_plugin.rs`

```rust
use std::collections::HashMap;

use godot::classes::editor_plugin::AfterGuiInput;
use godot::classes::editor_plugin::CustomControlContainer;
use godot::classes::{
    Button, ButtonGroup, Camera3D, CenterContainer, CheckBox, ColorPickerButton, EditorPlugin,
    EditorResourcePicker, HBoxContainer, HSeparator, HSlider, IEditorPlugin, Input, InputEvent,
    InputEventKey, InputEventMouseButton, InputEventMouseMotion, Label, MarginContainer,
    OptionButton, PhysicsRayQueryParameters3D, ScrollContainer, SpinBox, StaticBody3D,
    VBoxContainer,
};
use godot::prelude::*;

use crate::gizmo::{self, GizmoState, PixyTerrainGizmoPlugin};
use crate::marching_squares;
use crate::quick_paint::PixyQuickPaint;
use crate::terrain::PixyTerrain;

/// Minimum width of the toolbar panel.
const TOOLBAR_MIN_WIDTH: f32 = 140.0;
/// Padding around toolbar content.
const TOOLBAR_MARGIN: i32 = 8;
/// Vertical separation between toolbar items.
const TOOLBAR_SEPARATION: i32 = 4;
/// Minimum button size for toolbar buttons.
const BUTTON_MIN_WIDTH: f32 = 100.0;
/// Minimum button height for toolbar buttons.
const BUTTON_MIN_HEIGHT: f32 = 28.0;
/// Maximum brush size.
const MAX_BRUSH_SIZE: f32 = 50.0;
/// Minimum brush size.
const MIN_BRUSH_SIZE: f32 = 1.0;
/// Scroll wheel brush size step.
const BRUSH_SIZE_STEP: f32 = 0.5;
```

**What's happening:**

The import list is long but deliberate. Every type here is used somewhere in the file. A few notable ones:

- `AfterGuiInput` and `CustomControlContainer` are enum-like types from the `editor_plugin` submodule. They control whether the plugin consumes or passes through input events, and where UI containers are docked in the editor.
- `PhysicsRayQueryParameters3D` is the raycast configuration object. We need it to probe the terrain's collision bodies.
- `Input` is the singleton that tells us whether Shift/Ctrl/Alt are currently held.
- `StaticBody3D` is imported for the collision wireframe toggle -- we iterate chunk children looking for collision bodies.

The constants use `f32` and `i32` because that is what the Godot API expects for control sizes and theme overrides, respectively.

### Step 2: Helper functions -- lerp_f32() and godot_ease()

**Why:** Two small utility functions that the drawing logic needs. `lerp_f32()` is a standard linear interpolation. `godot_ease()` replicates Godot's `@GlobalScope.ease()` function, which the bridge tool uses to create non-linear slopes between two height points.

```rust
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Replicates Godot's @GlobalScope.ease() function.
/// See: https://docs.godotengine.org/en/stable/classes/class_%40globalscope.html#class-globalscope-method-ease
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

**What's happening:**

`godot_ease()` has four curve regions that produce different easing behaviors:

1. **curve > 1.0** -- `x.powf(curve)` produces an ease-in (slow start, fast finish). Higher curve values make the start slower.
2. **0.0 < curve < 1.0** -- `1.0 - (1.0 - x).powf(1.0 / curve)` produces an ease-out (fast start, slow finish). The reciprocal flips the power curve.
3. **curve < 0.0** -- A symmetric ease-in-out using the absolute value of curve. It splits at x=0.5: the first half uses `(2x)^(-curve) * 0.5`, the second half mirrors it. The negative sign is a Godot convention -- negative curve values mean "apply S-curve easing."
4. **curve == 0.0** -- Returns 0.0 (step function, effectively no transition).

The clamp at the top (returning 0 for x<0 and 1 for x>1) ensures the function is well-behaved even if the caller passes values outside [0,1], which can happen during bridge calculations when brush positions extend slightly beyond the start/end points.

We define `lerp_f32()` rather than using a Godot utility because gdext does not expose `@GlobalScope.lerp()` as a free function, and pulling in a crate for one line of arithmetic is not worthwhile.

### Step 3: Enums -- TerrainToolMode and BrushType

**Why:** The plugin supports 9 distinct editing modes and 2 brush shapes. Encoding these as Rust enums gives us exhaustive `match` checking -- the compiler will warn if we add a new mode but forget to handle it somewhere.

```rust
// =======================================
// Enums
// =======================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainToolMode {
    #[default]
    Height = 0,
    Level = 1,
    Smooth = 2,
    Bridge = 3,
    GrassMask = 4,
    VertexPaint = 5,
    DebugBrush = 6,
    ChunkManagement = 7,
    TerrainSettings = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushType {
    #[default]
    Round = 0,
    Square = 1,
}
```

**What's happening:**

The integer discriminants (`= 0`, `= 1`, etc.) match the indices of the toolbar buttons created in `enter_tree()`. When a button fires the `toggled` signal, it passes its index as a bound argument, and we match that integer back to the enum variant. The discriminants also serve as documentation -- you can read the enum definition and immediately know which button index maps to which mode.

The nine modes group into three logical categories that the toolbar separates with `HSeparator`s:

- **Landscape tools** (0-3): Height, Level, Smooth, Bridge -- these modify terrain geometry.
- **Visual tools** (4-5): GrassMask, VertexPaint -- these modify appearance without changing height.
- **Utility tools** (6-8): DebugBrush, ChunkManagement, TerrainSettings -- diagnostic and configuration modes.

`BrushType` controls how the brush area is calculated in `build_draw_pattern()` (Part 16). Round uses distance-from-center; Square uses a bounding box.

### Step 4: The PixyTerrainPlugin struct

**Why:** This is the central nervous system of the editor. It holds every piece of mutable state that the plugin needs across frames: which terrain is selected, which tool mode is active, the current brush position, the accumulated draw pattern, bridge start positions, vertex paint colors, and handles to every UI widget. Godot's `EditorPlugin` has no persistent state of its own -- you must store everything in your struct fields.

```rust
// =======================================
// Plugin Struct
// =======================================

#[derive(GodotClass)]
#[class(tool, init, base=EditorPlugin)]
pub struct PixyTerrainPlugin {
    base: Base<EditorPlugin>,

    // UI state
    #[init(val = None)]
    current_terrain: Option<Gd<Node>>,
    #[init(val = None)]
    margin_container: Option<Gd<MarginContainer>>,
    #[init(val = None)]
    toolbar: Option<Gd<VBoxContainer>>,
    #[init(val = None)]
    generate_button: Option<Gd<Button>>,
    #[init(val = None)]
    clear_button: Option<Gd<Button>>,
    #[init(val = Vec::new())]
    tool_buttons: Vec<Gd<Button>>,
    #[init(val = None)]
    attributes_container: Option<Gd<ScrollContainer>>,
    #[init(val = None)]
    attributes_hbox: Option<Gd<HBoxContainer>>,
    #[init(val = false)]
    is_modifying: bool,

    // Tool mode
    #[init(val = TerrainToolMode::Height)]
    mode: TerrainToolMode,
    #[init(val = BrushType::Round)]
    brush_type: BrushType,
    #[init(val = 15.0)]
    brush_size: f32,
    #[init(val = 1.0)]
    strength: f32,
    /// Target height for Level mode.
    #[init(val = 0.0)]
    height: f32,
    #[init(val = true)]
    flatten: bool,
    #[init(val = true)]
    falloff: bool,
    /// Ease value for bridge mode (-1.0 = no ease).
    #[init(val = -1.0)]
    ease_value: f32,
    #[init(val = false)]
    should_mask_grass: bool,

    // Vertex paint state
    #[init(val = 0)]
    vertex_color_idx: i32,
    #[init(val = Color::from_rgba(1.0, 0.0, 0.0, 0.0))]
    vertex_color_0: Color,
    #[init(val = Color::from_rgba(1.0, 0.0, 0.0, 0.0))]
    vertex_color_1: Color,
    #[init(val = false)]
    paint_walls_mode: bool,

    // Drawing state
    #[init(val = Vector3::ZERO)]
    brush_position: Vector3,
    #[init(val = false)]
    terrain_hovered: bool,
    #[init(val = HashMap::new())]
    current_draw_pattern: HashMap<[i32; 2], HashMap<[i32; 2], f32>>,
    #[init(val = false)]
    is_drawing: bool,
    #[init(val = false)]
    draw_height_set: bool,
    #[init(val = 0.0)]
    draw_height: f32,
    #[init(val = false)]
    is_setting: bool,
    /// Original click position for height drag calculations (two-click workflow).
    #[init(val = Vector3::ZERO)]
    setting_start_position: Vector3,

    // Gizmo plugin
    #[init(val = None)]
    gizmo_plugin: Option<Gd<PixyTerrainGizmoPlugin>>,

    // Right-side texture settings panel
    #[init(val = None)]
    texture_panel: Option<Gd<ScrollContainer>>,

    // Bridge state
    #[init(val = false)]
    is_making_bridge: bool,
    #[init(val = Vector3::ZERO)]
    bridge_start_pos: Vector3,
    #[init(val = Vector3::ZERO)]
    base_position: Vector3,
    /// Chunk where bridge started (for cross-chunk offset calculation).
    #[init(val = Vector2i::ZERO)]
    bridge_start_chunk: Vector2i,

    // QuickPaint presets
    #[init(val = Vec::new())]
    quick_paint_presets: Vec<Gd<PixyQuickPaint>>,
    #[init(val = None)]
    current_quick_paint: Option<Gd<PixyQuickPaint>>,

    // Collision debug toggle
    #[init(val = false)]
    show_collision_wireframes: bool,
    #[init(val = None)]
    collision_toggle_button: Option<Gd<CheckBox>>,

    // Chunk management state
    #[init(val = None)]
    selected_chunk_coords: Option<Vector2i>,
}
```

**What's happening:**

The `#[class(tool, init, base=EditorPlugin)]` attribute does three things:

1. `tool` -- This class runs in the editor, not just at game runtime. Without this, Godot would ignore it during editing.
2. `init` -- gdext generates a default constructor using the `#[init(val = ...)]` annotations on each field. This is required because Godot must be able to instantiate the plugin without arguments.
3. `base=EditorPlugin` -- The class extends Godot's `EditorPlugin`, which gives us access to `forward_3d_gui_input()`, `add_control_to_container()`, `get_undo_redo()`, and the other editor integration points.

The fields break into six categories:

**UI State** -- Handles to every widget we create in `enter_tree()`. We store them as `Option<Gd<T>>` because they do not exist until the plugin enters the tree, and we need to clean them up in `exit_tree()`. The `current_terrain` field tracks which `PixyTerrain` node is currently selected in the editor. `is_modifying` is a reentrancy guard -- when we call `regenerate()` on the terrain, Godot may fire `edit(None)` as the scene tree changes, which would clear our terrain reference. Setting `is_modifying = true` tells `make_visible()` to ignore those spurious deselection events.

**Tool Mode** -- The current brush configuration. `flatten` controls whether subsequent strokes stay at the height of the first click (true) or follow the terrain surface (false). `falloff` enables smoothstep distance falloff from brush center. `ease_value` controls the bridge slope curve -- a value of -1.0 means "linear, no easing."

**Vertex Paint State** -- The two vertex colors encode a texture index (see Part 03). `vertex_color_idx` is the human-readable slot number (0-15); `vertex_color_0` and `vertex_color_1` are the actual color values written to mesh vertices.

**Drawing State** -- The most complex group. The `current_draw_pattern` is a nested HashMap: outer key is chunk coordinates `[i32; 2]`, inner key is cell coordinates `[i32; 2]`, value is the brush falloff weight (0.0 to 1.0). This structure allows a single brush stroke to span multiple chunks. `is_setting` and `draw_height_set` form a two-phase state machine for the Height tool's two-click workflow (explained in detail in Step 9).

**Bridge State** -- Bridge mode needs to remember where the user first clicked (`bridge_start_pos`) and which chunk that click landed in (`bridge_start_chunk`), because the slope calculation must account for cross-chunk coordinate offsets.

**QuickPaint Presets** -- The plugin can hold multiple `PixyQuickPaint` resources (from Part 11) and apply them automatically during height operations, painting the correct ground/wall texture alongside the height change.

### Step 5: enter_tree() -- Building the toolbar UI

**Why:** `enter_tree()` is called once when Godot loads the plugin. This is where we construct the entire editor UI: a left-side toolbar with generation buttons and tool mode toggles, a bottom attributes bar for per-mode settings, a gizmo plugin for 3D viewport overlays, and a right-side texture settings panel.

```rust
// =======================================
// IEditorPlugin Implementation
// =======================================

#[godot_api]
impl IEditorPlugin for PixyTerrainPlugin {
    fn enter_tree(&mut self) {
        godot_print!("PixyTerrainPlugin: enter_tree called");

        let mut margin_container = MarginContainer::new_alloc();
        margin_container.set_name("PixyTerrainMargin");
        margin_container.set_visible(false);
        margin_container.set_custom_minimum_size(Vector2::new(TOOLBAR_MIN_WIDTH, 0.0));
        margin_container.add_theme_constant_override("margin_top", TOOLBAR_MARGIN);
        margin_container.add_theme_constant_override("margin_left", TOOLBAR_MARGIN);
        margin_container.add_theme_constant_override("margin_right", TOOLBAR_MARGIN);
        margin_container.add_theme_constant_override("margin_bottom", TOOLBAR_MARGIN);

        let mut toolbar = VBoxContainer::new_alloc();
        toolbar.set_name("PixyTerrainToolbar");
        toolbar.add_theme_constant_override("separation", TOOLBAR_SEPARATION);

        // Generation Section
        let mut gen_label = Label::new_alloc();
        gen_label.set_text("Generation");
        toolbar.add_child(&gen_label);

        let mut generate_button = Button::new_alloc();
        generate_button.set_text("Generate (G)");
        generate_button.set_custom_minimum_size(Vector2::new(BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT));

        let mut clear_button = Button::new_alloc();
        clear_button.set_text("Clear (C)");
        clear_button.set_custom_minimum_size(Vector2::new(BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT));

        toolbar.add_child(&generate_button);
        toolbar.add_child(&clear_button);

        // -- Tool Mode Buttons --
        let sep = HSeparator::new_alloc();
        toolbar.add_child(&sep);

        let mut tools_label = Label::new_alloc();
        tools_label.set_text("Tools");
        toolbar.add_child(&tools_label);

        let button_group = ButtonGroup::new_gd();
        let tool_labels = [
            "Height",
            "Level",
            "Smooth",
            "Bridge",
            "Grass Mask",
            "Vertex Paint",
            "Debug",
            "Chunks",
            "Settings",
        ];
        let tool_tooltips = [
            "Height Tool\n\nElevate or lower terrain height.\n\n[Shortcuts]\n\
             \u{2022} Click+Drag: Set height by dragging up/down\n\
             \u{2022} Shift+Click+Drag: Paint selection continuously\n\
             \u{2022} Shift+Scroll: Adjust brush size\n\
             \u{2022} Alt: Clear current selection",
            "Level Tool\n\nSet terrain to a specific height.\n\n[Shortcuts]\n\
             \u{2022} Ctrl+Click: Sample height from terrain\n\
             \u{2022} Shift+Click+Drag: Paint at set height",
            "Smooth Tool\n\nSmooth out rough terrain areas.\n\n[Shortcuts]\n\
             \u{2022} Shift+Click+Drag: Smooth terrain",
            "Bridge Tool\n\nCreate slopes between two points.\n\n[Shortcuts]\n\
             \u{2022} Click start, drag to end\n\u{2022} Ease controls slope curve",
            "Grass Mask Tool\n\nEnable/disable grass on terrain.\n\n[Shortcuts]\n\
             \u{2022} Click to toggle grass mask",
            "Vertex Paint Tool\n\nPaint texture materials on terrain.\n\n[Shortcuts]\n\
             \u{2022} Select material slot first\n\
             \u{2022} Paint Walls: toggle wall vs floor painting",
            "Debug Brush\n\nPrint cell data to console.\n\nUseful for debugging terrain data.",
            "Chunk Management\n\nAdd/remove terrain chunks.\n\n[Shortcuts]\n\
             \u{2022} Click empty area: Add chunk (if adjacent)\n\
             \u{2022} Click existing chunk: Remove chunk",
            "Terrain Settings\n\nAdjust global terrain parameters.\n\n\
             Dimensions, cell size, blend mode, etc.",
        ];

        let plugin_ref = self.to_gd();
        let mut tool_buttons: Vec<Gd<Button>> = Vec::new();

        for (i, label) in tool_labels.iter().enumerate() {
            // Add separators before visual, utility, and settings groups
            if i == 4 || i == 6 || i == 7 {
                let group_sep = HSeparator::new_alloc();
                toolbar.add_child(&group_sep);
            }

            let mut btn = Button::new_alloc();
            btn.set_text(*label);
            btn.set_tooltip_text(tool_tooltips[i]);
            btn.set_toggle_mode(true);
            btn.set_button_group(&button_group);
            btn.set_custom_minimum_size(Vector2::new(BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT));

            let callable = Callable::from_object_method(&plugin_ref, "on_tool_button_toggled")
                .bindv(&varray![i as i32]);
            btn.connect("toggled", &callable);

            toolbar.add_child(&btn);
            tool_buttons.push(btn);
        }

        // -- Debug Options --
        let debug_sep = HSeparator::new_alloc();
        toolbar.add_child(&debug_sep);

        let mut collision_toggle = CheckBox::new_alloc();
        collision_toggle.set_text("Show Colliders");
        collision_toggle.set_tooltip_text("Toggle collision wireframe visibility");
        collision_toggle.set_pressed(false);
        collision_toggle.set_custom_minimum_size(Vector2::new(BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT));
        let collision_callable =
            Callable::from_object_method(&plugin_ref, "on_collision_toggle_changed");
        collision_toggle.connect("toggled", &collision_callable);
        toolbar.add_child(&collision_toggle);

        // Pre-press Brush button (deferred to avoid triggering signal during enter_tree)
        if let Some(first_btn) = tool_buttons.first_mut() {
            first_btn.call_deferred("set_pressed", &[true.to_variant()]);
        }

        margin_container.add_child(&toolbar);

        // Connect generation signals
        generate_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_generate_pressed"),
        );
        clear_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_clear_pressed"),
        );

        self.base_mut().add_control_to_container(
            CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT,
            &margin_container,
        );

        // -- Bottom Attributes Panel --
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_name("PixyTerrainAttributes");
        scroll.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        scroll.set_vertical_scroll_mode(godot::classes::scroll_container::ScrollMode::DISABLED);
        scroll.set_visible(false);

        let hbox = HBoxContainer::new_alloc();
        scroll.add_child(&hbox);

        self.base_mut()
            .add_control_to_container(CustomControlContainer::SPATIAL_EDITOR_BOTTOM, &scroll);

        self.attributes_container = Some(scroll);
        self.attributes_hbox = Some(hbox);
        self.margin_container = Some(margin_container);
        self.toolbar = Some(toolbar);
        self.generate_button = Some(generate_button);
        self.clear_button = Some(clear_button);
        self.tool_buttons = tool_buttons;
        self.collision_toggle_button = Some(collision_toggle);

        // Register gizmo plugin
        let mut gizmo_plugin = Gd::<PixyTerrainGizmoPlugin>::default();
        gizmo::init_gizmo_plugin(&mut gizmo_plugin);
        gizmo_plugin.bind_mut().plugin_ref = Some(self.to_gd());
        self.base_mut().add_node_3d_gizmo_plugin(&gizmo_plugin);
        self.gizmo_plugin = Some(gizmo_plugin);

        // -- Right-Side Texture Settings Panel --
        let mut tex_scroll = ScrollContainer::new_alloc();
        tex_scroll.set_name("PixyTerrainTextureSettings");
        tex_scroll.set_custom_minimum_size(Vector2::new(220.0, 0.0));
        tex_scroll.set_visible(false);

        self.base_mut().add_control_to_container(
            CustomControlContainer::SPATIAL_EDITOR_SIDE_RIGHT,
            &tex_scroll,
        );
        self.texture_panel = Some(tex_scroll);

        godot_print!("PixyTerrainPlugin: toolbar added");
    }
```

**What's happening:**

This method builds three distinct UI regions and registers one gizmo plugin. Let's walk through each.

**Left toolbar (MarginContainer -> VBoxContainer):**

The `MarginContainer` wraps a `VBoxContainer` with 8px padding on all sides. The VBox stacks items vertically with 4px separation. This structure is standard Godot UI -- the margin container provides spacing, the VBox provides layout.

The toolbar starts hidden (`set_visible(false)`) and is shown only when the user selects a PixyTerrain node (via `set_ui_visible()` in the `edit()` callback).

**Generation buttons:**

Two plain `Button`s -- "Generate (G)" and "Clear (C)". Their `pressed` signals connect to `on_generate_pressed` and `on_clear_pressed` `#[func]` methods. The keyboard shortcuts (G/C) are handled separately in `forward_3d_gui_input()`, not through Godot's InputMap system, because editor plugins need to intercept input before it reaches the rest of the editor.

**Tool mode buttons with ButtonGroup:**

This is the most important UI pattern in the method. `ButtonGroup::new_gd()` creates a shared group that enforces radio-button behavior -- only one button in the group can be pressed at a time. Each button:

1. Gets `set_toggle_mode(true)` to act as a toggle rather than a momentary press.
2. Gets `set_button_group(&button_group)` to join the radio group.
3. Connects its `toggled` signal to `on_tool_button_toggled` with a bound argument.

The signal routing uses `Callable::from_object_method(&plugin_ref, "on_tool_button_toggled").bindv(&varray![i as i32])`. This is a critical gdext pattern:

- `from_object_method` creates a Callable that will call the `on_tool_button_toggled` `#[func]` method on this plugin instance.
- `.bindv(&varray![i as i32])` appends the button index as an extra argument. When Godot fires the `toggled` signal, it passes `(pressed: bool)` as the signal argument, then the bound argument is appended, so the method receives `(pressed: bool, tool_index: i32)`.

The separators at indices 4, 6, and 7 create visual groupings: landscape tools (0-3), then a separator, visual tools (4-5), separator, debug (6), separator, management/settings (7-8).

**Deferred button press:**

```rust
first_btn.call_deferred("set_pressed", &[true.to_variant()]);
```

We want the Height button pressed by default, but calling `set_pressed(true)` directly during `enter_tree()` would fire the `toggled` signal immediately -- while we are still inside `enter_tree()` and the struct is borrowed mutably. Using `call_deferred` schedules the press for the next frame, after `enter_tree()` returns and the borrow is released.

**Collision toggle checkbox:**

A standalone `CheckBox` below the tool buttons. Its `toggled` signal connects directly to `on_collision_toggle_changed` without bound arguments -- the signal already passes the boolean state.

**Bottom attributes panel:**

A `ScrollContainer` with vertical scrolling disabled (it scrolls horizontally for overflow). Inside sits an `HBoxContainer` that will be populated dynamically by `rebuild_attributes()` (Part 16) whenever the tool mode changes. The scroll container is docked at `SPATIAL_EDITOR_BOTTOM`, which places it below the 3D viewport.

**Gizmo plugin registration:**

```rust
let mut gizmo_plugin = Gd::<PixyTerrainGizmoPlugin>::default();
gizmo::init_gizmo_plugin(&mut gizmo_plugin);
gizmo_plugin.bind_mut().plugin_ref = Some(self.to_gd());
self.base_mut().add_node_3d_gizmo_plugin(&gizmo_plugin);
```

`PixyTerrainGizmoPlugin` extends `EditorNode3DGizmoPlugin`, which is a `Resource` (RefCounted). That means we create it with `Gd::<T>::default()`, not `new_alloc()`. `init_gizmo_plugin()` calls `create_materials()` to set up the line materials the gizmo uses for drawing brush circles and chunk grids. We store a reference back to the editor plugin (`plugin_ref`) so the gizmo can read brush state when it redraws.

**Right-side texture panel:**

A `ScrollContainer` docked at `SPATIAL_EDITOR_SIDE_RIGHT`. It starts hidden and is populated by `rebuild_texture_panel()` (Part 17) when a terrain is selected.

**Final field assignments:**

After constructing everything, we store handles to all the widgets in `self`. This is necessary because:
1. `exit_tree()` needs them to clean up.
2. Signal handlers need to read/modify the widgets (e.g., `sync_brush_size_slider()` updates slider values).
3. `set_ui_visible()` toggles visibility on the containers.

### Step 6: exit_tree() -- Cleanup

**Why:** When the plugin is unloaded (editor closing, plugin disabled), we must unregister the gizmo and remove all UI containers. Godot will not clean these up automatically because we added them via `add_control_to_container()`, which transfers ownership to the editor -- but we are responsible for removing them.

```rust
    fn exit_tree(&mut self) {
        // Unregister gizmo plugin
        if let Some(gizmo_plugin) = self.gizmo_plugin.take() {
            self.base_mut().remove_node_3d_gizmo_plugin(&gizmo_plugin);
        }

        self.generate_button = None;
        self.clear_button = None;
        self.collision_toggle_button = None;
        self.tool_buttons.clear();
        self.toolbar = None;
        self.attributes_hbox = None;

        if let Some(mut scroll) = self.attributes_container.take() {
            self.base_mut().remove_control_from_container(
                CustomControlContainer::SPATIAL_EDITOR_BOTTOM,
                &scroll,
            );
            scroll.queue_free();
        }

        if let Some(mut margin) = self.margin_container.take() {
            self.base_mut().remove_control_from_container(
                CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT,
                &margin,
            );
            margin.queue_free();
        }

        if let Some(mut tex_panel) = self.texture_panel.take() {
            self.base_mut().remove_control_from_container(
                CustomControlContainer::SPATIAL_EDITOR_SIDE_RIGHT,
                &tex_panel,
            );
            tex_panel.queue_free();
        }
    }
```

**What's happening:**

The cleanup follows a specific order:

1. **Unregister gizmo plugin first.** If the gizmo's `redraw()` fires after we have freed the UI but before the gizmo is unregistered, it would try to read plugin state from freed memory. Unregistering first prevents this.

2. **Null out child widget handles.** Setting `generate_button`, `clear_button`, etc. to `None` drops the `Gd<T>` references. These widgets are children of the toolbar and margin container, so they will be freed when their parent is freed -- we do not need to `queue_free()` them individually.

3. **Remove and free top-level containers.** For each container we added via `add_control_to_container()`, we call the corresponding `remove_control_from_container()` to detach it from the editor, then `queue_free()` to schedule it for deletion. The `.take()` pattern moves the value out of the `Option`, leaving `None` behind -- this ensures we cannot accidentally double-free.

### Step 7: handles() and edit() -- Terrain selection detection

**Why:** These two methods tell Godot "this plugin cares about PixyTerrain nodes." `handles()` returns true for PixyTerrain objects, which causes Godot to call `edit()` whenever the user selects or deselects one.

```rust
    fn handles(&self, object: Gd<Object>) -> bool {
        object.get_class() == "PixyTerrain"
    }

    fn edit(&mut self, object: Option<Gd<Object>>) {
        if let Some(obj) = object {
            if let Ok(node) = obj.try_cast::<Node>() {
                self.current_terrain = Some(node);
                self.set_ui_visible(true);
                self.base_mut()
                    .call_deferred("apply_collision_visibility_deferred", &[]);
                return;
            }
        }
        self.set_ui_visible(false);
        self.current_draw_pattern.clear();
        self.is_drawing = false;
        self.draw_height_set = false;
    }

    fn make_visible(&mut self, visible: bool) {
        if !visible && self.is_modifying {
            return;
        }
        self.set_ui_visible(visible);
        if !visible {
            self.current_terrain = None;
        }
    }
```

**What's happening:**

`handles()` uses string comparison against the class name rather than `try_cast`. This is a deliberate choice -- `try_cast::<PixyTerrain>()` would also work, but `get_class()` is slightly cheaper and does not require a mutable borrow.

`edit()` receives `Some(obj)` when a PixyTerrain is selected and `None` when it is deselected. On selection:

1. Store the terrain as a `Gd<Node>` (not `Gd<PixyTerrain>`). We store the base `Node` type to avoid holding a typed reference that would complicate borrow checking. We cast to `Gd<PixyTerrain>` only when we need terrain-specific methods.
2. Show the UI via `set_ui_visible(true)`, which shows all three panels and triggers deferred rebuilds of the attributes and texture panels.
3. Apply collision visibility deferred. This ensures that the "Show Colliders" toggle state is applied to the newly selected terrain's chunks. We use `call_deferred` because the terrain's children may not be fully initialized yet during the `edit()` call.

On deselection, we hide the UI and reset the drawing state. Clearing `current_draw_pattern` prevents a stale pattern from being applied if the user reselects the terrain.

`make_visible()` is Godot's way of telling the plugin to show or hide its UI. The `is_modifying` guard is important: when we call `terrain.regenerate()`, Godot may briefly deselect and reselect the terrain node as the scene tree changes. Without this guard, the UI would flicker off and on. By returning early when `is_modifying` is true, we keep the UI stable during terrain operations.

### Step 8: forward_3d_gui_input() -- The input handler

**Why:** This is the core of the editor plugin -- the method that receives every input event in the 3D viewport and decides what to do with it. It handles keyboard shortcuts, raycasting to find where the mouse points on the terrain, mouse click state transitions, scroll wheel brush resizing, and mouse motion during drawing.

The method returns an `i32` that is either `AfterGuiInput::STOP.ord()` (the plugin consumed the input, do not pass it to the editor) or `AfterGuiInput::PASS.ord()` (let the editor handle it normally, e.g., for camera orbit).

```rust
    fn forward_3d_gui_input(
        &mut self,
        camera: Option<Gd<Camera3D>>,
        event: Option<Gd<InputEvent>>,
    ) -> i32 {
        let Some(event) = event else {
            return AfterGuiInput::PASS.ord();
        };

        // Keyboard shortcuts for Generate / Clear
        if let Ok(key_event) = event.clone().try_cast::<InputEventKey>() {
            if key_event.is_pressed() && !key_event.is_echo() {
                match key_event.get_keycode() {
                    godot::global::Key::G => {
                        self.do_generate();
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::C => {
                        self.do_clear();
                        return AfterGuiInput::STOP.ord();
                    }
                    _ => {}
                }
            }
        }
```

**What's happening:**

The method starts with early exits. If there is no event, pass through. If the event is a key press (not echo -- echo events fire when a key is held down), check for G or C and call the corresponding terrain method. Returning `STOP` prevents the keystrokes from reaching the editor (which might otherwise type "G" into a text field or trigger a Godot shortcut).

The `event.clone().try_cast::<InputEventKey>()` pattern is necessary because `try_cast` consumes the `Gd`, so we clone first to preserve the original for later checks. The `.ord()` call converts the enum to its integer discriminant, which is what the return type requires (gdext uses `i32` for this return value, not the enum directly).

### Step 9: forward_3d_gui_input() -- Raycast strategies

**Why:** Different tool states need different raycast strategies. When setting height (the two-click workflow), we need a vertical plane so mouse movement maps to height changes. When in flatten mode, we need a horizontal plane at the locked height so the brush stays at the right elevation. When in level mode, same thing at the target height. Otherwise, we raycast against the terrain's physics collision bodies.

```rust
        // Only handle mouse events from here
        let Some(camera) = camera else {
            return AfterGuiInput::PASS.ord();
        };

        let Some(terrain_node) = self
            .current_terrain
            .as_ref()
            .filter(|t| t.is_instance_valid())
            .cloned()
        else {
            return AfterGuiInput::PASS.ord();
        };

        // Get mouse position from event
        let mouse_pos;
        let is_button_event;
        let is_motion_event;

        if let Ok(btn) = event.clone().try_cast::<InputEventMouseButton>() {
            mouse_pos = btn.get_position();
            is_button_event = true;
            is_motion_event = false;
        } else if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
            mouse_pos = motion.get_position();
            is_button_event = false;
            is_motion_event = true;
        } else {
            return AfterGuiInput::PASS.ord();
        }

        let terrain_gd: Gd<Node3D> = terrain_node.clone().cast();

        // Compute ray
        let ray_origin = camera.project_ray_origin(mouse_pos);
        let ray_dir = camera.project_ray_normal(mouse_pos);

        let input = Input::singleton();
        let shift_held = input.is_key_pressed(godot::global::Key::SHIFT);
        let alt_held = input.is_key_pressed(godot::global::Key::ALT);
        let ctrl_held = input.is_key_pressed(godot::global::Key::CTRL);

        // Get terrain dimensions
        let terrain: Gd<PixyTerrain> = terrain_node.clone().cast();
        let (dim, cell_size) = {
            let t = terrain.bind();
            (t.dimensions, t.cell_size)
        };
```

**What's happening:**

After confirming we have a camera, a valid terrain, and a mouse event, we extract the mouse position and classify the event type. The `terrain_node.clone().cast()` creates typed handles we will need -- one as `Node3D` for coordinate transforms, one as `PixyTerrain` for terrain-specific data.

The terrain dimensions and cell size are read inside a short `bind()` scope. The `bind()` call borrows the Godot object for reading. We copy the values out immediately and drop the borrow, because we will need to pass the terrain handle to other methods that may also need to bind it.

The modifier key state comes from `Input::singleton()` rather than from the event itself. This is because mouse motion events do not carry modifier key state reliably on all platforms. `Input::singleton()` queries the current keyboard state directly.

Now the four raycast strategies:

```rust
        // -- Brush/drawing tool modes --
        let is_draw_mode = matches!(
            self.mode,
            TerrainToolMode::Height
                | TerrainToolMode::Level
                | TerrainToolMode::Smooth
                | TerrainToolMode::Bridge
                | TerrainToolMode::GrassMask
                | TerrainToolMode::VertexPaint
                | TerrainToolMode::DebugBrush
        );

        if is_draw_mode {
            self.terrain_hovered = false;
            let mut draw_position: Option<Vector3> = None;

            // Raycast strategy depends on current state
            if self.is_setting && self.draw_height_set {
                // Strategy 1: Setting mode - vertical plane through base_position
                let terrain_transform = terrain_gd.get_global_transform();
                let local_ray_dir = terrain_transform.basis.inverse() * ray_dir;
                let set_normal = Vector3::new(local_ray_dir.x, 0.0, local_ray_dir.z).normalized();
                if set_normal.length() > 0.001 {
                    let d = set_normal.dot(self.base_position);
                    let set_plane = Plane::new(set_normal, d);
                    let local_origin = terrain_gd.to_local(ray_origin);
                    if let Some(pos) = set_plane.intersect_ray(local_origin, local_ray_dir) {
                        self.brush_position = pos;
                    }
                }
            } else if !self.current_draw_pattern.is_empty() && self.flatten {
                // Strategy 2: Flatten mode - horizontal plane at draw_height
                let chunk_plane = Plane::new(Vector3::UP, self.draw_height);
                if let Some(world_pos) = chunk_plane.intersect_ray(ray_origin, ray_dir) {
                    draw_position = Some(terrain_gd.to_local(world_pos));
                }
            } else if self.is_drawing && self.mode == TerrainToolMode::Level {
                // Strategy 3: Level drawing mode - horizontal plane at target height
                let level_plane = Plane::new(Vector3::UP, self.height);
                if let Some(world_pos) = level_plane.intersect_ray(ray_origin, ray_dir) {
                    draw_position = Some(terrain_gd.to_local(world_pos));
                }
            } else {
                // Strategy 4: Default - physics raycast
                if let Some(mut world) = camera.get_world_3d() {
                    if let Some(mut space) = world.get_direct_space_state() {
                        let ray_end = ray_origin + ray_dir * 10000.0;
                        let query = PhysicsRayQueryParameters3D::create_ex(ray_origin, ray_end)
                            .collision_mask(1 << 16)
                            .done()
                            .unwrap();
                        let result = space.intersect_ray(&query);
                        if !result.is_empty() {
                            if let Some(pos_variant) = result.get("position") {
                                let world_pos: Vector3 = pos_variant.to();
                                draw_position = Some(terrain_gd.to_local(world_pos));
                            }
                        }
                    }
                }
            }

            let draw_area_hovered = draw_position.is_some();
            if let Some(pos) = draw_position {
                self.terrain_hovered = true;
                // Don't overwrite brush_position when in setting mode (already set above)
                if !(self.is_setting && self.draw_height_set) {
                    self.brush_position = pos;
                }
            }

            // ALT to clear pattern (unless setting)
            if alt_held && !self.is_setting {
                self.current_draw_pattern.clear();
            }
```

**What's happening:**

**Strategy 1 -- Vertical plane (setting mode with height adjustment):** This is the most complex raycast. When the user has placed a pattern (first click) and released (entering height adjustment mode), mouse Y movement should change the height preview. We construct a vertical plane that faces the camera and passes through `base_position`. The plane normal is computed by projecting the camera's ray direction onto the XZ plane -- this makes the plane always face the camera regardless of orbit angle. `Plane::intersect_ray()` returns the point where the mouse ray hits this vertical plane. Only the Y component matters -- it becomes the new preview height.

Note that Strategy 1 writes to `self.brush_position` directly, while Strategies 2-4 write to `draw_position`. This is because Strategy 1 needs to update the brush position even when the ray does not hit the terrain surface.

**Strategy 2 -- Horizontal plane at draw_height (flatten mode):** When flatten is enabled and a pattern exists, the brush should track at the height where the first click landed. This prevents the brush from "following" the terrain surface as it is being modified. The plane is at Y=`draw_height` and the ray intersects it in world space, then converts to local space.

**Strategy 3 -- Horizontal plane at target height (level mode):** Similar to flatten, but uses the user-specified `self.height` value. Level mode sets all cells to a specific height, so the brush tracks at that height.

**Strategy 4 -- Physics raycast (default):** The standard approach. Cast a ray from the camera through the mouse position and intersect it with the terrain's collision bodies. The collision mask `1 << 16` targets layer 17 (bit 16, zero-indexed), which is the layer assigned to chunk collision shapes in Part 07. The `create_ex()` builder pattern uses Godot's extended constructor: `PhysicsRayQueryParameters3D::create_ex(from, to).collision_mask(mask).done()`.

The `draw_area_hovered` flag tracks whether the raycast hit anything. This is used later to decide whether mouse clicks should be processed -- clicking outside the terrain area is ignored.

### Step 10: forward_3d_gui_input() -- Mouse button handling

**Why:** Mouse clicks drive the entire editing workflow. The logic is dense because it must handle: the two-click height workflow, shift+click drawing, ctrl+click sampling, bridge start, mode-specific initialization, and mouse release behavior that varies by mode.

```rust
            // -- Mouse button handling --
            if is_button_event {
                let btn: Gd<InputEventMouseButton> = event.clone().cast();
                if btn.get_button_index() == godot::global::MouseButton::LEFT {
                    // Second click while in height adjustment mode -> apply and reset
                    if btn.is_pressed() && self.is_setting && self.draw_height_set {
                        self.draw_pattern(&terrain, dim, cell_size);
                        self.is_setting = false;
                        self.draw_height_set = false;
                        self.current_draw_pattern.clear();
                        return AfterGuiInput::STOP.ord();
                    }

                    if btn.is_pressed() && draw_area_hovered {
                        // Mode-specific press initialization
                        if self.mode == TerrainToolMode::Bridge && !self.is_making_bridge {
                            self.flatten = false;
                            self.is_making_bridge = true;
                            self.bridge_start_pos = self.brush_position;
                            // Capture chunk where bridge started for cross-chunk offset
                            let chunk_width = (dim.x - 1) as f32 * cell_size.x;
                            let chunk_depth = (dim.z - 1) as f32 * cell_size.y;
                            self.bridge_start_chunk = Vector2i::new(
                                (self.brush_position.x / chunk_width).floor() as i32,
                                (self.brush_position.z / chunk_depth).floor() as i32,
                            );
                        }
                        if self.mode == TerrainToolMode::Smooth && !self.falloff {
                            self.falloff = true;
                        }
                        if matches!(
                            self.mode,
                            TerrainToolMode::GrassMask | TerrainToolMode::DebugBrush
                        ) && self.falloff
                        {
                            self.falloff = false;
                        }
                        if matches!(
                            self.mode,
                            TerrainToolMode::GrassMask
                                | TerrainToolMode::VertexPaint
                                | TerrainToolMode::DebugBrush
                        ) && self.flatten
                        {
                            self.flatten = false;
                        }

                        if self.mode == TerrainToolMode::Level && ctrl_held {
                            // Ctrl+click in Level mode: set target height from click pos
                            self.height = self.brush_position.y;
                        } else if shift_held {
                            // Shift+click: enter drawing mode
                            self.is_drawing = true;
                        } else if matches!(
                            self.mode,
                            TerrainToolMode::Level
                                | TerrainToolMode::Smooth
                                | TerrainToolMode::GrassMask
                                | TerrainToolMode::VertexPaint
                        ) {
                            // Level/Smooth/GrassMask/VertexPaint: simple click-drag-release
                            self.is_drawing = true;
                        } else {
                            // Normal click: enter setting mode (two-click workflow)
                            self.is_setting = true;
                            if !self.flatten {
                                self.draw_height = self.brush_position.y;
                            }
                        }

                        // Initialize draw_height_set (moved from gizmo)
                        self.initialize_draw_state(&terrain, dim, cell_size);

                        // Build initial pattern
                        if self.is_drawing {
                            self.build_draw_pattern(&terrain, dim, cell_size);
                        }
                    } else if !btn.is_pressed() {
                        // Mouse button released
                        if self.is_making_bridge {
                            self.is_making_bridge = false;
                        }
                        if self.is_drawing {
                            self.is_drawing = false;
                            if matches!(
                                self.mode,
                                TerrainToolMode::GrassMask
                                    | TerrainToolMode::Level
                                    | TerrainToolMode::Bridge
                                    | TerrainToolMode::DebugBrush
                            ) {
                                self.draw_pattern(&terrain, dim, cell_size);
                                self.current_draw_pattern.clear();
                            }
                            if matches!(
                                self.mode,
                                TerrainToolMode::Smooth | TerrainToolMode::VertexPaint
                            ) {
                                self.current_draw_pattern.clear();
                            }
                            self.draw_height_set = false;
                        }
                        // Two-click workflow: release enters height adjustment mode
                        if self.is_setting && !self.draw_height_set {
                            // First release: enter height adjustment mode
                            // Pattern is locked, mouse movement adjusts height preview
                            self.draw_height_set = true;
                            // Keep is_setting = true, wait for second click to apply
                        }
                    }
                    return AfterGuiInput::STOP.ord();
                }

                // Shift+scroll wheel: adjust brush size
                if shift_held {
                    let button_idx = btn.get_button_index();
                    let factor = if btn.get_factor() != 0.0 {
                        btn.get_factor()
                    } else {
                        1.0
                    };
                    if button_idx == godot::global::MouseButton::WHEEL_UP {
                        self.brush_size =
                            (self.brush_size + BRUSH_SIZE_STEP * factor).min(MAX_BRUSH_SIZE);
                        self.sync_brush_size_slider();
                        return AfterGuiInput::STOP.ord();
                    } else if button_idx == godot::global::MouseButton::WHEEL_DOWN {
                        self.brush_size =
                            (self.brush_size - BRUSH_SIZE_STEP * factor).max(MIN_BRUSH_SIZE);
                        self.sync_brush_size_slider();
                        return AfterGuiInput::STOP.ord();
                    }
                }
            }
```

**What's happening:**

The mouse button logic is a state machine with several entry points. Let's trace the two most important workflows.

**The two-click height workflow (Height mode, no Shift):**

This is the primary terrain sculpting interaction, ported from Yugen's original GDScript plugin. It works in three phases:

1. **First click (press):** `is_setting` becomes true. If flatten is disabled, `draw_height` captures the current terrain height at the click point. `initialize_draw_state()` is called (Part 16), which may detect that the user clicked on an existing pattern and skip to phase 2. No pattern is built yet.

2. **Drag (motion while button held):** The mouse motion handler (Step 11) calls `build_draw_pattern()` to add cells to the pattern as the user drags. The gizmo renders a preview.

3. **First release:** `is_setting` is still true and `draw_height_set` is false, so we enter the `if self.is_setting && !self.draw_height_set` branch and set `draw_height_set = true`. The pattern is now locked. Mouse movement no longer expands the pattern -- instead, the vertical plane raycast (Strategy 1) updates `brush_position.y`, and the gizmo shows the height preview.

4. **Second click (press):** Now `is_setting && draw_height_set` is true, so we hit the first branch: `draw_pattern()` applies the accumulated pattern to the terrain, then everything resets.

**The shift+click continuous drawing workflow:**

Holding Shift and clicking enters `is_drawing = true` mode. This is simpler: the pattern is built on every mouse motion, and for some modes (Smooth, VertexPaint, GrassMask) it is applied immediately on each motion frame. For other modes (Level, Bridge, DebugBrush), the pattern accumulates and is applied on mouse release. This gives continuous painting for visual modes and batch application for height modes.

**Mode-specific initialization:**

Before entering either workflow, the code enforces mode constraints:

- **Bridge:** Records the start position and chunk coordinates. Flatten is forced off because bridge mode needs to see the actual terrain surface.
- **Smooth:** Forces falloff on (smooth without falloff would apply uniform smoothing, which looks wrong).
- **GrassMask/DebugBrush:** Forces falloff off (binary on/off operations do not use distance weighting).
- **GrassMask/VertexPaint/DebugBrush:** Forces flatten off (these modes do not modify heights).

**Ctrl+click height sampling:**

In Level mode, Ctrl+click sets `self.height` to the clicked position's Y value. This lets artists pick a height from the terrain and then paint other areas to that height -- similar to an eyedropper tool.

**Mouse release behavior:**

The release logic varies by mode because different tools have different commit semantics:

- **GrassMask, Level, Bridge, DebugBrush:** Apply the pattern on release, then clear. These are "paint area, then commit" tools.
- **Smooth, VertexPaint:** Just clear the pattern on release. These modes already applied their changes on each motion frame.
- **Height (two-click):** Do not apply on release. Instead, enter height adjustment mode and wait for the second click.

**Shift+scroll brush resize:**

Mouse wheel events arrive as `InputEventMouseButton` with `WHEEL_UP`/`WHEEL_DOWN` button indices. The `get_factor()` accounts for trackpad scroll sensitivity -- on a trackpad, factor may be a fractional value representing scroll velocity. If factor is 0 (discrete mouse wheel), we default to 1.0. `sync_brush_size_slider()` (Part 16) updates the UI slider to match the new brush size.

All left-click events return `AfterGuiInput::STOP.ord()` to prevent the editor from interpreting clicks as object selection or camera manipulation while editing terrain.

### Step 11: forward_3d_gui_input() -- Mouse motion and gizmo updates

**Why:** Mouse motion during drawing expands the brush pattern. Mouse motion during height adjustment lets the user preview different heights. After any mouse event, we trigger a gizmo redraw so the brush visualization stays current.

```rust
            // -- Mouse motion during paint phase (first click held, dragging) --
            // Build pattern as user drags to expand painted area
            if is_motion_event && self.is_setting && !self.draw_height_set && draw_area_hovered {
                self.build_draw_pattern(&terrain, dim, cell_size);
            }

            // -- Mouse motion in height adjustment mode (after first release) --
            // brush_position.y is updated by the vertical plane raycast above
            // Pattern is locked, just let gizmo redraw show height preview
            if is_motion_event && self.is_setting && self.draw_height_set {
                // Pattern already built, gizmo will show updated height
            }

            // -- Mouse motion while drawing (shift+drag mode) --
            if is_motion_event && draw_area_hovered && self.is_drawing {
                self.build_draw_pattern(&terrain, dim, cell_size);

                // Continuous modes: apply immediately
                if matches!(
                    self.mode,
                    TerrainToolMode::Smooth
                        | TerrainToolMode::VertexPaint
                        | TerrainToolMode::GrassMask
                ) {
                    self.draw_pattern(&terrain, dim, cell_size);
                    self.current_draw_pattern.clear();
                }
            }

            // Trigger gizmo redraw so brush visualization updates
            self.update_gizmos();

            return AfterGuiInput::PASS.ord();
        }
```

**What's happening:**

Three motion cases, each corresponding to a different phase:

1. **Paint phase (first click held, `is_setting && !draw_height_set`):** User is dragging after the first click. `build_draw_pattern()` adds new cells to the pattern based on the current brush position. The pattern grows as the user drags.

2. **Height adjustment phase (`is_setting && draw_height_set`):** The empty `if` block is intentional. The vertical plane raycast (Strategy 1 from Step 9) already updated `brush_position.y`. The gizmo will read this value on its next redraw and show the height preview. No pattern building needed -- the pattern is locked.

3. **Drawing phase (shift+drag, `is_drawing`):** Same as case 1 for pattern building, but continuous modes (Smooth, VertexPaint, GrassMask) also apply immediately and clear. This is what makes painting feel responsive -- each frame's brush application is independent.

`self.update_gizmos()` calls `terrain_3d.update_gizmos()` on the terrain node, which triggers the gizmo plugin's `redraw()` method. This redraws the brush circle/square, the chunk grid overlay, and the pattern preview. We call it on every mouse event to keep the visualization smooth.

The motion handler returns `AfterGuiInput::PASS.ord()`, not STOP. This is intentional -- we want the editor to still process mouse motion for camera tooltips, cursor changes, and other non-conflicting behaviors.

**Note on stub methods:** `build_draw_pattern()`, `draw_pattern()`, and `initialize_draw_state()` are called here but implemented later. `build_draw_pattern()` (Part 16) calculates which terrain cells fall within the brush radius and stores them with falloff weights in `current_draw_pattern`. `draw_pattern()` (Part 16) applies the accumulated pattern to the terrain by modifying height maps, color maps, or grass masks depending on the current tool mode. `initialize_draw_state()` (Part 16) handles the two-click workflow's entry conditions, detecting whether the user clicked on an existing pattern.

### Step 12: forward_3d_gui_input() -- Chunk Management mode

**Why:** Chunk Management mode uses a different interaction model than the brush-based tools. Instead of painting, the user clicks on the terrain grid to add or remove chunks. This code lives outside the `is_draw_mode` block because it uses its own raycast strategy (horizontal plane at Y=0) and its own click handling.

```rust
        // -- Chunk Management mode --
        if self.mode == TerrainToolMode::ChunkManagement {
            let chunk_plane = Plane::new(Vector3::UP, 0.0);
            if let Some(intersection) = chunk_plane.intersect_ray(ray_origin, ray_dir) {
                let chunk_width = (dim.x - 1) as f32 * cell_size.x;
                let chunk_depth = (dim.z - 1) as f32 * cell_size.y;
                let chunk_x = (intersection.x / chunk_width).floor() as i32;
                let chunk_z = (intersection.z / chunk_depth).floor() as i32;

                if is_button_event {
                    let btn: Gd<InputEventMouseButton> = event.clone().cast();
                    if btn.is_pressed()
                        && btn.get_button_index() == godot::global::MouseButton::LEFT
                    {
                        let has = terrain.bind().has_chunk(chunk_x, chunk_z);

                        if has {
                            // Remove existing chunk
                            self.register_chunk_undo_redo(
                                &terrain_node,
                                chunk_x,
                                chunk_z,
                                "remove chunk",
                                true,
                            );
                            return AfterGuiInput::STOP.ord();
                        } else {
                            // Add new chunk if adjacent to existing
                            let t = terrain.bind();
                            let can_add = t.get_chunk_keys().is_empty()
                                || t.has_chunk(chunk_x - 1, chunk_z)
                                || t.has_chunk(chunk_x + 1, chunk_z)
                                || t.has_chunk(chunk_x, chunk_z - 1)
                                || t.has_chunk(chunk_x, chunk_z + 1);
                            drop(t);

                            if can_add {
                                self.register_chunk_undo_redo(
                                    &terrain_node,
                                    chunk_x,
                                    chunk_z,
                                    "add chunk",
                                    false,
                                );
                                return AfterGuiInput::STOP.ord();
                            }
                        }
                    }
                }
            }

            // Consume left clicks in chunk management mode
            if is_button_event {
                let btn: Gd<InputEventMouseButton> = event.clone().cast();
                if btn.is_pressed() && btn.get_button_index() == godot::global::MouseButton::LEFT {
                    return AfterGuiInput::STOP.ord();
                }
            }
        }

        AfterGuiInput::PASS.ord()
    }
}
```

**What's happening:**

The chunk management raycast is simple: intersect the mouse ray with a horizontal plane at Y=0. This gives us a world-space XZ position, which we divide by chunk dimensions to get chunk grid coordinates.

The adjacency check enforces a rule: you can only add a chunk next to an existing chunk (or anywhere if the terrain is empty). This prevents isolated floating chunks that would have no neighbors to share edge data with.

`register_chunk_undo_redo()` (implemented later in the file) creates an undo/redo action through Godot's `EditorUndoRedoManager`. For removal, the "do" action calls `remove_chunk_from_tree()` and the "undo" action calls `add_new_chunk()`. For addition, it is the reverse. This gives full undo/redo support for chunk operations.

The `drop(t)` call after the adjacency check is important. `t` is a `bind()` borrow on the terrain. We must drop it before calling `register_chunk_undo_redo()`, which will need to borrow the terrain again (through the undo/redo system calling `#[func]` methods on it).

The trailing `AfterGuiInput::STOP.ord()` at the bottom of the chunk management block catches left clicks that did not match any action (e.g., clicking in empty space with no adjacent chunks). This prevents stray clicks from selecting other objects in the scene.

### Step 13: Signal handler methods (called from enter_tree signals)

**Why:** The toolbar buttons and attribute controls fire Godot signals that need `#[func]` methods as targets. These are thin wrappers that update plugin state and trigger UI rebuilds.

These methods live in a separate `#[godot_api]` impl block (after line 833, in the `#[func]` Methods section). They are wired up by the `enter_tree()` signal connections and are included here because they complete the picture of how the UI works.

```rust
// =======================================
// #[func] Methods (callable from GDScript / undo-redo)
// =======================================

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

**What's happening:**

`on_tool_button_toggled()` deserves attention. When a ButtonGroup radio button changes state, the previously-pressed button fires `toggled(false)` and the newly-pressed button fires `toggled(true)`. We ignore the `false` events -- only the `true` event matters.

The `match` maps the bound integer index back to the `TerrainToolMode` enum. After setting the mode, we rebuild the attributes panel via `call_deferred`. The deferred call is required because we are inside a signal callback -- Godot is still dispatching the signal, and calling `rebuild_attributes_impl()` directly would attempt to re-borrow `self` through `self.to_gd()`, which conflicts with the current mutable borrow. `call_deferred` schedules the rebuild for the next frame, when the signal dispatch is complete.

The `_rebuild_attributes_deferred()` and `_rebuild_texture_panel_deferred()` methods exist solely to bridge this borrowing gap. They obtain a `Gd<PixyTerrainPlugin>` reference (via `self.to_gd()`) and pass it to the implementation method. This reference is needed by the attribute controls' signal connections -- they need a `Gd` handle to create `Callable` objects pointing back to this plugin.

**Note on stub methods:** `do_generate()`, `do_clear()`, `rebuild_attributes_impl()`, `rebuild_texture_panel_impl()`, `apply_collision_visibility_to_all_chunks()`, and `sync_brush_size_slider()` are all called from these signal handlers but implemented further down in the file. They will be covered in Parts 16-17.

## Forward References

The following methods are called within lines 1-833 but are implemented later in the file. They will be covered in the subsequent parts:

| Method | Line Called | Implemented At | Covered In |
|--------|-----------|----------------|------------|
| `build_draw_pattern()` | 676, 739, 751 | ~line 2414 | Part 16 |
| `draw_pattern()` | 607, 692, 760 | ~line 2511 | Part 16 |
| `initialize_draw_state()` | 672 | ~line 2336 | Part 16 |
| `do_generate()` | 464 | ~line 2306 | Part 16 |
| `do_clear()` | 468 | ~line 2312 | Part 16 |
| `update_gizmos()` | 766 | ~line 2284 | Part 16 |
| `set_ui_visible()` | 428, 434, 444 | ~line 1152 | Part 16 |
| `rebuild_attributes_impl()` | 866 | ~line 1173 | Part 17 |
| `rebuild_texture_panel_impl()` | 873 | later in file | Part 17 |
| `sync_brush_size_slider()` | 725, 730 | ~line 1763 | Part 17 |
| `register_chunk_undo_redo()` | 789, 808 | ~line 3438 | Part 17 |
| `apply_collision_visibility_to_all_chunks()` | 854, 859 | later in file | Part 16 |

## Key Patterns Summary

**AfterGuiInput::STOP.ord() vs AfterGuiInput::PASS.ord():** STOP consumes the input event (editor ignores it). PASS lets the editor handle it too. Left clicks return STOP to prevent accidental object deselection. Mouse motion returns PASS to allow camera tooltips and cursor updates.

**PhysicsRayQueryParameters3D::create_ex().collision_mask(1 << 16).done():** The builder pattern for physics raycasts. `create_ex()` starts the builder, `.collision_mask()` filters to layer 17 (our terrain collision layer), `.done()` finalizes and returns `Option<Gd<...>>`.

**Input::singleton() for modifier keys:** More reliable than reading modifier state from the event, especially for mouse motion events on some platforms.

**call_deferred() for borrow safety:** When inside a Godot callback (signal handler, `enter_tree`, etc.), `self` is already mutably borrowed. Calling methods that need `self.to_gd()` would create a conflicting borrow. `call_deferred()` schedules the call for the next frame, after the current borrow is released.

**Callable::from_object_method().bindv() for signal routing:** Creates a callable that targets a specific method on a specific object, with extra arguments appended after the signal's own arguments. This is how one signal handler can serve multiple buttons -- each button binds a different index.

**The two-click height workflow state machine:** `is_setting=true, draw_height_set=false` is the paint phase (expanding the pattern). `is_setting=true, draw_height_set=true` is the height adjustment phase (previewing height). A second click applies and resets both flags.
