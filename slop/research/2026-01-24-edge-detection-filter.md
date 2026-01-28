# Depth-based edge detection in Godot 4.3+: Complete implementation guide

**The solution requires both correct uniform syntax AND a specific vertex shader trick introduced in Godot 4.3.** Your shader compilation errors likely stem from the removed `DEPTH_TEXTURE` built-in, but even after fixing the uniforms, the effect won't work without the **Reverse Z vertex positioning** change. Here's everything you need.

## The critical Godot 4.3+ change most tutorials miss

Godot 4.3 introduced **Reverse Z** depth buffering, which moved the near plane from `z=0` to `z=1`. This breaks all post-process shaders using the old vertex code. The fix is simple but essential:

```glsl
// OLD (Godot 4.2 and earlier) - WILL NOT WORK IN 4.3+
void vertex() {
    POSITION = vec4(VERTEX, 1.0);
}

// NEW (Godot 4.3+) - REQUIRED
void vertex() {
    POSITION = vec4(VERTEX.xy, 1.0, 1.0);
}
```

Without this change, your quad renders at the wrong depth and the effect only appears on the skybox background, not your 3D objects.

## Complete node setup for full-screen post-processing

The **MeshInstance3D with QuadMesh** approach is the only method that provides depth buffer access. ColorRect and SubViewport approaches cannot read the depth texture.

**Node hierarchy:**
```
Camera3D
└── MeshInstance3D ("PostProcessor")
    └── QuadMesh + ShaderMaterial
```

**Step-by-step configuration:**

1. Create a **MeshInstance3D** as a child of your Camera3D
2. In the Mesh property, create a new **QuadMesh**
3. Configure the QuadMesh:
   - **Size**: Width = 2, Height = 2 (exactly)
   - **Flip Faces**: ✓ Enabled (critical!)
4. Create a new **ShaderMaterial** and assign your shader
5. If not parenting to camera, set **Geometry → Extra Cull Margin** to `16384` to prevent frustum culling

## Complete working shader for Godot 4.5+

This shader uses the Sobel operator for robust edge detection and includes all required Godot 4.x syntax:

```glsl
shader_type spatial;
render_mode unshaded, depth_draw_never, depth_test_disabled, fog_disabled;

// New Godot 4.x uniform syntax (replaces DEPTH_TEXTURE and SCREEN_TEXTURE)
uniform sampler2D screen_texture : hint_screen_texture, filter_linear, repeat_disable;
uniform sampler2D depth_texture : hint_depth_texture, filter_linear, repeat_disable;

// Configurable parameters
uniform float edge_threshold : hint_range(0.0, 1.0) = 0.05;
uniform vec3 line_color : source_color = vec3(0.0, 0.0, 0.0);
uniform float line_opacity : hint_range(0.0, 1.0) = 1.0;

// Sobel kernels for gradient detection
const mat3 sobel_x = mat3(
    vec3(1.0, 2.0, 1.0),
    vec3(0.0, 0.0, 0.0),
    vec3(-1.0, -2.0, -1.0)
);

const mat3 sobel_y = mat3(
    vec3(1.0, 0.0, -1.0),
    vec3(2.0, 0.0, -2.0),
    vec3(1.0, 0.0, -1.0)
);

float linearize_depth(vec2 uv, mat4 inv_proj) {
    float depth = texture(depth_texture, uv).r;
    vec3 ndc = vec3(uv * 2.0 - 1.0, depth);
    vec4 view = inv_proj * vec4(ndc, 1.0);
    view.xyz /= view.w;
    return -view.z;
}

void vertex() {
    // CRITICAL: Godot 4.3+ Reverse Z requires this exact line
    POSITION = vec4(VERTEX.xy, 1.0, 1.0);
}

void fragment() {
    vec3 screen_color = texture(screen_texture, SCREEN_UV).rgb;
    vec2 pixel_size = 1.0 / VIEWPORT_SIZE;
    
    // Sample depth in 3x3 neighborhood
    float depth_c = linearize_depth(SCREEN_UV, INV_PROJECTION_MATRIX);
    float depth_n = linearize_depth(SCREEN_UV + vec2(0.0, -pixel_size.y), INV_PROJECTION_MATRIX);
    float depth_s = linearize_depth(SCREEN_UV + vec2(0.0, pixel_size.y), INV_PROJECTION_MATRIX);
    float depth_e = linearize_depth(SCREEN_UV + vec2(pixel_size.x, 0.0), INV_PROJECTION_MATRIX);
    float depth_w = linearize_depth(SCREEN_UV + vec2(-pixel_size.x, 0.0), INV_PROJECTION_MATRIX);
    float depth_nw = linearize_depth(SCREEN_UV + vec2(-pixel_size.x, -pixel_size.y), INV_PROJECTION_MATRIX);
    float depth_ne = linearize_depth(SCREEN_UV + vec2(pixel_size.x, -pixel_size.y), INV_PROJECTION_MATRIX);
    float depth_sw = linearize_depth(SCREEN_UV + vec2(-pixel_size.x, pixel_size.y), INV_PROJECTION_MATRIX);
    float depth_se = linearize_depth(SCREEN_UV + vec2(pixel_size.x, pixel_size.y), INV_PROJECTION_MATRIX);
    
    // Build matrix of surrounding depths
    mat3 depth_samples = mat3(
        vec3(depth_nw, depth_n, depth_ne),
        vec3(depth_w, depth_c, depth_e),
        vec3(depth_sw, depth_s, depth_se)
    );
    
    // Apply Sobel operators
    float edge_x = dot(sobel_x[0], depth_samples[0]) + dot(sobel_x[1], depth_samples[1]) + dot(sobel_x[2], depth_samples[2]);
    float edge_y = dot(sobel_y[0], depth_samples[0]) + dot(sobel_y[1], depth_samples[1]) + dot(sobel_y[2], depth_samples[2]);
    
    // Calculate gradient magnitude
    float edge = sqrt(edge_x * edge_x + edge_y * edge_y);
    float edge_mask = step(edge_threshold, edge);
    
    // Blend edge color with screen
    ALBEDO = mix(screen_color, line_color, edge_mask * line_opacity);
    ALPHA = 1.0;
}
```

## Understanding the render_mode flags

Each flag serves a specific purpose for post-processing:

| Flag | Purpose |
|------|---------|
| `unshaded` | Bypasses all lighting calculations—essential for post-process |
| `depth_draw_never` | Prevents the quad from writing to the depth buffer |
| `depth_test_disabled` | Renders the quad regardless of what's in front of it |
| `fog_disabled` | Prevents environment fog from tinting your effect |

The combination ensures your post-process quad always renders on top and doesn't interfere with the scene's depth buffer.

## The uniform hint system explained

Godot 4.0 replaced the built-in texture accessors with configurable uniforms. The variable names don't matter—only the **hints** do:

```glsl
// These are equivalent—the hint determines what texture is bound
uniform sampler2D depth_texture : hint_depth_texture;
uniform sampler2D DEPTH_TEXTURE : hint_depth_texture;
uniform sampler2D my_depth : hint_depth_texture;
```

**Available hints for post-processing:**

- `hint_screen_texture` — The rendered frame before transparent objects
- `hint_depth_texture` — The depth buffer (Forward+ and Mobile only)
- `hint_normal_roughness_texture` — Normal/roughness G-buffer (Forward+ only)

**Filter and repeat modifiers** can be appended:
```glsl
uniform sampler2D depth_texture : hint_depth_texture, filter_linear, repeat_disable;
```

## Renderer compatibility matters

| Renderer | Depth texture | Normal/roughness | Notes |
|----------|--------------|------------------|-------|
| Forward+ | ✅ Full support | ✅ Full support | Recommended for this effect |
| Mobile | ✅ Works | ❌ Not available | Depth returns 0 if MSAA enabled |
| Compatibility | ⚠️ Limited | ❌ Not available | Editor shows warning, may not work |

If targeting mobile, disable MSAA or the depth texture will return all zeros (known issue #103425).

## Why linearizing depth is necessary

The raw depth buffer stores values **non-linearly**—most precision is concentrated near the camera. Without linearization, edge detection produces inconsistent results at different distances. The linearization formula reconstructs view-space Z:

```glsl
float linearize_depth(vec2 uv, mat4 inv_proj) {
    float depth = texture(depth_texture, uv).r;
    vec3 ndc = vec3(uv * 2.0 - 1.0, depth);
    vec4 view = inv_proj * vec4(ndc, 1.0);
    view.xyz /= view.w;
    return -view.z;  // Negative because camera faces -Z
}
```

## Alternative: Adding normal-based detection for interior edges

Depth-only detection misses edges where objects meet at the same depth. Adding normal detection catches these:

```glsl
uniform sampler2D normal_roughness_texture : hint_normal_roughness_texture, filter_linear, repeat_disable;

// In fragment():
vec3 normal_c = texture(normal_roughness_texture, SCREEN_UV).xyz * 2.0 - 1.0;
vec3 normal_n = texture(normal_roughness_texture, SCREEN_UV + vec2(0.0, -pixel_size.y)).xyz * 2.0 - 1.0;
// ... sample other neighbors

float normal_diff = 1.0 - dot(normal_c, normal_n);
float normal_edge = step(normal_threshold, normal_diff);

// Combine both detection methods
float final_edge = max(depth_edge, normal_edge);
```

Note: `hint_normal_roughness_texture` only works with Forward+ renderer.

## Troubleshooting common issues

**Effect only shows on skybox, not objects**: You're using the old vertex shader code. Change to `POSITION = vec4(VERTEX.xy, 1.0, 1.0);`

**Quad disappears when camera rotates**: Either parent the MeshInstance3D to Camera3D, or set Extra Cull Margin to 16384.

**Shader compiles but screen is black**: Ensure QuadMesh has "Flip Faces" enabled and size is exactly 2×2.

**Edges flicker with TAA enabled**: Temporal Anti-Aliasing conflicts with edge detection. Use FXAA or MSAA instead.

**Depth appears wrong on mobile**: MSAA breaks depth texture on mobile renderer. Disable MSAA in project settings.

## Conclusion

The complete implementation requires three coordinated pieces: the **MeshInstance3D/QuadMesh scene setup** with proper configuration, the **new uniform hint syntax** replacing the removed built-ins, and critically, the **Reverse Z vertex positioning** introduced in Godot 4.3. Missing any one of these causes the effect to fail silently or produce incorrect results. The Sobel-based approach provided here works reliably across Forward+ scenes and can be extended with normal-based detection for more comprehensive edge coverage.
