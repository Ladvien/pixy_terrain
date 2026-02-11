# Texture Paint Presets

**Status:** Implemented
**Date:** 2026-02-11
**Files:** `rust/src/texture_preset.rs`, `rust/src/quick_paint.rs`

## Summary

Two complementary systems for managing terrain texture configurations: PixyTexturePreset saves/loads full 15-texture configs as reusable Resources, and PixyQuickPaint provides lightweight single-stroke presets combining ground texture + wall texture + grass toggle.

## What It Does

- Saves the entire texture configuration (15 textures, 15 scales, 6 grass sprites, 6 ground colors, 5 grass toggles) from a PixyTerrain into a named preset Resource
- Loads a saved preset back into a PixyTerrain, automatically syncing shader parameters
- Provides lightweight quick paint presets that encode ground/wall texture slots for single-stroke painting
- All presets persist as Godot `.tres` Resource files

## Scope

**Covers:** PixyTexturePreset (full save/load), PixyTextureList (data container), PixyQuickPaint (lightweight painting), shader parameter synchronization after loading.

**Does not cover:** Editor paint tools (see editor-plugin spec), texture rendering (see multi-texture spec), grass placement (see grass-vegetation spec).

## Interface

### PixyTexturePreset (GodotClass: Resource, tool)

| Export Property | Type | Default | Purpose |
|-----------------|------|---------|---------|
| `preset_name` | GString | "New Preset" | User-visible name |
| `textures` | Option\<Gd\<PixyTextureList\>\> | None | Embedded texture data container |

#### #[func] Methods

| Method | Signature | Purpose |
|--------|-----------|---------|
| `save_from_terrain` | `(&mut self, terrain: Gd<PixyTerrain>)` | Capture all texture settings from terrain |
| `load_into_terrain` | `(&self, terrain: Gd<PixyTerrain>)` | Apply saved settings to terrain + sync shaders |

### PixyTextureList (GodotClass: Resource, tool)

Data container with 65 export properties:

**Terrain Textures (15):** `texture_1` through `texture_15`: `Option<Gd<Texture2D>>`
**Texture Scales (15):** `scale_1` through `scale_15`: `f32` (default 1.0)
**Grass Sprites (6):** `grass_sprite_1` through `grass_sprite_6`: `Option<Gd<Texture2D>>`
**Ground Colors (6):** `grass_color_1` through `grass_color_6`: `Color` (various greens)
**Grass Toggles (5):** `has_grass_2` through `has_grass_6`: `bool` (default true)

Slot 1 always has grass (no toggle needed).

### PixyQuickPaint (GodotClass: Resource, tool)

| Export Property | Type | Default | Purpose |
|-----------------|------|---------|---------|
| `paint_name` | GString | "New Paint" | User-visible name |
| `ground_texture_slot` | i32 | 0 | Ground texture index (0-15) |
| `wall_texture_slot` | i32 | 0 | Wall texture index (0-15) |
| `has_grass` | bool | true | Whether painted area has grass |

#### #[func] Methods

| Method | Signature | Returns | Purpose |
|--------|-----------|---------|---------|
| `get_ground_colors` | `(&self)` | `Array<Color>` | Convert ground_texture_slot to [c0, c1] color pair |
| `get_wall_colors` | `(&self)` | `Array<Color>` | Convert wall_texture_slot to [c0, c1] color pair |

Both methods use `TextureIndex::to_color_pair()` to decode slot index into two one-hot RGBA colors.

## Behavior Details

### save_from_terrain() Flow

1. Reads entire texture configuration from `PixyTerrain` instance
2. Creates or clones existing `PixyTextureList`
3. Copies all 65 properties from terrain fields:
   - Textures: `ground_texture` -> `texture_1`, `texture_2`..`texture_15`
   - Scales: `texture_scale_1`..`texture_scale_15`
   - Grass sprites: `grass_sprite`, `grass_sprite_tex_2`..`grass_sprite_tex_6`
   - Ground colors: `ground_color`, `ground_color_2`..`ground_color_6`
   - Grass toggles: `tex2_has_grass`..`tex6_has_grass`
4. Stores list in `self.textures`

### load_into_terrain() Flow

1. Validates `self.textures` exists (warns if None)
2. Copies all 65 properties back into terrain (inverse of save)
3. **Calls `terrain.force_batch_update()`** to synchronize shader parameters
4. This updates:
   - Ground albedo colors (6 slots)
   - Texture scales (15 slots)
   - Grass sprites and toggles
   - All active chunk materials

### Texture Slot Encoding (QuickPaint)

Uses `TextureIndex` (0-15) mapped to two one-hot RGBA vertex colors:

```rust
// Slot -> Color Pair
TextureIndex(slot as u8).to_color_pair()
// Example: slot 5 = (Green, Green) = (0,1,0,0), (0,1,0,0)
// Example: slot 7 = (Green, Alpha) = (0,1,0,0), (0,0,0,1)
```

Returns `Array<Color>` with exactly 2 elements for GDScript consumption.

### QuickPaint Integration with Editor

The editor plugin loads PixyQuickPaint presets and offers them in a dropdown:
- When active during height/painting operations:
  - `get_ground_colors()` provides vertex colors for floor painting
  - `get_wall_colors()` provides vertex colors for wall painting
  - `has_grass` determines grass mask behavior
- QuickPaint is standalone -- not coupled to save_from_terrain/load_into_terrain

### Default Ground Colors

| Slot | Default Color | Description |
|------|--------------|-------------|
| 1 | RGBA(0.392, 0.471, 0.318, 1.0) | Forest green |
| 2 | RGBA(0.322, 0.482, 0.384, 1.0) | Muted green |
| 3 | RGBA(0.373, 0.424, 0.294, 1.0) | Olive |
| 4 | RGBA(0.392, 0.475, 0.255, 1.0) | Yellow-green |
| 5 | RGBA(0.290, 0.494, 0.365, 1.0) | Teal-green |
| 6 | RGBA(0.443, 0.447, 0.365, 1.0) | Sage green |

### Data Flow

```
PixyTerrain (runtime state)
    |
    v  [save_from_terrain()]
PixyTexturePreset.textures (PixyTextureList)
    |
    v  [persisted as .tres file]
    |
    v  [load_into_terrain()]
PixyTerrain (updated)
    |
    v  [force_batch_update()]
Terrain Material + Grass Material (shader parameters updated)
```

## Acceptance Criteria

- save_from_terrain captures all 65 properties correctly
- load_into_terrain restores them and updates shader parameters
- Round-trip: save then load produces identical terrain appearance
- QuickPaint color pairs round-trip through TextureIndex encoding (unit tested)
- Presets persist as .tres files and load across editor sessions

## Technical Notes

- PixyTexturePreset stores Godot Resource references (not texture data directly) -- references remain valid in .tres files
- Only 6 grass sprites exist (one per ground color variant) but 15 texture slots exist; grass planter maps based on slot index
- `force_batch_update()` syncs ALL shader parameters, not just the ones that changed -- simpler than tracking deltas
- PixyTextureList is used internally; users interact with PixyTexturePreset wrapper
- No signals defined on any of these types
