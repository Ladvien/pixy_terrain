# Multi-Texture System

**Status:** Implemented
**Date:** 2026-02-11
**Files:** `rust/src/marching_squares/vertex.rs` (color encoding), `rust/src/marching_squares/types.rs` (TextureIndex), `godot/addons/pixy_terrain/resources/shaders/mst_terrain.gdshader`

## Summary

16-slot texture system using vertex color pairs to encode texture indices, with three rendering paths of increasing cost: hard textures, phantom fix (3-texture blend), and full 16-weight vertex color blending.

## What It Does

- Encodes up to 16 texture selections per vertex using two RGBA color channels
- Renders terrain floors and walls with per-pixel texture blending
- Projects wall textures using biplanar projection (X/Z planes) to avoid stretching
- Adds procedural noise to blend boundaries for organic transitions
- Detects ridges at cliff edges and renders them as wall material
- Applies per-texture UV scaling (15 independent scale factors)
- Tints the first 6 texture slots with configurable ground albedo colors

## Scope

**Covers:** Texture index encoding, shader rendering paths, wall projection, blend noise, ridge detection, UV scaling, ground albedo tinting.

**Does not cover:** Toon lighting (see toon-shading spec), grass rendering, editor paint tools.

## Interface

### Shader Uniforms

#### Albedo Group
| Uniform | Type | Default | Purpose |
|---------|------|---------|---------|
| `wall_threshold` | float | 0.0 | dot(normal, up) cutoff for floor vs wall |
| `wall_depth_bias` | float | 0.0005 | Clip-space depth bias to prevent Z-fighting on walls |
| `chunk_size` | ivec3 | (33, 32, 33) | Grid dimensions for UV tiling |
| `cell_size` | vec2 | (2.0, 2.0) | World units per cell for wall UV projection |
| `ground_albedo` through `ground_albedo_6` | vec4 | Various greens | Tint colors for texture slots 0-5 |

#### Blending Group
| Uniform | Type | Default | Purpose |
|---------|------|---------|---------|
| `use_hard_textures` | bool | false | Pixel-art mode: one texture per cell, no blending |
| `blend_mode` | int | 0 | Floor rendering path (0=auto, 2=per-vertex index) |
| `blend_sharpness` | float | 5.0 | Transition steepness (0=soft gradient, 10=sharp) |
| `blend_noise_scale` | float | 10.0 | Noise frequency for blend edge variation |
| `blend_noise_strength` | float | 0.0 | Noise influence on blend weights |

#### Texture Scales (15 uniforms)
- `texture_scale_1` through `texture_scale_15`: float, default 1.0, range 0.1-20.0
- 2.0 = tile twice (more detailed), 0.5 = tile half (bigger)

#### Texture Slots (16 samplers)
- `vc_tex_rr` through `vc_tex_aa`: sampler2D with `source_color, filter_nearest`
- Named by channel pair: `vc_tex_XY` where X = COLOR channel, Y = CUSTOM0 channel
- Forms 4x4 grid: rr=0, rg=1, rb=2, ra=3, gr=4, ..., aa=15

### Vertex Attribute Layout

| Attribute | Godot Slot | Content |
|-----------|------------|---------|
| COLOR | COLOR | Primary texture one-hot RGBA |
| CUSTOM0 | CUSTOM0 | Secondary texture one-hot RGBA |
| CUSTOM1 | CUSTOM1 | R=grass mask, G=ridge flag |
| CUSTOM2 | CUSTOM2 | R=packed material indices, G=mat_c, B=weight_a, A=weight_b or sentinel |
| UV | UV | Floor/wall blend UVs |
| UV2 | UV2 | World-space floor position for tiling |

### Rust-Side Encoding

```rust
TextureIndex = dominant_channel(color_0) * 4 + dominant_channel(color_1)
// Range: 0-15
// Encode: TextureIndex::from_color_pair(c0, c1)
// Decode: TextureIndex::to_color_pair() -> (Color, Color)
```

## Behavior Details

### Three Rendering Paths (Floor)

**Path 1: Hard Textures** (cheapest)
- When: `use_hard_textures = true`
- Behavior: One texture per triangle, no blending. Pixel-perfect boundaries.
- `blend_mode == 2`: use per-vertex material_index (flat varying)
- `blend_mode != 2`: use dominant material from CUSTOM2 packed data

**Path 2: Vertex Color Blending** (most expensive)
- When: `CUSTOM2.a >= 1.5` (boundary cell sentinel set by Rust)
- Behavior: Samples up to 16 textures weighted by channel products
- Weight calculation: `raw_weight[i] = vc0_channel * vc1_channel`
- Power sharpening: `weight = pow(raw_weight, SHARPNESS_BASE + sharpness * SHARPNESS_SCALE)`
- Normalize to sum=1.0; fallback picks single max weight if all crushed to zero
- Optional noise: varies effective sharpness per-pixel for organic edges

**Path 3: Phantom Fix Blending** (default, fast)
- When: not hard textures and not boundary cell
- Behavior: Uses 3 texture IDs + 2 weights from CUSTOM2. Maximum 3 texture samples.
- Decodes mat_a, mat_b, mat_c from packed indices
- weight_c derived: `1.0 - weight_a - weight_b`
- Optional noise shifts weights per-pixel
- Optional sharpness via power curve

### Wall Rendering: Biplanar Projection

- Floor/wall classification: `dot(normal, up) > wall_threshold && ridge_flag < 0.5`
- Two UV projections:
  - X-facing walls: `uv_x = vertex.zy / cell_size`
  - Z-facing walls: `uv_z = vertex.xy / cell_size`
- Biplanar weights from `abs(normal)`: `weights = (abs_normal.x, 0, abs_normal.z)`, normalized
- Final color = `texture_x * weight.x + texture_z * weight.z`
- Wall blending uses `snap_to_dominant()` on vertex colors for crisp material boundaries (fights GPU interpolation bleed)

### Z-Fighting Prevention

In vertex shader, walls get a clip-space depth bias:
```glsl
if (dot(NORMAL, vec3(0, 1, 0)) <= wall_threshold) {
    clip_pos.z += wall_depth_bias * clip_pos.w;
}
```
Resolves depth buffer competition between wall backfaces (cull_disabled) and coplanar floor surfaces.

### Ridge Detection

- CUSTOM1.g carries ridge flag (1.0 near cliff top, 0.0 otherwise)
- If `is_ridge >= RIDGE_THRESHOLD (0.5)`, vertex renders as wall material even if normal points up
- Controlled by `use_ridge_texture` export on PixyTerrain
- Ridge threshold configurable via `ridge_threshold` export

### Procedural Noise Blending

- `hash(vec2)`: pseudo-random from 2D position (standard shader hash constants)
- `noise(vec2)`: smooth value noise via bilinear interpolation of hashed grid corners with smoothstep
- Applied to vertex color blending path: varies effective_sharpness per-pixel
- Applied to phantom fix path: shifts weight_a/b/c per-pixel
- Scale and strength configurable via uniforms

### Ground Albedo Tinting

- Slots 0-5: multiplied by corresponding `ground_albedo_N` tint color
- Slots 6-14: rendered untinted (texture color as-is)
- Slot 15 (VOID_TEXTURE): transparent, no scale override

## Acceptance Criteria

- TextureIndex round-trips correctly for all 16 values (unit tested)
- Shader correctly decodes vertex colors with `CHANNEL_THRESHOLD = 0.1` (survives GPU interpolation)
- Hard texture mode produces crisp per-cell boundaries
- Boundary cells blend smoothly across texture transitions
- Wall textures project without visible stretching on angled surfaces
- No Z-fighting visible at wall/floor boundaries

## Technical Notes

- Shader uses `render_mode diffuse_toon, depth_draw_opaque, cull_disabled`
- `cull_disabled` needed because walls can be viewed from either side
- GLSL cannot index into uniforms by variable, requiring switch statements for texture scale and material sampling
- `WEIGHT_SKIP_THRESHOLD = 0.01` skips texture samples contributing < 1% (performance optimization)
- Constants: `SHARPNESS_BASE = 2.0`, `SHARPNESS_SCALE = 2.0`, `CUSTOM2_UNPACK = 255.0`, `MAT_PACK_STRIDE = 16.0`
