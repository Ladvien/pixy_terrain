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

/// Add a vertex point to the cell geometry. Coordinates are relative to the cell's
/// top-left corner (0,0) to (1,1). The point is rotated by the current rotation before
/// being placed. UV.x = closeness to top terrace, UV.y = closeness to bottom of terrace.
#[allow(clippy::too_many_arguments)]
pub fn add_point(
    ctx: &mut CellContext,
    geo: &mut CellGeometry,
    mut x: f32,
    y: f32,
    mut z: f32,
    uv_x: f32,
    uv_y: f32,
    diag_midpoint: bool,
) {
    // Guard ALL input coordinates against NaN/Inf - replace with safe fallbacks
    // instead of skipping (skipping would cause incomplete triangles)
    let safe_x = if x.is_finite() {
        x
    } else {
        godot_warn!(
            "NaN/Inf x-coordinate at cell ({}, {}): x={}. Using 0.5 fallback.",
            ctx.cell_coords.x,
            ctx.cell_coords.y,
            x
        );
        0.5
    };
    let safe_y = if y.is_finite() {
        y
    } else {
        godot_warn!(
            "NaN/Inf y-coordinate at cell ({}, {}): y={}. Using 0.0 fallback.",
            ctx.cell_coords.x,
            ctx.cell_coords.y,
            y
        );
        0.0
    };
    let safe_z = if z.is_finite() {
        z
    } else {
        godot_warn!(
            "NaN/Inf z-coordinate at cell ({}, {}): z={}. Using 0.5 fallback.",
            ctx.cell_coords.x,
            ctx.cell_coords.y,
            z
        );
        0.5
    };
    x = safe_x;
    z = safe_z;

    // Rotate the point
    for _ in 0..ctx.rotation {
        let temp = x;
        x = 1.0 - z;
        z = temp;
    }

    // Post-rotation NaN check (rotation math could produce NaN if inputs were bad)
    if !x.is_finite() || !z.is_finite() {
        godot_warn!(
            "NaN after rotation at cell ({}, {}). Using center fallback.",
            ctx.cell_coords.x,
            ctx.cell_coords.y
        );
        x = 0.5;
        z = 0.5;
    }

    // UV: floor uses provided values, walls always (1, 1)
    let uv = if ctx.floor_mode {
        Vector2::new(uv_x, uv_y)
    } else {
        Vector2::new(1.0, 1.0)
    };

    // Ridge detection
    let is_ridge = ctx.floor_mode && ctx.use_ridge_texture && (uv.y > 1.0 - ctx.ridge_threshold);

    // Determine whether to use wall or floor color maps
    let use_wall_colors = !ctx.floor_mode || is_ridge;
    let use_wall_colors = if ctx.blend_mode == 1 && ctx.floor_mode && !is_ridge {
        false
    } else {
        use_wall_colors
    };

    let cc = ctx.cell_coords;
    let dim_x = ctx.dimensions.x;
    let blend_zone = ctx.upper_thresh - ctx.lower_thresh;

    // For new chunks, write back default color to source maps before creating references
    if ctx.is_new_chunk {
        let idx = (cc.y * dim_x + cc.x) as usize;
        let new_color = Color::from_rgba(1.0, 0.0, 0.0, 0.0);
        ctx.color_map_0[idx] = new_color;
        ctx.color_map_1[idx] = new_color;
        ctx.wall_color_map_0[idx] = new_color;
        ctx.wall_color_map_1[idx] = new_color;
    }

    let (source_map_0, source_map_1) = if use_wall_colors {
        (&ctx.wall_color_map_0, &ctx.wall_color_map_1)
    } else {
        (&ctx.color_map_0, &ctx.color_map_1)
    };

    // Compute color_0
    let color_0 = if ctx.is_new_chunk {
        Color::from_rgba(1.0, 0.0, 0.0, 0.0)
    } else if diag_midpoint {
        if ctx.blend_mode == 1 {
            source_map_0[(cc.y * dim_x + cc.x) as usize]
        } else {
            let a_idx = (cc.y * dim_x + cc.x) as usize;
            let d_idx = ((cc.y + 1) * dim_x + cc.x + 1) as usize;
            let b_idx = (cc.y * dim_x + cc.x + 1) as usize;
            let c_idx = ((cc.y + 1) * dim_x + cc.x) as usize;
            let ad_color = lerp_color(source_map_0[a_idx], source_map_0[d_idx], 0.5);
            let bc_color = lerp_color(source_map_0[b_idx], source_map_0[c_idx], 0.5);
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
            c
        }
    } else if ctx.cell_is_boundary {
        if ctx.blend_mode == 1 {
            source_map_0[(cc.y * dim_x + cc.x) as usize]
        } else {
            let height_range = ctx.cell_max_height - ctx.cell_min_height;
            let height_factor = if height_range > 0.001 {
                ((y - ctx.cell_min_height) / height_range).clamp(0.0, 1.0)
            } else {
                0.5 // Flat surface - use middle blend to avoid division by zero
            };
            let (lower_0, upper_0) = if use_wall_colors {
                (ctx.cell_wall_lower_color_0, ctx.cell_wall_upper_color_0)
            } else {
                (ctx.cell_floor_lower_color_0, ctx.cell_floor_upper_color_0)
            };
            let c = if height_factor < ctx.lower_thresh {
                lower_0
            } else if height_factor > ctx.upper_thresh {
                upper_0
            } else {
                let blend_factor = (height_factor - ctx.lower_thresh) / blend_zone;
                lerp_color(lower_0, upper_0, blend_factor)
            };
            get_dominant_color(c)
        }
    } else {
        let a_idx = (cc.y * dim_x + cc.x) as usize;
        let b_idx = (cc.y * dim_x + cc.x + 1) as usize;
        let c_idx = ((cc.y + 1) * dim_x + cc.x) as usize;
        let d_idx = ((cc.y + 1) * dim_x + cc.x + 1) as usize;
        let ab_color = lerp_color(source_map_0[a_idx], source_map_0[b_idx], x);
        let cd_color = lerp_color(source_map_0[c_idx], source_map_0[d_idx], x);
        if ctx.blend_mode != 1 {
            get_dominant_color(lerp_color(ab_color, cd_color, z))
        } else {
            source_map_0[(cc.y * dim_x + cc.x) as usize]
        }
    };

    // Compute color_1
    let color_1 = if ctx.is_new_chunk {
        // Source maps already updated in color_0 block above
        Color::from_rgba(1.0, 0.0, 0.0, 0.0)
    } else if diag_midpoint {
        if ctx.blend_mode == 1 {
            source_map_1[(cc.y * dim_x + cc.x) as usize]
        } else {
            let a_idx = (cc.y * dim_x + cc.x) as usize;
            let d_idx = ((cc.y + 1) * dim_x + cc.x + 1) as usize;
            let b_idx = (cc.y * dim_x + cc.x + 1) as usize;
            let c_idx = ((cc.y + 1) * dim_x + cc.x) as usize;
            let ad_color = lerp_color(source_map_1[a_idx], source_map_1[d_idx], 0.5);
            let bc_color = lerp_color(source_map_1[b_idx], source_map_1[c_idx], 0.5);
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
            c
        }
    } else if ctx.cell_is_boundary {
        if ctx.blend_mode == 1 {
            source_map_1[(cc.y * dim_x + cc.x) as usize]
        } else {
            let height_range = ctx.cell_max_height - ctx.cell_min_height;
            let height_factor = if height_range > 0.001 {
                ((y - ctx.cell_min_height) / height_range).clamp(0.0, 1.0)
            } else {
                0.5 // Flat surface - use middle blend to avoid division by zero
            };
            let (lower_1, upper_1) = if use_wall_colors {
                (ctx.cell_wall_lower_color_1, ctx.cell_wall_upper_color_1)
            } else {
                (ctx.cell_floor_lower_color_1, ctx.cell_floor_upper_color_1)
            };
            let c = if height_factor < 0.3 {
                lower_1
            } else if height_factor > 0.7 {
                upper_1
            } else {
                let blend_factor = (height_factor - 0.3) / 0.4;
                lerp_color(lower_1, upper_1, blend_factor)
            };
            get_dominant_color(c)
        }
    } else {
        let a_idx = (cc.y * dim_x + cc.x) as usize;
        let b_idx = (cc.y * dim_x + cc.x + 1) as usize;
        let c_idx = ((cc.y + 1) * dim_x + cc.x) as usize;
        let d_idx = ((cc.y + 1) * dim_x + cc.x + 1) as usize;
        let ab_color = lerp_color(source_map_1[a_idx], source_map_1[b_idx], x);
        let cd_color = lerp_color(source_map_1[c_idx], source_map_1[d_idx], x);
        if ctx.blend_mode != 1 {
            get_dominant_color(lerp_color(ab_color, cd_color, z))
        } else {
            source_map_1[(cc.y * dim_x + cc.x) as usize]
        }
    };

    // Grass mask
    let mut g_mask = ctx.grass_mask_map[(cc.y * dim_x + cc.x) as usize];
    g_mask.g = if is_ridge { 1.0 } else { 0.0 };

    // Material blend data (CUSTOM2)
    let mat_blend = calculate_material_blend_data(ctx, x, z, source_map_0, source_map_1);
    let blend_threshold = ctx.merge_threshold * BLEND_EDGE_SENSITIVITY;
    let blend_ab = (ctx.ay() - ctx.by()).abs() < blend_threshold;
    let blend_ac = (ctx.ay() - ctx.cy()).abs() < blend_threshold;
    let blend_bd = (ctx.by() - ctx.dy()).abs() < blend_threshold;
    let blend_cd = (ctx.cy() - ctx.dy()).abs() < blend_threshold;
    let cell_has_walls_for_blend = !(blend_ab && blend_ac && blend_bd && blend_cd);
    let mut mat_blend = mat_blend;
    if cell_has_walls_for_blend && ctx.floor_mode {
        mat_blend.a = 2.0;
    }

    // Compute final vertex position (NaN already guarded at function entry)
    let vert = Vector3::new(
        (cc.x as f32 + x) * ctx.cell_size.x,
        safe_y,
        (cc.y as f32 + z) * ctx.cell_size.y,
    );

    // Final sanity check on computed vertex
    if !vert.x.is_finite() || !vert.y.is_finite() || !vert.z.is_finite() {
        godot_error!(
            "NaN in final vertex at cell ({}, {}): ({}, {}, {}). Using origin fallback.",
            cc.x,
            cc.y,
            vert.x,
            vert.y,
            vert.z
        );
        // Use a safe fallback vertex at cell center
        let fallback_vert = Vector3::new(
            (cc.x as f32 + 0.5) * ctx.cell_size.x,
            0.0,
            (cc.y as f32 + 0.5) * ctx.cell_size.y,
        );
        geo.verts.push(fallback_vert);
        geo.uvs.push(uv);
        geo.uv2s
            .push(Vector2::new(fallback_vert.x, fallback_vert.z) / ctx.cell_size);
        geo.colors_0.push(color_0);
        geo.colors_1.push(color_1);
        geo.grass_mask.push(g_mask);
        geo.mat_blend.push(mat_blend);
        geo.is_floor.push(ctx.floor_mode);
        return;
    }

    // UV2: floor uses world XZ / cell_size, walls use global XY+ZY with chunk offset
    let uv2 = if ctx.floor_mode {
        Vector2::new(vert.x, vert.z) / ctx.cell_size
    } else {
        let global_pos = vert + ctx.chunk_position;
        Vector2::new(global_pos.x, global_pos.y) + Vector2::new(global_pos.z, global_pos.y)
    };

    // Store in geometry cache
    geo.verts.push(vert);
    geo.uvs.push(uv);
    geo.uv2s.push(uv2);
    geo.colors_0.push(color_0);
    geo.colors_1.push(color_1);
    geo.grass_mask.push(g_mask);
    geo.mat_blend.push(mat_blend);
    geo.is_floor.push(ctx.floor_mode);
}

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
        // Inner corner + custom floor/wall/upper corner
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
        // Inner corner + custom floor/wall/upper corner
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
        // A is lower than B and C, D is higher than C, BD edge connected
        else if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.is_higher(dy, cy) && ctx.bd()
        {
            add_inner_corner(ctx, geo, true, false, true, true, false);
            ctx.rotate_cell(1);
            add_edge(ctx, geo, false, true, 0.0, 1.0);
            case_found = true;
        }
        // Case 11: Inner corner at A with edge atop CD (GDScript Case 11)
        // A is lower than B and C, D is higher than B, CD edge connected
        else if ctx.is_lower(ay, by) && ctx.is_lower(ay, cy) && ctx.is_higher(dy, by) && ctx.cd()
        {
            add_inner_corner(ctx, geo, true, false, true, false, true);
            ctx.rotate_cell(2);
            add_edge(ctx, geo, false, true, 0.0, 1.0);
            case_found = true;
        }
        // Case 12 (GDScript): Clockwise upwards spiral A<B<D<C
        // A is lowest, then B, then D, C is highest
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
        // A is lowest, then C, then D, B is highest
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
        // Outer corner atop edge atop inner corner
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
            add_outer_corner(ctx, geo, false, true, true, by); // 9 verts: A floor + wall A→mid
            ctx.rotate_cell(2); // 180° so D is at position A
            add_inner_corner(ctx, geo, true, true, true, false, false); // 18 verts: D floor + wall D→mid + L-floor
            case_found = true;
        }
        // Case 18: All edges connected except AC, A higher than C
        // Merged-edge case with averaged BD edge height
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
        // Merged-edge case with averaged AC edge height
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

/// Calculate the 2-3 dominant textures for the current cell.
fn calculate_cell_material_pair(ctx: &mut CellContext) {
    let cc = ctx.cell_coords;
    let dim_x = ctx.dimensions.x;

    let tex_a = get_texture_index_from_colors(
        ctx.color_map_0[(cc.y * dim_x + cc.x) as usize],
        ctx.color_map_1[(cc.y * dim_x + cc.x) as usize],
    );
    let tex_b = get_texture_index_from_colors(
        ctx.color_map_0[(cc.y * dim_x + cc.x + 1) as usize],
        ctx.color_map_1[(cc.y * dim_x + cc.x + 1) as usize],
    );
    let tex_c = get_texture_index_from_colors(
        ctx.color_map_0[((cc.y + 1) * dim_x + cc.x) as usize],
        ctx.color_map_1[((cc.y + 1) * dim_x + cc.x) as usize],
    );
    let tex_d = get_texture_index_from_colors(
        ctx.color_map_0[((cc.y + 1) * dim_x + cc.x + 1) as usize],
        ctx.color_map_1[((cc.y + 1) * dim_x + cc.x + 1) as usize],
    );

    // Count texture occurrences
    let mut counts = std::collections::HashMap::new();
    for tex in [tex_a, tex_b, tex_c, tex_d] {
        *counts.entry(tex).or_insert(0) += 1;
    }

    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    ctx.cell_mat_a = sorted[0].0;
    ctx.cell_mat_b = if sorted.len() > 1 {
        sorted[1].0
    } else {
        sorted[0].0
    };
    ctx.cell_mat_c = if sorted.len() > 2 {
        sorted[2].0
    } else {
        ctx.cell_mat_b
    };
}

/// Calculate CUSTOM2 blend data with 3-texture support.
/// Encoding: Color(packed_mats, mat_c/15, weight_a, weight_b)
fn calculate_material_blend_data(
    ctx: &CellContext,
    vert_x: f32,
    vert_z: f32,
    source_map_0: &[Color],
    source_map_1: &[Color],
) -> Color {
    let cc = ctx.cell_coords;
    let dim_x = ctx.dimensions.x;

    let tex_a = get_texture_index_from_colors(
        source_map_0[(cc.y * dim_x + cc.x) as usize],
        source_map_1[(cc.y * dim_x + cc.x) as usize],
    );
    let tex_b = get_texture_index_from_colors(
        source_map_0[(cc.y * dim_x + cc.x + 1) as usize],
        source_map_1[(cc.y * dim_x + cc.x + 1) as usize],
    );
    let tex_c = get_texture_index_from_colors(
        source_map_0[((cc.y + 1) * dim_x + cc.x) as usize],
        source_map_1[((cc.y + 1) * dim_x + cc.x) as usize],
    );
    let tex_d = get_texture_index_from_colors(
        source_map_0[((cc.y + 1) * dim_x + cc.x + 1) as usize],
        source_map_1[((cc.y + 1) * dim_x + cc.x + 1) as usize],
    );

    // Position weights for bilinear interpolation
    let weight_a = (1.0 - vert_x) * (1.0 - vert_z);
    let weight_b = vert_x * (1.0 - vert_z);
    let weight_c = (1.0 - vert_x) * vert_z;
    let weight_d = vert_x * vert_z;

    let mut weight_mat_a = 0.0f32;
    let mut weight_mat_b = 0.0f32;
    let mut weight_mat_c = 0.0f32;

    for (tex, weight) in [
        (tex_a, weight_a),
        (tex_b, weight_b),
        (tex_c, weight_c),
        (tex_d, weight_d),
    ] {
        if tex == ctx.cell_mat_a {
            weight_mat_a += weight;
        } else if tex == ctx.cell_mat_b {
            weight_mat_b += weight;
        } else if tex == ctx.cell_mat_c {
            weight_mat_c += weight;
        }
    }

    let total_weight = weight_mat_a + weight_mat_b + weight_mat_c;
    if total_weight > 0.001 {
        weight_mat_a /= total_weight;
        weight_mat_b /= total_weight;
    }

    let packed_mats = (ctx.cell_mat_a as f32 + ctx.cell_mat_b as f32 * 16.0) / 255.0;

    Color::from_rgba(
        packed_mats,
        ctx.cell_mat_c as f32 / 15.0,
        weight_mat_a,
        weight_mat_b,
    )
}

/// Calculate boundary colors for cells with significant height variation.
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
        ctx.heights[0],
        ctx.heights[1],
        ctx.heights[3],
        ctx.heights[2],
    ]; // A, B, C, D in original order

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
        assert_eq!(geo.verts.len(), 12); // 4 triangles × 3 vertices
    }

    #[test]
    fn test_full_floor_low_poly_generates_6_vertices() {
        let mut ctx = default_context(3, 3);
        ctx.heights = [0.0, 0.0, 0.0, 0.0];
        ctx.higher_poly_floors = false;
        let mut geo = CellGeometry::default();
        add_full_floor(&mut ctx, &mut geo);
        assert_eq!(geo.verts.len(), 6); // 2 triangles × 3 vertices
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
