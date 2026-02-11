# Grass Vegetation

**Status:** Implemented
**Date:** 2026-02-11
**Files:** `rust/src/grass_planter.rs`, `godot/addons/pixy_terrain/resources/shaders/mst_grass.gdshader`

## Summary

MultiMesh billboard grass system that places grass instances on floor triangles using barycentric sampling, with 6 sprite texture variants, red/green channel masking, wind animation, character displacement, and per-instance quantized animation.

## What It Does

- Places grass billboards on floor geometry using barycentric coordinate testing
- Supports 6 different grass sprite textures with per-texture enable/disable toggles
- Masks grass off using red channel and forces grass on using green channel
- Animates grass with dual-noise wind sway and sine-wave idle animation
- Flattens grass near characters (up to 64 simultaneous) with power-law falloff
- Applies fake perspective UV distortion to simulate 3D depth on 2D sprites
- Quantizes animation timing per-instance with seeded phase offsets for organic variety

## Scope

**Covers:** Grass placement algorithm, sprite variant selection, masking, MultiMesh setup, wind animation, character displacement, fake perspective, toon lighting on grass.

**Does not cover:** Terrain mesh generation, terrain shader rendering, editor grass mask tools.

## Interface

### PixyGrassPlanter (GodotClass: MultiMeshInstance3D, tool)

No `#[func]` methods exposed to GDScript. Internal Rust API only:

- `setup_with_config(chunk_id, config: GrassConfig, force_rebuild: bool)` -- initialize from terrain config
- `regenerate_all_cells_with_geometry(cell_geometry: &HashMap<[i32; 2], CellGeometry>)` -- rebuild all grass from cached geometry

No signals defined.

### GrassConfig (passed from PixyTerrain)

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `dimensions` | Vector3i | (33, 32, 33) | Terrain grid size |
| `subdivisions` | i32 | 3 | Samples per cell axis (3 = 9 per cell) |
| `grass_size` | Vector2 | (1.0, 1.0) | Quad mesh width/height |
| `cell_size` | Vector2 | (2.0, 2.0) | World units per cell |
| `merge_mode` | MergeMode | Polyhedron | For wall threshold |
| `ledge_threshold` | f32 | 0.25 | UV threshold to avoid grass on ledges |
| `ridge_threshold` | f32 | 1.0 | UV threshold to avoid grass on ridges |
| `grass_sprites` | [Option\<Texture2D\>; 6] | None | 6 optional grass textures |
| `ground_colors` | [Color; 6] | Various greens | Fallback colors per texture |
| `tex_has_grass` | [bool; 5] | [true; 5] | Per-texture grass toggle (slots 2-6) |
| `grass_material` | Option\<ShaderMaterial\> | None | Shared grass shader material |
| `ground_images` | [Option\<Image\>; 6] | None | Texture images for color sampling |
| `texture_scales` | [f32; 6] | [1.0; 6] | Per-texture UV scale factors |

### PixyTerrain Export Properties (Grass-Related)

**Sprites:** `grass_sprite`, `grass_sprite_tex_2` through `grass_sprite_tex_6`
**Placement:** `grass_subdivisions` (1-10), `grass_size` (Vector2)
**Per-texture toggles:** `tex2_has_grass` through `tex6_has_grass` (all default true)
**Ground colors:** `ground_color` through `ground_color_6`
**Character displacement:** `character_displacement_enabled`, `player_displacement_angle_z/x`, `radius_exponent`, `displacement_radius`, `character_group_name` ("pixy_characters")
**Animation:** `grass_framerate`, `grass_quantised`, `world_space_sway`, `world_sway_angle`, `fake_perspective_scale`, `view_space_sway`, `view_sway_speed`, `view_sway_angle`
**Wind:** `wind_direction`, `wind_scale`, `wind_speed`
**Toon lighting:** `grass_toon_cuts`, `grass_toon_wrap`, `grass_toon_steepness`, `grass_threshold_gradient_size`

### Shader Uniforms (mst_grass.gdshader)

**Albedo:** `grass_texture` (primary), `grass_texture_2` through `grass_texture_6`, `grass_base_color`, `grass_color_2` through `grass_color_6`, `use_base_color` through `use_base_color_6`, `use_grass_tex_2` through `use_grass_tex_6`

**Animation:** `framerate` (5.0), `quantised` (true), `world_space_sway` (true), `world_sway_angle` (60.0), `fake_perspective_scale` (0.3), `view_space_sway` (true), `view_sway_speed` (0.1), `view_sway_angle` (10.0)

**Character Displacement:** `character_displacement` (true), `player_displacement_angle_z` (45.0), `player_displacement_angle_x` (45.0), `radius_exponent` (1.0), `character_positions` (vec4[64])

**Toon Lighting:** `cuts` (3), `wrap` (0.0), `steepness` (1.0), `threshold_gradient_size` (0.2), `shadow_color`

**Global (from PixyEnvironment):** `cloud_noise`, `cloud_scale`, `cloud_world_y`, `cloud_speed`, `cloud_contrast`, `cloud_threshold`, `cloud_direction`, `light_direction`, `cloud_shadow_min`, `wind_noise`, `wind_noise_scale`, `wind_noise_speed`, `wind_noise_direction`

## Behavior Details

### Barycentric Placement Algorithm

For each cell in the chunk:
1. Generate jittered sample points: `subdivisions x subdivisions` per cell with random offsets
2. For each floor triangle (where `is_floor[tri] == true`):
   - Precompute 2D barycentric basis vectors (XZ projection) once per triangle
   - For each sample point:
     - Compute barycentric coordinates (u, v, w = 1-u-v)
     - If u >= 0, v >= 0, u+v <= 1: point is inside triangle
     - Interpolate 3D position: `p = a*w + b*u + c*v`
     - Interpolate colors and mask using same weights

### Grass Masking

**Red channel (disable):** `mask.r < 0.9999` -> grass is masked OFF
**Green channel (force on):** `mask.g >= 0.9999` -> grass forced ON regardless of texture/ledge

Mask is interpolated from `geo.grass_mask` per-vertex Color array using barycentric weights.

### Texture Selection

Texture ID derived from interpolated vertex colors:
```rust
c0 * 4 + c1 + 1  // Results in 1-16
```

- texture_id 1: always has grass (base texture)
- texture_id 2-6: controlled by `tex_has_grass` toggles
- texture_id 7-16: no grass (additional texture combinations)

### Ledge/Ridge Avoidance

```rust
let on_ledge = uv.x > 1.0 - config.ledge_threshold || uv.y > 1.0 - config.ridge_threshold;
```

Grass placement skipped on geometry near ledges and ridges (UV encodes floor/wall proximity).

### MultiMesh Instance Setup

- Instance count: `(dim_x - 1) * (dim_z - 1) * subdivisions^2`
- Example: 32x32 cells with 3 subdivisions = 9,216 instances
- Transform: oriented perpendicular to triangle normal
- Hidden instances: scaled to zero and positioned at (9999, 9999, 9999)

### Custom Data Encoding

```rust
instance_color.rgb = sampled_terrain_color;  // Ground color from texture image or fallback
instance_color.a = match texture_id {         // Encodes sprite variant
    6 => 1.0, 5 => 0.8, 4 => 0.6, 3 => 0.4, 2 => 0.2, _ => 0.0
};
```

### Billboard Rendering

Shader replaces MODELVIEW_MATRIX to create camera-facing billboards:
```glsl
mat4 modified_model_view = VIEW_MATRIX * mat4(
    vec4(INV_VIEW_MATRIX[0].xyz, 0.0),
    vec4(INV_VIEW_MATRIX[1].xyz, 0.0),
    vec4(INV_VIEW_MATRIX[2].xyz, 0.0),
    MODEL_MATRIX[3]
);
```

### Wind Animation (Dual-Noise)

- Two noise texture samples at different scales (1.0x and 0.8x) and diverged directions
- Multiplied together for complex, non-repeating patterns
- Returns normalized range [-1.0, 1.0]
- Applied as world-space rotation around wind-perpendicular axis

### Quantized Per-Instance Animation

```glsl
float seed = 10.0 * location_seed(object_origin.xy);
float phase = mod(seed, 1.0 / framerate);
time = round((TIME + phase) * framerate) / framerate;
```

Each grass instance gets a deterministic phase offset from its world position, creating stepped animation at `framerate` FPS with organic per-blade timing variation.

### Character Displacement

- Iterates all 64 character positions per vertex
- `distance = length(char_pos - grass_pos) / char_radius`
- `strength = pow(1.0 - distance, radius_exponent)` (power-law falloff)
- Displacement projected into camera-relative X/Z via dot products
- Applied as two rotations: Z-axis and X-axis with configurable angles
- Accumulates all characters, clamped to [-1, 1]

### Fake Perspective UV Distortion

- Centers UV at 0.5, modifies X based on wind and character displacement
- `uv.x *= (1.0 - uv.y) * scale + 1.0` -- more distortion at bottom, less at top
- Wind distortion scaled by dot product with camera direction
- Character distortion inverted (bends away from character)

## Acceptance Criteria

- Grass appears only on floor triangles, not on walls
- Red mask channel removes grass; green channel forces it
- All 6 sprite variants render with correct textures and colors
- Grass animates with wind and responds to character proximity
- No grass placed on ledges/ridges when thresholds are set

## Technical Notes

- Shader uses `render_mode blend_mix, depth_draw_opaque, cull_disabled, diffuse_toon`
- `LIGHT_VERTEX = model_origin` ensures flat lighting across the entire billboard
- Alpha scissor threshold: 0.5 (strict cutoff for clean billboard edges)
- Material sharing: single grass_material created by terrain, shared across all chunk planters
- Mesh selection priority: custom grass_mesh > grass_quad_mesh > fallback QuadMesh
- `character_group_name` ("pixy_characters") determines which nodes are tracked for displacement [INFERRED from terrain exports]
