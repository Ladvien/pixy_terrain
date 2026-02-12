use super::cell_context::*;
use super::types::*;
use godot::prelude::*;

struct ColorSampleParams<'a> {
    source_map: &'a [Color],
    lower_color: Color,
    upper_color: Color,
    lower_threshold: f32,
    upper_threshold: f32,
}

// ================================
// ===== Color Helpers ============
// ================================
#[must_use]
#[inline]
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

pub fn get_dominant_color(c: Color) -> Color {
    ColorChannel::dominant(c).to_one_hot()
}

#[inline]
fn sanitize_float(value: f32, fallback: f32, label: &str, cell: Vector2i) -> f32 {
    if value.is_finite() {
        value
    } else {
        godot_warn!(
            "NaN/Inf {} at cell ({}, {}). Using {} fallback.",
            label,
            cell.x,
            cell.y,
            fallback
        );
        fallback
    }
}

// ================================
// ===== Vertex Generation ========
// ================================
fn compute_vertex_color(
    params: &ColorSampleParams,
    corners: &[usize; 4],
    ctx: &CellContext,
    x: f32,
    y: f32,
    z: f32,
    diagonal_midpoint: bool,
) -> Color {
    if ctx.is_new_chunk {
        return DEFAULT_TEXTURE_COLOR;
    }

    if diagonal_midpoint {
        if ctx.blend_mode == BlendMode::Direct {
            return params.source_map[corners[0]];
        }
        let ad_color = lerp_color(
            params.source_map[corners[0]],
            params.source_map[corners[3]],
            0.5,
        );
        let bc_color = lerp_color(
            params.source_map[corners[1]],
            params.source_map[corners[2]],
            0.5,
        );

        let c = Color::from_rgba(
            ad_color.r.min(bc_color.r),
            ad_color.g.min(bc_color.g),
            ad_color.b.min(bc_color.b),
            ad_color.a.min(bc_color.a),
        );
        let color = preserve_high_channels(c, ad_color, bc_color);
        return color;
    }

    if ctx.color_state.is_boundary {
        if ctx.blend_mode == BlendMode::Direct {
            return params.source_map[corners[0]];
        }
        let height_range = ctx.color_state.max_height - ctx.color_state.min_height;

        let has_meaningful_height_range = height_range > MIN_HEIGHT_RANGE;
        let height_factor = if has_meaningful_height_range {
            let normalized_height = (y - ctx.color_state.min_height) / height_range;
            normalized_height.clamp(0.0, 1.0)
        } else {
            0.5
        };

        let c = if height_factor < params.lower_threshold {
            params.lower_color
        } else if height_factor > params.upper_threshold {
            params.upper_color
        } else {
            let blend_zone = params.upper_threshold - params.lower_threshold;
            let blend_factor = (height_factor - params.lower_threshold) / blend_zone;
            lerp_color(params.lower_color, params.upper_color, blend_factor)
        };

        return get_dominant_color(c);
    }

    // Normal bilinear interpolation
    let ab_color = lerp_color(
        params.source_map[corners[0]],
        params.source_map[corners[1]],
        x,
    );
    let cd_color = lerp_color(
        params.source_map[corners[2]],
        params.source_map[corners[3]],
        x,
    );
    if ctx.blend_mode != BlendMode::Direct {
        get_dominant_color(lerp_color(ab_color, cd_color, z))
    } else {
        params.source_map[corners[0]]
    }
}

#[allow(clippy::too_many_arguments)]
fn push_vertex(
    geometry: &mut CellGeometry,
    vertex: Vector3,
    uv: Vector2,
    uv2: Vector2,
    color_0: Color,
    color_1: Color,
    grass_mask: Color,
    material_blend: Color,
    is_floor: bool,
) {
    geometry.verts.push(vertex);
    geometry.uvs.push(uv);
    geometry.uv2s.push(uv2);
    geometry.colors_0.push(color_0);
    geometry.colors_1.push(color_1);
    geometry.grass_mask.push(grass_mask);
    geometry.material_blend.push(material_blend);
    geometry.is_floor.push(is_floor);
}

#[allow(clippy::too_many_arguments)]
pub fn add_point(
    ctx: &mut CellContext,
    geometry: &mut CellGeometry,
    mut x: f32,
    height: f32,
    mut z: f32,
    uv_x: f32,
    uv_y: f32,
    diagonal_midpoint: bool,
) {
    let cell = ctx.cell_coords;
    x = sanitize_float(x, 0.5, "x", cell);
    let safe_height = sanitize_float(height, 0.0, "y", cell);
    z = sanitize_float(z, 0.5, "z", cell);

    // Rotate
    for _ in 0..ctx.rotation {
        let temp = x;
        x = 1.0 - z;
        z = temp;
    }
    if !x.is_finite() || !z.is_finite() {
        godot_warn!(
            "NaN after rotation at cell ({}, {}). Using center.",
            cell.x,
            cell.y
        );
        x = 0.5;
        z = 0.5;
    }

    let uv = if ctx.floor_mode {
        Vector2::new(uv_x, uv_y)
    } else {
        Vector2::new(1.0, 1.0)
    };
    let near_cliff_top = uv.y > 1.0 - ctx.ridge_threshold;
    let is_ridge = ctx.floor_mode && ctx.use_ridge_texture && near_cliff_top;
    let is_wall_geometry = !ctx.floor_mode;
    let use_wall_colors = is_wall_geometry || is_ridge;

    let cc = ctx.cell_coords;
    let dim_x = ctx.dimensions.x;
    let corners = ctx.corner_indices();

    if ctx.is_new_chunk {
        let new_color = DEFAULT_TEXTURE_COLOR;
        ctx.color_map_0[corners[0]] = new_color;
        ctx.color_map_1[corners[0]] = new_color;
        ctx.wall_color_map_0[corners[0]] = new_color;
        ctx.wall_color_map_1[corners[0]] = new_color;
    }

    let (source_map_0, source_map_1) = if use_wall_colors {
        (&ctx.wall_color_map_0, &ctx.wall_color_map_1)
    } else {
        (&ctx.color_map_0, &ctx.color_map_1)
    };

    let (lower_0, upper_0) = if use_wall_colors {
        (
            ctx.color_state.wall_lower_color_0,
            ctx.color_state.wall_upper_color_0,
        )
    } else {
        (
            ctx.color_state.floor_lower_color_0,
            ctx.color_state.floor_upper_color_0,
        )
    };

    let (lower_1, upper_1) = if use_wall_colors {
        (
            ctx.color_state.wall_lower_color_1,
            ctx.color_state.wall_upper_color_1,
        )
    } else {
        (
            ctx.color_state.floor_lower_color_1,
            ctx.color_state.floor_upper_color_1,
        )
    };

    let params_0 = ColorSampleParams {
        source_map: source_map_0,
        lower_color: lower_0,
        upper_color: upper_0,
        lower_threshold: ctx.lower_threshold,
        upper_threshold: ctx.upper_threshold,
    };

    let params_1 = ColorSampleParams {
        source_map: source_map_1,
        lower_color: lower_1,
        upper_color: upper_1,
        lower_threshold: COLOR_1_LOWER_THRESHOLD,
        upper_threshold: COLOR_1_UPPER_THRESHOLD,
    };

    let color_0 = compute_vertex_color(
        &params_0,
        &corners,
        ctx,
        x,
        safe_height,
        z,
        diagonal_midpoint,
    );

    let color_1 = compute_vertex_color(
        &params_1,
        &corners,
        ctx,
        x,
        safe_height,
        z,
        diagonal_midpoint,
    );

    // Grass mask
    let mut grass_mask = ctx.grass_mask_map[(cc.y * dim_x + cc.x) as usize];
    grass_mask.g = if is_ridge { 1.0 } else { 0.0 };
    grass_mask.b = if ctx.floor_mode { 1.0 } else { 0.0 };

    // Material blend
    let mut material_blend =
        ctx.calculate_material_blend_data(x, height, source_map_0, source_map_1);
    let blend_threshold = ctx.merge_threshold * BLEND_EDGE_SENSITIVITY;
    let all_edges_merge = {
        let ab_merged = (ctx.ay() - ctx.by()).abs() < blend_threshold;
        let ac_merged = (ctx.ay() - ctx.cy()).abs() < blend_threshold;
        let bd_merged = (ctx.by() - ctx.dy()).abs() < blend_threshold;
        let cd_merged = (ctx.cy() - ctx.dy()).abs() < blend_threshold;
        ab_merged && ac_merged && bd_merged && cd_merged
    };
    let floor_has_nearby_walls = !all_edges_merge && ctx.floor_mode;
    if floor_has_nearby_walls {
        material_blend.a = WALL_BLEND_SENTINEL;
    }

    // Vertex position
    let vertex = Vector3::new(
        (cc.x as f32 + x) * ctx.cell_size.x,
        safe_height,
        (cc.y as f32 + z) * ctx.cell_size.y,
    );

    // Final NaN check
    if !vertex.x.is_finite() || !vertex.y.is_finite() || !vertex.z.is_finite() {
        godot_error!(
            "NaN in final vertex at cell ({}, {}). Using origin fallback.",
            cc.x,
            cc.y
        );
        let fallback = Vector3::new(
            (cc.x as f32 + 0.5) * ctx.cell_size.x,
            safe_height,
            (cc.y as f32 + 0.5) * ctx.cell_size.y,
        );
        let uv2 = Vector2::new(fallback.x, fallback.z) / ctx.cell_size;
        push_vertex(
            geometry,
            fallback,
            uv,
            uv2,
            color_0,
            color_1,
            grass_mask,
            material_blend,
            ctx.floor_mode,
        );
        return;
    }

    // UV2
    let uv2 = if ctx.floor_mode {
        Vector2::new(vertex.x, vertex.z) / ctx.cell_size
    } else {
        let global_position = vertex + ctx.chunk_position;
        Vector2::new(global_position.x, global_position.y)
            + Vector2::new(global_position.z, global_position.y)
    };

    push_vertex(
        geometry,
        vertex,
        uv,
        uv2,
        color_0,
        color_1,
        grass_mask,
        material_blend,
        ctx.floor_mode,
    );
}

#[inline]
fn preserve_high_channels(mut color: Color, a: Color, b: Color) -> Color {
    if a.r > DOMINANT_CHANNEL_THRESHOLD || b.r > DOMINANT_CHANNEL_THRESHOLD {
        color.r = 1.0;
    }
    if a.g > DOMINANT_CHANNEL_THRESHOLD || b.g > DOMINANT_CHANNEL_THRESHOLD {
        color.g = 1.0;
    }
    if a.b > DOMINANT_CHANNEL_THRESHOLD || b.b > DOMINANT_CHANNEL_THRESHOLD {
        color.b = 1.0;
    }
    if a.a > DOMINANT_CHANNEL_THRESHOLD || b.a > DOMINANT_CHANNEL_THRESHOLD {
        color.a = 1.0;
    }

    color
}
