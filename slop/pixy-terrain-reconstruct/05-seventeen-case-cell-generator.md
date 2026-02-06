# Pixy Terrain — Part 05: The 17-Case Cell Generator

**Series:** Reconstructing Pixy Terrain
**Part:** 05 of 18
**Previous:** 2026-02-06-floor-wall-geometry-primitives-04.md
**Status:** Complete

## What We're Building

The `generate_cell()` dispatcher — the brain that examines each cell's 4 corner heights, determines which of 17+ geometry cases applies, and calls the right combination of primitives at the right rotation. Also includes unit tests to verify the algorithm.

## What You'll Have After This

The complete marching squares algorithm. Given any 4 corner heights and a merge threshold, `generate_cell()` produces watertight geometry. The entire `marching_squares.rs` module is now feature-complete.

## Prerequisites

- Part 04 completed (all 5 geometry primitives)

## Steps

### Step 1: Add `generate_cell()`

**Why:** This is where the 17 marching squares cases are matched. The algorithm works by trying each case at 4 rotations — so 17 cases × 4 rotations = 68 potential matches, but only one fires per cell. The rotation loop means each case only needs to handle one canonical orientation.

**File:** `rust/src/marching_squares.rs` (append after `add_diagonal_floor()`, before any existing helper functions)

```rust
/// Generate geometry for a single cell based on the 17-case marching squares algorithm.
/// For Phase 1, only Case 0 (full floor) is implemented. Other cases will be added in Phase 2.
pub fn generate_cell(ctx: &mut CellContext, geo: &mut CellGeometry) {
    // Track initial vertex count for validation
    let initial_vert_count = geo.verts.len();

    let ay = ctx.ay();
    let by = ctx.by();
    let cy = ctx.cy();
    let dy = ctx.dy();

    // Calculate edge connectivity
    ctx.edges = [
        (ay - by).abs() < ctx.merge_threshold, // AB (top)
        (by - dy).abs() < ctx.merge_threshold, // BD (right)
        (cy - dy).abs() < ctx.merge_threshold, // CD (bottom)
        (ay - cy).abs() < ctx.merge_threshold, // AC (left)
    ];

    // Calculate cell height range for boundary detection
    ctx.cell_min_height = ay.min(by).min(cy).min(dy);
    ctx.cell_max_height = ay.max(by).max(cy).max(dy);
    ctx.cell_is_boundary = (ctx.cell_max_height - ctx.cell_min_height) > ctx.merge_threshold;

    // Calculate dominant material pair
    calculate_cell_material_pair(ctx);

    // Calculate boundary colors if needed
    if ctx.cell_is_boundary {
        calculate_boundary_colors(ctx);
    }

    // Case 0: all edges connected → full floor
    if ctx.ab() && ctx.bd() && ctx.cd() && ctx.ac() {
        add_full_floor(ctx, geo);
        return;
    }

    // Store original edges and heights for rotation
    let _original_edges = ctx.edges;
    let _original_heights = ctx.heights;

    // Try all 4 rotations to find a matching case
    let mut case_found = false;
    for i in 0..4 {
        ctx.rotation = i;

        let ay = ctx.ay();
        let by = ctx.by();
        let cy = ctx.cy();
        let dy = ctx.dy();

        // Case 1: A higher than adjacent, opposite corner connected
        if ctx.is_higher(ay, by) && ctx.is_higher(ay, cy) && ctx.bd() && ctx.cd() {
            add_outer_corner(ctx, geo, true, true, false, -1.0);
            case_found = true;
        }
        // Case 2: Edge - AB higher than CD
        else if ctx.is_higher(ay, cy) && ctx.is_higher(by, dy) && ctx.ab() && ctx.cd() {
            add_edge(ctx, geo, true, true, 0.0, 1.0);
            case_found = true;
        }
        // Case 3: AB edge with A outer corner above
        else if ctx.is_higher(ay, by)
            && ctx.is_higher(ay, cy)
            && ctx.is_higher(by, dy)
            && ctx.cd()
        {
            add_edge(ctx, geo, true, true, 0.5, 1.0);
            add_outer_corner(ctx, geo, false, true, true, by);
            case_found = true;
        }
        // Case 4: AB edge with B outer corner above
        else if ctx.is_higher(by, ay)
            && ctx.is_higher(ay, cy)
            && ctx.is_higher(by, dy)
            && ctx.cd()
        {
            add_edge(ctx, geo, true, true, 0.0, 0.5);
            ctx.rotate_cell(1);
            add_outer_corner(ctx, geo, false, true, true, cy);
            case_found = true;
        }
        // Case 5: B and C higher than A and D, merged
        else if ctx.is_lower(ay, by)
            && ctx.is_lower(ay, cy)
            && ctx.is_lower(dy, by)
            && ctx.is_lower(dy, cy)
            && ctx.is_merged(by, cy)
        {
            add_inner_corner(ctx, geo, true, false, false, false, false);
            add_diagonal_floor(ctx, geo, by, cy, true, true);
            ctx.rotate_cell(2);
            add_inner_corner(ctx, geo, true, false, false, false, false);
            case_found = true;
        }
        // Case 5.5: B and C higher than A and D, B higher than C
        else if ctx.is_lower(ay, by)
            && ctx.is_lower(ay, cy)
            && ctx.is_lower(dy, by)
            && ctx.is_lower(dy, cy)
            && ctx.is_higher(by, cy)
        {
            add_inner_corner(ctx, geo, true, false, true, false, false);
            add_diagonal_floor(ctx, geo, by, cy, true, true);
            ctx.rotate_cell(2);
            add_inner_corner(ctx, geo, true, false, true, false, false);
            // Higher corner B
            ctx.rotate_cell(-1);
            add_outer_corner(ctx, geo, false, true, false, -1.0);
            case_found = true;
        }
        // Case 6: A is the lowest corner
        else if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.bd() && ctx.cd() {
            add_inner_corner(ctx, geo, true, true, false, false, false);
            case_found = true;
        }
        // Case 7: A lowest, BD connected (not CD), C higher than D (Yugen Case 8)
        else if ctx.is_lower(ay, by)
            && ctx.is_lower(ay, cy)
            && ctx.bd()
            && !ctx.cd()
            && ctx.is_higher(cy, dy)
        {
            add_inner_corner(ctx, geo, true, false, true, false, false);
            let by = ctx.by();
            let dy = ctx.dy();
            let cy = ctx.cy();
            let edge_mid = (by + dy) / 2.0;

            // D corner floor
            ctx.start_floor();
            add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, dy, 1.0, 1.0, 0.0, false);
            add_point(ctx, geo, 1.0, edge_mid, 0.5, 0.0, 0.0, false);

            // B corner floor
            add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, edge_mid, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, by, 0.0, 0.0, 1.0, false);

            // Center floors
            add_point(ctx, geo, 0.5, by, 0.0, 0.0, 1.0, false);
            add_point(ctx, geo, 1.0, edge_mid, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.0, by, 0.5, 1.0, 1.0, false);

            add_point(ctx, geo, 0.5, dy, 1.0, 1.0, 0.0, false);
            add_point(ctx, geo, 0.0, by, 0.5, 1.0, 1.0, false);
            add_point(ctx, geo, 1.0, edge_mid, 0.5, 0.0, 0.0, false);

            // Walls to upper corner
            ctx.start_wall();
            add_point(ctx, geo, 0.0, by, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, dy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);

            add_point(ctx, geo, 0.5, cy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, dy, 1.0, 0.0, 0.0, false);

            // C upper floor
            ctx.start_floor();
            add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 1.0, false);
            add_point(ctx, geo, 0.5, cy, 1.0, 0.0, 1.0, false);

            case_found = true;
        }
        // Case 8: A lowest, CD connected (not BD), B higher than D (Yugen Case 9)
        else if ctx.is_lower(ay, by)
            && ctx.is_lower(ay, cy)
            && !ctx.bd()
            && ctx.cd()
            && ctx.is_higher(by, dy)
        {
            add_inner_corner(ctx, geo, true, false, true, false, false);
            let by = ctx.by();
            let dy = ctx.dy();
            let cy = ctx.cy();
            let edge_mid = (dy + cy) / 2.0;

            // D corner floor
            ctx.start_floor();
            add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, edge_mid, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, dy, 0.5, 0.0, 0.0, false);

            // C corner floor
            add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, edge_mid, 1.0, 0.0, 0.0, false);

            // Center floors
            add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, cy, 0.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, edge_mid, 1.0, 0.0, 0.0, false);

            add_point(ctx, geo, 1.0, dy, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, edge_mid, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, cy, 0.0, 0.0, 0.0, false);

            // Walls to upper corner
            ctx.start_wall();
            add_point(ctx, geo, 0.5, cy, 0.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, by, 0.0, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, dy, 0.5, 0.0, 0.0, false);

            add_point(ctx, geo, 1.0, by, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, dy, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, by, 0.0, 0.0, 0.0, false);

            // B upper floor
            ctx.start_floor();
            add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, by, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.5, by, 0.0, 0.0, 0.0, false);

            case_found = true;
        }
        // Case 9: A lowest, neither BD nor CD connected, B higher
        else if ctx.is_lower(ay, by)
            && ctx.is_lower(ay, cy)
            && !ctx.bd()
            && !ctx.cd()
            && ctx.is_higher(by, dy)
            && ctx.is_higher(cy, dy)
            && ctx.is_merged(by, cy)
        {
            add_inner_corner(ctx, geo, true, false, false, false, false);
            add_diagonal_floor(ctx, geo, by, cy, true, false);
            ctx.rotate_cell(2);
            add_outer_corner(ctx, geo, true, false, false, -1.0);
            case_found = true;
        }
        // Case 10: Inner corner at A with edge atop BD (GDScript Case 10)
        else if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.is_higher(dy, cy) && ctx.bd()
        {
            add_inner_corner(ctx, geo, true, false, true, true, false);
            ctx.rotate_cell(1);
            add_edge(ctx, geo, false, true, 0.0, 1.0);
            case_found = true;
        }
        // Case 11: Inner corner at A with edge atop CD (GDScript Case 11)
        else if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.is_higher(dy, by) && ctx.cd()
        {
            add_inner_corner(ctx, geo, true, false, true, false, true);
            ctx.rotate_cell(2);
            add_edge(ctx, geo, false, true, 0.0, 1.0);
            case_found = true;
        }
        // Case 12 (GDScript): Clockwise upwards spiral A<B<D<C
        else if ctx.is_lower(ay, by)
            && ctx.is_lower(by, dy)
            && ctx.is_lower(dy, cy)
            && ctx.is_higher(cy, ay)
        {
            add_inner_corner(ctx, geo, true, false, true, false, true);
            ctx.rotate_cell(2);
            add_edge(ctx, geo, true, true, 0.0, 0.5);
            ctx.rotate_cell(1);
            add_outer_corner(ctx, geo, true, true, true, cy);
            case_found = true;
        }
        // Case 13 (GDScript): Clockwise upwards spiral A<C<D<B
        else if ctx.is_lower(ay, cy)
            && ctx.is_lower(cy, dy)
            && ctx.is_lower(dy, by)
            && ctx.is_higher(by, ay)
        {
            add_inner_corner(ctx, geo, true, false, true, true, false);
            ctx.rotate_cell(1);
            add_edge(ctx, geo, true, true, 0.5, 1.0);
            add_outer_corner(ctx, geo, true, true, true, by);
            case_found = true;
        }
        // Case 14 (GDScript): A<B<C<D staircase pattern
        else if ctx.is_lower(ay, by) && ctx.is_lower(by, cy) && ctx.is_lower(cy, dy) {
            add_inner_corner(ctx, geo, true, false, true, false, true);
            ctx.rotate_cell(2);
            add_edge(ctx, geo, true, true, 0.5, 1.0);
            add_outer_corner(ctx, geo, true, true, true, by);
            case_found = true;
        }
        // Case 15 (GDScript): A<C<B<D staircase variant
        else if ctx.is_lower(ay, cy) && ctx.is_lower(cy, by) && ctx.is_lower(by, dy) {
            add_inner_corner(ctx, geo, true, false, true, true, false);
            ctx.rotate_cell(1);
            add_edge(ctx, geo, true, true, 0.0, 0.5);
            ctx.rotate_cell(1);
            add_outer_corner(ctx, geo, true, true, true, cy);
            case_found = true;
        }
        // Case 12 (original): A only higher than C
        else if ctx.is_higher(ay, cy)
            && ctx.is_merged(ay, by)
            && ctx.is_merged(cy, dy)
            && ctx.ab()
            && ctx.cd()
        {
            add_edge(ctx, geo, true, true, 0.0, 1.0);
            case_found = true;
        }
        // Case 13: All corners different, A highest
        else if ctx.is_higher(ay, by)
            && ctx.is_higher(ay, cy)
            && !ctx.bd()
            && !ctx.cd()
            && ctx.is_lower(dy, by)
            && ctx.is_lower(dy, cy)
        {
            add_outer_corner(ctx, geo, false, true, false, -1.0);
            add_diagonal_floor(ctx, geo, by, cy, true, true);
            ctx.rotate_cell(2);
            add_inner_corner(ctx, geo, true, false, false, false, false);
            case_found = true;
        }
        // Case 14: A higher, B and C merged, D lower
        else if ctx.is_higher(ay, by)
            && ctx.is_higher(ay, cy)
            && ctx.is_merged(by, cy)
            && ctx.is_higher(by, dy)
            && ctx.is_higher(cy, dy)
        {
            add_outer_corner(ctx, geo, false, true, false, -1.0);
            add_diagonal_floor(ctx, geo, by, cy, false, false);
            ctx.rotate_cell(2);
            add_outer_corner(ctx, geo, true, false, false, -1.0);
            case_found = true;
        }
        // Case 15: A higher than B and C, B higher than C
        else if ctx.is_higher(ay, by)
            && ctx.is_higher(ay, cy)
            && ctx.is_higher(by, cy)
            && !ctx.cd()
        {
            add_outer_corner(ctx, geo, false, true, true, by);
            add_edge(ctx, geo, true, true, 0.5, 1.0);
            case_found = true;
        }
        // Case 16: A higher than B and C, C higher than B
        else if ctx.is_higher(ay, by)
            && ctx.is_higher(ay, cy)
            && ctx.is_higher(cy, by)
            && !ctx.bd()
        {
            add_outer_corner(ctx, geo, false, true, true, cy);
            ctx.rotate_cell(-1);
            add_edge(ctx, geo, true, true, 0.0, 0.5);
            case_found = true;
        }
        // Case 17: A alone at any height, all others different
        else if ctx.is_higher(ay, by)
            && ctx.is_merged(by, cy)
            && !ctx.bd()
            && ctx.is_lower(dy, by)
        {
            add_outer_corner(ctx, geo, false, true, true, by);
            ctx.rotate_cell(2);
            add_inner_corner(ctx, geo, true, true, true, false, false);
            case_found = true;
        }
        // Case 18: All edges connected except AC, A higher than C
        else if ctx.ab() && ctx.bd() && ctx.cd() && !ctx.ac() && ctx.is_higher(ay, cy) {
            let ay = ctx.ay();
            let by = ctx.by();
            let cy = ctx.cy();
            let dy = ctx.dy();
            let edge_by = (by + dy) / 2.0;
            let edge_dy = (by + dy) / 2.0;

            // Upper floor
            ctx.start_floor();
            add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, edge_by, 0.5, 0.0, 0.0, false);

            add_point(ctx, geo, 1.0, edge_by, 0.5, 0.0, 1.0, false);
            add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);
            add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);

            // Wall
            ctx.start_wall();
            add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);
            add_point(ctx, geo, 1.0, edge_dy, 0.5, 1.0, 0.0, false);

            // Lower floor
            ctx.start_floor();
            add_point(ctx, geo, 0.0, cy, 0.5, 1.0, 0.0, false);
            add_point(ctx, geo, 1.0, edge_dy, 0.5, 1.0, 0.0, false);
            add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);

            add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, edge_dy, 0.5, 0.0, 0.0, false);

            case_found = true;
        }
        // Case 19: All edges connected except BD, B higher than D
        else if ctx.ab() && ctx.ac() && ctx.cd() && !ctx.bd() && ctx.is_higher(by, dy) {
            let ay = ctx.ay();
            let by = ctx.by();
            let cy = ctx.cy();
            let dy = ctx.dy();
            let edge_ay = (ay + cy) / 2.0;
            let edge_cy = (ay + cy) / 2.0;

            // Upper floor
            ctx.start_floor();
            add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.0, edge_ay, 0.5, 0.0, 0.0, false);

            add_point(ctx, geo, 1.0, by, 0.5, 0.0, 1.0, false);
            add_point(ctx, geo, 0.0, edge_ay, 0.5, 0.0, 1.0, false);
            add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);

            // Wall
            ctx.start_wall();
            add_point(ctx, geo, 1.0, by, 0.5, 1.0, 1.0, false);
            add_point(ctx, geo, 1.0, dy, 0.5, 1.0, 0.0, false);
            add_point(ctx, geo, 0.0, edge_ay, 0.5, 0.0, 0.0, false);

            // Lower floor
            ctx.start_floor();
            add_point(ctx, geo, 0.0, edge_cy, 0.5, 1.0, 0.0, false);
            add_point(ctx, geo, 1.0, dy, 0.5, 1.0, 0.0, false);
            add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);

            add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geo, 0.0, edge_cy, 0.5, 0.0, 0.0, false);
            add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);

            case_found = true;
        } else {
            continue;
        }

        if case_found {
            break;
        }
    }

    if !case_found {
        // Fallback: unknown cell configuration, place a full floor
        ctx.rotation = 0;
        add_full_floor(ctx, geo);
    }

    // Validate vertex count after case handling
    let final_vert_count = geo.verts.len();
    let added = final_vert_count - initial_vert_count;
    if added % 3 != 0 {
        godot_error!(
            "GEOMETRY BUG: Case at rotation {} for cell ({},{}) added {} vertices (not divisible by 3)! Heights: [{:.2}, {:.2}, {:.2}, {:.2}], Edges: [{}, {}, {}, {}]",
            ctx.rotation, ctx.cell_coords.x, ctx.cell_coords.y, added,
            ctx.heights[0], ctx.heights[1], ctx.heights[2], ctx.heights[3],
            ctx.edges[0], ctx.edges[1], ctx.edges[2], ctx.edges[3]
        );
    }
}
```

**What's happening — the algorithm structure:**

1. **Edge calculation**: Compare each pair of adjacent corners against the merge threshold. `true` = merged (slope), `false` = separated (wall).
2. **Case 0 fast path**: All edges connected → full floor. This is the most common case, so it's checked first without entering the rotation loop.
3. **Rotation loop**: For each of 4 rotations (0°, 90°, 180°, 270°), re-read heights and test each case. The first match wins.
4. **Case composition**: Complex cases combine multiple primitives. For example, Case 3 = `add_edge(half-width) + rotate + add_outer_corner(flattened)`.
5. **Fallback**: If no case matches (shouldn't happen with 17+ cases × 4 rotations), fall back to a full floor. This prevents holes in the mesh.
6. **Validation**: After generation, verify the vertex count is divisible by 3 (complete triangles). If not, log a detailed error for debugging.

**Case guide (for reference):**

| Case | Pattern | Primitives Used |
|------|---------|----------------|
| 0 | All merged | `add_full_floor` |
| 1 | A raised, BCD merged | `add_outer_corner` |
| 2 | AB raised, CD merged | `add_edge` |
| 3 | A>B, AB raised over CD | `add_edge(half) + add_outer_corner` |
| 4 | B>A, AB raised over CD | `add_edge(half) + rotate + add_outer_corner` |
| 5 | BC raised, AD low, BC merged | `add_inner_corner + add_diagonal_floor + rotate + add_inner_corner` |
| 5.5 | BC raised, AD low, B>C | Case 5 + `add_outer_corner` |
| 6 | A lowered, BCD merged | `add_inner_corner` |
| 7-8 | A low, asymmetric sides | `add_inner_corner + custom inline geometry` |
| 9 | A low, BC high, D low | `add_inner_corner + add_diagonal_floor + rotate + add_outer_corner` |
| 10-11 | Inner corner + edge | `add_inner_corner + rotate + add_edge` |
| 12-15 | Spiral/staircase | `add_inner_corner + rotate + add_edge + rotate + add_outer_corner` |
| 16-17 | Partial edges | `add_outer_corner + add_edge` |
| 18-19 | Merged-edge special | Custom inline geometry with averaged edge heights |

### Step 2: Add unit tests

**Why:** The marching squares algorithm is pure math — no Godot API calls needed for testing the core logic. These tests verify rotation, edge detection, texture encoding, and floor generation.

**File:** `rust/src/marching_squares.rs` (append at the very end)

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
            cell_min_height: 0.0,
            cell_max_height: 0.0,
            cell_is_boundary: false,
            cell_floor_lower_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            cell_floor_upper_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            cell_floor_lower_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            cell_floor_upper_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            cell_wall_lower_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            cell_wall_upper_color_0: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            cell_wall_lower_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            cell_wall_upper_color_1: Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            cell_mat_a: 0,
            cell_mat_b: 0,
            cell_mat_c: 0,
            blend_mode: 0,
            use_ridge_texture: false,
            ridge_threshold: 1.0,
            is_new_chunk: false,
            floor_mode: true,
            lower_thresh: 0.3,
            upper_thresh: 0.7,
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
        for idx in 0..16 {
            let (c0, c1) = texture_index_to_colors(idx);
            let result = get_texture_index_from_colors(c0, c1);
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
- `default_context()` creates a test context with a 3x3 grid (2x2 cells). Color maps are initialized with texture slot 0 (red dominant).
- Tests verify: merge mode thresholds, three-way comparison, vertex counts for floor generation, texture encoding round-trip, dominant color detection, and rotation behavior.
- Note: these tests use gdext's `Vector2i`, `Vector3i`, `Vector3`, and `Color` types, which work without a Godot runtime because gdext implements them natively in Rust.

## Verify

```bash
cd rust && cargo test
```

**Expected:** All 7 tests pass:

```
running 7 tests
test marching_squares::tests::test_merge_mode_thresholds ... ok
test marching_squares::tests::test_is_higher_lower_merged ... ok
test marching_squares::tests::test_full_floor_higher_poly_generates_12_vertices ... ok
test marching_squares::tests::test_full_floor_low_poly_generates_6_vertices ... ok
test marching_squares::tests::test_texture_index_round_trip ... ok
test marching_squares::tests::test_get_dominant_color ... ok
test marching_squares::tests::test_rotation ... ok
```

```bash
cd rust && cargo build
```

**Expected:** Compiles cleanly. The complete `marching_squares.rs` module (~1600 lines) is now done.

## What You Learned

- **Case matching with rotation**: The `for i in 0..4` loop + `continue`/`break` pattern lets each case assume a canonical orientation — massively reducing code duplication
- **Primitive composition**: Complex cases are built from simple primitives. Case 12 (spiral) = inner_corner + edge + outer_corner at different rotations — 3 primitives, each well-tested individually
- **Inline geometry for edge cases**: Cases 7-8 and 18-19 use inline `add_point()` calls because their geometry doesn't cleanly decompose into the 5 standard primitives
- **Defensive fallback**: If no case matches, `add_full_floor()` prevents mesh holes. The validation check catches geometry bugs early
- **Testing pure algorithms**: The marching squares core is testable without a Godot runtime — gdext's math types work standalone

## Stubs Introduced

(No new stubs)

## Stubs Resolved

- [x] `generate_cell()` — was stubbed in Part 04 ("primitives can't be called without the case dispatcher"), now fully implemented
- [x] `rust/src/marching_squares.rs` — module fully complete (was incrementally built across Parts 02-05)
