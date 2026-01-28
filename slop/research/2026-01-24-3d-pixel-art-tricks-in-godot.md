# Complete 3D-to-pixel-art rendering pipeline in Godot 4.3–4.6

Implementing a real-time 3D-to-pixel-art pipeline in Godot 4.3+ requires coordinating **12 distinct rendering systems**: SubViewport configuration, camera snapping, edge detection, toon shading, color quantization, dithering, bloom, volumetrics, environmental effects, and a properly ordered post-processing stack. This report provides working code, GitHub repositories, and implementation details for each component, prioritizing Godot 4.3+ compatibility with the critical **reverse-Z depth buffer** changes introduced in April 2024.

---

## SubViewport setup establishes the pixel-perfect foundation

The entire pipeline begins with rendering your 3D scene at a deliberately low resolution (typically **320×180** or **480×270**) via a SubViewport, then upscaling with integer scaling and nearest-neighbor filtering.

**Project Settings configuration:**
```ini
[rendering]
textures/canvas_textures/default_texture_filter = 0  # Nearest

[display]
window/size/viewport_width = 320
window/size/viewport_height = 180
window/stretch/mode = "viewport"
window/stretch/scale_mode = "integer"  # Godot 4.3+
```

**Node hierarchy for SubViewport rendering:**
```
Main (Node)
├── SubViewportContainer (stretch=true, stretch_shrink=4)
│   └── SubViewport (size=322×182, snap_2d_vertices_to_pixel=true)
│       ├── Camera3D
│       └── GameContent
└── CanvasLayer (post-processing effects)
```

The SubViewport should be **1 pixel larger** than target resolution on each side (322×182 for 320×180) to accommodate the sub-texel correction technique. Key references include the [voithos/godot-smooth-pixel-camera-demo](https://github.com/voithos/godot-smooth-pixel-camera-demo) repository and [GDQuest's pixel art setup guide](https://www.gdquest.com/library/pixel_art_setup_godot4/).

---

## Camera snapping eliminates the "swimming pixel" artifact

When a camera moves through a low-resolution 3D scene, temporal artifacts cause pixels to "swim" or "creep." The fix requires a **two-step process**: snap the camera to a texel-aligned grid, then offset the rendered output in screen space by the snap error.

**Camera controller concept (GDScript):**
```gdscript
var virtual_position: Vector2 = Vector2.ZERO  # Precise position
var pixel_snap_delta: Vector2 = Vector2.ZERO  # Error from snapping

func _process(delta):
    virtual_position = virtual_position.lerp(target.global_position, smoothing * delta)
    var snapped_pos = virtual_position.round()
    global_position = snapped_pos
    pixel_snap_delta = virtual_position - snapped_pos

# In the sprite displaying the viewport:
func _process(delta):
    position = base_position + camera_controller.pixel_snap_delta * scale_factor
```

For **3D orthographic cameras**, calculate texel size as `camera.size / viewport_height` and snap accordingly. Key resources: [aarthificial's 2D explanation](https://youtu.be/jguyR4yJb1M), [David Holland's 3D adaptation](https://youtu.be/LQfAGAj9oNQ), and [David Holland's comprehensive article](https://www.davidhol.land/articles/3d-pixel-art-rendering/).

---

## The reverse-Z fix is critical for Godot 4.3+ post-processing

Godot 4.3 introduced **reverse-Z depth buffering** for improved precision, breaking shaders using the old vertex pattern. All post-process shaders applied to a MeshInstance3D quad must use:

```glsl
void vertex() {
    POSITION = vec4(VERTEX.xy, 1.0, 1.0);  // Godot 4.3+ REQUIRED
}
```

The old pattern `POSITION = vec4(VERTEX, 1.0)` places geometry at the far plane (now Z=0) instead of near plane (now Z=1). Depth values now range **1.0 (near) to 0.0 (far)**.

---

## Depth-based edge detection creates pixel-art outlines

Full-screen outline shaders sample `hint_depth_texture` and `hint_normal_roughness_texture` to detect edges via Sobel or Roberts Cross kernels.

**Complete Sobel outline shader (Godot 4.3+):**
```glsl
shader_type spatial;
render_mode unshaded;

uniform sampler2D SCREEN_TEXTURE: hint_screen_texture, filter_linear_mipmap;
uniform sampler2D DEPTH_TEXTURE: hint_depth_texture, filter_linear_mipmap;
uniform float edge_threshold = 0.1;
uniform vec3 line_color: source_color = vec3(0.0);

const mat3 sobel_x = mat3(vec3(1.0, 2.0, 1.0), vec3(0.0, 0.0, 0.0), vec3(-1.0, -2.0, -1.0));
const mat3 sobel_y = mat3(vec3(1.0, 0.0, -1.0), vec3(2.0, 0.0, -2.0), vec3(1.0, 0.0, -1.0));

float linearize_depth(vec2 uv, mat4 inv_proj) {
    float depth = texture(DEPTH_TEXTURE, uv).x;
    vec3 ndc = vec3(uv * 2.0 - 1.0, depth);
    vec4 view = inv_proj * vec4(ndc, 1.0);
    return -view.z / view.w;
}

void vertex() {
    POSITION = vec4(VERTEX.xy, 1.0, 1.0);
}

void fragment() {
    vec2 offset = 1.0 / VIEWPORT_SIZE;
    mat3 depths;
    for (int i = -1; i <= 1; i++) {
        for (int j = -1; j <= 1; j++) {
            depths[i+1][j+1] = linearize_depth(SCREEN_UV + vec2(float(i), float(j)) * offset, INV_PROJECTION_MATRIX);
        }
    }
    float edge_x = dot(sobel_x[0], depths[0]) + dot(sobel_x[1], depths[1]) + dot(sobel_x[2], depths[2]);
    float edge_y = dot(sobel_y[0], depths[0]) + dot(sobel_y[1], depths[1]) + dot(sobel_y[2], depths[2]);
    float edge = sqrt(edge_x * edge_x + edge_y * edge_y);
    
    ALBEDO = edge > edge_threshold ? line_color : texture(SCREEN_TEXTURE, SCREEN_UV).rgb;
}
```

**Key repositories:**
- [godotshaders.com high-quality outline](https://godotshaders.com/shader/high-quality-post-process-outline/)
- [leopeltola/Godot-3d-pixelart-demo](https://github.com/leopeltola/Godot-3d-pixelart-demo)
- [jocamar/Godot-Post-Process-Outlines](https://github.com/jocamar/Godot-Post-Process-Outlines)

**Important limitation:** `hint_normal_roughness_texture` only works in **Forward+ renderer**, not Mobile/Compatibility.

---

## Toon shading uses the void light() function

Godot 4's spatial shaders provide a `void light()` function that runs per-light, enabling stepped lighting and custom shadow colors.

**Basic toon shader with shadow color control:**
```glsl
shader_type spatial;

uniform vec3 base_color : source_color = vec3(1.0);
uniform vec4 shade_color : source_color = vec4(0.2, 0.1, 0.3, 1.0);
uniform float shade_threshold : hint_range(0.0, 1.0) = 0.5;
uniform float shade_softness : hint_range(0.0, 0.5) = 0.01;
uniform int shade_steps : hint_range(2, 8) = 3;

void fragment() {
    ALBEDO = base_color;
}

void light() {
    float NdotL = dot(NORMAL, LIGHT) * 0.5 + 0.5;
    float stepped = floor(NdotL * float(shade_steps)) / float(shade_steps);
    float toon = smoothstep(shade_threshold - shade_softness, shade_threshold + shade_softness, stepped);
    vec3 result = mix(shade_color.rgb * ALBEDO, ALBEDO, toon);
    DIFFUSE_LIGHT += result * LIGHT_COLOR * ATTENUATION;
}
```

**Texture ramp lighting** uses a global shader parameter (Project Settings → Shader Globals) to sample a gradient texture:
```glsl
global uniform sampler2D diffuse_curve;
void light() {
    float NdotL = dot(NORMAL, LIGHT) * 0.5 + 0.5;
    float stepped = texture(diffuse_curve, vec2(NdotL, 0.0)).r;
    DIFFUSE_LIGHT += ALBEDO * LIGHT_COLOR * stepped * ATTENUATION;
}
```

**Key repositories:** [eldskald/godot4-cel-shader](https://github.com/eldskald/godot4-cel-shader), [CaptainProton42/FlexibleToonShaderGD](https://github.com/CaptainProton42/FlexibleToonShaderGD)

---

## Color quantization and dithering complete the pixel art look

### Palette-based color matching

**Lospec palette shader (CC0):**
```glsl
shader_type canvas_item;
uniform sampler2D palette : filter_nearest;
uniform int palette_size = 16;
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_nearest;

void fragment() {
    vec4 color = texture(SCREEN_TEXTURE, SCREEN_UV);
    vec4 new_color = vec4(0.0);
    for (int i = 0; i < palette_size; i++) {
        vec4 pal = texture(palette, vec2((float(i) + 0.5) / float(palette_size), 0.0));
        if (distance(pal, color) < distance(new_color, color)) new_color = pal;
    }
    COLOR = vec4(new_color.rgb, color.a);
}
```

Download palettes from [Lospec](https://lospec.com/palette-list) as 1×N PNG images and import with `filter_nearest`.

### Bayer dithering

**4×4 Bayer matrix shader:**
```glsl
shader_type canvas_item;
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_nearest;

const mat4 bayerIndex = mat4(
    vec4(0.0/16.0, 12.0/16.0, 3.0/16.0, 15.0/16.0),
    vec4(8.0/16.0, 4.0/16.0, 11.0/16.0, 7.0/16.0),
    vec4(2.0/16.0, 14.0/16.0, 1.0/16.0, 13.0/16.0),
    vec4(10.0/16.0, 6.0/16.0, 9.0/16.0, 5.0/16.0)
);

void fragment() {
    vec4 col = texture(SCREEN_TEXTURE, SCREEN_UV);
    float bayer = bayerIndex[int(FRAGCOORD.x) % 4][int(FRAGCOORD.y) % 4];
    COLOR = vec4(step(bayer, col.r), step(bayer, col.g), step(bayer, col.b), col.a);
}
```

**Key resources:**
- [godotshaders.com/arbitrary-color-reduction-ordered-dithering](https://godotshaders.com/shader/arbitrary-color-reduction-ordered-dithering/)
- [samuelbigos/godot_dither_shader](https://github.com/samuelbigos/godot_dither_shader) (Obra Dinn-style)
- [tromero/BayerMatrix](https://github.com/tromero/BayerMatrix) – PNG textures for 2×2 through 16×16 matrices

---

## Bloom requires HDR 2D enabled in Godot 4.4+

For built-in glow, enable **Project Settings → Rendering → Viewport → HDR 2D**, then configure `WorldEnvironment` with glow threshold >1.0. Objects needing glow must have RAW color values exceeding 1.0.

**Custom pixel-perfect bloom shader for SubViewport:**
```glsl
shader_type canvas_item;
uniform float bloomRadius = 1.0;
uniform float bloomThreshold = 1.0;
uniform float bloomIntensity = 1.0;

vec3 GetBloomPixel(sampler2D tex, vec2 uv, vec2 texPixelSize) {
    vec2 uv2 = floor(uv / texPixelSize) * texPixelSize + texPixelSize * 0.001;
    vec3 tl = max(texture(tex, uv2).rgb - bloomThreshold, 0.0);
    vec3 tr = max(texture(tex, uv2 + vec2(texPixelSize.x, 0.0)).rgb - bloomThreshold, 0.0);
    vec3 bl = max(texture(tex, uv2 + vec2(0.0, texPixelSize.y)).rgb - bloomThreshold, 0.0);
    vec3 br = max(texture(tex, uv2 + texPixelSize).rgb - bloomThreshold, 0.0);
    vec2 f = fract(uv / texPixelSize);
    return mix(mix(tl, tr, f.x), mix(bl, br, f.x), f.y);
}

void fragment() {
    vec3 bloom = vec3(0.0);
    vec2 off = TEXTURE_PIXEL_SIZE * bloomRadius;
    // Sample 9 neighbors with weighted contribution
    bloom += GetBloomPixel(TEXTURE, UV + off * vec2(-1,-1), TEXTURE_PIXEL_SIZE * bloomRadius) * 0.29;
    bloom += GetBloomPixel(TEXTURE, UV + off * vec2(0,-1), TEXTURE_PIXEL_SIZE * bloomRadius) * 0.5;
    // ... (repeat for all 9 samples)
    COLOR = texture(TEXTURE, UV) + vec4(bloom * bloomIntensity, 0.0);
}
```

**Repository:** [godotshaders.com bloom for viewports](https://godotshaders.com/shader/bloom-post-processing-for-viewports/)

---

## God rays use shell texturing or screen-space radial blur

**t3ssel8r's shell texturing technique** (documented by David Holland): sample the directional shadow map at world-space coordinates across multiple parallel planes aligned with the light direction. Each plane tests whether its position is shadowed, and layered together they create volumetric light shafts.

**Simpler screen-space god rays** use radial blur from the light's screen position:
```glsl
shader_type canvas_item;
render_mode blend_add;
uniform vec2 light_source_pos;
uniform float ray_length = 1.0;
uniform float ray_intensity = 1.0;
const int SAMPLE_COUNT = 200;

void fragment() {
    vec2 dir = UV - light_source_pos;
    float rays = 0.0;
    for (int i = 0; i < SAMPLE_COUNT; i++) {
        float scale = 1.0 - ray_length * (float(i) / float(SAMPLE_COUNT - 1));
        rays += (1.0 - texture(TEXTURE, dir * scale + light_source_pos).a) / float(SAMPLE_COUNT);
    }
    COLOR = vec4(vec3(rays * ray_intensity), rays);
}
```

**Godot 4's native VolumetricFog** can create god rays automatically when combined with DirectionalLight3D shadows—set `light_volumetric_fog_energy > 0`.

**Key resources:**
- [godotshaders.com screen-space god rays](https://godotshaders.com/shader/screen-space-god-rays-godot-4-3/)
- [godotshaders.com pixelated god rays](https://godotshaders.com/shader/pixelated-god-rays/)
- [David Holland's volumetric implementation](https://www.davidhol.land/articles/3d-pixel-art-rendering/)

---

## Billboard grass benefits from the LIGHT_VERTEX technique

For even lighting across billboard quads, Godot PR #91136 introduced `LIGHT_VERTEX`—write a single base position to this variable in the fragment shader so lighting calculates from one point rather than per-fragment.

**Billboard grass with wind and player interaction:**
```glsl
shader_type spatial;
render_mode cull_disabled;
uniform float wind_strength = 0.3;
uniform float wind_speed = 1.0;
uniform vec3 player_position;
uniform float push_radius = 2.0;

void vertex() {
    vec3 world = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    // Wind displacement
    VERTEX.x += sin(world.x + TIME * wind_speed) * (1.0 - UV.y) * wind_strength;
    // Player push
    vec3 push_dir = world - player_position;
    float push = smoothstep(push_radius, 0.0, length(push_dir));
    VERTEX += normalize(push_dir) * push * (1.0 - UV.y);
    NORMAL = vec3(0.0, 1.0, 0.0);
}
```

**Key repositories:**
- [GDQuest/godot-shaders](https://github.com/GDQuest/godot-shaders) – stylized grass with wind
- [Malidos/Grass-Shader-Example](https://github.com/Malidos/Grass-Shader-Example) – MultiMesh grass
- [tylercchase/godot-grass-displacement](https://github.com/tylercchase/godot-grass-displacement) – texture-based interaction

---

## Stylized water combines refraction, waves, and depth fog

**Core techniques** include screen-space refraction via `hint_screen_texture`, Beer's Law depth coloring, and vertex-animated waves.

```glsl
// Depth-based color (Beer's Law)
float scene_depth = texture(DEPTH_TEXTURE, SCREEN_UV).x * 2.0 - 1.0;
scene_depth = PROJECTION_MATRIX[3][2] / (scene_depth + PROJECTION_MATRIX[2][2]) + VERTEX.z;
float depth_fade = exp(-scene_depth * beer_factor);
ALBEDO = mix(shallow_color, deep_color, clamp(depth_fade, 0.0, 1.0));

// Screen-space refraction
vec2 distortion = texture(normal_map, UV + TIME * 0.02).xy * 0.02;
vec3 refracted = texture(SCREEN_TEXTURE, SCREEN_UV + distortion).rgb;
```

**Key repositories:**
- [godotshaders.com stylized water](https://godotshaders.com/shader/stylized-water-for-godot-4-x/)
- [marcelb/GodotSSRWater](https://github.com/marcelb/GodotSSRWater) – custom SSR for transparent water
- [taillight-games/godot-4-pixelated-water-shader](https://github.com/taillight-games/godot-4-pixelated-water-shader) – pixel art compatible with buoyancy

---

## Essential open-source project repositories

| Repository | Techniques | Godot Version |
|------------|-----------|---------------|
| [MenacingMecha/godot-psx-style-demo](https://github.com/MenacingMecha/godot-psx-style-demo) | Vertex wobble, affine textures, hardware dithering, fog | 4.x |
| [bukkbeek/GodotPixelRenderer](https://github.com/bukkbeek/GodotPixelRenderer) | Color quantization, Sobel outlines, Bayer dithering, animation export | 4.4+ |
| [David Holland's project](https://git.sr.ht/~denovodavid/3d-pixel-art-in-godot) | Pixel-perfect camera, outlines, toon lighting, volumetrics, water | 4.3 custom |
| [leopeltola/Godot-3d-pixelart-demo](https://github.com/leopeltola/Godot-3d-pixelart-demo) | Depth-modulated outlines, normal highlights | 4.x |
| [voithos/godot-smooth-pixel-camera-demo](https://github.com/voithos/godot-smooth-pixel-camera-demo) | Smooth camera with pixel snap, physics interpolation | 4.1+ |

---

## Post-processing stack order matters for correct results

The **recommended effect order** is: Outlines → Color Quantization → Dithering → Bloom. This ensures outlines render against clean depth/normal buffers before colors change.

### CompositorEffect system (Godot 4.3+)

The new experimental `CompositorEffect` system provides the most control. Add a `Compositor` resource to `WorldEnvironment` or `Camera3D`, then populate its `compositor_effects` array with custom effects.

```gdscript
# Custom CompositorEffect
extends CompositorEffect
func _render_callback(effect_callback_type, render_data):
    var render_scene_buffers = render_data.get_render_scene_buffers()
    var color = render_scene_buffers.get_color_layer(0)
    # Apply shader via compute or raster pass
```

**Effect timing options:**
- `EFFECT_CALLBACK_TYPE_POST_OPAQUE` – after solid geometry
- `EFFECT_CALLBACK_TYPE_POST_TRANSPARENT` – after all rendering

**Key learning resources:**
- [PPMagic multi-pass example](https://github.com/peterprickarz/PPMagic)
- [pink-arcana/godot-distance-field-outlines](https://github.com/pink-arcana/godot-distance-field-outlines) – extensive compute shader notes
- [Official Compositor documentation](https://docs.godotengine.org/en/stable/tutorials/rendering/compositor.html)

### Simpler CanvasLayer method

For simpler setups, stack `CanvasLayer` nodes with increasing `layer` values, each containing a full-rect `ColorRect` with a shader:
```
CanvasLayer (layer=1) → Outline shader
CanvasLayer (layer=2) → Quantization shader  
CanvasLayer (layer=3) → Dithering shader
CanvasLayer (layer=4) → Bloom shader
```

---

## Conclusion: A unified rendering architecture

Building a complete 3D-to-pixel-art pipeline requires integrating these components into a coherent architecture. Start with **SubViewport low-resolution rendering** and **camera pixel snapping** as your foundation. Layer **outline detection** (using the reverse-Z vertex fix), **toon shading**, and **color quantization with dithering** as post-process effects. Add **bloom** and **volumetric lighting** for atmospheric depth. Use the **CompositorEffect system** for precise control over effect ordering, or the simpler CanvasLayer approach for prototyping.

The repositories from **MenacingMecha**, **bukkbeek**, and **David Holland** provide complete working examples that demonstrate these techniques in production-quality code. David Holland's article in particular offers the most comprehensive technical breakdown of t3ssel8r-inspired rendering, including solutions for grass lighting (`LIGHT_VERTEX`), water reflections, and volumetric god rays that are not documented elsewhere.
