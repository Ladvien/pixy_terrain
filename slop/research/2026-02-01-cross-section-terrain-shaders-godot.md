# Cross-section shaders for Godot 4 terrain systems

Implementing terrain cross-sections in Godot 4.3+ requires combining **fragment discard** for clipping, **two-pass rendering** for interior surfaces, and **world-space noise** for geological strata. The core technique clips geometry using signed distance from a plane while revealing interior layers through back-face rendering with triplanar texturing. Godot lacks native clipping plane support, making custom spatial shaders the only viable approach—but this method performs well even on large terrains when implemented correctly.

## Camera-relative clipping with `discard` and world coordinates

The foundation of any cross-section shader is calculating world-space position and discarding fragments on one side of a clipping plane. In Godot 4.x, pass world position from vertex to fragment shader using `MODEL_MATRIX`:

```glsl
shader_type spatial;
render_mode cull_disabled;

uniform vec3 clip_plane_normal = vec3(0.0, 1.0, 0.0);
uniform float clip_offset : hint_range(-50.0, 50.0) = 0.0;

varying vec3 world_position;

void vertex() {
    world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
    // Camera-relative horizontal clipping plane
    vec3 camera_pos = CAMERA_POSITION_WORLD;
    float clip_plane_y = camera_pos.y + clip_offset;
    
    float signed_distance = dot(world_position - vec3(0.0, clip_plane_y, 0.0), clip_plane_normal);
    if (signed_distance < 0.0) {
        discard;
    }
    
    ALBEDO = vec3(0.6);
}
```

**`CAMERA_POSITION_WORLD`** is a built-in variable in Godot 4.x spatial shaders that provides camera world position directly—no uniform passing required. A bug affecting `CAMERA_DIRECTION_WORLD` in Godot 4.2.1 was fixed in **4.3**, making camera-relative calculations reliable.

### Discard vs alpha scissor performance tradeoffs

Using `discard` breaks **early-Z rejection** because the GPU must run the fragment shader before knowing whether to write depth. This impacts tile-based deferred rendering (TBDR) mobile GPUs significantly and can disable Hi-Z acceleration on desktop.

**Alpha scissor** provides an alternative with better depth sorting behavior:

```glsl
render_mode depth_draw_opaque, cull_disabled;

void fragment() {
    ALPHA = signed_distance >= 0.0 ? 1.0 : 0.0;
    ALPHA_SCISSOR_THRESHOLD = 0.5;
}
```

Important: Godot activates scissor threshold mode if `ALPHA_SCISSOR_THRESHOLD` appears **anywhere** in shader code, even in unreachable branches. For large terrain meshes, consider chunk-based culling (pre-processing mesh or visibility layers) rather than per-fragment discard when performance is critical.

### Global uniforms for multi-material clipping

When multiple materials need synchronized clipping, use global shader uniforms defined in **Project Settings → Shader Globals**:

```glsl
global uniform vec3 clip_plane_position;
global uniform vec3 clip_plane_normal;
```

Update from GDScript:
```gdscript
func _process(delta):
    RenderingServer.global_shader_parameter_set(
        "clip_plane_position", 
        camera.global_position + Vector3(0, clip_offset, 0)
    )
```

## Two-pass rendering reveals the cut interior

A single-pass shader can only color existing mesh surfaces—it cannot fill the void where geometry was clipped. The standard solution uses **two render passes** via Godot's `next_pass` material property: first pass renders back faces (interior), second pass renders front faces (exterior).

**Pass 1 — Interior fill (cull_front):**
```glsl
shader_type spatial;
render_mode blend_mix, depth_draw_always, unshaded, cull_front;

varying vec3 world_pos;
uniform vec4 interior_color : source_color = vec4(0.8, 0.3, 0.2, 1.0);
uniform vec3 plane_position = vec3(0.0);
uniform vec3 plane_normal = vec3(0.0, 1.0, 0.0);

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    VERTEX -= NORMAL * 0.001;  // Bias prevents z-fighting
}

void fragment() {
    float d = dot(world_pos - plane_position, plane_normal);
    if (d < 0.0) discard;
    ALBEDO = interior_color.rgb;
}
```

**Pass 2 — Exterior surface (assign to `next_pass`, set `render_priority = 1`):**
```glsl
shader_type spatial;
render_mode cull_back, diffuse_burley, specular_schlick_ggx;

varying vec3 world_pos;
uniform sampler2D terrain_texture : source_color;
uniform vec3 plane_position = vec3(0.0);
uniform vec3 plane_normal = vec3(0.0, 1.0, 0.0);

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
    float d = dot(world_pos - plane_position, plane_normal);
    if (d < 0.0) discard;
    ALBEDO = texture(terrain_texture, UV).rgb;
}
```

The **vertex bias** (`VERTEX -= NORMAL * 0.001`) in pass 1 is critical—without it, z-fighting creates flickering artifacts where both passes compete for the same depth value.

### Single-shader alternative using FRONT_FACING

For simpler use cases, detect face orientation within one shader:

```glsl
shader_type spatial;
render_mode cull_disabled;

void fragment() {
    if (signed_distance < 0.0) discard;
    
    if (FRONT_FACING) {
        ALBEDO = exterior_color;
    } else {
        ALBEDO = interior_color;
        NORMAL = -NORMAL;  // Essential for correct lighting
    }
}
```

Always flip `NORMAL` for back faces when using `cull_disabled`—otherwise lighting calculations produce incorrect results with dark or inverted shading.

## Triplanar mapping for textured cross-sections

Cut surfaces lack UV coordinates, making traditional texture mapping impossible. **Triplanar projection** samples textures from three orthogonal world-space projections, weighted by surface normal:

```glsl
uniform sampler2D strata_texture : source_color, repeat_enable, filter_linear_mipmap;
uniform float triplanar_scale : hint_range(0.01, 10.0) = 0.2;
uniform float triplanar_sharpness : hint_range(0.5, 20.0) = 5.0;

varying vec3 triplanar_pos;
varying vec3 triplanar_weight;

void vertex() {
    triplanar_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz * triplanar_scale;
    
    vec3 world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
    triplanar_weight = pow(abs(world_normal), vec3(triplanar_sharpness));
    triplanar_weight /= dot(triplanar_weight, vec3(1.0));  // Normalize
}

vec3 triplanar_sample(sampler2D tex, vec3 pos, vec3 weight) {
    return (texture(tex, pos.zy).rgb * weight.x +   // X projection
            texture(tex, pos.xz).rgb * weight.y +   // Y projection (top-down)
            texture(tex, pos.xy).rgb * weight.z);   // Z projection
}

void fragment() {
    if (!FRONT_FACING) {
        ALBEDO = triplanar_sample(strata_texture, triplanar_pos, triplanar_weight);
        NORMAL = -NORMAL;
    } else {
        ALBEDO = texture(terrain_texture, UV).rgb;
    }
}
```

The `triplanar_sharpness` uniform controls blending between projections—higher values create sharper transitions (useful for rocky surfaces), while lower values blend smoothly (better for soil).

## Procedural geological strata using noise and Y-coordinate

Underground visualization requires layered coloring based on world-space depth. The technique combines **Y-coordinate thresholds** with **noise offsets** for natural-looking wavy strata.

### Basic layered strata with smooth transitions

```glsl
uniform float layer1_depth = -1.0;   // Topsoil
uniform float layer2_depth = -4.0;   // Clay  
uniform float layer3_depth = -10.0;  // Rock
uniform float transition_width = 0.5;

uniform vec3 topsoil_color : source_color = vec3(0.35, 0.25, 0.15);
uniform vec3 clay_color : source_color = vec3(0.65, 0.45, 0.35);
uniform vec3 rock_color : source_color = vec3(0.55, 0.50, 0.50);
uniform vec3 bedrock_color : source_color = vec3(0.30, 0.28, 0.32);

vec3 calculate_strata_color(float y) {
    float blend1 = smoothstep(layer1_depth - transition_width, 
                               layer1_depth + transition_width, y);
    float blend2 = smoothstep(layer2_depth - transition_width,
                               layer2_depth + transition_width, y);
    float blend3 = smoothstep(layer3_depth - transition_width,
                               layer3_depth + transition_width, y);
    
    vec3 color = bedrock_color;
    color = mix(color, rock_color, blend3);
    color = mix(color, clay_color, blend2);
    color = mix(color, topsoil_color, blend1);
    return color;
}
```

### Adding natural waviness with fractal noise

Real geological layers aren't perfectly horizontal. Sample a noise texture using XZ position to offset Y:

```glsl
uniform sampler2D noise_texture : hint_default_white;
uniform float noise_scale = 0.05;
uniform float noise_amplitude = 2.5;

float fbm(vec2 p) {
    float value = 0.0;
    float amp = 0.5;
    for (int i = 0; i < 4; i++) {
        value += amp * texture(noise_texture, p).r;
        p *= 2.0;
        amp *= 0.5;
    }
    return value;
}

void fragment() {
    float noise_offset = (fbm(world_position.xz * noise_scale) - 0.5) * noise_amplitude;
    float wavy_y = world_position.y + noise_offset;
    
    ALBEDO = calculate_strata_color(wavy_y);
}
```

For **complex geological folding**, apply domain warping—nested noise functions that distort the input coordinates:

```glsl
float domain_warped_fbm(vec2 p) {
    vec2 q;
    q.x = fbm(p + vec2(0.0, 0.0));
    q.y = fbm(p + vec2(5.2, 1.3));
    
    vec2 r;
    r.x = fbm(p + 4.0 * q + vec2(1.7, 9.2));
    r.y = fbm(p + 4.0 * q + vec2(8.3, 2.8));
    
    return fbm(p + 4.0 * r);
}
```

This technique, described extensively by Inigo Quilez, produces organic undulating patterns that resemble real sedimentary formations.

## Complete integrated cross-section shader

This shader combines clipping, two-sided rendering, triplanar texturing, and procedural strata:

```glsl
shader_type spatial;
render_mode cull_disabled, depth_draw_opaque;

// Clipping
uniform vec3 clip_plane_position = vec3(0.0);
uniform vec3 clip_plane_normal = vec3(0.0, 1.0, 0.0);
uniform float edge_highlight_width = 0.1;
uniform vec3 edge_color : source_color = vec3(1.0, 0.8, 0.2);

// Exterior
uniform sampler2D terrain_texture : source_color;

// Interior strata
uniform sampler2D noise_texture : hint_default_white;
uniform float noise_scale = 0.03;
uniform float noise_amplitude = 3.0;
uniform float layer1_depth = -1.0;
uniform float layer2_depth = -5.0;
uniform float layer3_depth = -12.0;
uniform vec3 layer1_color : source_color = vec3(0.40, 0.30, 0.18);
uniform vec3 layer2_color : source_color = vec3(0.60, 0.42, 0.30);
uniform vec3 layer3_color : source_color = vec3(0.50, 0.48, 0.45);
uniform vec3 layer4_color : source_color = vec3(0.32, 0.30, 0.34);

varying vec3 world_pos;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

float fbm(vec2 p) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) {
        v += a * texture(noise_texture, p).r;
        p *= 2.0; a *= 0.5;
    }
    return v;
}

vec3 strata_color(float y) {
    float tw = 0.4;
    float b1 = smoothstep(layer1_depth - tw, layer1_depth + tw, y);
    float b2 = smoothstep(layer2_depth - tw, layer2_depth + tw, y);
    float b3 = smoothstep(layer3_depth - tw, layer3_depth + tw, y);
    vec3 c = mix(layer4_color, layer3_color, b3);
    c = mix(c, layer2_color, b2);
    return mix(c, layer1_color, b1);
}

void fragment() {
    float dist = dot(world_pos - clip_plane_position, normalize(clip_plane_normal));
    if (dist < 0.0) discard;
    
    if (FRONT_FACING) {
        // Exterior with edge highlight
        if (dist < edge_highlight_width) {
            ALBEDO = edge_color;
            EMISSION = edge_color * 0.3;
        } else {
            ALBEDO = texture(terrain_texture, UV).rgb;
        }
    } else {
        // Interior: procedural strata
        float noise_offset = (fbm(world_pos.xz * noise_scale) - 0.5) * noise_amplitude;
        float wavy_y = world_pos.y + noise_offset;
        ALBEDO = strata_color(wavy_y);
        
        // Add detail variation
        float detail = texture(noise_texture, world_pos.xz * 0.4).r;
        ALBEDO *= 0.9 + detail * 0.2;
        
        NORMAL = -NORMAL;
    }
    
    ROUGHNESS = 0.85;
}
```

## Godot 4.3+ specific considerations

**Reverse-Z depth buffer** introduced in Godot 4.3 flips near/far plane conventions. If writing directly to `DEPTH`, adjust accordingly—but using `PROJECTION_MATRIX` for transformations handles this automatically.

**Stencil buffer support** arrives in Godot 4.5, enabling proper capping techniques. The classic OpenGL approach draws a capping polygon only where the stencil buffer indicates the plane intersects solid geometry. Until 4.5, the two-pass approach described above remains the standard workaround.

**Key render modes** for cross-section shaders:

| Mode | Purpose |
|------|---------|
| `cull_disabled` | Render both faces |
| `cull_front` | Interior pass only |
| `depth_draw_always` | Write depth even with discard |
| `world_vertex_coords` | VERTEX in world space automatically |

## Existing resources and addons

**Terrain3D** and **HTerrain** plugins both support visual holes using alpha channel masking with shader discard—the same technique used for cross-sections. Neither includes dedicated cross-section visualization, but their shader architecture demonstrates production-quality implementation patterns.

**godotshaders.com** hosts several relevant examples: the **Intersection Dissolve** shader (Godot 4 compatible) clips geometry using sphere SDF with smooth transitions, while the **3D Vertical Dissolve** demonstrates Y-axis cutoffs with emission borders. These require porting older `WORLD_MATRIX` references to `MODEL_MATRIX` for Godot 4.

No dedicated cross-section addon exists in the Asset Library—custom shaders remain the standard approach. The GitHub proposal for native clipping plane support (#1753) remains open, suggesting this will continue to require manual implementation for the foreseeable future.

## Conclusion

Building terrain cross-sections in Godot 4 requires layering several techniques: signed distance clipping via `discard`, two-pass rendering with face culling for interior surfaces, triplanar projection for cut-face texturing, and procedural noise for geological realism. The **`CAMERA_POSITION_WORLD`** built-in and **global shader uniforms** enable camera-following behavior without script overhead. Performance scales well for static cross-sections—the primary cost is `discard` preventing early-Z optimization, which becomes significant only with extremely high triangle counts or on mobile TBDR GPUs. For production terrain, combine these shader techniques with chunk-based mesh culling to minimize discarded fragments.
