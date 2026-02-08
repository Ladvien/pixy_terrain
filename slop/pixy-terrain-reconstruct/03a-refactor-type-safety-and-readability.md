# Pixy Terrain — Part 03a: Refactor for Type Safety & Readability

**Series:** Reconstructing Pixy Terrain
**Part:** 03a of 18 (addendum — applied after Part 03, before Part 04)
**Previous:** 03-vertex-generation-color-encoding.md
**Status:** Complete

## Why This Addendum Exists

After completing Parts 02-03, the code in `marching_squares.rs` compiled and was functionally correct, but had serious maintainability problems:

- **4 duplicate argmax implementations** — finding the dominant RGBA channel was copy-pasted in `get_dominant_color`, `get_texture_index_from_colors` (twice), and `texture_index_to_colors`
- **Raw `i32` primitives** where domain types belonged — `blend_mode`, `cell_material_a/b/c`, and texture indices were all bare integers with no type safety
- **A heap allocation per cell** — `HashMap` used to count 4 items over a 0-15 domain in `calculate_cell_material_pair`, creating 1,024 unnecessary allocations per chunk rebuild
- **Magic numbers throughout** — `0.99`, `0.001`, `16.0`, `255.0`, `15.0`, `0.3`, `0.7`, `2.0` scattered across functions with no explanation
- **A latent bug** — `calculate_material_blend_data` accepted source map parameters but ignored them, always reading from floor color maps even for wall vertices
- **Opaque expressions** — compound boolean conditions and inline arithmetic with no explaining variables

This refactor addresses all of the above before Part 04 adds 400+ lines of geometry cases on top of this foundation.

## What Changed

### File Modified

- `rust/src/marching_squares.rs` — the only file touched

### Bug Fixed

**`calculate_material_blend_data` ignored passed source maps.** The function accepted `source_map_0`/`source_map_1` but read corner textures from `ctx.color_map_0`/`ctx.color_map_1` (always floor maps). The Yugen GDScript (`marching_squares_terrain_chunk.gd:920-971`) reads from the *passed* maps — wall maps for wall/ridge vertices, floor maps for floor vertices. Fixed by reading from the passed parameters instead of hardcoded floor maps.

## Steps

### Step 1: Extract Constants

**Why:** Magic numbers make the code impossible to reason about. `0.001` appears in two places with different meanings (minimum weight threshold vs minimum height range). `0.3`/`0.7` are the color_1 blend thresholds but look identical to any other arbitrary float. Named constants make each value's purpose self-documenting.

**What changed:** Replaced the lone `BLEND_EDGE_SENSITIVITY` constant with a full block:

```rust
pub const BLEND_EDGE_SENSITIVITY: f32 = 1.25;
const DEFAULT_TEXTURE_COLOR: Color = Color::from_rgba(1.0, 0.0, 0.0, 0.0);
const DOMINANT_CHANNEL_THRESHOLD: f32 = 0.99;
const MIN_WEIGHT_THRESHOLD: f32 = 0.001;
const MIN_HEIGHT_RANGE: f32 = 0.001;
const MATERIAL_PACK_SCALE: f32 = 16.0;
const MATERIAL_PACK_NORMALIZE: f32 = 255.0;
const MATERIAL_INDEX_SCALE: f32 = 15.0;
const COLOR_1_LOWER_THRESHOLD: f32 = 0.3;
const COLOR_1_UPPER_THRESHOLD: f32 = 0.7;
const WALL_BLEND_SENTINEL: f32 = 2.0;
```

All magic number usage sites updated to reference these constants.

### Step 2: `BlendMode` Enum

**Why:** `blend_mode: i32` was only ever compared to `1`. The intent ("use corner A's color directly" vs "bilinear interpolation") was invisible without reading the GDScript source.

**What changed:** Added a two-variant enum, updated `CellContext.blend_mode` field type, and replaced all 3 comparison sites:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    #[default]
    Interpolated, // bilinear interpolation across corners
    Direct,       // use corner A's color directly
}
```

- `ctx.blend_mode == 1` became `ctx.blend_mode == BlendMode::Direct`
- `ctx.blend_mode != 1` became `ctx.blend_mode != BlendMode::Direct`

### Step 3: `ColorChannel` Enum (consolidates 4 duplicate argmax)

**Why:** The "find max RGBA channel, return one-hot color" pattern was implemented 4 separate times: once in `get_dominant_color`, twice in `get_texture_index_from_colors` (once for each color input), and once in `texture_index_to_colors` (the reverse mapping). Four implementations means four places for bugs to hide and four places to update if the logic changes.

**What changed:** Single enum with one implementation of the argmax:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChannel {
    Red = 0,
    Green = 1,
    Blue = 2,
    Alpha = 3,
}

impl ColorChannel {
    pub fn dominant(c: Color) -> Self { /* single argmax impl */ }
    pub fn dominant_index(c: Color) -> u8 { Self::dominant(c) as u8 }
    pub fn from_index(idx: u8) -> Self { /* match 0-3 */ }
    pub fn to_one_hot(self) -> Color { /* match → one-hot Color */ }
}
```

`get_dominant_color` reduced to a one-line wrapper: `ColorChannel::dominant(c).to_one_hot()`.

### Step 4: `TextureIndex` Newtype

**Why:** Texture indices (0-15, encoding which of 16 possible textures from the 4x4 RGBA grid) were raw `i32`. This meant `cell_material_a`, `cell_material_b`, and `cell_material_c` had no type distinction from any other integer. The encoding/decoding functions (`get_texture_index_from_colors`, `texture_index_to_colors`) were free functions with no connection to the type they operated on.

**What changed:** Newtype wrapping `u8`, with encoding/decoding as methods:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextureIndex(pub u8);

impl TextureIndex {
    pub fn from_color_pair(c0: Color, c1: Color) -> Self { /* uses ColorChannel */ }
    pub fn to_color_pair(self) -> (Color, Color) { /* reverse */ }
    pub fn as_f32(self) -> f32 { self.0 as f32 }
}
```

- `cell_material_a`/`b`/`c` changed from `i32` to `TextureIndex`
- `get_texture_index_from_colors` and `texture_index_to_colors` deleted — replaced by `TextureIndex::from_color_pair` and `TextureIndex::to_color_pair`
- Material packing uses `self.cell_material_a.as_f32()` instead of `ctx.cell_material_a as f32`

### Step 5: Method Migration

**Why:** `corner_indices(cc, dim_x)`, `calculate_cell_material_pair(ctx)`, `calculate_boundary_colors(ctx)`, and `calculate_material_blend_data(ctx, ...)` were all free functions that always read from `CellContext` fields. Threading `cc`/`dim_x` parameters through every call was noisy and error-prone. These are naturally methods on `CellContext`.

**What changed:**

- `corner_indices(cc, dim_x)` → `CellContext::corner_indices(&self)` — eliminates parameter threading at all 4 call sites
- `calculate_cell_material_pair(ctx)` → `CellContext::calculate_cell_material_pair(&mut self)` — also replaced `HashMap` with `[u8; 16]` frequency table (zero heap allocations)
- `calculate_boundary_colors(ctx)` → `CellContext::calculate_boundary_colors(&mut self)` — uses `self.corner_indices()` internally
- `calculate_material_blend_data(ctx, ...)` → `CellContext::calculate_material_blend_data(&self, ...)` — **bug fix**: now reads from passed `source_map_0`/`source_map_1` instead of hardcoded `self.color_map_0`/`self.color_map_1`

The standalone `corner_indices` free function was deleted.

### Step 6: HashMap → `[u8; 16]` Frequency Table

**Why:** `calculate_cell_material_pair` used `std::collections::HashMap` to count which of 4 corner textures appeared most often. With texture indices in 0-15, a fixed-size array is the obvious choice — zero heap allocation, cache-friendly, and clearer intent. With 1,024 cells per chunk, this eliminated 1,024 HashMap allocations per rebuild.

**What changed:**

```rust
// Before: heap allocation per cell
let mut counts = std::collections::HashMap::new();
for texture in [...] { *counts.entry(texture).or_insert(0) += 1; }
let mut sorted: Vec<_> = counts.into_iter().collect();
sorted.sort_by(|a, b| b.1.cmp(&a.1));

// After: stack-only, linear scan for top 3
let mut counts = [0u8; 16];
for t in [...] { counts[t.0 as usize] += 1; }
// Triple linear scan to find first/second/third
```

### Step 7: `preserve_high_channels()` Helper

**Why:** The 4-line channel normalization pattern (if either input has a channel > 0.99, force the output to 1.0) was inlined in `compute_vertex_color`. Extracting it makes the diagonal midpoint path read as intent rather than arithmetic.

**What changed:**

```rust
#[inline]
fn preserve_high_channels(mut color: Color, a: Color, b: Color) -> Color {
    if a.r > DOMINANT_CHANNEL_THRESHOLD || b.r > DOMINANT_CHANNEL_THRESHOLD { color.r = 1.0; }
    if a.g > DOMINANT_CHANNEL_THRESHOLD || b.g > DOMINANT_CHANNEL_THRESHOLD { color.g = 1.0; }
    if a.b > DOMINANT_CHANNEL_THRESHOLD || b.b > DOMINANT_CHANNEL_THRESHOLD { color.b = 1.0; }
    if a.a > DOMINANT_CHANNEL_THRESHOLD || b.a > DOMINANT_CHANNEL_THRESHOLD { color.a = 1.0; }
    color
}
```

Call site: `let color = preserve_high_channels(c, ad_color, bc_color);`

### Step 8: Explaining Variables

**Why:** Compound boolean expressions and inline arithmetic obscured intent. Adding named intermediate values makes the code read as English.

**What changed in `add_point`:**

```rust
// Before:
let is_ridge = ctx.floor_mode && ctx.use_ridge_texture && (uv.y > 1.0 - ctx.ridge_threshold);
let use_wall_colors = !ctx.floor_mode || is_ridge;

// After:
let near_cliff_top = uv.y > 1.0 - ctx.ridge_threshold;
let is_ridge = ctx.floor_mode && ctx.use_ridge_texture && near_cliff_top;
let use_wall_colors = !ctx.floor_mode || is_ridge;
```

```rust
// Before:
let blend_ab = (ctx.ay() - ctx.by()).abs() < blend_threshold;
// ... 3 more
if !(blend_ab && blend_ac && blend_bd && blend_cd) && ctx.floor_mode {
    material_blend.a = 2.0;
}

// After:
let all_edges_merged = {
    let ab_merged = (ctx.ay() - ctx.by()).abs() < blend_threshold;
    let ac_merged = (ctx.ay() - ctx.cy()).abs() < blend_threshold;
    let bd_merged = (ctx.by() - ctx.dy()).abs() < blend_threshold;
    let cd_merged = (ctx.cy() - ctx.dy()).abs() < blend_threshold;
    ab_merged && ac_merged && bd_merged && cd_merged
};
let floor_has_nearby_walls = !all_edges_merged && ctx.floor_mode;
if floor_has_nearby_walls {
    material_blend.a = WALL_BLEND_SENTINEL;
}
```

**What changed in `compute_vertex_color`:**

```rust
// Before:
let height_factor = if height_range > 0.001 {
    ((y - ctx.cell_min_height) / height_range).clamp(0.0, 1.0)

// After:
let has_meaningful_height_range = height_range > MIN_HEIGHT_RANGE;
let height_factor = if has_meaningful_height_range {
    let normalized_height = (y - ctx.cell_min_height) / height_range;
    normalized_height.clamp(0.0, 1.0)
```

### Step 9: Naming and Cleanup

**What changed:**

- `y` parameter in `add_point` renamed to `height` (it's world-space height, not cell-local Y). `safe_y` → `safe_height`.
- References removed on `Color` values in `add_point` — `Color` is `Copy`, so `&ctx.cell_wall_lower_color_0` → `ctx.cell_wall_lower_color_0` and `*lower_0` → `lower_0`
- Consistent semicolons on `start_floor()`/`start_wall()`

## What Was NOT Changed

- `CellContext` kept flat (no sub-structs) — decided in Part 02
- File not split — still ~730 lines, manageable as one file
- No external crates added — `contour-rs` evaluated and rejected (wrong problem domain: 2D contour lines, not 3D mesh generation)
- `compute_vertex_color` left as a free function — intentionally decoupled from `CellContext` via `ColorSampleParams`
- No tests added — that's Part 05
- `generate_cell` case documentation deferred — will be added when those cases are built in Parts 04-05

## Remaining Phase 4 Items (Deferred)

These are minor structural cleanup items that can be done at the start of Part 04:

- `#[inline]` on `sanitize_float` and `lerp_color` (hot-path trivial functions)
- `#[must_use]` on `lerp_color`
- Move `ColorSampleParams` above its first use (into the Types section)
- Section header comments (`// ===== Constants =====`, etc.)

## Verification

```bash
cd rust && cargo build    # compiles with only "never used" warnings
cd rust && cargo clippy   # clean
```

## Impact Summary

1. **4 duplicate argmax → 1 implementation** via `ColorChannel::dominant_index`
2. **1,024 HashMap allocations eliminated** per chunk rebuild via `[u8; 16]`
3. **Source map bug fixed** — wall vertices now use wall color maps for material blend
4. **3 raw `i32` fields → typed** (`BlendMode`, `TextureIndex`)
5. **11 magic numbers → named constants**
6. **5 free functions → CellContext methods** eliminating parameter threading
7. **3 explaining variable blocks** making blend logic self-documenting
