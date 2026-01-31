# Walkthrough: Pixy Terrain Feature Batch

**Date:** 2026-01-30
**Status:** Ready for User Implementation
**Checkpoint:** `04fa0af` (last user commit before Claude Code)
**Final State:** `8d18035` (mesh translation fix)

## Goal

Implement five interconnected features: editor plugin with toolbar buttons, upload buffer to prevent mesh holes, LOD transition seam fixes, and enclosed box geometry with walls that follow terrain contours.

## Commits Covered

```
8d18035 fix: translate mesh vertices to fill expected bounds
dc922db fix: use SDF-based walls to eliminate terrain/wall overlap
bf06cd3 feat: add box bounds with terrain-following walls
92822b7 fix: remove phantom floor plane at floor_y in box geometry
4e9217d fix: wall tops now follow terrain contour into valleys
2ad5c61 fix: transition timing and checkerboard shader for LOD seams
a4d626a fix: prevent mesh holes with upload buffer and chunk state fixes
090112d feat: add PixyTerrain editor plugin with working toolbar buttons
```

## Overview

| Order | Feature | Files |
|-------|---------|-------|
| 1 | Editor Plugin (Rust) | `rust/src/editor_plugin.rs`, `rust/src/lib.rs` |
| 2 | Upload Buffer & Map Bounds | `terrain.rs`, `chunk_manager.rs`, `mesh_worker.rs`, `lod.rs`, `noise_field.rs` |
| 3 | LOD Transition Fixes | `chunk.rs`, `chunk_manager.rs`, `mesh_extraction.rs`, `terrain.rs` |
| 4 | Box Geometry | `box_geometry.rs` (new), `lib.rs`, `noise_field.rs`, `terrain.rs` |
| 5 | SDF-Based Walls | `noise_field.rs`, `box_geometry.rs` |
| 6 | Mesh Translation Fix | `noise_field.rs`, `mesh_extraction.rs`, `terrain.rs` |

---

## Step 1: Editor Plugin (Pure Rust)

**What you'll build:** A Godot editor plugin with Generate/Clear buttons in the 3D editor's left sidebar, implemented entirely in Rust using gdext.

**Key pattern:** Use `#[class(tool, init, base=EditorPlugin)]` with `IEditorPlugin` trait. No `plugin.cfg` needed - automatic registration via gdextension.

### 1.1 Delete GDScript addon (if exists)

If you have `godot/addons/pixy_terrain_tools/`, delete the entire directory - we're replacing it with pure Rust.

### 1.2 Create `rust/src/editor_plugin.rs`

Key concepts:
- `handles()` returns true for PixyTerrain nodes
- `edit()` sets current_terrain when selected
- `forward_3d_gui_input()` handles G/C keyboard shortcuts
- `is_modifying` flag guards against Godot bug #40166 (false hide during child modification)
- `SPATIAL_EDITOR_SIDE_LEFT` places buttons in 3D viewport's left sidebar
- `MarginContainer` + `VBoxContainer` for proper padding and vertical layout

```rust
use godot::classes::editor_plugin::AfterGuiInput;
use godot::classes::editor_plugin::CustomControlContainer;
use godot::classes::{
    Button, Camera3D, EditorPlugin, IEditorPlugin, InputEvent, InputEventKey, MarginContainer,
    VBoxContainer,
};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(tool, init, base=EditorPlugin)]
pub struct PixyTerrainPlugin {
    base: Base<EditorPlugin>,
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
    #[init(val = false)]
    is_modifying: bool,
}

#[godot_api]
impl IEditorPlugin for PixyTerrainPlugin {
    fn enter_tree(&mut self) {
        godot_print!("PixyTerrainPlugin: enter_tree called");

        // Create MarginContainer for outer padding
        let mut margin_container = MarginContainer::new_alloc();
        margin_container.set_name("PixyTerrainMargin");
        margin_container.set_visible(false);
        margin_container.set_custom_minimum_size(Vector2::new(120.0, 0.0)); // Min width
        margin_container.add_theme_constant_override("margin_top", 8);
        margin_container.add_theme_constant_override("margin_left", 8);
        margin_container.add_theme_constant_override("margin_right", 8);
        margin_container.add_theme_constant_override("margin_bottom", 8);

        // Create VBoxContainer for vertical button layout
        let mut toolbar = VBoxContainer::new_alloc();
        toolbar.set_name("PixyTerrainToolbar");
        toolbar.add_theme_constant_override("separation", 8); // Space between buttons

        // Create Generate button
        let mut generate_button = Button::new_alloc();
        generate_button.set_text("Generate (G)");
        generate_button.set_custom_minimum_size(Vector2::new(100.0, 30.0));

        // Create Clear button
        let mut clear_button = Button::new_alloc();
        clear_button.set_text("Clear (C)");
        clear_button.set_custom_minimum_size(Vector2::new(100.0, 30.0));

        // Add buttons to VBoxContainer
        toolbar.add_child(&generate_button);
        toolbar.add_child(&clear_button);

        // Add VBoxContainer to MarginContainer
        margin_container.add_child(&toolbar);

        // Connect button signals
        let plugin_ref = self.to_gd();
        generate_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_generate_pressed"),
        );
        clear_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_clear_pressed"),
        );

        // Add MarginContainer to the spatial editor side left
        self.base_mut().add_control_to_container(
            CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT,
            &margin_container,
        );

        self.margin_container = Some(margin_container);
        self.toolbar = Some(toolbar);
        self.generate_button = Some(generate_button);
        self.clear_button = Some(clear_button);
        godot_print!("PixyTerrainPlugin: toolbar added to SPATIAL_EDITOR_SIDE_LEFT");
    }

    fn exit_tree(&mut self) {
        // Clean up child refs (they'll be freed with parent, but clear refs)
        self.generate_button = None;
        self.clear_button = None;
        self.toolbar = None;

        // Remove and free the margin container (and all children)
        if let Some(mut margin) = self.margin_container.take() {
            self.base_mut().remove_control_from_container(
                CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT,
                &margin,
            );
            margin.queue_free();
        }
    }

    fn handles(&self, object: Gd<Object>) -> bool {
        let class_name = object.get_class();
        godot_print!("PixyTerrainPlugin: handles called for class: {}", class_name);
        class_name == "PixyTerrain"
    }

    fn edit(&mut self, object: Option<Gd<Object>>) {
        godot_print!("PixyTerrainPlugin: edit called, object is_some: {}", object.is_some());
        if let Some(obj) = object {
            if let Ok(node) = obj.try_cast::<Node>() {
                self.current_terrain = Some(node);
                self.set_ui_visible(true);
                return;
            }
        }
        self.set_ui_visible(false)
    }

    fn make_visible(&mut self, visible: bool) {
        // Guard against false-positive hides during child modifications (bug #40166)
        if !visible && self.is_modifying {
            return;
        }

        self.set_ui_visible(visible);
        if !visible {
            self.current_terrain = None;
        }
    }

    fn forward_3d_gui_input(
        &mut self,
        _camera: Option<Gd<Camera3D>>,
        event: Option<Gd<InputEvent>>,
    ) -> i32 {
        let Some(event) = event else {
            return AfterGuiInput::PASS.ord();
        };

        if let Ok(key_event) = event.try_cast::<InputEventKey>() {
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

        AfterGuiInput::PASS.ord()
    }
}

#[godot_api]
impl PixyTerrainPlugin {
    #[func]
    fn on_generate_pressed(&mut self) {
        godot_print!("PixyTerrainPlugin: Generate button pressed");
        self.do_generate();
    }

    #[func]
    fn on_clear_pressed(&mut self) {
        godot_print!("PixyTerrainPlugin: Clear button pressed");
        self.do_clear();
    }
}

impl PixyTerrainPlugin {
    fn set_ui_visible(&mut self, visible: bool) {
        godot_print!("PixyTerrainPlugin: set_ui_visible({})", visible);
        if let Some(ref mut margin) = self.margin_container {
            margin.set_visible(visible);
        }
    }

    fn do_generate(&mut self) {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                let mut terrain_clone = terrain.clone();
                if terrain_clone.has_method("regenerate") {
                    self.is_modifying = true;
                    terrain_clone.call("regenerate", &[]);
                    self.is_modifying = false
                }
            }
        }
    }

    fn do_clear(&mut self) {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                let mut terrain_clone = terrain.clone();
                if terrain_clone.has_method("clear") {
                    self.is_modifying = true;
                    terrain_clone.call("clear", &[]);
                    self.is_modifying = false
                }
            }
        }
    }
}
```

### 1.3 Add module to `rust/src/lib.rs`

Add the module declaration:

```rust
mod editor_plugin;
```

### 1.4 Key Implementation Details

| Concept | Implementation |
|---------|----------------|
| **Container placement** | `CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT` - left sidebar of 3D editor |
| **Vertical layout** | `VBoxContainer` with `separation: 8` theme constant |
| **Outer padding** | `MarginContainer` with 8px margins on all sides |
| **Min panel width** | `set_custom_minimum_size(Vector2::new(120.0, 0.0))` |
| **Button signals** | `Callable::from_object_method(&self.to_gd(), "method_name")` |
| **Exposed callbacks** | Separate `#[godot_api] impl` block with `#[func]` methods |
| **String comparison** | `object.get_class() == "PixyTerrain"` (GString auto-converts) |

### 1.5 Build and verify

```bash
cd rust && cargo build
```

**Verify:**
1. Open Godot project (it auto-loads the extension)
2. Select a PixyTerrain node in the scene tree
3. Buttons appear in 3D editor's **left sidebar** (not menu bar)
4. Buttons have proper padding and spacing
5. Press G to generate terrain, C to clear
6. Click buttons - they trigger the same actions

---

## Step 2: Upload Buffer & Map Bounds (Rust)

**What you'll build:** FIFO mesh upload buffer, map dimension bounds, floor_y parameter, debug material shader.

**Key pattern:** VecDeque as bounded FIFO buffer with overflow handling.

### 2.1 Add exports to `terrain.rs`

Add new export groups and fields:

```rust
// Map Settings group
#[export] map_width_x: i32,    // default 10
#[export] map_height_y: i32,   // default 4
#[export] map_depth_z: i32,    // default 10

// Parallelization group
#[export] max_uploads_per_frame: i32,  // default 8
#[export] max_pending_uploads: i32,    // default 512

// Terrain Floor group
#[export] terrain_floor_y: f32,  // default 32.0

// Debug group
#[export] debug_logging: bool,   // default false
#[export] debug_material: bool,  // default false
```

### 2.2 Add internal state to `terrain.rs`

```rust
#[init(val = VecDeque::new())]
pending_uploads: VecDeque<MeshResult>,

#[init(val = None)]
cached_material: Option<Gd<ShaderMaterial>>,

#[init(val = 0)]
meshes_dropped: u64,

#[init(val = 0)]
meshes_uploaded: u64,
```

### 2.3 Implement `regenerate()` and `clear()` methods

```rust
#[godot_api]
impl PixyTerrain {
    #[func]
    pub fn regenerate(&mut self) {
        self.clear();
        self.initialize_systems();
    }

    #[func]
    pub fn clear(&mut self) {
        // 1. Stop worker pool first (prevent new results)
        if let Some(ref mut pool) = self.worker_pool {
            pool.shutdown();
        }

        // 2. Clear chunk manager's internal state
        if let Some(ref mut manager) = self.chunk_manager {
            manager.clear_all_chunks();
        }

        // 3. Free chunk mesh nodes
        let nodes: Vec<_> = self.chunk_nodes.drain().map(|(_, node)| node).collect();
        for mut node in nodes {
            if node.is_instance_valid() {
                self.base_mut().remove_child(&node);
                node.queue_free();
            }
        }

        // 4. Clear pending uploads buffer
        self.pending_uploads.clear();

        // 5. Drop systems
        self.worker_pool = None;
        self.chunk_manager = None;
        self.noise_field = None;
        self.initialized = false;
    }
}
```

### 2.4 Modify `update_terrain()` for buffered uploads

Replace direct upload with FIFO buffer:

```rust
fn update_terrain(&mut self) {
    // ... existing code to get new_meshes ...

    // Add to back of queue, mark chunks Ready
    for mesh in new_meshes {
        if self.pending_uploads.len() < self.max_pending_uploads as usize {
            if let Some(ref mut manager) = self.chunk_manager {
                manager.mark_chunk_ready(&mesh.coord);
            }
            self.pending_uploads.push_back(mesh);
        } else {
            // Buffer overflow - drop oldest, reset for re-request
            if let Some(old) = self.pending_uploads.pop_front() {
                self.meshes_dropped += 1;
                if let Some(ref mut manager) = self.chunk_manager {
                    manager.reset_chunk_for_rerequest(&old.coord);
                }
            }
            // ... add new mesh ...
        }
    }

    // Upload from front (FIFO - oldest first)
    let max_uploads = self.max_uploads_per_frame.max(1) as usize;
    for _ in 0..max_uploads {
        if let Some(mesh_result) = self.pending_uploads.pop_front() {
            self.upload_mesh_to_godot(mesh_result);
        } else {
            break;
        }
    }
}
```

### 2.5 Update `chunk_manager.rs`

Add map bounds to constructor:

```rust
pub fn new(
    lod_config: LODConfig,
    base_voxel_size: f32,
    request_tx: Sender<MeshRequest>,
    result_rx: Receiver<MeshResult>,
    debug_logging: bool,
    map_width: i32,
    map_height: i32,
    map_depth: i32,
) -> Self
```

In `compute_desired_chunks()`, filter by bounds:

```rust
// Only generate chunks within map bounds
if coord.x < 0 || coord.x >= self.map_width { continue; }
if coord.y < 0 || coord.y >= self.map_height { continue; }
if coord.z < 0 || coord.z >= self.map_depth { continue; }
```

Add helper methods:

```rust
pub fn clear_all_chunks(&mut self) { ... }
pub fn mark_chunk_ready(&mut self, coord: &ChunkCoord) { ... }
pub fn reset_chunk_for_rerequest(&mut self, coord: &ChunkCoord) { ... }
```

### 2.6 Update `mesh_worker.rs`

- Add `debug_logging` parameter
- Add `shutdown()` method with atomic flag
- Change `try_send` to `send_timeout(Duration::from_millis(100))`

### 2.7 Update `lod.rs`

Make `chunk_subdivisions` configurable:

```rust
pub fn new(base_distance: f32, max_lod: u8, chunk_subdivisions: u32) -> Self
```

### 2.8 Update `noise_field.rs`

Add `floor_y` parameter:

```rust
pub fn new(..., floor_y: f32) -> Self

pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
    let noise_value = self.fbm.get([x as f64, y as f64, z as f64]) as f32;
    (y - self.floor_y) - self.height_offset - noise_value * self.amplitude
}
```

### 2.9 Create checkerboard debug shader

In `initialize_systems()`:

```rust
if self.debug_material {
    let mut shader = Shader::new_gd();
    shader.set_code(r#"
shader_type spatial;

varying vec3 world_vertex;

uniform vec3 color_a : source_color = vec3(0.8, 0.8, 0.8);
uniform vec3 color_b : source_color = vec3(0.4, 0.4, 0.4);
uniform float scale : hint_range(0.1, 10.0) = 1.0;

void vertex() {
    world_vertex = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
    float checker = mod(floor(world_vertex.x * scale) + floor(world_vertex.y * scale) + floor(world_vertex.z * scale), 2.0);
    ALBEDO = mix(color_a, color_b, checker);
}
"#);
    let mut mat = ShaderMaterial::new_gd();
    mat.set_shader(&shader);
    self.cached_material = Some(mat);
}
```

**Verify:** `cargo build`, enable debug_material in inspector, see checkerboard pattern.

---

## Step 3: LOD Transition Fixes (Rust)

**What you'll build:** Consistent transition geometry at LOD boundaries.

**Key insight:** Use the DESIRED LOD map (target state) instead of LOADED chunks (current state) to compute transition sides. This ensures geometry matches even when neighbors haven't loaded yet.

### 3.1 Add `transition_sides` to `Chunk` struct

```rust
// In chunk.rs
pub struct Chunk {
    // ... existing fields ...
    pub transition_sides: u8,
}

impl Chunk {
    pub fn new(coord: ChunkCoord, lod_level: u8) -> Self {
        Self {
            coord,
            state: ChunkState::Pending,
            lod_level,
            mesh_instance_id: None,
            last_access_frame: 0,
            transition_sides: 0,  // NEW
        }
    }
}
```

### 3.2 Modify `ensure_chunk_requested()` signature

Pass the desired map:

```rust
fn ensure_chunk_requested(
    &mut self,
    coord: ChunkCoord,
    desired_lod: u8,
    noise_field: &Arc<NoiseField>,
    desired: &HashMap<ChunkCoord, u8>,  // NEW
) -> bool
```

### 3.3 Regenerate when transition_sides changes

```rust
let transition_sides = self.compute_transition_sides(coord, desired_lod, desired);

let needs_request = match self.chunks.get(&coord) {
    Some(chunk) => {
        // Regenerate if LOD changed OR transition sides changed
        (chunk.lod_level != desired_lod || chunk.transition_sides != transition_sides)
            && chunk.state != ChunkState::Pending
    }
    None => true,
};

// When creating/updating chunk entry, store transition_sides:
chunk.transition_sides = transition_sides;
```

### 3.4 Update `compute_transition_sides()` to use desired map

```rust
fn compute_transition_sides(
    &self,
    coord: ChunkCoord,
    lod: u8,
    desired: &HashMap<ChunkCoord, u8>,  // Use desired, not self.chunks
) -> u8 {
    if lod == 0 {
        return 0;
    }

    let mut sides: u8 = 0;
    let neighbors = [
        (ChunkCoord::new(coord.x - 1, coord.y, coord.z), 0b000001), // LowX
        (ChunkCoord::new(coord.x + 1, coord.y, coord.z), 0b000010), // HighX
        (ChunkCoord::new(coord.x, coord.y - 1, coord.z), 0b000100), // LowY
        (ChunkCoord::new(coord.x, coord.y + 1, coord.z), 0b001000), // HighY
        (ChunkCoord::new(coord.x, coord.y, coord.z - 1), 0b010000), // LowZ
        (ChunkCoord::new(coord.x, coord.y, coord.z + 1), 0b100000), // HighZ
    ];

    for (neighbor_coord, flag) in neighbors {
        // Use desired LOD - always has target state for all visible chunks
        if let Some(&neighbor_lod) = desired.get(&neighbor_coord) {
            if neighbor_lod < lod {
                sides |= flag;
            }
        }
    }
    sides
}
```

### 3.5 Fix normal fallback in `mesh_extraction.rs`

Return zero vector instead of arbitrary up for degenerate geometry:

```rust
if len > 0.0001 {
    [c[0] / len, c[1] / len, c[2] / len]
} else {
    // Zero-length normal indicates degenerate geometry
    // Return zero vector - Godot will use flat shading
    [0.0, 0.0, 0.0]
}
```

**Verify:** Fly around terrain, watch LOD transitions - seams should be invisible.

---

## Step 4: Box Geometry with Terrain-Following Walls (Rust)

**What you'll build:** Enclosed terrain with walls that follow the terrain contour, no phantom floor plane.

**Key insights:**
1. Walls are tessellated strips with top edges that binary-search for terrain height
2. No floor quad - terrain surface IS the top, walls extend to Y=0
3. SDF uses 2D noise (x,z only) so wall height search converges correctly

### 4.1 Create `box_geometry.rs`

```rust
/// Generates box geometry (walls only) with sharp 90° corners
/// Walls have tessellated top edges that follow terrain height

use crate::noise_field::NoiseField;

const WATERTIGHT_EPSILON: f32 = 0.001;

pub struct BoxMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<i32>,
}

impl BoxMesh {
    pub fn generate_with_terrain(
        min: [f32; 3],
        max: [f32; 3],
        floor_y: f32,
        noise: &NoiseField,
        segments: usize,
    ) -> Self {
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();

        let x0 = min[0];
        let z0 = min[2];
        let x1 = max[0];
        let z1 = max[2];

        // NO floor quad - walls extend to Y=0, terrain provides top surface

        // Wall -X, +X, -Z, +Z (4 walls)
        Self::add_wall_strip(&mut vertices, &mut normals, &mut indices,
            x0, z0, z1, floor_y, true, [-1.0, 0.0, 0.0], noise, segments);
        Self::add_wall_strip(&mut vertices, &mut normals, &mut indices,
            x1, z1, z0, floor_y, true, [1.0, 0.0, 0.0], noise, segments);
        Self::add_wall_strip(&mut vertices, &mut normals, &mut indices,
            z0, x1, x0, floor_y, false, [0.0, 0.0, -1.0], noise, segments);
        Self::add_wall_strip(&mut vertices, &mut normals, &mut indices,
            z1, x0, x1, floor_y, false, [0.0, 0.0, 1.0], noise, segments);

        Self { vertices, normals, indices }
    }

    fn add_wall_strip(
        vertices: &mut Vec<[f32; 3]>,
        normals: &mut Vec<[f32; 3]>,
        indices: &mut Vec<i32>,
        fixed_coord: f32,
        vary_start: f32,
        vary_end: f32,
        floor_y: f32,
        along_z: bool,
        normal: [f32; 3],
        noise: &NoiseField,
        segments: usize,
    ) {
        let segments = segments.max(1);
        let step = (vary_end - vary_start) / segments as f32;

        for i in 0..segments {
            let t0 = vary_start + step * i as f32;
            let t1 = vary_start + step * (i + 1) as f32;

            let (x0, z0, x1, z1) = if along_z {
                (fixed_coord, t0, fixed_coord, t1)
            } else {
                (t0, fixed_coord, t1, fixed_coord)
            };

            // Binary search to find terrain height
            let y0_top = Self::find_terrain_height(noise, x0, z0, floor_y) + WATERTIGHT_EPSILON;
            let y1_top = Self::find_terrain_height(noise, x1, z1, floor_y) + WATERTIGHT_EPSILON;

            // Quad from Y=0 to terrain height
            Self::add_quad(vertices, normals, indices,
                [x0, 0.0, z0], [x1, 0.0, z1], [x1, y1_top, z1], [x0, y0_top, z0], normal);
        }
    }

    fn find_terrain_height(noise: &NoiseField, x: f32, z: f32, floor_y: f32) -> f32 {
        let amplitude = noise.get_amplitude();
        let height_offset = noise.get_height_offset();

        // Search full terrain range including valleys below floor_y
        let search_min = (floor_y - amplitude * 2.0).max(0.0);
        let search_max = floor_y + height_offset.abs() + amplitude * 2.0 + 100.0;

        let mut low = search_min;
        let mut high = search_max;

        // Binary search for zero crossing (24 iterations = ~0.001 precision)
        for _ in 0..24 {
            let mid = (low + high) * 0.5;
            let sdf = noise.sample_terrain_only(x, mid, z);

            if sdf < 0.0 {
                low = mid;  // Inside terrain, search higher
            } else {
                high = mid; // Outside terrain, search lower
            }

            if (high - low) < 0.001 {
                break;
            }
        }

        (low + high) * 0.5
    }

    fn add_quad(
        vertices: &mut Vec<[f32; 3]>,
        normals: &mut Vec<[f32; 3]>,
        indices: &mut Vec<i32>,
        v0: [f32; 3], v1: [f32; 3], v2: [f32; 3], v3: [f32; 3],
        normal: [f32; 3],
    ) {
        let base = vertices.len() as i32;
        vertices.extend([v0, v1, v2, v3]);
        normals.extend([normal; 4]);
        // Two triangles with correct winding
        indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}
```

### 4.2 Add `mod box_geometry;` to `lib.rs`

### 4.3 Extend `noise_field.rs` with box bounds

```rust
pub struct NoiseField {
    fbm: Fbm<Perlin>,
    amplitude: f32,
    height_offset: f32,
    floor_y: f32,
    box_min: [f32; 3],
    box_max: [f32; 3],
    enable_box_bounds: bool,
}

impl NoiseField {
    pub fn with_box_bounds(
        seed: u32, octaves: usize, frequency: f32,
        amplitude: f32, height_offset: f32, floor_y: f32,
        box_bounds: Option<([f32; 3], [f32; 3])>,
    ) -> Self {
        // ... fbm setup ...

        let (box_min, box_max, enable_box_bounds) = match box_bounds {
            Some((min, max)) => (min, max, true),
            None => ([0.0; 3], [0.0; 3], false),
        };

        Self { fbm, amplitude, height_offset, floor_y, box_min, box_max, enable_box_bounds }
    }

    /// Sample with XZ clamping to box walls (no transvoxel wall geometry)
    pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        if !self.enable_box_bounds {
            return self.sample_terrain_only(x, y, z);
        }

        // Clamp XZ to prevent zero-crossings at walls
        let clamped_x = x.clamp(self.box_min[0], self.box_max[0]);
        let clamped_z = z.clamp(self.box_min[2], self.box_max[2]);

        self.sample_terrain_only(clamped_x, y, clamped_z)
    }

    /// Sample terrain without wall clipping - CRITICAL: uses 2D noise
    pub fn sample_terrain_only(&self, x: f32, y: f32, z: f32) -> f32 {
        // 2D noise for heightmap terrain (no Y dependency)
        let noise_value = self.fbm.get([x as f64, z as f64]) as f32;
        let surface_height = self.floor_y + self.height_offset + noise_value * self.amplitude;
        y - surface_height
    }

    pub fn get_box_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        if self.enable_box_bounds { Some((self.box_min, self.box_max)) } else { None }
    }

    pub fn get_floor_y(&self) -> f32 { self.floor_y }
    pub fn get_amplitude(&self) -> f32 { self.amplitude }
    pub fn get_height_offset(&self) -> f32 { self.height_offset }
}
```

### 4.4 Add box geometry creation to `terrain.rs`

```rust
#[export]
#[init(val = true)]
enable_box_bounds: bool,

#[init(val = None)]
box_mesh_node: Option<Gd<MeshInstance3D>>,

fn initialize_systems(&mut self) {
    // Calculate box bounds from map dimensions
    let chunk_size = self.voxel_size * self.chunk_subdivisions as f32;
    let boundary_offset = self.voxel_size;

    let box_bounds = if self.enable_box_bounds {
        Some((
            [boundary_offset, boundary_offset, boundary_offset],
            [
                self.map_width_x.max(1) as f32 * chunk_size - boundary_offset,
                self.map_height_y.max(1) as f32 * chunk_size - boundary_offset,
                self.map_depth_z.max(1) as f32 * chunk_size - boundary_offset,
            ],
        ))
    } else {
        None
    };

    let noise = NoiseField::with_box_bounds(..., box_bounds);
    let noise_arc = Arc::new(noise);
    self.noise_field = Some(Arc::clone(&noise_arc));

    // Create box geometry after noise is ready
    if let Some((box_min, box_max)) = noise_arc.get_box_bounds() {
        self.create_box_geometry(box_min, box_max, &noise_arc);
    }

    // ... rest of initialization ...
}

fn create_box_geometry(&mut self, box_min: [f32; 3], box_max: [f32; 3], noise: &NoiseField) {
    let floor_y = noise.get_floor_y();
    let wall_segments = (self.chunk_subdivisions as usize
        * self.map_width_x.max(self.map_depth_z).max(1) as usize).max(8);

    let box_mesh = BoxMesh::generate_with_terrain(box_min, box_max, floor_y, noise, wall_segments);

    if box_mesh.is_empty() {
        return;
    }

    // Convert to Godot ArrayMesh and add as MeshInstance3D child
    // Apply cached_material if debug_material is enabled
}

fn clear(&mut self) {
    // ... existing clear code ...

    // Remove box geometry node
    if let Some(mut box_node) = self.box_mesh_node.take() {
        if box_node.is_instance_valid() {
            self.base_mut().remove_child(&box_node);
            box_node.queue_free();
        }
    }
}
```

**Verify:**
1. `cargo build`
2. Open Godot, enable `enable_box_bounds`, regenerate
3. Fly into a valley, look up - no phantom horizontal plane
4. Look at walls - they should follow the terrain contour smoothly

---

## Step 5: SDF-Based Walls (Rust)

**What you'll build:** Walls created via SDF zero-crossing rather than separate geometry, eliminating vertex misalignment between walls and terrain.

**Key insight:** Instead of generating wall geometry separately and trying to match terrain vertices, return "air" (positive SDF) outside the XZ bounds. The transvoxel algorithm naturally creates walls at the boundary where inside transitions to outside.

### 5.1 Modify `noise_field.rs` to return air outside bounds

Change `sample()` to return a positive value (outside surface) when coordinates are outside XZ bounds:

```rust
pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
    if self.enable_box_bounds {
        // Return "air" outside XZ bounds - creates wall via SDF zero-crossing
        if x < self.box_min[0] || x > self.box_max[0] ||
           z < self.box_min[2] || z > self.box_max[2] {
            return 1.0;  // Outside = air
        }
    }

    self.sample_terrain_only(x, y, z)
}
```

### 5.2 Simplify `box_geometry.rs` to short skirt only

With SDF-based walls handling the terrain junction, box geometry only needs a short skirt at the base for sharp corners:

```rust
// Generate a short 2-unit skirt at the base for sharp corners
// The SDF-based walls handle the terrain junction above
let skirt_height = 2.0;

fn add_wall_strip(...) {
    // Simple constant-height quad strip at base
    let y_bottom = 0.0;
    let y_top = skirt_height;

    // No binary search needed - just flat quads
    Self::add_quad(vertices, normals, indices,
        [x0, y_bottom, z0], [x1, y_bottom, z1],
        [x1, y_top, z1], [x0, y_top, z0], normal);
}
```

**Verify:**
1. `cargo build`
2. Fly along terrain/wall boundary - no slivers or gaps where they meet
3. Wall geometry seamlessly integrates with terrain surface

---

## Step 6: Mesh Translation Fix (Rust)

**What you'll build:** Translate mesh vertices so terrain fills expected bounds despite internal SDF offset.

**Problem:** The boundary_offset moves the SDF boundary inward (by `max_voxel_size`) for LOD alignment. This causes terrain mesh to start at X=4, Z=4 instead of near origin.

**Solution:** Translate vertices by `-boundary_offset` in X and Z after mesh generation.

### 6.1 Store boundary_offset in `NoiseField`

```rust
// noise_field.rs
pub struct NoiseField {
    // ... existing fields ...
    boundary_offset: f32,
}

impl NoiseField {
    pub fn with_box_bounds(
        ...,
        boundary_offset: f32,  // NEW parameter
    ) -> Self {
        // Store in struct
    }

    pub fn get_boundary_offset(&self) -> f32 {
        self.boundary_offset
    }
}
```

### 6.2 Translate vertices in `mesh_extraction.rs`

```rust
// After mesh.positions is populated, translate X and Z
let boundary_offset = noise.get_boundary_offset();
let vertices: Vec<[f32; 3]> = mesh
    .positions
    .chunks(3)
    .map(|c| [
        c[0] - boundary_offset,  // Translate X
        c[1],                     // Keep Y unchanged
        c[2] - boundary_offset,  // Translate Z
    ])
    .collect();
```

### 6.3 Calculate boundary_offset from max LOD voxel size in `terrain.rs`

```rust
fn initialize_systems(&mut self) {
    // Align boundary with largest LOD voxel size so all LOD levels agree
    let max_voxel_size = self.voxel_size * (1 << self.max_lod_level.max(0)) as f32;
    let boundary_offset = max_voxel_size;

    // Pass to NoiseField
    let noise = NoiseField::with_box_bounds(..., boundary_offset);

    // Translate box geometry bounds to match
    if let Some((box_min, box_max)) = noise_arc.get_box_bounds() {
        let translated_min = [
            box_min[0] - boundary_offset,
            box_min[1],
            box_min[2] - boundary_offset,
        ];
        let translated_max = [
            box_max[0] - boundary_offset,
            box_max[1],
            box_max[2] - boundary_offset,
        ];
        self.create_box_geometry(translated_min, translated_max, &noise_arc);
    }
}
```

**Verify:**
1. `cargo build`
2. Regenerate terrain
3. Terrain mesh starts near origin, filling expected bounds

---

## Known Dragons

1. **Godot bug #40166**: Plugin's `_make_visible(false)` called incorrectly when adding/removing chunk children. Mitigation: `_is_modifying` state guard.

2. **Shader world position**: Use `MODEL_MATRIX` in vertex shader, not `INV_VIEW_MATRIX` in fragment shader. The latter gives view-space coordinates.

3. **2D vs 3D noise for walls**: `sample_terrain_only()` MUST use 2D noise `[x, z]` not 3D `[x, y, z]`. Otherwise binary search doesn't converge because terrain height at (x,z) varies with Y sample position.

4. **Buffer overflow handling**: When upload buffer is full, drop OLDEST (front) not newest, and reset chunk for re-request. Otherwise chunks stay in Ready state forever.

---

## Verification Checklist

- [ ] Plugin: Select PixyTerrain, toolbar appears with Generate/Clear buttons
- [ ] Plugin: Press G key to generate, C key to clear
- [ ] Upload buffer: No mesh holes when flying around quickly
- [ ] LOD transitions: Seams invisible at LOD boundaries
- [ ] Box geometry: Walls follow terrain contour
- [ ] Box geometry: No phantom floor plane in valleys
- [ ] Debug material: Checkerboard shader works correctly

---

## To Reset and Implement Yourself

```bash
# Reset to checkpoint
git reset --hard 04fa0af

# Then follow this walkthrough step by step
```

---

## Session Log

- 2026-01-30: Walkthrough created from commits 090112d..bf06cd3
- 2026-01-31: Revised Step 1 to use Pure Rust EditorPlugin instead of GDScript (no plugin.cfg needed)
- 2026-01-31: **Proven implementation** - Step 1 updated with working code:
  - Changed from `SPATIAL_EDITOR_MENU` (toolbar) to `SPATIAL_EDITOR_SIDE_LEFT` (left sidebar)
  - Added `MarginContainer` for 8px outer padding
  - Added `VBoxContainer` for vertical button layout with 8px separation
  - Added `set_custom_minimum_size(120, 0)` for min panel width
  - Simplified `forward_3d_gui_input` signature (Option params, returns i32)
  - Checkpoint commit: b74cf28
