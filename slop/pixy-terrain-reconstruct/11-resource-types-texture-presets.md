# Pixy Terrain — Part 11: Resource Types & Texture Presets

**Series:** Reconstructing Pixy Terrain
**Part:** 11 of 18
**Previous:** 2026-02-06-terrain-manager-chunk-operations-10.md
**Status:** Complete

## What We're Building

Two custom Resource types that let artists save and recall texture configurations. `PixyQuickPaint` is a lightweight preset that stores a ground texture slot, wall texture slot, and grass toggle for single-stroke painting. `PixyTexturePreset` (with its companion `PixyTextureList`) is a heavyweight container that snapshots every texture, scale, grass sprite, grass color, and grass toggle from the terrain — 56 fields in total — for full save/load workflow.

## What You'll Have After This

Two new Resource classes visible in the Godot inspector. `PixyQuickPaint` can be created as a `.tres` file and assigned to an editor tool mode to paint with a specific texture/grass combination. `PixyTexturePreset` can snapshot the entire terrain's texture configuration and restore it later — or share it across projects. Both files compile and integrate with the terrain manager from Parts 09-10.

## Prerequisites

- Part 10 completed (`PixyTerrain` with chunk operations, `force_batch_update()`, all 50+ export fields)
- Part 03 completed (`texture_index_to_colors()` function in `marching_squares.rs`)

## Steps

### Step 1: Create `PixyQuickPaint` resource

**Why:** The editor's paint tool needs a way to bundle "ground texture slot + wall texture slot + grass toggle" into a single reusable preset. Rather than passing three separate values every time, artists create a `PixyQuickPaint` resource, configure it once, and assign it to a tool slot. The resource also exposes helper methods that convert the integer texture slot index into the vertex color pair encoding from Part 03.

**File:** `rust/src/quick_paint.rs` (replace the empty stub from Part 01)

```rust
use godot::classes::Resource;
use godot::prelude::*;

/// Quick paint preset: applies ground texture + wall texture + grass toggle in a single stroke.
/// Port of Yugen's MarchingSquaresQuickPaint.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyQuickPaint {
    base: Base<Resource>,

    /// User-visible name for this quick paint preset.
    #[export]
    #[init(val = GString::from("New Paint"))]
    pub paint_name: GString,

    /// Ground texture slot (0-15, maps to color encoding).
    #[export]
    #[init(val = 0)]
    pub ground_texture_slot: i32,

    /// Wall texture slot (0-15, maps to color encoding).
    #[export]
    #[init(val = 0)]
    pub wall_texture_slot: i32,

    /// Whether grass is enabled for this paint preset.
    #[export]
    #[init(val = true)]
    pub has_grass: bool,
}

#[godot_api]
impl PixyQuickPaint {
    /// Get the vertex color pair for the ground texture slot.
    #[func]
    pub fn get_ground_colors(&self) -> Array<Color> {
        let (c0, c1) = crate::marching_squares::texture_index_to_colors(self.ground_texture_slot);
        let mut arr = Array::new();
        arr.push(c0);
        arr.push(c1);
        arr
    }

    /// Get the vertex color pair for the wall texture slot.
    #[func]
    pub fn get_wall_colors(&self) -> Array<Color> {
        let (c0, c1) = crate::marching_squares::texture_index_to_colors(self.wall_texture_slot);
        let mut arr = Array::new();
        arr.push(c0);
        arr.push(c1);
        arr
    }
}
```

**What's happening:**

`PixyQuickPaint` extends `Resource`, not `Node`. This is the key design choice: Resources are data-only objects that Godot can serialize to `.tres` files, embed inside other resources, and duplicate cheaply. They don't live in the scene tree.

The `#[class(base=Resource, init, tool)]` attribute tells gdext three things:
1. `base=Resource` — this class extends Godot's Resource class.
2. `init` — generate a default constructor (required for Godot to create instances in the inspector).
3. `tool` — this class runs in the editor, not just at runtime.

The `get_ground_colors()` and `get_wall_colors()` methods bridge two encoding systems. The artist thinks in texture slot indices (0-15, visible in the inspector as an integer). The mesh system thinks in vertex color pairs (two `Color` values per vertex that the shader decodes into a texture index). `texture_index_to_colors()` from Part 03 performs that conversion — given slot index 5, it returns the specific `(Color, Color)` pair that the shader interprets as "sample texture 5".

The methods return `Array<Color>` rather than a tuple because GDScript has no tuple type. The array always has exactly 2 elements: `[color_0, color_1]`.

### Step 2: Define `PixyTextureList` resource

**Why:** The terrain has 56 texture-related fields spread across 15 texture slots, 15 scale values, 6 grass sprites, 6 grass colors, and 5 has_grass toggles. `PixyTextureList` mirrors all of these fields in a single Resource, creating a portable snapshot of the terrain's texture state. This is the data container — it holds the values but doesn't know how to apply them.

**File:** `rust/src/texture_preset.rs` (replace the stub from Part 09)

```rust
use godot::classes::{Resource, Texture2D};
use godot::prelude::*;

use crate::terrain::PixyTerrain;

/// Stores per-texture configuration data: textures, scales, grass sprites, grass colors, grass toggles.
/// Port of Yugen's MarchingSquaresTextureList.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyTextureList {
    base: Base<Resource>,

    // 15 terrain textures (slots 1-15)
    #[export]
    pub texture_1: Option<Gd<Texture2D>>,
    #[export]
    pub texture_2: Option<Gd<Texture2D>>,
    #[export]
    pub texture_3: Option<Gd<Texture2D>>,
    #[export]
    pub texture_4: Option<Gd<Texture2D>>,
    #[export]
    pub texture_5: Option<Gd<Texture2D>>,
    #[export]
    pub texture_6: Option<Gd<Texture2D>>,
    #[export]
    pub texture_7: Option<Gd<Texture2D>>,
    #[export]
    pub texture_8: Option<Gd<Texture2D>>,
    #[export]
    pub texture_9: Option<Gd<Texture2D>>,
    #[export]
    pub texture_10: Option<Gd<Texture2D>>,
    #[export]
    pub texture_11: Option<Gd<Texture2D>>,
    #[export]
    pub texture_12: Option<Gd<Texture2D>>,
    #[export]
    pub texture_13: Option<Gd<Texture2D>>,
    #[export]
    pub texture_14: Option<Gd<Texture2D>>,
    #[export]
    pub texture_15: Option<Gd<Texture2D>>,

    // 15 texture scales
    #[export]
    #[init(val = 1.0)]
    pub scale_1: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_2: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_3: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_4: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_5: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_6: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_7: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_8: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_9: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_10: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_11: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_12: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_13: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_14: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale_15: f32,

    // 6 grass sprites (slots 1-6)
    #[export]
    pub grass_sprite_1: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_2: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_3: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_4: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_5: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_6: Option<Gd<Texture2D>>,

    // 6 grass/ground colors (slots 1-6)
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4706, 0.3176, 1.0))]
    pub grass_color_1: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3216, 0.4824, 0.3843, 1.0))]
    pub grass_color_2: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3725, 0.4235, 0.2941, 1.0))]
    pub grass_color_3: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4745, 0.2549, 1.0))]
    pub grass_color_4: Color,
    #[export]
    #[init(val = Color::from_rgba(0.2902, 0.4941, 0.3647, 1.0))]
    pub grass_color_5: Color,
    #[export]
    #[init(val = Color::from_rgba(0.4431, 0.4471, 0.3647, 1.0))]
    pub grass_color_6: Color,

    // 5 has_grass toggles (slots 2-6; slot 1 always has grass)
    #[export]
    #[init(val = true)]
    pub has_grass_2: bool,
    #[export]
    #[init(val = true)]
    pub has_grass_3: bool,
    #[export]
    #[init(val = true)]
    pub has_grass_4: bool,
    #[export]
    #[init(val = true)]
    pub has_grass_5: bool,
    #[export]
    #[init(val = true)]
    pub has_grass_6: bool,
}
```

**What's happening:**

`PixyTextureList` is a flat data bag with 56 exported fields across 5 groups:

| Group | Count | Type | Purpose |
|---|---|---|---|
| Textures | 15 | `Option<Gd<Texture2D>>` | Ground textures for slots 1-15 |
| Scales | 15 | `f32` (default 1.0) | UV scale per texture slot |
| Grass sprites | 6 | `Option<Gd<Texture2D>>` | Sprite sheets for grass blades |
| Grass colors | 6 | `Color` | Ground albedo tints per slot |
| Has grass | 5 | `bool` (default true) | Per-slot grass enable (slots 2-6) |

Slot 1 always has grass (no toggle needed), which is why there are only 5 `has_grass_*` fields for 6 texture slots.

The texture fields use `Option<Gd<Texture2D>>` rather than bare `Gd<Texture2D>`. In gdext, `Option<Gd<T>>` maps to Godot's nullable resource reference — the inspector shows a "None" placeholder that artists can drag a texture onto. Without the `Option`, the field would need a non-null value at construction time, which isn't possible for textures.

The grass color defaults match the terrain's ground color defaults from Part 09. These are artist-chosen earth tones — muted greens and olive shades that work well for pixel art terrain.

### Step 3: Define `PixyTexturePreset` resource and implement `save_from_terrain()`

**Why:** `PixyTextureList` holds the data, but needs a wrapper that knows how to populate itself from a terrain and restore itself back. `PixyTexturePreset` is that wrapper — it contains a `PixyTextureList` plus a user-friendly name, and provides the bidirectional sync methods.

**File:** `rust/src/texture_preset.rs` (append after `PixyTextureList`)

```rust
/// Container resource for saving/loading a complete texture preset.
/// Port of Yugen's MarchingSquaresTexturePreset.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyTexturePreset {
    base: Base<Resource>,

    #[export]
    #[init(val = GString::from("New Preset"))]
    pub preset_name: GString,

    #[export]
    pub textures: Option<Gd<PixyTextureList>>,
}

#[godot_api]
impl PixyTexturePreset {
    /// Save the current terrain texture settings into this preset.
    #[func]
    pub fn save_from_terrain(&mut self, terrain: Gd<PixyTerrain>) {
        let t = terrain.bind();

        let mut list = if let Some(ref existing) = self.textures {
            existing.clone()
        } else {
            Gd::<PixyTextureList>::default()
        };

        {
            let mut l = list.bind_mut();

            // Textures
            l.texture_1 = t.ground_texture.clone();
            l.texture_2 = t.texture_2.clone();
            l.texture_3 = t.texture_3.clone();
            l.texture_4 = t.texture_4.clone();
            l.texture_5 = t.texture_5.clone();
            l.texture_6 = t.texture_6.clone();
            l.texture_7 = t.texture_7.clone();
            l.texture_8 = t.texture_8.clone();
            l.texture_9 = t.texture_9.clone();
            l.texture_10 = t.texture_10.clone();
            l.texture_11 = t.texture_11.clone();
            l.texture_12 = t.texture_12.clone();
            l.texture_13 = t.texture_13.clone();
            l.texture_14 = t.texture_14.clone();
            l.texture_15 = t.texture_15.clone();

            // Scales
            l.scale_1 = t.texture_scale_1;
            l.scale_2 = t.texture_scale_2;
            l.scale_3 = t.texture_scale_3;
            l.scale_4 = t.texture_scale_4;
            l.scale_5 = t.texture_scale_5;
            l.scale_6 = t.texture_scale_6;
            l.scale_7 = t.texture_scale_7;
            l.scale_8 = t.texture_scale_8;
            l.scale_9 = t.texture_scale_9;
            l.scale_10 = t.texture_scale_10;
            l.scale_11 = t.texture_scale_11;
            l.scale_12 = t.texture_scale_12;
            l.scale_13 = t.texture_scale_13;
            l.scale_14 = t.texture_scale_14;
            l.scale_15 = t.texture_scale_15;

            // Grass sprites
            l.grass_sprite_1 = t.grass_sprite.clone();
            l.grass_sprite_2 = t.grass_sprite_tex_2.clone();
            l.grass_sprite_3 = t.grass_sprite_tex_3.clone();
            l.grass_sprite_4 = t.grass_sprite_tex_4.clone();
            l.grass_sprite_5 = t.grass_sprite_tex_5.clone();
            l.grass_sprite_6 = t.grass_sprite_tex_6.clone();

            // Ground colors
            l.grass_color_1 = t.ground_color;
            l.grass_color_2 = t.ground_color_2;
            l.grass_color_3 = t.ground_color_3;
            l.grass_color_4 = t.ground_color_4;
            l.grass_color_5 = t.ground_color_5;
            l.grass_color_6 = t.ground_color_6;

            // Has grass
            l.has_grass_2 = t.tex2_has_grass;
            l.has_grass_3 = t.tex3_has_grass;
            l.has_grass_4 = t.tex4_has_grass;
            l.has_grass_5 = t.tex5_has_grass;
            l.has_grass_6 = t.tex6_has_grass;
        }

        self.textures = Some(list);
    }
```

**What's happening:**

`save_from_terrain()` takes a `Gd<PixyTerrain>` — an owned smart pointer to the terrain node. The method reads every texture-related field from the terrain and writes it into the texture list.

The borrow pattern here is worth studying:

1. `terrain.bind()` returns an immutable borrow of the `PixyTerrain` inner data. This is stored in `t`.
2. If the preset already has a texture list, clone the `Gd` handle. Otherwise, create a new `PixyTextureList` with `Gd::<PixyTextureList>::default()`. The `default()` call works because `PixyTextureList` has `init` in its class attribute, which generates a `Default` impl.
3. `list.bind_mut()` borrows the texture list mutably. Now we have two simultaneous borrows: `t` (immutable, into terrain) and `l` (mutable, into list). This is legal because they're borrowing into *different* `Gd` objects — Rust's borrow rules apply per-object, not globally.
4. Copy all 56 fields. Textures use `.clone()` because `Option<Gd<T>>` clones the smart pointer (a reference count bump, not a deep copy of the texture data). Scalars (`f32`, `bool`, `Color`) are `Copy` types.
5. The inner block `{ let mut l = ... }` ensures `l` is dropped before `self.textures = Some(list)` — we can't set the field while it's still borrowed.

Note the field name mismatches between terrain and list. The terrain uses `ground_texture` for slot 1, but the list uses `texture_1`. The terrain uses `grass_sprite` for sprite 1, but the list uses `grass_sprite_1`. These naming differences come from the original GDScript — the first slot in each group had a "special" name. The list normalizes them with consistent numbering.

### Step 4: Implement `load_into_terrain()`

**Why:** The inverse of save: read all values from the texture list and write them into the terrain. After writing, call `force_batch_update()` to push the new values to the shader.

**File:** `rust/src/texture_preset.rs` (continue in the `#[godot_api] impl PixyTexturePreset` block)

```rust
    /// Load this preset's texture settings into the terrain.
    #[func]
    pub fn load_into_terrain(&self, terrain: Gd<PixyTerrain>) {
        let Some(ref list_gd) = self.textures else {
            godot_warn!("PixyTexturePreset: no texture list to load");
            return;
        };
        let l = list_gd.bind();
        let mut terrain = terrain;
        let mut t = terrain.bind_mut();

        // Textures
        t.ground_texture = l.texture_1.clone();
        t.texture_2 = l.texture_2.clone();
        t.texture_3 = l.texture_3.clone();
        t.texture_4 = l.texture_4.clone();
        t.texture_5 = l.texture_5.clone();
        t.texture_6 = l.texture_6.clone();
        t.texture_7 = l.texture_7.clone();
        t.texture_8 = l.texture_8.clone();
        t.texture_9 = l.texture_9.clone();
        t.texture_10 = l.texture_10.clone();
        t.texture_11 = l.texture_11.clone();
        t.texture_12 = l.texture_12.clone();
        t.texture_13 = l.texture_13.clone();
        t.texture_14 = l.texture_14.clone();
        t.texture_15 = l.texture_15.clone();

        // Scales
        t.texture_scale_1 = l.scale_1;
        t.texture_scale_2 = l.scale_2;
        t.texture_scale_3 = l.scale_3;
        t.texture_scale_4 = l.scale_4;
        t.texture_scale_5 = l.scale_5;
        t.texture_scale_6 = l.scale_6;
        t.texture_scale_7 = l.scale_7;
        t.texture_scale_8 = l.scale_8;
        t.texture_scale_9 = l.scale_9;
        t.texture_scale_10 = l.scale_10;
        t.texture_scale_11 = l.scale_11;
        t.texture_scale_12 = l.scale_12;
        t.texture_scale_13 = l.scale_13;
        t.texture_scale_14 = l.scale_14;
        t.texture_scale_15 = l.scale_15;

        // Grass sprites
        t.grass_sprite = l.grass_sprite_1.clone();
        t.grass_sprite_tex_2 = l.grass_sprite_2.clone();
        t.grass_sprite_tex_3 = l.grass_sprite_3.clone();
        t.grass_sprite_tex_4 = l.grass_sprite_4.clone();
        t.grass_sprite_tex_5 = l.grass_sprite_5.clone();
        t.grass_sprite_tex_6 = l.grass_sprite_6.clone();

        // Ground colors
        t.ground_color = l.grass_color_1;
        t.ground_color_2 = l.grass_color_2;
        t.ground_color_3 = l.grass_color_3;
        t.ground_color_4 = l.grass_color_4;
        t.ground_color_5 = l.grass_color_5;
        t.ground_color_6 = l.grass_color_6;

        // Has grass
        t.tex2_has_grass = l.has_grass_2;
        t.tex3_has_grass = l.has_grass_3;
        t.tex4_has_grass = l.has_grass_4;
        t.tex5_has_grass = l.has_grass_5;
        t.tex6_has_grass = l.has_grass_6;

        // Sync shader
        t.force_batch_update();
    }
}
```

**What's happening:**

The borrow pattern in `load_into_terrain()` differs from `save_from_terrain()` in a subtle but important way.

`save_from_terrain()` takes `Gd<PixyTerrain>` by value and binds it immutably (`terrain.bind()`). The preset owns `&mut self`, the terrain is read-only. No conflict.

`load_into_terrain()` takes `&self` (immutable preset) and `Gd<PixyTerrain>` by value. It needs to:
1. Read from `self.textures` (immutable borrow of self).
2. Write to the terrain (mutable borrow of the Gd handle).
3. Call `t.force_batch_update()` (mutable method on the terrain bind).

The `let mut terrain = terrain;` line rebinds the parameter as mutable — `Gd<T>` must be `mut` to call `bind_mut()`. Then `terrain.bind_mut()` gives us `t`, a mutable reference to the terrain's inner fields.

Both `l` (immutable list bind) and `t` (mutable terrain bind) are alive simultaneously. This works because they borrow different `Gd` objects. The list is inside `self.textures` (part of the preset), while the terrain is a separate argument. Rust's borrow checker treats them as independent borrows.

The `force_batch_update()` call at the end is critical. Without it, the terrain's Rust fields would have new values, but the GPU shader would still be using the old uniform values. `force_batch_update()` iterates all shader uniform names and pushes the current field values — this was implemented in Part 09, Step 6.

The early return with `godot_warn!` handles the case where the preset has no texture list (the artist created a preset but never saved terrain settings into it). `godot_warn!` prints to Godot's output panel without crashing.

## Verify

```bash
cd rust && cargo build
```

Both files should compile without errors. You can verify the new types are registered in Godot:

1. Open the Godot editor with the project.
2. In the FileSystem dock, right-click and select "New Resource...".
3. Search for "PixyQuickPaint" — it should appear as a creatable resource type.
4. Search for "PixyTexturePreset" — it should also appear.
5. Search for "PixyTextureList" — this appears as well, though it's typically created automatically by the preset.

You can also test the preset workflow:
1. Select the `PixyTerrain` node in the scene.
2. In the inspector, assign textures to a few slots.
3. Create a `PixyTexturePreset` resource and assign it to the terrain's `current_texture_preset` slot.
4. Call `save_to_preset()` from Part 10 — the preset now contains your texture configuration.
5. Change the terrain's textures to something different.
6. Call `load_from_preset()` — the original textures are restored.

## What You Learned

- **Resource vs Node**: Resources extend `Resource` (not `Node`), making them serializable data objects that live outside the scene tree. They can be saved as `.tres` files, embedded in other resources, and shared across scenes.
- **`Gd::<T>::default()` for RefCounted types**: Resources are reference-counted in Godot. `default()` creates a new instance with the `init` defaults — equivalent to `new()` in GDScript.
- **`texture_index_to_colors()` bridge**: Converts an integer texture slot index (0-15) into the vertex color pair that the shader decodes back to a texture selection. This bridges the artist's mental model (slot numbers) with the renderer's encoding (color channels).
- **Bidirectional Gd borrow pattern**: `save_from_terrain` binds the terrain immutably and the list mutably. `load_into_terrain` binds the list immutably and the terrain mutably. Both work because the borrows target different `Gd` objects — Rust tracks borrows per-object, not globally.
- **Cloning `Gd<T>` vs cloning data**: `.clone()` on `Option<Gd<Texture2D>>` increments the reference count on the Godot object. It does NOT duplicate the texture data. Both the terrain and the preset point to the same underlying texture resource in memory.

## Stubs Introduced

- None

## Stubs Resolved

- [x] `quick_paint` module (empty) — introduced in Part 01, now full `PixyQuickPaint` resource implementation
- [x] `PixyTextureList` / `PixyTexturePreset` stub — introduced in Part 09, now full implementation with `save_from_terrain()` and `load_into_terrain()`
