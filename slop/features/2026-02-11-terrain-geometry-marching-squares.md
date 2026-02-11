# Terrain Geometry: Marching Squares

**Status:** Implemented
**Date:** 2026-02-11
**Files:** `rust/src/marching_squares/` (mod.rs, cases.rs, primitives.rs, vertex.rs, cell_context.rs, types.rs, validator.rs)

## Summary

Converts a per-vertex heightmap into watertight 3D terrain meshes using a modified marching squares algorithm with 23 geometric cases, 5 merge modes, and canonical boundary profiles for cross-cell matching.

## What It Does

- Takes 4 corner heights per cell and produces triangulated 3D geometry (floors, walls, corners)
- Determines whether adjacent corners form slopes (merged) or walls (not merged) based on a configurable merge threshold
- Guarantees watertight geometry within cells and across cell boundaries
- Encodes per-vertex texture indices, grass masks, and material blend weights into vertex attributes

## Scope

**Covers:** Heightmap-to-mesh conversion, case matching, geometry primitives, vertex attribute encoding, cross-cell boundary matching, watertightness validation.

**Does not cover:** Chunk management, shader rendering, editor tools, grass placement.

## Interface

### Public Entry Point

```rust
pub fn generate_cell(ctx: &mut CellContext, geo: &mut CellGeometry)
```

Called once per cell. Reads heights from `ctx`, appends triangles to `geo`.

### Key Types

| Type | Purpose |
|------|---------|
| `CellContext` | All inputs for one cell: heights, colors, merge threshold, rotation, profiles |
| `CellGeometry` | Output container: verts, UVs, colors, grass mask, material blend, is_floor flags |
| `MergeMode` | Enum with 5 threshold presets (Cubic through Spherical) |
| `BoundaryProfile` | Canonical edge description from two shared corner heights |
| `TextureIndex(u8)` | 0-15 texture slot encoded as two one-hot RGBA color pairs |
| `BlendMode` | Interpolated (bilinear across corners) or Direct (corner A only) |
| `ValidationResult` | Watertightness check output: `is_watertight` + list of open edges |

### MergeMode Thresholds

| Mode | Threshold | Effect |
|------|-----------|--------|
| Cubic | 0.6 | Sharp, blocky (voxel-like) |
| Polyhedron | 1.3 | Default; good balance |
| RoundedPolyhedron | 2.1 | Smoother slopes |
| SemiRound | 5.0 | Very rounded, fewer walls |
| Spherical | 20.0 | Almost everything merged |

Higher threshold = more edges treated as slopes instead of walls.

## Behavior Details

### Case Matching Algorithm

1. Compute edge connectivity: `edges[i] = true` if `|h[i] - h[(i+1)%4]| < merge_threshold`
   - edges: [AB (top), BD (right), CD (bottom), AC (left)]
2. Pre-compute `BoundaryProfile` for each edge
3. Fast path: all 4 edges merged -> full floor (case 0)
4. Try all 4 rotations to find a matching case pattern
5. Each case calls primitive builders that query boundary profiles for vertex heights

### The 23 Cases

| Case | Pattern | Geometry |
|------|---------|----------|
| 0 | All edges merged | Full floor (8 triangles, center fan) |
| 1 | A raised, BD+CD merged | Outer corner at A |
| 2 | A,B higher than C,D | Raised edge AB |
| 3 | AB edge with A corner | Half-width edge AB + outer corner at A |
| 4 | AB edge with B corner | Half-width edge AB + outer corner at B |
| 5 | B,C raised; A,D lowered; BC merged | Two inner corners + diagonal floor |
| 5.5 | Like 5 but B > C | Two inner corners + outer corner at B |
| 6 | A lowest, BCD merged | Single inner corner at A |
| 7 | A lowest, BD merged, C > D | Inner corner + asymmetric geometry |
| 8 | A lowest, CD merged, B > D | Inner corner + asymmetric geometry |
| 9 | A lowest, B~C merged, D highest | Inner corner + diagonal + outer corner |
| 10 | A lowest, BD merged, D > C | Inner corner + edge bridge |
| 11 | A lowest, CD merged, D > B | Inner corner + edge bridge |
| 12-15 | Spiral/staircase patterns | Inner corner + edge + outer corner |
| 16 | Degenerate merged edge | Reuses case 2 logic |
| 17 | A highest, D lowest, all different | Outer corner + diagonal + inner corner |
| 18 | A highest, B~C merged, D lowest | Outer corner + diagonal + outer corner |
| 19-20 | A higher with partial edges | Outer corner + partial edge |
| 21 | A raised, B~C merged, D lowest | Outer corner + inner corner composite |
| 22-23 | Single wall at AC or BD boundary | Wall-only cases |

Cases 9 and 18 are currently unreachable (known latent issue due to pattern ordering).

### Geometry Primitives

| Primitive | When Used | Description |
|-----------|-----------|-------------|
| `add_full_floor` | Case 0 | 8 triangles in fan from center point E, splits every boundary at midpoint |
| `add_outer_corner` | Cases 1, 19, 20, 21 | Upper floor triangle at raised corner + walls + lower floor |
| `add_edge` | Cases 2, 3, 4, partial edges | Floor strip across AB + wall between AC/BD + lower floor |
| `add_inner_corner` | Cases 6, 7, 8, 10, 11 | Small floor at lowered corner + wall + upper BCD floor |
| `add_diagonal_floor` | Cases 5, 9, 17, 18 | 4 triangles connecting B-C diagonal across A and D |

### BoundaryProfile (Cross-Cell Matching)

The critical design for watertightness:

```rust
pub fn compute_boundary_profile(h1: f32, h2: f32, merge_threshold: f32) -> BoundaryProfile
```

Depends ONLY on the two shared corner heights + merge_threshold. Both adjacent cells compute identical profiles for their shared edge.

`height_at(t, is_upper)`:
- Merged: linear interpolation from h1 to h2
- Walled, is_upper=true: `max(h1, h2)` (wall top)
- Walled, is_upper=false: `min(h1, h2)` (wall bottom)

Key pattern: when floor level differs from wall top/bottom, split boundary edges at midpoint using floor height as intermediate (not wall height).

### TextureIndex Encoding

16 textures encoded as two one-hot RGBA vertex colors:

```
TextureIndex = dominant_channel(color_0) * 4 + dominant_channel(color_1)
```

- (R,R) = 0, (R,G) = 1, ..., (A,A) = 15
- Round-trip: `TextureIndex::from_color_pair()` / `to_color_pair()`
- Shader decodes via `get_material_index()` using channel threshold detection

### Material Blend Data (CUSTOM2 vertex attribute)

For cells with multiple textures meeting:
- `R`: Two material indices packed as `(mat_a + mat_b * 16) / 255`
- `G`: Third material index normalized to 0..1
- `B`: Weight for material A
- `A`: Weight for material B (or sentinel 2.0 for boundary cells needing 16-weight blending)

### Vertex Color Sampling

- `BlendMode::Interpolated`: Bilinear interpolation from 4 corner colors, then take dominant channel
- `BlendMode::Direct`: Use corner A color directly
- Boundary detection: if cell height range > merge_threshold, interpolate between lower/upper colors based on vertex height

### Grass Mask Encoding

- `red`: Grass density (from grass_mask_map)
- `green`: Ridge texture flag (1.0 if near cliff top, 0.0 otherwise)

## Acceptance Criteria

- Within-cell geometry: fully watertight across 160,000+ height combinations (brute-force validated)
- Cross-cell boundary matching: 0 mismatches across all 15,625 adjacent-cell pairs (horizontal and vertical)
- All 43 tests pass including canonical boundary checks
- Triangle count always divisible by 3 (invalid geometry replaced with flat floor fallback)

## Technical Notes

- Ported from Yugen's GDScript Terrain Authoring Toolkit
- Corner ordering: [A, B, D, C] (not [A, B, C, D]) -- D and C are swapped relative to typical grid order
- Rotation system: 0-3 rotations tested sequentially; first matching case wins
- CellGeometry is cached per cell in chunk's HashMap for lazy regeneration
- `higher_poly_floors` flag controls interior triangle density on flat floors
- Constants: `BLEND_EDGE_SENSITIVITY=1.25`, `DOMINANT_CHANNEL_THRESHOLD=0.99`, `WALL_BLEND_SENTINEL=2.0`
