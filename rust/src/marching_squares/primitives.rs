use super::cell_context::CellContext;
use super::types::CellGeometry;
use super::vertex::add_point;

/// Case 1: Outer corner where A is the raised corner.
pub fn add_outer_corner(
    ctx: &mut CellContext,
    geometry: &mut CellGeometry,
    floor_below: bool,
    floor_above: bool,
    flatten_bottom: bool,
    bottom_height: f32,
) {
    let (ay, by, cy, dy) = ctx.corner_heights();
    let edge_by = if flatten_bottom { bottom_height } else { by };
    let edge_cy = if flatten_bottom { bottom_height } else { cy };

    if floor_above {
        ctx.start_floor();
        add_point(ctx, geometry, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, ay, 0.0, 0.0, 1.0, false);
        add_point(ctx, geometry, 0.0, ay, 0.5, 0.0, 1.0, false);
    }

    // Walls
    ctx.start_wall();
    add_point(ctx, geometry, 0.0, edge_cy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, ay, 0.5, 0.0, 1.0, false);
    add_point(ctx, geometry, 0.5, edge_by, 0.0, 1.0, 0.0, false);

    add_point(ctx, geometry, 0.5, ay, 0.0, 1.0, 1.0, false);
    add_point(ctx, geometry, 0.5, edge_by, 0.0, 1.0, 0.0, false);
    add_point(ctx, geometry, 0.0, ay, 0.5, 0.0, 1.0, false);

    if floor_below {
        ctx.start_floor();
        add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);

        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 0.5, 1.0, 0.0, false);
        add_point(ctx, geometry, 0.5, by, 0.0, 1.0, 0.0, false);

        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, by, 0.0, 1.0, 0.0, false);
    }
}

/// Generate case 0: full floor with all edges connected
pub fn add_full_floor(ctx: &mut CellContext, geometry: &mut CellGeometry) {
    ctx.start_floor();
    let (ay, by, cy, dy) = ctx.corner_heights();

    if ctx.higher_poly_floors {
        let ey = (ay + by + cy + dy) / 4.0;

        // Triangle 1: A-B-E
        add_point(ctx, geometry, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, true);

        // Triangle 2: B-D-E
        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, true);

        // Triangle 3: D-C-E
        add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, true);

        // Triangle 4: C-A-E
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, true);
    } else {
        // Simple 2-triangle floor
        add_point(ctx, geometry, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);

        add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
    }
}

/// Case 2: Edge where AB is the raised edge.
pub fn add_edge(
    ctx: &mut CellContext,
    geometry: &mut CellGeometry,
    floor_below: bool,
    floor_above: bool,
    a_x: f32,
    b_x: f32,
) {
    let (ay, by, cy, dy) = ctx.corner_heights();
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

        add_point(ctx, geometry, a_x, edge_ay, 0.0, uv_a, 0.0, false);
        add_point(ctx, geometry, b_x, edge_by, 0.0, uv_b, 0.0, false);
        add_point(ctx, geometry, 0.0, edge_ay, 0.5, uv_left, 1.0, false);

        add_point(ctx, geometry, 1.0, edge_by, 0.5, uv_right, 1.0, false);
        add_point(ctx, geometry, 0.0, edge_ay, 0.5, uv_left, 1.0, false);
        add_point(ctx, geometry, b_x, edge_by, 0.0, uv_b, 0.0, false);
    }

    // Wall
    ctx.start_wall();
    add_point(ctx, geometry, 0.0, edge_cy, 0.5, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, edge_ay, 0.5, 0.0, 1.0, false);
    add_point(ctx, geometry, 1.0, edge_dy, 0.5, 1.0, 0.0, false);

    add_point(ctx, geometry, 1.0, edge_by, 0.5, 1.0, 1.0, false);
    add_point(ctx, geometry, 1.0, edge_dy, 0.5, 1.0, 0.0, false);
    add_point(ctx, geometry, 0.0, edge_ay, 0.5, 0.0, 1.0, false);

    if floor_below {
        ctx.start_floor();
        add_point(ctx, geometry, 0.0, cy, 0.5, 1.0, 0.0, false);
        add_point(ctx, geometry, 1.0, dy, 0.5, 1.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);

        add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, dy, 0.5, 1.0, 0.0, false);
    }
}

/// Inner corner where A is the lowered corner
pub fn add_inner_corner(
    ctx: &mut CellContext,
    geometry: &mut CellGeometry,
    lower_floor: bool,
    full_upper_floor: bool,
    flatten: bool,
    bd_floor: bool,
    cd_floor: bool,
) {
    let (ay, by, cy, dy) = ctx.corner_heights();
    let corner_by = if flatten { by.min(cy) } else { by };
    let corner_cy = if flatten { by.min(cy) } else { cy };

    if lower_floor {
        ctx.start_floor();
        add_point(ctx, geometry, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, ay, 0.0, 1.0, 0.0, false);
        add_point(ctx, geometry, 0.0, ay, 0.5, 1.0, 0.0, false);
    }

    ctx.start_wall();
    add_point(ctx, geometry, 0.0, ay, 0.5, 1.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, corner_cy, 0.5, 1.0, 1.0, false);

    add_point(ctx, geometry, 0.5, corner_by, 0.0, 0.0, 1.0, false);
    add_point(ctx, geometry, 0.0, corner_cy, 0.5, 1.0, 1.0, false);
    add_point(ctx, geometry, 0.5, ay, 0.0, 0.0, 0.0, false);

    ctx.start_floor();
    if full_upper_floor {
        add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, corner_cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, corner_by, 0.0, 0.0, 0.0, false);

        add_point(ctx, geometry, 0.0, corner_cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, corner_cy, 0.5, 0.0, 1.0, false);
        add_point(ctx, geometry, 0.5, corner_by, 0.0, 0.0, 1.0, false);

        add_point(ctx, geometry, 1.0, corner_by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, corner_cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, corner_by, 0.0, 0.0, 1.0, false);
    }

    if cd_floor {
        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, by, 0.5, 1.0, 1.0, false);
        add_point(ctx, geometry, 0.5, by, 0.0, 0.0, 1.0, false);

        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, by, 0.5, 1.0, -1.0, false);
        add_point(ctx, geometry, 0.0, by, 0.5, 1.0, 1.0, false);
    }

    if bd_floor {
        add_point(ctx, geometry, 0.0, cy, 0.5, 0.0, 1.0, false);
        add_point(ctx, geometry, 0.5, cy, 0.0, 1.0, 1.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);

        add_point(ctx, geometry, 0.5, cy, 1.0, 1.0, -1.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, cy, 0.0, 1.0, 1.0, false);
    }
}

/// Diagonal floor connecting B and C corners.
pub fn add_diagonal_floor(
    ctx: &mut CellContext,
    geometry: &mut CellGeometry,
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

    add_point(ctx, geometry, 1.0, b_y, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, c_y, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, b_y, 0.0, a_uv_x, a_uv_y, false);

    add_point(ctx, geometry, 0.0, c_y, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, c_y, 0.5, a_uv_x, a_uv_y, false);
    add_point(ctx, geometry, 0.5, b_y, 0.0, a_uv_x, a_uv_y, false);

    add_point(ctx, geometry, 1.0, b_y, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 1.0, b_y, 0.5, d_uv_x, d_uv_y, false);
    add_point(ctx, geometry, 0.0, c_y, 1.0, 0.0, 0.0, false);

    add_point(ctx, geometry, 0.0, c_y, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 1.0, b_y, 0.5, d_uv_x, d_uv_y, false);
    add_point(ctx, geometry, 0.5, c_y, 1.0, d_uv_x, d_uv_y, false);
}
