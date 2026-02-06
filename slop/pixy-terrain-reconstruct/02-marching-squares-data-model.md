# Pixy Terrain — Part 02: Marching Squares Data Model

**Series:** Reconstructing Pixy Terrain
**Part:** 02 of 18
**Previous:** 2026-02-06-project-scaffolding-01.md
**Status:** Complete

## What We're Building

The data structures that underpin the entire terrain system: `MergeMode` (how aggressively corners merge into slopes), `CellGeometry` (cached vertex data per grid cell), and `CellContext` (the per-cell state bag that every geometry function reads from). Plus the helper functions for color math.

## What You'll Have After This

A compiling project with the complete `MergeMode` enum, `CellContext` struct, `CellGeometry` struct, and helper functions (`lerp_color`, `get_dominant_color`, texture index encoding). No geometry generation yet — that's Parts 03-05.

## Prerequisites

- Part 01 completed (project compiles)

## Steps

### Step 1: Define MergeMode

**Why:** The core design decision in Yugen's system is the "merge threshold" — how much height difference between two adjacent grid corners before a wall is created between them. Low threshold = walls everywhere (blocky Minecraft look). High threshold = smooth slopes. This enum encodes 5 presets that the user picks from a dropdown.

**File:** `rust/src/marching_squares.rs`

Replace the placeholder comment with:

```rust
use godot::prelude::*;

/// Merge mode determines the height threshold before walls are created between corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    Cubic,
    Polyhedron,
    RoundedPolyhedron,
    SemiRound,
    Spherical,
}

#[allow(dead_code)]
impl MergeMode {
    pub fn threshold(self) -> f32 {
        match self {
            MergeMode::Cubic => 0.6,
            MergeMode::Polyhedron => 1.3,
            MergeMode::RoundedPolyhedron => 2.1,
            MergeMode::SemiRound => 5.0,
            MergeMode::Spherical => 20.0,
        }
    }

    pub fn from_index(idx: i32) -> Self {
        match idx {
            0 => MergeMode::Cubic,
            1 => MergeMode::Polyhedron,
            2 => MergeMode::RoundedPolyhedron,
            3 => MergeMode::SemiRound,
            4 => MergeMode::Spherical,
            _ => MergeMode::Polyhedron,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            MergeMode::Cubic => 0,
            MergeMode::Polyhedron => 1,
            MergeMode::RoundedPolyhedron => 2,
            MergeMode::SemiRound => 3,
            MergeMode::Spherical => 4,
        }
    }

    pub fn is_round(self) -> bool {
        matches!(self, MergeMode::SemiRound | MergeMode::Spherical)
    }
}
```

**What's happening:**
- `Cubic` (0.6) creates walls even for tiny height differences — very blocky, Minecraft-like terrain.
- `Spherical` (20.0) almost never creates walls — everything smoothly blends, good for rolling hills.
- `Polyhedron` (1.3) is the default and most versatile — creates clean walls for deliberate height changes while allowing gentle slopes.
- `from_index` maps from Godot's `OptionButton` integer selection to the enum. The default fallback is `Polyhedron`.
- `is_round()` is used later by the grass shader to adjust blade rendering on curved surfaces.

### Step 2: Define CellGeometry

**Why:** Every grid cell's geometry (vertices, UVs, colors) needs to be stored for two reasons: (1) SurfaceTool replay — when rebuilding a mesh, unchanged cells reuse cached geometry instead of regenerating. (2) The grass planter reads floor triangle positions to place grass blades.

**File:** `rust/src/marching_squares.rs` (append after MergeMode)

```rust
/// Sensitivity factor for blend edge detection vs merge threshold.
/// < 1.0 = more aggressive wall detection, > 1.0 = less aggressive / more slope blend.
pub const BLEND_EDGE_SENSITIVITY: f32 = 1.25;

/// Stored geometry for a single cell, used for caching and SurfaceTool replay.
#[derive(Debug, Clone, Default)]
pub struct CellGeometry {
    pub verts: Vec<Vector3>,
    pub uvs: Vec<Vector2>,
    pub uv2s: Vec<Vector2>,
    pub colors_0: Vec<Color>,
    pub colors_1: Vec<Color>,
    pub grass_mask: Vec<Color>,
    pub mat_blend: Vec<Color>,
    pub is_floor: Vec<bool>,
}
```

**What's happening:**
- Every field is a parallel array — `verts[i]`, `uvs[i]`, `colors_0[i]` all describe the same vertex. This is the SurfaceTool data model (per-vertex attributes).
- `colors_0` / `colors_1`: The 16-texture system encodes two RGBA colors per vertex. The dominant channel in each color selects one of 4 possibilities, giving 4×4 = 16 texture combinations. `colors_0` goes in `COLOR`, `colors_1` goes in `CUSTOM0`.
- `grass_mask`: Per-vertex grass control. R channel = mask (0 = no grass), G channel = ridge flag.
- `mat_blend`: `CUSTOM2` — encodes the 3-texture blend weights for smooth transitions between materials at cell boundaries.
- `is_floor`: Distinguishes floor triangles (grass grows here) from wall triangles (no grass, different smooth groups).
- `#[derive(Default)]` gives us `CellGeometry::default()` with empty Vecs — important for the "generate then cache" pattern.

### Step 3: Define CellContext

**Why:** `CellContext` is the "god object" of the marching squares algorithm. Every geometry function (`add_point`, `add_outer_corner`, etc.) takes `&mut CellContext` instead of 15+ individual parameters. It holds corner heights, edge connectivity, rotation state, color maps, and per-cell boundary detection results. In GDScript this was class-level mutable state; in Rust we make it an explicit struct to satisfy the borrow checker.

**File:** `rust/src/marching_squares.rs` (append after CellGeometry)

```rust
/// Context for cell generation: corner heights, edge connectivity, rotation state, and dimensions.
pub struct CellContext {
    /// Corner heights: [A(top-left), B(top-right), C(bottom-left), D(bottom-right)]
    pub heights: [f32; 4],
    /// Edge connectivity: [AB(top), BD(right), CD(bottom), AC(left)]
    pub edges: [bool; 4],
    /// Current rotation (0-3, counter-clockwise)
    pub rotation: usize,
    /// Cell coordinates in chunk-local space
    pub cell_coords: Vector2i,
    /// Chunk dimensions (from terrain)
    pub dimensions: Vector3i,
    /// Cell size in world units (XZ)
    pub cell_size: Vector2,
    /// Merge threshold from merge mode
    pub merge_threshold: f32,
    /// Whether to use higher-poly floors (4 triangles instead of 2)
    pub higher_poly_floors: bool,

    // Color state — set per cell before generation
    pub color_map_0: Vec<Color>,
    pub color_map_1: Vec<Color>,
    pub wall_color_map_0: Vec<Color>,
    pub wall_color_map_1: Vec<Color>,
    pub grass_mask_map: Vec<Color>,

    // Cell boundary detection
    pub cell_min_height: f32,
    pub cell_max_height: f32,
    pub cell_is_boundary: bool,

    // Floor/wall boundary colors
    pub cell_floor_lower_color_0: Color,
    pub cell_floor_upper_color_0: Color,
    pub cell_floor_lower_color_1: Color,
    pub cell_floor_upper_color_1: Color,
    pub cell_wall_lower_color_0: Color,
    pub cell_wall_upper_color_0: Color,
    pub cell_wall_lower_color_1: Color,
    pub cell_wall_upper_color_1: Color,

    // Per-cell dominant materials (3-texture system)
    pub cell_mat_a: i32,
    pub cell_mat_b: i32,
    pub cell_mat_c: i32,

    // Blend mode from terrain system
    pub blend_mode: i32,
    pub use_ridge_texture: bool,
    pub ridge_threshold: f32,

    // Whether this is a new (freshly created) chunk
    pub is_new_chunk: bool,

    // Floor mode toggle: true = floor geometry, false = wall geometry
    pub floor_mode: bool,

    // Blend thresholds
    pub lower_thresh: f32,
    pub upper_thresh: f32,

    // Chunk world position for wall UV2 offset
    pub chunk_position: Vector3,
}
```

**What's happening:**

The height/edge layout deserves special attention:

```
  A ──AB── B       heights[0]=A  heights[1]=B
  |        |       heights[2]=D  heights[3]=C
  AC       BD
  |        |       edges[0]=AB  edges[1]=BD
  C ──CD── D       edges[2]=CD  edges[3]=AC
```

- `heights` stores corner heights in the order `[A, B, D, C]` — **not** `[A, B, C, D]`. This matches Yugen's clockwise rotation convention where rotating by 1 shifts `A→B→D→C→A`.
- `edges` are booleans: `true` means the two corners are "merged" (height difference < threshold), so no wall is needed between them.
- `rotation` (0-3) enables the key trick: instead of writing 17 cases × 4 orientations = 68 code paths, we write 17 cases and rotate the cell to match. The rotation transforms which physical corner maps to the logical A/B/C/D positions.
- Color maps are `Vec<Color>` — they're owned by the context (moved in with `std::mem::take()`) to avoid borrow checker issues with the chunk struct. They get moved back after the cell loop.
- `cell_is_boundary` is true when the cell has significant height variation — these cells need gradient color blending between upper and lower corners.
- `floor_mode` toggles between floor geometry (grass grows, smooth group 0) and wall geometry (no grass, smooth group MAX).

### Step 4: Add CellContext methods

**Why:** The rotation system is the cleverest part of the algorithm. Instead of `ctx.heights[0]`, you call `ctx.ay()` which returns `heights[rotation]`. After `rotate_cell(1)`, `ay()` returns what was previously `by()`. This lets all 17 geometry cases assume A is the "interesting" corner, and the caller just rotates to make that true.

**File:** `rust/src/marching_squares.rs` (append after CellContext struct)

```rust
impl CellContext {
    /// Rotated corner height A (top-left after rotation)
    pub fn ay(&self) -> f32 {
        self.heights[self.rotation]
    }
    /// Rotated corner height B (top-right after rotation)
    pub fn by(&self) -> f32 {
        self.heights[(self.rotation + 1) % 4]
    }
    /// Rotated corner height D (bottom-right after rotation)
    pub fn dy(&self) -> f32 {
        self.heights[(self.rotation + 2) % 4]
    }
    /// Rotated corner height C (bottom-left after rotation)
    pub fn cy(&self) -> f32 {
        self.heights[(self.rotation + 3) % 4]
    }

    /// Rotated edge AB (top edge after rotation)
    pub fn ab(&self) -> bool {
        self.edges[self.rotation]
    }
    /// Rotated edge BD (right edge after rotation)
    pub fn bd(&self) -> bool {
        self.edges[(self.rotation + 1) % 4]
    }
    /// Rotated edge CD (bottom edge after rotation)
    pub fn cd(&self) -> bool {
        self.edges[(self.rotation + 2) % 4]
    }
    /// Rotated edge AC (left edge after rotation)
    pub fn ac(&self) -> bool {
        self.edges[(self.rotation + 3) % 4]
    }

    /// Rotate the cell by `rotations` steps clockwise.
    pub fn rotate_cell(&mut self, rotations: i32) {
        self.rotation = ((self.rotation as i32 + 4 + rotations) % 4) as usize;
    }

    /// True if a is higher than b and outside merge distance.
    pub fn is_higher(&self, a: f32, b: f32) -> bool {
        a - b > self.merge_threshold
    }

    /// True if a is lower than b and outside merge distance.
    pub fn is_lower(&self, a: f32, b: f32) -> bool {
        a - b < -self.merge_threshold
    }

    /// True if a and b are within merge distance.
    pub fn is_merged(&self, a: f32, b: f32) -> bool {
        (a - b).abs() < self.merge_threshold
    }

    pub fn start_floor(&mut self) {
        self.floor_mode = true;
    }

    pub fn start_wall(&mut self) {
        self.floor_mode = false;
    }

    /// Color map index for a given (x, z) position in the grid.
    #[allow(dead_code)]
    fn color_idx(&self, x: i32, z: i32) -> usize {
        (z * self.dimensions.x + x) as usize
    }
}
```

**What's happening:**
- The `ay/by/cy/dy` accessors and `ab/bd/cd/ac` accessors both use `(self.rotation + offset) % 4` — the same modular arithmetic gives rotation-aware access to both heights and edges.
- `rotate_cell(-1)` rotates counter-clockwise (the `+ 4` prevents negative modulo issues).
- `is_higher`, `is_lower`, `is_merged` are the three-way comparison that drives case selection in `generate_cell()`. Two corners are "merged" if their height difference is less than the threshold — meaning they should be connected by a slope, not separated by a wall.
- `start_floor()` / `start_wall()` toggle `floor_mode`, which affects how `add_point()` handles UVs, smooth groups, and color selection.

### Step 5: Add helper functions

**Why:** Color interpolation and texture encoding are used everywhere — by `add_point()`, by the grass planter, and by the editor's paint tools. The texture encoding system is particularly clever: 16 textures are encoded as two RGBA colors where the dominant channel (R/G/B/A) in each color selects one of 4 options, giving 4×4 = 16 combinations.

**File:** `rust/src/marching_squares.rs` (append at the end)

```rust
// ── Helper functions ──

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// Returns the dominant channel as a one-hot color (argmax of RGBA).
pub fn get_dominant_color(c: Color) -> Color {
    let mut max_val = c.r;
    let mut idx = 0;

    if c.g > max_val {
        max_val = c.g;
        idx = 1;
    }
    if c.b > max_val {
        max_val = c.b;
        idx = 2;
    }
    if c.a > max_val {
        idx = 3;
    }

    match idx {
        0 => Color::from_rgba(1.0, 0.0, 0.0, 0.0),
        1 => Color::from_rgba(0.0, 1.0, 0.0, 0.0),
        2 => Color::from_rgba(0.0, 0.0, 1.0, 0.0),
        3 => Color::from_rgba(0.0, 0.0, 0.0, 1.0),
        _ => Color::from_rgba(1.0, 0.0, 0.0, 0.0),
    }
}

/// Convert vertex color pair to texture index (0-15).
pub fn get_texture_index_from_colors(c0: Color, c1: Color) -> i32 {
    let c0_idx = {
        let mut idx = 0;
        let mut max = c0.r;
        if c0.g > max {
            max = c0.g;
            idx = 1;
        }
        if c0.b > max {
            max = c0.b;
            idx = 2;
        }
        if c0.a > max {
            idx = 3;
        }
        idx
    };
    let c1_idx = {
        let mut idx = 0;
        let mut max = c1.r;
        if c1.g > max {
            max = c1.g;
            idx = 1;
        }
        if c1.b > max {
            max = c1.b;
            idx = 2;
        }
        if c1.a > max {
            idx = 3;
        }
        idx
    };
    c0_idx * 4 + c1_idx
}

/// Convert texture index (0-15) back to color pair.
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

**What's happening:**

The texture encoding system is worth studying:

```
Color0 channel (R/G/B/A) = row (0-3)
Color1 channel (R/G/B/A) = column (0-3)
Texture index = row * 4 + column = 0..15

Example: Texture 5 → row=1(G), col=1(G)
  color_0 = (0, 1, 0, 0)  ← green dominant
  color_1 = (0, 1, 0, 0)  ← green dominant
```

This is a GPU-friendly encoding — the shader can read the texture index directly from vertex colors without any lookup tables. `get_dominant_color()` snaps interpolated colors back to one-hot form (only one channel = 1.0, rest = 0.0), preventing blurry texture boundaries.

`lerp_color()` is deliberately not `pub` — it's only used internally by `add_point()` for bilinear color interpolation across cell corners. The `get_dominant_color()` call after interpolation quantizes the result.

## Verify

```bash
cd rust && cargo build
```

**Expected:** Compiles successfully. You'll see warnings about unused imports and dead code — that's expected since nothing calls these types yet.

```bash
cd rust && cargo test
```

**Expected:** No tests yet (those come in Part 05), but `cargo test` should succeed with "0 tests".

## What You Learned

- **Rotation trick**: Writing 17 cases instead of 68 by rotating the cell so the "interesting" corner is always at position A
- **Height array order**: `[A, B, D, C]` not `[A, B, C, D]` — the clockwise order makes rotation a simple index shift
- **Three-way height comparison**: `is_higher`, `is_lower`, `is_merged` — the merge threshold creates a dead zone where corners are considered "same height"
- **16-texture color encoding**: Two RGBA vertex colors → 4×4 = 16 texture slots, decoded on GPU with zero overhead
- **CellContext as borrow-checker solution**: Moving color maps into the context struct (with `std::mem::take()`) avoids fighting the borrow checker in the hot loop

## Stubs Introduced

(No new stubs — this part only adds types and functions)

## Stubs Resolved

- [x] `rust/src/marching_squares.rs` — was empty stub from Part 01 (partially resolved; geometry functions come in Parts 03-05)
