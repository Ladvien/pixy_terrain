# Part 12 — Grass Planting System

**Series:** Reconstructing Pixy Terrain
**Part:** 12 of 18
**Previous:** 2026-02-06-resource-types-texture-presets-11.md
**Status:** Complete

## What We're Building

A MultiMesh-based grass instancing system that scatters grass blades across floor triangles. Each chunk gets a child `PixyGrassPlanter` node that reads the chunk's geometry, runs barycentric point-in-triangle tests to find valid floor positions, then places billboard grass instances with per-blade color sampled from the ground texture. The system supports 6 texture slots, per-slot grass sprites, ledge/ridge avoidance, a grass mask channel, and a wind animation shader.

## What You'll Have After This

Every terrain chunk spawns a `PixyGrassPlanter` child node. When the chunk's mesh regenerates, the planter scatters grass instances across all floor triangles. Grass respects the artist's grass mask (painted in the editor), avoids ledges and ridges, inherits the correct sprite for each texture slot, and samples the ground texture for per-blade color variation. The grass shader handles billboarding, wind animation, and cel-shaded lighting.

## Prerequisites

- Part 07 completed (`PixyTerrainChunk` with `regenerate_mesh`, `cell_geometry` HashMap)
- Part 05 completed (`CellGeometry` struct with `verts`, `uvs`, `colors_0`, `colors_1`, `grass_mask`, `is_floor`)
- Part 08 completed (grass shader `mst_grass.gdshader` in `godot/resources/shaders/`)
- Part 09 completed (`PixyTerrain` with grass-related exports: `grass_subdivisions`, `grass_size`, `ledge_threshold`, `ridge_threshold`, etc.)
- `get_dominant_color()` in `marching_squares.rs` (from Part 03)

## Steps

### Step 1: Define the `GrassConfig` struct

**Why:** The grass planter needs data from three different levels of the hierarchy: terrain settings (dimensions, subdivisions, thresholds), texture configuration (sprites, colors, toggles), and shared resources (material, mesh). In GDScript, these lived as class-level variables that any method could access. In Rust, the borrow checker prevents the planter from holding a reference back to its grandparent terrain while the chunk is also mutably borrowed. The solution is a `Clone`-able config struct that snapshots everything the planter needs at initialization time. The terrain builds it, passes it to the chunk, and the chunk passes it to the planter. After that, the planter operates independently with zero back-references.

**File:** `rust/src/grass_planter.rs`

```rust
use std::collections::HashMap;

use godot::classes::{
    IMultiMeshInstance3D, Image, Mesh, MultiMesh, MultiMeshInstance3D, QuadMesh, ResourceLoader,
    Shader, ShaderMaterial, Texture2D,
};
use godot::obj::InstanceId;
use godot::prelude::*;

use crate::chunk::PixyTerrainChunk;
use crate::marching_squares::{get_dominant_color, CellGeometry, MergeMode};

/// Cached grass configuration (avoids needing to bind terrain during grass operations).
/// Passed from terrain to chunk to grass planter at initialization time.
#[derive(Clone)]
pub struct GrassConfig {
    pub dimensions: Vector3i,
    pub subdivisions: i32,
    pub grass_size: Vector2,
    pub cell_size: Vector2,
    pub wall_threshold: f32,
    pub merge_mode: i32,
    pub animation_fps: i32,
    pub ledge_threshold: f32,
    pub ridge_threshold: f32,
    pub grass_sprites: [Option<Gd<Texture2D>>; 6],
    pub ground_colors: [Color; 6],
    pub tex_has_grass: [bool; 5],
    pub grass_mesh: Option<Gd<Mesh>>,
    /// Shared grass ShaderMaterial from terrain (one instance for all planters).
    pub grass_material: Option<Gd<ShaderMaterial>>,
    /// Shared QuadMesh with grass ShaderMaterial already applied (from terrain).
    /// When set, planters use this mesh directly instead of creating their own material.
    pub grass_quad_mesh: Option<Gd<Mesh>>,
    pub ground_images: [Option<Gd<Image>>; 6],
    pub texture_scales: [f32; 6],
}

impl Default for GrassConfig {
    fn default() -> Self {
        Self {
            dimensions: Vector3i::new(33, 32, 33),
            subdivisions: 3,
            grass_size: Vector2::new(1.0, 1.0),
            cell_size: Vector2::new(2.0, 2.0),
            wall_threshold: 0.0,
            merge_mode: 1,
            animation_fps: 0,
            ledge_threshold: 0.25,
            ridge_threshold: 1.0,
            grass_sprites: [None, None, None, None, None, None],
            ground_colors: [
                Color::from_rgba(0.3922, 0.4706, 0.3176, 1.0),
                Color::from_rgba(0.3216, 0.4824, 0.3843, 1.0),
                Color::from_rgba(0.3725, 0.4235, 0.2941, 1.0),
                Color::from_rgba(0.3922, 0.4745, 0.2549, 1.0),
                Color::from_rgba(0.2902, 0.4941, 0.3647, 1.0),
                Color::from_rgba(0.4431, 0.4471, 0.3647, 1.0),
            ],
            tex_has_grass: [true, true, true, true, true],
            grass_mesh: None,
            grass_material: None,
            grass_quad_mesh: None,
            ground_images: [None, None, None, None, None, None],
            texture_scales: [1.0; 6],
        }
    }
}
```

**What's happening:**

Every field in `GrassConfig` exists to answer a specific question the planter asks during grass placement:

| Field | Question it answers |
|---|---|
| `dimensions` | How many grid cells does this chunk have? (determines instance count) |
| `subdivisions` | How many grass sample points per cell? (subdivisions x subdivisions) |
| `grass_size` | How large is each grass billboard? (width/height of the QuadMesh) |
| `cell_size` | How large is each terrain cell in world units? (converts grid coords to world coords) |
| `wall_threshold` | Below what height difference do we still consider ground "flat enough" for grass? |
| `merge_mode` | Which marching squares merge mode is active? (affects shader `is_merge_round` flag) |
| `animation_fps` | Sprite sheet frame rate for animated grass textures |
| `ledge_threshold` | UV.x threshold for ledge detection (near cell edge = cliff = no grass) |
| `ridge_threshold` | UV.y threshold for ridge detection (near peak = ridge = no grass) |
| `grass_sprites` | 6 sprite textures for the grass shader (one per texture slot) |
| `ground_colors` | 6 ground albedo tints (fallback color when no texture image exists) |
| `tex_has_grass` | 5 toggles for slots 2-6 (slot 1 always has grass) |
| `grass_mesh` | Optional custom mesh overriding the default quad billboard |
| `grass_material` | Shared ShaderMaterial from terrain (avoids per-planter material creation) |
| `grass_quad_mesh` | Shared QuadMesh with material pre-applied (middle priority in the 3-tier mesh system) |
| `ground_images` | 6 extracted `Image` objects for per-blade ground texture sampling |
| `texture_scales` | 6 UV scale values for tiling the ground texture when sampling |

The `#[derive(Clone)]` is critical. `Gd<T>` implements `Clone` by incrementing Godot's reference count -- it does not deep-copy the texture data. Cloning a `GrassConfig` with 6 textures, 6 images, a mesh, and a material is essentially 15 reference count bumps plus a few scalar copies. This makes the terrain-to-chunk-to-planter handoff effectively free.

The six `ground_colors` defaults are the same muted earth tones from Part 09's terrain exports. They serve as fallback tint when no ground texture image is available for per-blade color sampling.

The `tex_has_grass` array has 5 elements, not 6, because texture slot 1 (the base grass) always has grass. There is no toggle for it. Slots 2-6 each get an artist-controlled toggle.

### Step 2: Define the constants

**Why:** The grass shader and wind noise texture live at fixed resource paths. Constants keep them in one place rather than scattering magic strings through the material setup code.

```rust
/// Path to the grass shader file.
const GRASS_SHADER_PATH: &str = "res://resources/shaders/mst_grass.gdshader";
/// Path to the wind noise texture.
const WIND_NOISE_TEXTURE_PATH: &str = "res://resources/textures/wind_noise_texture.tres";
```

**What's happening:**

`GRASS_SHADER_PATH` points to the grass shader from Part 08. This shader handles billboarding (rotating quads to face the camera), wind animation (vertex displacement driven by a noise texture), sprite sheet frame selection (using the custom data alpha channel), and cel-shaded lighting.

`WIND_NOISE_TEXTURE_PATH` is a Godot noise texture resource (`.tres` file) that the shader samples for wind displacement. Unlike code-generated noise, a `.tres` file gives artists control over the wind pattern by editing the resource in Godot's inspector.

### Step 3: Define the `PixyGrassPlanter` struct

**Why:** Each terrain chunk owns one grass planter. The planter extends `MultiMeshInstance3D`, which is Godot's high-performance instancing node -- it renders thousands of mesh copies in a single draw call using a `MultiMesh` resource. The struct stores the parent chunk's instance ID (for legacy compatibility) and the cached grass config.

```rust
/// MultiMeshInstance3D grass placement system.
/// Port of Yugen's MarchingSquaresGrassPlanter.
#[derive(GodotClass)]
#[class(base=MultiMeshInstance3D, init, tool)]
pub struct PixyGrassPlanter {
    base: Base<MultiMeshInstance3D>,

    /// Instance ID of the parent chunk (avoids borrow issues with Gd storage).
    chunk_instance_id: Option<InstanceId>,

    /// Cached grass configuration (avoids needing to bind terrain).
    grass_config: Option<GrassConfig>,
}

#[godot_api]
impl IMultiMeshInstance3D for PixyGrassPlanter {}
```

**What's happening:**

`#[class(base=MultiMeshInstance3D, init, tool)]` tells gdext three things:
1. This node extends `MultiMeshInstance3D` (Godot's GPU instancing node).
2. `init` generates a default constructor so Godot can instantiate it.
3. `tool` means this code runs in the editor, not just at runtime.

`chunk_instance_id` stores an `InstanceId` rather than a `Gd<PixyTerrainChunk>`. Storing a `Gd<T>` to the parent would create a reference cycle (chunk owns planter, planter references chunk) that complicates the borrow checker. An `InstanceId` is a plain integer that can be resolved back to a `Gd` on demand. In practice, the planter rarely needs to reach back to the chunk because the config snapshot contains everything it needs.

`grass_config` is `Option<GrassConfig>` because the planter starts unconfigured. It gets populated when `setup_with_config()` is called during chunk initialization.

The empty `IMultiMeshInstance3D` impl is required by gdext. Even if you don't override any virtual methods (`_ready`, `_process`, etc.), the trait impl must exist for the class to compile.

### Step 4: Implement `setup_with_config()` -- the 3-tier mesh priority system

**Why:** The planter needs a mesh to instance. There are three possible sources, forming a priority chain: (1) a custom artist-provided mesh, (2) a shared QuadMesh from the terrain with the grass material pre-applied, or (3) a fallback plain QuadMesh that the planter materials itself. The 3-tier system lets the terrain share a single material across all planters (tier 2) while still supporting custom meshes (tier 1) and graceful degradation (tier 3).

```rust
#[godot_api]
impl PixyGrassPlanter {
    /// Initialize the MultiMesh using cached config (avoids borrow issues).
    pub fn setup_with_config(
        &mut self,
        chunk_id: InstanceId,
        config: GrassConfig,
        regenerate: bool,
    ) {
        self.chunk_instance_id = Some(chunk_id);
        self.grass_config = Some(config.clone());

        let dim = config.dimensions;
        let subdivisions = config.subdivisions.max(1);
        let grass_size = config.grass_size;

        let instance_count = (dim.x - 1) * (dim.z - 1) * subdivisions * subdivisions;

        let existing = self.base().get_multimesh();
        if (regenerate && existing.is_some()) || existing.is_none() {
            let mut mm = MultiMesh::new_gd();
            mm.set_instance_count(0);
            mm.set_transform_format(godot::classes::multi_mesh::TransformFormat::TRANSFORM_3D);
            mm.set_use_custom_data(true);
            mm.set_instance_count(instance_count);

            if let Some(ref mesh) = config.grass_mesh {
                // User-provided custom grass mesh takes top priority
                mm.set_mesh(mesh);
            } else if let Some(ref shared) = config.grass_quad_mesh {
                // Shared QuadMesh from terrain (carries the shared ShaderMaterial)
                mm.set_mesh(shared);
            } else {
                // Fallback: create a plain QuadMesh (no material)
                let mut quad = QuadMesh::new_gd();
                quad.set_size(grass_size);
                quad.set_center_offset(Vector3::new(0.0, grass_size.y / 2.0, 0.0));
                mm.set_mesh(&quad);
            }

            self.base_mut().set_multimesh(&mm);
        }

        self.base_mut().set_cast_shadows_setting(
            godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
        );

        // Apply material: prefer shared material as material_override (most reliable path),
        // fall back to per-planter material creation if no shared material exists.
        let using_custom_mesh = config.grass_mesh.is_some();

        if using_custom_mesh {
            // Custom mesh provides its own material; clear any override
            self.base_mut().set_material_override(Gd::null_arg());
        } else if let Some(ref mat) = config.grass_material {
            // Apply shared material as material_override on each planter.
            // This is more reliable than relying on mesh surface material propagation
            // through MultiMesh, and updates propagate instantly since all planters
            // reference the same Godot ShaderMaterial object.
            self.base_mut().set_material_override(mat);
        } else {
            // Fallback: per-planter material (legacy path)
            self.setup_grass_material(
                config.wall_threshold,
                config.merge_mode,
                config.animation_fps,
                &config.grass_sprites,
                &config.ground_colors,
                &config.tex_has_grass,
            );
        }
    }

    /// Initialize the MultiMesh with proper instance count and mesh (legacy method).
    /// Note: This requires the chunk to have a terrain_config set.
    #[func]
    pub fn setup(&mut self, chunk: Gd<PixyTerrainChunk>, regenerate: bool) {
        let chunk_id = chunk.instance_id();
        // Use default grass config since we can't get terrain config from chunk in legacy path
        let config = GrassConfig::default();
        self.setup_with_config(chunk_id, config, regenerate);
    }
```

**What's happening:**

The instance count formula `(dim.x - 1) * (dim.z - 1) * subdivisions * subdivisions` deserves explanation. A 33x33 dimension grid has 32x32 = 1024 cells. With `subdivisions = 3`, each cell gets 3x3 = 9 sample points. Total: 1024 * 9 = 9216 potential grass instances per chunk.

The `MultiMesh` setup sequence has a subtlety:

```rust
mm.set_instance_count(0);
mm.set_transform_format(...);
mm.set_use_custom_data(true);
mm.set_instance_count(instance_count);
```

Setting instance count to 0 first, then configuring format and custom data, then setting the real count is required by Godot. The `MultiMesh` allocates its internal buffer when `set_instance_count` is called with a non-zero value. If you set the format *after* setting the count, the buffer layout is wrong. Setting to 0 first clears any stale allocation.

`set_use_custom_data(true)` enables a per-instance `Color` value that the shader reads as `INSTANCE_CUSTOM`. The planter uses this to encode the texture slot ID in the alpha channel and the sampled ground color in the RGB channels.

The 3-tier mesh priority:

| Priority | Condition | What happens |
|---|---|---|
| 1 (highest) | `config.grass_mesh` is Some | Use the custom mesh, clear material_override (mesh provides its own material) |
| 2 | `config.grass_quad_mesh` is Some | Use the shared QuadMesh (material is baked into the mesh surface), also set material_override for reliability |
| 3 (fallback) | Neither is set | Create a plain QuadMesh sized to `grass_size`, set up a per-planter ShaderMaterial |

Shadow casting is set to `OFF` because grass blades are thin billboards that produce ugly, flickering shadows. Disabling shadows also saves significant GPU time -- with 9000+ instances per chunk and multiple chunks, shadow map rendering would double the draw cost for negligible visual benefit in a pixel art game.

The `set_material_override` approach for tier 2 is intentional. In theory, the shared QuadMesh already carries the ShaderMaterial on its surface. But Godot's MultiMesh rendering path sometimes fails to propagate surface materials correctly. Using `material_override` on the `MultiMeshInstance3D` bypasses this entirely -- it overrides whatever the mesh says, guaranteeing the correct shader is used. Because all planters reference the same `Gd<ShaderMaterial>` object, changing a shader parameter on the terrain updates every planter simultaneously.

The legacy `setup()` method is marked `#[func]` (exposed to GDScript) and uses `GrassConfig::default()`. It exists for backward compatibility with scenes that call setup from GDScript. The preferred path is `setup_with_config()`, which takes a fully populated config from the terrain.

### Step 5: Implement `regenerate_all_cells_with_geometry()`

**Why:** When the chunk's mesh changes (height painting, texture painting, etc.), all grass must be recalculated. This method force-rebuilds the MultiMesh to clear stale instances, then iterates every cell to place fresh grass.

```rust
    /// Regenerate grass for all cells in the chunk.
    /// Takes cell_geometry as parameter to avoid needing to bind the chunk.
    /// Forces MultiMesh rebuild to clear stale instances from previous geometry.
    pub fn regenerate_all_cells_with_geometry(
        &mut self,
        cell_geometry: &HashMap<[i32; 2], CellGeometry>,
    ) {
        let Some(config) = self.grass_config.as_ref() else {
            godot_error!("PixyGrassPlanter: regenerate_all_cells — no grass config");
            return;
        };

        let dim = config.dimensions;

        // Always force-recreate the MultiMesh to clear stale instances from
        // previous geometry (e.g. after height changes via leveling).
        if let (Some(c_id), Some(cfg)) = (self.chunk_instance_id, self.grass_config.clone()) {
            self.setup_with_config(c_id, cfg, true);
        }

        for z in 0..(dim.z - 1) {
            for x in 0..(dim.x - 1) {
                self.generate_grass_on_cell_with_geometry(Vector2i::new(x, z), cell_geometry);
            }
        }
    }

    /// Legacy method for backward compatibility - warns and does nothing.
    #[func]
    pub fn regenerate_all_cells(&mut self) {
        godot_warn!("PixyGrassPlanter: regenerate_all_cells() called without geometry - skipping");
    }
```

**What's happening:**

The method takes `&HashMap<[i32; 2], CellGeometry>` as a parameter rather than reading it from the chunk. This is the key borrow-avoidance pattern throughout Pixy Terrain: the chunk calls `planter.bind_mut().regenerate_all_cells_with_geometry(&self.cell_geometry)`. If the planter tried to reach back and read the chunk's `cell_geometry` itself, we'd need a simultaneous mutable borrow of the planter and an immutable borrow of the chunk -- impossible when the planter is a child of the chunk and both are `Gd`-wrapped Godot objects.

The force-recreate call `self.setup_with_config(c_id, cfg, true)` with `regenerate: true` rebuilds the MultiMesh from scratch. This is necessary because height changes (leveling, smoothing) may change which cells have floor triangles. Stale instances from cells that are now walls would remain visible without a full rebuild.

The iteration `for z in 0..(dim.z - 1)` runs through all cells. A 33x33 grid has 32x32 = 1024 cells. Each cell is processed independently.

The legacy `regenerate_all_cells()` exists so scenes saved with older versions don't crash if they call the old API. It prints a warning instead of silently doing nothing.

### Step 6: Implement `generate_grass_on_cell_with_geometry()`

**Why:** Each cell gets `subdivisions x subdivisions` random sample points. The method calculates the base index into the MultiMesh, generates random points within the cell bounds, hands them to `place_grass_on_triangles()`, then hides any unused instances by teleporting them off-screen.

```rust
    /// Generate grass instances for a single cell using provided geometry.
    fn generate_grass_on_cell_with_geometry(
        &mut self,
        cell_coords: Vector2i,
        cell_geometry: &HashMap<[i32; 2], CellGeometry>,
    ) {
        let Some(config) = self.grass_config.as_ref() else {
            return;
        };

        // Get geometry from passed parameter instead of chunk lookup
        let geo = match cell_geometry.get(&[cell_coords.x, cell_coords.y]) {
            Some(g) => g.clone(),
            None => return,
        };

        // Use cell_size from config instead of chunk lookup
        let cell_size = config.cell_size;

        let dim = config.dimensions;
        let subdivisions = config.subdivisions.max(1);
        let ledge_threshold = config.ledge_threshold;
        let ridge_threshold = config.ridge_threshold;
        let tex_has_grass = [
            true, // tex1 (base grass) always has grass
            config.tex_has_grass[0],
            config.tex_has_grass[1],
            config.tex_has_grass[2],
            config.tex_has_grass[3],
            config.tex_has_grass[4],
        ];

        let count = (subdivisions * subdivisions) as usize;

        // Generate random sample points within this cell
        let mut points: Vec<Vector2> = Vec::with_capacity(count);
        for z in 0..subdivisions {
            for x in 0..subdivisions {
                let rx: f32 = rand_f32();
                let rz: f32 = rand_f32();
                points.push(Vector2::new(
                    (cell_coords.x as f32 + (x as f32 + rx) / subdivisions as f32) * cell_size.x,
                    (cell_coords.y as f32 + (z as f32 + rz) / subdivisions as f32) * cell_size.y,
                ));
            }
        }

        let base_index = (cell_coords.y * (dim.x - 1) + cell_coords.x) as usize * count;
        let end_index = base_index + count;
        let mut index = base_index;

        let Some(mm) = self.base().get_multimesh() else {
            return;
        };
        let mut mm = mm.clone();
        let total_instances = mm.get_instance_count() as usize;

        // Process each floor triangle
        self.place_grass_on_triangles(
            &geo,
            &mut points,
            &mut index,
            end_index.min(total_instances),
            &mut mm,
            cell_size,
            ledge_threshold,
            ridge_threshold,
            &tex_has_grass,
            config,
        );

        // Hide remaining unused instances
        let hidden_transform = Transform3D::new(
            Basis::from_scale(Vector3::ZERO),
            Vector3::new(9999.0, 9999.0, 9999.0),
        );
        while index < end_index && index < total_instances {
            mm.set_instance_transform(index as i32, hidden_transform);
            index += 1;
        }
    }

    /// Legacy method for backward compatibility - warns and does nothing.
    #[func]
    pub fn generate_grass_on_cell(&mut self, _cell_coords: Vector2i) {
        godot_warn!(
            "PixyGrassPlanter: generate_grass_on_cell() called without geometry - skipping"
        );
    }
}
```

**What's happening:**

The `tex_has_grass` array expansion is important. The config stores 5 booleans for slots 2-6. The method prepends `true` for slot 1 to create a 6-element array indexed 0-5, where index 0 = slot 1 (always has grass). This local expansion avoids off-by-one errors in the triangle placement loop.

The random point generation uses a subdivided jitter pattern. For `subdivisions = 3`, the cell is divided into a 3x3 sub-grid. Each sub-cell gets one random point, placed at a random offset within that sub-cell:

```
cell_coords.x as f32 + (x as f32 + rx) / subdivisions as f32
```

This produces a stratified random distribution -- more uniform than purely random placement, avoiding both clumping and the artificial regularity of a grid. The points are in world-space XZ coordinates (multiplied by `cell_size`).

The base index formula `(cell_coords.y * (dim.x - 1) + cell_coords.x) * count` maps 2D cell coordinates to a contiguous range within the MultiMesh's instance array. Cell (0,0) gets indices [0, count), cell (1,0) gets [count, 2*count), and so on. This is a standard row-major linearization.

Unused instances (points that didn't land on any floor triangle) are "hidden" by setting their transform to `(9999, 9999, 9999)` with a zero-scale basis. Godot's MultiMesh has no API to selectively disable instances -- the instance count is fixed at allocation time. Moving them far off-screen with zero scale is the standard workaround. The GPU still processes them, but they produce zero visible pixels.

### Step 7: Implement `place_grass_on_triangles()` -- the core placement algorithm

**Why:** This is the heart of the grass system. It takes the random sample points from Step 6 and tests each one against each floor triangle using barycentric coordinate math. When a point falls inside a triangle, the method interpolates the 3D position, checks ledge/ridge/mask/texture conditions, and if everything passes, places a billboard grass instance with per-blade color.

```rust
impl PixyGrassPlanter {
    /// Place grass instances on floor triangles using barycentric testing.
    #[allow(clippy::too_many_arguments)]
    fn place_grass_on_triangles(
        &self,
        geo: &CellGeometry,
        points: &mut Vec<Vector2>,
        index: &mut usize,
        end_index: usize,
        mm: &mut Gd<MultiMesh>,
        _cell_size: Vector2,
        ledge_threshold: f32,
        ridge_threshold: f32,
        tex_has_grass: &[bool; 6],
        config: &GrassConfig,
    ) {
        let hidden_transform = Transform3D::new(
            Basis::from_scale(Vector3::ZERO),
            Vector3::new(9999.0, 9999.0, 9999.0),
        );

        let num_verts = geo.verts.len();
        let mut tri_idx = 0;

        while tri_idx + 2 < num_verts {
            // Only place grass on floor triangles
            if !geo.is_floor[tri_idx] {
                tri_idx += 3;
                continue;
            }

            let a = geo.verts[tri_idx];
            let b = geo.verts[tri_idx + 1];
            let c = geo.verts[tri_idx + 2];

            // Precompute barycentric denominator (2D projection on XZ)
            let v0 = Vector2::new(c.x - a.x, c.z - a.z);
            let v1 = Vector2::new(b.x - a.x, b.z - a.z);
            let dot00 = v0.dot(v0);
            let dot01 = v0.dot(v1);
            let dot11 = v1.dot(v1);
            let denom = dot00 * dot11 - dot01 * dot01;

            if denom.abs() < 1e-10 {
                tri_idx += 3;
                continue;
            }
            let inv_denom = 1.0 / denom;

            let mut pt_idx = 0;
            while pt_idx < points.len() {
                if *index >= end_index {
                    return;
                }

                let v2 = Vector2::new(points[pt_idx].x - a.x, points[pt_idx].y - a.z);
                let dot02 = v0.dot(v2);
                let dot12 = v1.dot(v2);

                let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
                if u < 0.0 {
                    pt_idx += 1;
                    continue;
                }

                let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;
                if v < 0.0 {
                    pt_idx += 1;
                    continue;
                }

                if u + v <= 1.0 {
                    // Point is inside this triangle — remove it from candidates
                    points.remove(pt_idx);

                    // Interpolate 3D position using barycentric weights
                    let w = 1.0 - u - v;
                    let p = Vector3::new(
                        a.x * w + b.x * u + c.x * v,
                        a.y * w + b.y * u + c.y * v,
                        a.z * w + b.z * u + c.z * v,
                    );

                    // Ledge/ridge avoidance (same barycentric order)
                    let uv = Vector2::new(
                        geo.uvs[tri_idx].x * u
                            + geo.uvs[tri_idx + 1].x * v
                            + geo.uvs[tri_idx + 2].x * w,
                        geo.uvs[tri_idx].y * u
                            + geo.uvs[tri_idx + 1].y * v
                            + geo.uvs[tri_idx + 2].y * w,
                    );
                    let on_ledge = uv.x > 1.0 - ledge_threshold || uv.y > 1.0 - ridge_threshold;

                    // Interpolate vertex colors to determine texture
                    let col0 = interpolate_color(
                        geo.colors_0[tri_idx],
                        geo.colors_0[tri_idx + 1],
                        geo.colors_0[tri_idx + 2],
                        u,
                        v,
                        w,
                    );
                    let col1 = interpolate_color(
                        geo.colors_1[tri_idx],
                        geo.colors_1[tri_idx + 1],
                        geo.colors_1[tri_idx + 2],
                        u,
                        v,
                        w,
                    );
                    let col0 = get_dominant_color(col0);
                    let col1 = get_dominant_color(col1);

                    // Grass mask check
                    let mask = interpolate_color(
                        geo.grass_mask[tri_idx],
                        geo.grass_mask[tri_idx + 1],
                        geo.grass_mask[tri_idx + 2],
                        u,
                        v,
                        w,
                    );
                    let is_masked = mask.r < 0.9999;
                    let force_grass_on = mask.g >= 0.9999;

                    // Texture grass check
                    let texture_id = get_texture_id(col0, col1);
                    let on_grass_tex = if force_grass_on {
                        true
                    } else if (1..=6).contains(&texture_id) {
                        tex_has_grass[(texture_id - 1) as usize]
                    } else {
                        false
                    };

                    if on_grass_tex && !on_ledge && !is_masked {
                        // Compute billboard basis from triangle normal
                        let edge1 = b - a;
                        let edge2 = c - a;
                        let normal = edge1.cross(edge2).normalized();

                        let right = Vector3::FORWARD.cross(normal).normalized();
                        let forward = normal.cross(Vector3::RIGHT).normalized();
                        let basis = Basis::from_cols(right, forward, -normal);

                        mm.set_instance_transform(*index as i32, Transform3D::new(basis, p));

                        // Set custom data: RGB = sampled terrain color (per-blade variation),
                        // A = sprite ID encoding (0.0 = tex1, 0.2 = tex2, ... 1.0 = tex6)
                        let alpha = match texture_id {
                            6 => 1.0,
                            5 => 0.8,
                            4 => 0.6,
                            3 => 0.4,
                            2 => 0.2,
                            _ => 0.0, // base grass
                        };

                        // Sample ground texture at blade position for per-blade color variation.
                        let slot_idx = (texture_id - 1).clamp(0, 5) as usize;
                        let rgb = if let Some(ref img) = config.ground_images[slot_idx] {
                            let dim = config.dimensions;
                            let total_x = dim.x as f32 * config.cell_size.x;
                            let total_z = dim.z as f32 * config.cell_size.y;
                            let mut uv_x = (p.x / total_x).clamp(0.0, 1.0);
                            let mut uv_y = (p.z / total_z).clamp(0.0, 1.0);

                            uv_x *= config.texture_scales[slot_idx];
                            uv_y *= config.texture_scales[slot_idx];

                            uv_x = uv_x.rem_euclid(1.0).abs();
                            uv_y = uv_y.rem_euclid(1.0).abs();

                            let w = img.get_width().max(1);
                            let h = img.get_height().max(1);
                            let px = (uv_x * (w - 1) as f32) as i32;
                            let py = (uv_y * (h - 1) as f32) as i32;

                            img.get_pixel(px, py)
                        } else {
                            // No texture image — fall back to ground color
                            config.ground_colors[slot_idx]
                        };

                        let instance_color = Color::from_rgba(rgb.r, rgb.g, rgb.b, alpha);
                        mm.set_instance_custom_data(*index as i32, instance_color);
                    } else {
                        mm.set_instance_transform(*index as i32, hidden_transform);
                    }

                    *index += 1;
                } else {
                    pt_idx += 1;
                }
            }

            tri_idx += 3;
        }
    }
}
```

**What's happening:**

This is the largest and most algorithmic method in the file. Let's break it down into its key subsystems.

**Barycentric point-in-triangle test (2D XZ projection)**

The terrain is essentially 2.5D -- the XZ plane determines which cell a point belongs to, and Y is the height. The barycentric test projects the 3D triangle onto the XZ plane and tests the 2D point against it.

Given triangle vertices A, B, C projected to XZ, and a test point P:

```
v0 = C - A     (edge from A to C, in XZ)
v1 = B - A     (edge from A to B, in XZ)
v2 = P - A     (vector from A to test point, in XZ)
```

The barycentric coordinates (u, v) are computed using the dot product formulas:

```
u = (dot11 * dot02 - dot01 * dot12) / denom
v = (dot00 * dot12 - dot01 * dot02) / denom
```

where `denom = dot00 * dot11 - dot01 * dot01`. The point is inside the triangle when `u >= 0`, `v >= 0`, and `u + v <= 1`. The third coordinate `w = 1 - u - v`.

The denominator is precomputed once per triangle (`inv_denom`) since it's the same for all test points against that triangle.

The degenerate triangle check (`denom.abs() < 1e-10`) catches collinear vertices that would cause a division-by-zero.

**Candidate point removal**

When a point is found inside a triangle, `points.remove(pt_idx)` removes it from the candidate list. This is an O(n) removal that shifts later elements, but with only 9 points per cell (subdivisions=3), the cost is negligible. Removing claimed points prevents them from being placed again in a later triangle -- each grass blade belongs to exactly one triangle.

Note the loop structure: `pt_idx` only advances when the point is NOT consumed. When a point is removed, the next point slides into `pt_idx`, so we don't increment.

**3D position interpolation**

Once barycentric coordinates are known, the 3D position is:

```
p = a * w + b * u + c * v
```

This places the grass blade at the correct height on the triangle surface, even for sloped terrain.

**Ledge and ridge avoidance**

The UV coordinates (from the marching squares geometry in Part 05) encode proximity to cell edges. `uv.x` close to 1.0 means the point is near a ledge (height discontinuity). `uv.y` close to 1.0 means the point is near a ridge (peak). The thresholds (`ledge_threshold = 0.25`, `ridge_threshold = 1.0` by default) control how wide the exclusion zone is. Grass near cliff edges looks unnatural because the blades would float in mid-air.

**Vertex color interpolation and texture ID**

The `colors_0` and `colors_1` arrays from `CellGeometry` encode which texture slot each vertex belongs to (the encoding system from Part 03). Interpolating these colors across the triangle surface and then calling `get_dominant_color()` snaps the result to the nearest one-hot color. This determines which texture slot the grass blade is on.

**Grass mask check**

The grass mask is a per-vertex color channel painted by the artist:
- `mask.r < 0.9999` = the red channel is below threshold = grass is masked (hidden) at this point
- `mask.g >= 0.9999` = the green channel is above threshold = grass is force-enabled regardless of texture toggle

This gives artists two override controls: paint red to remove grass, paint green to force grass on textures that otherwise have `has_grass = false`.

**Billboard basis from triangle normal**

The grass quad needs to face the camera while following the terrain slope. The basis is computed from the triangle normal:

```
right   = FORWARD x normal   (horizontal axis)
forward = normal x RIGHT     (vertical-ish axis)
basis   = [right, forward, -normal]
```

This orients the quad perpendicular to the terrain surface. The shader handles the final camera-facing rotation at render time.

**INSTANCE_CUSTOM alpha encoding**

The alpha channel encodes the texture slot ID as a float in [0.0, 1.0]:

| texture_id | alpha |
|---|---|
| 1 (base) | 0.0 |
| 2 | 0.2 |
| 3 | 0.4 |
| 4 | 0.6 |
| 5 | 0.8 |
| 6 | 1.0 |

The grass shader reads `INSTANCE_CUSTOM.a` and selects the corresponding sprite texture. The 0.2 spacing gives the shader enough precision to distinguish 6 slots using simple threshold comparisons.

**Ground texture sampling for per-blade color variation**

This is what makes the grass look natural rather than uniformly colored. Each grass blade samples the ground texture at its XZ position:

1. Convert world position to UV: `uv_x = p.x / total_x`, `uv_y = p.z / total_z`
2. Apply texture scale: `uv_x *= texture_scales[slot_idx]`
3. Tile with `rem_euclid(1.0)` so scaled UVs wrap correctly
4. Sample the pixel: `img.get_pixel(px, py)`

The sampled color goes into `INSTANCE_CUSTOM.rgb`. The shader uses this to tint each grass blade differently, matching the terrain pixel directly beneath it. On a mossy rock texture, blades over dark green pixels are darker; blades over light patches are lighter.

If no ground image exists (the artist didn't assign a texture to that slot), the fallback is the flat `ground_colors[slot_idx]` tint from the config.

### Step 8: Implement `setup_grass_material()` -- the fallback material path

**Why:** When no shared material exists (tier 3 fallback from Step 4), each planter creates its own `ShaderMaterial`. This method loads the grass shader, configures all uniforms (textures, colors, wind, shading), and sets it as the planter's `material_override`.

```rust
impl PixyGrassPlanter {
    /// Set up or update the grass ShaderMaterial with proper parameters.
    #[allow(clippy::too_many_arguments)]
    fn setup_grass_material(
        &mut self,
        wall_threshold: f32,
        merge_mode: i32,
        animation_fps: i32,
        grass_sprites: &[Option<Gd<godot::classes::Texture2D>>; 6],
        ground_colors: &[Color; 6],
        tex_has_grass: &[bool; 5],
    ) {
        let mut loader = ResourceLoader::singleton();

        // Try loading the grass shader
        if !loader.exists(GRASS_SHADER_PATH) {
            godot_warn!("PixyGrassPlanter: Grass shader not found at {GRASS_SHADER_PATH}");
            return;
        }

        let resource = loader.load(GRASS_SHADER_PATH);
        let Some(res) = resource else {
            godot_warn!("PixyGrassPlanter: Failed to load grass shader");
            return;
        };

        let Ok(shader) = res.try_cast::<Shader>() else {
            godot_warn!("PixyGrassPlanter: Resource is not a Shader");
            return;
        };

        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);

        // Core parameters
        let is_merge_round = matches!(
            MergeMode::from_index(merge_mode),
            MergeMode::RoundedPolyhedron | MergeMode::SemiRound | MergeMode::Spherical
        );
        mat.set_shader_parameter("is_merge_round", &is_merge_round.to_variant());
        mat.set_shader_parameter("wall_threshold", &wall_threshold.to_variant());
        mat.set_shader_parameter("fps", &(animation_fps as f32).to_variant());

        // Grass textures (grass_texture, grass_texture_2, ..., grass_texture_6)
        let texture_names = [
            "grass_texture",
            "grass_texture_2",
            "grass_texture_3",
            "grass_texture_4",
            "grass_texture_5",
            "grass_texture_6",
        ];
        for (i, name) in texture_names.iter().enumerate() {
            if let Some(ref tex) = grass_sprites[i] {
                mat.set_shader_parameter(*name, &tex.to_variant());
            }
        }

        // Base grass color (grass_base_color) and per-texture colors
        mat.set_shader_parameter(
            "grass_base_color",
            &Vector3::new(ground_colors[0].r, ground_colors[0].g, ground_colors[0].b).to_variant(),
        );
        let color_names = [
            "grass_color_2",
            "grass_color_3",
            "grass_color_4",
            "grass_color_5",
            "grass_color_6",
        ];
        for (i, name) in color_names.iter().enumerate() {
            let c = ground_colors[i + 1];
            mat.set_shader_parameter(*name, &Vector3::new(c.r, c.g, c.b).to_variant());
        }

        // use_base_color flags (true when no texture is assigned)
        mat.set_shader_parameter("use_base_color", &grass_sprites[0].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_2", &grass_sprites[1].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_3", &grass_sprites[2].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_4", &grass_sprites[3].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_5", &grass_sprites[4].is_none().to_variant());
        mat.set_shader_parameter("use_base_color_6", &grass_sprites[5].is_none().to_variant());

        // use_grass_tex_* flags (tex2-6 has_grass toggles)
        mat.set_shader_parameter("use_grass_tex_2", &tex_has_grass[0].to_variant());
        mat.set_shader_parameter("use_grass_tex_3", &tex_has_grass[1].to_variant());
        mat.set_shader_parameter("use_grass_tex_4", &tex_has_grass[2].to_variant());
        mat.set_shader_parameter("use_grass_tex_5", &tex_has_grass[3].to_variant());
        mat.set_shader_parameter("use_grass_tex_6", &tex_has_grass[4].to_variant());

        // Wind animation parameters (mst_grass.gdshader lines 39-46)
        // Values from Yugen's mst_grass_mesh.tres: wind_scale=0.02, wind_speed=0.14
        mat.set_shader_parameter("wind_direction", &Vector2::new(1.0, 1.0).to_variant());
        mat.set_shader_parameter("wind_scale", &0.02_f32.to_variant());
        mat.set_shader_parameter("wind_speed", &0.14_f32.to_variant());
        mat.set_shader_parameter("animate_active", &true.to_variant());

        // Load wind noise texture for wind animation
        if loader.exists(WIND_NOISE_TEXTURE_PATH) {
            if let Some(wind_tex) = loader.load(WIND_NOISE_TEXTURE_PATH) {
                mat.set_shader_parameter("wind_texture", &wind_tex.to_variant());
            }
        }

        // Shading parameters (mst_grass.gdshader lines 48-52)
        mat.set_shader_parameter(
            "shadow_color",
            &Color::from_rgba(0.0, 0.0, 0.0, 1.0).to_variant(),
        );
        mat.set_shader_parameter("bands", &5_i32.to_variant());
        mat.set_shader_parameter("shadow_intensity", &0.0_f32.to_variant());

        self.base_mut().set_material_override(&mat);
        godot_print!("PixyGrassPlanter: Grass material set up successfully");
    }
```

**What's happening:**

This method is in a separate `impl PixyGrassPlanter` block (not `#[godot_api]`), making it a private Rust method invisible to GDScript. It's only called from the fallback path in `setup_with_config()`.

The shader parameters fall into 5 groups:

**Core parameters:**
- `is_merge_round`: Whether the terrain uses rounded merge modes. The grass shader adjusts its ground-clipping behavior based on this -- rounded terrain has smoother transitions that need different clip thresholds.
- `wall_threshold`: Height difference below which surfaces are still "floor". The shader uses this to clip grass blades that would poke through walls.
- `fps`: Frame rate for animated sprite sheets. 0 = static grass.

**Texture parameters (6 slots):**
The first slot uses `grass_texture` (no suffix). Slots 2-6 use `grass_texture_2` through `grass_texture_6`. This naming asymmetry comes from the original GDScript shader. Textures are only set if non-None -- the shader has default behavior when a texture is absent.

**Color parameters:**
`grass_base_color` is a `Vector3` (RGB only, no alpha) for slot 1. Slots 2-6 use `grass_color_2` through `grass_color_6`. The shader converts Godot `Color` to `Vector3` because shader color uniforms in Godot's shading language are vec3 for opaque colors.

**use_base_color flags:**
When no sprite texture is assigned to a slot (`grass_sprites[i].is_none()`), the shader falls back to the flat color tint. These boolean flags tell the shader which slots are texture-driven vs color-driven.

**Wind parameters:**
The values (direction `(1,1)`, scale `0.02`, speed `0.14`) come from Yugen's original `mst_grass_mesh.tres`. The wind noise texture provides spatial variation so grass doesn't sway uniformly.

**Shading parameters:**
`shadow_color` (black), `bands` (5), and `shadow_intensity` (0.0) configure the cel-shading pass. With `shadow_intensity = 0.0`, shadows are effectively disabled by default -- artists can increase it for a more stylized look.

The `ResourceLoader::singleton()` call requires `mut` because loading resources is a stateful operation in Godot (it may trigger imports, update caches, etc.). The three-step load-and-cast pattern (`exists` -> `load` -> `try_cast`) is defensive: each step can fail independently.

### Step 9: Implement free functions -- `interpolate_color()`, `get_texture_id()`, `rand_f32()`

**Why:** These utility functions are used by `place_grass_on_triangles()` but don't need access to `self`. Keeping them as free functions (outside any impl block) makes them testable and reusable.

```rust
/// Interpolate three colors using barycentric weights.
/// GDScript order: colors_0[i]*u + colors_0[i+1]*v + colors_0[i+2]*(1-u-v)
/// So: a*u + b*v + c*w where w = 1-u-v
fn interpolate_color(a: Color, b: Color, c: Color, u: f32, v: f32, w: f32) -> Color {
    Color::from_rgba(
        a.r * u + b.r * v + c.r * w,
        a.g * u + b.g * v + c.g * w,
        a.b * u + b.b * v + c.b * w,
        a.a * u + b.a * v + c.a * w,
    )
}

/// Map vertex color pair to 1-based texture ID (1-16).
/// Matches Yugen's _get_texture_id() encoding.
fn get_texture_id(col0: Color, col1: Color) -> i32 {
    let c0 = if col0.r > 0.9999 {
        0
    } else if col0.g > 0.9999 {
        1
    } else if col0.b > 0.9999 {
        2
    } else if col0.a > 0.9999 {
        3
    } else {
        0
    };

    let c1 = if col1.r > 0.9999 {
        0
    } else if col1.g > 0.9999 {
        1
    } else if col1.b > 0.9999 {
        2
    } else if col1.a > 0.9999 {
        3
    } else {
        0
    };

    c0 * 4 + c1 + 1 // 1-based to match Yugen
}

/// Simple random float [0, 1) using Godot's random.
fn rand_f32() -> f32 {
    // Use a simple hash-based random since we don't need crypto quality
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEED: AtomicU32 = AtomicU32::new(12345);
    let mut s = SEED.load(Ordering::Relaxed);
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    SEED.store(s, Ordering::Relaxed);
    (s as f32) / (u32::MAX as f32)
}
```

**What's happening:**

**`interpolate_color()`** performs component-wise linear interpolation using barycentric weights. Given three vertex colors (a, b, c) and weights (u, v, w) where `w = 1 - u - v`, the result is `a*u + b*v + c*w`. Each RGBA channel is interpolated independently. This is used for all three per-vertex color attributes: `colors_0`, `colors_1`, and `grass_mask`.

**`get_texture_id()`** is the inverse of the `texture_index_to_colors()` function from Part 03. That function converts a slot index to a `(Color, Color)` pair. This function converts the pair back to an index. The encoding uses one-hot RGBA channels across two colors:

| Channel above 0.9999 | Value |
|---|---|
| R | 0 |
| G | 1 |
| B | 2 |
| A | 3 |

The formula `c0 * 4 + c1 + 1` gives a 1-based ID from 1 to 16. For example, if `col0.r > 0.9999` (c0=0) and `col1.r > 0.9999` (c1=0), the result is `0*4 + 0 + 1 = 1` (texture slot 1). If `col0.g > 0.9999` (c0=1) and `col1.b > 0.9999` (c1=2), the result is `1*4 + 2 + 1 = 7` (texture slot 7).

The 0.9999 threshold (rather than exact 1.0 comparison) accounts for floating-point imprecision from the barycentric interpolation. After `interpolate_color()` and `get_dominant_color()`, the values are snapped back to one-hot, but the threshold adds a safety margin.

**`rand_f32()`** implements a xorshift32 PRNG using a global `AtomicU32` seed. The three xor-shift operations (left 13, right 17, left 5) are the standard xorshift32 constants from Marsaglia's 2003 paper. This produces a pseudo-random sequence that's good enough for grass jitter -- we don't need cryptographic quality.

The `AtomicU32` with `Ordering::Relaxed` makes this thread-safe without a mutex. `Relaxed` ordering means no synchronization guarantees beyond atomicity -- concurrent calls might read stale seeds and produce the same value, but for grass placement, occasional duplicates are invisible. A proper `Mutex<u32>` would be correct but needlessly slow for this use case.

The seed starts at `12345`. This means grass placement is deterministic across runs (same positions every time), but different enough cell-to-cell because the seed advances globally. If truly random placement were needed, the seed could be initialized from system time.

## Verify

```bash
cd rust && cargo build
```

The build should complete without errors. The `PixyGrassPlanter` class is now a fully functional MultiMeshInstance3D node.

## Integration with Chunk

The chunk from Part 07 already has a `grass_planter: Option<Gd<PixyGrassPlanter>>` field. The connection happens in `PixyTerrainChunk::initialize_terrain()`, which receives a `GrassConfig` parameter from the terrain:

```rust
// In chunk.rs — initialize_terrain()

// Reuse existing grass planter from scene save, or create new one
if self.grass_planter.is_none() {
    let name = GString::from("GrassPlanter");
    if let Some(child) = self
        .base()
        .find_child_ex(&name)
        .recursive(false)
        .owned(false)
        .done()
    {
        if let Ok(mut planter) = child.try_cast::<PixyGrassPlanter>() {
            let chunk_id = self.base().instance_id();
            planter
                .bind_mut()
                .setup_with_config(chunk_id, grass_config.clone(), true);
            self.grass_planter = Some(planter);
        }
    }
}

if self.grass_planter.is_none() {
    // Create new planter (for genuinely new chunks)
    let mut planter = PixyGrassPlanter::new_alloc();
    planter.set_name("GrassPlanter");

    let chunk_id = self.base().instance_id();
    planter
        .bind_mut()
        .setup_with_config(chunk_id, grass_config, true);

    self.base_mut().add_child(&planter);

    // Set owner for editor persistence
    if let Some(owner) = self.base().get_owner() {
        planter.set_owner(&owner);
    }

    self.grass_planter = Some(planter);
}
```

And after mesh regeneration:

```rust
// In chunk.rs — regenerate_mesh_with_material()

// Regenerate grass on top of the new mesh
// Pass cell_geometry directly to avoid grass planter needing to bind chunk
if let Some(ref mut planter) = self.grass_planter {
    planter
        .bind_mut()
        .regenerate_all_cells_with_geometry(&self.cell_geometry);
}
```

The terrain builds the `GrassConfig` in `make_grass_config()` before iterating chunks, then passes it as a parameter. This "build config once, pass by value" pattern avoids all the borrow-checker issues that would arise from the planter trying to reach back up the hierarchy to read terrain settings.

## What You Learned

- **Config snapshot pattern**: When Rust's borrow checker prevents child nodes from referencing parent data, snapshot the needed values into a `Clone`-able struct at initialization time. The struct travels terrain -> chunk -> planter as a value, creating zero back-references.
- **MultiMesh instancing**: Godot's `MultiMesh` renders thousands of mesh copies in one draw call. Each instance has a `Transform3D` (position/rotation/scale) and an optional `Color` as custom data. The instance count is fixed at allocation time; unused instances are hidden by moving them off-screen.
- **Barycentric point-in-triangle testing**: Project 3D triangles to 2D (XZ plane), precompute the denominator once per triangle, then test each candidate point with two dot products and three comparisons. Points inside the triangle are consumed (removed from the candidate list) and interpolated back to 3D.
- **INSTANCE_CUSTOM encoding**: The 4 channels of the custom data `Color` pack two pieces of information: RGB = per-blade ground texture sample, A = texture slot ID (0.0-1.0 in 0.2 increments). The shader reads this to select the correct sprite and tint each blade individually.
- **Xorshift PRNG**: A simple, fast, non-cryptographic random number generator using three xor-shift operations on a 32-bit integer. An `AtomicU32` makes it thread-safe with minimal overhead.
- **Ground texture sampling**: Converting world XZ position to UV, applying texture scale with tiling (`rem_euclid`), then sampling the `Image` pixel gives each grass blade the exact color of the terrain beneath it, creating natural color variation without additional textures.

## Stubs Introduced

- None

## Stubs Resolved

- [x] `grass_planter` module (empty stub from Part 01) -- now full `PixyGrassPlanter` implementation
- [x] `chunk.grass_planter` field -- now connected via `initialize_terrain()` and `regenerate_mesh_with_material()`
