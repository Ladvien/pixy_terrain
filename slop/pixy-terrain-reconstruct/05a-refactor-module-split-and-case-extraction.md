# Pixy Terrain — Part 05a: Module Split & Case Extraction

**Series:** Reconstructing Pixy Terrain
**Part:** 05a of 18 (addendum — applied after Part 05, before Part 06)
**Previous:** 05-seventeen-case-cell-generator.md
**Status:** Pending

## Why This Addendum Exists

After Part 05, `marching_squares.rs` is ~1600 lines in a single file containing:
- 11 constants
- 5 type definitions (`MergeMode`, `BlendMode`, `ColorChannel`, `TextureIndex`, `CellGeometry`)
- 1 god struct (`CellContext`) with ~40 fields spanning rotation state, color maps, material indices, boundary detection, and blend configuration
- 1 parameter struct (`ColorSampleParams`)
- ~10 helper functions
- 5 geometry primitives
- 1 monolithic case dispatcher (`generate_cell`) with 17+ cases as a nested if/else waterfall
- 1 vertex factory (`add_point`) doing rotation, NaN defense, 4 color paths, material blend, UV, and vertex push
- Unit tests

This is the most critical file in the codebase — every terrain vertex passes through it. It's also the hardest to debug: when a case produces wrong geometry, you're reading through 400+ lines of interleaved conditionals and `rotate_cell` side effects to find the problem. The single-file layout makes it impossible to see the architecture at a glance.

This refactor splits the module into focused files, extracts each case into a named function, and separates `CellContext` into logical sub-states. No behavioral changes — the output mesh is bit-identical.

## What Changes

### File Structure

**Before:**
```
rust/src/marching_squares.rs  (~1600 lines, everything)
```

**After:**
```
rust/src/marching_squares/
├── mod.rs              (~60 lines)   — public API re-exports
├── types.rs            (~200 lines)  — MergeMode, BlendMode, ColorChannel, TextureIndex, CellGeometry
├── cell_context.rs     (~350 lines)  — CellContext, CellColorState, impl blocks
├── vertex.rs           (~250 lines)  — add_point, compute_vertex_color, ColorSampleParams, helpers
├── primitives.rs       (~300 lines)  — add_full_floor, add_outer_corner, add_edge, add_inner_corner, add_diagonal_floor
├── cases.rs            (~500 lines)  — generate_cell dispatcher + per-case functions
└── tests.rs            (~100 lines)  — unit tests
```

**Why this split works:** Rust treats `mod marching_squares;` identically whether it resolves to `marching_squares.rs` or `marching_squares/mod.rs`. Every existing `use crate::marching_squares::{...}` continues to compile with zero changes to external callers (`chunk.rs`, `lib.rs`).

## Steps

### Step 1: Create the module directory and move the file

**Why:** Rust's module system resolves `mod marching_squares;` to either `marching_squares.rs` or `marching_squares/mod.rs`. We convert from the former to the latter.

```bash
mkdir -p rust/src/marching_squares
mv rust/src/marching_squares.rs rust/src/marching_squares/mod.rs
```

Verify: `cargo build` should still compile — nothing has changed yet, just the file location.

### Step 2: Extract `types.rs` — stable, self-contained types

**Why:** `MergeMode`, `BlendMode`, `ColorChannel`, `TextureIndex`, and `CellGeometry` are stable definitions that almost never change. They have no dependencies on the rest of the module (only `godot::prelude::*`). Isolating them means you can read the type definitions without scrolling past 1400 lines of geometry code.

**File:** `rust/src/marching_squares/types.rs`

Move the following from `mod.rs` into `types.rs`:
- All constants (`BLEND_EDGE_SENSITIVITY` through `WALL_BLEND_SENTINEL`)
- `MergeMode` enum + impl
- `BlendMode` enum
- `ColorChannel` enum + impl
- `TextureIndex` struct + impl
- `CellGeometry` struct

The only import needed:
```rust
use godot::prelude::*;
```

**In `mod.rs`**, add:
```rust
mod types;
pub use types::*;
```

Verify: `cargo build` compiles. External callers still use `crate::marching_squares::MergeMode` etc. unchanged.

### Step 3: Extract `cell_context.rs` — split the god struct

**Why:** `CellContext` has ~40 fields serving three distinct roles:
1. **Grid state** — heights, edges, rotation, cell_coords, dimensions, cell_size, merge_threshold, higher_poly_floors (the marching squares algorithm state)
2. **Color state** — 5 color maps, 8 boundary colors, 3 material indices, blend_mode, blend thresholds, is_new_chunk (the vertex coloring state)
3. **Mode flags** — floor_mode, use_ridge_texture, ridge_threshold, chunk_position (output configuration)

The grid state is read by `generate_cell` and the primitives. The color state is read by `add_point` and `compute_vertex_color`. Separating them makes it clear which functions depend on what.

**File:** `rust/src/marching_squares/cell_context.rs`

Extract a new struct for the per-cell color computation results:

```rust
use godot::prelude::*;
use super::types::*;

/// Pre-computed color state for the current cell.
/// Calculated once per cell in `generate_cell`, consumed by `add_point`.
#[derive(Clone, Debug, Default)]
pub struct CellColorState {
    // Cell boundary detection
    pub cell_min_height: f32,
    pub cell_max_height: f32,
    pub cell_is_boundary: bool,

    // Boundary colors (floor)
    pub cell_floor_lower_color_0: Color,
    pub cell_floor_upper_color_0: Color,
    pub cell_floor_lower_color_1: Color,
    pub cell_floor_upper_color_1: Color,

    // Boundary colors (wall)
    pub cell_wall_lower_color_0: Color,
    pub cell_wall_upper_color_0: Color,
    pub cell_wall_lower_color_1: Color,
    pub cell_wall_upper_color_1: Color,

    // Per-cell dominant materials (3-texture system)
    pub cell_material_a: TextureIndex,
    pub cell_material_b: TextureIndex,
    pub cell_material_c: TextureIndex,
}
```

Then update `CellContext` to embed it:

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

Move all existing `CellContext` methods (`ay`, `by`, `cy`, `dy`, `ab`, `bd`, `cd`, `ac`, `rotate_cell`, `is_higher`, `is_lower`, `is_merged`, `start_floor`, `start_wall`, `color_index`, `corner_indices`, `calculate_boundary_colors`, `calculate_cell_material_pair`, `calculate_material_blend_data`) into this file.

Update method bodies that read boundary colors / material indices to go through `self.color_state.*` instead of `self.*`. For example:

```rust
// Before:
self.cell_material_a = TextureIndex(first.0);

// After:
self.color_state.cell_material_a = TextureIndex(first.0);
```

Add a reset method that `generate_cell` calls at the top of each cell iteration:

```rust
impl CellColorState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
```

**In `mod.rs`**, add:
```rust
mod cell_context;
pub use cell_context::*;
```

**Migration note:** Every site in `add_point` / `compute_vertex_color` / `calculate_material_blend_data` that reads `ctx.cell_is_boundary`, `ctx.cell_min_height`, `ctx.cell_material_a`, etc. becomes `ctx.color_state.cell_is_boundary`, `ctx.color_state.cell_min_height`, `ctx.color_state.cell_material_a`. This is a mechanical find-and-replace. There are approximately 30 such sites.

Similarly, the per-cell reset block in `generate_terrain_cells` (chunk.rs, Part 07) that resets 11 color fields individually becomes:
```rust
ctx.color_state.reset();
```

Verify: `cargo build` + `cargo test` pass.

### Step 4: Extract `vertex.rs` — the vertex factory

**Why:** `add_point` and its helpers (`compute_vertex_color`, `push_vertex`, `sanitize_float`, `lerp_color`, `preserve_high_channels`, `get_dominant_color`, `ColorSampleParams`) form a self-contained subsystem. They depend on `CellContext` and `CellGeometry` but nothing else in the module. Isolating them means when you debug a color blending issue, you look at one ~250 line file.

**File:** `rust/src/marching_squares/vertex.rs`

Move from `mod.rs`:
- `ColorSampleParams` struct
- `lerp_color()`
- `get_dominant_color()` (the pub wrapper around `ColorChannel::dominant(...).to_one_hot()`)
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

**In `mod.rs`**, add:
```rust
mod vertex;
pub use vertex::*;
```

Verify: `cargo build` compiles.

### Step 5: Extract `primitives.rs` — the 5 geometry building blocks

**Why:** The primitives are stable, well-tested, and independently understandable. Each is a pure function of `CellContext` + `CellGeometry` that calls `add_point`. Grouping them in one file makes it easy to see the complete set of building blocks.

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

**In `mod.rs`**, add:
```rust
mod primitives;
pub use primitives::*;
```

Verify: `cargo build` compiles.

### Step 6: Extract `cases.rs` — the dispatcher and per-case functions

**Why:** This is the highest-churn, hardest-to-debug part of the codebase. The current `generate_cell()` is a ~400 line waterfall of if/else blocks with inline `rotate_cell` calls and primitive invocations. Extracting each case into a named function turns the dispatcher into a readable table of conditions.

**File:** `rust/src/marching_squares/cases.rs`

**Step 6a: Extract each case body into a named function.**

The pattern for each case:

```rust
/// Case 3: AB edge with A outer corner above.
/// Geometry: half-width edge + flattened outer corner at A.
///
/// ```text
///   A ─────┐ B
///   │ top  │ │
///   │floor │ │
///   └──┬───┘ │
///      │wall │  ← edge (half-width, 0.5 to 1.0)
///      │     │
///   C ─┴──── D
/// ```
fn case_3_edge_a_outer_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_edge(ctx, geo, true, true, 0.5, 1.0);
    add_outer_corner(ctx, geo, false, true, true, by);
}
```

Extract all ~20 case bodies this way. Cases 7-8 and 18-19 (which have inline geometry) become their own functions too:

```rust
/// Case 7: A lowest, BD connected, C higher than D.
/// Custom inline geometry — doesn't decompose into standard primitives.
fn case_7_inner_corner_asymmetric_bd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, false, false);
    let by = ctx.by();
    let dy = ctx.dy();
    let cy = ctx.cy();
    let edge_mid = (by + dy) / 2.0;

    // D corner floor
    ctx.start_floor();
    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
    // ... rest of the inline geometry
}
```

**Step 6b: Rewrite `generate_cell` as a match table.**

```rust
use super::cell_context::CellContext;
use super::types::CellGeometry;
use super::primitives::*;
use super::vertex::add_point;

pub fn generate_cell(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let initial_vert_count = geo.verts.len();

    // Calculate edge connectivity
    ctx.edges = [
        (ctx.heights[0] - ctx.heights[1]).abs() < ctx.merge_threshold,
        (ctx.heights[1] - ctx.heights[2]).abs() < ctx.merge_threshold,
        (ctx.heights[3] - ctx.heights[2]).abs() < ctx.merge_threshold,
        (ctx.heights[0] - ctx.heights[3]).abs() < ctx.merge_threshold,
    ];

    // Pre-compute cell color state
    let ay = ctx.ay();
    let by = ctx.by();
    let cy = ctx.cy();
    let dy = ctx.dy();
    ctx.color_state.cell_min_height = ay.min(by).min(cy).min(dy);
    ctx.color_state.cell_max_height = ay.max(by).max(cy).max(dy);
    ctx.color_state.cell_is_boundary =
        (ctx.color_state.cell_max_height - ctx.color_state.cell_min_height) > ctx.merge_threshold;

    ctx.calculate_cell_material_pair();
    if ctx.color_state.cell_is_boundary {
        ctx.calculate_boundary_colors();
    }

    // Case 0: all edges connected → full floor (fast path, most common)
    if ctx.ab() && ctx.bd() && ctx.cd() && ctx.ac() {
        add_full_floor(ctx, geo);
        validate_geometry(ctx, geo, initial_vert_count);
        return;
    }

    // Try all 4 rotations to find a matching case
    let matched = 'rotation: {
        for i in 0..4 {
            ctx.rotation = i;
            if let Some(case_fn) = match_case(ctx) {
                case_fn(ctx, geo);
                break 'rotation true;
            }
        }
        false
    };

    if !matched {
        ctx.rotation = 0;
        add_full_floor(ctx, geo);
    }

    validate_geometry(ctx, geo, initial_vert_count);
}

/// Match the current rotation against all cases. Returns the case handler if matched.
fn match_case(ctx: &CellContext) -> Option<fn(&mut CellContext, &mut CellGeometry)> {
    let (ay, by, cy, dy) = (ctx.ay(), ctx.by(), ctx.cy(), ctx.dy());

    // Case 1: A higher than adjacent, opposite corners connected
    if ctx.is_higher(ay, by) && ctx.is_higher(ay, cy) && ctx.bd() && ctx.cd() {
        return Some(case_1_outer_corner);
    }

    // Case 2: Edge — AB higher than CD
    if ctx.is_higher(ay, cy) && ctx.is_higher(by, dy) && ctx.ab() && ctx.cd() {
        return Some(case_2_edge);
    }

    // Case 3: AB edge with A outer corner above
    if ctx.is_higher(ay, by) && ctx.is_higher(ay, cy) && ctx.is_higher(by, dy) && ctx.cd() {
        return Some(case_3_edge_a_outer_corner);
    }

    // Case 4: AB edge with B outer corner above
    if ctx.is_higher(by, ay) && ctx.is_higher(ay, cy) && ctx.is_higher(by, dy) && ctx.cd() {
        return Some(case_4_edge_b_outer_corner);
    }

    // Case 5: B and C higher than A and D, merged
    if ctx.is_lower(ay, by)
        && ctx.is_lower(ay, cy)
        && ctx.is_lower(dy, by)
        && ctx.is_lower(dy, cy)
        && ctx.is_merged(by, cy)
    {
        return Some(case_5_double_inner_corner);
    }

    // Case 5.5: B and C higher than A and D, B higher than C
    if ctx.is_lower(ay, by)
        && ctx.is_lower(ay, cy)
        && ctx.is_lower(dy, by)
        && ctx.is_lower(dy, cy)
        && ctx.is_higher(by, cy)
    {
        return Some(case_5_5_double_inner_with_outer);
    }

    // Case 6: A is the lowest corner
    if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.bd() && ctx.cd() {
        return Some(case_6_inner_corner);
    }

    // Case 7: A lowest, BD connected, C higher than D
    if ctx.is_lower(ay, by)
        && ctx.is_lower(ay, cy)
        && ctx.bd()
        && !ctx.cd()
        && ctx.is_higher(cy, dy)
    {
        return Some(case_7_inner_asymmetric_bd);
    }

    // Case 8: A lowest, CD connected, B higher than D
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

    // Case 10: Inner corner at A with edge atop BD
    if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.is_higher(dy, cy) && ctx.bd() {
        return Some(case_10_inner_corner_edge_bd);
    }

    // Case 11: Inner corner at A with edge atop CD
    if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.is_higher(dy, by) && ctx.cd() {
        return Some(case_11_inner_corner_edge_cd);
    }

    // Case 12: Clockwise spiral A<B<D<C
    if ctx.is_lower(ay, by)
        && ctx.is_lower(by, dy)
        && ctx.is_lower(dy, cy)
        && ctx.is_higher(cy, ay)
    {
        return Some(case_12_spiral_clockwise);
    }

    // Case 13: Clockwise spiral A<C<D<B
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

    // Case 16: A higher, merged edge variant (AB+CD connected)
    if ctx.is_higher(ay, cy)
        && ctx.is_merged(ay, by)
        && ctx.is_merged(cy, dy)
        && ctx.ab()
        && ctx.cd()
    {
        return Some(case_2_edge); // Degenerates to edge
    }

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

    // Case 21: A higher than B, B merged with C, BD not connected, D lower
    if ctx.is_higher(ay, by)
        && ctx.is_merged(by, cy)
        && !ctx.bd()
        && ctx.is_lower(dy, by)
    {
        return Some(case_21_outer_inner_composite);
    }

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

Then each case function is a focused, documented unit. Example for the simple ones:

```rust
fn case_1_outer_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_outer_corner(ctx, geo, true, true, false, -1.0);
}

fn case_2_edge(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_edge(ctx, geo, true, true, 0.0, 1.0);
}

fn case_3_edge_a_outer_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_edge(ctx, geo, true, true, 0.5, 1.0);
    add_outer_corner(ctx, geo, false, true, true, by);
}
```

And for the complex inline-geometry cases (7, 8, 22, 23), the full body goes inside the named function with a doc comment showing the geometry layout.

**In `mod.rs`**, add:
```rust
mod cases;
pub use cases::generate_cell;
```

**Critical detail on `match_case` taking `&CellContext` (immutable):** The predicate matching only reads heights, edges, and merge_threshold — it never mutates. This is deliberate. Mutation (rotation, `start_floor`/`start_wall`, `add_point`) happens only inside the case functions. If a future case needs to test a rotation before committing, save and restore `ctx.rotation` inside the case function, not in the matcher.

Verify: `cargo build` + `cargo test` pass. The case numbering in comments may not be 1:1 with the Part 05 tutorial — rename to match whatever the actual implemented cases are. The names should describe the geometry, not just a number.

### Step 7: Extract `tests.rs`

**Why:** Tests should be in their own file for the same reason as everything else — readability and focus. The `#[cfg(test)]` attribute still works in a separate file.

**File:** `rust/src/marching_squares/tests.rs`

Move the entire `#[cfg(test)] mod tests { ... }` block from `mod.rs` into `tests.rs`, but remove the outer `mod tests` wrapper (the file *is* the module):

```rust
#[cfg(test)]
use super::*;

#[test]
fn test_merge_mode_thresholds() {
    // ...
}
// ... rest of tests
```

**In `mod.rs`**, add:
```rust
#[cfg(test)]
mod tests;
```

Verify: `cargo test` passes.

### Step 8: Clean up `mod.rs`

After all extractions, `mod.rs` should be approximately:

```rust
mod types;
mod cell_context;
mod vertex;
mod primitives;
mod cases;

#[cfg(test)]
mod tests;

// Re-export the public API
pub use types::*;
pub use cell_context::{CellContext, CellColorState};
pub use vertex::{add_point, get_dominant_color};
pub use primitives::*;
pub use cases::generate_cell;
```

This file is the module's table of contents. Anyone reading it sees the full architecture in 15 lines.

### Step 9: Update `chunk.rs` imports

`chunk.rs` currently uses:
```rust
use crate::marching_squares::{self, CellContext, CellGeometry, MergeMode};
```

And calls:
```rust
marching_squares::generate_cell(&mut ctx, &mut geo);
marching_squares::add_full_floor(&mut ctx, &mut geo);
```

These continue to work unchanged because `mod.rs` re-exports everything. The only thing to check: if `chunk.rs` directly references any field that moved into `CellColorState`, update those references.

The per-cell reset block in `generate_terrain_cells` changes from:

```rust
ctx.cell_floor_lower_color_0 = default_color;
ctx.cell_floor_upper_color_0 = default_color;
// ... 8 more lines
ctx.cell_material_a = TextureIndex::default();
ctx.cell_material_b = TextureIndex::default();
ctx.cell_material_c = TextureIndex::default();
```

To:
```rust
ctx.color_state.reset();
```

Verify: `cargo build` + `cargo test` pass with zero behavioral changes.

## What Was NOT Changed

- **No new functionality.** Mesh output is bit-identical before and after.
- **No external API changes.** `chunk.rs` and `lib.rs` see the same public types and functions.
- **No new dependencies.**
- **Primitive function signatures unchanged.** `add_full_floor`, `add_edge`, etc. keep their exact parameter lists.
- **`add_point` signature unchanged.** It still takes `&mut CellContext, &mut CellGeometry, ...`.
- **Case ordering in the dispatcher preserved.** Cases are tested in the same order as Part 05 to ensure identical matching behavior (first match wins across rotations).

## What To Watch For

1. **`match_case` takes `&CellContext` not `&mut`.** If you find a case predicate that needs mutation, that's a design smell — predicates should be pure reads.

2. **Case function pointer type is `fn(&mut CellContext, &mut CellGeometry)`.** If a case needs extra parameters (like Cases 22-23 which compute `edge_by`/`edge_cy` inside the body), those computations happen inside the case function, not in the dispatcher.

3. **The `CellColorState` migration is mechanical but tedious.** There are ~30 sites across `vertex.rs`, `cell_context.rs`, and `chunk.rs` that need `ctx.field` → `ctx.color_state.field`. Use find-and-replace but verify each one — some fields stayed on `CellContext` (like `is_new_chunk`, `blend_mode`).

4. **Test after each step.** Don't batch all extractions and hope. The module split in Step 1 should compile. Each subsequent extraction should compile independently.

## Verification

After all steps:

```bash
cd rust && cargo build     # compiles clean
cd rust && cargo test      # all tests pass
cd rust && cargo clippy    # no new warnings
```

Open the Godot project and verify terrain renders identically (if you have terrain scenes from earlier testing).

## Impact Summary

1. **1600-line single file → 7 focused files** averaging ~250 lines each
2. **God struct partially decomposed** — `CellColorState` isolates per-cell color computation from grid state
3. **Case dispatcher reads as a table** — `match_case` returns `Option<fn>`, each case is a named function with a doc comment
4. **11-field per-cell reset → `color_state.reset()`** — one call instead of 11 assignments
5. **Zero behavioral changes** — bit-identical mesh output
