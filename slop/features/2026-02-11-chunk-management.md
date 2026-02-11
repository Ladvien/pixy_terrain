# Chunk Management

**Status:** Implemented
**Date:** 2026-02-11
**Files:** `rust/src/chunk.rs`, `rust/src/terrain.rs`

## Summary

Grid-based terrain organization where each chunk owns a heightmap, color maps, and cached geometry. Chunks support lazy mesh regeneration via dirty-cell tracking and persist data through Godot's PackedArray serialization.

## What It Does

- Organizes terrain into a grid of `PixyTerrainChunk` nodes (MeshInstance3D), each covering a configurable number of cells
- Tracks which cells need regeneration after edits (dirty-cell system)
- Caches per-cell geometry to skip unchanged cells during mesh rebuilds
- Shares edge vertices between adjacent chunks for seamless boundaries
- Generates trimesh collision shapes on configurable physics layers
- Persists heightmap and color data as PackedArrays in .tscn scene files

## Scope

**Covers:** Chunk creation/removal, heightmap storage, color map storage, dirty tracking, geometry caching, collision generation, persistence, cross-chunk edge sharing.

**Does not cover:** Marching squares algorithm (see terrain-geometry spec), shader rendering, editor tools, grass placement.

## Interface

### PixyTerrainChunk (GodotClass: MeshInstance3D, tool)

#### Export Properties

| Property | Type | Default | Purpose |
|----------|------|---------|---------|
| `chunk_coords` | Vector2i | (0,0) | Grid position of this chunk |
| `merge_mode` | i32 | 1 (Polyhedron) | Per-chunk merge mode override |
| `higher_poly_floors` | bool | true | Higher-polygon floor geometry |
| `skip_save_on_exit` | bool | false | Skip persisting data on scene exit |

#### Persisted Data (PackedArrays)

| Field | Type | Layout |
|-------|------|--------|
| `saved_height_map` | PackedFloat32Array | Row-major: `z * dim_x + x` |
| `saved_color_map_0` | PackedColorArray | Primary floor texture color |
| `saved_color_map_1` | PackedColorArray | Secondary floor texture color |
| `saved_wall_color_map_0` | PackedColorArray | Primary wall texture color |
| `saved_wall_color_map_1` | PackedColorArray | Secondary wall texture color |
| `saved_grass_mask_map` | PackedColorArray | Per-vertex grass visibility |

#### GDScript API (#[func] methods)

**Height:**
- `get_height(coords: Vector2i) -> f32`
- `draw_height(x, z, y)` -- sets height, marks 4 cells dirty, notifies neighbors

**Color (6 read/write pairs):**
- `draw_color_0(x, z, color)` / `get_color_0(x, z) -> Color`
- `draw_color_1(x, z, color)` / `get_color_1(x, z) -> Color`
- `draw_wall_color_0(x, z, color)` / `get_wall_color_0(x, z) -> Color`
- `draw_wall_color_1(x, z, color)` / `get_wall_color_1(x, z) -> Color`

**Grass Mask:**
- `draw_grass_mask(x, z, color)` / `get_grass_mask_at(x, z) -> Color`

**Mesh:**
- `regenerate_all_cells()` -- mark all cells dirty, rebuild mesh
- `validate_mesh_gaps() -> i32` -- check watertightness, log gaps, return count

### PixyTerrain (GodotClass: Node3D, tool)

#### Chunk Management API

- `add_new_chunk(chunk_x, chunk_z)` -- create chunk, copy shared edges from neighbors, regenerate
- `remove_chunk(x, z)` -- remove and free chunk
- `remove_chunk_from_tree(x, z)` -- remove without freeing (for undo/redo)
- `has_chunk(x, z) -> bool`
- `get_chunk(x, z) -> Option<Gd<PixyTerrainChunk>>`
- `get_chunk_keys() -> PackedVector2Array`
- `clear()` -- remove all chunks
- `regenerate()` -- clear all, create single chunk at (0,0)

#### Batch Operations

- `apply_composite_pattern(patterns: VarDictionary)` -- apply multi-layer changes atomically
  - Layers: "height", "color_0", "color_1", "wall_color_0", "wall_color_1", "grass_mask"
  - Structure: `{layer: {chunk_coords: {cell_coords: value}}}`
- `regenerate_all_grass()` -- rebuild grass on all chunks
- `force_batch_update()` -- sync all shader parameters to terrain material

## Behavior Details

### Heightmap Layout

- Default dimensions: 33x33 vertices per chunk (32x32 cells)
- Runtime: `Vec<Vec<f32>>` indexed as `height_map[z][x]`
- Persisted: `PackedFloat32Array` in row-major order (`z * dim_x + x`)
- Values: float heights, typically 0.0 to `dimensions.y`

### Dirty-Cell Tracking

When a heightmap vertex at (x, z) is modified via `draw_height()`:

1. **4-cell notification**: The vertex is shared by up to 4 cells, so `notify_neighbors()` marks cells at (x,z), (x-1,z), (x,z-1), (x-1,z-1) dirty
2. **3x3 kernel expansion**: `mark_cell_needs_update()` further expands each dirty cell to its 3x3 neighborhood, because edge cases and boundary profiles depend on adjacent cell heights
3. **Cache invalidation**: Each newly-dirty cell has its `CellGeometry` removed from the cache

Grid: `needs_update: Vec<Vec<bool>>` with dimensions `(dim_x - 1) x (dim_z - 1)`.

### Lazy Mesh Regeneration

During `generate_terrain_cells()`:
- For each cell (x, z):
  - If `needs_update[z][x]` is false AND cached geometry exists: replay cached geometry (fast path)
  - Otherwise: call `generate_cell()`, store result in cache
- After generation: `needs_update` flags are reset, `sync_to_packed()` keeps persisted data current

### CellGeometry Cache

- Storage: `HashMap<[i32; 2], CellGeometry>` keyed by `[cell_x, cell_z]`
- Contains: verts, UVs, colors, grass mask, material blend, is_floor flags
- Invalidated: removed when cell marked dirty
- Replayed: `replay_geometry()` copies cached vertex data directly into mesh arrays

### Cross-Chunk Edge Sharing

When `add_new_chunk(chunk_x, chunk_z)` is called:
- **Left neighbor**: copy rightmost column heights -> new chunk's leftmost column
- **Right neighbor**: copy leftmost column -> rightmost column
- **Up neighbor**: copy bottom row -> top row
- **Down neighbor**: copy top row -> bottom row

This ensures shared vertices have identical heights. BoundaryProfile then guarantees identical geometry along shared edges.

### Collision Generation

After mesh generation:
1. `create_trimesh_collision()` creates a StaticBody3D + CollisionShape3D child
2. Primary layer: 17 (bit 16, `1 << 16`)
3. Extra layer: configurable via `extra_collision_layer` (default: 9, range 1-32)
4. Collision body hidden by default

### Persistence Cycle

**Save (exit_tree -> sync_to_packed):**
- Flattens `Vec<Vec<f32>>` height_map into `PackedFloat32Array`
- Converts all `Vec<Color>` color maps into `PackedColorArray`
- Godot serializes these packed arrays into .tscn

**Load (initialize_terrain -> restore_from_packed):**
- Validates packed array sizes match `dim_x * dim_z`
- Reconstructs runtime `Vec<>` from packed arrays
- Falls back to noise generation if packed arrays are empty (new chunk)

**Also syncs after mesh generation** to ensure Ctrl+S captures current state.

### Chunk World Positioning

```
pos.x = chunk_x * (dim_x - 1) * cell_size.x
pos.z = chunk_z * (dim_z - 1) * cell_size.y
pos.y = 0.0
```

The `-1` accounts for shared edge vertices between chunks.

## Acceptance Criteria

- Chunks persist heightmap and 5 color maps through scene save/load
- Dirty-cell tracking limits mesh regeneration to affected cells only
- Cross-chunk edges produce seamless geometry (validated by watertightness tests)
- Collision shapes generated on correct physics layers

## Technical Notes

- No signals defined on either PixyTerrainChunk or PixyTerrain
- Chunks stored in `HashMap<[i32; 2], Gd<PixyTerrainChunk>>` on PixyTerrain
- `TerrainConfig` struct caches terrain settings per chunk to avoid repeated lookups
- Terrain material is shared across all chunks (single `ShaderMaterial` instance)
- Grass planter stored as optional child node named "GrassPlanter" on each chunk
