# Pixy Terrain — Part 03: Vertex Generation & Color Encoding

**Series:** Reconstructing Pixy Terrain
**Part:** 03 of 18
**Previous:** 2026-02-06-marching-squares-data-model-02.md
**Status:** Complete

## What We're Building

The `add_point()` function — the single most important function in the entire codebase. Every triangle vertex in every terrain chunk passes through this function. It transforms cell-local coordinates into world space, applies rotation, samples color maps, computes material blend data, and stores the result in `CellGeometry`. We also build `calculate_material_blend_data()` and `calculate_cell_material_pair()`.

## What You'll Have After This

A compiling project with the complete vertex pipeline: given a position within a cell, `add_point()` produces a fully-attributed vertex ready for SurfaceTool. Still no geometry cases yet — those come in Parts 04-05.

## Prerequisites

- Part 02 completed (MergeMode, CellContext, CellGeometry, helper functions)

## Steps

### Step 1: Add `calculate_cell_material_pair()`

**Why:** Each cell can have up to 3 different textures at its corners (the 4th corner always shares one of the first 3). Before generating vertices, we identify the dominant, secondary, and tertiary textures. These are packed into `CUSTOM2` so the shader can blend between them.

**File:** `rust/src/marching_squares.rs` (append before the tests module, or at the end if no tests yet)

```rust
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
```

**What's happening:**
- Each corner's texture index (0-15) is decoded from the two color maps. Corner A's index comes from `color_map_0[z*dim_x + x]` and `color_map_1[z*dim_x + x]`.
- We count how many corners use each texture, then sort by frequency. The most common becomes `cell_mat_a`, second most `cell_mat_b`, third `cell_mat_c`.
- This gives the shader up to 3 textures to blend within a single cell. Most cells only have 1-2 textures; 3-texture cells occur at paint boundaries.

### Step 2: Add `calculate_material_blend_data()`

**Why:** The shader needs to know how to blend between the 3 cell textures at each vertex. This function computes per-vertex weights using bilinear interpolation based on the vertex position within the cell.

**File:** `rust/src/marching_squares.rs` (append after `calculate_cell_material_pair`)

```rust
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
```

**What's happening:**
- Bilinear interpolation: a vertex at the top-left corner (0,0) gets 100% weight from corner A. A vertex at center (0.5, 0.5) gets 25% from each corner.
- Each corner's weight goes to whichever of the 3 cell materials matches that corner's texture index.
- **Packing trick**: `mat_a` and `mat_b` are packed into a single float as `(a + b*16) / 255`. The shader unpacks with `floor(r*255)` and modulo. This avoids wasting a vertex attribute channel.
- `mat_c` is stored in the G channel divided by 15 (since texture indices go 0-15).
- Weights for `mat_a` and `mat_b` go in B and A channels. `mat_c`'s weight is implicitly `1 - weight_a - weight_b`.

### Step 3: Add `calculate_boundary_colors()`

**Why:** When a cell has significant height variation (walls), the color needs to transition from the lower corner's texture to the upper corner's texture along the height gradient. This pre-computes which corner's color is "lower" and which is "upper" for both floor and wall color maps.

**File:** `rust/src/marching_squares.rs` (append after `calculate_material_blend_data`)

```rust
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
```

**What's happening:**
- `corner_heights` uses the original (unrotated) order `[A, B, C, D]`, but note `heights[2]` is D and `heights[3]` is C — matching the `[A, B, D, C]` storage order.
- We find which corner is lowest and highest. The lowest corner's color is the "base" and the highest corner's color is the "top". Vertices between get a gradient.
- Floor and wall have separate color maps because you might paint grass-green on floors and stone-grey on walls.

### Step 4: Add `add_point()` — the vertex factory

**Why:** This is the central function. Every geometry case calls `add_point()` 3-30+ times. It takes a cell-local position `(x, y, z)`, applies rotation, samples colors from the maps, computes material blend data, and pushes a fully-attributed vertex into `CellGeometry`. The function is ~300 lines because it handles color interpolation for 5 different blend modes.

**File:** `rust/src/marching_squares.rs` (insert after the CellContext impl block, before the geometry functions)

```rust
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
```

**What's happening — the key concepts:**

**Rotation**: The input `(x, z)` is in cell-local space (0-1). The rotation loop applies a 90-degree CW rotation: `(x, z) → (1-z, x)`. Applied `rotation` times, this maps the cell-local coordinates from the canonical case orientation to the actual cell orientation.

**Color sampling** has 4 paths depending on the vertex type:
1. **New chunk**: All vertices get the default texture (slot 0 = `(1,0,0,0)`)
2. **Diagonal midpoint**: The center vertex of a higher-poly floor. Average the A↔D and B↔C diagonals, take the min per channel (with a 0.99 threshold hack to preserve dominant channels)
3. **Boundary cell** (has walls): Height-based gradient between lower and upper corner colors, with a 30-70% threshold band for smooth transition
4. **Normal cell**: Bilinear interpolation across the 4 corners, then `get_dominant_color()` to snap to nearest one-hot

**UV system**: Two UV channels serve different purposes:
- `UV1.x`: Distance to the cliff top (0 = on top, 1 = at edge). Used by the shader for edge darkening.
- `UV1.y`: Distance to the cliff bottom (0 = at bottom, 1 = at top). Used for grass exclusion near ridges.
- `UV2`: Floor gets world-space coordinates for triplanar texturing. Walls get `XY + ZY` coordinates with chunk position offset so textures tile seamlessly across chunk boundaries.

**NaN defense**: Three levels of NaN guards — input validation, post-rotation check, and final vertex check. This is battle-tested: terrain painting can produce NaN from division by zero in edge cases, and a single NaN vertex corrupts the entire mesh.

**Material blend postprocess**: The `mat_blend.a = 2.0` sentinel tells the shader "this floor cell has walls nearby" — triggering a special blend mode that prevents texture seams at wall/floor transitions.

## Verify

```bash
cd rust && cargo build
```

**Expected:** Compiles successfully. Warnings about unused functions are expected — `add_point()` and the calculation functions aren't called by anything yet.

## What You Learned

- **The vertex pipeline**: position rotation → color sampling → material blend → NaN guard → world-space transform → push to geometry cache
- **4 color sampling strategies**: new chunk, diagonal midpoint, boundary gradient, bilinear interpolation — each handles a different geometric situation
- **Material blend encoding**: 3 texture IDs + 2 weights packed into a single Color (RGBA) vertex attribute
- **Defensive programming**: NaN guards at every stage prevent a single corrupt vertex from cascading into a broken mesh
- **UV conventions**: UV1 for cliff detection (grass/ridge exclusion), UV2 for triplanar texturing with cross-chunk continuity

## Stubs Introduced

(No new stubs)

## Stubs Resolved

(No stubs resolved — this adds functions to marching_squares.rs which was partially resolved in Part 02)
