# eliminating swimming pixels in godot 3d pixel art shaders

**Swimming pixels occur when subpixel camera or geometry movement causes rasterization decisions to shift frame-to-frame**, creating temporal artifacts where textures appear to "crawl" and edges flicker unpredictably. The definitive solution combines three techniques: vertex snapping in clip space to align geometry to the pixel grid, camera position snapping to a texel-sized grid, and post-process subpixel correction to restore smooth motion. In Godot, this requires matching your shader's snap resolution exactly to your SubViewport resolution—a mismatch is the most common cause of persistent swimming.

The PlayStation 1's characteristic "wobbly" rendering resulted from hardware limitations that modern retro shaders deliberately recreate or prevent. Understanding why the PS1 rendered this way illuminates the mathematical foundations of pixel-stable 3D rendering.

## Why swimming pixels occur in low-resolution 3D rendering

When rendering 3D scenes at low resolutions like **320×240**, objects and cameras frequently move by amounts smaller than a single pixel. The rasterizer must decide which pixel receives which color, and these decisions change unpredictably as positions shift between pixel boundaries. David Holland's research describes the phenomenon: "When moving a camera through a 3D scene at low resolution, it does not look like a 2D image being scrolled across the display. There are many temporal artefacts; swimming and creeping and jittering of pixels on screen."

Three distinct mechanisms contribute to swimming. **Subpixel texture swimming** occurs when UV interpolation values change based on which pixels the rasterizer selects, causing texture details to appear and disappear between frames. **Geometry jitter** happens when vertex positions fall between pixel boundaries—the rasterizer rounds them to integers, causing vertices to "jump" rather than move smoothly. **Edge instability** emerges when polygon outlines flicker as the underlying geometry snaps to different grid positions.

The original PlayStation 1 exhibited extreme versions of these artifacts due to hardware constraints. The PS1 used **16-bit fixed-point precision** for vertex calculations and had **no subpixel rasterization**—vertices snapped directly to integer pixel coordinates. Additionally, it lacked a Z-buffer (using ordering tables instead) and performed **affine texture mapping** without perspective correction. The GTE (Geometry Transformation Engine) coprocessor output 2D screen coordinates with depth information discarded, meaning the GPU rasterized primitives with no knowledge of 3D space.

## Vertex snapping mathematics across coordinate spaces

The fundamental snapping operation converts continuous positions to discrete grid positions:

```
snapped_value = floor(value × grid_resolution) / grid_resolution
```

However, *where* you apply this operation—world space, object space, or clip space—dramatically affects visual results and determines whether snapping actually prevents swimming.

**Clip space snapping** most accurately recreates PS1-style rendering because it snaps vertices to actual screen pixels after projection. The complete calculation transforms vertices, performs perspective divide, snaps to the grid, then reverses the perspective divide:

```glsl
vec4 snap_to_position(vec4 clip_pos, ivec2 resolution) {
    vec2 grid = vec2(resolution) * 0.5;  // Half resolution because NDC is -1 to 1
    vec4 snapped = clip_pos;
    snapped.xyz = clip_pos.xyz / clip_pos.w;        // Perspective divide to NDC
    snapped.xy = floor(grid * snapped.xy) / grid;   // Snap to pixel grid
    snapped.xyz *= clip_pos.w;                       // Restore clip space
    return snapped;
}
```

The **grid calculation uses half the resolution** because normalized device coordinates range from -1 to 1, making the total span 2 units. A 320×240 resolution therefore needs a grid of 160×120 to produce single-pixel snapping.

**World space snapping** offers an alternative where objects maintain consistent vertex positions regardless of camera movement. This approach transforms vertices to world coordinates, snaps them, then transforms back:

```glsl
void vertex() {
    vec4 world_pos = MODEL_MATRIX * vec4(VERTEX, 1.0);
    world_pos.xyz = round(world_pos.xyz * snap_resolution) / snap_resolution;
    vec4 local_pos = inverse(MODEL_MATRIX) * world_pos;
    VERTEX = local_pos.xyz;
}
```

World space snapping is independent of screen resolution, which can cause issues when the camera zooms—objects may jitter more visibly at different distances. Clip space snapping automatically adapts to the projection and maintains consistent screen-pixel alignment.

## Complete Godot shader implementation

The following shader demonstrates PSX-style vertex snapping with optional affine texture mapping for Godot 4:

```glsl
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_prepass_alpha, 
            shadows_disabled, specular_disabled, vertex_lighting;

uniform bool affine_mapping = false;
uniform sampler2D albedo : source_color, filter_nearest;
uniform float alpha_scissor : hint_range(0, 1) = 0.5;
uniform float jitter : hint_range(0, 1) = 0.25;
uniform ivec2 resolution = ivec2(320, 240);

vec4 snap_to_position(vec4 base_position) {
    vec4 snapped_position = base_position;
    snapped_position.xyz = base_position.xyz / base_position.w;
    
    vec2 snap_resolution = floor(vec2(resolution) * (1.0 - jitter));
    snapped_position.x = floor(snap_resolution.x * snapped_position.x) / snap_resolution.x;
    snapped_position.y = floor(snap_resolution.y * snapped_position.y) / snap_resolution.y;
    
    snapped_position.xyz *= base_position.w;
    return snapped_position;
}

void vertex() {
    vec4 clip_pos = PROJECTION_MATRIX * MODELVIEW_MATRIX * vec4(VERTEX, 1.0);
    vec4 snapped = snap_to_position(clip_pos);
    
    if (affine_mapping) {
        POSITION = snapped;
        POSITION /= abs(POSITION.w);  // Removes perspective correction
    } else {
        POSITION = snapped;
    }
}

void fragment() {
    vec4 tex_color = texture(albedo, UV);
    ALBEDO = (COLOR * tex_color).rgb;
    ALPHA = tex_color.a * COLOR.a;
    ALPHA_SCISSOR_THRESHOLD = alpha_scissor;
}
```

The `jitter` parameter controls snapping intensity—**0 produces no snapping while 1 produces maximum jitter**. The `resolution` uniform must match your SubViewport dimensions exactly.

## Camera snapping with subpixel correction

Vertex snapping alone doesn't eliminate swimming if the camera moves by subpixel amounts—the entire scene shifts between pixel boundaries. The solution involves **snapping the camera to a texel-sized grid, then compensating in post-processing**.

The technique, adapted from aarthificial's 2D pixel-perfect camera work by David Holland for 3D, follows two steps. First, snap the camera position to a view-aligned grid sized to your render resolution's texel dimensions:

```gdscript
var render_resolution := Vector2(320, 180)
var texel_size := 1.0 / render_resolution

func _physics_process(delta):
    var target_pos = follow_target.global_position
    
    # Snap to texel grid
    var snapped_pos = Vector3(
        floor(target_pos.x / texel_size.x) * texel_size.x,
        floor(target_pos.y / texel_size.y) * texel_size.y,
        target_pos.z
    )
    
    # Store error for post-process correction
    snap_error = target_pos - snapped_pos
    global_position = snapped_pos
```

Second, shift the final rendered output in screen space by the snap error to restore smooth motion:

```glsl
shader_type canvas_item;

uniform vec2 snap_offset;
uniform vec2 resolution;

void fragment() {
    vec2 corrected_uv = UV + (snap_offset / resolution);
    COLOR = texture(TEXTURE, corrected_uv);
}
```

This creates **visually smooth camera movement while maintaining perfect pixel stability**. Without the correction step, camera motion appears choppy as it jumps between texel positions.

## SubViewport configuration and resolution matching

Godot's SubViewport system enables low-resolution rendering, but several settings must align correctly. The critical relationship: **your shader's vertex snap resolution must exactly match your SubViewport resolution**. A mismatch is the single most common cause of persistent swimming pixels.

Configure your scene hierarchy as follows:
```
SubViewportContainer
  └── SubViewport (Size: 320×180)
      └── Camera3D
      └── [3D Scene nodes]
```

Essential SubViewport settings include enabling **Snap 2D Transforms to Pixel** and **Snap 2D Vertices to Pixel** in the inspector. Set the SubViewportContainer's `stretch` property to true with `stretch_shrink` calculated as your output resolution divided by your render resolution (for example, 4 for 1280×720 output from 320×180 render).

Project-wide settings require attention: set **Rendering → Textures → Default Texture Filter** to Nearest, configure **Display → Window → Stretch Mode** to viewport for authentic retro rendering, and on Godot 4.3+ enable **Display → Window → Scale Mode** as integer for proper integer scaling.

Known Godot 4 issues affect this pipeline. Issue #66764 causes 3D SubViewports to appear blurry because 3D scaling lacks nearest-neighbor options—the workaround applies the SubViewport texture to a Sprite2D with nearest filtering explicitly set. Issue #86837 tracks ongoing pixel-perfect improvements.

## Depth buffer precision at low resolutions

Low-resolution rendering exacerbates Z-buffer precision problems because **depth precision concentrates near the near plane** following a hyperbolic distribution. With a 16-bit depth buffer and near/far planes at 0.1/100 units, approximately 90% of precision exists in the first 10% of the depth range.

For retro rendering, several strategies mitigate Z-fighting. Push your **near plane as far from the camera as possible**—changing from 0.1 to 1.0 dramatically improves precision distribution. Consider using **reversed depth buffers** where available, which approximate logarithmic depth distribution. The PS1 avoided this entirely by using ordering tables (software depth sorting via painter's algorithm) rather than hardware Z-buffering.

When Z-fighting persists, the URSC (Ultimate Retro Shader Collection) implements distance-based fog that naturally hides far-plane artifacts while adding authentic retro atmosphere.

## Community implementations and shader collections

**MenacingMecha's godot-psx-style-demo** (685 GitHub stars) provides the most established foundation for PSX-style Godot rendering. It implements vertex snapping via a `precision_multiplier` global shader uniform, affine texture mapping, limited color depth with hardware dithering, and metallic surface shaders. The repository includes a `psx_base.gdshaderinc` include file containing the core snapping implementation.

**Ultimate Retro Shader Collection (URSC)** offers the most comprehensive feature set, combining PlayStation, Saturn, and N64 techniques. It provides vertex snapping with configurable resolution, affine texture mapping, N64-style 3-point texture filtering, distance-based LOD and fog, and a complete material library. Integration uses global uniforms configured via a setup script, with macro-based customization for advanced users.

**David Holland's 3D pixel art rendering** techniques address swimming pixels directly through camera snapping with subpixel correction. His work extends to procedural outlines using depth/normal edge detection, volumetric god rays adapted from t3ssel8r, and custom grass rendering using the `LIGHT_VERTEX` variable introduced in Godot 4.3. Implementation files exist at `git.sr.ht/~denovodavid/3d-pixel-art-in-godot`.

**t3ssel8r's Unity techniques** transfer conceptually to Godot. Key methods include Robert's Cross edge detection for pixel-perfect outlines, custom texture sampling with derivatives for stable mip selection, and shadow map sampling for volumetric lighting via shell texturing. His Patreon documents pixel art anti-aliasing using pre-multiplied alpha blending.

## Technical resources and further reading

David Colson's "Building a PS1 Style Retro 3D Renderer" provides the clearest mathematical treatment of vertex snapping and affine texture mapping implementation. Pikuma's "How PlayStation Graphics & Visual Artefacts Work" offers comprehensive coverage of PS1 hardware including the GTE coprocessor, fixed-point mathematics, and ordering table algorithms. Rodrigo Copetti's PlayStation Architecture analysis documents the complete rendering pipeline from a hardware perspective.

For depth buffer mathematics, the Khronos OpenGL Wiki's "Depth Buffer Precision" article and Zero Radiance's "Quantitative Analysis of Z-Buffer Precision" explain the hyperbolic distribution and reversed-Z improvements. Scratchapixel's rasterization tutorials cover subpixel precision allocation (typically 4 bits on modern GPUs) and fixed-point coordinate handling.

GDC presentations relevant to low-resolution aesthetics include Playdead's "Low Complexity, High Fidelity - INSIDE Rendering" and Capybara Games' "The Rendering of Below" covering techniques for maintaining visual clarity with small primitives.

## Conclusion

Eliminating swimming pixels in 3D pixel art requires understanding that **the artifact has multiple causes requiring multiple solutions**. Vertex snapping prevents per-polygon jitter but doesn't address camera-induced swimming. Camera snapping eliminates scene-wide pixel creep but creates choppy movement without subpixel correction. Only the combination of clip-space vertex snapping, camera position snapping, and post-process offset compensation produces truly stable pixels with smooth motion.

The most overlooked detail in Godot implementations is **resolution matching**—the shader's snap grid must equal the SubViewport render resolution. Most persistent swimming problems trace to this mismatch. Integer scaling at the display stage prevents additional artifacts from non-integer upscaling.

For implementation, start with MenacingMecha's demo as a foundation and incorporate camera snapping from David Holland's techniques. The URSC provides a production-ready material library if you need N64-style filtering or Saturn-style stippled transparency alongside PSX vertex snapping. All major approaches use the same underlying mathematics: floor-based quantization in normalized device coordinates, with the perspective divide performed before snapping and restored afterward.
