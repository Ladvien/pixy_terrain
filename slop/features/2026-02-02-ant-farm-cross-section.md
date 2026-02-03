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

1. Each frame, position a quad mesh at `clip_plane_position`, oriented along `clip_plane_normal` (perpendicular to camera forward)
2. Terrain renders normally but discards fragments beyond the clip plane
3. Cap quad renders with the interior texture, masked to only show where terrain exists (via depth or stencil test)

### Key Decisions

- **Separate mesh vs. shader-only**: Separate mesh because the cap must be a flat plane at a specific position, not a re-rendering of terrain geometry
- **Depth/stencil masking**: Cap quad needs to know where terrain exists. Options: depth test against already-rendered terrain, or stencil written by back faces. TBD during implementation.
- **Camera-relative positioning**: Quad position updates each frame based on camera position + offset

## Alternatives Considered

### Stencil-only (current implementation)

**Approach:** Three-pass stencil technique where back faces write stencil, front faces clear it, and a third pass renders the cap where stencil remains.

**Pros:**
- No extra geometry to manage
- Classic technique for cross-sections

**Cons:**
- Pass 3 renders terrain geometry, not a flat plane
- Results in fog-like interior instead of solid cap
- Harder to debug (stencil state is invisible)

**Why not:** Fundamentally can't produce a flat cap surface without significant rework. The stencil identifies *where* to draw but draws the wrong *what*.

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

- **Depth fighting**: Cap quad and terrain edge might z-fight at the clip boundary. May need slight offset or polygon offset.
- **Masking the quad**: How exactly to hide the quad where there's no terrain? Stencil from back faces seems cleanest, but need to verify.
- **Quad sizing**: Should the quad be sized to terrain bounds, or oversized and rely on masking?
- **UV mapping on cap**: How to texture the flat cap? World-space UVs projected onto the plane? Triplanar?

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
