// Marching squares terrain algorithm — implemented in Parts 02-05

use godot::prelude::*;

pub const BLEND_EDGE_SENSITIVITY: f32 = 1.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    Cubic,
    Polyhedron,
    RoundedPolyhedron,
    SemiRound,
    Spherical,
}

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

#[derive(Clone, Debug, Default)]
pub struct CellGeometry {
    pub verts: Vec<Vector3>,
    pub uvs: Vec<Vector2>,
    pub uv2s: Vec<Vector2>,
    pub colors_0: Vec<Color>,
    pub colors_1: Vec<Color>,
    pub grass_mask: Vec<Color>,
    pub material_blend: Vec<Color>,
    pub is_floor: Vec<bool>,
}

#[derive(Clone, Debug, Default)]
pub struct CellContext {
    // Grid
    pub heights: [f32; 4],
    pub edges: [bool; 4],
    pub rotation: usize,
    pub cell_coords: Vector2i,
    pub dimensions: Vector3i,
    pub cell_size: Vector2,
    pub merge_threshold: f32,
    pub higher_poly_floors: bool,

    // Colors
    pub color_map_0: Vec<Color>,
    pub color_map_1: Vec<Color>,
    pub wall_color_map_0: Vec<Color>,

    pub wall_color_map_1: Vec<Color>,
    pub grass_mask_map: Vec<Color>,

    // Cell boundary detection
    pub cell_min_height: f32,
    pub cell_max_height: f32,
    pub cell_is_boundary: bool,

    // Boundary colors
    pub cell_floor_lower_color_0: Color,
    pub cell_floor_upper_color_0: Color,
    pub cell_floor_lower_color_1: Color,
    pub cell_floor_upper_color_1: Color,
    pub cell_wall_lower_color_0: Color,
    pub cell_wall_upper_color_0: Color,
    pub cell_wall_lower_color_1: Color,
    pub cell_wall_upper_color_1: Color,

    // Precell dominant materials (3 texture system)
    pub cell_material_a: i32,
    pub cell_material_b: i32,
    pub cell_material_c: i32,

    // Blend mode from terrain system
    pub blend_mode: i32,
    pub use_ridge_texture: bool,
    pub ridge_threshold: f32,

    // Whether this is a new (freshly created) chunk
    pub is_new_chunk: bool,

    // Floor mode toggle: true = floor geometry, false = wall geometry
    pub floor_mode: bool,

    // Blend thresholds
    pub lower_threshold: f32,
    pub upper_threshold: f32,

    // Chunk world position for wall UV2 offset
    pub chunk_position: Vector3,
}

impl CellContext {
    pub fn ay(&self) -> f32 {
        self.heights[self.rotation]
    }
    pub fn by(&self) -> f32 {
        self.heights[(self.rotation + 1) % 4]
    }
    pub fn dy(&self) -> f32 {
        self.heights[(self.rotation + 2) % 4]
    }
    pub fn cy(&self) -> f32 {
        self.heights[(self.rotation + 3) % 4]
    }
    pub fn ab(&self) -> bool {
        self.edges[self.rotation]
    }
    pub fn bd(&self) -> bool {
        self.edges[(self.rotation + 1) % 4]
    }
    pub fn cd(&self) -> bool {
        self.edges[(self.rotation + 2) % 4]
    }
    pub fn ac(&self) -> bool {
        self.edges[(self.rotation + 3) % 4]
    }
    pub fn rotate_cell(&mut self, rotations: i32) {
        self.rotation = ((self.rotation as i32 + 4 + rotations) % 4) as usize
    }
    pub fn is_higher(&self, a: f32, b: f32) -> bool {
        a - b > self.merge_threshold
    }
    pub fn is_lower(&self, a: f32, b: f32) -> bool {
        a - b < -self.merge_threshold
    }
    pub fn is_merged(&self, a: f32, b: f32) -> bool {
        (a - b).abs() < self.merge_threshold
    }
    pub fn start_floor(&mut self) {
        self.floor_mode = true
    }
    pub fn start_wall(&mut self) {
        self.floor_mode = false;
    }
    pub fn color_index(&self, x: i32, z: i32) -> usize {
        (z * self.dimensions.x + x) as usize
    }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

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

fn calculate_cell_material_pair(ctx: &mut CellContext) {
    let cc = ctx.cell_coords;
    let dim_x = ctx.dimensions.x;

    let texture_a = get_texture_index_from_colors(
        ctx.color_map_0[(cc.y * dim_x + cc.x) as usize],
        ctx.color_map_1[(cc.y * dim_x + cc.x) as usize],
    );

    let texture_b = get_texture_index_from_colors(
        ctx.color_map_0[(cc.y * dim_x + cc.x + 1) as usize],
        ctx.color_map_1[(cc.y * dim_x + cc.x + 1) as usize],
    );

    let texture_c = get_texture_index_from_colors(
        ctx.color_map_0[((cc.y + 1) * dim_x + cc.x) as usize],
        ctx.color_map_1[((cc.y + 1) * dim_x + cc.x) as usize],
    );

    let texture_d = get_texture_index_from_colors(
        ctx.color_map_0[((cc.y + 1) * dim_x + cc.x + 1) as usize],
        ctx.color_map_1[((cc.y + 1) * dim_x + cc.x + 1) as usize],
    );

    let mut counts = std::collections::HashMap::new();
    for texture in [texture_a, texture_b, texture_c, texture_d] {
        *counts.entry(texture).or_insert(0) += 1;
    }

    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    ctx.cell_material_a = sorted[0].0;
    ctx.cell_material_b = if sorted.len() > 1 {
        sorted[1].0
    } else {
        sorted[0].0
    };
    ctx.cell_material_c = if sorted.len() > 2 {
        sorted[2].0
    } else {
        ctx.cell_material_b
    };
}

fn calculate_material_blend_data(
    ctx: &CellContext,
    vertex_x: f32,
    vertex_z: f32,
    source_map_0: &[Color],
    source_map_1: &[Color],
) -> Color {
    let cc = ctx.cell_coords;
    let dim_x = ctx.dimensions.x;

    let texture_a = get_texture_index_from_colors(
        ctx.color_map_0[(cc.y * dim_x + cc.x) as usize],
        ctx.color_map_1[(cc.y * dim_x + cc.x) as usize],
    );

    let texture_b = get_texture_index_from_colors(
        ctx.color_map_0[(cc.y * dim_x + cc.x + 1) as usize],
        ctx.color_map_1[(cc.y * dim_x + cc.x + 1) as usize],
    );

    let texture_c = get_texture_index_from_colors(
        ctx.color_map_0[((cc.y + 1) * dim_x + cc.x) as usize],
        ctx.color_map_1[((cc.y + 1) * dim_x + cc.x) as usize],
    );

    let texture_d = get_texture_index_from_colors(
        ctx.color_map_0[((cc.y + 1) * dim_x + cc.x + 1) as usize],
        ctx.color_map_1[((cc.y + 1) * dim_x + cc.x + 1) as usize],
    );

    // Bilinear interpolation weights
    let weight_a = (1.0 - vertex_x) * (1.0 - vertex_z);
    let weight_b = vertex_x * (1.0 - vertex_z);
    let weight_c = (1.0 - vertex_x) * vertex_z;
    let weight_d = vertex_x * vertex_z;

    let mut weight_material_a = 0.0f32;
    let mut weight_material_b = 0.0f32;
    let mut weight_material_c = 0.0f32;

    for (texture, weight) in [
        (texture_a, weight_a),
        (texture_b, weight_b),
        (texture_c, weight_c),
        (texture_d, weight_d),
    ] {
        if texture == ctx.cell_material_a {
            weight_material_a += weight;
        } else if texture == ctx.cell_material_b {
            weight_material_b += weight;
        } else if texture == ctx.cell_material_c {
            weight_material_c += weight;
        }
    }

    let total_weight = weight_material_a + weight_material_b + weight_material_c;
    if total_weight > 0.001 {
        weight_material_a /= total_weight;
        weight_material_b /= total_weight;
    };

    let packed_materials = (ctx.cell_material_a as f32 + ctx.cell_material_b as f32 * 16.0) / 255.0;

    Color::from_rgba(
        packed_materials,
        ctx.cell_material_c as f32 / 15.0,
        weight_material_a,
        weight_material_b,
    )
}

fn calculate_boundary_colors(ctx: &mut CellContext) {
    let cc = ctx.cell_coords;
    let dim_x = ctx.dimensions.x;

    let corner_indices = [
        (cc.y * dim_x + cc.x) as usize,           // A
        (cc.y * dim_x + cc.x + 1) as usize,       // B
        ((cc.y + 1) * dim_x + cc.x) as usize,     // C
        ((cc.y + 1) * dim_x + cc.x + 1) as usize, // D
    ];
    let corner_heights = [
        ctx.heights[0], // A
        ctx.heights[1], // B
        ctx.heights[3], // C
        ctx.heights[2], // D
    ];

    let mut min_idx = 0;
    let mut max_idx = 0;
    for i in 1..4 {
        if corner_heights[i] < corner_heights[min_idx] {
            min_idx = i;
        }
        if corner_heights[i] > corner_heights[max_idx] {
            max_idx = i;
        }
    }

    // Floor boundary colors
    ctx.cell_floor_lower_color_0 = ctx.color_map_0[corner_indices[min_idx]];
    ctx.cell_floor_upper_color_0 = ctx.color_map_0[corner_indices[max_idx]];
    ctx.cell_floor_lower_color_1 = ctx.color_map_1[corner_indices[min_idx]];
    ctx.cell_floor_upper_color_1 = ctx.color_map_1[corner_indices[max_idx]];

    // Wall boundary colors
    ctx.cell_wall_lower_color_0 = ctx.wall_color_map_0[corner_indices[min_idx]];
    ctx.cell_wall_upper_color_0 = ctx.wall_color_map_0[corner_indices[max_idx]];

    ctx.cell_wall_lower_color_1 = ctx.wall_color_map_1[corner_indices[min_idx]];
    ctx.cell_wall_upper_color_1 = ctx.wall_color_map_1[corner_indices[max_idx]];
}

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

fn corner_indices(cc: Vector2i, dim_x: i32) -> [usize; 4] {
    [
        (cc.y * dim_x + cc.x) as usize,           // A
        (cc.y * dim_x + cc.x + 1) as usize,       // B
        ((cc.y + 1) * dim_x + cc.x) as usize,     // C
        ((cc.y + 1) * dim_x + cc.x + 1) as usize, // D
    ]
}

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
        return Color::from_rgba(1.0, 0.0, 0.0, 0.0);
    }

    if diagonal_midpoint {
        if ctx.blend_mode == 1 {
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

        let mut c = Color::from_rgba(
            ad_color.r.min(bc_color.r),
            ad_color.g.min(bc_color.g),
            ad_color.b.min(bc_color.b),
            ad_color.a.min(bc_color.a),
        );
        if ad_color.r > 0.99 || bc_color.r > 0.99 {
            c.r = 1.0;
        }
        if ad_color.g > 0.99 || bc_color.g > 0.99 {
            c.g = 1.0;
        }
        if ad_color.b > 0.99 || bc_color.b > 0.99 {
            c.b = 1.0;
        }
        if ad_color.a > 0.99 || bc_color.a > 0.99 {
            c.a = 1.0;
        }

        return c;
    }

    if ctx.cell_is_boundary {
        if ctx.blend_mode == 1 {
            return params.source_map[corners[0]];
        }
        let height_range = ctx.cell_max_height - ctx.cell_min_height;

        let height_factor = if height_range > 0.001 {
            ((y - ctx.cell_min_height) / height_range).clamp(0.0, 1.0)
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
    if ctx.blend_mode != 1 {
        get_dominant_color(lerp_color(ab_color, cd_color, z))
    } else {
        params.source_map[corners[0]]
    }
}

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

struct ColorSampleParams<'a> {
    source_map: &'a [Color],
    lower_color: Color,
    upper_color: Color,
    lower_threshold: f32,
    upper_threshold: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn add_point(
    ctx: &mut CellContext,
    geometry: &mut CellGeometry,
    mut x: f32,
    y: f32,
    mut z: f32,
    uv_x: f32,
    uv_y: f32,
    diagonal_midpoint: bool,
) {
    let cell = ctx.cell_coords;
    x = sanitize_float(x, 0.5, "x", cell);
    let safe_y = sanitize_float(y, 0.0, "y", cell);
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
    let is_ridge = ctx.floor_mode && ctx.use_ridge_texture && (uv.y > 1.0 - ctx.ridge_threshold);
    let use_wall_colors = !ctx.floor_mode || is_ridge;

    let cc = ctx.cell_coords;
    let dim_x = ctx.dimensions.x;
    let corners = corner_indices(cc, dim_x);

    // New chunk write-back
    if ctx.is_new_chunk {
        let new_color = Color::from_rgba(1.0, 0.0, 0.0, 0.0);
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
        (&ctx.cell_wall_lower_color_0, &ctx.cell_wall_upper_color_0)
    } else {
        (&ctx.cell_floor_lower_color_0, &ctx.cell_floor_upper_color_0)
    };

    let (lower_1, upper_1) = if use_wall_colors {
        (&ctx.cell_wall_lower_color_1, &ctx.cell_wall_upper_color_1)
    } else {
        (&ctx.cell_floor_lower_color_1, &ctx.cell_floor_upper_color_1)
    };

    let params_0 = ColorSampleParams {
        source_map: source_map_0,
        lower_color: *lower_0,
        upper_color: *upper_0,
        lower_threshold: ctx.lower_threshold,
        upper_threshold: ctx.upper_threshold,
    };

    let params_1 = ColorSampleParams {
        source_map: source_map_1,
        lower_color: *lower_1,
        upper_color: *upper_1,
        lower_threshold: 0.3,
        upper_threshold: 0.7,
    };

    let color_0 = compute_vertex_color(&params_0, &corners, ctx, x, safe_y, z, diagonal_midpoint);

    let color_1 = compute_vertex_color(&params_1, &corners, ctx, x, safe_y, z, diagonal_midpoint);

    // Grass mask
    let mut grass_mask = ctx.grass_mask_map[(cc.y * dim_x + cc.x) as usize];
    grass_mask.g = if is_ridge { 1.0 } else { 0.0 };

    // Material blend
    let mut material_blend = calculate_material_blend_data(ctx, x, y, source_map_0, source_map_1);
    let blend_threshold = ctx.merge_threshold * BLEND_EDGE_SENSITIVITY;
    let blend_ab = (ctx.ay() - ctx.by()).abs() < blend_threshold;
    let blend_ac = (ctx.ay() - ctx.cy()).abs() < blend_threshold;
    let blend_bd = (ctx.by() - ctx.dy()).abs() < blend_threshold;
    let blend_cd = (ctx.cy() - ctx.dy()).abs() < blend_threshold;
    if !(blend_ab && blend_ac && blend_bd && blend_cd) && ctx.floor_mode {
        material_blend.a = 2.0;
    }

    // Vertex position
    let vertex = Vector3::new(
        (cc.x as f32 + x) * ctx.cell_size.x,
        safe_y,
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
            safe_y,
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
