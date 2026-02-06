# Pixy Terrain — Part 07: Chunk Mesh Generation

**Series:** Reconstructing Pixy Terrain
**Part:** 07 of 18
**Previous:** 2026-02-06-chunk-data-and-persistence-06.md
**Status:** Complete

## What We're Building

The mesh generation pipeline — the code that turns a chunk's heightmap and color maps into a visible 3D mesh. This replaces the stubs from Part 06 with the full SurfaceTool workflow: iterate all cells, generate geometry via the marching squares algorithm (Part 05), commit vertices with multi-attribute encoding, generate normals, build collision, and regenerate grass.

## What You'll Have After This

A `PixyTerrainChunk` that generates real terrain meshes. Given a heightmap, it produces a watertight mesh with correct normals, collision shapes, and all vertex attributes (UV, UV2, COLOR, CUSTOM0-2) that the shader needs. The chunk is now fully functional — add it to a scene and it renders.

## Prerequisites

- Part 06 completed (chunk data model, persistence, all draw/get methods)
- Part 05 completed (`generate_cell()`, all geometry primitives, `CellContext`/`CellGeometry`)

## Steps

### Step 1: Replace `regenerate_mesh_with_material()` with the full implementation

**Why:** This is the top-level mesh rebuild method. It orchestrates the entire pipeline: clear caches, mark all cells dirty, run the SurfaceTool pipeline, generate normals, apply material, create collision, sync persistence, and regenerate grass.

**File:** `rust/src/chunk.rs` — replace the two stub methods from Part 06 (Step 11) with:

```rust
    /// Rebuild the mesh using SurfaceTool with CUSTOM0-2 format.
    /// Uses the stored terrain_material reference (set during initialize_terrain).
    pub fn regenerate_mesh(&mut self) {
        // Use stored material reference instead of None
        let material = self.terrain_material.clone();
        self.regenerate_mesh_with_material(material);
    }

    /// Rebuild the mesh using SurfaceTool with CUSTOM0-2 format.
    /// Material is passed as parameter to avoid needing to bind terrain.
    pub fn regenerate_mesh_with_material(&mut self, terrain_material: Option<Gd<ShaderMaterial>>) {
        // Clear geometry cache to force full regeneration (debug: isolate caching issues)
        self.cell_geometry.clear();

        // Mark all cells as needing update
        let (dim_x, dim_z) = self.get_dimensions_xz();
        for z in 0..(dim_z - 1) {
            for x in 0..(dim_x - 1) {
                self.needs_update[z as usize][x as usize] = true;
            }
        }

        let mut st = SurfaceTool::new_gd();
        st.begin(PrimitiveType::TRIANGLES);
        st.set_custom_format(0, CustomFormat::RGBA_FLOAT);
        st.set_custom_format(1, CustomFormat::RGBA_FLOAT);
        st.set_custom_format(2, CustomFormat::RGBA_FLOAT);

        self.generate_terrain_cells(&mut st);

        st.generate_normals();
        st.index();

        let mesh = st.commit();
        if let Some(mesh) = mesh {
            // Apply terrain material if available (passed as parameter)
            if let Some(ref mat) = terrain_material {
                mesh.clone()
                    .cast::<ArrayMesh>()
                    .surface_set_material(0, mat);
            }
            self.base_mut().set_mesh(&mesh);
        }

        // Remove old collision body before creating a new one (prevents leaking StaticBody3D children)
        let children = self.base().get_children();
        for i in (0..children.len()).rev() {
            if let Some(child) = children.get(i) {
                if child.is_class("StaticBody3D") {
                    let mut child = child;
                    self.base_mut().remove_child(&child);
                    child.queue_free();
                }
            }
        }

        // Create trimesh collision
        self.base_mut().create_trimesh_collision();

        // Configure collision layer on the generated StaticBody3D
        self.configure_collision_layer();

        // Sync data to packed arrays for persistence
        self.sync_to_packed();

        // Regenerate grass on top of the new mesh
        // Pass cell_geometry directly to avoid grass planter needing to bind chunk
        if let Some(ref mut planter) = self.grass_planter {
            planter
                .bind_mut()
                .regenerate_all_cells_with_geometry(&self.cell_geometry);
        }

        godot_print!(
            "PixyTerrainChunk: Mesh regenerated for chunk ({}, {})",
            self.chunk_coords.x,
            self.chunk_coords.y
        );
    }
```

**What's happening:**

The pipeline has 8 stages:

1. **Clear cache + mark dirty**: `cell_geometry.clear()` + loop setting all `needs_update` = true. This forces full regeneration. Incremental updates (only regenerating changed cells) use the dirty flag system from Part 06 — but for a full rebuild, everything starts fresh.

2. **SurfaceTool setup**: `begin(TRIANGLES)` starts a triangle list. The three `set_custom_format()` calls register CUSTOM0, CUSTOM1, and CUSTOM2 as `RGBA_FLOAT` channels. This matches the shader's `CUSTOM_DATA` inputs:
   - COLOR = ground color channel 0 (texture encoding)
   - CUSTOM0 = ground color channel 1
   - CUSTOM1 = grass mask (R=mask value, G=ridge flag)
   - CUSTOM2 = material blend data (3 texture IDs + 2 blend weights)

3. **Cell generation**: `generate_terrain_cells()` (Step 2) iterates every cell and pushes vertices into the SurfaceTool.

4. **Normal generation**: `st.generate_normals()` computes per-vertex normals from triangle winding order. `st.index()` deduplicates shared vertices into an index buffer.

5. **Mesh commit + material**: `st.commit()` produces an `ArrayMesh`. The material is applied via `surface_set_material(0, mat)` — the mesh has one surface (index 0) containing all the terrain geometry.

6. **Collision cleanup + creation**: Before creating new collision, we remove any existing `StaticBody3D` child. Without this cleanup, every `regenerate_mesh()` call would leak a new collision body. `create_trimesh_collision()` is a Godot built-in that generates a concave collision shape from the mesh triangles.

7. **Persistence sync**: `sync_to_packed()` (Part 06, Step 9) writes current data to exported PackedArrays.

8. **Grass regeneration**: Passes the `cell_geometry` cache directly to the planter. This is a key design decision — the planter reads floor triangle positions from the cached geometry instead of re-querying the mesh. This avoids a borrow cycle (planter would need to bind chunk to read its mesh).

### Step 2: Implement `generate_terrain_cells()`

**Why:** This is the heart of chunk mesh generation. It iterates every cell in the chunk grid, constructs a `CellContext` for the marching squares algorithm, calls `generate_cell()` (Part 05), validates the output, and caches the geometry for incremental updates.

**File:** `rust/src/chunk.rs` (add this method to the same `impl PixyTerrainChunk` block, after the methods from Step 1)

```rust
    /// Generate terrain cells, using cached geometry for unchanged cells.
    fn generate_terrain_cells(&mut self, st: &mut Gd<SurfaceTool>) {
        let dim = self.get_terrain_dimensions();
        let cell_size = self.get_cell_size();
        let merge_threshold = MergeMode::from_index(self.merge_mode).threshold();
        let blend_mode = self.get_blend_mode();
        let use_ridge_texture = self.get_use_ridge_texture();
        let ridge_threshold = self.get_ridge_threshold();

        // Get chunk position once for wall UV2 offset
        let chunk_position = if self.base().is_inside_tree() {
            self.base().get_global_position()
        } else {
            self.base().get_position()
        };

        // Create CellContext once, moving color maps in (avoids cloning per cell).
        // Maps are moved back after the loop.
        let default_color = Color::from_rgba(1.0, 0.0, 0.0, 0.0);
        let mut ctx = CellContext {
            heights: [0.0; 4],
            edges: [true; 4],
            rotation: 0,
            cell_coords: Vector2i::ZERO,
            dimensions: dim,
            cell_size,
            merge_threshold,
            higher_poly_floors: self.higher_poly_floors,
            color_map_0: std::mem::take(&mut self.color_map_0),
            color_map_1: std::mem::take(&mut self.color_map_1),
            wall_color_map_0: std::mem::take(&mut self.wall_color_map_0),
            wall_color_map_1: std::mem::take(&mut self.wall_color_map_1),
            grass_mask_map: std::mem::take(&mut self.grass_mask_map),
            cell_min_height: 0.0,
            cell_max_height: 0.0,
            cell_is_boundary: false,
            cell_floor_lower_color_0: default_color,
            cell_floor_upper_color_0: default_color,
            cell_floor_lower_color_1: default_color,
            cell_floor_upper_color_1: default_color,
            cell_wall_lower_color_0: default_color,
            cell_wall_upper_color_0: default_color,
            cell_wall_lower_color_1: default_color,
            cell_wall_upper_color_1: default_color,
            cell_mat_a: 0,
            cell_mat_b: 0,
            cell_mat_c: 0,
            blend_mode,
            use_ridge_texture,
            ridge_threshold,
            is_new_chunk: self.is_new_chunk,
            floor_mode: true,
            lower_thresh: 0.3,
            upper_thresh: 0.7,
            chunk_position,
        };

        for z in 0..(dim.z - 1) {
            for x in 0..(dim.x - 1) {
                let key = [x, z];

                // If geometry didn't change, replay cached geometry
                if !self.needs_update[z as usize][x as usize] {
                    if let Some(geo) = self.cell_geometry.get(&key) {
                        let _ = replay_geometry(st, geo);
                        continue;
                    }
                }

                // Mark cell as updated
                self.needs_update[z as usize][x as usize] = false;

                // Get corner heights: A(top-left), B(top-right), C(bottom-left), D(bottom-right)
                let ay = self.height_map[z as usize][x as usize];
                let by = self.height_map[z as usize][(x + 1) as usize];
                let cy = self.height_map[(z + 1) as usize][x as usize];
                let dy = self.height_map[(z + 1) as usize][(x + 1) as usize];

                // Update per-cell context fields (reuse shared ctx)
                // CRITICAL: Reset ALL per-cell state to avoid corruption from previous cells
                ctx.heights = [ay, by, dy, cy];
                ctx.edges = [true; 4];
                ctx.rotation = 0;
                ctx.cell_coords = Vector2i::new(x, z);
                ctx.cell_min_height = 0.0;
                ctx.cell_max_height = 0.0;
                ctx.cell_is_boundary = false;
                ctx.floor_mode = true;

                // Reset color/material state that persists from previous cell
                ctx.cell_floor_lower_color_0 = default_color;
                ctx.cell_floor_upper_color_0 = default_color;
                ctx.cell_floor_lower_color_1 = default_color;
                ctx.cell_floor_upper_color_1 = default_color;
                ctx.cell_wall_lower_color_0 = default_color;
                ctx.cell_wall_upper_color_0 = default_color;
                ctx.cell_wall_lower_color_1 = default_color;
                ctx.cell_wall_upper_color_1 = default_color;
                ctx.cell_mat_a = 0;
                ctx.cell_mat_b = 0;
                ctx.cell_mat_c = 0;

                let mut geo = CellGeometry::default();

                // Generate geometry for this cell
                marching_squares::generate_cell(&mut ctx, &mut geo);

                // Validate geometry before commit - must have complete triangles
                if geo.verts.len() % 3 != 0 {
                    godot_error!(
                        "Cell ({}, {}) generated invalid geometry: {} verts (not divisible by 3). Heights: [{:.2}, {:.2}, {:.2}, {:.2}]. Replacing with flat floor.",
                        x, z, geo.verts.len(),
                        ctx.heights[0], ctx.heights[1], ctx.heights[2], ctx.heights[3]
                    );
                    // Reset and generate safe fallback (flat floor)
                    geo = CellGeometry::default();
                    ctx.rotation = 0;
                    marching_squares::add_full_floor(&mut ctx, &mut geo);
                }

                // Commit geometry to SurfaceTool
                let _ = replay_geometry(st, &geo);

                // Cache the geometry
                self.cell_geometry.insert(key, geo);
            }
        }

        // Move color maps back (may have been mutated by new_chunk source map writes)
        self.color_map_0 = ctx.color_map_0;
        self.color_map_1 = ctx.color_map_1;
        self.wall_color_map_0 = ctx.wall_color_map_0;
        self.wall_color_map_1 = ctx.wall_color_map_1;
        self.grass_mask_map = ctx.grass_mask_map;

        if self.is_new_chunk {
            self.is_new_chunk = false;
        }
    }
```

**What's happening:**

This method has four key patterns worth understanding deeply:

**Pattern 1: `std::mem::take()` for color maps**

```rust
color_map_0: std::mem::take(&mut self.color_map_0),
```

`std::mem::take()` replaces `self.color_map_0` with an empty Vec and gives ownership of the original data to `ctx`. This avoids cloning 33×33 = 1,089 Color values per map (6 maps × 1,089 = 6,534 clones per rebuild). After the loop, we move them back:

```rust
self.color_map_0 = ctx.color_map_0;
```

Why not just pass references? Because `generate_cell()` needs to *mutate* the color maps when `is_new_chunk` is true (writing source colors back for newly created chunks). Rust won't let you hold `&mut` to `self.color_map_0` while also passing `&mut self` to other methods.

**Pattern 2: Per-cell context reset**

Every field in `ctx` that changes per-cell must be explicitly reset. Missing a reset means cell N's state leaks into cell N+1. The most dangerous: `ctx.rotation` — if not reset to 0, the next cell starts with the previous cell's rotation offset, producing completely wrong geometry.

**Pattern 3: Incremental update with cached geometry**

```rust
if !self.needs_update[z as usize][x as usize] {
    if let Some(geo) = self.cell_geometry.get(&key) {
        let _ = replay_geometry(st, geo);
        continue;
    }
}
```

If a cell hasn't changed, we skip the expensive `generate_cell()` call and replay the cached geometry directly. This makes brush operations fast — only the cells under the brush are regenerated. For a full rebuild (this method clears the cache), this path isn't taken.

**Pattern 4: Geometry validation fallback**

```rust
if geo.verts.len() % 3 != 0 {
    geo = CellGeometry::default();
    ctx.rotation = 0;
    marching_squares::add_full_floor(&mut ctx, &mut geo);
}
```

If a marching squares case produces an incomplete triangle (vertex count not divisible by 3), the entire cell's geometry is replaced with a flat floor. This is a safety net — it should never trigger with correct case implementations, but it prevents mesh corruption from propagating if a case has a bug.

### Step 3: Implement `replay_geometry()`

**Why:** This free function commits cached `CellGeometry` to a SurfaceTool. It's used both during initial generation (Step 2) and during incremental updates when replaying unchanged cells.

**File:** `rust/src/chunk.rs` (add this as a module-level function after the `impl` blocks, at the end of the file)

```rust
/// Replay cached geometry into a SurfaceTool.
/// Returns true if geometry was valid and added, false if skipped due to invalid vertex count.
fn replay_geometry(st: &mut Gd<SurfaceTool>, geo: &CellGeometry) -> bool {
    // CRITICAL: Skip cells with incomplete triangles to prevent index errors
    if geo.verts.len() % 3 != 0 {
        godot_warn!(
            "Skipping cell with invalid vertex count: {} (not divisible by 3)",
            geo.verts.len()
        );
        return false;
    }

    for i in 0..geo.verts.len() {
        // Additional NaN guard on vertex position
        let vert = geo.verts[i];
        if !vert.x.is_finite() || !vert.y.is_finite() || !vert.z.is_finite() {
            godot_warn!(
                "Skipping vertex with NaN/Inf coordinates: ({}, {}, {})",
                vert.x,
                vert.y,
                vert.z
            );
            // Skip the entire cell since we can't have incomplete triangles
            return false;
        }

        let smooth_group = if geo.is_floor[i] { 0 } else { u32::MAX };
        st.set_smooth_group(smooth_group);
        st.set_uv(geo.uvs[i]);
        st.set_uv2(geo.uv2s[i]);
        st.set_color(geo.colors_0[i]);
        st.set_custom(0, geo.colors_1[i]);
        st.set_custom(1, geo.grass_mask[i]);
        st.set_custom(2, geo.mat_blend[i]);
        st.add_vertex(vert);
    }
    true
}
```

**What's happening:**

The SurfaceTool API works in a specific order: set all vertex attributes, then call `add_vertex()` to commit. Each `add_vertex()` call consumes all previously-set attributes and produces one vertex in the mesh.

**Vertex attribute mapping** (how each CellGeometry array maps to a shader input):

| CellGeometry field | SurfaceTool call | Shader variable | Purpose |
|---|---|---|---|
| `colors_0[i]` | `set_color()` | `COLOR` | Ground texture pair selector |
| `colors_1[i]` | `set_custom(0, ...)` | `CUSTOM0` | Second ground texture selector |
| `grass_mask[i]` | `set_custom(1, ...)` | `CUSTOM1` | Grass mask + ridge flag |
| `mat_blend[i]` | `set_custom(2, ...)` | `CUSTOM2` | 3-texture blend IDs + weights |
| `uvs[i]` | `set_uv()` | `UV` | Standard texture coordinates |
| `uv2s[i]` | `set_uv2()` | `UV2` | World-space offset for walls |

**Smooth groups**: Floor vertices get group 0, wall vertices get `u32::MAX`. Godot's normal generation smooths normals within the same group. Different groups create hard edges at the floor-to-wall transition — this gives the terrain its characteristic blocky-meets-smooth look.

**NaN guard**: The third and final line of defense. If a NaN vertex somehow survived `add_point()` (Part 03) and `set_height_at()` (Part 06), it's caught here and the entire cell is skipped. A mesh with even one NaN vertex may crash Godot's rendering pipeline.

### Step 4: Implement collision layer configuration

**Why:** After `create_trimesh_collision()` generates a StaticBody3D child, we need to configure its collision layer. The terrain uses layer 17 (bit 16) as its base layer — this keeps it separate from game physics (typically layers 1-8) so the editor's raycast can target terrain specifically.

**File:** `rust/src/chunk.rs` (add to the `impl PixyTerrainChunk` block, after `generate_terrain_cells`)

```rust
    /// Configure collision layer on the StaticBody3D created by create_trimesh_collision().
    /// Sets layer 17 (bit 16) as the base terrain collision layer, plus any extra layer
    /// specified by the terrain's extra_collision_layer setting.
    fn configure_collision_layer(&mut self) {
        // Look for the collision body child created by create_trimesh_collision()
        // It will be named something like "ChunkName_col"
        let children = self.base().get_children();
        for i in 0..children.len() {
            let Some(child) = children.get(i) else {
                continue;
            };
            if let Ok(mut body) = child.try_cast::<StaticBody3D>() {
                // Set layer 17 (bit 16) as base terrain layer
                body.set_collision_layer(1 << 16);

                // Add extra collision layer from cached terrain config
                let extra = self.terrain_config.extra_collision_layer;
                if (1..=32).contains(&extra) {
                    body.set_collision_layer_value(extra, true);
                }
                return;
            }
        }
    }
```

**What's happening:**
- `create_trimesh_collision()` (called in Step 1) generates a child node named `{NodeName}_col`. It's a `StaticBody3D` with a `CollisionShape3D` child containing a `ConcavePolygonShape3D`.
- `set_collision_layer(1 << 16)` clears all layers and sets only bit 16 (layer 17). Godot layers are 1-indexed in the UI but 0-indexed in the API.
- `set_collision_layer_value(extra, true)` adds an additional layer. The editor plugin's raycast uses `collision_mask(1 << 16)` to specifically target terrain chunks, while the extra layer allows game code to also detect terrain on a different layer.

## Verify

```bash
cd rust && cargo build
```

The project now compiles with a fully functional chunk data model and mesh generation pipeline. While the terrain manager (Part 09) isn't built yet, you can test chunk generation in isolation by:
1. Creating a `PixyTerrainChunk` in a Godot scene
2. Calling `set_terrain_config()` with default config
3. Calling `generate_height_map_with_noise(None)` for a flat terrain
4. Calling `generate_color_maps()`, `generate_wall_color_maps()`, `generate_grass_mask_map()`
5. Calling `regenerate_mesh()` — this produces a flat mesh at Y=0

Without a shader material, the mesh will render with Godot's default material (white/gray).

## What You Learned

- **SurfaceTool pipeline**: `begin()` → set formats → per-vertex: set attributes then `add_vertex()` → `generate_normals()` → `index()` → `commit()`
- **CUSTOM format channels**: Three RGBA_FLOAT channels provide 12 extra floats per vertex — enough for texture encoding, grass masking, and material blending data
- **`std::mem::take()` pattern**: Move ownership of Vec data into a temporary struct, process it, then move it back. Zero-cost alternative to cloning.
- **Smooth group trick**: Floor=0, wall=MAX creates hard normals at the floor-wall boundary while keeping each surface smooth internally
- **Collision layer architecture**: Editor raycast targets layer 17 specifically, keeping terrain detection separate from game physics
- **Fallback geometry**: When a marching squares case produces invalid output, replace with a flat floor rather than crashing

## Stubs Introduced

- None — all chunk functionality is now implemented (mesh generation replaces the Part 06 stubs)

## Stubs Resolved

- [x] `PixyTerrainChunk::regenerate_mesh()` — introduced in Part 06, now fully implemented
- [x] `PixyTerrainChunk::regenerate_mesh_with_material()` — introduced in Part 06, now fully implemented
