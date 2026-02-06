# Pixy Terrain — Part 06: Chunk Data & Persistence

**Series:** Reconstructing Pixy Terrain
**Part:** 06 of 18
**Previous:** 2026-02-06-seventeen-case-cell-generator-05.md
**Status:** Complete

## What We're Building

The `PixyTerrainChunk` data model — the Godot node that owns a chunk's heightmap, color maps, grass mask, and provides the draw/get API for the editor plugin to read and write terrain data. Also: the persistence layer that converts between runtime `Vec` data and Godot's `PackedArray` types for scene serialization.

## What You'll Have After This

A `PixyTerrainChunk` (MeshInstance3D) that can be created, initialized with heightmap data (from noise or flat), store 6 color/mask maps, serialize to Godot packed arrays on exit, and restore on scene reload. It cannot generate a mesh yet — that comes in Part 07.

## Prerequisites

- Part 05 completed (full `marching_squares.rs` with `CellContext`, `CellGeometry`, `MergeMode`)

## Steps

### Step 1: Add stub types to `grass_planter.rs`

**Why:** `chunk.rs` depends on two types from `grass_planter.rs`: `GrassConfig` (a configuration struct passed from terrain → chunk → planter) and `PixyGrassPlanter` (a MultiMeshInstance3D node created as a child of each chunk). The full grass system isn't built until Part 12, but we need these types to exist now so chunk.rs compiles.

**File:** `rust/src/grass_planter.rs` (replace the empty stub from Part 01)

```rust
use std::collections::HashMap;

use godot::classes::{
    IMultiMeshInstance3D, Image, Mesh, MultiMesh, MultiMeshInstance3D, QuadMesh, ResourceLoader,
    Shader, ShaderMaterial, Texture2D,
};
use godot::obj::InstanceId;
use godot::prelude::*;

use crate::marching_squares::CellGeometry;

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
            ground_colors: [Color::from_rgba(0.4, 0.5, 0.3, 1.0); 6],
            tex_has_grass: [true; 5],
            grass_mesh: None,
            grass_material: None,
            grass_quad_mesh: None,
            ground_images: [None, None, None, None, None, None],
            texture_scales: [1.0; 6],
        }
    }
}

/// Grass planter node — places MultiMesh grass blades on floor triangles.
/// Full implementation in Part 12. Stub for now.
#[derive(GodotClass)]
#[class(base=MultiMeshInstance3D, init, tool)]
pub struct PixyGrassPlanter {
    base: Base<MultiMeshInstance3D>,

    grass_config: GrassConfig,
    parent_chunk_id: Option<InstanceId>,
}

impl PixyGrassPlanter {
    /// Initialize the planter with a cached config. Called by chunk during initialization.
    pub fn setup_with_config(
        &mut self,
        chunk_id: InstanceId,
        config: GrassConfig,
        _force_rebuild: bool,
    ) {
        self.parent_chunk_id = Some(chunk_id);
        self.grass_config = config;
    }

    /// Regenerate grass on all cells using pre-built geometry.
    /// Full implementation in Part 12. Stub: does nothing.
    pub fn regenerate_all_cells_with_geometry(
        &mut self,
        _cell_geometry: &HashMap<[i32; 2], CellGeometry>,
    ) {
        // Stub: grass placement implemented in Part 12
    }
}
```

**What's happening:**
- `GrassConfig` is the *full* final struct — it's a plain data carrier with no methods beyond `Default`, so we can define it completely now. Every field maps to a terrain export that the grass system needs.
- `PixyGrassPlanter` is a `MultiMeshInstance3D` with two stub methods: `setup_with_config()` (stores config) and `regenerate_all_cells_with_geometry()` (no-op). Part 12 fills in the actual grass placement algorithm.
- The imports look heavy for a stub, but they match the types `GrassConfig` uses (Texture2D, Image, Mesh, ShaderMaterial). We declare them now so we don't have to restructure imports later.

### Step 2: Create `TerrainConfig`

**Why:** `PixyTerrainChunk` needs terrain-level settings (dimensions, cell size, blend mode) to generate geometry and configure collision. But in Rust, you can't hold a reference to the parent terrain node while also mutating the chunk — that violates the borrow checker. The solution: a plain `Clone`-able config struct that captures everything the chunk needs at initialization time.

**File:** `rust/src/chunk.rs` (replace the empty stub from Part 01)

```rust
use std::collections::HashMap;

use godot::classes::mesh::PrimitiveType;
use godot::classes::surface_tool::CustomFormat;
use godot::classes::{
    ArrayMesh, Engine, IMeshInstance3D, MeshInstance3D, Noise, ShaderMaterial, StaticBody3D,
    SurfaceTool,
};
use godot::prelude::*;

use crate::grass_planter::{GrassConfig, PixyGrassPlanter};
use crate::marching_squares::{self, CellContext, CellGeometry, MergeMode};

/// Cached terrain configuration (avoids needing to bind terrain during chunk operations).
/// Passed from terrain to chunk at initialization time to break the borrow cycle.
#[derive(Clone, Debug)]
pub struct TerrainConfig {
    pub dimensions: Vector3i,
    pub cell_size: Vector2,
    pub blend_mode: i32,
    pub use_ridge_texture: bool,
    pub ridge_threshold: f32,
    pub extra_collision_layer: i32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            dimensions: Vector3i::new(33, 32, 33),
            cell_size: Vector2::new(2.0, 2.0),
            blend_mode: 0,
            use_ridge_texture: false,
            ridge_threshold: 1.0,
            extra_collision_layer: 9,
        }
    }
}
```

**What's happening:**
- `dimensions: Vector3i::new(33, 32, 33)` — 33×33 grid = 32×32 cells per chunk. The Y component (32) is the maximum height range.
- `cell_size: Vector2::new(2.0, 2.0)` — each cell occupies 2×2 world units. With 32 cells, a chunk is 64×64 units.
- `blend_mode` controls how texture transitions look (smooth vs hard edge).
- `extra_collision_layer` is the physics layer for editor raycasting (defaults to layer 9).
- `Clone` and `Debug` are derived because `TerrainConfig` is passed around by value — one copy lives in each chunk.

### Step 3: Define the `PixyTerrainChunk` struct

**Why:** This is the core terrain data container. Each chunk is a `MeshInstance3D` that holds a heightmap, 5 color/mask maps (ground×2, wall×2, grass), dirty flags for incremental updates, and cached geometry per cell. The struct has two sets of data: *runtime* `Vec`s (fast indexed access during generation) and *persisted* `PackedArray`s (Godot's serialization format for scene files).

**File:** `rust/src/chunk.rs` (append after the `TerrainConfig` block)

```rust
/// Per-chunk mesh instance that holds heightmap data and generates geometry.
/// Port of Yugen's MarchingSquaresTerrainChunk (MeshInstance3D).
#[derive(GodotClass)]
#[class(base=MeshInstance3D, init, tool)]
pub struct PixyTerrainChunk {
    base: Base<MeshInstance3D>,

    /// Chunk coordinates in the terrain grid.
    #[export]
    pub chunk_coords: Vector2i,

    /// Merge mode index (mirrors terrain setting, stored per-chunk for serialization).
    #[export]
    #[init(val = 1)]
    pub merge_mode: i32,

    // ═══════════════════════════════════════════
    // Persisted Terrain Data (Godot PackedArrays for scene serialization)
    // ═══════════════════════════════════════════
    /// Flat height data for serialization: row-major, dim_z rows of dim_x values.
    #[export]
    #[init(val = PackedFloat32Array::new())]
    pub saved_height_map: PackedFloat32Array,

    /// Persisted ground color channel 0.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_color_map_0: PackedColorArray,

    /// Persisted ground color channel 1.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_color_map_1: PackedColorArray,

    /// Persisted wall color channel 0.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_wall_color_map_0: PackedColorArray,

    /// Persisted wall color channel 1.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_wall_color_map_1: PackedColorArray,

    /// Persisted grass mask map.
    #[export]
    #[init(val = PackedColorArray::new())]
    pub saved_grass_mask_map: PackedColorArray,

    // ═══════════════════════════════════════════
    // Runtime Terrain Data Maps (working copies)
    // ═══════════════════════════════════════════
    /// 2D height array: height_map[z][x] = f32
    pub height_map: Vec<Vec<f32>>,

    /// Ground vertex color channel 0 (flat array: z * dim_x + x).
    pub color_map_0: Vec<Color>,

    /// Ground vertex color channel 1.
    pub color_map_1: Vec<Color>,

    /// Wall vertex color channel 0.
    pub wall_color_map_0: Vec<Color>,

    /// Wall vertex color channel 1.
    pub wall_color_map_1: Vec<Color>,

    /// Grass mask per vertex (R=mask, G=ridge flag).
    pub grass_mask_map: Vec<Color>,

    /// Dirty flags per cell: needs_update[z][x] = bool.
    pub needs_update: Vec<Vec<bool>>,

    /// Cached geometry per cell for incremental updates.
    pub cell_geometry: HashMap<[i32; 2], CellGeometry>,

    /// Whether to use higher-poly floors (4 triangles vs 2).
    #[init(val = true)]
    pub higher_poly_floors: bool,

    /// Whether this chunk was just created (affects initial color assignment).
    pub is_new_chunk: bool,

    /// Skip saving data to packed arrays on exit_tree (e.g., during undo operations).
    #[export]
    #[init(val = false)]
    pub skip_save_on_exit: bool,

    /// Cached terrain configuration (set once at initialization, avoids needing to bind terrain).
    terrain_config: TerrainConfig,

    /// Terrain material reference for mesh regeneration (stored so regenerate_mesh() can use it).
    terrain_material: Option<Gd<ShaderMaterial>>,

    /// Grass planter child node.
    grass_planter: Option<Gd<PixyGrassPlanter>>,
}
```

**What's happening:**

The struct has three layers:

1. **Exported fields** (`#[export]`): `chunk_coords`, `merge_mode`, all `saved_*` arrays, `skip_save_on_exit`. These are visible in the Godot inspector and saved to `.tscn` files. The packed arrays are the *only* way terrain data persists between editor sessions.

2. **Runtime working data** (no `#[export]`): `height_map`, color maps, `needs_update`, `cell_geometry`. These are populated from packed arrays on load or generated fresh for new chunks. Using `Vec<Vec<f32>>` for the heightmap (2D access: `height_map[z][x]`) and flat `Vec<Color>` for color maps (1D access: `z * dim_x + x`) — the different indexing schemes match how each is accessed during generation.

3. **Private state**: `terrain_config`, `terrain_material`, `grass_planter`. These hold references to parent-level data that's passed in once during initialization.

Key design decision — **dual storage**:
- Runtime `Vec`s are cheap to index and mutate during brush operations (hundreds of edits per stroke).
- Godot `PackedArray`s are needed for scene serialization but expensive to index randomly.
- The two are kept in sync: `sync_to_packed()` flattens Vecs → packed on save; `restore_from_packed()` expands packed → Vecs on load.

### Step 4: Create the custom constructor

**Why:** Godot calls the default `init()` constructor (from `#[class(init)]`), but we also need a way to create chunks with a specific `Base<MeshInstance3D>`. The `from_init_fn` pattern in gdext lets the terrain create chunks with: `Gd::<PixyTerrainChunk>::from_init_fn(PixyTerrainChunk::new_with_base)`.

**File:** `rust/src/chunk.rs` (append after the struct definition)

```rust
impl PixyTerrainChunk {
    pub fn new_with_base(base: Base<MeshInstance3D>) -> Self {
        Self {
            base,
            chunk_coords: Vector2i::ZERO,
            merge_mode: 1,
            saved_height_map: PackedFloat32Array::new(),
            saved_color_map_0: PackedColorArray::new(),
            saved_color_map_1: PackedColorArray::new(),
            saved_wall_color_map_0: PackedColorArray::new(),
            saved_wall_color_map_1: PackedColorArray::new(),
            saved_grass_mask_map: PackedColorArray::new(),
            height_map: Vec::new(),
            color_map_0: Vec::new(),
            color_map_1: Vec::new(),
            wall_color_map_0: Vec::new(),
            wall_color_map_1: Vec::new(),
            grass_mask_map: Vec::new(),
            needs_update: Vec::new(),
            cell_geometry: HashMap::new(),
            higher_poly_floors: true,
            is_new_chunk: false,
            skip_save_on_exit: false,
            terrain_config: TerrainConfig::default(),
            terrain_material: None,
            grass_planter: None,
        }
    }
}
```

**What's happening:**
- Every field must be explicitly initialized — Rust has no concept of "default class members" like GDScript does.
- `from_init_fn` passes the base object as an argument, so you get a properly wired-up Godot node. The alternative `Gd::<T>::default()` uses the `init` attribute, but `from_init_fn` gives you a custom constructor that still receives the base.
- All Vec fields start empty — they're populated later by `initialize_terrain()`.

### Step 5: Implement `exit_tree` for persistence

**Why:** When the Godot editor saves a scene, each node's `exit_tree()` is called. This is our hook to sync runtime Vec data back to the exported PackedArrays that Godot actually serializes. Without this, terrain edits would be lost every time you save and reload.

**File:** `rust/src/chunk.rs` (append)

```rust
#[godot_api]
impl IMeshInstance3D for PixyTerrainChunk {
    fn exit_tree(&mut self) {
        // Sync runtime data to packed arrays before leaving tree (scene save)
        if !self.skip_save_on_exit {
            self.sync_to_packed();
        }
    }
}
```

**What's happening:**
- `IMeshInstance3D` is the virtual method trait for MeshInstance3D nodes. `exit_tree()` fires when the node leaves the scene tree.
- `skip_save_on_exit` is a safety valve used during undo operations — when the editor is undoing a chunk addition, we don't want to overwrite the packed arrays with potentially stale data.
- `sync_to_packed()` is defined in Step 9. It converts all runtime Vecs to their PackedArray counterparts.

### Step 6: Add the data access API

**Why:** The editor plugin needs to read and write terrain data through well-defined methods. Each `draw_*` method updates a map value and marks surrounding cells as dirty (needing mesh regeneration). Each `get_*` method reads a map value with bounds checking.

**File:** `rust/src/chunk.rs` (append)

```rust
#[godot_api]
impl PixyTerrainChunk {
    /// Regenerate all cells (mark all dirty and rebuild mesh).
    /// Stub: mesh generation implemented in Part 07.
    #[func]
    pub fn regenerate_all_cells(&mut self) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        for z in 0..(dim_z - 1) {
            for x in 0..(dim_x - 1) {
                self.needs_update[z as usize][x as usize] = true;
            }
        }
        self.regenerate_mesh();
    }

    /// Get height at a grid point.
    #[func]
    pub fn get_height(&self, coords: Vector2i) -> f32 {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if coords.x < 0 || coords.y < 0 || coords.x >= dim_x || coords.y >= dim_z {
            return 0.0;
        }
        self.height_map[coords.y as usize][coords.x as usize]
    }

    /// Draw (set) height at a grid point and mark surrounding cells dirty.
    #[func]
    pub fn draw_height(&mut self, x: i32, z: i32, y: f32) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.height_map[z as usize][x as usize] = y;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw ground color channel 0 at a grid point.
    #[func]
    pub fn draw_color_0(&mut self, x: i32, z: i32, color: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.color_map_0[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw ground color channel 1 at a grid point.
    #[func]
    pub fn draw_color_1(&mut self, x: i32, z: i32, color: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.color_map_1[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw wall color channel 0.
    #[func]
    pub fn draw_wall_color_0(&mut self, x: i32, z: i32, color: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.wall_color_map_0[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw wall color channel 1.
    #[func]
    pub fn draw_wall_color_1(&mut self, x: i32, z: i32, color: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.wall_color_map_1[(z * dim_x + x) as usize] = color;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Draw grass mask.
    #[func]
    pub fn draw_grass_mask(&mut self, x: i32, z: i32, masked: Color) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return;
        }
        self.grass_mask_map[(z * dim_x + x) as usize] = masked;
        self.notify_needs_update(z, x);
        self.notify_needs_update(z, x - 1);
        self.notify_needs_update(z - 1, x);
        self.notify_needs_update(z - 1, x - 1);
    }

    /// Get ground color channel 0 at a grid point.
    #[func]
    pub fn get_color_0(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.color_map_0[(z * dim_x + x) as usize]
    }

    /// Get ground color channel 1 at a grid point.
    #[func]
    pub fn get_color_1(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.color_map_1[(z * dim_x + x) as usize]
    }

    /// Get wall color channel 0 at a grid point.
    #[func]
    pub fn get_wall_color_0(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.wall_color_map_0[(z * dim_x + x) as usize]
    }

    /// Get wall color channel 1 at a grid point.
    #[func]
    pub fn get_wall_color_1(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.wall_color_map_1[(z * dim_x + x) as usize]
    }

    /// Get grass mask at a grid point.
    #[func]
    pub fn get_grass_mask_at(&self, x: i32, z: i32) -> Color {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        if x < 0 || z < 0 || x >= dim_x || z >= dim_z {
            return Color::default();
        }
        self.grass_mask_map[(z * dim_x + x) as usize]
    }
}
```

**What's happening:**

Every `draw_*` method follows the same pattern:
1. **Bounds check** — if the coordinate is out of range, silently return. No panic, no error. This is important because brush strokes near chunk edges will try to draw at coordinates that wrap to adjacent chunks.
2. **Write the value** — update the appropriate map at the flat index `z * dim_x + x`.
3. **Mark 4 cells dirty** — the modified grid point is shared by up to 4 cells: `(z, x)`, `(z, x-1)`, `(z-1, x)`, `(z-1, x-1)`. All must be regenerated because their geometry depends on this grid point.

The `draw_height()` method is different: the heightmap uses 2D indexing (`height_map[z][x]`) instead of flat indexing. This is because height generation needs row-level access patterns.

Each `draw_*` call does NOT trigger mesh regeneration. The editor plugin batches many draws into a single stroke, then calls `regenerate_mesh()` once at the end. This is critical for performance — a brush size of 10 can touch 100+ grid points per frame.

### Step 7: Add private configuration accessors

**Why:** Several internal methods need dimensions, cell size, etc. Instead of accessing `self.terrain_config.dimensions` everywhere, thin accessor methods keep the code readable and make it easy to change the backing store later.

**File:** `rust/src/chunk.rs` (append after the `#[godot_api]` block)

```rust
impl PixyTerrainChunk {
    /// Set terrain configuration (called by terrain when adding/initializing chunk).
    /// This caches all needed terrain data so we don't need to bind terrain later.
    pub fn set_terrain_config(&mut self, config: TerrainConfig) {
        self.terrain_config = config;
    }

    /// Get the cached terrain config (for external callers).
    #[allow(dead_code)]
    pub fn get_terrain_config(&self) -> &TerrainConfig {
        &self.terrain_config
    }

    /// Get dimensions from cached config.
    fn get_dimensions_xz(&self) -> (i32, i32) {
        (
            self.terrain_config.dimensions.x,
            self.terrain_config.dimensions.z,
        )
    }

    fn get_terrain_dimensions(&self) -> Vector3i {
        self.terrain_config.dimensions
    }

    fn get_cell_size(&self) -> Vector2 {
        self.terrain_config.cell_size
    }

    fn get_blend_mode(&self) -> i32 {
        self.terrain_config.blend_mode
    }

    fn get_use_ridge_texture(&self) -> bool {
        self.terrain_config.use_ridge_texture
    }

    fn get_ridge_threshold(&self) -> f32 {
        self.terrain_config.ridge_threshold
    }

    /// Get height at a specific grid coordinate, returning None if out of bounds.
    pub fn get_height_at(&self, x: i32, z: i32) -> Option<f32> {
        let z_idx = z as usize;
        let x_idx = x as usize;
        self.height_map
            .get(z_idx)
            .and_then(|row| row.get(x_idx))
            .copied()
    }

    /// Set height at a specific grid coordinate.
    /// Guards against NaN/Inf values to prevent mesh corruption.
    pub fn set_height_at(&mut self, x: i32, z: i32, h: f32) {
        let z_idx = z as usize;
        let x_idx = x as usize;
        if z_idx < self.height_map.len() && x_idx < self.height_map[z_idx].len() {
            // Guard against NaN/Inf - use 0.0 as fallback
            let safe_h = if h.is_finite() {
                h
            } else {
                godot_warn!("NaN/Inf height detected at ({}, {}), using 0.0", x, z);
                0.0
            };
            self.height_map[z_idx][x_idx] = safe_h;
        }
    }
}
```

**What's happening:**
- `get_dimensions_xz()` returns `(i32, i32)` for the common pattern `let (dim_x, dim_z) = self.get_dimensions_xz()` — used in nearly every method.
- `get_height_at()` returns `Option<f32>` (safe, for external callers like terrain edge-copying), while `get_height()` in the `#[func]` block returns `f32` with a 0.0 default (Godot-facing API can't use Option).
- `set_height_at()` includes a **NaN guard**. This is the second line of defense against mesh corruption (the first is in `add_point()` from Part 03). A single NaN height propagates through all geometry calculations and produces invisible triangles.

### Step 8: Add the dirty flag system

**Why:** When height or color changes, we need to mark cells as needing update so the mesh regenerator knows what to rebuild. A grid point change affects up to 4 cells, and each of those cells' 8 neighbors also need updating because marching squares geometry depends on edge connectivity.

**File:** `rust/src/chunk.rs` (append to the same `impl` block from Step 7)

```rust
    /// Mark a cell as needing update.
    /// Also marks adjacent cells (8 neighbors) because their geometry depends on edge connectivity.
    pub fn notify_needs_update(&mut self, z: i32, x: i32) {
        self.mark_cell_needs_update(x, z);
    }

    /// Mark a cell and its 8 neighbors as needing update.
    /// Adjacent cells must be invalidated because marching squares geometry depends on
    /// edge connectivity with neighbors - a height change in one cell affects how
    /// adjacent cells render their shared edges.
    fn mark_cell_needs_update(&mut self, x: i32, z: i32) {
        let (dim_x, dim_z) = self.get_dimensions_xz();
        for dz in -1..=1 {
            for dx in -1..=1 {
                let nx = x + dx;
                let nz = z + dz;
                if nx >= 0 && nx < dim_x - 1 && nz >= 0 && nz < dim_z - 1 {
                    self.needs_update[nz as usize][nx as usize] = true;
                    // Also invalidate cached geometry for this cell
                    self.cell_geometry.remove(&[nx, nz]);
                }
            }
        }
    }
```

**What's happening:**
- `notify_needs_update(z, x)` delegates to `mark_cell_needs_update(x, z)` — note the argument order swap. The public API uses `(z, x)` to match the `draw_*` methods' calling convention, while the internal method uses `(x, z)` to match the `cell_geometry` key format `[x, z]`.
- The 3×3 neighbor loop (`-1..=1` in both axes) marks up to 9 cells dirty. This is aggressive but correct — marching squares case selection for a cell depends on the heights of its 4 corners, and those corners are shared with neighbors.
- `cell_geometry.remove()` evicts the cached geometry for that cell. This forces `generate_terrain_cells()` (Part 07) to regenerate it from scratch instead of replaying stale cached data.

### Step 9: Implement packed array persistence

**Why:** Godot saves scenes by serializing exported properties. Our heightmap lives in `Vec<Vec<f32>>` at runtime, but Godot can only serialize `PackedFloat32Array`. We need bidirectional conversion: Vec → PackedArray for saving, PackedArray → Vec for loading.

**File:** `rust/src/chunk.rs` (continue in the same `impl` block)

```rust
    // ═══════════════════════════════════════════
    // Data Persistence: Packed Array Conversion
    // ═══════════════════════════════════════════

    /// Sync runtime data to packed arrays for scene serialization.
    pub fn sync_to_packed(&mut self) {
        let dim = self.get_terrain_dimensions();
        let dim_x = dim.x as usize;
        let dim_z = dim.z as usize;
        let expected_total = dim_x * dim_z;

        // Validate height map dimensions before packing
        if !self.height_map.is_empty() {
            if self.height_map.len() != dim_z {
                godot_warn!(
                    "PixyTerrainChunk: sync_to_packed height_map row count mismatch: {} vs expected {}",
                    self.height_map.len(),
                    dim_z
                );
                return;
            }
            for (z, row) in self.height_map.iter().enumerate() {
                if row.len() != dim_x {
                    godot_warn!(
                        "PixyTerrainChunk: sync_to_packed height_map[{}] col count mismatch: {} vs expected {}",
                        z,
                        row.len(),
                        dim_x
                    );
                    return;
                }
            }
        }

        // Validate color map lengths before packing
        if !self.color_map_0.is_empty() && self.color_map_0.len() != expected_total {
            godot_warn!(
                "PixyTerrainChunk: sync_to_packed color_map_0 size mismatch: {} vs expected {}",
                self.color_map_0.len(),
                expected_total
            );
            return;
        }

        // Height map: flatten 2D → 1D (row-major: z * dim_x + x)
        if !self.height_map.is_empty() {
            let mut packed = PackedFloat32Array::new();
            packed.resize(dim_x * dim_z);
            for z in 0..dim_z {
                for x in 0..dim_x {
                    packed[z * dim_x + x] = self.height_map[z][x];
                }
            }
            self.saved_height_map = packed;
        }

        // Color maps
        self.saved_color_map_0 = Self::vec_color_to_packed(&self.color_map_0);
        self.saved_color_map_1 = Self::vec_color_to_packed(&self.color_map_1);
        self.saved_wall_color_map_0 = Self::vec_color_to_packed(&self.wall_color_map_0);
        self.saved_wall_color_map_1 = Self::vec_color_to_packed(&self.wall_color_map_1);
        self.saved_grass_mask_map = Self::vec_color_to_packed(&self.grass_mask_map);
    }

    /// Restore runtime data from packed arrays (after scene load).
    /// Returns true if data was restored, false if packed arrays were empty.
    fn restore_from_packed(&mut self) -> bool {
        let dim = self.get_terrain_dimensions();
        let dim_x = dim.x as usize;
        let dim_z = dim.z as usize;
        let expected_total = dim_x * dim_z;

        // Check if height data was saved
        if self.saved_height_map.len() != expected_total {
            return false;
        }

        // Restore height map from flat packed array → 2D Vec
        self.height_map = Vec::with_capacity(dim_z);
        for z in 0..dim_z {
            let mut row = Vec::with_capacity(dim_x);
            for x in 0..dim_x {
                row.push(self.saved_height_map[z * dim_x + x]);
            }
            self.height_map.push(row);
        }

        // Restore color maps
        self.color_map_0 = Self::packed_to_vec_color(&self.saved_color_map_0, expected_total);
        self.color_map_1 = Self::packed_to_vec_color(&self.saved_color_map_1, expected_total);
        self.wall_color_map_0 =
            Self::packed_to_vec_color(&self.saved_wall_color_map_0, expected_total);
        self.wall_color_map_1 =
            Self::packed_to_vec_color(&self.saved_wall_color_map_1, expected_total);
        self.grass_mask_map = Self::packed_to_vec_color(&self.saved_grass_mask_map, expected_total);

        godot_print!(
            "PixyTerrainChunk: Restored data from saved arrays for chunk ({}, {})",
            self.chunk_coords.x,
            self.chunk_coords.y
        );
        true
    }

    /// Convert Vec<Color> to PackedColorArray.
    fn vec_color_to_packed(colors: &[Color]) -> PackedColorArray {
        let mut packed = PackedColorArray::new();
        packed.resize(colors.len());
        for (i, color) in colors.iter().enumerate() {
            packed[i] = *color;
        }
        packed
    }

    /// Convert PackedColorArray to Vec<Color>, with fallback if wrong size.
    fn packed_to_vec_color(packed: &PackedColorArray, expected: usize) -> Vec<Color> {
        if packed.len() == expected {
            (0..expected).map(|i| packed[i]).collect()
        } else {
            vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); expected]
        }
    }
```

**What's happening:**

**`sync_to_packed()`** converts runtime data → Godot packed arrays:
- Height map: flattens the 2D `Vec<Vec<f32>>` to a 1D `PackedFloat32Array` using row-major order (`z * dim_x + x`). This layout matches how `restore_from_packed()` reads it back.
- Color maps: each `Vec<Color>` maps directly to `PackedColorArray` (already flat, same indexing).
- Validation gates prevent corrupt data from being saved — if the height map has wrong dimensions, bail out entirely and keep the previous packed data.

**`restore_from_packed()`** does the reverse:
- Reads the flat `PackedFloat32Array` back into a 2D Vec, row by row.
- Returns `false` if the packed array has wrong length (new chunk with no saved data) — the caller then generates fresh data instead.

**`packed_to_vec_color()`** has a fallback: if the packed array is wrong size, it returns a fresh Vec filled with `Color::from_rgba(1.0, 0.0, 0.0, 0.0)`. This is "texture slot 0" in the color encoding — the dominant red channel selects texture 0 (first pair's first slot). This ensures new or corrupted chunks default to the base ground texture.

Key API note: `PackedFloat32Array` and `PackedColorArray` index with `usize`, not `i64`. In gdext, the index operator is Rust-native (zero-based usize), even though the Godot API uses int.

### Step 10: Add data generation methods

**Why:** New chunks need initial data — a heightmap (optionally seeded by noise), default color maps (texture slot 0), and a grass mask (all enabled). These are called during `initialize_terrain()` when no packed array data exists to restore.

**File:** `rust/src/chunk.rs` (continue in the same `impl` block)

```rust
    // ═══════════════════════════════════════════
    // Initialization
    // ═══════════════════════════════════════════

    /// Initialize terrain data (called by terrain parent after adding chunk to tree).
    /// All needed data is passed as parameters to avoid needing to bind terrain.
    pub fn initialize_terrain(
        &mut self,
        should_regenerate_mesh: bool,
        noise: Option<Gd<Noise>>,
        terrain_material: Option<Gd<ShaderMaterial>>,
        grass_config: GrassConfig,
    ) {
        if !Engine::singleton().is_editor_hint() {
            godot_error!(
                "PixyTerrainChunk: Trying to initialize terrain during runtime (NOT SUPPORTED)"
            );
            return;
        }

        // Store terrain material reference for use by regenerate_mesh()
        self.terrain_material = terrain_material.clone();

        let dim = self.get_terrain_dimensions();

        // Initialize needs_update grid
        self.needs_update = Vec::with_capacity((dim.z - 1) as usize);
        for _ in 0..(dim.z - 1) {
            self.needs_update.push(vec![true; (dim.x - 1) as usize]);
        }

        // Try to restore from saved packed arrays first (scene reload)
        let restored = self.restore_from_packed();

        if !restored {
            // Generate fresh data maps
            if self.height_map.is_empty() {
                self.generate_height_map_with_noise(noise);
            }
            if self.color_map_0.is_empty() || self.color_map_1.is_empty() {
                self.generate_color_maps();
            }
            if self.wall_color_map_0.is_empty() || self.wall_color_map_1.is_empty() {
                self.generate_wall_color_maps();
            }
            if self.grass_mask_map.is_empty() {
                self.generate_grass_mask_map();
            }
        }

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

        if should_regenerate_mesh && self.base().get_mesh().is_none() {
            self.regenerate_mesh_with_material(terrain_material);
        }
    }

    /// Generate a flat height map, optionally seeded by noise (passed as parameter).
    pub fn generate_height_map_with_noise(&mut self, noise: Option<Gd<Noise>>) {
        let dim = self.get_terrain_dimensions();
        let dim_x = dim.x as usize;
        let dim_z = dim.z as usize;

        self.height_map = vec![vec![0.0; dim_x]; dim_z];

        if let Some(noise) = noise {
            for z in 0..dim_z {
                for x in 0..dim_x {
                    let noise_x = (self.chunk_coords.x * (dim.x - 1)) + x as i32;
                    let noise_z = (self.chunk_coords.y * (dim.z - 1)) + z as i32;
                    let sample = noise.get_noise_2d(noise_x as f32, noise_z as f32);
                    self.height_map[z][x] = sample * dim.y as f32;
                }
            }
        }
    }

    /// Generate default ground color maps (texture slot 0).
    pub fn generate_color_maps(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        self.color_map_0 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
        self.color_map_1 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
    }

    /// Generate default wall color maps (texture slot 0).
    pub fn generate_wall_color_maps(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        self.wall_color_map_0 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
        self.wall_color_map_1 = vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total];
    }

    /// Generate default grass mask map (all enabled).
    pub fn generate_grass_mask_map(&mut self) {
        let dim = self.get_terrain_dimensions();
        let total = (dim.x * dim.z) as usize;
        self.grass_mask_map = vec![Color::from_rgba(1.0, 1.0, 1.0, 1.0); total];
    }
```

**What's happening:**

**`initialize_terrain()`** is the main entry point for setting up a chunk. The control flow:

1. Guard: only runs in editor mode (terrain is an editor tool, not a runtime game system).
2. Store the terrain material for later mesh generation.
3. Initialize the `needs_update` grid with all cells marked dirty (first generation).
4. Try `restore_from_packed()` — if the scene was saved, packed arrays have data.
5. If restore fails (new chunk, no saved data): generate fresh maps from defaults/noise.
6. Look for an existing `GrassPlanter` child (scene reload reuses persisted nodes).
7. If no planter found: create a new one with `PixyGrassPlanter::new_alloc()`, add it as a child, set owner for scene persistence.
8. If mesh generation is requested and no mesh exists: generate it.

**`generate_height_map_with_noise()`**:
- Starts flat (all zeros).
- If noise is provided, samples it using world-space coordinates: `chunk_coords * (dim - 1) + local_coord`. The `dim - 1` offset ensures adjacent chunks sample adjacent noise regions seamlessly.
- `noise.get_noise_2d()` takes `f32` — a gdext API specificity. Godot's built-in noise in GDScript takes float, but gdext binds it as `f32`.

**Default color maps**:
- `Color::from_rgba(1.0, 0.0, 0.0, 0.0)` — this is the *texture slot 0* encoding. Recall from Part 03: the dominant channel in the color determines the texture index. Red=1.0, everything else=0.0 → dominant channel is R → first pair → first slot → texture index 0.
- Both color_map_0 and color_map_1 get the same default because the shader blends between them. When both point to texture 0, there's no blend visible.

### Step 11: Add mesh generation stubs

**Why:** The `initialize_terrain()` method and `regenerate_all_cells()` call `regenerate_mesh()` and `regenerate_mesh_with_material()`. These are fully implemented in Part 07, but we need stubs now so the code compiles.

**File:** `rust/src/chunk.rs` (continue in the same `impl` block)

```rust
    /// Rebuild the mesh using SurfaceTool with CUSTOM0-2 format.
    /// Uses the stored terrain_material reference (set during initialize_terrain).
    /// Stub: full implementation in Part 07.
    pub fn regenerate_mesh(&mut self) {
        let material = self.terrain_material.clone();
        self.regenerate_mesh_with_material(material);
    }

    /// Rebuild the mesh using SurfaceTool with CUSTOM0-2 format.
    /// Material is passed as parameter to avoid needing to bind terrain.
    /// Stub: full implementation in Part 07.
    pub fn regenerate_mesh_with_material(&mut self, _terrain_material: Option<Gd<ShaderMaterial>>) {
        // Stub: mesh generation implemented in Part 07
        godot_print!(
            "PixyTerrainChunk: regenerate_mesh stub for chunk ({}, {})",
            self.chunk_coords.x,
            self.chunk_coords.y
        );
    }
}
```

**What's happening:**
- `regenerate_mesh()` is the convenience wrapper: it reads the stored material reference (set during `initialize_terrain()`) and delegates.
- `regenerate_mesh_with_material()` is the real entry point — it takes the material as a parameter so the terrain can pass it explicitly during first initialization (before it's stored on the chunk).
- Both are stubs that print a log message. Part 07 replaces them with the full SurfaceTool pipeline.

## Verify

At this point, `cargo build` should succeed. The chunk module compiles with:
- Full data model and persistence
- All draw/get methods for the editor API
- Data generation from noise or defaults
- GrassPlanter stub creation
- Mesh generation stubs (Part 07 fills these in)

The other module stubs (`terrain.rs`, `editor_plugin.rs`, `gizmo.rs`, `quick_paint.rs`, `texture_preset.rs`) are still empty comments from Part 01 — they compile fine as empty modules since nothing imports from them yet.

## What You Learned

- **Dual storage pattern**: Runtime Vec (fast random access for editing) + exported PackedArray (Godot serialization). Sync on save, restore on load.
- **Borrow cycle breaking**: `TerrainConfig` captures terrain settings by value so chunks don't need to hold references to their parent terrain node.
- **Dirty flag propagation**: Changing one grid point marks up to 9 cells dirty (3×3 neighborhood) because marching squares geometry depends on shared corners and edges.
- **NaN defense**: Height setters guard against non-finite values — a single NaN corrupts all downstream geometry calculations.
- **Color encoding as data**: `Color::from_rgba(1.0, 0.0, 0.0, 0.0)` isn't a visual color — it's a data encoding that selects texture slot 0 in the shader's 16-texture blending system.
- **Editor-only initialization**: `Engine::singleton().is_editor_hint()` gates terrain setup — this is a tool-time system, not a game runtime system.

## Stubs Introduced

- [ ] `PixyGrassPlanter::regenerate_all_cells_with_geometry()` — no-op, implemented in Part 12
- [ ] `PixyTerrainChunk::regenerate_mesh()` — prints stub message, implemented in Part 07
- [ ] `PixyTerrainChunk::regenerate_mesh_with_material()` — prints stub message, implemented in Part 07

## Stubs Resolved

- [x] `chunk` module (empty) — introduced in Part 01, now has full data model
- [x] `grass_planter` module (empty) — introduced in Part 01, now has `GrassConfig` struct and `PixyGrassPlanter` stub class
