# Feature: Ant Farm Cross-Section

**Date:** 2026-02-02
**Status:** Draft

## Problem

The game needs an "ant farm" aesthetic — a always-on cross-sectional view that follows the camera, revealing the interior of terrain with a solid textured surface (like looking through glass at a cut piece of earth).

The existing stencil-based cross-section system doesn't achieve this effect. It renders the terrain's back-face geometry instead of a flat cap surface, resulting in fog-like visuals rather than a solid "glass" cut.

## Goals

- Always-on cross-section view that follows the camera
- Flat, solid textured surface at the clip plane (the "glass")
- Interior texture shows the "inside" material of the terrain
- Integrates with existing transvoxel terrain system

## Non-Goals

- Mobile support (desktop only)
- User-togglable cross-section (it's always on)
- Multiple clip planes or complex CSG operations
- Fixing the existing stencil-only approach

## Proposed Solution

**Clip plane + separate cap mesh**

Two-part system:

1. **Terrain shader clips geometry** — Fragments beyond the clip plane are discarded. (Already implemented in `triplanar_pbr.gdshader`)

2. **Separate quad mesh at clip plane** — A flat plane positioned at the clip plane, perpendicular to camera view, textured with the "inside" material. Only renders where terrain actually exists.

### How It Works

**Three-pass render pipeline:**

| Pass | Material | render_priority | Purpose |
|------|----------|-----------------|---------|
| 1 | Terrain (front faces) | -1 | Render visible terrain, clip below plane |
| 2 | Terrain back-face stencil | 0 | Write stencil=1 where back-faces visible |
| 3 | Cap quad | 1 | Render cap only where stencil=1 |

1. **Terrain front faces** render normally with clip plane discard
2. **Terrain back faces** write stencil=1 (marks where interior is visible through the cut)
3. **Cap quad** at clip plane position reads stencil, renders solid texture only where stencil=1

### Technical Implementation

**Godot 4.5+ stencil_mode directive:**

Back-face stencil writer:
```glsl
shader_type spatial;
render_mode cull_front, depth_draw_never, unshaded;
stencil_mode write, compare_always, 1;
// Discard below clip plane, write stencil where visible
```

Cap quad stencil reader:
```glsl
shader_type spatial;
render_mode unshaded, depth_draw_never;
stencil_mode read, compare_equal, 1;
// ALPHA = 0.999 required to force alpha pass for stencil read
```

**Critical gotchas:**
- Stencil reference must be ≥1 (buffer initializes to 0 each frame)
- Cap shader needs `ALPHA = 0.999` to enter alpha pass where stencil read works
- Use `ImmediateMesh` from Rust for efficient per-frame quad updates

**Z-fighting prevention:**
```glsl
// In cap vertex shader - offset 0.5-1cm toward camera
vec3 dir_to_cam = normalize(CAMERA_POSITION_WORLD - world_position);
world_position += dir_to_cam * 0.01;
```

**World-space UV projection for cap texture:**
```glsl
// Build orthonormal basis on clip plane, project world position
vec3 tangent = normalize(cross(up, clip_plane_normal));
vec3 bitangent = cross(clip_plane_normal, tangent);
vec2 uv = vec2(dot(offset, tangent), dot(offset, bitangent)) * uv_scale;
```

**Global shader uniforms** keep terrain and cap synchronized:
```rust
RenderingServer::singleton().global_shader_parameter_set(
    "clip_plane_position", &position.to_variant()
);
```

### Key Decisions

- **Stencil + separate quad**: Back faces write stencil to mark interior, cap quad reads stencil. This gives pixel-perfect masking AND a flat cap surface.
- **ImmediateMesh**: Efficient for simple 6-vertex quad updated each frame from Rust
- **Global uniforms**: Single source of truth for clip plane params across all shaders
- **Camera-relative positioning**: Quad position updates each frame based on camera position + offset

## Alternatives Considered

### Stencil-only with terrain geometry (current broken implementation)

**Approach:** Three-pass stencil technique where back faces write stencil, front faces clear it, and a third pass re-renders terrain geometry where stencil remains.

**Pros:**
- No extra geometry to manage
- Classic technique for cross-sections

**Cons:**
- Pass 3 renders terrain geometry, not a flat plane
- Results in fog-like interior instead of solid cap
- The terrain's back faces at those pixels are clipped anyway

**Why not:** The cap pass renders on *terrain mesh* via `next_pass`, which draws back-face triangles — not a flat plane. Stencil identifies *where* to draw but draws the wrong *what*.

### Full-screen post-process

**Approach:** Render terrain with clip, then full-screen pass that fills clipped areas with texture based on depth/stencil.

**Pros:**
- No per-frame mesh positioning
- Could handle arbitrary clip plane shapes

**Cons:**
- More complex shader logic
- Harder to get correct UVs for the interior texture
- Post-process pipeline integration required

**Why not:** Overkill for a single flat plane. Separate quad is simpler and more direct.

## Trade-offs

| Optimizing For | Giving Up |
|----------------|-----------|
| Clean separation (terrain vs cap) | Extra mesh to manage per frame |
| Flat "glass" surface guaranteed | Need to sync quad position with clip plane |
| Easier to debug | Slightly more draw calls |
| Conceptual simplicity | Two systems instead of one |

## Risks & Open Questions

**Resolved:**
- **Masking the quad**: Stencil from back faces — pixel-perfect, proven technique
- **UV mapping on cap**: World-space projection onto clip plane with orthonormal basis
- **Z-fighting**: Vertex offset toward camera (0.5-1cm)

**Still open:**
- **Quad sizing**: Oversized quad relying on stencil mask, or dynamically sized to terrain bounds?
- **Performance validation**: Confirm three-pass approach is acceptable on target hardware
- **Godot 4.5+ requirement**: Stencil_mode is 4.5+. If targeting 4.3-4.4, need depth-buffer fallback.

## Implementation Notes

**Existing code to leverage:**
- `clip_plane_position`, `clip_plane_normal`, `clip_camera_relative` exports in `terrain.rs`
- Clip discard logic in `triplanar_pbr.gdshader`
- `stencil_write.gdshader` back-face stencil marking (may be reusable for masking)
- Underground texture exports (`underground_albedo`, etc.)

**Files likely to modify:**
- `terrain.rs` — Add cap quad mesh creation and per-frame positioning
- `triplanar_pbr.gdshader` — Verify clip logic works correctly
- New shader for cap quad (or modify `stencil_cap.gdshader`)

## Next Steps

- [ ] Review design
- [ ] Run `/walkthrough` to implement

---
*Design conversation: 2026-02-02*
