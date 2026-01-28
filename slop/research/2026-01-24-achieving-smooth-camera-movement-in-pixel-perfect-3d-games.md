# Achieving smooth camera movement in pixel-perfect 3D games

The dominant technique for maintaining pixel stability while enabling smooth camera movement is **viewport padding with fractional offset compensation**: rendering to an oversized buffer (typically +1 pixel per dimension), snapping all game objects and the camera to integer pixel coordinates, then sliding the final output by the camera's sub-pixel remainder. This approach ships in games like Celeste and Enter the Gungeon, and is documented extensively by developers including Pedro Medeiros, David Holland, and YellowAfterlife. The technique solves a fundamental tension—pixel art requires grid alignment for visual integrity, but choppy integer-only camera movement feels jarring to players.

## The core technical mechanism

The buffer padding technique works by rendering more pixels than you display, then cropping. A game targeting **320×180** resolution renders instead to **321×181**—one extra pixel in each scrolling dimension. The camera's true position (e.g., `142.73, 89.41`) is decomposed into an integer component (`142, 89`) used for rendering and a fractional remainder (`0.73, 0.41`) used as display offset.

The pipeline follows three steps. First, render the entire scene to the padded framebuffer with all sprites snapped to whole-pixel positions relative to the floored camera coordinates. Second, calculate the fractional camera offset. Third, draw only the target resolution portion of the buffer, offset by that fraction—effectively sliding a window across the slightly larger rendered content.

```
// Pseudocode from YellowAfterlife's GameMaker implementation
camera_set_view_pos(floor(camera_x), floor(camera_y));
draw_surface_part(view_surf, frac(camera_x), frac(camera_y), 
                  game_width, game_height, 0, 0);
```

This achieves sub-pixel smoothness while every sprite remains locked to the pixel grid. The extra pixel provides "headroom" for the offset—when you shift by 0.7 pixels, you're reading into that padding rather than beyond the buffer edge.

## How shipped games handle this tradeoff

**Celeste** (320×180, 6× scaling to 1080p) uses this exact approach. Pedro Medeiros, the game's pixel artist, explained their implementation: the game canvas includes a "bleed area" of extra pixels, sprites render snapped to the pixel grid, and when drawing to screen, the camera applies an offset calculated as `gameScale * ((float2)camera.position - (int2)camera.position - BLEED_SIZE / 2)`. For parallax layers that move slowly, Celeste draws sprites to separate canvases and slides entire layers at screen resolution—creating temporary "thin pixels" during movement that stabilize when motion stops.

**Enter the Gungeon** (270p, 4× macro pixels) made an explicit design decision documented by the developers: "We chose to allow the camera to move in screen pixels instead of macro pixels because it makes camera motion feel much smoother and generally improves game feel." They faced a critical choice about player tracking—tracking the player's screen-pixel position creates smooth world movement but causes the player sprite to jitter; tracking at macro-pixel resolution keeps the player stable but makes the world jump. Dodge Roll chose player stability: "the majority of our team feels that keeping the character from jittering looks and feels better."

**Dead Cells** acknowledges they never fully solved the problem. Artist Thomas Vasseur stated: "we still haven't found any solution for flickering pixels. We could clean that by hand, but the point of this workflow is to go fast." Motion Twin explicitly prioritized animation fluidity over pixel consistency—"movement is love, movement is life."

**Octopath Traveler's HD-2D** approach handles the 2D/3D integration differently because sprites exist as billboards in true 3D space with dynamic lighting. Acquire's developers noted struggling with how far to rotate the camera "without compromising graphical fidelity"—they pushed to 90-degree rotations in battle effects. The game accepts some anti-aliasing blur on sprites at non-integer viewing angles as the cost of full 3D camera freedom.

## Sub-pixel camera offset with shader implementation

For games using integer upscaling (4×, 6×), the fractional offset can be further subdivided. Daniel Ludwig's shader-based approach from his libGDX demo uses the formula:

```
game_camera.xy = render_camera.xy + (displacement.xy + subsampling.xy) / upscale
```

Here, `displacement` represents **discrete sub-pixel steps** (values 0 to upscale−1), and `subsampling` provides optional bilinear interpolation between those steps (values 0 to 1). At 4× upscale, you effectively have 4 discrete sub-pixel positions per game pixel, enabling smoother scrolling than the base resolution allows.

The vertex shader applies the displacement as a UV offset:
```glsl
v_texCoord = a_texCoord - (u_displacement * u_texelSize);
```

Edge artifacts from sampling beyond buffer content are hidden using viewport/scissor adjustments—offsetting the viewport by `upscale/2` pixels and shrinking the scissor rect by `upscale` total.

## Adapting the technique for 3D pixel art

David Holland's 2024 article on 3D pixel art rendering (implemented in a custom Godot 4.3 build) addresses the specific challenges of 3D: "When moving a camera through a 3D scene at low resolution, it does not look like a 2D image being scrolled. There are many temporal artifacts; swimming and creeping and jittering of pixels."

His two-step solution:

1. **Snap the camera to a view-aligned, texel-sized grid.** This eliminates pixel creep—where pixels appear to shift relative to each other as the camera moves—but makes movement noticeably choppy.
2. **Shift the render output back in screen space by the snap error.** The difference between the true camera position and the snapped position becomes the display offset, restoring smooth perceived movement.

The critical insight is that in 3D, the grid must be **view-aligned** (perpendicular to the camera direction) rather than world-aligned, and sized to match the rendered texel size at the camera's distance.

## Key technical resources and GDC talks

**Itay Keren's "Scroll Back: The Theory and Practice of Cameras in Side-Scrollers"** (GDC 2015) remains the definitive reference on 2D camera design, covering pixel-perfect scrolling, lerp-smoothing, physics-smoothing, and platform-snapping. The talk specifically discusses how Hyper Light Drifter achieves smooth scrolling with low-res pixel art by pre-rendering to a game-pixel-perfect canvas then shifting by screen pixels.

**aarthificial's YouTube video** on 2D pixel-perfect cameras provides a clear visual explanation of the padding technique that David Holland's 3D adaptation builds upon. **YellowAfterlife's 2020 GameMaker tutorial** (yal.cc/gamemaker-smooth-pixel-perfect-camera) offers the most practical implementation guide with full source code on GitHub.

For HD-2D specifically, **"The Fusion of Nostalgia and Novelty in the Development of Octopath Traveler"** (Unreal Fest Europe 2019) covers Acquire's approach to combining 2D sprites with 3D environments, including their dynamic lighting and depth-of-field techniques.

## Godot implementation with SubViewport

The recommended Godot 4.3+ approach uses this node hierarchy:

```
Root
├── Sprite2D (displays ViewportTexture, scaled 6×)
└── SubViewport (322×182 = base + 2px padding)
    ├── Game World
    └── Camera2D (separate from player)
```

The camera controller tracks a `virtual_position` with full float precision, then snaps `global_position` to integers while storing the `snap_delta`:

```gdscript
func _process(delta):
    virtual_position = virtual_position.lerp(target.global_position, smoothing)
    var snapped = virtual_position.floor()
    snap_delta = virtual_position - snapped
    global_position = snapped
    # Sprite2D uses snap_delta to offset ViewportTexture
```

**Key project settings**: Default Texture Filter → Nearest, and on the SubViewport enable "Snap 2D Vertices to Pixel." Avoid project-level snap settings which can introduce jitter bugs. Since Godot 4.3, built-in physics interpolation helps with object movement smoothness—enable it and set `physics_jitter_fix = 0`.

The community addon **godot-smooth-pixel-subviewport-container** packages this approach with optional shader-based anti-aliasing. The **Phantom Camera** addon (v0.6.1+) adds a pixel-snap toggle for its Camera2D system. GitHub proposal #6389 (277+ upvotes) documents that this remains an active pain point with no perfect built-in solution.

## Tradeoffs and edge cases

**Parallax layers** break the single-buffer approach. Each parallax layer moving at a different rate needs its own padded framebuffer with proportionally adjusted displacement values, or you'll see pixel inconsistency where layers intersect. Celeste handles this with separate canvases per layer.

**Slow-moving objects** (less than 1 pixel per frame) will jitter regardless of camera smoothing—an object moving 0.3 pixels/frame will stay still for 3 frames, then jump 1 pixel. Solutions include ensuring minimum movement speeds, using specialized pixel-art filtering shaders, or accepting the artifact as period-authentic.

**Non-integer display scaling** remains problematic. A 320×180 game displays perfectly at 1920×1080 (6×) but poorly at 2560×1440 (8×—but with letterboxing needed). Games like CrossCode show smoothing/blurring at non-integer scales. Godot 4.2+ offers built-in integer scaling that adds black bars rather than blur.

**Editor workflow** suffers with SubViewports—scene content is only visible within the SubViewport bounds, making large levels difficult to edit. The workaround is editing SubViewport contents as separate scene files.

## Conclusion

The viewport padding technique is proven, shipping in multiple successful games with documented implementations. The core principle—render at integer coordinates, offset on display—is engine-agnostic and scales from 2D to 3D. For most projects, **1 pixel of padding per scrolling axis** suffices; shader-based approaches with larger padding enable finer sub-pixel steps but add complexity. The key design decisions are whether to prioritize player stability (Enter the Gungeon) or world stability (some games allow slight player jitter), and how to handle parallax layers. For 3D pixel art, view-aligned camera snapping with screen-space offset compensation extends the same principle. Godot users should expect to implement this manually using SubViewports until a streamlined built-in solution emerges.
