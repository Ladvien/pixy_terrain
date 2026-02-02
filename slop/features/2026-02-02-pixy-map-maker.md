# Feature: Pixy Map Maker

**Date:** 2026-02-02
**Status:** Draft

## Problem

A small indie team needs to quickly create 3D pixel art terrain in Godot with caves and an "ant farm" cross-section view.

**Current pain points:**
- No existing workflow for 3D pixel art terrain
- External assets don't align to grid, wasting time on alignment
- Standard Godot terrain tools don't support subterrain/caves
- Voxel approaches (transvoxel, dual contouring) produce organic surfaces that clash with pixel art aesthetic and make clean cross-sections difficult

**Who has this problem:** Small indie teams building 3D pixel art games with destructible/explorable terrain.

**Why now:** Blocking game development. Need to ship a damn game.

## Goals

**MVP (Must Have):**
- Basic tile placement/editing in Godot editor (lay out terrain fast)
- WANG tile wizard (generate cohesive 3D tile sets procedurally)
- Clean "ant farm" cross-section view of terrain

**Full Vision:**
- Multi-tile props (large Blender assets that snap to grid)
- Destructible terrain (runtime tile replacement when digging)
- WFC-based auto-fill for rapid level layout
- Decal/overlay painting (grid-independent detail: weathering, paths, moss, etc.)
- Runtime decals (footprints, damage, dynamic effects)

## Non-Goals

- Organic/smooth terrain (that's what Terrain3D is for)
- Mobile optimization (desktop-first, mobile is stretch)
- GDScript implementation (Rust/GDExtension only)
- Runtime tile generation (WANG wizard is editor-time only)

## Proposed Solution

A tile-based terrain editor built on Godot's GridMap, with a WANG tile wizard for procedural tile set generation and WFC for intelligent auto-placement.

### Core Philosophy

**"Easy but not limiting"** - Paint broad strokes, system handles details, manual override when needed. Progressive disclosure of complexity.

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Godot Editor                         │
├─────────────────────────────────────────────────────────┤
│  PixyMapMaker Plugin (Rust GDExtension)                │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐ │
│  │ WANG Wizard │  │ Tile Editor │  │ Cross-Section   │ │
│  │ (tile gen)  │  │ (placement) │  │ Renderer        │ │
│  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘ │
│         │                │                   │          │
│  ┌──────▼────────────────▼───────────────────▼────────┐ │
│  │              Tile Rule System                      │ │
│  │         (6-face edge matching, WFC rules)          │ │
│  └──────────────────────┬─────────────────────────────┘ │
│                         │                               │
│  ┌──────────────────────▼─────────────────────────────┐ │
│  │              Godot GridMap                         │ │
│  │         (native tile placement & rendering)        │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### Key Components

#### 1. Project Initialization
- **Tile size is mandatory** - User must choose base tile dimensions at project start
- No default value - forces intentional decision
- Affects all downstream generation and placement

#### 2. WANG Tile Wizard
User inputs:
- **Block mesh** - Base geometry for the tile
- **Texture A** (the "cake") - Primary/base terrain texture
- **Texture B** (the "icing") - Secondary/overlay terrain texture
- **Interaction model** - How textures relate (A is base, B covers it)
- **Feathering style** - How the two textures blend at transitions
- **Metadata** - Tags, properties for the generated tiles
- **Static props** - Optional decorations attached to tiles

System outputs:
- All edge-matching tile variants for the two terrain types
- **Full 3D edge matching** - 6 faces, not just top face
- MeshLibrary compatible with GridMap
- Auto-generated WFC rules for the tile set

#### 3. Tile Editor
- Paint regions at high level (this area = grass, that = cave)
- WFC auto-resolves transitions between regions
- Manual override for any individual tile
- Keyboard shortcuts for speed (like current T/W/C keys)

#### 4. Cross-Section Renderer
- "Ant farm" view - see underground like a side cutaway
- **Textbook-style cutaway** - like anatomical diagrams where a body is sliced in half
- Cut surface must show "innards" texture that looks like solid material, not hollow
- Works at any zoom level
- **Must look clean and pretty** - this is non-negotiable
- Approach TBD: clip plane shader, geometry slicing, or camera trick

#### 5. Tile Rule System
- Developer-friendly rule definition for WFC
- Defines which tile edges can connect to which
- Supports procedural generation of base tiles
- **Compound tags for edge matching** (see Data Format below)

### Tile Data Format

```rust
struct TileDefinition {
    // Identity
    id: u32,
    name: String,

    // Geometry
    mesh: Gd<Mesh>,

    // Edge matching - 6 faces: +X, -X, +Y, -Y, +Z, -Z
    // Uses compound tags (e.g., "dirt_wet") for simple matching
    face_tags: [String; 6],

    // WFC
    weight: f32,  // Probability weight for selection

    // Materials
    terrain_type: TerrainTypeId,

    // Metadata
    tags: Vec<String>,  // "grass", "cave_entrance", etc.
    props: Vec<PropPlacement>,  // Static props attached
}

struct ConnectionRule {
    tag_a: String,
    tag_b: String,
    allowed: bool,
}
```

**Edge Matching Decision:** Compound tags over multi-tag priority system.

Instead of `["dirt", "wet"]` with complex priority evaluation, use specific compound tags like `"dirt_wet"`. This is simpler to implement, debug, and reason about. Follows the "design to strengths" philosophy from WFC literature—strong tileset design prevents problems better than complex rule systems.

Reference: [Boris the Brave on WFC](https://www.boristhebrave.com/2020/02/08/wave-function-collapse-tips-and-tricks/), [Tessera](https://www.boristhebrave.com/2021/10/31/constraint-based-tile-generators/)

### How It Works

**Tile Creation Flow:**
1. User runs WANG wizard
2. Selects two terrain types + transition options
3. Wizard generates all edge-matching tile variants
4. Tiles export to MeshLibrary
5. Rules auto-generated for WFC

**Level Editing Flow:**
1. User paints broad regions (grass area, cave entrance, cliff)
2. WFC fills in transition tiles automatically
3. User manually tweaks any tile that needs adjustment
4. Props placed on top of tile grid
5. Cross-section view used to verify underground looks right

### Key Decisions

- **GridMap over custom solution**: Lean into Godot's existing system. Less code, better integration, native performance.
- **Full 3D WANG (6-face)**: Required for caves and vertical terrain. More complex but necessary.
- **Rust/GDExtension only**: Performance matters, team prefers Rust, no GDScript unless absolutely required.
- **Tile size as forced choice**: Prevents "default trap" and ensures intentional grid decisions.
- **Editor-time WANG generation**: Keeps runtime simple, allows inspection/tweaking of generated tiles.
- **Compound tags for edge matching**: Use `"dirt_wet"` instead of `["dirt", "wet"]` with priority rules. Simpler to implement and debug. Strong tileset design > complex rule systems.
- **Decals are Full Vision, not MVP**: Decals are polish (make good terrain look better). WANG tiles are foundational (create the terrain blocks). Ship MVP first, add decals when terrain feels "too clean."

## Alternatives Considered

### Pure Manual Placement
**Approach:** Simple tile palette, user places each tile manually.

**Pros:**
- Simpler to build
- Full control

**Cons:**
- Too slow for iteration
- Tedious for large maps
- Error-prone for transitions

**Why not:** Speed is critical for small indie team. Manual placement doesn't scale.

### Heightmap-Based Terrain
**Approach:** Paint height values, system picks appropriate tiles.

**Pros:**
- Intuitive for surface terrain
- Fast for outdoor areas

**Cons:**
- Can't represent caves/overhangs
- Limited to 2.5D effectively

**Why not:** Caves and subterrain are core requirements.

### Marching Cubes / Voxels with Pixel Art Textures
**Approach:** Keep voxel representation, use stylized rendering.

**Pros:**
- Organic shapes possible
- Destructibility is natural

**Cons:**
- Produces smooth/organic surfaces (wrong aesthetic)
- Stencil cap rendering was problematic
- Meshing complexity was painful
- Clean cross-sections difficult

**Why not:** Already tried this. Too complex, wrong aesthetic for 3D pixel art.

### Existing Plugins (Tile to Gridmap, WFC plugins)
**Approach:** Use or fork existing Godot plugins.

**Pros:**
- Less code to write
- Community support

**Cons:**
- [Tile to Gridmap](https://godotengine.org/asset-library/asset/3672): No WANG wizard, no WFC, no cave support
- [godot-constraint-solving](https://github.com/AlexeyBond/godot-constraint-solving): 3D GridMap "not fully implemented"
- [WFC 3D](https://godotengine.org/asset-library/asset/2888): Old, incomplete

**Why not:** None combine WANG + WFC + caves + 3D pixel art. Gap is too large to bridge.

## Trade-offs

| Optimizing For | Giving Up |
|----------------|-----------|
| Speed of iteration | Some organic terrain flexibility |
| Clean tile edges | Fully smooth transitions |
| Godot-native (GridMap) | Custom optimizations |
| Predictable geometry | Organic randomness |
| Editor-time generation | Runtime tile variety |

## Risks & Open Questions

### Known Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| **Performance with large maps** | Medium | Chunk-based loading, LOD, profile early |
| **"Too tiled" look** | Medium | More tile variants, rotation/flip randomization, props to break patterns |
| **Losing organic feel** | Low | Intentional - pixel art aesthetic is blocky by nature |
| **Being boxed in by GridMap** | Low | Keep tile format simple/extensible, don't over-engineer rules |

*Team assessment: These are "solve later" problems, not blockers.*

### Open Questions

- **Cross-section rendering approach**: Clip plane shader? Geometry slicing? Camera trick? Needs prototyping. Must show solid "innards" texture at cut surface.
- **WANG wizard UI/UX**: How to make terrain selection and feathering options intuitive?
- **Feathering algorithm**: How exactly do two textures blend? Noise-based? Distance-based? User-configurable patterns?
- **Tile size recommendations**: What sizes work best for 3D pixel art? Should we provide guidance even if we don't enforce defaults?

### Resolved Questions

- **Edge matching approach**: Compound tags (decided). Simpler than multi-tag priority systems.
- **Decals in MVP**: No (decided). Moved to Full Vision. WANG tiles are foundational, decals are polish.

## Constraints

- Godot 4.6+
- Rust / GDExtension (GDScript only as last resort)
- Desktop-first (mobile is stretch goal)
- No specific performance targets yet

## Next Steps

### MVP Phase 1: Foundation
- [ ] Define tile data format and rule structure
- [ ] Basic tile placement on GridMap
- [ ] Tile size configuration at project init

### MVP Phase 2: WANG Wizard
- [ ] Terrain type definition (materials, textures)
- [ ] Procedural tile generation for two terrain types
- [ ] 6-face edge matching generation
- [ ] MeshLibrary export

### MVP Phase 3: WFC Integration
- [ ] Rule definition system
- [ ] Basic WFC solver for GridMap
- [ ] Region painting UI
- [ ] Manual override system

### MVP Phase 4: Cross-Section
- [ ] Prototype rendering approaches
- [ ] Implement cleanest solution
- [ ] Polish for "pretty" requirement

### Post-MVP
- [ ] Multi-tile prop support
- [ ] Destructible terrain runtime
- [ ] Performance optimization
- [ ] Mobile considerations

---

*Design conversation: 2026-02-02*
*Last updated: 2026-02-02 - Added tile data format, compound tag decision, WANG wizard inputs, decals moved to Full Vision*
