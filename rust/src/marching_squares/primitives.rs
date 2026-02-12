use super::cell_context::CellContext;
use super::types::CellGeometry;
use super::vertex::add_point;

/// Case 1: Outer corner where A is the raised corner.
pub fn add_outer_corner(
    ctx: &mut CellContext,
    geometry: &mut CellGeometry,
    floor_below: bool,
    floor_above: bool,
    _flatten_bottom: bool,
    _bottom_height: f32,
) {
    let (ay, by, cy, dy) = ctx.corner_heights();

    // Boundary midpoint heights from profiles (A is raised, midpoints are below A)
    let mid_ab = ctx.ab_height(0.5, false); // (0.5, ?, 0.0) on AB, lower side
    let mid_ac = ctx.ac_height(0.5, false); // (0.0, ?, 0.5) on AC, lower side

    // Upper profile heights at midpoints — for merged boundaries these interpolate
    // between corner heights (giving correct slope); for walled boundaries these
    // equal max(h1,h2) = ay (A is the raised corner), so no visible change.
    let mid_ab_upper = ctx.ab_height(0.5, true);
    let mid_ac_upper = ctx.ac_height(0.5, true);

    if floor_above {
        ctx.start_floor();
        add_point(ctx, geometry, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_ab_upper, 0.0, 0.0, 1.0, false);
        add_point(ctx, geometry, 0.0, mid_ac_upper, 0.5, 0.0, 1.0, false);
    }

    // Walls — skip degenerate triangles when a boundary is merged (upper == lower)
    let ac_walled = (mid_ac_upper - mid_ac).abs() > 1e-5;
    let ab_walled = (mid_ab_upper - mid_ab).abs() > 1e-5;

    if ac_walled || ab_walled {
        ctx.start_wall();
        if mid_ab >= mid_ac {
            if ac_walled {
                add_point(ctx, geometry, 0.0, mid_ac, 0.5, 0.0, 0.0, false);
                add_point(ctx, geometry, 0.0, mid_ac_upper, 0.5, 0.0, 1.0, false);
                add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 0.0, false);
            }
            if ab_walled {
                add_point(ctx, geometry, 0.5, mid_ab_upper, 0.0, 1.0, 1.0, false);
                add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 0.0, false);
                add_point(ctx, geometry, 0.0, mid_ac_upper, 0.5, 0.0, 1.0, false);
            }
        } else {
            if ac_walled {
                add_point(ctx, geometry, 0.0, mid_ac, 0.5, 0.0, 0.0, false);
                add_point(ctx, geometry, 0.0, mid_ac_upper, 0.5, 0.0, 1.0, false);
                add_point(ctx, geometry, 0.5, mid_ab_upper, 0.0, 1.0, 1.0, false);
            }
            if ab_walled {
                add_point(ctx, geometry, 0.0, mid_ac, 0.5, 0.0, 0.0, false);
                add_point(ctx, geometry, 0.5, mid_ab_upper, 0.0, 1.0, 1.0, false);
                add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 0.0, false);
            }
        }
    }

    // Corner cap: closes the wall bottom edge when floor_below=false.
    // When both walled, use the diagonal direction (mid_ab >= mid_ac) to pick AC or AB cap.
    // When only one walled, cap closes that side's wall.
    if !floor_below && (ac_walled || ab_walled) {
        ctx.start_floor();
        let use_ac_cap = if ac_walled && ab_walled {
            mid_ab >= mid_ac
        } else {
            ac_walled
        };
        if use_ac_cap {
            add_point(ctx, geometry, 0.0, mid_ac_upper, 0.5, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.0, mid_ac, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 0.0, false);
        } else {
            add_point(ctx, geometry, 0.5, mid_ab_upper, 0.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.0, mid_ac, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 0.0, false);
        }
    }

    if floor_below {
        ctx.start_floor();
        // Split BD and CD midpoints: each half uses the height matching its corner
        let mid_bd_b = ctx.bd_height(0.5, by >= dy); // B-side of BD
        let mid_bd_d = ctx.bd_height(0.5, dy >= by); // D-side of BD
        let mid_cd_c = ctx.cd_height(0.5, cy >= dy); // C-side of CD
        let mid_cd_d = ctx.cd_height(0.5, dy >= cy); // D-side of CD

        // B → BD_mid_b → C (B-side of BD, flat at B level if walled)
        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, mid_bd_b, 0.5, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);

        // BD_mid_d → D → CD_mid_d (D-side of BD and CD, flat at D level if walled)
        add_point(ctx, geometry, 1.0, mid_bd_d, 0.5, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 0.0, 0.0, false);

        // BD_mid_d → CD_mid_c → C (connects D-side midpoints to C)
        add_point(ctx, geometry, 1.0, mid_bd_d, 0.5, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);

        // BD_mid_d → CD_mid_d → CD_mid_c (connects midpoints, only if CD walled)
        if (mid_cd_c - mid_cd_d).abs() > 1e-5 {
            add_point(ctx, geometry, 1.0, mid_bd_d, 0.5, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 0.0, 0.0, false);
        }

        // BD_mid_b → BD_mid_d → C (BD wall triangle, only if BD walled)
        if (mid_bd_b - mid_bd_d).abs() > 1e-5 {
            add_point(ctx, geometry, 1.0, mid_bd_b, 0.5, 0.0, 0.0, false);
            add_point(ctx, geometry, 1.0, mid_bd_d, 0.5, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        }

        // Connecting triangles from wall bottom to corners
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, mid_ac, 0.5, 1.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 0.0, false);

        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 0.0, false);
    }
}

/// Generate case 0: full floor with all edges connected.
/// Uses 8-triangle fan from center E, splitting every boundary at its midpoint
/// so that adjacent cells always produce matching half-edge decompositions.
pub fn add_full_floor(ctx: &mut CellContext, geometry: &mut CellGeometry) {
    ctx.start_floor();
    let (ay, by, cy, dy) = ctx.corner_heights();
    let ey = (ay + by + cy + dy) / 4.0;
    let interior = ctx.higher_poly_floors;

    // Boundary midpoints (for merged boundaries, is_upper doesn't matter)
    let mid_ab = ctx.ab_height(0.5, true);
    let mid_bd = ctx.bd_height(0.5, true);
    let mid_cd = ctx.cd_height(0.5, true);
    let mid_ac = ctx.ac_height(0.5, true);

    // AB boundary: A → mid_AB → B
    add_point(ctx, geometry, 0.0, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, interior);

    add_point(ctx, geometry, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, interior);

    // BD boundary: B → mid_BD → D
    add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 1.0, mid_bd, 0.5, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, interior);

    add_point(ctx, geometry, 1.0, mid_bd, 0.5, 0.0, 0.0, false);
    add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, interior);

    // CD boundary: D → mid_CD → C
    add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, mid_cd, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, interior);

    add_point(ctx, geometry, 0.5, mid_cd, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, interior);

    // AC boundary: C → mid_AC → A
    add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, mid_ac, 0.5, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, interior);

    add_point(ctx, geometry, 0.0, mid_ac, 0.5, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ey, 0.5, 0.0, 0.0, interior);
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
    let (_ay, _by, cy, dy) = ctx.corner_heights();

    // AB boundary: heights at (a_x, ?, 0) and (b_x, ?, 0)
    // At corners these return the corner height; at midpoints the floor level
    let va_y = ctx.ab_height(a_x, false);
    let vb_y = ctx.ab_height(b_x, false);

    // AC midpoint (0.0, ?, 0.5): upper (wall top / raised floor) and lower (wall bottom / lower floor)
    let ac_top = ctx.ac_height(0.5, true);
    let ac_bot = ctx.ac_height(0.5, false);

    // BD midpoint (1.0, ?, 0.5): upper and lower
    let bd_top = ctx.bd_height(0.5, true);
    let bd_bot = ctx.bd_height(0.5, false);

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

        let full_width = (a_x - 0.0).abs() < 1e-5 && (b_x - 1.0).abs() < 1e-5;
        if full_width {
            // Split AB boundary at midpoint for cross-cell matching
            let mid_ab = ctx.ab_height(0.5, false);
            // A → mid_AB → AC_top
            add_point(ctx, geometry, 0.0, va_y, 0.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.0, ac_top, 0.5, uv_left, 1.0, false);
            // mid_AB → B → BD_top
            add_point(ctx, geometry, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 1.0, vb_y, 0.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 1.0, bd_top, 0.5, uv_right, 1.0, false);
            // mid_AB → BD_top → AC_top
            add_point(ctx, geometry, 0.5, mid_ab, 0.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 1.0, bd_top, 0.5, uv_right, 1.0, false);
            add_point(ctx, geometry, 0.0, ac_top, 0.5, uv_left, 1.0, false);
        } else {
            // T1: left triangle — split AC boundary only when AC is walled
            if (a_x).abs() < 1e-5 && (ac_top - ac_bot).abs() > 1e-5 && (ac_top - va_y).abs() > 1e-5
            {
                // A → B → AC_mid_floor (flat half at floor level)
                add_point(ctx, geometry, a_x, va_y, 0.0, uv_a, 0.0, false);
                add_point(ctx, geometry, b_x, vb_y, 0.0, uv_b, 0.0, false);
                add_point(ctx, geometry, 0.0, va_y, 0.5, uv_left, 1.0, false);
                // AC_mid_floor → B → AC_top (wall segment at AC midpoint)
                add_point(ctx, geometry, 0.0, va_y, 0.5, uv_left, 1.0, false);
                add_point(ctx, geometry, b_x, vb_y, 0.0, uv_b, 0.0, false);
                add_point(ctx, geometry, 0.0, ac_top, 0.5, uv_left, 1.0, false);
            } else {
                add_point(ctx, geometry, a_x, va_y, 0.0, uv_a, 0.0, false);
                add_point(ctx, geometry, b_x, vb_y, 0.0, uv_b, 0.0, false);
                add_point(ctx, geometry, 0.0, ac_top, 0.5, uv_left, 1.0, false);
            }

            // T2: right triangle — split BD boundary only when BD is walled
            if (b_x - 1.0).abs() < 1e-5
                && (bd_top - bd_bot).abs() > 1e-5
                && (bd_top - vb_y).abs() > 1e-5
            {
                // BD_mid_floor → AC_top → B (flat half at floor level)
                add_point(ctx, geometry, 1.0, vb_y, 0.5, uv_right, 1.0, false);
                add_point(ctx, geometry, 0.0, ac_top, 0.5, uv_left, 1.0, false);
                add_point(ctx, geometry, b_x, vb_y, 0.0, uv_b, 0.0, false);
                // BD_top → AC_top → BD_mid_floor (wall segment at BD midpoint)
                add_point(ctx, geometry, 1.0, bd_top, 0.5, uv_right, 1.0, false);
                add_point(ctx, geometry, 0.0, ac_top, 0.5, uv_left, 1.0, false);
                add_point(ctx, geometry, 1.0, vb_y, 0.5, uv_right, 1.0, false);
            } else {
                add_point(ctx, geometry, 1.0, bd_top, 0.5, uv_right, 1.0, false);
                add_point(ctx, geometry, 0.0, ac_top, 0.5, uv_left, 1.0, false);
                add_point(ctx, geometry, b_x, vb_y, 0.0, uv_b, 0.0, false);
            }
        }
    }

    // Wall — skip degenerate triangles when a side is merged (top == bot)
    ctx.start_wall();
    let ac_has_wall = (ac_top - ac_bot).abs() > 1e-5;
    let bd_has_wall = (bd_top - bd_bot).abs() > 1e-5;
    if ac_has_wall || bd_has_wall {
        if ac_has_wall {
            add_point(ctx, geometry, 0.0, ac_bot, 0.5, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.0, ac_top, 0.5, 0.0, 1.0, false);
            add_point(ctx, geometry, 1.0, bd_bot, 0.5, 1.0, 0.0, false);
        }
        if bd_has_wall {
            add_point(ctx, geometry, 1.0, bd_top, 0.5, 1.0, 1.0, false);
            add_point(ctx, geometry, 1.0, bd_bot, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 0.0, ac_top, 0.5, 0.0, 1.0, false);
        }
    }

    if floor_below {
        ctx.start_floor();
        // Split CD midpoint: each half uses the height matching its corner
        // so boundary edges are flat (walled) or correctly sloped (merged).
        let mid_cd_c = ctx.cd_height(0.5, cy >= dy); // C-side midpoint
        let mid_cd_d = ctx.cd_height(0.5, dy >= cy); // D-side midpoint

        // C-side: wall_left → mid_c → C — split AC only when AC is walled
        if (ac_top - ac_bot).abs() > 1e-5 && (ac_bot - cy).abs() > 1e-5 {
            // AC flat half at C level
            add_point(ctx, geometry, 0.0, cy, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
            // AC wall at midpoint
            add_point(ctx, geometry, 0.0, ac_bot, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.0, cy, 0.5, 1.0, 0.0, false);
        } else {
            add_point(ctx, geometry, 0.0, ac_bot, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        }

        // D-side: wall_right → D → mid_d — split BD only when BD is walled
        if (bd_top - bd_bot).abs() > 1e-5 && (bd_bot - dy).abs() > 1e-5 {
            // BD flat half at D level
            add_point(ctx, geometry, 1.0, dy, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 0.0, 0.0, false);
            // BD wall at midpoint
            add_point(ctx, geometry, 1.0, bd_bot, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 1.0, dy, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 0.0, 0.0, false);
        } else {
            add_point(ctx, geometry, 1.0, bd_bot, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 0.0, 0.0, false);
        }

        // Central: wall_left → wall_right → mid_d
        add_point(ctx, geometry, 0.0, ac_bot, 0.5, 1.0, 0.0, false);
        add_point(ctx, geometry, 1.0, bd_bot, 0.5, 1.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 0.0, 0.0, false);

        // Wall at CD midpoint (only when walled — midpoints differ)
        if (mid_cd_c - mid_cd_d).abs() > 1e-5 {
            add_point(ctx, geometry, 0.0, ac_bot, 0.5, 1.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 0.0, 0.0, false);
        }
    }
}

/// Inner corner where A is the lowered corner
pub fn add_inner_corner(
    ctx: &mut CellContext,
    geometry: &mut CellGeometry,
    lower_floor: bool,
    full_upper_floor: bool,
    _flatten: bool,
    bd_floor: bool,
    cd_floor: bool,
) {
    let (ay, by, cy, _dy) = ctx.corner_heights();

    // Boundary midpoint heights from profiles (A is lowered, midpoints are above A)
    let mid_ab = ctx.ab_height(0.5, true); // (0.5, ?, 0.0) on AB, upper side
    let mid_ac = ctx.ac_height(0.5, true); // (0.0, ?, 0.5) on AC, upper side

    if lower_floor {
        ctx.start_floor();
        add_point(ctx, geometry, 0.0, ay, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, ay, 0.0, 1.0, 0.0, false);
        add_point(ctx, geometry, 0.0, ay, 0.5, 1.0, 0.0, false);
    }

    ctx.start_wall();
    add_point(ctx, geometry, 0.0, ay, 0.5, 1.0, 0.0, false);
    add_point(ctx, geometry, 0.5, ay, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, mid_ac, 0.5, 1.0, 1.0, false);

    add_point(ctx, geometry, 0.5, mid_ab, 0.0, 0.0, 1.0, false);
    add_point(ctx, geometry, 0.0, mid_ac, 0.5, 1.0, 1.0, false);
    add_point(ctx, geometry, 0.5, ay, 0.0, 0.0, 0.0, false);

    ctx.start_floor();
    if full_upper_floor {
        let dy = ctx.dy();
        // Split BD and CD midpoints: each half uses height matching its corner
        let mid_bd_b = ctx.bd_height(0.5, by >= dy); // B-side of BD
        let mid_bd_d = ctx.bd_height(0.5, dy >= by); // D-side of BD
        let mid_cd_c = ctx.cd_height(0.5, cy >= dy); // C-side of CD
        let mid_cd_d = ctx.cd_height(0.5, dy >= cy); // D-side of CD

        // B → BD_mid_b → C
        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, mid_bd_b, 0.5, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);

        // BD_mid_d → D → CD_mid_d
        add_point(ctx, geometry, 1.0, mid_bd_d, 0.5, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, dy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 0.0, 0.0, false);

        // BD_mid_d → CD_mid_c → C
        add_point(ctx, geometry, 1.0, mid_bd_d, 0.5, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);

        // CD wall at midpoint (only if walled)
        if (mid_cd_c - mid_cd_d).abs() > 1e-5 {
            add_point(ctx, geometry, 1.0, mid_bd_d, 0.5, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 0.0, 0.0, false);
        }

        // BD wall at midpoint (only if walled)
        if (mid_bd_b - mid_bd_d).abs() > 1e-5 {
            add_point(ctx, geometry, 1.0, mid_bd_b, 0.5, 0.0, 0.0, false);
            add_point(ctx, geometry, 1.0, mid_bd_d, 0.5, 0.0, 0.0, false);
            add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        }

        // Connecting triangles from wall top to corners
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, mid_ac, 0.5, 0.0, 1.0, false);
        add_point(ctx, geometry, 0.5, mid_ab, 0.0, 0.0, 1.0, false);

        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_ab, 0.0, 0.0, 1.0, false);
    }

    if cd_floor {
        let dy = ctx.dy();
        // Split BD midpoint: B-side for canonical boundary edge, D-side for internal wall match
        let mid_bd_b = ctx.bd_height(0.5, by >= dy); // B-side (flat at B level)
        let mid_bd_d = ctx.bd_height(0.5, dy >= by); // D-side (matches edge primitive wall)

        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.0, mid_ac, 0.5, 1.0, 1.0, false);
        add_point(ctx, geometry, 0.5, mid_ab, 0.0, 0.0, 1.0, false);

        // B → mid_bd_b → mid_ac (BD boundary edge is canonical)
        add_point(ctx, geometry, 1.0, by, 0.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 1.0, mid_bd_b, 0.5, 1.0, -1.0, false);
        add_point(ctx, geometry, 0.0, mid_ac, 0.5, 1.0, 1.0, false);

        // BD wall at midpoint (only if walled)
        if (mid_bd_b - mid_bd_d).abs() > 1e-5 {
            add_point(ctx, geometry, 1.0, mid_bd_b, 0.5, 1.0, -1.0, false);
            add_point(ctx, geometry, 1.0, mid_bd_d, 0.5, 1.0, -1.0, false);
            add_point(ctx, geometry, 0.0, mid_ac, 0.5, 1.0, 1.0, false);
        }
    }

    if bd_floor {
        let dy = ctx.dy();
        // Split CD midpoint: C-side for canonical boundary edge, D-side for internal wall match
        let mid_cd_c = ctx.cd_height(0.5, cy >= dy); // C-side (flat at C level)
        let mid_cd_d = ctx.cd_height(0.5, dy >= cy); // D-side (matches edge primitive wall)

        add_point(ctx, geometry, 0.0, mid_ac, 0.5, 0.0, 1.0, false);
        add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 1.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);

        // mid_cd_c → C → mid_ab (CD boundary edge is canonical)
        add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 1.0, -1.0, false);
        add_point(ctx, geometry, 0.0, cy, 1.0, 0.0, 0.0, false);
        add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 1.0, false);

        // CD wall at midpoint (only if walled)
        if (mid_cd_c - mid_cd_d).abs() > 1e-5 {
            add_point(ctx, geometry, 0.5, mid_cd_c, 1.0, 1.0, -1.0, false);
            add_point(ctx, geometry, 0.5, mid_cd_d, 1.0, 1.0, -1.0, false);
            add_point(ctx, geometry, 0.5, mid_ab, 0.0, 1.0, 1.0, false);
        }
    }
}

/// Diagonal floor connecting B and C corners.
/// `a_edge_y`/`d_edge_y`: optional height overrides (now unused, profiles handle this).
pub fn add_diagonal_floor(
    ctx: &mut CellContext,
    geometry: &mut CellGeometry,
    b_y: f32,
    c_y: f32,
    a_cliff: bool,
    d_cliff: bool,
    _a_edge_y: Option<f32>,
    _d_edge_y: Option<f32>,
) {
    ctx.start_floor();

    let a_uv_x = if a_cliff { 0.0 } else { 1.0 };
    let a_uv_y = if a_cliff { 1.0 } else { 0.0 };
    let d_uv_x = if d_cliff { 0.0 } else { 1.0 };
    let d_uv_y = if d_cliff { 1.0 } else { 0.0 };

    let ay = ctx.ay();
    let dy = ctx.dy();

    // Boundary midpoint heights from profiles.
    // The diagonal floor connects B and C. At each boundary midpoint,
    // we want B's or C's side of the wall (whichever the floor extends from).
    let a_by = ctx.ab_height(0.5, b_y >= ay); // (0.5, ?, 0.0) on AB, B's side
    let a_cy = ctx.ac_height(0.5, c_y >= ay); // (0.0, ?, 0.5) on AC, C's side
    let d_by = ctx.bd_height(0.5, b_y >= dy); // (1.0, ?, 0.5) on BD, B's side
    let d_cy = ctx.cd_height(0.5, c_y >= dy); // (0.5, ?, 1.0) on CD, C's side

    add_point(ctx, geometry, 1.0, b_y, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, c_y, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.5, a_by, 0.0, a_uv_x, a_uv_y, false);

    add_point(ctx, geometry, 0.0, c_y, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 0.0, a_cy, 0.5, a_uv_x, a_uv_y, false);
    add_point(ctx, geometry, 0.5, a_by, 0.0, a_uv_x, a_uv_y, false);

    add_point(ctx, geometry, 1.0, b_y, 0.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 1.0, d_by, 0.5, d_uv_x, d_uv_y, false);
    add_point(ctx, geometry, 0.0, c_y, 1.0, 0.0, 0.0, false);

    add_point(ctx, geometry, 0.0, c_y, 1.0, 0.0, 0.0, false);
    add_point(ctx, geometry, 1.0, d_by, 0.5, d_uv_x, d_uv_y, false);
    add_point(ctx, geometry, 0.5, d_cy, 1.0, d_uv_x, d_uv_y, false);
}
