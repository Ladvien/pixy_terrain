# Pixy Terrain — Part 05: Module Split & The 17-Case Cell Generator

**Series:** Reconstructing Pixy Terrain
**Part:** 05 of 18
**Previous:** 04-floor-wall-geometry-primitives.md
**Status:** Complete

## What We're Building

Two things in one part:

1. **Module split** — Converting the monolithic `marching_squares.rs` into a `marching_squares/` directory with focused sub-files. After Part 04, the single file holds ~1200 lines of types, structs, helpers, color math, and 5 geometry primitives. Adding ~500 more lines of case logic on top would make it unnavigable.

2. **The case generator** — `generate_cell()`, the dispatcher that examines a cell's 4 corner heights, determines which of 17+ geometry cases applies, and calls the right combination of primitives. Each case is extracted into a named function so the dispatcher reads as a table of conditions, not a 400-line if/else waterfall.

## What You'll Have After This

The complete marching squares algorithm. Given any 4 corner heights and a merge threshold, `generate_cell()` produces watertight geometry. The `marching_squares/` module is feature-complete with clean file boundaries.

```
rust/src/marching_squares/
├── mod.rs              (~15 lines)   — module declarations + public re-exports
├── types.rs            (~200 lines)  — constants, MergeMode, BlendMode, ColorChannel, TextureIndex, CellGeometry
├── cell_context.rs     (~350 lines)  — CellColorState, CellContext, all impl blocks
├── vertex.rs           (~250 lines)  — ColorSampleParams, color helpers, compute_vertex_color, add_point
├── primitives.rs       (~300 lines)  — add_full_floor, add_outer_corner, add_edge, add_inner_corner, add_diagonal_floor
└── cases.rs            (~600 lines)  — generate_cell dispatcher, match_case, 20+ named case functions, tests
```

## Prerequisites

- Part 04 completed (all 5 geometry primitives in `marching_squares.rs`)

## Steps

### Step 1: Convert to a module directory

**Why:** Rust resolves `mod marching_squares;` identically whether it finds `marching_squares.rs` or `marching_squares/mod.rs`. Converting to a directory lets us split into focused files without changing any external imports in `lib.rs` or future `chunk.rs`.

```bash
mkdir -p rust/src/marching_squares
mv rust/src/marching_squares.rs rust/src/marching_squares/mod.rs
```

Verify: `cargo build` should still compile — nothing changed except the file location.

### Step 2: Extract `types.rs` — stable, self-contained types

**Why:** `MergeMode`, `BlendMode`, `ColorChannel`, `TextureIndex`, `CellGeometry`, and the named constants are stable definitions that rarely change. They depend only on `godot::prelude::*`. Isolating them means you can read the type system without scrolling past 1000+ lines of geometry code.

**File:** `rust/src/marching_squares/types.rs`

Move the following items from `mod.rs` into `types.rs`:
- All constants (`BLEND_EDGE_SENSITIVITY` through `WALL_BLEND_SENTINEL`)
- `MergeMode` enum + impl
- `BlendMode` enum
- `ColorChannel` enum + impl
- `TextureIndex` struct + impl
- `CellGeometry` struct

The only import needed at the top:
```rust
use godot::prelude::*;
```

**In `mod.rs`**, replace those items with:
```rust
mod types;
pub use types::*;
```

Verify: `cargo build` compiles.

### Step 3: Extract `cell_context.rs` — the context struct and `CellColorState`

**Why:** `CellContext` has ~30+ fields serving three distinct roles: grid state (heights, edges, rotation), color state (boundary colors, material indices), and output configuration (blend mode, thresholds). Extracting a `CellColorState` sub-struct groups the per-cell computed color fields so they can be reset with a single call instead of 11 individual assignments.

**File:** `rust/src/marching_squares/cell_context.rs`

**Step 3a:** Create a `CellColorState` sub-struct for per-cell computed color data:

```rust
use godot::prelude::*;
use super::types::*;

/// Pre-computed color state for the current cell.
/// Calculated once per cell in `generate_cell`, consumed by `add_point`.
#[derive(Clone, Debug, Default)]
pub struct CellColorState {
    // Cell boundary detection
    pub min_height: f32,
    pub max_height: f32,
    pub is_boundary: bool,

    // Boundary colors (floor)
    pub floor_lower_color_0: Color,
    pub floor_upper_color_0: Color,
    pub floor_lower_color_1: Color,
    pub floor_upper_color_1: Color,

    // Boundary colors (wall)
    pub wall_lower_color_0: Color,
    pub wall_upper_color_0: Color,
    pub wall_lower_color_1: Color,
    pub wall_upper_color_1: Color,

    // Per-cell dominant materials (3-texture system)
    pub material_a: TextureIndex,
    pub material_b: TextureIndex,
    pub material_c: TextureIndex,
}

impl CellColorState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
```

**Step 3b:** Update `CellContext` to embed it. Move the full `CellContext` struct and all its methods from `mod.rs` into this file. The struct becomes:

```rust
#[derive(Clone, Debug, Default)]
pub struct CellContext {
    // ── Grid state (read by generate_cell + primitives) ──
    pub heights: [f32; 4],
    pub edges: [bool; 4],
    pub rotation: usize,
    pub cell_coords: Vector2i,
    pub dimensions: Vector3i,
    pub cell_size: Vector2,
    pub merge_threshold: f32,
    pub higher_poly_floors: bool,

    // ── Color maps (owned, moved in/out with std::mem::take) ──
    pub color_map_0: Vec<Color>,
    pub color_map_1: Vec<Color>,
    pub wall_color_map_0: Vec<Color>,
    pub wall_color_map_1: Vec<Color>,
    pub grass_mask_map: Vec<Color>,

    // ── Per-cell computed color state ──
    pub color_state: CellColorState,

    // ── Output configuration ──
    pub blend_mode: BlendMode,
    pub use_ridge_texture: bool,
    pub ridge_threshold: f32,
    pub is_new_chunk: bool,
    pub floor_mode: bool,
    pub lower_threshold: f32,
    pub upper_threshold: f32,
    pub chunk_position: Vector3,
}
```

Move all `CellContext` methods into this file: `ay`, `by`, `cy`, `dy`, `ab`, `bd`, `cd`, `ac`, `rotate_cell`, `is_higher`, `is_lower`, `is_merged`, `start_floor`, `start_wall`, `color_index`, `corner_indices`, `calculate_boundary_colors`, `calculate_cell_material_pair`, `calculate_material_blend_data`.

**Migration:** Every method body that previously read `self.cell_min_height`, `self.cell_material_a`, `self.cell_is_boundary`, etc. now reads `self.color_state.min_height`, `self.color_state.material_a`, `self.color_state.is_boundary`. This is a mechanical find-and-replace — approximately 30 sites. The `cell_` prefix is dropped because the struct name already provides that context.

**In `mod.rs`**, replace the moved items with:
```rust
mod cell_context;
pub use cell_context::*;
```

Verify: `cargo build` compiles.

### Step 4: Extract `vertex.rs` — the vertex factory

**Why:** `add_point` and its helpers form a self-contained subsystem: take a position in cell-local coords, apply rotation, compute color from 4 sampling paths, encode material blend, push the vertex. When debugging a color blending issue, you look at one ~250 line file.

**File:** `rust/src/marching_squares/vertex.rs`

Move from `mod.rs`:
- `ColorSampleParams` struct
- `lerp_color()`
- `get_dominant_color()`
- `sanitize_float()`
- `preserve_high_channels()`
- `compute_vertex_color()`
- `push_vertex()`
- `add_point()`

Imports needed:
```rust
use godot::prelude::*;
use super::types::*;
use super::cell_context::*;
```

**Update `compute_vertex_color` and `calculate_material_blend_data`** to access boundary colors and material indices through `ctx.color_state.*` instead of the old flat fields.

**In `mod.rs`**:
```rust
mod vertex;
pub use vertex::*;
```

Verify: `cargo build` compiles.

### Step 5: Extract `primitives.rs` — the 5 geometry building blocks

**Why:** The primitives are stable, well-tested, and independently understandable. Each is a pure function of `CellContext` + `CellGeometry` that calls `add_point`. Grouping them in one file makes the complete set of building blocks visible at a glance.

**File:** `rust/src/marching_squares/primitives.rs`

Move from `mod.rs`:
- `add_full_floor()`
- `add_outer_corner()`
- `add_edge()`
- `add_inner_corner()`
- `add_diagonal_floor()`

Imports needed:
```rust
use super::vertex::add_point;
use super::cell_context::CellContext;
use super::types::CellGeometry;
```

**In `mod.rs`**:
```rust
mod primitives;
pub use primitives::*;
```

Verify: `cargo build` compiles.

### Step 6: Clean up `mod.rs`

After all extractions, `mod.rs` should be approximately:

```rust
// Marching squares terrain algorithm — implemented in Parts 02-05
mod types;
mod cell_context;
mod vertex;
mod primitives;
mod cases;

// Re-export the public API
pub use types::*;
pub use cell_context::*;
pub use vertex::*;
pub use primitives::*;
pub use cases::generate_cell;
```

This file is the module's table of contents. Anyone reading it sees the full architecture in 12 lines.

**Note:** `mod cases;` won't compile yet — we create that file in the next step.

### Step 7: Create `cases.rs` — the dispatcher with extracted case functions

**Why:** This is the heart of the marching squares algorithm. The design has two layers:

1. **`generate_cell()`** — sets up edge connectivity and color state, then enters the rotation loop
2. **`match_case()`** — a pure-read function (`&CellContext`, not `&mut`) that tests predicates and returns `Option<fn(&mut CellContext, &mut CellGeometry)>` — a function pointer to the matched case handler

Each case body is extracted into a named function. The dispatcher reads as a table of conditions; the geometry lives in focused, documented functions you can find by name.

**File:** `rust/src/marching_squares/cases.rs` (new file)

#### The dispatcher: `generate_cell` + `match_case` + `validate_geometry`

```rust
use godot::prelude::*;

use super::cell_context::CellContext;
use super::primitives::*;
use super::types::CellGeometry;
use super::vertex::add_point;

/// Generate geometry for a single cell based on the 17-case marching squares algorithm.
///
/// Algorithm:
/// 1. Calculate edge connectivity (which adjacent corners are within merge threshold)
/// 2. Compute per-cell color state (boundary detection, material pair)
/// 3. Fast-path Case 0 (all edges connected → full floor, most common)
/// 4. Try all 4 rotations against the case table — first match wins
/// 5. Fallback to full floor if no case matches (prevents mesh holes)
/// 6. Validate triangle count
pub fn generate_cell(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let initial_vert_count = geo.verts.len();

    let ay = ctx.ay();
    let by = ctx.by();
    let cy = ctx.cy();
    let dy = ctx.dy();

    // Calculate edge connectivity: true = slope (merged), false = wall (separated)
    ctx.edges = [
        (ay - by).abs() < ctx.merge_threshold, // AB (top)
        (by - dy).abs() < ctx.merge_threshold, // BD (right)
        (cy - dy).abs() < ctx.merge_threshold, // CD (bottom)
        (ay - cy).abs() < ctx.merge_threshold, // AC (left)
    ];

    // Pre-compute cell color state for boundary detection
    ctx.color_state.min_height = ay.min(by).min(cy).min(dy);
    ctx.color_state.max_height = ay.max(by).max(cy).max(dy);
    ctx.color_state.is_boundary =
        (ctx.color_state.max_height - ctx.color_state.min_height) > ctx.merge_threshold;

    ctx.calculate_cell_material_pair();
    if ctx.color_state.is_boundary {
        ctx.calculate_boundary_colors();
    }

    // Case 0: all edges connected → full floor (fast path, most common case)
    if ctx.ab() && ctx.bd() && ctx.cd() && ctx.ac() {
        add_full_floor(ctx, geo);
        validate_geometry(ctx, geo, initial_vert_count);
        return;
    }

    // Try all 4 rotations to find a matching case
    let matched = 'rotation: {
        for rotation in 0..4 {
            ctx.rotation = rotation;
            if let Some(case_fn) = match_case(ctx) {
                case_fn(ctx, geo);
                break 'rotation true;
            }
        }
        false
    };

    if !matched {
        // Fallback: unknown configuration, fill with floor to prevent mesh holes
        ctx.rotation = 0;
        add_full_floor(ctx, geo);
    }

    validate_geometry(ctx, geo, initial_vert_count);
}

/// Match the current rotation against all cases.
///
/// Returns a function pointer to the case handler, or None if no case matches
/// at this rotation. Takes `&CellContext` (immutable) — predicates are pure reads.
/// Mutation (rotation, start_floor/start_wall, add_point) happens inside the
/// case functions, not here.
fn match_case(ctx: &CellContext) -> Option<fn(&mut CellContext, &mut CellGeometry)> {
    let (ay, by, cy, dy) = (ctx.ay(), ctx.by(), ctx.cy(), ctx.dy());

    // ── Single-feature cases ──

    // Case 1: A raised, opposite corners merged
    if ctx.is_higher(ay, by) && ctx.is_higher(ay, cy) && ctx.bd() && ctx.cd() {
        return Some(case_1_outer_corner);
    }

    // Case 2: AB edge raised above CD
    if ctx.is_higher(ay, cy) && ctx.is_higher(by, dy) && ctx.ab() && ctx.cd() {
        return Some(case_2_edge);
    }

    // ── Edge + corner composites ──

    // Case 3: AB edge with A outer corner above
    if ctx.is_higher(ay, by) && ctx.is_higher(ay, cy) && ctx.is_higher(by, dy) && ctx.cd() {
        return Some(case_3_edge_a_outer_corner);
    }

    // Case 4: AB edge with B outer corner above
    if ctx.is_higher(by, ay) && ctx.is_higher(ay, cy) && ctx.is_higher(by, dy) && ctx.cd() {
        return Some(case_4_edge_b_outer_corner);
    }

    // ── Diagonal / double-inner cases ──

    // Case 5: B and C raised, A and D lowered, BC merged
    if ctx.is_lower(ay, by)
        && ctx.is_lower(ay, cy)
        && ctx.is_lower(dy, by)
        && ctx.is_lower(dy, cy)
        && ctx.is_merged(by, cy)
    {
        return Some(case_5_double_inner_corner);
    }

    // Case 5.5: B and C raised, A and D lowered, B higher than C
    if ctx.is_lower(ay, by)
        && ctx.is_lower(ay, cy)
        && ctx.is_lower(dy, by)
        && ctx.is_lower(dy, cy)
        && ctx.is_higher(by, cy)
    {
        return Some(case_5_5_double_inner_with_outer);
    }

    // ── Inner corner cases ──

    // Case 6: A is the lowest corner, BCD merged
    if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.bd() && ctx.cd() {
        return Some(case_6_inner_corner);
    }

    // Case 7: A lowest, BD connected, C higher than D (Yugen Case 8)
    if ctx.is_lower(ay, by)
        && ctx.is_lower(ay, cy)
        && ctx.bd()
        && !ctx.cd()
        && ctx.is_higher(cy, dy)
    {
        return Some(case_7_inner_asymmetric_bd);
    }

    // Case 8: A lowest, CD connected, B higher than D (Yugen Case 9)
    if ctx.is_lower(ay, by)
        && ctx.is_lower(ay, cy)
        && !ctx.bd()
        && ctx.cd()
        && ctx.is_higher(by, dy)
    {
        return Some(case_8_inner_asymmetric_cd);
    }

    // Case 9: A lowest, neither BD nor CD connected, BC merged
    if ctx.is_lower(ay, by)
        && ctx.is_lower(ay, cy)
        && !ctx.bd()
        && !ctx.cd()
        && ctx.is_higher(by, dy)
        && ctx.is_higher(cy, dy)
        && ctx.is_merged(by, cy)
    {
        return Some(case_9_inner_diagonal_outer);
    }

    // ── Inner corner + edge composites ──

    // Case 10: Inner corner at A with edge atop BD
    if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.is_higher(dy, cy) && ctx.bd() {
        return Some(case_10_inner_corner_edge_bd);
    }

    // Case 11: Inner corner at A with edge atop CD
    if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.is_higher(dy, by) && ctx.cd() {
        return Some(case_11_inner_corner_edge_cd);
    }

    // ── Spiral / staircase cases (all 4 corners at different heights) ──

    // Case 12: Clockwise spiral A<B<D<C
    if ctx.is_lower(ay, by)
        && ctx.is_lower(by, dy)
        && ctx.is_lower(dy, cy)
        && ctx.is_higher(cy, ay)
    {
        return Some(case_12_spiral_clockwise);
    }

    // Case 13: Counter-clockwise spiral A<C<D<B
    if ctx.is_lower(ay, cy)
        && ctx.is_lower(cy, dy)
        && ctx.is_lower(dy, by)
        && ctx.is_higher(by, ay)
    {
        return Some(case_13_spiral_counter);
    }

    // Case 14: Staircase A<B<C<D
    if ctx.is_lower(ay, by) && ctx.is_lower(by, cy) && ctx.is_lower(cy, dy) {
        return Some(case_14_staircase_abcd);
    }

    // Case 15: Staircase A<C<B<D
    if ctx.is_lower(ay, cy) && ctx.is_lower(cy, by) && ctx.is_lower(by, dy) {
        return Some(case_15_staircase_acbd);
    }

    // ── Degenerate / merged edge variants ──

    // Case 16: A higher than C, but AB merged and CD merged (degenerates to edge)
    if ctx.is_higher(ay, cy)
        && ctx.is_merged(ay, by)
        && ctx.is_merged(cy, dy)
        && ctx.ab()
        && ctx.cd()
    {
        return Some(case_2_edge); // Same geometry as Case 2
    }

    // ── Outer corner composites ──

    // Case 17: A highest, D lowest, all corners different
    if ctx.is_higher(ay, by)
        && ctx.is_higher(ay, cy)
        && !ctx.bd()
        && !ctx.cd()
        && ctx.is_lower(dy, by)
        && ctx.is_lower(dy, cy)
    {
        return Some(case_17_outer_diagonal_inner);
    }

    // Case 18: A highest, BC merged, D lowest
    if ctx.is_higher(ay, by)
        && ctx.is_higher(ay, cy)
        && ctx.is_merged(by, cy)
        && ctx.is_higher(by, dy)
        && ctx.is_higher(cy, dy)
    {
        return Some(case_18_outer_diagonal_outer);
    }

    // Case 19: A higher, B higher than C, CD not connected
    if ctx.is_higher(ay, by)
        && ctx.is_higher(ay, cy)
        && ctx.is_higher(by, cy)
        && !ctx.cd()
    {
        return Some(case_19_outer_partial_edge_b);
    }

    // Case 20: A higher, C higher than B, BD not connected
    if ctx.is_higher(ay, by)
        && ctx.is_higher(ay, cy)
        && ctx.is_higher(cy, by)
        && !ctx.bd()
    {
        return Some(case_20_outer_partial_edge_c);
    }

    // Case 21: A higher, BC merged, D lowest
    if ctx.is_higher(ay, by)
        && ctx.is_merged(by, cy)
        && !ctx.bd()
        && ctx.is_lower(dy, by)
    {
        return Some(case_21_outer_inner_composite);
    }

    // ── Single-wall cases (3 of 4 edges connected) ──

    // Case 22: All edges except AC, A higher than C
    if ctx.ab() && ctx.bd() && ctx.cd() && !ctx.ac() && ctx.is_higher(ay, cy) {
        return Some(case_22_single_wall_ac);
    }

    // Case 23: All edges except BD, B higher than D
    if ctx.ab() && ctx.ac() && ctx.cd() && !ctx.bd() && ctx.is_higher(by, dy) {
        return Some(case_23_single_wall_bd);
    }

    None
}

fn validate_geometry(ctx: &CellContext, geo: &CellGeometry, initial_vert_count: usize) {
    let added = geo.verts.len() - initial_vert_count;
    if added % 3 != 0 {
        godot_error!(
            "GEOMETRY BUG: Cell ({},{}) added {} vertices (not divisible by 3). \
             Heights: [{:.2}, {:.2}, {:.2}, {:.2}], Edges: [{}, {}, {}, {}]",
            ctx.cell_coords.x, ctx.cell_coords.y, added,
            ctx.heights[0], ctx.heights[1], ctx.heights[2], ctx.heights[3],
            ctx.edges[0], ctx.edges[1], ctx.edges[2], ctx.edges[3]
        );
    }
}
```

**What's happening — the algorithm structure:**

1. **Edge calculation**: Compare each pair of adjacent corners against the merge threshold. `true` = merged (slope), `false` = separated (wall).
2. **Color state setup**: Height range and material data are computed on `ctx.color_state`, the `CellColorState` sub-struct. `calculate_cell_material_pair()` and `calculate_boundary_colors()` are methods on `CellContext`.
3. **Case 0 fast path**: All edges connected → full floor. Most common case, checked before entering the rotation loop.
4. **Rotation loop**: For each of 4 rotations (0°, 90°, 180°, 270°), `match_case` tests predicates against the rotated heights. First match wins.
5. **`match_case` is immutable**: Takes `&CellContext`, not `&mut`. Predicates are pure reads. Mutation happens inside the case functions.
6. **Function pointers**: `match_case` returns `Option<fn(&mut CellContext, &mut CellGeometry)>`. This enforces a uniform signature on all case functions and makes the dispatcher a clean table.
7. **Fallback**: If no case matches, fall back to a full floor to prevent mesh holes.
8. **Validation**: Verify the vertex count is divisible by 3 (complete triangles).

#### Simple case functions (1–3 lines each)

These cases compose existing primitives directly:

```rust
// ── Single-feature cases ──

/// Case 1: One raised corner (outer corner).
/// A is higher than B and C; B-D and C-D are merged.
fn case_1_outer_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_outer_corner(ctx, geo, true, true, false, -1.0);
}

/// Case 2: Raised edge.
/// A-B side is higher than C-D side; both pairs merged.
fn case_2_edge(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_edge(ctx, geo, true, true, 0.0, 1.0);
}

/// Case 6: One lowered corner (inner corner).
/// A is lowest; B-D and C-D merged.
fn case_6_inner_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, true, false, false, false);
}
```

#### Edge + corner composites (Cases 3–4)

```rust
/// Case 3: Half-width edge with A outer corner above.
/// A > B, both above CD. Edge covers right half, outer corner covers A.
fn case_3_edge_a_outer_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_edge(ctx, geo, true, true, 0.5, 1.0);
    add_outer_corner(ctx, geo, false, true, true, by);
}

/// Case 4: Half-width edge with B outer corner above.
/// B > A, both above CD. Edge covers left half, rotate to place outer corner at B.
fn case_4_edge_b_outer_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let cy = ctx.cy();
    add_edge(ctx, geo, true, true, 0.0, 0.5);
    ctx.rotate_cell(1);
    add_outer_corner(ctx, geo, false, true, true, cy);
}
```

#### Diagonal / double-inner cases (Cases 5, 5.5, 9)

```rust
/// Case 5: B and C raised, A and D lowered, BC merged.
/// Two inner corners at A and D with a diagonal floor bridge.
fn case_5_double_inner_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    add_inner_corner(ctx, geo, true, false, false, false, false);
    add_diagonal_floor(ctx, geo, by, cy, true, true);
    ctx.rotate_cell(2);
    add_inner_corner(ctx, geo, true, false, false, false, false);
}

/// Case 5.5: Like Case 5 but B higher than C — adds outer corner at B.
fn case_5_5_double_inner_with_outer(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    add_inner_corner(ctx, geo, true, false, true, false, false);
    add_diagonal_floor(ctx, geo, by, cy, true, true);
    ctx.rotate_cell(2);
    add_inner_corner(ctx, geo, true, false, true, false, false);
    ctx.rotate_cell(-1);
    add_outer_corner(ctx, geo, false, true, false, -1.0);
}

/// Case 9: A lowest, BD/CD disconnected, BC merged.
/// Inner corner at A, diagonal floor, outer corner at D.
fn case_9_inner_diagonal_outer(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    add_inner_corner(ctx, geo, true, false, false, false, false);
    add_diagonal_floor(ctx, geo, by, cy, true, false);
    ctx.rotate_cell(2);
    add_outer_corner(ctx, geo, true, false, false, -1.0);
}
```

#### Inner corner + edge composites (Cases 10–11)

```rust
/// Case 10: Inner corner at A with edge atop BD.
fn case_10_inner_corner_edge_bd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, true, false);
    ctx.rotate_cell(1);
    add_edge(ctx, geo, false, true, 0.0, 1.0);
}

/// Case 11: Inner corner at A with edge atop CD.
fn case_11_inner_corner_edge_cd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, false, true);
    ctx.rotate_cell(2);
    add_edge(ctx, geo, false, true, 0.0, 1.0);
}
```

#### Spiral / staircase cases (Cases 12–15)

All 4 corners at different heights, producing inner corner + edge + outer corner:

```rust
/// Case 12: Clockwise spiral A<B<D<C.
fn case_12_spiral_clockwise(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let cy = ctx.cy();
    add_inner_corner(ctx, geo, true, false, true, false, true);
    ctx.rotate_cell(2);
    add_edge(ctx, geo, true, true, 0.0, 0.5);
    ctx.rotate_cell(1);
    add_outer_corner(ctx, geo, true, true, true, cy);
}

/// Case 13: Counter-clockwise spiral A<C<D<B.
fn case_13_spiral_counter(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_inner_corner(ctx, geo, true, false, true, true, false);
    ctx.rotate_cell(1);
    add_edge(ctx, geo, true, true, 0.5, 1.0);
    add_outer_corner(ctx, geo, true, true, true, by);
}

/// Case 14: Staircase A<B<C<D.
fn case_14_staircase_abcd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_inner_corner(ctx, geo, true, false, true, false, true);
    ctx.rotate_cell(2);
    add_edge(ctx, geo, true, true, 0.5, 1.0);
    add_outer_corner(ctx, geo, true, true, true, by);
}

/// Case 15: Staircase A<C<B<D.
fn case_15_staircase_acbd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let cy = ctx.cy();
    add_inner_corner(ctx, geo, true, false, true, true, false);
    ctx.rotate_cell(1);
    add_edge(ctx, geo, true, true, 0.0, 0.5);
    ctx.rotate_cell(1);
    add_outer_corner(ctx, geo, true, true, true, cy);
}
```

#### Outer corner composites (Cases 17–21)

```rust
/// Case 17: A highest, D lowest, all corners different.
/// Outer corner at A, diagonal floor, inner corner at D.
fn case_17_outer_diagonal_inner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    add_outer_corner(ctx, geo, false, true, false, -1.0);
    add_diagonal_floor(ctx, geo, by, cy, true, true);
    ctx.rotate_cell(2);
    add_inner_corner(ctx, geo, true, false, false, false, false);
}

/// Case 18: A highest, BC merged, D lowest.
/// Outer corner at A, diagonal floor, outer corner at D.
fn case_18_outer_diagonal_outer(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    add_outer_corner(ctx, geo, false, true, false, -1.0);
    add_diagonal_floor(ctx, geo, by, cy, false, false);
    ctx.rotate_cell(2);
    add_outer_corner(ctx, geo, true, false, false, -1.0);
}

/// Case 19: A higher, B higher than C, CD not connected.
/// Outer corner at A (flattened to B height), half-width edge below.
fn case_19_outer_partial_edge_b(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_outer_corner(ctx, geo, false, true, true, by);
    add_edge(ctx, geo, true, true, 0.5, 1.0);
}

/// Case 20: A higher, C higher than B, BD not connected.
/// Mirror of Case 19 — outer corner flattened to C, edge on opposite side.
fn case_20_outer_partial_edge_c(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let cy = ctx.cy();
    add_outer_corner(ctx, geo, false, true, true, cy);
    ctx.rotate_cell(-1);
    add_edge(ctx, geo, true, true, 0.0, 0.5);
}

/// Case 21: A raised, BC merged, D lowest.
/// Outer corner at A flattened to B, rotate 180°, inner corner at D.
fn case_21_outer_inner_composite(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_outer_corner(ctx, geo, false, true, true, by);
    ctx.rotate_cell(2);
    add_inner_corner(ctx, geo, true, true, true, false, false);
}
```

#### Asymmetric inner corner cases with inline geometry (Cases 7–8)

These cases don't decompose cleanly into the 5 standard primitives. The geometry is hand-built with `add_point` calls. Each function is self-contained with explaining variables.

```rust
/// Case 7: A lowest, BD connected (not CD), C higher than D.
/// Inner corner at A + custom floor/wall fan connecting B-D edge to raised C corner.
///
/// ```text
///   A (low)  ──  B (mid)
///   │             │
///   │  inner      │ BD connected
///   │  corner     │
///   C (high) ──  D (mid)   CD disconnected
/// ```
fn case_7_inner_asymmetric_bd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, false, false);

    let by = ctx.by();
    let dy = ctx.dy();
    let cy = ctx.cy();
    let bd_edge_midpoint = (by + dy) / 2.0;

    // Floor fan: 4 triangles bridging from B corner through BD midpoint to D corner
    ctx.start_floor();
    // D corner triangle
    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, dy, 1.0, 1.0, 0.0, false);
    add_point(ctx, geo, 1.0, bd_edge_midpoint, 0.5, 0.0, 0.0, false);
    // B corner triangle
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, bd_edge_midpoint, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, by, 0.0, 0.0, 1.0, false);
    // Center triangles (connecting B and D through midpoint)
    add_point(ctx, geo, 0.5, by, 0.0, 0.0, 1.0, false);
    add_point(ctx, geo, 1.0, bd_edge_midpoint, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, by, 0.5, 1.0, 1.0, false);

    add_point(ctx, geo, 0.5, dy, 1.0, 1.0, 0.0, false);
    add_point(ctx, geo, 0.0, by, 0.5, 1.0, 1.0, false);
    add_point(ctx, geo, 1.0, bd_edge_midpoint, 0.5, 0.0, 0.0, false);

    // Wall: 2 triangles rising from floor to C's height
    ctx.start_wall();
    add_point(ctx, geo, 0.0, by, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);

    add_point(ctx, geo, 0.5, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, dy, 1.0, 0.0, 0.0, false);

    // C upper floor: small triangle at the raised corner
    ctx.start_floor();
    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 0.5, cy, 1.0, 0.0, 1.0, false);
}

/// Case 8: A lowest, CD connected (not BD), B higher than D.
/// Mirror of Case 7 — inner corner at A + custom geometry connecting C-D edge to raised B.
///
/// ```text
///   A (low)  ──  B (high)
///   │             │
///   │  inner      │ BD disconnected
///   │  corner     │
///   C (mid)  ──  D (mid)   CD connected
/// ```
fn case_8_inner_asymmetric_cd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, false, false);

    let by = ctx.by();
    let dy = ctx.dy();
    let cy = ctx.cy();
    let cd_edge_midpoint = (dy + cy) / 2.0;

    // Floor fan: 4 triangles bridging from C corner through CD midpoint to D corner
    ctx.start_floor();
    // D corner triangle
    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, cd_edge_midpoint, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 0.5, 0.0, 0.0, false);
    // C corner triangle
    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, cd_edge_midpoint, 1.0, 0.0, 0.0, false);
    // Center triangles
    add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, cy, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, cd_edge_midpoint, 1.0, 0.0, 0.0, false);

    add_point(ctx, geo, 1.0, dy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, cd_edge_midpoint, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, cy, 0.0, 0.0, 0.0, false);

    // Wall: 2 triangles rising to B's height
    ctx.start_wall();
    add_point(ctx, geo, 0.5, cy, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 0.5, 0.0, 0.0, false);

    add_point(ctx, geo, 1.0, by, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, by, 0.0, 0.0, 0.0, false);

    // B upper floor: small triangle at the raised corner
    ctx.start_floor();
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, by, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, by, 0.0, 0.0, 0.0, false);
}
```

#### Single-wall cases with inline geometry (Cases 22–23)

Three of four edges connected, one wall across the cell. These average the connected edge heights for smooth wall placement.

```rust
/// Case 22: All edges connected except AC. A higher than C.
/// Upper floor at A-B height, single wall, lower floor at C-D height.
///
/// ```text
///   A ════════ B          upper floor (2 triangles)
///   ║          ║
///   ╠══════════╣  ← wall at z=0.5 (1 triangle, averaged heights)
///   ║          ║
///   C ──────── D          lower floor (2 triangles)
/// ```
fn case_22_single_wall_ac(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let ay = ctx.ay();
    let by = ctx.by();
    let cy = ctx.cy();
    let dy = ctx.dy();
    let upper_edge_height = (by + dy) / 2.0;
    let lower_edge_height = (by + dy) / 2.0;

    // Upper floor: 2 triangles at A-B height
    ctx.start_floor();
    add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, upper_edge_height, 0.5, 0.0, 0.0, false);

    add_point(ctx, geo, 1.0, upper_edge_height, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);

    // Wall: single triangle spanning the cell width
    ctx.start_wall();
    add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 1.0, lower_edge_height, 0.5, 1.0, 0.0, false);

    // Lower floor: 2 triangles at C-D height
    ctx.start_floor();
    add_point(ctx, geo, 0.0, cy, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 1.0, lower_edge_height, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);

    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, lower_edge_height, 0.5, 0.0, 0.0, false);
}

/// Case 23: All edges connected except BD. B higher than D.
/// Mirror of Case 22 — wall runs along the BD axis instead of AC.
fn case_23_single_wall_bd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let ay = ctx.ay();
    let by = ctx.by();
    let cy = ctx.cy();
    let dy = ctx.dy();
    let upper_edge_height = (ay + cy) / 2.0;
    let lower_edge_height = (ay + cy) / 2.0;

    // Upper floor: 2 triangles at A-B height
    ctx.start_floor();
    add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, upper_edge_height, 0.5, 0.0, 0.0, false);

    add_point(ctx, geo, 1.0, by, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 0.0, upper_edge_height, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);

    // Wall: single triangle
    ctx.start_wall();
    add_point(ctx, geo, 1.0, by, 0.5, 1.0, 1.0, false);
    add_point(ctx, geo, 1.0, dy, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 0.0, upper_edge_height, 0.5, 0.0, 0.0, false);

    // Lower floor: 2 triangles at C-D height
    ctx.start_floor();
    add_point(ctx, geo, 0.0, lower_edge_height, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);

    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, lower_edge_height, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
}
```

**Case guide (for reference):**

| Case | Pattern | Handler |
|------|---------|---------|
| 0 | All merged | `add_full_floor` (fast path, no rotation) |
| 1 | A raised, BCD merged | `case_1_outer_corner` |
| 2 | AB raised, CD merged | `case_2_edge` |
| 3 | A>B, AB raised over CD | `case_3_edge_a_outer_corner` |
| 4 | B>A, AB raised over CD | `case_4_edge_b_outer_corner` |
| 5 | BC raised, AD low, BC merged | `case_5_double_inner_corner` |
| 5.5 | BC raised, AD low, B>C | `case_5_5_double_inner_with_outer` |
| 6 | A lowered, BCD merged | `case_6_inner_corner` |
| 7 | A low, BD connected, C>D | `case_7_inner_asymmetric_bd` (inline geometry) |
| 8 | A low, CD connected, B>D | `case_8_inner_asymmetric_cd` (inline geometry) |
| 9 | A low, BC high+merged, D low | `case_9_inner_diagonal_outer` |
| 10 | Inner corner + edge (BD) | `case_10_inner_corner_edge_bd` |
| 11 | Inner corner + edge (CD) | `case_11_inner_corner_edge_cd` |
| 12 | Spiral A<B<D<C | `case_12_spiral_clockwise` |
| 13 | Spiral A<C<D<B | `case_13_spiral_counter` |
| 14 | Staircase A<B<C<D | `case_14_staircase_abcd` |
| 15 | Staircase A<C<B<D | `case_15_staircase_acbd` |
| 16 | Degenerate merged edge | `case_2_edge` (reused) |
| 17 | A highest, D lowest, all different | `case_17_outer_diagonal_inner` |
| 18 | A highest, BC merged, D lowest | `case_18_outer_diagonal_outer` |
| 19 | A>B>C, CD disconnected | `case_19_outer_partial_edge_b` |
| 20 | A>C>B, BD disconnected | `case_20_outer_partial_edge_c` |
| 21 | A raised, BC merged, D lowest | `case_21_outer_inner_composite` |
| 22 | 3/4 edges (missing AC) | `case_22_single_wall_ac` (inline geometry) |
| 23 | 3/4 edges (missing BD) | `case_23_single_wall_bd` (inline geometry) |

### Step 8: Add unit tests

**Why:** The marching squares algorithm is pure math — no Godot runtime needed for testing core logic. These tests verify rotation, edge detection, texture encoding, and floor generation.

**File:** `rust/src/marching_squares/cases.rs` (append at end of file)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::*;
    use super::super::cell_context::*;

    fn default_context(dim_x: i32, dim_z: i32) -> CellContext {
        let total = (dim_x * dim_z) as usize;
        CellContext {
            heights: [0.0; 4],
            edges: [true; 4],
            rotation: 0,
            cell_coords: Vector2i::new(0, 0),
            dimensions: Vector3i::new(dim_x, 32, dim_z),
            cell_size: Vector2::new(2.0, 2.0),
            merge_threshold: 1.3,
            higher_poly_floors: true,
            color_map_0: vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total],
            color_map_1: vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total],
            wall_color_map_0: vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total],
            wall_color_map_1: vec![Color::from_rgba(1.0, 0.0, 0.0, 0.0); total],
            grass_mask_map: vec![Color::from_rgba(1.0, 1.0, 1.0, 1.0); total],
            color_state: CellColorState {
                min_height: 0.0,
                max_height: 0.0,
                is_boundary: false,
                floor_lower_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                floor_upper_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                floor_lower_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                floor_upper_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                wall_lower_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                wall_upper_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                wall_lower_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                wall_upper_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
                material_a: TextureIndex(0),
                material_b: TextureIndex(0),
                material_c: TextureIndex(0),
            },
            blend_mode: BlendMode::Interpolated,
            use_ridge_texture: false,
            ridge_threshold: 1.0,
            is_new_chunk: false,
            floor_mode: true,
            lower_threshold: 0.3,
            upper_threshold: 0.7,
            chunk_position: Vector3::ZERO,
        }
    }

    #[test]
    fn test_merge_mode_thresholds() {
        assert_eq!(MergeMode::Cubic.threshold(), 0.6);
        assert_eq!(MergeMode::Polyhedron.threshold(), 1.3);
        assert_eq!(MergeMode::Spherical.threshold(), 20.0);
    }

    #[test]
    fn test_is_higher_lower_merged() {
        let ctx = default_context(3, 3);
        assert!(ctx.is_higher(5.0, 2.0));
        assert!(!ctx.is_higher(2.0, 5.0));
        assert!(ctx.is_lower(2.0, 5.0));
        assert!(!ctx.is_lower(5.0, 2.0));
        assert!(ctx.is_merged(5.0, 5.5));
        assert!(!ctx.is_merged(5.0, 10.0));
    }

    #[test]
    fn test_full_floor_higher_poly_generates_12_vertices() {
        let mut ctx = default_context(3, 3);
        ctx.heights = [0.0, 0.0, 0.0, 0.0];
        ctx.higher_poly_floors = true;
        let mut geo = CellGeometry::default();
        add_full_floor(&mut ctx, &mut geo);
        assert_eq!(geo.verts.len(), 12); // 4 triangles x 3 vertices
    }

    #[test]
    fn test_full_floor_low_poly_generates_6_vertices() {
        let mut ctx = default_context(3, 3);
        ctx.heights = [0.0, 0.0, 0.0, 0.0];
        ctx.higher_poly_floors = false;
        let mut geo = CellGeometry::default();
        add_full_floor(&mut ctx, &mut geo);
        assert_eq!(geo.verts.len(), 6); // 2 triangles x 3 vertices
    }

    #[test]
    fn test_texture_index_round_trip() {
        for idx in 0..16u8 {
            let (c0, c1) = TextureIndex(idx).to_color_pair();
            let result = TextureIndex::from_color_pair(c0, c1).0;
            assert_eq!(result, idx, "Round-trip failed for index {}", idx);
        }
    }

    #[test]
    fn test_get_dominant_color() {
        let c = get_dominant_color(Color::from_rgba(0.3, 0.8, 0.1, 0.2));
        assert_eq!(c.g, 1.0);
        assert_eq!(c.r, 0.0);
    }

    #[test]
    fn test_rotation() {
        let mut ctx = default_context(3, 3);
        ctx.heights = [1.0, 2.0, 3.0, 4.0]; // A, B, D, C
        ctx.rotation = 0;
        assert_eq!(ctx.ay(), 1.0);
        assert_eq!(ctx.by(), 2.0);
        assert_eq!(ctx.dy(), 3.0);
        assert_eq!(ctx.cy(), 4.0);

        ctx.rotate_cell(1);
        assert_eq!(ctx.ay(), 2.0); // was B
        assert_eq!(ctx.by(), 3.0); // was D
        assert_eq!(ctx.dy(), 4.0); // was C
        assert_eq!(ctx.cy(), 1.0); // was A
    }
}
```

**What's happening:**
- `default_context()` creates a test context with a 3×3 grid. `color_state` uses the `CellColorState` sub-struct with field names matching the extraction (no `cell_` prefix). Material indices use the `TextureIndex(0)` wrapper type.
- Tests verify: merge mode thresholds, height comparison helpers, vertex counts for both floor modes, texture encoding round-trip, dominant color detection, and rotation behavior.
- These tests use gdext's `Vector2i`, `Vector3i`, `Vector3`, and `Color` types, which work without a Godot runtime.

## Verify

```bash
cd rust && cargo test
```

**Expected:** All 7 tests pass:

```
running 7 tests
test marching_squares::cases::tests::test_merge_mode_thresholds ... ok
test marching_squares::cases::tests::test_is_higher_lower_merged ... ok
test marching_squares::cases::tests::test_full_floor_higher_poly_generates_12_vertices ... ok
test marching_squares::cases::tests::test_full_floor_low_poly_generates_6_vertices ... ok
test marching_squares::cases::tests::test_texture_index_round_trip ... ok
test marching_squares::cases::tests::test_get_dominant_color ... ok
test marching_squares::cases::tests::test_rotation ... ok
```

```bash
cd rust && cargo build
cd rust && cargo clippy
```

**Expected:** Compiles cleanly with no warnings. The `marching_squares/` module is feature-complete:

```
rust/src/marching_squares/
├── mod.rs              — module declarations + re-exports
├── types.rs            — constants, MergeMode, BlendMode, ColorChannel, TextureIndex, CellGeometry
├── cell_context.rs     — CellColorState, CellContext, all impl blocks
├── vertex.rs           — ColorSampleParams, color helpers, compute_vertex_color, push_vertex, add_point
├── primitives.rs       — add_full_floor, add_outer_corner, add_edge, add_inner_corner, add_diagonal_floor
└── cases.rs            — generate_cell, match_case, 20+ named case functions, tests
```

## What You Learned

- **Module directory conversion**: `mod marching_squares;` resolves identically to `marching_squares.rs` or `marching_squares/mod.rs`. Zero external import changes.
- **Sub-struct extraction**: `CellColorState` groups 14 per-cell computed fields so reset is `color_state.reset()` instead of 11 assignments. The `cell_` prefix drops because the struct name provides context.
- **Dispatcher as a match table**: `match_case` returns `Option<fn(...)>` — predicates are pure reads (`&CellContext`), mutation happens in case functions. This enforces the separation.
- **Named case functions**: Each case is a focused, documented unit. When a case produces wrong geometry, you find `case_7_inner_asymmetric_bd` by name and read 30 lines, not 400.
- **Case matching with rotation**: The `for rotation in 0..4` loop means each case only handles one canonical orientation. 17 cases × 4 rotations = 68 potential matches, one fires per cell.
- **Primitive composition**: Simple cases combine primitives (Case 12 spiral = inner_corner + edge + outer_corner). Complex cases (7, 8, 22, 23) use inline `add_point` when geometry doesn't decompose into standard primitives.
- **Defensive fallback**: If no case matches, `add_full_floor()` prevents mesh holes. The validation check catches geometry bugs early.
- **Testing pure algorithms**: gdext's math types work without a Godot runtime, so the marching squares core is fully testable with `cargo test`.

## Stubs Introduced

(No new stubs)

## Stubs Resolved

- [x] `generate_cell()` — was stubbed in Part 04 ("primitives can't be called without the case dispatcher"), now fully implemented
- [x] `rust/src/marching_squares/cases.rs` — new file completing the module
- [x] Module split — `marching_squares.rs` → `marching_squares/` directory with focused sub-files
- [x] `CellColorState` — per-cell color fields extracted from `CellContext` into sub-struct
