# Part 13 — Terrain Grass Integration

**Series:** Reconstructing Pixy Terrain
**Part:** 13 of 18
**Previous:** 2026-02-06-grass-planter-multimesh-placement-12.md
**Status:** Complete

## What We're Building

The grass integration layer inside `terrain.rs` -- the methods that create, configure, and push data into the grass planter system from Part 12. Parts 09-10 introduced these methods alongside terrain materials and chunk operations, but treated them as supporting code. This part revisits them as a cohesive grass subsystem, explaining why each piece exists and how it connects to `PixyGrassPlanter`.

Specifically:
1. **`ensure_grass_material()`** -- creates a shared `ShaderMaterial` + `QuadMesh` for all grass planters.
2. **`force_grass_material_update()`** -- re-pushes all grass shader uniforms when settings change.
3. **`make_grass_config()`** -- builds a `GrassConfig` snapshot for chunk initialization.
4. **`regenerate_all_grass()`** -- triggers grass re-placement across all chunks.
5. **Grass data in preset save/load** -- how grass sprites, colors, and toggles survive the preset round-trip.

## What You'll Have After This

A complete understanding of how the terrain manager orchestrates grass. Every grass-related export field on `PixyTerrain` now has a clear path through the system: field -> shader parameter (via material update) -> planter config (via `GrassConfig`) -> actual MultiMesh instances (via chunk's `regenerate_mesh()`).

## Prerequisites

- Part 09 completed (`PixyTerrain` struct with all exports, `ensure_terrain_material()`, `force_batch_update()`)
- Part 10 completed (chunk operations, `add_chunk_internal()`, preset save/load)
- Part 11 completed (`PixyTexturePreset` and `PixyTextureList` resource types)
- Part 12 completed (`PixyGrassPlanter`, `GrassConfig` struct, MultiMesh placement algorithm)

## The Problem: Why Grass Needs a Manager-Level Integration

Grass in Yugen's system is not a standalone feature. Every grass blade depends on:

- The terrain shader's merge mode (rounded modes shift the wall threshold).
- The ground texture assigned to each slot (determines per-blade color sampling).
- The grass sprite assigned to each slot (determines blade shape).
- The ground color per slot (fallback tint when no texture is assigned).
- The `tex_has_grass` toggles (controls which texture slots grow grass at all).
- Wind, shading, and animation parameters (shared by all planters).

All of these live as `#[export]` fields on `PixyTerrain`. The grass planter (Part 12) can't read them directly -- doing so would require binding the terrain node from inside a chunk child, creating borrow conflicts. Instead, the terrain pushes data into planters through two channels:

1. **Shared ShaderMaterial** -- one `Gd<ShaderMaterial>` that all planters reference. When terrain updates a uniform, every planter sees it immediately.
2. **GrassConfig snapshot** -- a plain Rust struct cloned into each chunk at initialization time. Contains everything the planter needs for placement decisions.

This part covers the terrain-side code that feeds both channels.

## Steps

### Step 1: The `GrassConfig` struct (defined in `grass_planter.rs`, consumed here)

**Why:** The grass planter needs ~20 terrain fields to decide where and how to place grass. Rather than passing 20 individual parameters, we bundle them into a single `Clone`-able struct. This struct is defined in `grass_planter.rs` (Part 12) but constructed in `terrain.rs`. Understanding its shape is essential for the factory method in Step 4.

**File:** `rust/src/grass_planter.rs` (already exists from Part 12 -- review only, do not duplicate)

```rust
/// Cached grass configuration (avoids needing to bind terrain during grass operations).
/// Passed from terrain to chunk to grass planter at initialization time.
#[derive(Clone)]
pub struct GrassConfig {
    pub dimensions: Vector3i,
    pub subdivisions: i32,
    pub grass_size: Vector2,
    pub cell_size: Vector2,
    pub wall_threshold: f32,
    pub merge_mode: i32,
    pub animation_fps: i32,
    pub ledge_threshold: f32,
    pub ridge_threshold: f32,
    pub grass_sprites: [Option<Gd<Texture2D>>; 6],
    pub ground_colors: [Color; 6],
    pub tex_has_grass: [bool; 5],
    pub grass_mesh: Option<Gd<Mesh>>,
    /// Shared grass ShaderMaterial from terrain (one instance for all planters).
    pub grass_material: Option<Gd<ShaderMaterial>>,
    /// Shared QuadMesh with grass ShaderMaterial already applied (from terrain).
    /// When set, planters use this mesh directly instead of creating their own material.
    pub grass_quad_mesh: Option<Gd<Mesh>>,
    pub ground_images: [Option<Gd<Image>>; 6],
    pub texture_scales: [f32; 6],
}
```

**What's happening:**

The struct captures three categories of data:

| Category | Fields | Used For |
|---|---|---|
| Geometry | `dimensions`, `cell_size`, `subdivisions`, `grass_size` | Instance count calculation, sample point generation |
| Placement rules | `wall_threshold`, `merge_mode`, `ledge_threshold`, `ridge_threshold`, `tex_has_grass` | Deciding which triangles receive grass |
| Rendering | `grass_sprites`, `ground_colors`, `grass_material`, `grass_quad_mesh`, `ground_images`, `texture_scales`, `animation_fps` | Visual appearance of placed grass |

The `tex_has_grass` array has 5 entries (slots 2-6), not 6. Slot 1 (the base texture) always has grass -- there is no toggle for it. This matches Yugen's design: the base ground texture is assumed to be a grass-compatible surface.

The `ground_images` array holds CPU-readable `Gd<Image>` copies of the ground textures. These are expensive to create (requires `get_image()` + `decompress()`), so they're extracted once when building the config and reused for all cells in all chunks.

The `grass_quad_mesh` is stored as `Option<Gd<Mesh>>` (the base `Mesh` type), not `Option<Gd<QuadMesh>>`. This is because the planter's `MultiMesh::set_mesh()` accepts `&Gd<Mesh>`, and `QuadMesh` must be upcast. The upcast happens in the factory method.

### Step 2: Implement `ensure_grass_material()`

**Why:** All grass planters across all chunks must share a single `ShaderMaterial` instance. If each planter created its own material, changing a grass parameter would require iterating every planter to update every material. With a shared material, one `set_shader_parameter()` call propagates to all planters instantly -- Godot's rendering backend sees the same GPU resource everywhere.

The method also creates a shared `QuadMesh` with the material pre-applied. This mesh becomes the template for every planter's `MultiMesh`.

**File:** `rust/src/terrain.rs` (add to the `impl PixyTerrain` block -- non-godot_api, after `ensure_terrain_material()`)

```rust
    /// Ensure shared grass material and QuadMesh exist.
    /// Creates ONE ShaderMaterial + ONE QuadMesh that all chunk planters share.
    pub fn ensure_grass_material(&mut self) {
        if self.grass_material.is_some() {
            return;
        }

        let mut loader = ResourceLoader::singleton();

        let shader_path = "res://resources/shaders/mst_grass.gdshader";
        if !loader.exists(shader_path) {
            godot_warn!("PixyTerrain: Grass shader not found at {}", shader_path);
            return;
        }

        let Some(res) = loader.load(shader_path) else {
            godot_warn!("PixyTerrain: Failed to load grass shader");
            return;
        };

        let Ok(shader) = res.try_cast::<Shader>() else {
            godot_warn!("PixyTerrain: Resource is not a Shader");
            return;
        };

        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);
        self.grass_material = Some(mat.clone());

        // Create shared QuadMesh with the material applied.
        // Use set_material() (PrimitiveMesh property) to match Yugen's mst_grass_mesh.tres,
        // plus surface_set_material(0) as belt-and-suspenders.
        let mut quad = QuadMesh::new_gd();
        quad.set_size(self.grass_size);
        quad.set_center_offset(Vector3::new(0.0, self.grass_size.y / 2.0, 0.0));
        quad.set_material(&mat);
        quad.surface_set_material(0, &mat);
        self.grass_quad_mesh = Some(quad);

        godot_print!("PixyTerrain: Created shared grass material and mesh");
    }
```

**What's happening:**

The method is idempotent -- the `if self.grass_material.is_some()` guard means it's safe to call from `_deferred_enter_tree()`, `regenerate()`, and anywhere else without creating duplicate materials.

The shader loading follows the same three-step pattern as `ensure_terrain_material()`: check existence, load resource, try-cast to `Shader`. Each step can fail independently (file missing, load error, wrong resource type), so each has its own early return with a warning.

The `mat.clone()` when storing `self.grass_material = Some(mat.clone())` is a `Gd` clone -- it copies the smart pointer, not the underlying Godot object. Both `self.grass_material` and the `mat` local variable point to the same `ShaderMaterial` in Godot's memory. This is important: the same material object is then applied to the `QuadMesh`, creating a single shared rendering resource.

**The QuadMesh setup:**

```rust
quad.set_size(self.grass_size);
quad.set_center_offset(Vector3::new(0.0, self.grass_size.y / 2.0, 0.0));
```

`set_center_offset` shifts the mesh's pivot point to its bottom edge. Without this, the quad would be centered at each grass instance's transform origin, meaning half the blade would be underground. The Y offset of `grass_size.y / 2.0` ensures the blade extends upward from the ground surface.

**Belt-and-suspenders material application:**

```rust
quad.set_material(&mat);
quad.surface_set_material(0, &mat);
```

Two material assignment paths exist in Godot:
- `set_material()` on `PrimitiveMesh` sets the material as a base property.
- `surface_set_material(0, &mat)` sets the material on surface index 0 directly.

Different code paths in Godot's rendering pipeline may check one or the other. Setting both ensures the grass material is always active regardless of how the mesh is accessed (directly or through `MultiMesh`). This matches Yugen's `mst_grass_mesh.tres` resource configuration.

### Step 3: Implement `force_grass_material_update()`

**Why:** When an artist changes any grass-related setting in the editor (sprite textures, wind parameters, grass colors, has_grass toggles), the shared grass `ShaderMaterial` must be updated. This method reads all grass-related fields from `PixyTerrain` and pushes them as shader uniform values.

Unlike `ensure_grass_material()` which creates the material once, this method re-pushes all parameters to an existing material. It's called during initialization and whenever the editor UI modifies a grass setting.

**File:** `rust/src/terrain.rs` (add to the `impl PixyTerrain` block -- non-godot_api)

```rust
    /// Sync all grass shader parameters from terrain fields to the shared grass material.
    /// Mirrors Yugen's force_batch_update() grass section (lines 612-641).
    pub fn force_grass_material_update(&mut self) {
        if self.grass_material.is_none() {
            return;
        }

        // Collect ALL values BEFORE borrowing grass_material mutably
        let is_merge_round = matches!(
            MergeMode::from_index(self.merge_mode),
            MergeMode::RoundedPolyhedron | MergeMode::SemiRound | MergeMode::Spherical
        );
        let wall_threshold = self.wall_threshold;
        let animation_fps = self.animation_fps as f32;

        // Use get_grass_sprite_or_default() to load grass_leaf_sprite.png as fallback
        // when no custom sprite is assigned. Without this, the shader samples an unbound
        // sampler2D -> returns white (alpha=1.0) -> full quad is visible as a square.
        let sprites = [
            self.get_grass_sprite_or_default(0),
            self.get_grass_sprite_or_default(1),
            self.get_grass_sprite_or_default(2),
            self.get_grass_sprite_or_default(3),
            self.get_grass_sprite_or_default(4),
            self.get_grass_sprite_or_default(5),
        ];

        let ground_colors = [
            self.ground_color,
            self.ground_color_2,
            self.ground_color_3,
            self.ground_color_4,
            self.ground_color_5,
            self.ground_color_6,
        ];

        let use_base_color = [
            self.ground_texture.is_none(),
            self.texture_2.is_none(),
            self.texture_3.is_none(),
            self.texture_4.is_none(),
            self.texture_5.is_none(),
            self.texture_6.is_none(),
        ];

        let tex_has_grass = [
            self.tex2_has_grass,
            self.tex3_has_grass,
            self.tex4_has_grass,
            self.tex5_has_grass,
            self.tex6_has_grass,
        ];

        let shadow_color = self.shadow_color;
        let shadow_bands = self.shadow_bands;
        let shadow_intensity = self.shadow_intensity;
        let grass_size = self.grass_size;
        let wind_direction = self.wind_direction;
        let wind_scale = self.wind_scale;
        let wind_speed = self.wind_speed;

        // Now borrow grass_material mutably
        let mat = self.grass_material.as_mut().unwrap();

        // Core parameters
        mat.set_shader_parameter("is_merge_round", &is_merge_round.to_variant());
        mat.set_shader_parameter("wall_threshold", &wall_threshold.to_variant());
        mat.set_shader_parameter("fps", &animation_fps.to_variant());

        // Grass textures -- always set (fallback provides blade-shaped alpha cutout)
        let texture_names = [
            "grass_texture",
            "grass_texture_2",
            "grass_texture_3",
            "grass_texture_4",
            "grass_texture_5",
            "grass_texture_6",
        ];
        for (i, name) in texture_names.iter().enumerate() {
            if let Some(ref tex) = sprites[i] {
                mat.set_shader_parameter(*name, &tex.to_variant());
            }
        }

        // Grass base colors
        mat.set_shader_parameter("grass_base_color", &ground_colors[0].to_variant());
        let color_names = [
            "grass_color_2",
            "grass_color_3",
            "grass_color_4",
            "grass_color_5",
            "grass_color_6",
        ];
        for (i, name) in color_names.iter().enumerate() {
            mat.set_shader_parameter(*name, &ground_colors[i + 1].to_variant());
        }

        // use_base_color flags -- driven by ground TEXTURE (not grass sprite), matching Yugen
        mat.set_shader_parameter("use_base_color", &use_base_color[0].to_variant());
        mat.set_shader_parameter("use_base_color_2", &use_base_color[1].to_variant());
        mat.set_shader_parameter("use_base_color_3", &use_base_color[2].to_variant());
        mat.set_shader_parameter("use_base_color_4", &use_base_color[3].to_variant());
        mat.set_shader_parameter("use_base_color_5", &use_base_color[4].to_variant());
        mat.set_shader_parameter("use_base_color_6", &use_base_color[5].to_variant());

        // use_grass_tex_* flags (tex2-6 has_grass toggles)
        mat.set_shader_parameter("use_grass_tex_2", &tex_has_grass[0].to_variant());
        mat.set_shader_parameter("use_grass_tex_3", &tex_has_grass[1].to_variant());
        mat.set_shader_parameter("use_grass_tex_4", &tex_has_grass[2].to_variant());
        mat.set_shader_parameter("use_grass_tex_5", &tex_has_grass[3].to_variant());
        mat.set_shader_parameter("use_grass_tex_6", &tex_has_grass[4].to_variant());

        // Wind animation parameters
        mat.set_shader_parameter("wind_direction", &wind_direction.to_variant());
        mat.set_shader_parameter("wind_scale", &wind_scale.to_variant());
        mat.set_shader_parameter("wind_speed", &wind_speed.to_variant());
        mat.set_shader_parameter("animate_active", &true.to_variant());

        // Load wind noise texture
        let mut loader = ResourceLoader::singleton();
        let wind_path = "res://resources/textures/wind_noise_texture.tres";
        if loader.exists(wind_path) {
            if let Some(wind_tex) = loader.load(wind_path) {
                mat.set_shader_parameter("wind_texture", &wind_tex.to_variant());
            }
        }

        // Shading parameters
        mat.set_shader_parameter("shadow_color", &shadow_color.to_variant());
        mat.set_shader_parameter("bands", &shadow_bands.to_variant());
        mat.set_shader_parameter("shadow_intensity", &shadow_intensity.to_variant());

        // Update quad mesh size if it changed
        if let Some(ref mut quad) = self.grass_quad_mesh {
            quad.set_size(grass_size);
            quad.set_center_offset(Vector3::new(0.0, grass_size.y / 2.0, 0.0));
        }
    }
```

**What's happening:**

This method sets roughly 30 shader uniforms across 6 parameter groups. Each group has a specific purpose in the grass shader pipeline:

**Group 1 -- Core parameters (3 uniforms):**

```rust
mat.set_shader_parameter("is_merge_round", &is_merge_round.to_variant());
mat.set_shader_parameter("wall_threshold", &wall_threshold.to_variant());
mat.set_shader_parameter("fps", &animation_fps.to_variant());
```

- `is_merge_round`: The grass shader uses this to adjust its vertex displacement formula. Rounded merge modes (RoundedPolyhedron, SemiRound, Spherical) curve the wall-floor transition, so the threshold where grass stops growing needs to shift. This boolean is cheaper than passing the full merge mode index.
- `wall_threshold`: Height below which a surface is considered "wall" rather than "floor." The shader uses this to fade out grass blades near wall transitions.
- `fps`: Animation frame rate for sprite-sheet grass. When 0, sprites are static. When positive, the shader cycles through sprite frames.

**Group 2 -- Grass textures (6 uniforms):**

Each texture slot maps to a grass sprite. The `get_grass_sprite_or_default()` helper ensures every slot has a texture -- the fallback is `grass_leaf_sprite.png`. Without the fallback, unbound `sampler2D` uniforms in GLSL return `vec4(1.0)` (opaque white), which renders as solid white rectangles instead of blade-shaped sprites.

**Group 3 -- Grass colors (6 uniforms):**

The first slot uses `grass_base_color`, and slots 2-6 use `grass_color_2` through `grass_color_6`. This naming asymmetry comes from Yugen's GDScript shader and must be matched exactly. These colors tint the grass blades when `use_base_color` is true (no ground texture assigned).

**Group 4 -- `use_base_color` flags (6 uniforms):**

A subtle but critical detail: the `use_base_color` flag for each slot is driven by whether the *ground texture* is assigned, not the grass sprite:

```rust
let use_base_color = [
    self.ground_texture.is_none(),
    self.texture_2.is_none(),
    // ...
];
```

When a ground texture exists, the grass shader samples it for color variation. When no ground texture exists, the shader falls back to the flat `grass_base_color` / `grass_color_*` tint. This matches Yugen's behavior: ground textures and grass share the same color source.

**Group 5 -- `use_grass_tex` flags (5 uniforms):**

These are the per-texture "does this texture slot grow grass?" toggles. Only 5 flags for slots 2-6 because slot 1 always has grass. The grass planter uses these during placement (deciding whether to emit an instance), and the shader uses them to discard fragments on non-grass textures.

**Group 6 -- Wind and shading (7 uniforms):**

Wind animation uses a scrolling noise texture (`wind_noise_texture.tres`) sampled at world-space UV coordinates. The `wind_direction`, `wind_scale`, and `wind_speed` control the scroll direction, noise frequency, and scroll speed respectively. `animate_active` is always `true` -- there is no editor toggle to disable wind entirely, matching Yugen.

The shading parameters (`shadow_color`, `bands`, `shadow_intensity`) are shared with the terrain shader. Grass receives the same cel-shading treatment as the ground surface, ensuring visual consistency.

**The borrow collector pattern:**

The method begins by collecting ~20 field values into local variables before the line:

```rust
let mat = self.grass_material.as_mut().unwrap();
```

This pattern is mandatory in Rust. The helper methods like `get_grass_sprite_or_default()` borrow `self` immutably (they read fields and call `ResourceLoader`). The `as_mut().unwrap()` borrows `self` mutably. Rust forbids overlapping immutable and mutable borrows. By collecting all values first, the immutable borrows are dropped before the mutable borrow begins. This is the same pattern used by `force_batch_update()` for the terrain shader.

**QuadMesh size sync:**

The method ends by updating the shared `QuadMesh` size:

```rust
if let Some(ref mut quad) = self.grass_quad_mesh {
    quad.set_size(grass_size);
    quad.set_center_offset(Vector3::new(0.0, grass_size.y / 2.0, 0.0));
}
```

If the artist changes `grass_size` in the inspector, the QuadMesh must be updated to match. Since all planters share this mesh, the size change propagates everywhere. The center offset recalculation keeps blades rooted at the ground.

### Step 4: Implement `make_grass_config()`

**Why:** The grass planter cannot bind the terrain node at placement time -- doing so would create a circular borrow (terrain -> chunk -> planter -> terrain). Instead, the terrain creates a `GrassConfig` snapshot and passes it to each chunk during initialization. The chunk then forwards it to its planter. This breaks the dependency cycle.

`make_grass_config()` is a factory method that reads all grass-relevant fields from `PixyTerrain` and assembles them into a `GrassConfig` struct.

**File:** `rust/src/terrain.rs` (add to the `impl PixyTerrain` block -- non-godot_api, after `make_terrain_config()`)

```rust
    /// Create a GrassConfig from current terrain settings.
    /// This is called before chunk operations to avoid needing to bind terrain later.
    fn make_grass_config(&self) -> GrassConfig {
        GrassConfig {
            dimensions: self.dimensions,
            subdivisions: self.grass_subdivisions,
            grass_size: self.grass_size,
            cell_size: self.cell_size,
            wall_threshold: self.wall_threshold,
            merge_mode: self.merge_mode,
            animation_fps: self.animation_fps,
            ledge_threshold: self.ledge_threshold,
            ridge_threshold: self.ridge_threshold,
            grass_sprites: [
                self.get_grass_sprite_or_default(0),
                self.get_grass_sprite_or_default(1),
                self.get_grass_sprite_or_default(2),
                self.get_grass_sprite_or_default(3),
                self.get_grass_sprite_or_default(4),
                self.get_grass_sprite_or_default(5),
            ],
            ground_colors: [
                self.ground_color,
                self.ground_color_2,
                self.ground_color_3,
                self.ground_color_4,
                self.ground_color_5,
                self.ground_color_6,
            ],
            tex_has_grass: [
                self.tex2_has_grass,
                self.tex3_has_grass,
                self.tex4_has_grass,
                self.tex5_has_grass,
                self.tex6_has_grass,
            ],
            grass_mesh: self.grass_mesh.clone(),
            grass_material: self.grass_material.clone(),
            grass_quad_mesh: self.grass_quad_mesh.as_ref().map(|q| q.clone().upcast::<godot::classes::Mesh>()),
            ground_images: [
                self.extract_ground_image(0),
                self.extract_ground_image(1),
                self.extract_ground_image(2),
                self.extract_ground_image(3),
                self.extract_ground_image(4),
                self.extract_ground_image(5),
            ],
            texture_scales: [
                self.texture_scale_1,
                self.texture_scale_2,
                self.texture_scale_3,
                self.texture_scale_4,
                self.texture_scale_5,
                self.texture_scale_6,
            ],
        }
    }
```

**What's happening:**

The factory method reads from `self` (immutable borrow) and returns an owned `GrassConfig`. This is cheap for scalar fields (copied directly) but involves Godot object operations for textures and images.

**Sprite fallbacks via `get_grass_sprite_or_default()`:**

```rust
grass_sprites: [
    self.get_grass_sprite_or_default(0),
    // ...
],
```

Each call checks the corresponding `grass_sprite_*` export field. If `None`, it loads `grass_leaf_sprite.png` as the fallback. This helper was implemented in Part 09:

```rust
    fn get_grass_sprite_or_default(&self, index: usize) -> Option<Gd<Texture2D>> {
        let sprite = match index {
            0 => &self.grass_sprite,
            1 => &self.grass_sprite_tex_2,
            2 => &self.grass_sprite_tex_3,
            3 => &self.grass_sprite_tex_4,
            4 => &self.grass_sprite_tex_5,
            5 => &self.grass_sprite_tex_6,
            _ => return None,
        };

        if sprite.is_some() {
            return sprite.clone();
        }

        // Load default grass sprite for all slots if no custom texture assigned
        let mut loader = ResourceLoader::singleton();
        let path = "res://resources/textures/grass_leaf_sprite.png";
        if loader.exists(path) {
            return loader
                .load(path)
                .and_then(|r| r.try_cast::<Texture2D>().ok());
        }

        None
    }
```

The fallback is loaded each time `make_grass_config()` is called. This is acceptable because `ResourceLoader::load()` returns cached resources -- Godot does not re-read the file from disk on every call. The Gd handle is a new smart pointer to the same underlying resource.

**Ground image extraction via `extract_ground_image()`:**

```rust
ground_images: [
    self.extract_ground_image(0),
    // ...
],
```

This is the most expensive part of config creation. Each call:
1. Gets the ground texture (or default noise texture) for the slot.
2. Calls `get_image()` to get a CPU-side `Image` from the GPU texture.
3. Calls `decompress()` to ensure the image is in a readable format (DXT/ETC2 compressed textures cannot be sampled by `get_pixel()` without decompression first).

```rust
    fn extract_ground_image(&self, index: usize) -> Option<Gd<Image>> {
        let tex = self.get_ground_texture_or_default(index)?;
        let mut img = tex.get_image()?;
        // Decompress so get_pixel() works on compressed formats (e.g. DXT, ETC2)
        img.decompress();
        Some(img)
    }
```

The `?` operator chains two failure points: the texture might not exist (returns `None`), and `get_image()` might fail on certain texture types (also returns `None`). If either fails, the config slot is `None` and the grass planter will fall back to the flat ground color for that slot.

This extraction happens once per config creation, not once per grass blade. The resulting `Gd<Image>` objects are cloned into the `GrassConfig` and reused across all cells in all chunks.

**QuadMesh upcast:**

```rust
grass_quad_mesh: self.grass_quad_mesh.as_ref().map(|q| q.clone().upcast::<godot::classes::Mesh>()),
```

The terrain stores the quad mesh as `Option<Gd<QuadMesh>>` (the concrete type), but the `GrassConfig` stores it as `Option<Gd<Mesh>>` (the base type). The planter's `MultiMesh::set_mesh()` API accepts `&Gd<Mesh>`, not `&Gd<QuadMesh>`. The `.upcast::<godot::classes::Mesh>()` performs this conversion. In gdext, `upcast()` requires an explicit type parameter -- the compiler cannot infer the target type.

**The config is cloned, not moved:**

In `_deferred_enter_tree()` and `add_chunk_internal()`, the config is created once and then cloned for each chunk:

```rust
let grass_config = self.make_grass_config();
// ...
for key in chunk_keys {
    // ...
    chunk.bind_mut().initialize_terrain(
        true,
        noise.clone(),
        material.clone(),
        grass_config.clone(),  // Clone for each chunk
    );
}
```

The `Clone` on `GrassConfig` clones all `Gd` smart pointers (cheap pointer copies) and the scalar fields. The `Gd<Image>` objects inside `ground_images` are also pointer copies -- all chunks share the same underlying `Image` data in Godot's memory.

### Step 5: Implement `regenerate_all_grass()`

**Why:** After changing grass-related settings (sprites, colors, toggles, wind), the existing MultiMesh instances need to be rebuilt. `regenerate_all_grass()` iterates all chunks and triggers a full mesh regeneration on each. The mesh regeneration in turn triggers grass re-placement through the chunk's internal `regenerate_mesh()` -> planter `regenerate_all_cells_with_geometry()` pipeline.

**File:** `rust/src/terrain.rs` (add to the `#[godot_api] impl PixyTerrain` block)

```rust
    /// Regenerate grass on all chunks (call after changing grass-related settings).
    #[func]
    pub fn regenerate_all_grass(&mut self) {
        let chunk_keys: Vec<[i32; 2]> = self.chunks.keys().cloned().collect();
        for key in chunk_keys {
            if let Some(chunk) = self.chunks.get(&key) {
                let mut chunk = chunk.clone();
                chunk.bind_mut().regenerate_mesh();
            }
        }
    }
```

**What's happening:**

The method follows the standard chunk iteration pattern used throughout the terrain manager:

1. **Collect keys first**: `self.chunks.keys().cloned().collect()` creates a Vec of all chunk coordinates. This releases the immutable borrow on `self.chunks` before the loop begins.

2. **Clone the Gd handle**: `self.chunks.get(&key)` borrows the HashMap immutably. `.clone()` clones the `Gd<PixyTerrainChunk>` smart pointer (not the chunk data). This allows the subsequent `.bind_mut()` to take a mutable borrow on the chunk's inner data without conflicting with the HashMap borrow.

3. **Regenerate the full mesh**: `chunk.bind_mut().regenerate_mesh()` is a full mesh rebuild, not just grass. This is deliberate -- the chunk's `regenerate_mesh()` method already calls the grass planter at the end:

```rust
// Inside chunk.rs regenerate_mesh():
if let Some(ref mut planter) = self.grass_planter {
    planter
        .bind_mut()
        .regenerate_all_cells_with_geometry(&self.cell_geometry);
}
```

The grass planter receives the chunk's `cell_geometry` HashMap directly. This contains the cached `CellGeometry` for every cell -- vertices, UVs, colors, floor/wall flags. The planter uses this geometry to determine which triangles are floor surfaces, perform barycentric point-in-triangle tests, and place grass instances.

A potential optimization would be to regenerate only the grass (not the full mesh) when only grass settings change. However, the current approach is simpler and matches Yugen's GDScript behavior where mesh and grass are always regenerated together. The cost is acceptable because grass regeneration is typically triggered by editor UI changes (not per-frame operations).

### Step 6: Grass data in preset save/load

**Why:** Artists need to save and restore complete texture configurations, including grass sprites, grass colors, and per-texture grass toggles. The preset system stores these alongside ground textures and scales in a single `PixyTextureList` resource.

The grass-related fields in `save_to_preset()` and `load_from_preset()` were introduced in Part 10 as part of the full preset workflow. Here we examine the grass-specific portions.

**File:** `rust/src/terrain.rs` (within `save_to_preset()` in the `#[godot_api] impl PixyTerrain` block)

The grass data written during save:

```rust
            {
                let mut l = list_gd.bind_mut();
                // ... (texture and scale fields omitted for clarity) ...

                // Grass sprites (6 slots)
                l.grass_sprite_1 = self.grass_sprite.clone();
                l.grass_sprite_2 = self.grass_sprite_tex_2.clone();
                l.grass_sprite_3 = self.grass_sprite_tex_3.clone();
                l.grass_sprite_4 = self.grass_sprite_tex_4.clone();
                l.grass_sprite_5 = self.grass_sprite_tex_5.clone();
                l.grass_sprite_6 = self.grass_sprite_tex_6.clone();

                // Ground/grass colors (6 slots)
                l.grass_color_1 = self.ground_color;
                l.grass_color_2 = self.ground_color_2;
                l.grass_color_3 = self.ground_color_3;
                l.grass_color_4 = self.ground_color_4;
                l.grass_color_5 = self.ground_color_5;
                l.grass_color_6 = self.ground_color_6;

                // Per-texture grass toggles (slots 2-6)
                l.has_grass_2 = self.tex2_has_grass;
                l.has_grass_3 = self.tex3_has_grass;
                l.has_grass_4 = self.tex4_has_grass;
                l.has_grass_5 = self.tex5_has_grass;
                l.has_grass_6 = self.tex6_has_grass;
            }
```

The grass data read during load:

```rust
        let l = list_gd.bind();
        // ... (texture and scale fields omitted for clarity) ...

        // Grass sprites
        self.grass_sprite = l.grass_sprite_1.clone();
        self.grass_sprite_tex_2 = l.grass_sprite_2.clone();
        self.grass_sprite_tex_3 = l.grass_sprite_3.clone();
        self.grass_sprite_tex_4 = l.grass_sprite_4.clone();
        self.grass_sprite_tex_5 = l.grass_sprite_5.clone();
        self.grass_sprite_tex_6 = l.grass_sprite_6.clone();

        // Ground/grass colors
        self.ground_color = l.grass_color_1;
        self.ground_color_2 = l.grass_color_2;
        self.ground_color_3 = l.grass_color_3;
        self.ground_color_4 = l.grass_color_4;
        self.ground_color_5 = l.grass_color_5;
        self.ground_color_6 = l.grass_color_6;

        // Has grass toggles
        self.tex2_has_grass = l.has_grass_2;
        self.tex3_has_grass = l.has_grass_3;
        self.tex4_has_grass = l.has_grass_4;
        self.tex5_has_grass = l.has_grass_5;
        self.tex6_has_grass = l.has_grass_6;

        // Drop borrows before calling methods on self
        drop(l);
        drop(p);

        self.force_batch_update();
        self.force_grass_material_update();
```

**What's happening:**

Three categories of grass data are persisted in presets:

1. **Grass sprites** (`grass_sprite_1` through `grass_sprite_6`): These are `Option<Gd<Texture2D>>` -- the alpha-cutout textures that define blade shape. When saved to a `.tres` resource, Godot serializes the texture path. When loaded, the path is resolved back to the texture resource.

2. **Ground/grass colors** (`grass_color_1` through `grass_color_6`): Plain `Color` values (4 floats each). Note the naming: the preset calls them `grass_color_*` but they map to `ground_color_*` on the terrain. This is because the same colors serve double duty -- they tint the ground surface in the terrain shader AND tint grass blades in the grass shader. The naming follows Yugen's convention where the `PixyTextureList` resource uses the "grass" perspective.

3. **Has-grass toggles** (`has_grass_2` through `has_grass_6`): Booleans that control whether textures 2-6 grow grass. Slot 1 has no toggle (always grows grass).

The explicit `drop(l); drop(p);` before `self.force_batch_update()` is the borrow dance from Part 10. Without the drops, the immutable borrows on the preset data (through `l` and `p`) would be alive when `force_batch_update()` tries to borrow `self` mutably. Rust rejects this. The explicit drops release the borrows early.

After loading, both `force_batch_update()` (terrain shader sync) and `force_grass_material_update()` (grass shader sync) are called. This ensures the loaded grass settings take immediate visual effect without requiring a manual regeneration.

### Step 7: How the pieces connect -- the grass data flow

The complete data flow from terrain export field to rendered grass blade:

```
PixyTerrain fields (grass_sprite, ground_color, tex_has_grass, etc.)
    |
    +-- force_grass_material_update() --> shared ShaderMaterial uniforms
    |       (visual appearance: sprite cutout, colors, wind, shading)
    |
    +-- make_grass_config() --> GrassConfig struct
            |
            +-- passed to chunk.initialize_terrain()
                    |
                    +-- chunk stores grass_config, passes to planter.setup_with_config()
                            |
                            +-- planter creates MultiMesh
                            +-- planter applies shared material as material_override
                            +-- planter uses config for placement decisions
                                    |
                                    +-- regenerate_all_cells_with_geometry()
                                            |
                                            +-- per-cell barycentric placement
                                            +-- tex_has_grass check
                                            +-- ledge/ridge avoidance
                                            +-- ground image color sampling
```

Two parallel paths, two purposes:
- **Material path**: Controls how grass *looks* (shader uniforms).
- **Config path**: Controls where grass *grows* (placement rules).

The material path is instant -- change a uniform, all planters update immediately because they share the same `Gd<ShaderMaterial>`. The config path requires regeneration -- changing placement rules (like `tex_has_grass` toggles) requires calling `regenerate_all_grass()` to re-run the placement algorithm.

## Verify

```bash
cd rust && cargo build
```

This part adds no new code beyond what was introduced in Parts 09-10 and 12. The build should succeed without changes. If you are following the series in order, the grass integration layer was compiled incrementally across those earlier parts. This part provides the conceptual framework that ties them together.

## What You Learned

- **Shared material pattern**: One `ShaderMaterial` instance referenced by all grass planters. Uniform changes propagate instantly without iterating planters.
- **Config snapshot pattern**: `make_grass_config()` creates a plain Rust struct from terrain fields. This struct is cloned into each chunk, breaking the terrain-chunk-planter borrow cycle.
- **Fallback texture loading**: `get_grass_sprite_or_default()` ensures every sprite slot has a texture, preventing white-rectangle artifacts from unbound samplers.
- **Ground image extraction**: `extract_ground_image()` calls `get_image()` + `decompress()` once per config creation, providing CPU-readable textures for per-blade color sampling.
- **Borrow collector pattern**: Collect all field reads into local variables before mutably borrowing the material. Required by Rust's borrow checker when reading multiple fields and writing to one.
- **`use_base_color` driven by ground texture, not grass sprite**: This subtle distinction matches Yugen's design where ground textures and grass share the same color source.
- **Preset round-trip**: Grass sprites, colors, and toggles are saved/loaded alongside ground textures. The `drop()` dance releases preset borrows before calling shader update methods.

## Stubs Introduced

- None

## Stubs Resolved

- [x] Grass integration stubs from Part 09 -- all grass-related methods now have full context and explanation of their role in the grass pipeline
