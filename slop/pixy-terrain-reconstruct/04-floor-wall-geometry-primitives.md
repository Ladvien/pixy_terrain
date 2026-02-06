# Pixy Terrain — Part 04: Floor & Wall Geometry Primitives

**Series:** Reconstructing Pixy Terrain
**Part:** 04 of 18
**Previous:** 2026-02-06-vertex-generation-color-encoding-03.md
**Status:** Complete

## What We're Building

The 5 geometry building blocks that every marching squares case is composed from: `add_full_floor()`, `add_outer_corner()`, `add_edge()`, `add_inner_corner()`, and `add_diagonal_floor()`. These are like LEGO bricks — the 17+ cases in `generate_cell()` assemble combinations of these primitives at different rotations.

## What You'll Have After This

All geometry primitives are callable. Combined with `add_point()` from Part 03, you can now generate any terrain cell geometry. The case dispatcher (`generate_cell()`) comes in Part 05.

## Prerequisites

- Part 03 completed (`add_point()`, material blend, boundary colors)

## Steps

### Step 1: `add_full_floor()` — Case 0

**Why:** When all 4 edges are connected (all corners within merge threshold), the cell is a smooth slope with no walls. This is the most common case — flat terrain. Higher-poly mode uses 4 triangles meeting at the center for better curvature; low-poly mode uses 2 triangles (a simple quad).

**File:** `rust/src/marching_squares.rs` (append after `add_point()`)

```rust
/// Generate Case 0: full floor with all edges connected.
pub fn add_full_floor(ctx: &mut CellContext, geo: &mut CellGeometry) {
    ctx.start_floor();
    let (ay, by, cy, dy) = (ctx.ay(), ctx.by(), ctx.cy(), ctx.dy());

    if ctx.higher_poly_floors {
        let ey = (ay + by + cy + dy) / 4.0;

        // Triangle 1: A-B-E
        add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, ey, 0.5, 0.0, 0.0, true);

        // Triangle 2: B-D-E
        add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, ey, 0.5, 0.0, 0.0, true);

        // Triangle 3: D-C-E
        add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, ey, 0.5, 0.0, 0.0, true);

        // Triangle 4: C-A-E
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, ey, 0.5, 0.0, 0.0, true);
    } else {
        // Simple 2-triangle floor
        add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);

        add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    }
}
```

**What's happening:**

The higher-poly floor creates a "fan" of 4 triangles around a center point E:

```
  A ─── B         A ─── B
  │ ╲ ╱ │         │   / │
  │  E  │   vs    │  /  │   (low poly: 2 triangles)
  │ ╱ ╲ │         │ /   │
  C ─── D         C ─── D
```

The center point E has height `(A+B+C+D)/4` — the average of all corners. This prevents the "bowtie" artifact where two opposite high corners create a ridge along the diagonal. With 4 triangles, the center smoothly interpolates.

The center vertex has `diag_midpoint = true`, which triggers the special midpoint color blending in `add_point()`.

All UV values are `(0, 0)` for flat floors — no cliff detection needed.

### Step 2: `add_outer_corner()` — Case 1

**Why:** An outer corner appears when one corner (A) is raised above its neighbors while the opposite corners are connected. Think of a mesa with one raised pillar — the pillar's corner is the outer corner. It generates a floor triangle on top, two wall triangles on the sides, and optionally a floor below.

**File:** `rust/src/marching_squares.rs` (append after `add_full_floor()`)

```rust
/// Case 1: Outer corner where A is the raised corner.
pub fn add_outer_corner(
    ctx: &mut CellContext,
    geo: &mut CellGeometry,
    floor_below: bool,
    floor_above: bool,
    flatten_bottom: bool,
    bottom_height: f32,
) {
    let (ay, by, cy, dy) = (ctx.ay(), ctx.by(), ctx.cy(), ctx.dy());
    let edge_by = if flatten_bottom { bottom_height } else { by };
    let edge_cy = if flatten_bottom { bottom_height } else { cy };

    if floor_above {
        ctx.start_floor();
        add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, ay, 0.0, 0.0, 1.0, false);
        add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);
    }

    // Walls
    ctx.start_wall();
    add_point(ctx, geo, 0.0, edge_cy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 0.5, edge_by, 0.0, 1.0, 0.0, false);

    add_point(ctx, geo, 0.5, ay, 0.0, 1.0, 1.0, false);
    add_point(ctx, geo, 0.5, edge_by, 0.0, 1.0, 0.0, false);
    add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);

    if floor_below {
        ctx.start_floor();
        add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);

        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, cy, 0.5, 1.0, 0.0, false);
        add_point(ctx, geo, 0.5, by, 0.0, 1.0, 0.0, false);

        add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, by, 0.0, 1.0, 0.0, false);
    }
}
```

**What's happening:**

```
  A ────┐               A is raised
  │ top │               ┌───┐
  │floor│               │   │ ← wall (2 triangles)
  └──┬──┘               └───┘
     │                  ┌─────────┐
     │ wall             │  lower  │
     │                  │  floor  │ ← 3 triangles
  ┌──┴──────────┐       └─────────┘
  │ lower floor │
  │  B ──── D   │
  └─── C ──────┘
```

- `floor_above`: A single triangle on top of the raised corner. UV1.y = 1.0 at the edges (ridge detection).
- `flatten_bottom`: When combining with other primitives (e.g., Case 3: edge + outer corner), the wall bottom needs to match the adjacent edge's height, not the corner's actual height.
- `bottom_height`: The flattened height to use when `flatten_bottom` is true.
- `floor_below`: An L-shaped floor around the base of the corner. Uses 3 triangles because the L-shape isn't a simple quad.

### Step 3: `add_edge()` — Case 2

**Why:** An edge appears when one side (A-B) is raised above the opposite side (C-D). Think of a cliff or terrace step. It generates a floor on top, a wall face, and optionally a floor below.

**File:** `rust/src/marching_squares.rs` (append after `add_outer_corner()`)

```rust
/// Case 2: Edge where AB is the raised edge.
pub fn add_edge(
    ctx: &mut CellContext,
    geo: &mut CellGeometry,
    floor_below: bool,
    floor_above: bool,
    a_x: f32,
    b_x: f32,
) {
    let (ay, by, cy, dy) = (ctx.ay(), ctx.by(), ctx.cy(), ctx.dy());
    let ab = ctx.ab();
    let cd = ctx.cd();

    let edge_ay = if ab { ay } else { ay.min(by) };
    let edge_by = if ab { by } else { ay.min(by) };
    let edge_cy = if cd { cy } else { cy.max(dy) };
    let edge_dy = if cd { dy } else { cy.max(dy) };

    if floor_above {
        ctx.start_floor();
        let uv_a = if a_x > 0.0 { 1.0 } else { 0.0 };
        let uv_b = if b_x < 1.0 { 1.0 } else { 0.0 };
        let uv_left = if b_x < 1.0 {
            -1.0
        } else if a_x > 0.0 {
            1.0
        } else {
            0.0
        };
        let uv_right = if a_x > 0.0 {
            -1.0
        } else if b_x < 1.0 {
            1.0
        } else {
            0.0
        };

        add_point(ctx, geo, a_x, edge_ay, 0.0, uv_a, 0.0, false);
        add_point(ctx, geo, b_x, edge_by, 0.0, uv_b, 0.0, false);
        add_point(ctx, geo, 0.0, edge_ay, 0.5, uv_left, 1.0, false);

        add_point(ctx, geo, 1.0, edge_by, 0.5, uv_right, 1.0, false);
        add_point(ctx, geo, 0.0, edge_ay, 0.5, uv_left, 1.0, false);
        add_point(ctx, geo, b_x, edge_by, 0.0, uv_b, 0.0, false);
    }

    // Wall
    ctx.start_wall();
    add_point(ctx, geo, 0.0, edge_cy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, edge_ay, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 1.0, edge_dy, 0.5, 1.0, 0.0, false);

    add_point(ctx, geo, 1.0, edge_by, 0.5, 1.0, 1.0, false);
    add_point(ctx, geo, 1.0, edge_dy, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 0.0, edge_ay, 0.5, 0.0, 1.0, false);

    if floor_below {
        ctx.start_floor();
        add_point(ctx, geo, 0.0, cy, 0.5, 1.0, 0.0, false);
        add_point(ctx, geo, 1.0, dy, 0.5, 1.0, 0.0, false);
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);

        add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 1.0, dy, 0.5, 1.0, 0.0, false);
    }
}
```

**What's happening:**

```
  A ════════ B     (raised edge)
  ║  upper  ║     ← 2 triangles (floor_above)
  ║  floor  ║
  ╠════════╣     ← wall at z=0.5 (2 triangles)
  ║  lower  ║
  ║  floor  ║     ← 2 triangles (floor_below)
  C ──────── D
```

- `a_x` and `b_x` control where the edge starts/ends horizontally. For a full edge, `a_x=0.0, b_x=1.0`. For a partial edge (Cases 3-4), one side is `0.5` — leaving room for an outer corner.
- Edge heights are averaged when AB or CD aren't connected: `edge_ay = ay.min(by)`. This prevents a "stepped" wall when the top corners have slightly different heights.
- UV values on the upper floor encode which edges are cliff edges (-1.0 or 1.0) vs interior edges (0.0). This lets the shader darken cliff edges for depth.

### Step 4: `add_inner_corner()` — Case 6

**Why:** An inner corner is the inverse of an outer corner — one corner (A) is *lowered* while the rest are raised. Think of a canyon or pit. It generates a small floor at the bottom of the lowered corner, walls going up, and a large floor at the top.

**File:** `rust/src/marching_squares.rs` (append after `add_edge()`)

```rust
/// Inner corner where A is the lowered corner.
pub fn add_inner_corner(
    ctx: &mut CellContext,
    geo: &mut CellGeometry,
    lower_floor: bool,
    full_upper_floor: bool,
    flatten: bool,
    bd_floor: bool,
    cd_floor: bool,
) {
    let (ay, by, cy, dy) = (ctx.ay(), ctx.by(), ctx.cy(), ctx.dy());
    let corner_by = if flatten { by.min(cy) } else { by };
    let corner_cy = if flatten { by.min(cy) } else { cy };

    if lower_floor {
        ctx.start_floor();
        add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, ay, 0.0, 1.0, 0.0, false);
        add_point(ctx, geo, 0.0, ay, 0.5, 1.0, 0.0, false);
    }

    ctx.start_wall();
    add_point(ctx, geo, 0.0, ay, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 0.5, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, corner_cy, 0.5, 1.0, 1.0, false);

    add_point(ctx, geo, 0.5, corner_by, 0.0, 0.0, 1.0, false);
    add_point(ctx, geo, 0.0, corner_cy, 0.5, 1.0, 1.0, false);
    add_point(ctx, geo, 0.5, ay, 0.0, 0.0, 0.0, false);

    ctx.start_floor();
    if full_upper_floor {
        add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, corner_cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 1.0, corner_by, 0.0, 0.0, 0.0, false);

        add_point(ctx, geo, 0.0, corner_cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, corner_cy, 0.5, 0.0, 1.0, false);
        add_point(ctx, geo, 0.5, corner_by, 0.0, 0.0, 1.0, false);

        add_point(ctx, geo, 1.0, corner_by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, corner_cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, corner_by, 0.0, 0.0, 1.0, false);
    }

    if cd_floor {
        add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, by, 0.5, 1.0, 1.0, false);
        add_point(ctx, geo, 0.5, by, 0.0, 0.0, 1.0, false);

        add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 1.0, by, 0.5, 1.0, -1.0, false);
        add_point(ctx, geo, 0.0, by, 0.5, 1.0, 1.0, false);
    }

    if bd_floor {
        add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 1.0, false);
        add_point(ctx, geo, 0.5, cy, 0.0, 1.0, 1.0, false);
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);

        add_point(ctx, geo, 0.5, cy, 1.0, 1.0, -1.0, false);
        add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, cy, 0.0, 1.0, 1.0, false);
    }
}
```

**What's happening:**

```
  ┌─A─┐                A is lowered (pit corner)
  │   │ ← lower floor (1 triangle at A's height)
  │   │
  ╠═══╝ ← wall (2 triangles, L-shaped)
  ║
  ║  ┌─────────────┐
  ╚══╡ upper floor │   ← full_upper_floor: 3 triangles
     │  B ──── D   │
     └─── C ──────┘
```

- `flatten`: When combining with other primitives, forces B and C wall heights to `min(by, cy)` so walls line up.
- `full_upper_floor`: The large L-shaped floor covering B-C-D. Set to false when another primitive (diagonal floor, edge) covers part of this area.
- `bd_floor` / `cd_floor`: Partial floor sections for Cases 10-11 where an edge atop this inner corner needs floor strips on specific sides.

### Step 5: `add_diagonal_floor()` — Connecting B and C

**Why:** When B and C are at the same height but A and D are at different heights, the floor runs diagonally from B to C. This creates the "saddle" geometry you see at terrain transitions.

**File:** `rust/src/marching_squares.rs` (append after `add_inner_corner()`)

```rust
/// Diagonal floor connecting B and C corners.
pub fn add_diagonal_floor(
    ctx: &mut CellContext,
    geo: &mut CellGeometry,
    b_y: f32,
    c_y: f32,
    a_cliff: bool,
    d_cliff: bool,
) {
    ctx.start_floor();

    let a_uv_x = if a_cliff { 0.0 } else { 1.0 };
    let a_uv_y = if a_cliff { 1.0 } else { 0.0 };
    let d_uv_x = if d_cliff { 0.0 } else { 1.0 };
    let d_uv_y = if d_cliff { 1.0 } else { 0.0 };

    add_point(ctx, geo, 1.0, b_y, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, c_y, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, b_y, 0.0, a_uv_x, a_uv_y, false);

    add_point(ctx, geo, 0.0, c_y, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, c_y, 0.5, a_uv_x, a_uv_y, false);
    add_point(ctx, geo, 0.5, b_y, 0.0, a_uv_x, a_uv_y, false);

    add_point(ctx, geo, 1.0, b_y, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, b_y, 0.5, d_uv_x, d_uv_y, false);
    add_point(ctx, geo, 0.0, c_y, 1.0, 0.0, 0.0, false);

    add_point(ctx, geo, 0.0, c_y, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, b_y, 0.5, d_uv_x, d_uv_y, false);
    add_point(ctx, geo, 0.5, c_y, 1.0, d_uv_x, d_uv_y, false);
}
```

**What's happening:**

```
  A ╲         ╲ B       The diagonal floor is the 4-triangle
     ╲  ┌───┐  ╲       strip running from B to C.
      ╲ │   │   ╲
       ╲│   │    ╲     a_cliff/d_cliff control UV encoding
  C ╱   └───┘  ╱ D     near the A and D walls.
   ╱          ╱
```

- 4 triangles form a diagonal strip connecting B's edge (top-right) to C's edge (bottom-left).
- `a_cliff` / `d_cliff` booleans control UV encoding for cliff edge detection. When the A side has a wall, the floor near A gets cliff UVs; same for D side.
- This primitive is used in Cases 5, 5.5, 9, 13, and 14 — any case where B and C form the "bridge" between two walls.

## Verify

```bash
cd rust && cargo build
```

**Expected:** Compiles successfully. All 5 geometry functions are defined but not yet called by a dispatcher.

## What You Learned

- **Primitive composition**: Complex terrain geometry comes from combining just 5 building blocks at different rotations — the same design pattern Yugen used in GDScript
- **Floor/wall mode switching**: `ctx.start_floor()` and `ctx.start_wall()` toggle how `add_point()` handles UVs, smooth groups, and color sampling
- **Flatten trick**: When combining primitives (e.g., edge + outer corner), `flatten_bottom` / `flatten` force wall heights to align, preventing gaps between independently-generated pieces
- **UV encoding for cliff detection**: UV1.x and UV1.y encode distance-to-cliff, allowing the shader to darken edges and exclude grass near drops
- **Triangle strip layout**: Every primitive generates triangles with consistent winding order (CCW when viewed from outside), which Godot's normal generation relies on

## Stubs Introduced

- [ ] `generate_cell()` — not yet written, primitives can't be called without the case dispatcher (Part 05)

## Stubs Resolved

(No stubs resolved — these are additions to the existing marching_squares.rs)
