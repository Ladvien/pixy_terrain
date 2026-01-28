# Eliminating near-plane outline artifacts in 3D pixel art games

**The fastest fix**: Add a depth threshold check to your edge detection shader that skips pixels where `depth > 0.995` (reverse-Z). This single condition catches geometry clipped by the near plane and suppresses false outlines along that boundary. For more robust results, combine this with a camera constraint system that prevents the frustum from intersecting terrain entirely—the approach used by shipped RTS games like Age of Empires 4.

The root cause isn't depth buffer precision but **geometry intersection**: your camera's near plane is physically inside the mountain mesh, creating artificial depth discontinuities that Sobel detection interprets as edges. Reverse-Z provides minimal benefit for orthographic projection since depth is already linear, so the solution must address either the shader (mask the artifacts) or the camera (prevent the intersection).

## Quick shader fix: depth threshold masking

The simplest modification to your existing Sobel shader checks whether the current pixel or its neighbors sit at the near-plane depth value. In Godot 4.3+ reverse-Z, near plane = **1.0** and far plane = **0.0**, so you're looking for depth values very close to 1.0:

```glsl
uniform float near_plane_mask: hint_range(0.9, 1.0) = 0.995;

bool isNearPlane(vec2 uv) {
    return texture(DEPTH_TEXTURE, uv).r > near_plane_mask;
}

void fragment() {
    vec2 uv = SCREEN_UV;
    vec2 px = 1.0 / VIEWPORT_SIZE;
    
    // Skip outline for any pixel touching the near-plane clip boundary
    if (isNearPlane(uv) ||
        isNearPlane(uv + vec2(-px.x, 0.0)) ||
        isNearPlane(uv + vec2(px.x, 0.0)) ||
        isNearPlane(uv + vec2(0.0, -px.y)) ||
        isNearPlane(uv + vec2(0.0, px.y))) {
        ALBEDO = texture(SCREEN_TEXTURE, uv).rgb;
        return;  // No outline drawn here
    }
    // ... continue with normal Sobel detection
}
```

The threshold of **0.995** works well as a starting point—lower values (0.98) catch more artifacts but risk masking legitimate edges, while higher values (0.999) may miss some clipping. For orthographic cameras, depth maps linearly across the frustum, so this epsilon remains consistent regardless of distance.

## Soft falloff creates cleaner transitions

Rather than hard-masking clipped pixels, a **smoothstep falloff** produces less jarring results when geometry approaches the near plane. This gradually fades outline intensity as depth nears the clip boundary:

```glsl
uniform float falloff_start = 0.97;
uniform float falloff_end = 1.0;

float near_falloff = 1.0 - smoothstep(falloff_start, falloff_end, depth_raw);
edge_strength *= near_falloff;
```

This approach preserves partial outlines on geometry that's *near* but not *at* the clip plane, avoiding the visual pop of outlines suddenly appearing or disappearing as the camera moves.

## Combining depth and normal detection improves robustness

Near-plane clipping typically creates depth discontinuities **without** corresponding normal discontinuities, since the clipped surface doesn't actually have a geometric edge there. By requiring both depth and normal edges to agree for suspicious regions, you can filter out false positives:

```glsl
float depth_edge = computeDepthSobel(depth_samples);
float normal_edge = computeNormalSobel(NORMAL_TEXTURE, uvs);

// Near the near plane, require both signals to agree
if (center_depth > 0.95) {
    final_edge = depth_edge * normal_edge;  // Multiplicative—both must fire
} else {
    final_edge = max(depth_edge, normal_edge);  // Normal behavior elsewhere
}
```

The high-quality outline shader from EMBYR on Godot Shaders implements this pattern with additional **grazing angle modulation** that adjusts thresholds based on surface orientation relative to the view direction.

## Camera constraint systems prevent the problem entirely

Shipped RTS games solve this architecturally rather than with shader tricks. **Age of Empires 4** uses a "Camera Mesh" system—a separate, simplified geometry layer that the camera rides on, hanging above the actual terrain. This mesh is auto-generated from terrain data with smoothing applied, ensuring the camera frustum never intersects playable geometry.

For Godot implementation, the equivalent approach:
- Cast rays downward from the camera's logical view center
- Query terrain height at that position
- Constrain `camera.position.y` so the near plane (in world space) clears the terrain plus a buffer

```gdscript
var terrain_height = raycast_terrain(view_center)
var near_plane_world_y = camera.global_position.y - camera.near
var min_camera_y = terrain_height + near_plane_distance + buffer
camera.global_position.y = max(camera.global_position.y, min_camera_y)
```

For smooth movement, apply this constraint through `lerp()` or `move_toward()` rather than snapping instantly.

## Why reverse-Z doesn't help orthographic cameras

Reverse-Z improves depth precision by counteracting the **non-linear 1/z distortion** inherent in perspective projection, where most precision concentrates near the near plane. But orthographic projection already distributes depth **linearly**—there's no 1/z term to counteract. The Ogre3D rendering team confirmed this: "Reverse Z here either does nothing, barely improves, or barely degrades. This is because it uses an orthographic projection, so the distribution of depth is linear."

Your near plane at **0.1 units** isn't causing precision problems. The visual artifacts stem from the geometric fact that rendered geometry ends abruptly at an arbitrary depth value, creating exactly the kind of discontinuity edge detection is designed to find.

## Industry approaches favor design constraints over runtime fixes

Researching how shipped indie games handle this reveals a striking pattern: **most avoid the problem through design rather than solving it technically**.

| Game | Approach |
|------|----------|
| **Hyper Light Drifter** | Actually 2D—isometric look comes from art direction, not 3D rendering |
| **Dead Cells** | Pre-renders 3D to 2D sprite sheets; no runtime 3D camera issues |
| **A Short Hike** | Low-res rendering (160x140) naturally masks artifacts; no edge detection shader |
| **TUNIC** | Fixed orthographic camera with careful level design preventing intersection |

Dead Cells' developers explicitly noted they **haven't fully solved** pixel flickering artifacts in their 3D-to-2D pipeline. Even TUNIC's developer discussed experimenting with geometry cutaway systems for when the camera might clip through trees.

The most applicable technique for your case is **near-camera dithering fade**, where geometry approaching the camera dissolves using a screen-space dither pattern rather than alpha blending (better performance, fits pixel art aesthetic):

```glsl
half DITHER_THRESHOLDS[16] = { 1.0/17.0, 9.0/17.0, 3.0/17.0, ... };
uint index = (uint(screen_uv.x * viewport_width) % 4) * 4 + uint(screen_uv.y * viewport_height) % 4;
clip(fade_alpha - DITHER_THRESHOLDS[index]);
```

## Complete Godot 4.3+ shader with near-plane fix

Here's a production-ready Sobel edge detection shader incorporating the near-plane masking:

```glsl
shader_type spatial;
render_mode unshaded, depth_draw_never, depth_test_disabled;

uniform sampler2D SCREEN_TEXTURE: hint_screen_texture, filter_nearest;
uniform sampler2D DEPTH_TEXTURE: hint_depth_texture, filter_nearest;

uniform float edge_threshold: hint_range(0.0, 1.0) = 0.1;
uniform float near_plane_mask: hint_range(0.9, 1.0) = 0.995;
uniform vec4 outline_color: source_color = vec4(0.0, 0.0, 0.0, 1.0);

const mat3 sobel_x = mat3(
    vec3(1.0, 2.0, 1.0), vec3(0.0, 0.0, 0.0), vec3(-1.0, -2.0, -1.0));
const mat3 sobel_y = mat3(
    vec3(1.0, 0.0, -1.0), vec3(2.0, 0.0, -2.0), vec3(1.0, 0.0, -1.0));

float getLinearDepth(vec2 uv, mat4 inv_proj) {
    float depth = texture(DEPTH_TEXTURE, uv).r;
    vec3 ndc = vec3(uv * 2.0 - 1.0, depth);
    vec4 view = inv_proj * vec4(ndc, 1.0);
    view.xyz /= view.w;
    return -view.z;
}

void vertex() {
    POSITION = vec4(VERTEX.xy, 1.0, 1.0);  // Required for Godot 4.3+ reverse-Z
}

void fragment() {
    vec2 uv = SCREEN_UV;
    vec2 px = 1.0 / VIEWPORT_SIZE;
    
    // Near-plane artifact suppression
    float center_raw = texture(DEPTH_TEXTURE, uv).r;
    bool near_clipped = center_raw > near_plane_mask ||
        texture(DEPTH_TEXTURE, uv + vec2(-px.x, 0.0)).r > near_plane_mask ||
        texture(DEPTH_TEXTURE, uv + vec2(px.x, 0.0)).r > near_plane_mask ||
        texture(DEPTH_TEXTURE, uv + vec2(0.0, -px.y)).r > near_plane_mask ||
        texture(DEPTH_TEXTURE, uv + vec2(0.0, px.y)).r > near_plane_mask;
    
    if (near_clipped) {
        ALBEDO = texture(SCREEN_TEXTURE, uv).rgb;
        return;
    }
    
    // Standard Sobel edge detection on linearized depth
    float d[9];
    d[0] = getLinearDepth(uv + vec2(-px.x, -px.y), INV_PROJECTION_MATRIX);
    d[1] = getLinearDepth(uv + vec2(0.0, -px.y), INV_PROJECTION_MATRIX);
    d[2] = getLinearDepth(uv + vec2(px.x, -px.y), INV_PROJECTION_MATRIX);
    d[3] = getLinearDepth(uv + vec2(-px.x, 0.0), INV_PROJECTION_MATRIX);
    d[4] = getLinearDepth(uv, INV_PROJECTION_MATRIX);
    d[5] = getLinearDepth(uv + vec2(px.x, 0.0), INV_PROJECTION_MATRIX);
    d[6] = getLinearDepth(uv + vec2(-px.x, px.y), INV_PROJECTION_MATRIX);
    d[7] = getLinearDepth(uv + vec2(0.0, px.y), INV_PROJECTION_MATRIX);
    d[8] = getLinearDepth(uv + vec2(px.x, px.y), INV_PROJECTION_MATRIX);
    
    mat3 depths = mat3(vec3(d[0],d[1],d[2]), vec3(d[3],d[4],d[5]), vec3(d[6],d[7],d[8]));
    float gx = dot(sobel_x[0], depths[0]) + dot(sobel_x[1], depths[1]) + dot(sobel_x[2], depths[2]);
    float gy = dot(sobel_y[0], depths[0]) + dot(sobel_y[1], depths[1]) + dot(sobel_y[2], depths[2]);
    float edge = sqrt(gx*gx + gy*gy);
    
    ALBEDO = edge > edge_threshold ? outline_color.rgb : texture(SCREEN_TEXTURE, uv).rgb;
}
```

## Recommended implementation strategy

For the **best player experience** balancing visual quality with implementation effort:

1. **Immediate fix**: Add the `near_plane_mask` check to your existing shader (5 minutes of work, eliminates the jagged outlines)

2. **Short-term improvement**: Implement camera height constraints using terrain raycasts—prevents the intersection from occurring rather than just masking artifacts

3. **Polish pass**: Add soft falloff using smoothstep for geometry approaching the clip boundary, and consider dithered fade for objects that do clip

4. **Optional refinement**: Combine depth + normal edge detection for more robust outlines that naturally resist false positives from clipping

The shader fix alone will likely solve your immediate problem. The camera constraint system adds robustness and prevents edge cases where geometry might clip without triggering the depth threshold check (such as very thin geometry or specific viewing angles).