# Toon Shading & Atmosphere

**Status:** Implemented
**Date:** 2026-02-11
**Files:** `godot/addons/pixy_terrain/resources/shaders/mst_terrain.gdshader` (light function), `godot/addons/pixy_terrain/resources/shaders/mst_grass.gdshader` (light function + cloud sampling)

## Summary

Cel-shaded lighting system with discrete brightness bands for terrain and grass, cloud shadow projection via raycast to a noise-driven cloud plane, and Z-fighting prevention on walls via clip-space depth bias.

## What It Does

- Replaces smooth PBR lighting with quantized toon bands on terrain surfaces
- Applies a separate toon lighting model on grass with configurable wrap, steepness, and smooth threshold gradients
- Projects animated cloud shadows onto grass using a raycast from vertex position to a fixed-height cloud plane
- Prevents Z-fighting between wall and floor surfaces at cliff edges using depth bias
- Sets global shader parameters (cloud_noise, wind_noise, light_direction) at terrain startup

## Scope

**Covers:** Terrain toon lighting, grass toon lighting, cloud shadow projection, Z-fighting prevention, global shader parameter initialization.

**Does not cover:** Texture blending (see multi-texture spec), grass placement (see grass-vegetation spec), wind animation details (see grass-vegetation spec).

## Interface

### Terrain Shader Uniforms (Shading Group)

| Uniform | Type | Default | Purpose |
|---------|------|---------|---------|
| `shadow_color` | vec4 | (0,0,0,1) | Tint for shadowed regions |
| `bands` | int | 5 | Number of discrete brightness steps (1-10) |
| `shadow_intensity` | float | 0.0 | Darkest shadow level (-1.0 to 0.5) |

### Grass Shader Uniforms (Lighting Group)

| Uniform | Type | Default | Purpose |
|---------|------|---------|---------|
| `cuts` | int | 3 | Number of toon bands (1-8) |
| `wrap` | float | 0.0 | Light wrap: simulates subsurface scattering (-2.0 to 2.0) |
| `steepness` | float | 1.0 | Contrast multiplier for toon transitions (1.0-8.0) |
| `threshold_gradient_size` | float | 0.2 | Smoothness between bands (0 = hard, 1 = very soft) |
| `shadow_color` | vec4 | source_color | Shadow tint color |

### Terrain Z-Fighting Uniforms (Albedo Group)

| Uniform | Type | Default | Purpose |
|---------|------|---------|---------|
| `wall_threshold` | float | 0.0 | dot(normal, up) cutoff for floor vs wall |
| `wall_depth_bias` | float | 0.0005 | Clip-space depth push for wall geometry |

### Global Shader Parameters (set by PixyTerrain/PixyEnvironment)

**Cloud System:**
- `cloud_noise` (sampler2D) -- noise texture for cloud patterns
- `cloud_scale` (float) -- world-space sampling scale
- `cloud_world_y` (float) -- fixed Y coordinate of cloud plane
- `cloud_speed` (float) -- cloud animation speed
- `cloud_contrast` (float) -- shadow intensity modulation
- `cloud_threshold` (float) -- visibility threshold
- `cloud_direction` (vec2) -- normalized wind direction for clouds
- `light_direction` (vec3) -- directional light direction (for raycast)
- `cloud_shadow_min` (float) -- minimum shadow intensity (prevents complete blackness)
- `cloud_diverge_angle` (float) -- angle spread for dual-noise sampling

**Wind System:**
- `wind_noise` (sampler2D) -- noise texture for wind patterns
- `wind_noise_threshold` (float) -- activation threshold
- `wind_noise_scale` (float) -- world-space sampling scale
- `wind_noise_speed` (float) -- wind animation speed
- `wind_noise_direction` (vec2) -- normalized wind direction
- `wind_diverge_angle` (float) -- angle spread for dual-noise

### PixyTerrain Export Properties

- `grass_toon_cuts`: i32 = 3
- `grass_toon_wrap`: f32 = 0.0
- `grass_toon_steepness`: f32 = 1.0
- `grass_threshold_gradient_size`: f32 = 0.2

Terrain shadow_color and bands are set directly on the terrain material shader, not via terrain exports [INFERRED].

## Behavior Details

### Terrain Toon Lighting (mst_terrain.gdshader `light()`)

```glsl
float NdotL = dot(NORMAL, LIGHT) * ATTENUATION;
float stepped = floor(NdotL * float(bands)) / float(bands);
float light_amount = max(stepped + shadow_intensity, 0.0);
vec3 lit_color = mix(shadow_color.rgb * ALBEDO, ALBEDO, light_amount);
DIFFUSE_LIGHT += lit_color * LIGHT_COLOR * TOON_LIGHT_MAX;
```

1. Standard Lambert: `dot(normal, light_dir) * attenuation`
2. Quantize to `bands` discrete steps (5 bands -> 0.0, 0.2, 0.4, 0.6, 0.8)
3. Blend between shadow-tinted albedo and full albedo
4. `TOON_LIGHT_MAX = 0.3` caps each light contribution to prevent overbrightening
5. Accumulates across multiple lights

### Grass Toon Lighting (mst_grass.gdshader `light()`)

More sophisticated than terrain lighting with smooth threshold gradients:

```glsl
float NdotL = dot(NORMAL, LIGHT);
float diffuse_amount = NdotL + (ATTENUATION - 1.0) + wrap;
diffuse_amount *= steepness;
```

1. Base diffuse: NdotL + attenuation + wrap (subsurface simulation)
2. Multiply by steepness for contrast control
3. Apply cloud shadows on directional lights (see below)
4. Quantize to `cuts` bands with optional smooth threshold gradients

**Threshold Gradient Smoothing:**
- Finds nearest band threshold
- Creates smooth transition zone: `+/- halfWidth` around threshold
- `halfWidth = 0.5 * cut_size * threshold_gradient_size`
- Uses `smoothstep(low, high, diffuse_amount)` within transition zone
- Falls back to hard `step()` if zone collapses

**Final output:**
```glsl
vec3 diffuse = ALBEDO.rgb * LIGHT_COLOR / PI;
diffuse *= diffuse_stepped;
DIFFUSE_LIGHT = max(DIFFUSE_LIGHT, diffuse);
```

Note: grass uses `max()` accumulation (not additive like terrain) to prevent double-brightening from multiple lights.

### Cloud Shadow Projection

Applied only to grass, only on directional lights:

**Raycast to Cloud Plane:**
```glsl
float t = (cloud_world_y - world_pos.y) / light_direction.y;
vec3 hit_pos = world_pos + t * light_direction;
```

Intersects a ray from vertex position along light direction with a horizontal plane at `cloud_world_y`.

**Dual-Noise Cloud Sampling:**
- Two noise texture samples at different scales (1.0x and 0.8x)
- Directions diverged by `+/- cloud_diverge_angle`
- Second sample scrolls at different speed (0.89 * PI / 3.0 multiplier)
- Multiplied together: `cloud_sample = sample1 * sample2`
- Creates complex, non-tiling cloud patterns

**Shadow Application:**
```glsl
float light_value = clamp(cloud_sample + cloud_threshold, 0.0, 1.0);
light_value = (light_value - 0.5) * cloud_contrast + 0.5;
light_value = clamp(light_value + cloud_threshold, cloud_shadow_min, 1.0);
diffuse_amount = min(diffuse_amount, light_value);
```

- Threshold controls minimum cloud visibility
- Contrast boost centered at 0.5
- `cloud_shadow_min` prevents complete darkness under clouds
- Cloud shadow caps the diffuse amount (cannot make brighter than base lighting)

### Z-Fighting Prevention

In the terrain vertex shader:
```glsl
vec4 clip_pos = PROJECTION_MATRIX * MODELVIEW_MATRIX * vec4(VERTEX, 1.0);
float normal_up = dot(NORMAL, vec3(0.0, 1.0, 0.0));
if (normal_up <= wall_threshold) {
    clip_pos.z += wall_depth_bias * clip_pos.w;
}
POSITION = clip_pos;
```

- Detects wall geometry by normal direction
- Pushes wall fragments slightly further from camera in clip space
- `wall_depth_bias` default 0.0005 is enough to resolve competition
- Multiplied by `clip_pos.w` to make bias perspective-correct

### Global Parameter Initialization

PixyTerrain sets global shader parameters at startup [INFERRED from terrain code]:
- Cloud noise texture and configuration
- Wind noise texture and configuration
- Light direction from scene's directional light

These are available to all materials in the scene, not just terrain/grass.

## Acceptance Criteria

- Terrain shows discrete brightness bands matching `bands` count
- Grass shows discrete bands with smooth gradients when `threshold_gradient_size > 0`
- Cloud shadows appear on grass surfaces as moving dark patches
- No Z-fighting visible at wall/floor cliff edges
- Shadow color tints shadowed regions appropriately

## Technical Notes

- Terrain uses additive light accumulation (`+=`) capped by TOON_LIGHT_MAX
- Grass uses max-based accumulation (`max()`) to prevent multi-light artifacts
- Grass uses `LIGHT_VERTEX = model_origin` for flat shading (entire billboard lit uniformly)
- Cloud shadows only apply to directional lights (`LIGHT_IS_DIRECTIONAL`)
- Dual-noise technique shared between cloud and wind systems with identical structure but different parameters
- The `rotate_vec2()` helper rotates 2D vectors for divergence angle application
