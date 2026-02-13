use godot::prelude::*;

use super::cell_context::CellContext;
use super::primitives::*;
use super::types::CellGeometry;
use super::vertex::add_point;

/// Generate geometry for a single cell based on the 17-case marching squares algorithm.
pub fn generate_cell(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let initial_vert_count = geo.verts.len();

    let ay = ctx.ay();
    let by = ctx.by();
    let cy = ctx.cy();
    let dy = ctx.dy();

    // Edge connectivitys: true = slop (merged), false = wall (separated)
    ctx.edges = [
        (ay - by).abs() < ctx.config.merge_threshold, // AB (top)
        (by - dy).abs() < ctx.config.merge_threshold, // BD (right)
        (cy - dy).abs() < ctx.config.merge_threshold, // CD (bottom)
        (ay - cy).abs() < ctx.config.merge_threshold, // AC (left)
    ];

    // Pre-compute cell color state
    ctx.color_state.min_height = ay.min(by).min(cy).min(dy);
    ctx.color_state.max_height = ay.max(by).max(cy).max(dy);
    ctx.color_state.is_boundary =
        (ctx.color_state.max_height - ctx.color_state.min_height) > ctx.config.merge_threshold;

    ctx.compute_profiles();

    ctx.calculate_cell_material_pair();
    if ctx.color_state.is_boundary {
        ctx.calculate_boundary_colors();
    }

    // Case 0: all edges connected --> full floor (fast path)
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
        ctx.rotation = 0;
        add_full_floor(ctx, geo);
    };

    validate_geometry(ctx, geo, initial_vert_count);
}

fn match_case(ctx: &CellContext) -> Option<fn(&mut CellContext, geo: &mut CellGeometry)> {
    let (ay, by, cy, dy) = (ctx.ay(), ctx.by(), ctx.cy(), ctx.dy());

    // Case 1: A raised, opposite corners merged.
    if ctx.is_higher(ay, by) && ctx.is_higher(ay, cy) && ctx.bd() && ctx.cd() {
        return Some(case_1_outer_corner);
    }

    // Case 2: AB edge raised above CD
    if ctx.is_higher(ay, cy) && ctx.is_higher(by, dy) && ctx.bd() && ctx.cd() {
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
        return Some(case_5_5_double_inner_corner);
    }

    // Case 6: A is the lowest corner, BCD merged
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

    // Case 8: A loest, CD connected, B higher than D
    if ctx.is_lower(ay, by)
        && ctx.is_lower(ay, cy)
        && !ctx.bd()
        && ctx.cd()
        && ctx.is_higher(by, dy)
    {
        return Some(case_8_inner_asymmetric_cd);
    }

    // Case 9: A lowest, neither BD or CD connected, BC merged
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
    if ctx.is_lower(ay, by) && ctx.is_lower(by, dy) && ctx.is_lower(dy, cy) && ctx.is_higher(cy, ay)
    {
        return Some(case_12_spiral_clockwise);
    }

    // Case 13: Counter-clockwise spiral A<C<D<B
    if ctx.is_lower(ay, cy) && ctx.is_lower(cy, dy) && ctx.is_lower(dy, by) && ctx.is_higher(by, ay)
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

    // Case 16: Degenerate merged edge (same geometry as Case 2)
    if ctx.is_higher(ay, cy)
        && ctx.is_merged(ay, by)
        && ctx.is_merged(cy, dy)
        && ctx.ab()
        && ctx.cd()
    {
        return Some(case_2_edge);
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
    if ctx.is_higher(ay, by) && ctx.is_higher(ay, cy) && ctx.is_higher(by, cy) && !ctx.cd() {
        return Some(case_19_outer_partial_edge_b);
    }

    // Case 20: A higher, C higher than B, BD not connected
    if ctx.is_higher(ay, by) && ctx.is_higher(ay, cy) && ctx.is_higher(cy, by) && !ctx.bd() {
        return Some(case_20_outer_partial_edge_c);
    }

    // Case 21: A higher, BC merged, D lowest
    if ctx.is_higher(ay, by) && ctx.is_merged(by, cy) && !ctx.bd() && ctx.is_lower(dy, by) {
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

/// Case 1: One raised corner (outer corner)
fn case_1_outer_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_outer_corner(ctx, geo, true, true, false, -1.0);
}

/// Case 2: Raised edge
fn case_2_edge(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_edge(ctx, geo, true, true, 0.0, 1.0);
}

/// Case 3: Half-width edge with A outer corner above
fn case_3_edge_a_outer_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_edge(ctx, geo, true, true, 0.5, 1.0);
    add_outer_corner(ctx, geo, false, true, true, by);
}

/// Case 4: Half-width edge with B outer corner above.
fn case_4_edge_b_outer_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let edge_floor = ctx.ay().min(ctx.by());
    add_edge(ctx, geo, true, true, 0.0, 0.5);
    ctx.rotate_cell(1);
    add_outer_corner(ctx, geo, false, true, true, edge_floor);
}

/// Case 5: To inner corners at A and D with diagonal floor bridge
fn case_5_double_inner_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    add_inner_corner(ctx, geo, true, false, false, false, false);
    add_diagonal_floor(ctx, geo, by, cy, true, true, None, None);
    ctx.rotate_cell(2);
    add_inner_corner(ctx, geo, true, false, false, false, false);
}

/// Case 5.5: Like case 5 but B higher than C - ads outer corner at B.
fn case_5_5_double_inner_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    let flat = by.min(cy);
    add_inner_corner(ctx, geo, true, false, true, false, false);
    add_diagonal_floor(ctx, geo, by, cy, true, true, Some(flat), Some(flat));
    ctx.rotate_cell(2);
    add_inner_corner(ctx, geo, true, false, true, false, false);
    ctx.rotate_cell(-1);
    add_outer_corner(ctx, geo, false, true, false, -1.0);
}

/// Case 6: One lowered corner (inner corner)
fn case_6_inner_corner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, true, false, false, false)
}

/// Case 7: A lowest, BD connected (not CD), C higher than D.
fn case_7_inner_asymmetric_bd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, false, true);

    let (_, _, cy, dy) = ctx.corner_heights();
    let mid_ac = ctx.ac_height(0.5, true);
    let mid_bd = ctx.bd_height(0.5, true);
    let cd_upper = ctx.cd_height(0.5, true);
    let cd_lower = ctx.cd_height(0.5, false);

    // Floor from BD_mid to D to CD_lower
    ctx.start_floor();
    add_point(ctx, geo, 1.0, mid_bd, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, cd_lower, 1.0, 1.0, 0.0, false);

    // Connecting floor: AC_mid to BD_mid to CD_lower
    add_point(ctx, geo, 0.0, mid_ac, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, mid_bd, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, cd_lower, 1.0, 1.0, 0.0, false);

    // Wall at CD midpoint
    ctx.start_wall();
    add_point(ctx, geo, 0.5, cd_lower, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, mid_ac, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 0.5, cd_upper, 1.0, 0.0, 1.0, false);

    // C upper floor
    ctx.start_floor();
    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, mid_ac, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 0.5, cd_upper, 1.0, 0.0, 1.0, false);
}

/// Case 8: A lowest, CD connected (not BD), B higher than D.
fn case_8_inner_asymmetric_cd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, true, false);

    let (_, by, _, dy) = ctx.corner_heights();
    let mid_ab = ctx.ab_height(0.5, true);
    let bd_upper = ctx.bd_height(0.5, true);
    let bd_lower = ctx.bd_height(0.5, false);
    let cd_mid = ctx.cd_height(0.5, true);

    // Floor from AB_mid toward D: AB_mid - D - CD_mid
    ctx.start_floor();
    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, cd_mid, 1.0, 1.0, 0.0, false);

    // Floor: AB_mid - BD_lower - D
    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, bd_lower, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);

    // BD wall
    ctx.start_wall();
    add_point(ctx, geo, 1.0, bd_lower, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 1.0, false);
    add_point(ctx, geo, 1.0, bd_upper, 0.5, 0.0, 1.0, false);

    // B upper floor
    ctx.start_floor();
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 1.0, false);
    add_point(ctx, geo, 1.0, bd_upper, 0.5, 0.0, 1.0, false);
}

/// Case 9: Inner corner at A, diagonal floor, outer corner at D
fn case_9_inner_diagonal_outer(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    add_inner_corner(ctx, geo, true, false, false, false, false);
    add_diagonal_floor(ctx, geo, by, cy, true, false, None, None);
    ctx.rotate_cell(2);
    add_outer_corner(ctx, geo, true, false, false, -1.0);
}

/// Case 10: Inner corner at A with edge atop BD.
fn case_10_inner_corner_edge_bd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, true, false);

    // Bridge floor: connects inner corner's lower region to the edge wall bottom.
    // Uses AC_upper (wall-top height) so boundary edge C→AC_upper matches the
    // canonical decomposition that adjacent cells expect.
    let (ay, _, cy, _) = ctx.corner_heights();
    let mid_cd_lower = ctx.cd_height(0.5, false);
    let mid_ac_upper = ctx.ac_height(0.5, true);
    ctx.start_floor();
    // Tri 1: AB_mid → C → CD_mid_lower
    add_point(ctx, geo, 0.5, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_cd_lower, 1.0, 1.0, 0.0, false);
    // Tri 2: C → AB_mid → AC_upper (canonical AC boundary edge)
    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, mid_ac_upper, 0.5, 0.0, 0.0, false);

    ctx.rotate_cell(1);
    add_edge(ctx, geo, false, true, 0.0, 1.0);
}

/// Case 11: Inner corner at A with edge atop CD.
fn case_11_inner_corner_edge_cd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, false, true);

    // Bridge floor: connects inner corner's lower region to the edge wall bottom.
    // Split the AB boundary edge at the midpoint for canonical cross-cell matching.
    let (ay, by, _, _) = ctx.corner_heights();
    let mid_bd_lower = ctx.bd_height(0.5, false);
    let mid_ab_upper = ctx.ab_height(0.5, true);
    let mid_ab_lower = ctx.ab_height(0.5, false);
    ctx.start_floor();
    // T1: AC_mid → B → BD_lower (unchanged)
    add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, mid_bd_lower, 0.5, 1.0, 0.0, false);
    // T2b: AB_mid_upper → B → AC_mid (upper half of AB)
    add_point(ctx, geo, 0.5, mid_ab_upper, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 0.0, false);
    // T2c: AB_mid_lower → AB_mid_upper → AC_mid (wall at AB midpoint)
    if (mid_ab_upper - mid_ab_lower).abs() > 1e-5 {
        add_point(ctx, geo, 0.5, mid_ab_lower, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.5, mid_ab_upper, 0.0, 0.0, 0.0, false);
        add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 0.0, false);
    }

    ctx.rotate_cell(2);
    add_edge(ctx, geo, false, true, 0.0, 1.0);
}

/// Case 12: Clockwise spiral A<B<D<C.
fn case_12_spiral_clockwise(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, false, true);
    ctx.rotate_cell(2);
    let edge_floor = ctx.ay().min(ctx.by());
    add_edge(ctx, geo, true, true, 0.0, 0.5);
    ctx.rotate_cell(1);
    add_outer_corner(ctx, geo, true, true, true, edge_floor);
}

/// Case 13: Counter-clockwise spiral A<C<D<B.
fn case_13_spiral_counter(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, true, false);
    ctx.rotate_cell(1);
    let edge_floor = ctx.ay().min(ctx.by());
    add_edge(ctx, geo, true, true, 0.5, 1.0);
    add_outer_corner(ctx, geo, true, true, true, edge_floor);
}

/// Case 14: Staircase A<B<C<D.
fn case_14_staircase_abcd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, false, true);
    ctx.rotate_cell(2);
    let edge_floor = ctx.ay().min(ctx.by());
    add_edge(ctx, geo, true, true, 0.5, 1.0);
    add_outer_corner(ctx, geo, true, true, true, edge_floor);
}

/// Case 15: Staircase A<C<B<D.
fn case_15_staircase_acbd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    add_inner_corner(ctx, geo, true, false, true, true, false);
    ctx.rotate_cell(1);
    let edge_floor = ctx.ay().min(ctx.by());
    add_edge(ctx, geo, true, true, 0.0, 0.5);
    ctx.rotate_cell(1);
    add_outer_corner(ctx, geo, true, true, true, edge_floor);
}

/// Case 17: A highest, D lowest, all corners different.
fn case_17_outer_diagonal_inner(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    add_outer_corner(ctx, geo, false, true, false, -1.0);
    add_diagonal_floor(ctx, geo, by, cy, true, true, None, None);
    ctx.rotate_cell(2);
    add_inner_corner(ctx, geo, true, false, false, false, false);
}

/// Case 18: A highest, BC merged, D lowest.
fn case_18_outer_diagonal_outer(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let (by, cy) = (ctx.by(), ctx.cy());
    add_outer_corner(ctx, geo, false, true, false, -1.0);
    add_diagonal_floor(ctx, geo, by, cy, false, false, None, None);
    ctx.rotate_cell(2);
    add_outer_corner(ctx, geo, true, false, false, -1.0);
}

/// Case 19: A higher, B higher than C, CD not connected.
fn case_19_outer_partial_edge_b(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_outer_corner(ctx, geo, false, true, true, by);
    add_edge(ctx, geo, true, true, 0.5, 1.0);
}

/// Case 20: A higher, C higher than B, BD not connected.
fn case_20_outer_partial_edge_c(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let cy = ctx.cy();
    add_outer_corner(ctx, geo, false, true, true, cy);
    ctx.rotate_cell(-1);
    add_edge(ctx, geo, true, true, 0.0, 0.5);
}

/// Case 21: A raised, BC merged, D lowest.
fn case_21_outer_inner_composite(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let by = ctx.by();
    add_outer_corner(ctx, geo, false, true, true, by);
    ctx.rotate_cell(2);
    add_inner_corner(ctx, geo, true, true, true, false, false);
}

/// Case 22: All edges connected except AC. A higher than C.
fn case_22_single_wall_ac(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let ay = ctx.ay();
    let by = ctx.by();
    let cy = ctx.cy();
    let dy = ctx.dy();

    // Profile heights for BD midpoint (merged boundary)
    let mid_bd = ctx.bd_height(0.5, true);
    let mid_ab = ctx.ab_height(0.5, true);
    let mid_cd = ctx.cd_height(0.5, true);

    // Upper floor — split AB boundary at midpoint
    ctx.start_floor();
    add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);

    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, mid_bd, 0.5, 0.0, 0.0, false);

    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, mid_bd, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);

    // Wall
    ctx.start_wall();
    add_point(ctx, geo, 0.0, cy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, ay, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 1.0, mid_bd, 0.5, 1.0, 0.0, false);

    // Lower floor — split CD boundary at midpoint
    ctx.start_floor();
    add_point(ctx, geo, 0.0, cy, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 1.0, mid_bd, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_cd, 1.0, 0.0, 0.0, false);

    add_point(ctx, geo, 0.0, cy, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_cd, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);

    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_cd, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, mid_bd, 0.5, 0.0, 0.0, false);
}

/// Case 23: All edges connected except BD. B higher than D.
fn case_23_single_wall_bd(ctx: &mut CellContext, geo: &mut CellGeometry) {
    let ay = ctx.ay();
    let by = ctx.by();
    let cy = ctx.cy();
    let dy = ctx.dy();

    // Profile heights for AC midpoint (merged boundary)
    let mid_ac = ctx.ac_height(0.5, true);
    let mid_ab = ctx.ab_height(0.5, true);
    let mid_cd = ctx.cd_height(0.5, true);

    // Upper floor — split AB boundary at midpoint
    ctx.start_floor();
    add_point(ctx, geo, 0.0, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, mid_ac, 0.5, 0.0, 0.0, false);

    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, by, 0.5, 0.0, 1.0, false);

    add_point(ctx, geo, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, by, 0.5, 0.0, 1.0, false);
    add_point(ctx, geo, 0.0, mid_ac, 0.5, 0.0, 1.0, false);

    // Wall
    ctx.start_wall();
    add_point(ctx, geo, 1.0, by, 0.5, 1.0, 1.0, false);
    add_point(ctx, geo, 1.0, dy, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 0.0, mid_ac, 0.5, 0.0, 0.0, false);

    // Lower floor — split CD boundary at midpoint
    ctx.start_floor();
    add_point(ctx, geo, 0.0, mid_ac, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_cd, 1.0, 0.0, 0.0, false);

    add_point(ctx, geo, 0.0, mid_ac, 0.5, 1.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_cd, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.0, cy, 1.0, 0.0, 0.0, false);

    add_point(ctx, geo, 1.0, dy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geo, 1.0, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geo, 0.5, mid_cd, 1.0, 0.0, 0.0, false);
}

fn validate_geometry(ctx: &CellContext, geo: &CellGeometry, initial_vert_count: usize) {
    let added = geo.verts.len() - initial_vert_count;
    if added % 3 != 0 {
        godot_error!(
            "GEOMETRY BUG: Cell ({},{}) added {} vertices (not
  divisible by 3). \
               Heights: [{:.2}, {:.2}, {:.2}, {:.2}], Edges: [{}, {}, {},
   {}]",
            ctx.cell_coords.x,
            ctx.cell_coords.y,
            added,
            ctx.heights[0],
            ctx.heights[1],
            ctx.heights[2],
            ctx.heights[3],
            ctx.edges[0],
            ctx.edges[1],
            ctx.edges[2],
            ctx.edges[3]
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::marching_squares::get_dominant_color;

    use super::super::cell_context::*;
    use super::super::types::*;
    use super::*;

    fn default_context(dim_x: i32, dim_z: i32) -> CellContext {
        CellContext::test_default(dim_x, dim_z)
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
    fn test_full_floor_higher_poly_generates_24_vertices() {
        let mut ctx = default_context(3, 3);
        ctx.heights = [0.0, 0.0, 0.0, 0.0];
        ctx.config.higher_poly_floors = true;
        let mut geo = CellGeometry::default();
        add_full_floor(&mut ctx, &mut geo);
        assert_eq!(geo.verts.len(), 24); // 8 triangles with midpoint splits
    }

    #[test]
    fn test_full_floor_low_poly_generates_24_vertices() {
        let mut ctx = default_context(3, 3);
        ctx.heights = [0.0, 0.0, 0.0, 0.0];
        ctx.config.higher_poly_floors = false;
        let mut geo = CellGeometry::default();
        add_full_floor(&mut ctx, &mut geo);
        assert_eq!(geo.verts.len(), 24); // same structure, different interior flag
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
        assert_eq!(ctx.ay(), 2.0);
        assert_eq!(ctx.by(), 3.0);
        assert_eq!(ctx.dy(), 4.0);
        assert_eq!(ctx.cy(), 1.0);
    }
}
