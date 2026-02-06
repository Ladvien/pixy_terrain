# Part 18 — Test Scene & Final Integration

**Series:** Reconstructing Pixy Terrain
**Part:** 18 of 18 (Final)
**Previous:** Part 17 — Review Fixes & Parity Polish
**Status:** Complete

## What We're Building

The capstone. Every module is written, every shader is in place, every editor tool responds to clicks. Now we wire it all together: the Rust extension entry point that registers every class with Godot, the Cargo build configuration that produces the right shared library, the GDExtension manifest that tells Godot where to find it, the project configuration, and finally a test scene where you can open the editor and sculpt terrain.

This part is less about writing new code and more about understanding the connective tissue that makes eight Rust modules, four shaders, a GDScript camera controller, and a Godot project all talk to each other.

## What You'll Have After This

A complete, working Pixy Terrain project. You run `cargo build` from the `rust/` directory, open Godot, and see a 3D pixel-art terrain editor with sculpting, painting, grass placement, and undo/redo. The full reconstruction is done.

---

## 1. lib.rs — The Extension Entry Point

**File:** `rust/src/lib.rs`

```rust
use godot::prelude::*;

mod chunk;
mod editor_plugin;
mod gizmo;
mod grass_planter;
mod marching_squares;
mod quick_paint;
mod terrain;
mod texture_preset;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
```

15 lines. That is the entire file. It is the most important 15 lines in the project.

### Why it looks like this

**The `mod` declarations are the module registry.** Every Rust file we wrote across Parts 2 through 17 becomes a module here. The order is alphabetical by convention, but it does not matter to the compiler. What matters is that every file is listed. If you forget one, every `#[derive(GodotClass)]` struct inside that file silently disappears from Godot. No compiler error. No runtime warning. Just a missing class in the editor. This has bitten every gdext developer at least once.

Here are the eight modules and what they register with Godot:

| Module | Godot Class(es) | Base Type |
|---|---|---|
| `marching_squares` | (no class — pure Rust logic) | N/A |
| `chunk` | `PixyTerrainChunk` | `MeshInstance3D` |
| `terrain` | `PixyTerrain` | `Node3D` |
| `grass_planter` | `PixyGrassPlanter` | `MultiMeshInstance3D` |
| `editor_plugin` | `PixyTerrainPlugin` | `EditorPlugin` |
| `gizmo` | `PixyTerrainGizmoPlugin` | `EditorNode3DGizmoPlugin` |
| `quick_paint` | `PixyQuickPaint` | `Resource` |
| `texture_preset` | `PixyTexturePreset` | `Resource` |

**The `#[gdextension]` macro** generates the C ABI entry point that Godot calls when loading the shared library. The function name it generates is `gdext_rust_init` — you will see this exact string referenced in the `.gdextension` file. The `unsafe impl ExtensionLibrary` trait tells gdext to scan all registered modules for `#[derive(GodotClass)]` structs and register them with Godot's ClassDB.

**Why `marching_squares` has no Godot class:** It is a pure algorithm module. It exposes `generate_cell()`, `CellContext`, `CellGeometry`, and the helper functions that `PixyTerrainChunk` calls during mesh generation. Keeping the geometry math separate from the Godot node means we can unit-test marching squares logic with `cargo test` without needing a running Godot engine.

**Why `struct PixyTerrainExtension` is empty:** It is a marker type. The `#[gdextension]` macro needs a type to hang the trait implementation on, but the type itself carries no data. Think of it as a namespace anchor.

---

## 2. Cargo.toml — Build Configuration

**File:** `rust/Cargo.toml`

```toml
[package]
name = "pixy_terrain"
version = "0.1.0"
edition = "2021"
rust-version = "1.78"

[lib]
crate-type = ["cdylib"]

[dependencies]
godot = { git = "https://github.com/godot-rust/gdext", branch = "master" }

[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3
```

### Why each section matters

**`crate-type = ["cdylib"]`** — This is the critical line. Without it, Cargo produces a `.rlib` (Rust library for linking into other Rust code). With `cdylib`, it produces a platform-specific shared library: `.dylib` on macOS, `.so` on Linux, `.dll` on Windows. Godot loads shared libraries at runtime via its GDExtension system, so we must produce one.

**`godot = { git = "...", branch = "master" }`** — We track the master branch of godot-rust/gdext. This gives us access to the latest Godot 4.6 API bindings. For production projects you would pin to a specific commit or tag, but during active development against Godot 4.6, tracking master is practical. The `godot` crate is the single dependency — gdext provides everything: node types, math types, `Variant`, `Dictionary`, `PackedArray`, signal infrastructure, editor plugin support.

**`rust-version = "1.78"`** — Minimum supported Rust version. gdext uses features stabilized in 1.78 (notably some const generic improvements). If someone clones this repo with an older toolchain, Cargo will tell them to upgrade rather than producing cryptic compile errors.

**`[profile.dev] opt-level = 1`** — Debug builds with zero optimization are painfully slow for terrain generation. Even a single chunk calls `generate_cell()` over 1,000 times, each invoking trigonometry and vertex math. `opt-level = 1` gives us basic optimizations (inlining, dead code removal) without losing debug symbols or significantly slowing compilation.

**`[profile.dev.package."*"] opt-level = 3`** — Dependencies (the `godot` crate and its transitive dependencies) get full optimization even in debug mode. The gdext binding layer involves a lot of FFI wrapper code that benefits enormously from optimization. This gives us near-release performance for Godot API calls while keeping our own code debuggable.

---

## 3. .cargo/config.toml — macOS Linker Flags

**File:** `rust/.cargo/config.toml`

```toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-Wl,-install_name,@rpath/libpixy_terrain.dylib"]

[target.x86_64-apple-darwin]
rustflags = ["-C", "link-arg=-Wl,-install_name,@rpath/libpixy_terrain.dylib"]
```

### Why this exists (macOS only)

macOS dynamic libraries embed an "install name" — a path that the dynamic linker uses to find the library at runtime. By default, Rust sets this to the absolute build path (something like `/Users/you/pixy_terrain/rust/target/debug/libpixy_terrain.dylib`). That path is meaningless on anyone else's machine and breaks when you move the project.

The `@rpath` prefix tells macOS: "look for this library relative to the executable's runtime search path." Godot sets up its rpath to include the directory where the `.gdextension` file points, so everything resolves correctly.

We define this for both `aarch64-apple-darwin` (Apple Silicon) and `x86_64-apple-darwin` (Intel Macs). If you are on Linux or Windows, this file is harmless — Cargo ignores target sections that do not match the current build target.

**Without this file**, Godot on macOS will either fail to load the library or produce a warning about absolute paths. It is one of those "invisible" configuration requirements that the gdext documentation mentions but is easy to miss.

---

## 4. pixy_terrain.gdextension — The GDExtension Manifest

**File:** `godot/pixy_terrain.gdextension`

```ini
[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = 4.1
reloadable = true

[libraries]
linux.debug.x86_64 = "res://../rust/target/debug/libpixy_terrain.so"
linux.release.x86_64 = "res://../rust/target/release/libpixy_terrain.so"
windows.debug.x86_64 = "res://../rust/target/debug/pixy_terrain.dll"
windows.release.x86_64 = "res://../rust/target/release/pixy_terrain.dll"
macos.debug = "res://../rust/target/debug/libpixy_terrain.dylib"
macos.release = "res://../rust/target/release/libpixy_terrain.dylib"
macos.debug.arm64 = "res://../rust/target/debug/libpixy_terrain.dylib"
macos.release.arm64 = "res://../rust/target/release/libpixy_terrain.dylib"
macos.debug.x86_64 = "res://../rust/target/debug/libpixy_terrain.dylib"
macos.release.x86_64 = "res://../rust/target/release/libpixy_terrain.dylib"
```

### Anatomy of the manifest

**`entry_symbol = "gdext_rust_init"`** — This is the C function name that Godot calls to initialize the extension. It must exactly match what the `#[gdextension]` macro generates. If you renamed your extension struct, the macro would generate a different symbol name and Godot would fail to load the library with a "symbol not found" error. The convention is `gdext_rust_init` and there is no reason to change it.

**`compatibility_minimum = 4.1`** — The oldest Godot version this extension claims to support. In practice we target 4.6, but gdext's ABI is backward-compatible to 4.1. Setting this lower lets the extension load in slightly older Godot versions if someone needs that flexibility.

**`reloadable = true`** — This enables Godot 4.2+ hot reloading. When you rebuild the Rust library while Godot is running, Godot detects the change and reloads the extension without restarting the editor. This is transformative for iteration speed — you edit Rust code, run `cargo build`, and see changes in the editor within seconds. Without this flag, you must close and reopen Godot after every build.

**Library paths use `res://../rust/target/`** — The `res://` prefix means "relative to the Godot project root" (the directory containing `project.godot`). The `../` steps up into the repository root, then down into `rust/target/`. This path structure reflects our project layout where `godot/` and `rust/` are siblings.

**macOS has six entries** — Three for debug (generic, arm64, x86_64) and three for release. Godot tries the most specific match first (e.g., `macos.debug.arm64` on Apple Silicon), then falls back to the generic `macos.debug`. We point them all at the same library because Cargo produces a single `.dylib` for the host architecture. If you were cross-compiling, you would use different paths.

**Windows omits the `lib` prefix** — On Windows, the library is `pixy_terrain.dll`, not `libpixy_terrain.dll`. This matches Cargo's default naming convention on each platform.

---

## 5. project.godot — Godot Project Configuration

**File:** `godot/project.godot`

```ini
; Engine configuration file.
; It's best edited using the editor UI and not directly,
; since the parameters that go here are not all obvious.
;
; Format:
;   [section] ; section goes between []
;   param=value ; assign values to parameters

config_version=5

[animation]

compatibility/default_parent_skeleton_in_mesh_instance_3d=true

[application]

config/name="Pixy Terrain"
run/main_scene="res://scenes/test_scene.tscn"
config/features=PackedStringArray("4.6", "Forward Plus")
config/icon="res://icon.svg"

[rendering]

textures/canvas_textures/default_texture_filter=0
```

### Key settings explained

**`config_version=5`** — Godot 4.x project format version. This distinguishes it from Godot 3.x projects (which use `config_version=4`). Godot will refuse to open the project if this does not match.

**`config/features=PackedStringArray("4.6", "Forward Plus")`** — Declares that this project uses Godot 4.6 features and the Forward+ rendering backend. Forward+ is the full-featured desktop renderer with real-time GI, volumetric fog, and advanced lighting — the right choice for a 3D terrain editor. The alternatives (Mobile and Compatibility) trade visual quality for performance on lower-end hardware.

**`run/main_scene="res://scenes/test_scene.tscn"`** — When you press F5 (Play), Godot loads this scene. It is our test scene with the terrain, camera, and lights.

**`textures/canvas_textures/default_texture_filter=0`** — Sets the default 2D texture filter to "Nearest" (pixel-perfect, no interpolation). This is a pixel-art project. We want crisp, blocky textures, not blurry bilinear filtering. This setting affects canvas/UI textures; 3D textures use the shader's own sampler settings.

**No `[editor_plugins]` section** — You might expect to see our `PixyTerrainPlugin` registered here. It is not. GDExtension editor plugins are registered differently from GDScript plugins. When Godot loads our `.gdextension` library, it discovers the `PixyTerrainPlugin` class (which extends `EditorPlugin`) and automatically activates it. GDScript-based plugins require explicit registration in `project.godot`; GDExtension plugins do not.

---

## 6. test_scene.tscn — The Test Scene

**File:** `godot/scenes/test_scene.tscn` (711 lines, ~2.6 MB due to inline mesh/collision data)

The scene file is large because Godot serializes the chunk mesh data, collision shapes, height maps, and color maps inline. The structural skeleton, however, is straightforward.

### Scene hierarchy

```
TestScene (Node3D)                          -- Scene root
  WorldEnvironment                          -- Sky and ambient lighting
  DirectionalLight3D                        -- Sun
  Camera3D                                  -- Orbit camera with GDScript
  PixyTerrain                               -- Our custom terrain manager
    Chunk (0, 0) (PixyTerrainChunk)         -- Center chunk
      GrassPlanter (PixyGrassPlanter)       -- Grass multimesh
      Chunk (0, 0)_col (StaticBody3D)       -- Collision body
        CollisionShape3D                    -- ConcavePolygonShape3D
      Chunk (0, 0)_col2 (StaticBody3D)      -- Additional collision
        CollisionShape3D
      ... (up to _col5 for complex chunks)
    Chunk (0, -1) (PixyTerrainChunk)        -- Neighboring chunks
    Chunk (1, 0)  (PixyTerrainChunk)
    Chunk (1, -1) (PixyTerrainChunk)
    Chunk (-1, -1)(PixyTerrainChunk)
    Chunk (-1, 0) (PixyTerrainChunk)
    Chunk (-1, 1) (PixyTerrainChunk)
    Chunk (1, 1)  (PixyTerrainChunk)
    Chunk (0, 1)  (PixyTerrainChunk)
```

The test scene contains a 3x3 grid of chunks (9 total, from (-1,-1) to (1,1)) with sculpted terrain data already baked in.

### External resources

```
[ext_resource type="Script"    path="res://scripts/orbit_camera.gd"               id="1_orbit"]
[ext_resource type="Shader"    path="res://resources/shaders/mst_terrain.gdshader" id="2_3qnke"]
[ext_resource type="Texture2D" path="res://resources/textures/default_ground_noise.tres" id="3_wtsjf"]
[ext_resource type="Shader"    path="res://resources/shaders/mst_grass.gdshader"   id="4_rnaij"]
[ext_resource type="Texture2D" path="res://resources/textures/grass_leaf_sprite.png" id="5_h3xc6"]
[ext_resource type="Texture2D" path="res://resources/textures/wind_noise_texture.tres" id="6_s36qc"]
```

Six external resources: the orbit camera script, two shaders (terrain and grass), and three textures (ground noise, grass sprite, wind noise).

### Sub-resources: environment

```
[sub_resource type="ProceduralSkyMaterial" id="ProceduralSkyMaterial_sky"]
sky_top_color = Color(0.4, 0.6, 0.9, 1)
sky_horizon_color = Color(0.7, 0.8, 0.9, 1)
ground_bottom_color = Color(0.2, 0.2, 0.2, 1)
ground_horizon_color = Color(0.5, 0.5, 0.5, 1)

[sub_resource type="Sky" id="Sky_main"]
sky_material = SubResource("ProceduralSkyMaterial_sky")

[sub_resource type="Environment" id="Environment_env"]
background_mode = 2
sky = SubResource("Sky_main")
ambient_light_source = 2
ambient_light_color = Color(0.5, 0.5, 0.5, 1)
```

A procedural sky with soft blue tones and neutral ambient light. `background_mode = 2` means "Sky" (as opposed to solid color or custom). `ambient_light_source = 2` means "use the sky as the ambient light source," which gives gentle fill lighting from all directions.

### Key node configurations

**DirectionalLight3D:**
```
transform = Transform3D(0.866, -0.250, 0.433, 0.465, 0.086, -0.881, 0.183, 0.964, 0.191, 0, 112.3, 0)
```
The transform encodes a rotation of roughly 30 degrees from vertical and a Y position of 112 units. The rotation angles create a sunlight direction that casts shadows at roughly 60 degrees — good for revealing terrain contours.

**Camera3D:**
```
transform = Transform3D(1, 0, 0, 0, 0.866, 0.5, 0, -0.5, 0.866, 0, 15, 25)
current = true
fov = 60.0
script = ExtResource("1_orbit")
```
Initial position is at (0, 15, 25) looking down at roughly 30 degrees. The orbit camera script (see next section) takes over from here. `current = true` means this is the active camera at scene start.

**PixyTerrain:**
```
[node name="PixyTerrain" type="PixyTerrain" parent="."]
```
No explicit properties — all defaults. The terrain manager discovers its child chunks on `_ready()` and rebuilds its internal `HashMap<[i32;2], Gd<PixyTerrainChunk>>`. The shader materials, ground colors, and blend settings are all configured through the editor inspector and serialized into the chunk sub-resources.

**PixyTerrainChunk (example: Chunk (0, 0)):**
```
saved_height_map = PackedFloat32Array(0, 0, 0, ..., 2.627, 7.183, 8.310, ...)
saved_ground_color_0 = PackedColorArray(...)
saved_wall_color_0 = PackedColorArray(...)
saved_grass_mask = PackedFloat32Array(...)
```
Each chunk serializes its full data arrays as `PackedFloat32Array` and `PackedColorArray` exports. These are the `saved_*` fields from Part 6. On `_ready()`, each chunk calls `restore_from_packed()` to hydrate its runtime `Vec<f32>` and `Vec<Color>` data, then `regenerate()` to rebuild the mesh.

### Why the scene file is so large

The 711-line, 2.6 MB file is mostly data, not structure. A single chunk's `saved_height_map` contains 33 x 33 = 1,089 floats. Multiply by 9 chunks, add color maps (4 color channels per entry), collision shape vertex arrays, and the inline `ArrayMesh` surface data, and you reach megabytes quickly. This is normal for terrain scenes. In a production project, you might serialize chunk data to separate `.tres` files to keep the scene file manageable, but for a test scene this inline approach works fine.

### Creating the scene from scratch

If you are starting fresh rather than using the saved scene, here is the minimal scene you need:

1. Create a new 3D scene (Node3D root, name it "TestScene")
2. Add a `WorldEnvironment` node with a procedural sky
3. Add a `DirectionalLight3D`, rotate it to taste
4. Add a `Camera3D`, attach the orbit camera script, set `current = true`
5. Add a `PixyTerrain` node (it appears in the "Create Node" dialog because our extension registers it)
6. Select the terrain, use the editor tools to add chunks and sculpt

The chunks, collision shapes, and grass planters are all created programmatically by the terrain system — you never need to add them manually.

---

## 7. orbit_camera.gd — Camera Controller

**File:** `godot/scripts/orbit_camera.gd`

```gdscript
extends Camera3D
## Simple orbit camera for testing - use mouse to rotate, scroll to zoom

@export var target: Vector3 = Vector3.ZERO
@export var distance: float = 30.0
@export var min_distance: float = 5.0
@export var max_distance: float = 100.0
@export var rotation_speed: float = 0.005
@export var zoom_speed: float = 2.0

var _yaw: float = 0.0
var _pitch: float = -0.5
var _dragging: bool = false


func _ready() -> void:
	_update_camera_position()


func _input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var mb := event as InputEventMouseButton
		if mb.button_index == MOUSE_BUTTON_RIGHT:
			_dragging = mb.pressed
		elif mb.button_index == MOUSE_BUTTON_WHEEL_UP:
			distance = max(min_distance, distance - zoom_speed)
			_update_camera_position()
		elif mb.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			distance = min(max_distance, distance + zoom_speed)
			_update_camera_position()

	elif event is InputEventMouseMotion and _dragging:
		var mm := event as InputEventMouseMotion
		_yaw -= mm.relative.x * rotation_speed
		_pitch -= mm.relative.y * rotation_speed
		_pitch = clamp(_pitch, -PI / 2 + 0.1, PI / 2 - 0.1)
		_update_camera_position()


func _update_camera_position() -> void:
	var offset := Vector3(
		cos(_pitch) * sin(_yaw),
		sin(_pitch),
		cos(_pitch) * cos(_yaw)
	) * distance

	global_position = target + offset
	look_at(target, Vector3.UP)
```

48 lines. A simple spherical-coordinate orbit camera.

### Design decisions

**Right-click to orbit** — Left-click is reserved for the terrain editor tools (brush, paint, smooth, etc.). The editor plugin captures left-click in `_forward_3d_gui_input()`. Right-click is the standard orbit convention in 3D editors.

**Scroll wheel to zoom** — `distance` is clamped between 5 and 100 units. The step size of 2.0 feels responsive without being jarring. These are all `@export` variables, so you can tune them in the inspector without editing code.

**Spherical coordinates** — The camera position is computed as `target + spherical_offset` where the offset uses `(yaw, pitch, distance)`. The `_pitch` is clamped to avoid gimbal lock at the poles (`-PI/2 + 0.1` to `PI/2 - 0.1` keeps us 0.1 radians away from straight up/down).

**Initial pitch of -0.5** — About -28.6 degrees, looking slightly downward. This gives a good initial view of terrain topography.

**`look_at(target, Vector3.UP)`** — After positioning the camera, we point it at the target. This is simpler than computing a rotation matrix ourselves and handles all edge cases.

**Why GDScript instead of Rust** — The camera is a testing utility, not part of the terrain system. Writing it in GDScript keeps it simple, editable without recompiling, and demonstrates that GDExtension classes coexist naturally with GDScript. There is no performance reason to write a camera controller in Rust.

---

## 8. Shader Assets & Resources

### Shaders

All shaders live in `godot/resources/shaders/`:

| File | Lines | Purpose |
|---|---|---|
| `mst_terrain.gdshader` | 454 | Main terrain surface shader — texture blending, cel-shading, wall detection |
| `mst_grass.gdshader` | 197 | Grass blade shader — wind animation, ground color sampling, alpha cutout |
| `round_brush_radius_visual.gdshader` | 15 | Circular brush indicator overlay |
| `square_brush_radius_visual.gdshader` | 14 | Square brush indicator overlay |

The terrain and grass shaders were covered in detail in Part 8. The brush shaders are minimal visual indicators used by the gizmo system (Part 15).

### Textures

All textures live in `godot/resources/textures/`:

| File | Type | Purpose |
|---|---|---|
| `default_ground_noise.tres` | `NoiseTexture2D` | FastNoiseLite (Cellular, freq 0.05, 256x256) — used for texture blend variation |
| `wind_noise_texture.tres` | `NoiseTexture2D` | FastNoiseLite (Cellular, seed 7, seamless, 3D) — drives grass wind animation |
| `grass_leaf_sprite.png` | PNG | Default grass blade sprite for the MultiMesh grass system |

The noise textures are Godot `.tres` resources, not image files. They procedurally generate their content at load time using FastNoiseLite. This means they are resolution-independent and never need to be checked into version control as binary blobs.

**`default_ground_noise.tres`:**
```
[sub_resource type="FastNoiseLite" id="FastNoiseLite_ground"]
noise_type = 3
frequency = 0.05

[resource]
width = 256
height = 256
noise = SubResource("FastNoiseLite_ground")
```
`noise_type = 3` is Cellular (Worley) noise — it creates organic, cell-like patterns that break up texture repetition on the terrain surface.

**`wind_noise_texture.tres`:**
```
[sub_resource type="FastNoiseLite" id="FastNoiseLite_wind"]
noise_type = 3
seed = 7
fractal_gain = 0.4
fractal_weighted_strength = 0.69

[resource]
noise = SubResource("FastNoiseLite_wind")
seamless = true
in_3d_space = true
seamless_blend_skirt = 0.76
```
`seamless = true` and `in_3d_space = true` make this texture tile without visible seams in all three dimensions. The grass shader scrolls UV coordinates through this texture over time to create wind motion. The `seamless_blend_skirt` of 0.76 provides a wide blend region at the seams, preventing any visible tiling artifacts as the wind scrolls.

### Materials

| File | Type | Purpose |
|---|---|---|
| `mst_terrain_material.tres` | `ShaderMaterial` | Pre-configured terrain material with default parameters |

**`mst_terrain_material.tres`:**
```
[gd_resource type="ShaderMaterial" format=3]

[ext_resource type="Shader" path="res://resources/shaders/mst_terrain.gdshader" id="1_shader"]

[resource]
render_priority = -1
shader = ExtResource("1_shader")
shader_parameter/wall_threshold = 0.0
shader_parameter/chunk_size = Vector3i(33, 32, 33)
shader_parameter/cell_size = Vector2(2, 2)
shader_parameter/ground_albedo = Color(0.392, 0.471, 0.318, 1)
shader_parameter/ground_albedo_2 = Color(0.322, 0.482, 0.384, 1)
... (15 ground colors, 15 texture scales, blend/shadow parameters)
```

`render_priority = -1` ensures the terrain renders before transparent objects (like grass blades). The six default ground colors are muted greens and olive tones — a natural palette for a fantasy-style terrain. The `terrain.rs` module's `ensure_terrain_material()` can load this material or create one from scratch if it is missing.

---

## 9. End-to-End Verification

### Step 1: Build the extension

```bash
cd rust && cargo build
```

Expected output (first build takes 2-3 minutes due to godot crate compilation):
```
   Compiling godot-bindings v0.2.x
   Compiling godot-ffi v0.2.x
   ...
   Compiling pixy_terrain v0.1.0 (/path/to/pixy_terrain/rust)
    Finished `dev` profile [optimized + debuginfo] target(s) in XXs
```

The build artifact appears at:
- macOS: `rust/target/debug/libpixy_terrain.dylib`
- Linux: `rust/target/debug/libpixy_terrain.so`
- Windows: `rust/target/debug/pixy_terrain.dll`

### Step 2: Open the Godot project

Open Godot 4.6 and import the `godot/` directory (or open the `godot/project.godot` file directly). The editor will:

1. Find `pixy_terrain.gdextension` in the project root
2. Read the `entry_symbol` and library path
3. Load the shared library and call `gdext_rust_init`
4. Register all 7 custom classes with ClassDB
5. Activate `PixyTerrainPlugin` (the editor plugin)

If the extension loaded successfully, you will see:
- "PixyTerrain" available in the Create Node dialog
- The terrain editor toolbar appears when you select a PixyTerrain node
- No errors in the Output panel

### Step 3: Open the test scene

Open `scenes/test_scene.tscn`. You should see:

- A 3D viewport with a procedural sky
- 9 terrain chunks arranged in a 3x3 grid
- Sculpted terrain with hills, cliffs, and flat areas
- Grass blades swaying in procedural wind
- Collision wireframes (toggleable with "Show Colliders")

### Step 4: Test the editor tools

1. **Select the PixyTerrain node** in the scene tree — the editor toolbar appears
2. **Choose the Brush tool** — click and drag on the terrain to raise it
3. **Switch to Vertex Paint** — paint vertex colors on the terrain surface
4. **Ctrl+Z** — undo the last operation
5. **Right-click and drag** — orbit the camera
6. **Scroll wheel** — zoom in/out

### Step 5: Verify hot reload

With Godot open:
1. Make a trivial change to any `.rs` file
2. Run `cargo build` in a terminal
3. Godot detects the library change and reloads the extension
4. Your changes are live without restarting the editor

### Common issues

| Symptom | Cause | Fix |
|---|---|---|
| "Can't open dynamic library" | Library not built | Run `cargo build` in `rust/` |
| PixyTerrain not in Create Node | `mod terrain;` missing in `lib.rs` | Add the module declaration |
| Terrain appears but no editor tools | Plugin not recognized | Verify `PixyTerrainPlugin` extends `EditorPlugin` |
| macOS: "code signature invalid" | Signing issue after rebuild | `codesign --force --deep --sign - rust/target/debug/libpixy_terrain.dylib` |
| Chunks don't render | Shader not found | Verify shaders exist in `godot/resources/shaders/` |

---

## 10. Full Project Summary

### The complete 18-part journey

| Part | Title | Key Deliverable |
|---|---|---|
| 1 | Project Scaffolding & Hello Godot | Directory structure, empty `lib.rs`, `Cargo.toml`, `.gdextension` |
| 2 | Marching Squares Data Model | `CellContext`, `CellGeometry`, height array, rotation system |
| 3 | Vertex Generation & Color Encoding | `add_point()`, `add_point_wall()`, UV/CUSTOM0-2 encoding |
| 4 | Floor & Wall Geometry Primitives | `add_floor()`, `add_wall()`, `add_diagonal_floor()`, `add_outer_corner()` |
| 5 | The 17-Case Cell Generator | `generate_cell()` with all marching squares cases |
| 6 | Chunk Data & Persistence | `PixyTerrainChunk` data model, `saved_*` arrays, `sync_to_packed()`/`restore_from_packed()` |
| 7 | Chunk Mesh Generation | SurfaceTool mesh building, collision shapes, `regenerate()` |
| 8 | Terrain Shaders | `mst_terrain.gdshader`, `mst_grass.gdshader`, brush shaders |
| 9 | Terrain Manager — Materials & Settings | `PixyTerrain` exports, `ensure_terrain_material()`, `force_batch_update()` |
| 10 | Terrain Manager — Chunk Operations | Chunk CRUD, `get_chunk()`, `get_chunk_keys()`, cross-chunk coordination |
| 11 | Resource Types & Texture Presets | `PixyTexturePreset`, `PixyQuickPaint`, save/load preset workflow |
| 12 | Editor Plugin — Tool Framework | `PixyTerrainPlugin`, 9 tool modes, mouse handling, raycast |
| 13 | Editor Plugin — Draw Pattern & Undo | `build_draw_pattern()`, composite undo/redo, `apply_composite_pattern_action()` |
| 14 | Editor Plugin — Cross-Chunk Operations | `propagate_cross_chunk_edges()`, `expand_wall_colors()`, edge sync |
| 15 | Gizmo Plugin | `PixyTerrainGizmoPlugin`, brush circle/square, chunk grid visualization |
| 16 | Grass Planter | `PixyGrassPlanter`, barycentric sampling, ledge avoidance, MultiMesh |
| 17 | Review Fixes & Parity Polish | Geometry corrections, bridge mode, UI additions, clippy clean |
| 18 | Test Scene & Final Integration | `lib.rs`, build config, scene, camera, verification (this part) |

### Module dependency graph

```
lib.rs
  |
  +-- marching_squares  (pure Rust, no Godot class)
  |     ^
  |     |  (generate_cell, CellContext, CellGeometry)
  |     |
  +-- chunk -----------+  (PixyTerrainChunk: MeshInstance3D)
  |     ^              |
  |     |              |  (get_color_0, get_height_at, regenerate)
  |     |              |
  +-- terrain ---------+  (PixyTerrain: Node3D)
  |     ^              |
  |     |              |  (get_chunk, ensure_terrain_material)
  |     |              |
  +-- grass_planter    |  (PixyGrassPlanter: MultiMeshInstance3D)
  |     ^              |
  |     |              |
  +-- editor_plugin ---+  (PixyTerrainPlugin: EditorPlugin)
  |     |
  |     +-- uses terrain, chunk (draw_pattern, undo/redo)
  |     +-- uses gizmo (registers gizmo plugin)
  |
  +-- gizmo               (PixyTerrainGizmoPlugin: EditorNode3DGizmoPlugin)
  |
  +-- quick_paint          (PixyQuickPaint: Resource)
  |
  +-- texture_preset       (PixyTexturePreset: Resource)
```

The dependency flow is strictly downward: `editor_plugin` depends on `terrain` and `chunk`, `terrain` depends on `chunk`, `chunk` depends on `marching_squares`. The resource types (`quick_paint`, `texture_preset`) are leaf nodes with no intra-project dependencies. `gizmo` is referenced by `editor_plugin` but does not depend on other project modules.

### Line counts

| File | Lines |
|---|---|
| `editor_plugin.rs` | 3,483 |
| `marching_squares.rs` | 1,617 |
| `terrain.rs` | 1,357 |
| `chunk.rs` | 973 |
| `grass_planter.rs` | 671 |
| `gizmo.rs` | 391 |
| `texture_preset.rs` | 307 |
| `quick_paint.rs` | 53 |
| `lib.rs` | 15 |
| **Rust total** | **8,867** |
| | |
| `mst_terrain.gdshader` | 454 |
| `mst_grass.gdshader` | 197 |
| `round_brush_radius_visual.gdshader` | 15 |
| `square_brush_radius_visual.gdshader` | 14 |
| `orbit_camera.gd` | 48 |
| **Godot total** | **728** |
| | |
| **Grand total** | **9,595** |

Under 10,000 lines for a complete 3D terrain editor with 17 marching squares cases, 9 editor tools, undo/redo, grass planting, texture presets, and hot-reloadable GDExtension integration.

### Architecture decisions that mattered

**Separating marching_squares from chunk.** The geometry algorithm is the most complex code in the project (1,617 lines). By keeping it in a pure Rust module with no Godot dependencies, we can test individual cases with `cargo test`, reason about the math without Godot API noise, and potentially reuse the algorithm in other contexts.

**CellContext as a value struct.** The original GDScript used class-level mutable state for the current cell being processed. We pass an immutable `CellContext` to every function instead. This eliminates an entire class of bugs where one function accidentally reads state set by a previous function for a different cell.

**PackedArray dual storage.** Runtime data lives in `Vec<f32>` and `Vec<Color>` for fast indexing. Serialized data lives in `PackedFloat32Array` and `PackedColorArray` for Godot scene persistence. The `sync_to_packed()` and `restore_from_packed()` methods bridge the two. This avoids the overhead of going through Godot's array API during tight inner loops.

**HashMap for chunk storage.** `HashMap<[i32;2], Gd<PixyTerrainChunk>>` gives O(1) chunk lookup by grid coordinate. The original GDScript used `get_children()` and searched by name. For 9 chunks the difference is negligible; for larger terrains it matters.

**Composite Dictionary for undo/redo.** Each brush stroke produces a single Dictionary containing per-chunk before/after snapshots. The `apply_composite_pattern_action()` method applies the entire Dictionary atomically. This means undo/redo always operates on complete brush strokes, never on partial states.

**Hot-reloadable by default.** The `reloadable = true` flag in `.gdextension`, combined with the `@rpath` linker flag on macOS, means the typical edit-build-test cycle takes seconds rather than minutes. This was a deliberate priority from the start.

---

## The Finish Line

You now have every file, every line, every design decision that makes Pixy Terrain work. From an empty directory to a fully functional 3D pixel-art terrain editor in 18 parts and 9,595 lines of code.

The original Yugen's Terrain Authoring Toolkit was roughly 3,200 lines of GDScript across three files. Our Rust reconstruction is about three times larger, but it includes things the original did not: static type safety across all 17 geometry cases, compile-time borrow checking that prevents the class of mutable-state bugs that plague GDScript tool scripts, editor UI panels built in Rust with full undo/redo integration, and a modular architecture that makes each subsystem independently testable.

What started as a port became a reconstruction. The terrain is the same. The code is new.

Build it. Open it. Sculpt something.
