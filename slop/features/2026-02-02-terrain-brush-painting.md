# Feature: Terrain Brush Painting

**Date:** 2026-02-02
**Status:** Draft

## Problem

Game developers need to quickly create intentional terrain shapes for their games. The current random noise generation doesn't allow for designed terrain — users can't sculpt specific hills, valleys, or plateaus.

**Users:** Game developers using the Pixy Terrain editor in Godot 4.

## Goals

- Two-phase paint interaction (paint area → set height)
- Visual feedback via transparent raycast projection (+Y to -Y)
- Texture painting (separate mode from geometry)
- Multiple brush shapes (square, round)
- Undo/redo support
- Save/load terrain (voxel data)
- Editor-only tool (Godot `@tool`)

## Non-Goals

- Runtime terrain editing (Minecraft-style)
- Procedural terrain generation (keeping noise as separate feature)

## Proposed Solution

A two-phase brush interaction for painting terrain geometry, with a separate mode for texture painting.

### How It Works

**Geometry Painting Flow:**

1. **Select brush** — Choose square or round brush, set size parameter
2. **Phase 1 (Paint Area)** — Click and hold, drag to paint footprint on terrain surface
   - Visual: Transparent projection from +Y to -Y shows painted region
   - Footprint grows as user drags
3. **Release** — Footprint is locked in
4. **Phase 2 (Set Height)** — Click and hold, drag vertically to raise/lower terrain
   - Drag up = raise terrain
   - Drag down = lower terrain (dig)
   - Real-time mesh preview as user drags
5. **Release** — Commit operation
   - Blends with existing terrain geometry
   - Counts as one undo step

**Under the Hood:**

- Phase 1 marks voxels within brush footprint
- Phase 2 modifies voxel density/occupancy for marked voxels
- Transvoxel mesh regenerates from modified voxel grid
- Overlapping paints blend at voxel level

**Texture Painting:**

- Separate mode/tool from geometry brush
- Terrain has a default texture
- (Details TBD in implementation)

**Save/Load:**

- Saves voxel grid data
- Mesh regenerated from voxel data on load

### Key Decisions

- **Two-phase interaction**: Separates area selection from height adjustment for precision control over plateaus and flat-topped features
- **Voxel-based editing**: Preserves existing transvoxel architecture, enables future features like caves/overhangs
- **Blend on overlap**: Natural behavior when painting over existing terrain
- **One undo step per paint sequence**: Paint area + set height treated as atomic operation
- **Save voxel data (not mesh)**: Allows continued editing after reload

## Alternatives Considered

(User declined to explore alternatives — firm on two-phase interaction model)

## Trade-offs

| Optimizing For | Giving Up |
|----------------|-----------|
| Precision (separate area/height phases) | Speed of single-stroke sculpting |
| Voxel flexibility (caves possible later) | Simplicity of heightmap approach |
| Full edit capability on reload | Smaller save file size (mesh-only) |

## Risks & Open Questions

- **Performance during Phase 2**: Regenerating transvoxel mesh on every mouse move could be expensive. May need chunk-based updates or throttling.
- **Voxel resolution vs brush precision**: Grid density affects both visual fidelity and performance. Need to determine target density.
- **Blend behavior specifics**: "Blend" could mean additive, max, average — needs clarification during implementation.
- **Editor tool input handling**: Godot `@tool` scripts + gdext for viewport input is relatively unexplored territory. May hit quirks.
- **Undo system**: Use Godot's built-in `UndoRedo` or custom implementation?

## Next Steps

- [ ] Review design
- [ ] Run `/walkthrough` to implement

---
*Design conversation: 2026-02-02*
