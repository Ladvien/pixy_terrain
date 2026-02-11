# Editor Plugin

**Status:** Implemented
**Date:** 2026-02-11
**Files:** `rust/src/editor_plugin.rs`, `rust/src/gizmo.rs`, `godot/addons/pixy_terrain/resources/shaders/round_brush_radius_visual.gdshader`, `godot/addons/pixy_terrain/resources/shaders/square_brush_radius_visual.gdshader`

## Summary

Godot editor integration providing 9 tool modes for terrain editing with configurable round/square brushes, undo/redo batching via composite patterns, multiple raycast strategies, and real-time gizmo visualization.

## What It Does

- Activates when a PixyTerrain node is selected in the editor
- Provides a left toolbar with tool mode buttons, a bottom attribute panel with mode-specific controls, and a right texture settings panel
- Handles mouse/keyboard input for terrain editing operations
- Batches all modifications into composite patterns for single-action undo/redo
- Draws real-time brush circle/square and draw pattern preview via gizmo plugin
- Manages chunk grid visualization for add/remove operations

## Scope

**Covers:** All 9 tool modes, brush configuration, pattern accumulation, undo/redo integration, raycast strategies, gizmo rendering, UI layout, keyboard shortcuts.

**Does not cover:** Marching squares algorithm, shader rendering, grass placement logic, texture preset persistence.

## Interface

### Tool Modes

| Mode | Index | Description |
|------|-------|-------------|
| Height | 0 | Raise/lower terrain with two-click workflow |
| Level | 1 | Set terrain to specific height |
| Smooth | 2 | Average surrounding cell heights |
| Bridge | 3 | Create interpolated slope between two points |
| GrassMask | 4 | Add/remove grass (toggle on re-click) |
| VertexPaint | 5 | Paint ground or wall vertex colors (15 material slots) |
| DebugBrush | 6 | Print cell data to console |
| ChunkManagement | 7 | Add/remove terrain chunks |
| TerrainSettings | 8 | Global terrain parameter overlay |

### Brush Types

| Type | Falloff Curve |
|------|--------------|
| Round | `smoothstep(1 - dist_sq / max_distance)` -- smooth radial falloff |
| Square | `1 - clamp(max(abs(x), abs(z)) / half_size, 0.2, 1.0)` -- min 0.2 at edges |

### Brush Configuration

| Property | Range | Default | Purpose |
|----------|-------|---------|---------|
| Size | 1.0-50.0 | 15.0 | Brush radius in world units |
| Strength | 0.1-10.0 | 1.0 | Blend amount for Smooth mode |
| Falloff | bool | true | Enable distance-based falloff |
| Flatten | bool | true | Paint to absolute height (Height mode) |
| Ease | -5.0 to 5.0 | -1.0 | Bridge curve (-1.0 = no easing) |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| G | Generate terrain (regenerate) |
| C | Clear terrain |
| Shift+Scroll | Adjust brush size (step 0.5) |
| Ctrl+Click | Sample height from terrain (Level mode only) |
| Alt | Clear current pattern accumulation [INFERRED] |

### GDScript API (#[func] methods)

**Signal/Button Handlers:**
- `on_generate_pressed()`, `on_clear_pressed()`
- `on_tool_button_toggled(pressed, tool_index)`
- `on_grass_mask_button_down()` -- toggles add/remove mode
- `on_settings_toggled(pressed)`
- `on_attribute_changed(value, setting_name)` -- slider/dropdown changes
- `on_texture_resource_changed(resource, setting_name)` -- texture picker changes
- `on_collision_toggle_changed(pressed)`

**Deferred Operations:**
- `_rebuild_attributes_deferred()` -- safe rebuild of bottom panel
- `_rebuild_texture_panel_deferred()` -- safe rebuild of texture panel
- `apply_collision_visibility_deferred()` -- toggle collision visibility on all chunks

**Godot Lifecycle:**
- `enter_tree()`, `exit_tree()`, `handles(object)`, `edit(object)`, `make_visible(visible)`
- `forward_3d_gui_input(camera, event) -> i32` -- main input handler

## Behavior Details

### Pattern Accumulation System

**Draw Pattern Structure:**
```
HashMap<[i32; 2], HashMap<[i32; 2], f32>>
         ^chunk_key        ^cell_key   ^falloff_sample
```

**Phase 1: Mouse Motion** (`build_draw_pattern`)
- Iterates all chunks and cells within brush radius
- Computes falloff sample per cell based on brush type
- Accumulates maximum sample value per cell (overlapping strokes don't stack)
- Handles cross-chunk boundaries

**Phase 2: Mouse Release** (`draw_pattern`)
- Snapshots pattern to avoid borrow conflicts
- Computes do/undo dictionaries per layer:
  - Height: `old_height + (target - old_height) * sample` (or absolute for flatten)
  - Color: stores old and new values for each affected cell
  - Grass mask: Color(1,0,0,0) for add, Color(0,0,0,0) for remove

**Phase 3: Cross-Chunk Edge Propagation**
- Propagates values across chunk boundaries (shared edge vertices)
- Full blend (1.0) for edge cells
- 0.5 blend for inner-adjacent cells in height modes

**Phase 4: Wall Color Expansion**
- For height modes without QuickPaint: expands wall colors to all adjacent cells
- Applies default wall texture to prevent uncolored wall faces

### apply_composite_pattern (Undo/Redo)

Registered via `EditorUndoRedoManager` as a single action:

```
{
  "height": {chunk_coords: {cell_coords: height_value}},
  "color_0": {chunk_coords: {cell_coords: color}},
  "color_1": {chunk_coords: {cell_coords: color}},
  "wall_color_0": {chunk_coords: {cell_coords: color}},
  "wall_color_1": {chunk_coords: {cell_coords: color}},
  "grass_mask": {chunk_coords: {cell_coords: mask_color}}
}
```

Action names: "terrain height", "terrain level", "terrain smooth", "terrain slope", "terrain grass mask", "terrain vertex paint", "terrain wall paint".

### Raycast Strategies

| # | Condition | Method | Used By |
|---|-----------|--------|---------|
| 1 | Setting mode + draw_height_set | Vertical plane through base_position, normal from camera XZ | Height mode second-click |
| 2 | Pattern not empty + flatten | Horizontal plane at draw_height | Height mode with flatten |
| 3 | Drawing + Level mode | Horizontal plane at target height | Level mode |
| 4 | Default | PhysicsRayQueryParameters3D, mask `1<<16`, 10,000 unit ray | All other modes |

### Mode-Specific Behaviors

**Height:** Two-click workflow. First click captures base position and height. Drag adjusts height via vertical plane raycast. Supports flatten (absolute height) and falloff.

**Level:** Click-drag to paint cells to target height. Ctrl+Click samples height from terrain.

**Smooth:** Computes global average of affected cells, blends toward it using strength parameter.

**Bridge/Slope:** First click sets start, second sets end. Interpolates heights between points with optional easing curve (`godot_ease()` function).

**GrassMask:** Toggle button re-click switches between add/remove. Button text updates accordingly.

**VertexPaint:** 15 material slots (0-14 ground, 15 is wall). "Paint Walls" checkbox switches between wall and ground vertex color painting. Uses default_wall_texture for wall defaults.

**DebugBrush:** Prints chunk coords, cell coords, height, color_0, color_1 to console.

**ChunkManagement:** Click empty area adjacent to existing chunks to add. Click existing chunk to remove. Dropdown selects chunk for per-chunk merge mode editing.

**TerrainSettings:** Overlay mode that displays all terrain parameters on the bottom panel. Stays active alongside other tool modes.

### Gizmo Visualization

**GizmoState** (snapshot from plugin to gizmo):
- mode, brush_type, brush_position, brush_size, terrain_hovered
- flatten, draw_height, draw_pattern, is_setting, is_drawing

**Materials:**
- brush: white (RGBA 1,1,1,0.7) unshaded lines
- brush_pattern: gray (RGBA 0.7,0.7,0.7,0.6) unshaded lines
- addchunk: green (RGBA 0,1,0,0.5) for addable chunks
- removechunk: red (RGBA 1,0,0,0.5) for existing chunks

**Visualizations:**
- **Brush circle/square**: 32-segment circle (round) or 4-side square with 8 subdivisions. Samples terrain height at each point, offset 0.3 units above surface.
- **Draw pattern preview**: Small squares per affected cell showing preview heights. Square size proportional to falloff sample. Offset 0.2 above preview height.
- **Chunk management grid**: Red X for existing chunks (remove), Green + for adjacent slots (add).

### UI Layout

**Left Toolbar** (SPATIAL_EDITOR_SIDE_LEFT, 140px min width):
- Generation: "Generate (G)", "Clear (C)" buttons
- Tools: ButtonGroup with 8 tool mode buttons
- Section labels: "Visuals", "Utility", "Management", "Debug", "Settings"
- Debug: "Show Colliders" checkbox
- Settings: "Settings" toggle button

**Bottom Panel** (SPATIAL_EDITOR_BOTTOM, 48px height):
- Dynamic controls based on active tool mode
- All modes with brush: Brush Type dropdown + Size slider
- Mode-specific: Height/Level/Flatten checkboxes, Strength slider, Ease slider, Material dropdown, Paint Walls checkbox
- QuickPaint dropdown on Height/Level/Smooth/Bridge modes
- TerrainSettings mode shows comprehensive parameter grid

**Right Panel** (SPATIAL_EDITOR_SIDE_RIGHT, 220px min width):
- 15 texture slots with: EditorResourcePicker, Scale slider (0.1-40.0)
- Slots 1-6 additionally: Ground Color picker, Grass Sprite picker
- Slots 2-6: Has Grass checkbox

### QuickPaint Integration

- Dropdown lists loaded PixyQuickPaint presets
- When active during height/painting operations:
  - Ground colors applied from `get_ground_colors()`
  - Wall colors applied from `get_wall_colors()`
  - Grass toggle from `has_grass`
- Integrates with all pattern-based modes

## Acceptance Criteria

- All 9 tool modes function with appropriate UI controls
- Undo/redo works across multi-chunk editing sessions
- Brush preview matches actual edit area
- Cross-chunk edits produce seamless results
- Keyboard shortcuts (G, C, Shift+Scroll) work correctly

## Technical Notes

- Plugin connects to Godot button/slider signals via `Callable::from_object_method` with `bindv`
- No custom signals defined -- uses Godot built-in signal connections
- `forward_3d_gui_input` returns `AfterGuiInput::STOP` (1) to consume events or `AfterGuiInput::PASS` (0)
- Gizmo uses `DepthDrawMode::DISABLED`, `ShadingMode::UNSHADED`, `Transparency::ALPHA` materials
- `_rebuild_attributes_deferred()` prevents borrow conflicts during UI rebuilds
- editor_plugin.rs is ~3,757 lines; gizmo.rs is ~393 lines
