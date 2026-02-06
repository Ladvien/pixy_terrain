# Pixy Terrain — Part 09: Terrain Manager — Materials & Settings

**Series:** Reconstructing Pixy Terrain
**Part:** 09 of 18
**Previous:** 2026-02-06-terrain-shaders-08.md
**Status:** Complete

## What We're Building

The `PixyTerrain` node — the top-level manager that owns the terrain material, grass material, and all export settings. This part covers the struct definition (50+ exports), shader material creation, and the `force_batch_update()` / `force_grass_material_update()` methods that sync Rust exports to shader uniforms.

## What You'll Have After This

A `PixyTerrain` (Node3D) that loads shaders, creates materials, and syncs all terrain/grass settings to the GPU. It can't manage chunks yet (Part 10), but the material pipeline is complete — the shader receives all its uniform values from Rust export fields.

## Prerequisites

- Part 08 completed (terrain and grass shaders exist in `godot/resources/shaders/`)
- Part 06-07 completed (`PixyTerrainChunk` and `TerrainConfig` exist in `chunk.rs`)

## Steps

### Step 1: Define constants and imports

**Why:** The terrain shader has 16 texture slots, 15 scale uniforms, and 6 ground color uniforms. Rather than building uniform name strings at runtime, we define them as compile-time constant arrays. This also ensures the Rust uniform names exactly match the shader uniform names.

**File:** `rust/src/terrain.rs` (replace the empty stub from Part 01)

```rust
use std::collections::HashMap;

use godot::classes::{
    Engine, Image, Node3D, QuadMesh, ResourceLoader, Shader, ShaderMaterial, Texture2D,
};
use godot::prelude::*;

use crate::chunk::{PixyTerrainChunk, TerrainConfig};
use crate::grass_planter::GrassConfig;
use crate::marching_squares::MergeMode;

/// Path to the terrain shader file.
const TERRAIN_SHADER_PATH: &str = "res://resources/shaders/mst_terrain.gdshader";

/// Path to the default ground noise texture.
const DEFAULT_GROUND_TEXTURE_PATH: &str = "res://resources/textures/default_ground_noise.tres";

/// Texture uniform names in the shader (16 slots, index 0-15).
const TEXTURE_UNIFORM_NAMES: [&str; 16] = [
    "vc_tex_rr",
    "vc_tex_rg",
    "vc_tex_rb",
    "vc_tex_ra",
    "vc_tex_gr",
    "vc_tex_gg",
    "vc_tex_gb",
    "vc_tex_ga",
    "vc_tex_br",
    "vc_tex_bg",
    "vc_tex_bb",
    "vc_tex_ba",
    "vc_tex_ar",
    "vc_tex_ag",
    "vc_tex_ab",
    "vc_tex_aa",
];

/// Ground albedo uniform names in the shader (6 slots matching texture slots 1-6).
const GROUND_ALBEDO_NAMES: [&str; 6] = [
    "ground_albedo",
    "ground_albedo_2",
    "ground_albedo_3",
    "ground_albedo_4",
    "ground_albedo_5",
    "ground_albedo_6",
];

/// Texture scale uniform names (15 slots, indices 1-15).
const TEXTURE_SCALE_NAMES: [&str; 15] = [
    "texture_scale_1",
    "texture_scale_2",
    "texture_scale_3",
    "texture_scale_4",
    "texture_scale_5",
    "texture_scale_6",
    "texture_scale_7",
    "texture_scale_8",
    "texture_scale_9",
    "texture_scale_10",
    "texture_scale_11",
    "texture_scale_12",
    "texture_scale_13",
    "texture_scale_14",
    "texture_scale_15",
];
```

**What's happening:**
- The uniform name arrays map 1:1 to the shader's `uniform` declarations from Part 08. `vc_tex_rr` = texture slot 0, `vc_tex_rg` = slot 1, etc. The naming convention encodes which vertex color channels select that texture.
- `DEFAULT_GROUND_TEXTURE_PATH` points to a noise texture `.tres` resource — this is the fallback texture when no custom texture is assigned to a slot.
- Only 6 of 16 texture slots get ground albedo tints. Slots 0-5 (rr, rg, rb, ra, gr, gg) are the "named" textures with artist-friendly color controls. Slots 6-15 are raw textures without tinting.

### Step 2: Define the `PixyTerrain` struct

**Why:** This is the largest struct in the project — 50+ fields covering core settings, blending, textures, texture scales, ground colors, shading, grass, and internal state. Every setting that affects the terrain's appearance is an `#[export]` field, making it editable in the Godot inspector.

**File:** `rust/src/terrain.rs` (append)

```rust
/// Main terrain manager node. Manages chunks, exports terrain settings, and syncs shader uniforms.
/// Port of Yugen's MarchingSquaresTerrain (Node3D).
#[derive(GodotClass)]
#[class(base=Node3D, init, tool)]
#[allow(clippy::approx_constant)]
pub struct PixyTerrain {
    base: Base<Node3D>,

    // ═══════════════════════════════════════════
    // Core Settings
    // ═══════════════════════════════════════════
    /// Total height values in X and Z direction, and total height range (Y).
    #[export]
    #[init(val = Vector3i::new(33, 32, 33))]
    pub dimensions: Vector3i,

    /// XZ unit size of each cell.
    #[export]
    #[init(val = Vector2::new(2.0, 2.0))]
    pub cell_size: Vector2,

    /// Blend mode: 0 = smooth, 1 = hard edge, 2 = hard with blend.
    #[export]
    #[init(val = 0)]
    pub blend_mode: i32,

    /// Height threshold that determines where walls begin on the terrain mesh.
    #[export]
    #[init(val = 0.0)]
    pub wall_threshold: f32,

    /// Noise used to generate initial heightmap. If None, terrain starts flat.
    #[export]
    pub noise_hmap: Option<Gd<godot::classes::Noise>>,

    /// Extra collision layer for terrain chunks (9-32).
    #[export]
    #[init(val = 9)]
    pub extra_collision_layer: i32,

    /// Ridge threshold for grass exclusion and ridge texture detection.
    #[export]
    #[init(val = 1.0)]
    pub ridge_threshold: f32,

    /// Ledge threshold for grass exclusion.
    #[export]
    #[init(val = 0.25)]
    pub ledge_threshold: f32,

    /// Whether ridge vertices use wall texture instead of ground texture.
    #[export]
    #[init(val = false)]
    pub use_ridge_texture: bool,

    /// Merge mode index: 0=Cubic, 1=Polyhedron, 2=RoundedPolyhedron, 3=SemiRound, 4=Spherical.
    #[export]
    #[init(val = 1)]
    pub merge_mode: i32,

    // ═══════════════════════════════════════════
    // Blending Settings
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = 5.0)]
    pub blend_sharpness: f32,

    #[export]
    #[init(val = 10.0)]
    pub blend_noise_scale: f32,

    #[export]
    #[init(val = 0.0)]
    pub blend_noise_strength: f32,

    // ═══════════════════════════════════════════
    // Texture Settings (15 texture slots)
    // ═══════════════════════════════════════════
    #[export]
    pub ground_texture: Option<Gd<Texture2D>>,
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

    // ═══════════════════════════════════════════
    // Per-Texture UV Scales
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_1: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_2: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_3: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_4: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_5: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_6: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_7: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_8: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_9: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_10: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_11: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_12: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_13: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_14: f32,
    #[export]
    #[init(val = 1.0)]
    pub texture_scale_15: f32,

    // ═══════════════════════════════════════════
    // Ground Colors (6 slots matching texture slots 1-6)
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4706, 0.3176, 1.0))]
    pub ground_color: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3216, 0.4824, 0.3843, 1.0))]
    pub ground_color_2: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3725, 0.4235, 0.2941, 1.0))]
    pub ground_color_3: Color,
    #[export]
    #[init(val = Color::from_rgba(0.3922, 0.4745, 0.2549, 1.0))]
    pub ground_color_4: Color,
    #[export]
    #[init(val = Color::from_rgba(0.2902, 0.4941, 0.3647, 1.0))]
    pub ground_color_5: Color,
    #[export]
    #[init(val = Color::from_rgba(0.4431, 0.4471, 0.3647, 1.0))]
    pub ground_color_6: Color,

    // ═══════════════════════════════════════════
    // Shading Settings
    // ═══════════════════════════════════════════
    #[export]
    #[init(val = Color::from_rgba(0.0, 0.0, 0.0, 1.0))]
    pub shadow_color: Color,

    #[export]
    #[init(val = 5)]
    pub shadow_bands: i32,

    #[export]
    #[init(val = 0.0)]
    pub shadow_intensity: f32,

    // ═══════════════════════════════════════════
    // Grass Settings
    // ═══════════════════════════════════════════
    #[export]
    pub grass_sprite: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_2: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_3: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_4: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_5: Option<Gd<Texture2D>>,
    #[export]
    pub grass_sprite_tex_6: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = 0)]
    pub animation_fps: i32,
    #[export]
    #[init(val = 3)]
    pub grass_subdivisions: i32,
    #[export]
    #[init(val = Vector2::new(1.0, 1.0))]
    pub grass_size: Vector2,

    #[export]
    #[init(val = true)]
    pub tex2_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex3_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex4_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex5_has_grass: bool,
    #[export]
    #[init(val = true)]
    pub tex6_has_grass: bool,

    #[export]
    #[init(val = Vector2::new(1.0, 1.0))]
    pub wind_direction: Vector2,

    #[export]
    #[init(val = 0.02)]
    pub wind_scale: f32,

    #[export]
    #[init(val = 0.14)]
    pub wind_speed: f32,

    #[export]
    #[init(val = 5)]
    pub default_wall_texture: i32,

    /// Optional grass mesh to use instead of QuadMesh.
    #[export]
    pub grass_mesh: Option<Gd<godot::classes::Mesh>>,

    /// Current texture preset for save/load workflow.
    #[export]
    pub current_texture_preset: Option<Gd<crate::texture_preset::PixyTexturePreset>>,

    // ═══════════════════════════════════════════
    // Internal State (not exported)
    // ═══════════════════════════════════════════
    pub terrain_material: Option<Gd<ShaderMaterial>>,

    /// Shared grass ShaderMaterial — one instance used by all chunk planters.
    pub grass_material: Option<Gd<ShaderMaterial>>,

    /// Shared grass QuadMesh — carries the grass_material, used as MultiMesh mesh by all planters.
    pub grass_quad_mesh: Option<Gd<QuadMesh>>,

    pub is_batch_updating: bool,

    /// Map of chunk coordinates → chunk node.
    #[init(val = HashMap::new())]
    chunks: HashMap<[i32; 2], Gd<PixyTerrainChunk>>,
}
```

**What's happening:**

The struct has 7 field groups:

1. **Core Settings**: Dimensions (33×32×33), cell size (2×2), blend mode, merge mode, noise. These define the geometry.
2. **Blending**: Sharpness and noise parameters that control texture transition quality.
3. **Textures**: 15 texture slots (`Option<Gd<Texture2D>>`). Slot 16 (index 15) is always void/transparent, used for "no texture" areas.
4. **Texture Scales**: Per-texture UV scaling (1.0 = default, higher = smaller texture repeat).
5. **Ground Colors**: 6 albedo tints for the first 6 texture slots, matching the shader's `ground_albedo_*` uniforms.
6. **Grass**: 6 sprite textures, per-texture grass toggles, animation settings, wind parameters.
7. **Internal**: The material objects, batch update flag, and the chunk HashMap.

The `#[allow(clippy::approx_constant)]` attribute suppresses Clippy warnings about color values (like `0.3922`) being close to mathematical constants (like `1/e`). These are artist-chosen colors, not mathematical approximations.

Note that `current_texture_preset` references `crate::texture_preset::PixyTexturePreset` — that module is still a stub (Part 11). The `Option<Gd<...>>` means it starts as `None`, so there's no dependency issue at compile time. The type just needs to exist.

### Step 3: Add texture_preset.rs stub type

**Why:** The `PixyTerrain` struct references `PixyTexturePreset`. We need a minimal type definition so the code compiles. The full implementation comes in Part 11.

**File:** `rust/src/texture_preset.rs` (replace the empty stub)

```rust
use godot::prelude::*;

/// A list of terrain textures, scales, grass sprites, and colors.
/// Full implementation in Part 11.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyTextureList {
    base: Base<Resource>,

    #[export] pub texture_1: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_2: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_3: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_4: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_5: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_6: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_7: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_8: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_9: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_10: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_11: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_12: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_13: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_14: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub texture_15: Option<Gd<godot::classes::Texture2D>>,

    #[export] #[init(val = 1.0)] pub scale_1: f32,
    #[export] #[init(val = 1.0)] pub scale_2: f32,
    #[export] #[init(val = 1.0)] pub scale_3: f32,
    #[export] #[init(val = 1.0)] pub scale_4: f32,
    #[export] #[init(val = 1.0)] pub scale_5: f32,
    #[export] #[init(val = 1.0)] pub scale_6: f32,
    #[export] #[init(val = 1.0)] pub scale_7: f32,
    #[export] #[init(val = 1.0)] pub scale_8: f32,
    #[export] #[init(val = 1.0)] pub scale_9: f32,
    #[export] #[init(val = 1.0)] pub scale_10: f32,
    #[export] #[init(val = 1.0)] pub scale_11: f32,
    #[export] #[init(val = 1.0)] pub scale_12: f32,
    #[export] #[init(val = 1.0)] pub scale_13: f32,
    #[export] #[init(val = 1.0)] pub scale_14: f32,
    #[export] #[init(val = 1.0)] pub scale_15: f32,

    #[export] pub grass_sprite_1: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub grass_sprite_2: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub grass_sprite_3: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub grass_sprite_4: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub grass_sprite_5: Option<Gd<godot::classes::Texture2D>>,
    #[export] pub grass_sprite_6: Option<Gd<godot::classes::Texture2D>>,

    #[export] #[init(val = Color::from_rgba(0.4, 0.5, 0.3, 1.0))] pub grass_color_1: Color,
    #[export] #[init(val = Color::from_rgba(0.3, 0.5, 0.4, 1.0))] pub grass_color_2: Color,
    #[export] #[init(val = Color::from_rgba(0.4, 0.4, 0.3, 1.0))] pub grass_color_3: Color,
    #[export] #[init(val = Color::from_rgba(0.4, 0.5, 0.3, 1.0))] pub grass_color_4: Color,
    #[export] #[init(val = Color::from_rgba(0.3, 0.5, 0.4, 1.0))] pub grass_color_5: Color,
    #[export] #[init(val = Color::from_rgba(0.4, 0.4, 0.4, 1.0))] pub grass_color_6: Color,

    #[export] #[init(val = true)] pub has_grass_2: bool,
    #[export] #[init(val = true)] pub has_grass_3: bool,
    #[export] #[init(val = true)] pub has_grass_4: bool,
    #[export] #[init(val = true)] pub has_grass_5: bool,
    #[export] #[init(val = true)] pub has_grass_6: bool,
}

/// Texture preset resource for save/load workflow.
/// Full implementation in Part 11.
#[derive(GodotClass)]
#[class(base=Resource, init, tool)]
pub struct PixyTexturePreset {
    base: Base<Resource>,

    #[export]
    pub textures: Option<Gd<PixyTextureList>>,
}
```

### Step 4: Implement `enter_tree` and deferred initialization

**Why:** When the terrain node enters the scene tree, it needs to load shaders, create materials, sync all settings, and discover existing chunk children. But Godot's tree isn't fully ready during `enter_tree()` — some child node references may not be valid yet. The solution: `call_deferred()` schedules the real initialization for the next frame.

**File:** `rust/src/terrain.rs` (append)

```rust
#[godot_api]
impl INode3D for PixyTerrain {
    fn enter_tree(&mut self) {
        if !Engine::singleton().is_editor_hint() {
            return;
        }

        // Deferred initialization to ensure tree is ready
        self.base_mut().call_deferred("_deferred_enter_tree", &[]);
    }
}

#[godot_api]
impl PixyTerrain {
    #[func]
    fn _deferred_enter_tree(&mut self) {
        if !Engine::singleton().is_editor_hint() {
            return;
        }

        // Create/load terrain material and shared grass material
        self.ensure_terrain_material();
        self.ensure_grass_material();
        self.force_batch_update();
        self.force_grass_material_update();

        // Discover existing chunk children
        self.chunks.clear();
        let children = self.base().get_children();
        for i in 0..children.len() {
            let Some(child): Option<Gd<Node>> = children.get(i) else {
                continue;
            };
            if let Ok(chunk) = child.try_cast::<PixyTerrainChunk>() {
                let coords = chunk.bind().chunk_coords;
                self.chunks.insert([coords.x, coords.y], chunk);
            }
        }

        // Create configs ONCE before iterating chunks (avoids borrow issues)
        let terrain_config = self.make_terrain_config();
        let grass_config = self.make_grass_config();
        let noise = self.noise_hmap.clone();
        let material = self.terrain_material.clone();

        // Initialize all discovered chunks with cached configs
        let chunk_keys: Vec<[i32; 2]> = self.chunks.keys().cloned().collect();
        for key in chunk_keys {
            if let Some(chunk) = self.chunks.get(&key) {
                let mut chunk = chunk.clone();
                {
                    let mut bind = chunk.bind_mut();
                    bind.set_terrain_config(terrain_config.clone());
                }
                chunk.bind_mut().initialize_terrain(
                    true,
                    noise.clone(),
                    material.clone(),
                    grass_config.clone(),
                );
            }
        }
    }
```

**What's happening:**

The deferred pattern is critical:
1. `enter_tree()` calls `self.base_mut().call_deferred("_deferred_enter_tree", &[])`.
2. `call_deferred` schedules `_deferred_enter_tree` to run at the end of the current frame, after the entire tree is settled.
3. In `_deferred_enter_tree`:
   - Create/reload materials.
   - Sync all shader parameters.
   - Discover chunk children that were saved in the scene.
   - Initialize each chunk with configs and material.

The chunk discovery loop iterates children and `try_cast`s each to `PixyTerrainChunk`. This handles scene reload: when Godot loads a saved scene, the chunk nodes exist as children but haven't been registered in the `chunks` HashMap yet.

The config creation pattern (`make_terrain_config()`, `make_grass_config()`) is done ONCE before the chunk loop. This avoids re-borrowing `self` inside the loop while chunks are being modified.

### Step 5: Implement material creation

**Why:** The terrain material is a `ShaderMaterial` that wraps the terrain shader from Part 08. It's created once and shared by all chunks. The grass material follows the same pattern.

**File:** `rust/src/terrain.rs` (append in a new `impl PixyTerrain` block — non-godot_api)

```rust
impl PixyTerrain {
    /// Ensure terrain material exists, creating it from shader if needed.
    pub fn ensure_terrain_material(&mut self) {
        if self.terrain_material.is_some() {
            return;
        }

        let mut loader = ResourceLoader::singleton();

        if loader.exists(TERRAIN_SHADER_PATH) {
            let resource = loader.load(TERRAIN_SHADER_PATH);
            if let Some(res) = resource {
                if let Ok(shader) = res.try_cast::<Shader>() {
                    let mut mat = ShaderMaterial::new_gd();
                    mat.set_shader(&shader);
                    mat.set_render_priority(-1);
                    self.terrain_material = Some(mat);
                    godot_print!("PixyTerrain: Created terrain material from shader");
                    return;
                }
            }
        }

        godot_warn!("PixyTerrain: Could not load terrain shader at {TERRAIN_SHADER_PATH}");
    }

    /// Ensure shared grass material and QuadMesh exist.
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

        // Create shared QuadMesh with the material applied
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
- `ResourceLoader::singleton()` requires a `mut` binding — even read-only operations like `exists()` take `&mut self` in gdext.
- The terrain shader gets `render_priority(-1)` to ensure it renders before transparent objects (grass).
- The grass material creates a `QuadMesh` immediately — this mesh is shared by all grass planters (one mesh, many MultiMesh instances). Setting both `set_material()` (PrimitiveMesh property) and `surface_set_material()` is belt-and-suspenders — different code paths in Godot may read one or the other.
- `set_center_offset(Vector3::new(0.0, y/2, 0.0))` moves the pivot to the bottom of the quad so grass blades are rooted at the ground.

### Step 6: Implement `force_batch_update()` for terrain shader sync

**Why:** When any terrain setting changes, all shader uniforms must be re-synced. This method reads all Rust export fields and pushes them to the `ShaderMaterial` as uniform values. It's called once during initialization and again whenever the editor UI modifies a setting.

**File:** `rust/src/terrain.rs` (add to the `#[godot_api] impl PixyTerrain` block, after `_deferred_enter_tree`)

```rust
    /// Sync all shader parameters from terrain exports to the terrain material.
    #[func]
    pub fn force_batch_update(&mut self) {
        if self.terrain_material.is_none() {
            return;
        }

        self.is_batch_updating = true;

        // Collect all values before borrowing material
        let dimensions = self.dimensions;
        let cell_size = self.cell_size;
        let wall_threshold = self.wall_threshold;
        let use_hard = self.blend_mode != 0;
        let blend_mode = self.blend_mode;
        let blend_sharpness = self.blend_sharpness;
        let blend_noise_scale = self.blend_noise_scale;
        let blend_noise_strength = self.blend_noise_strength;
        let ground_colors = self.get_ground_colors();
        let scales = self.get_texture_scales();
        let textures = self.get_texture_slots();
        let shadow_color = self.shadow_color;
        let shadow_bands = self.shadow_bands;
        let shadow_intensity = self.shadow_intensity;

        let mat = self.terrain_material.as_mut().unwrap();

        // Core geometry params
        mat.set_shader_parameter("chunk_size", &dimensions.to_variant());
        mat.set_shader_parameter("cell_size", &cell_size.to_variant());
        mat.set_shader_parameter("wall_threshold", &wall_threshold.to_variant());

        // Blend settings
        mat.set_shader_parameter("use_hard_textures", &use_hard.to_variant());
        mat.set_shader_parameter("blend_mode", &blend_mode.to_variant());
        mat.set_shader_parameter("blend_sharpness", &blend_sharpness.to_variant());
        mat.set_shader_parameter("blend_noise_scale", &blend_noise_scale.to_variant());
        mat.set_shader_parameter("blend_noise_strength", &blend_noise_strength.to_variant());

        // Ground colors
        for (i, name) in GROUND_ALBEDO_NAMES.iter().enumerate() {
            mat.set_shader_parameter(*name, &ground_colors[i].to_variant());
        }

        // Texture scales
        for (i, name) in TEXTURE_SCALE_NAMES.iter().enumerate() {
            mat.set_shader_parameter(*name, &scales[i].to_variant());
        }

        // Textures (16 slots)
        for (i, name) in TEXTURE_UNIFORM_NAMES.iter().enumerate() {
            if let Some(ref tex) = textures[i] {
                mat.set_shader_parameter(*name, &tex.to_variant());
            }
        }

        // Shading
        mat.set_shader_parameter("shadow_color", &shadow_color.to_variant());
        mat.set_shader_parameter("bands", &shadow_bands.to_variant());
        mat.set_shader_parameter("shadow_intensity", &shadow_intensity.to_variant());

        self.is_batch_updating = false;
    }
```

**What's happening:**

The borrow checker pattern here is critical:
1. **Collect all values** into local variables BEFORE borrowing `self.terrain_material`.
2. **Borrow the material** with `self.terrain_material.as_mut().unwrap()`.
3. **Set all parameters** using the local variables.

Why? `self.get_ground_colors()` borrows `self` immutably. `self.terrain_material.as_mut()` borrows `self` mutably. Rust won't allow both simultaneously. By collecting first, all immutable borrows are dropped before the mutable borrow begins.

The `set_shader_parameter(*name, &value.to_variant())` pattern: the `*name` dereferences `&&str` to `&str` (the iterator yields `&&str` from the constant array). The `to_variant()` converts any Godot-compatible type to the generic `Variant` type that `set_shader_parameter` expects.

### Step 7: Implement `force_grass_material_update()`

**Why:** The grass material has its own set of uniforms (sprite textures, wind params, per-texture grass toggles). This syncs them all from Rust fields to the grass shader.

**File:** `rust/src/terrain.rs` (add to the `impl PixyTerrain` block — non-godot_api)

```rust
    /// Sync all grass shader parameters from terrain fields to the shared grass material.
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

        let mat = self.grass_material.as_mut().unwrap();

        mat.set_shader_parameter("is_merge_round", &is_merge_round.to_variant());
        mat.set_shader_parameter("wall_threshold", &wall_threshold.to_variant());
        mat.set_shader_parameter("fps", &animation_fps.to_variant());

        let texture_names = [
            "grass_texture", "grass_texture_2", "grass_texture_3",
            "grass_texture_4", "grass_texture_5", "grass_texture_6",
        ];
        for (i, name) in texture_names.iter().enumerate() {
            if let Some(ref tex) = sprites[i] {
                mat.set_shader_parameter(*name, &tex.to_variant());
            }
        }

        mat.set_shader_parameter("grass_base_color", &ground_colors[0].to_variant());
        let color_names = [
            "grass_color_2", "grass_color_3", "grass_color_4",
            "grass_color_5", "grass_color_6",
        ];
        for (i, name) in color_names.iter().enumerate() {
            mat.set_shader_parameter(*name, &ground_colors[i + 1].to_variant());
        }

        mat.set_shader_parameter("use_base_color", &use_base_color[0].to_variant());
        mat.set_shader_parameter("use_base_color_2", &use_base_color[1].to_variant());
        mat.set_shader_parameter("use_base_color_3", &use_base_color[2].to_variant());
        mat.set_shader_parameter("use_base_color_4", &use_base_color[3].to_variant());
        mat.set_shader_parameter("use_base_color_5", &use_base_color[4].to_variant());
        mat.set_shader_parameter("use_base_color_6", &use_base_color[5].to_variant());

        mat.set_shader_parameter("use_grass_tex_2", &tex_has_grass[0].to_variant());
        mat.set_shader_parameter("use_grass_tex_3", &tex_has_grass[1].to_variant());
        mat.set_shader_parameter("use_grass_tex_4", &tex_has_grass[2].to_variant());
        mat.set_shader_parameter("use_grass_tex_5", &tex_has_grass[3].to_variant());
        mat.set_shader_parameter("use_grass_tex_6", &tex_has_grass[4].to_variant());

        mat.set_shader_parameter("wind_direction", &wind_direction.to_variant());
        mat.set_shader_parameter("wind_scale", &wind_scale.to_variant());
        mat.set_shader_parameter("wind_speed", &wind_speed.to_variant());
        mat.set_shader_parameter("animate_active", &true.to_variant());

        let mut loader = ResourceLoader::singleton();
        let wind_path = "res://resources/textures/wind_noise_texture.tres";
        if loader.exists(wind_path) {
            if let Some(wind_tex) = loader.load(wind_path) {
                mat.set_shader_parameter("wind_texture", &wind_tex.to_variant());
            }
        }

        mat.set_shader_parameter("shadow_color", &shadow_color.to_variant());
        mat.set_shader_parameter("bands", &shadow_bands.to_variant());
        mat.set_shader_parameter("shadow_intensity", &shadow_intensity.to_variant());

        if let Some(ref mut quad) = self.grass_quad_mesh {
            quad.set_size(grass_size);
            quad.set_center_offset(Vector3::new(0.0, grass_size.y / 2.0, 0.0));
        }
    }
```

**What's happening:**
- `use_base_color` flags are driven by whether the corresponding *ground texture* (not grass sprite) is assigned. If no ground texture exists for a slot, the grass uses the flat base color instead of trying to sample a missing texture.
- `is_merge_round` is true for 3 of the 5 merge modes. This affects the wall threshold calculation in the grass shader — rounded merge modes need a lower threshold to avoid grass growing on wall faces.
- The wind texture is loaded from a resource file. This is a pre-built noise texture that the shader scrolls across world space for wind animation.

### Step 8: Add helper methods for texture collection

**Why:** `force_batch_update()` and `force_grass_material_update()` need to collect exports into arrays for iteration. These helper methods avoid repeating the field-to-array mapping.

**File:** `rust/src/terrain.rs` (continue in the `impl PixyTerrain` block)

```rust
    fn get_ground_colors(&self) -> [Color; 6] {
        [
            self.ground_color,
            self.ground_color_2,
            self.ground_color_3,
            self.ground_color_4,
            self.ground_color_5,
            self.ground_color_6,
        ]
    }

    fn get_texture_scales(&self) -> [f32; 15] {
        [
            self.texture_scale_1, self.texture_scale_2, self.texture_scale_3,
            self.texture_scale_4, self.texture_scale_5, self.texture_scale_6,
            self.texture_scale_7, self.texture_scale_8, self.texture_scale_9,
            self.texture_scale_10, self.texture_scale_11, self.texture_scale_12,
            self.texture_scale_13, self.texture_scale_14, self.texture_scale_15,
        ]
    }

    fn get_texture_slots(&self) -> [Option<Gd<Texture2D>>; 16] {
        [
            self.get_ground_texture_or_default(0),
            self.get_ground_texture_or_default(1),
            self.get_ground_texture_or_default(2),
            self.get_ground_texture_or_default(3),
            self.get_ground_texture_or_default(4),
            self.get_ground_texture_or_default(5),
            self.get_ground_texture_or_default(6),
            self.get_ground_texture_or_default(7),
            self.get_ground_texture_or_default(8),
            self.get_ground_texture_or_default(9),
            self.get_ground_texture_or_default(10),
            self.get_ground_texture_or_default(11),
            self.get_ground_texture_or_default(12),
            self.get_ground_texture_or_default(13),
            self.get_ground_texture_or_default(14),
            None, // Slot 15: void texture (transparent)
        ]
    }

    fn get_ground_texture_or_default(&self, index: usize) -> Option<Gd<Texture2D>> {
        let texture = match index {
            0 => &self.ground_texture,
            1 => &self.texture_2,
            2 => &self.texture_3,
            3 => &self.texture_4,
            4 => &self.texture_5,
            5 => &self.texture_6,
            6 => &self.texture_7,
            7 => &self.texture_8,
            8 => &self.texture_9,
            9 => &self.texture_10,
            10 => &self.texture_11,
            11 => &self.texture_12,
            12 => &self.texture_13,
            13 => &self.texture_14,
            14 => &self.texture_15,
            _ => return None,
        };

        if texture.is_some() {
            return texture.clone();
        }

        // Load default ground noise texture
        let mut loader = ResourceLoader::singleton();
        if loader.exists(DEFAULT_GROUND_TEXTURE_PATH) {
            let result = loader
                .load(DEFAULT_GROUND_TEXTURE_PATH)
                .and_then(|r| r.try_cast::<Texture2D>().ok());
            if result.is_none() {
                godot_warn!(
                    "Failed to cast default ground texture at {}",
                    DEFAULT_GROUND_TEXTURE_PATH
                );
            }
            return result;
        } else {
            godot_warn!(
                "Default ground texture not found at {}",
                DEFAULT_GROUND_TEXTURE_PATH
            );
        }

        None
    }

    #[allow(dead_code)]
    pub fn set_shader_param(&mut self, name: &str, value: &Variant) {
        if let Some(ref mut mat) = self.terrain_material {
            mat.set_shader_parameter(name, value);
        }
    }

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

        let mut loader = ResourceLoader::singleton();
        let path = "res://resources/textures/grass_leaf_sprite.png";
        if loader.exists(path) {
            return loader
                .load(path)
                .and_then(|r| r.try_cast::<Texture2D>().ok());
        }

        None
    }

    fn extract_ground_image(&self, index: usize) -> Option<Gd<Image>> {
        let tex = self.get_ground_texture_or_default(index)?;
        let mut img = tex.get_image()?;
        img.decompress();
        Some(img)
    }
```

**What's happening:**
- `get_ground_texture_or_default()` falls back to a default noise texture when no custom texture is assigned. This prevents unbound sampler2D in the shader (which would show as white).
- `get_grass_sprite_or_default()` falls back to `grass_leaf_sprite.png`. Without this, the grass shader would sample an unbound texture and show solid white rectangles instead of blade-shaped sprites.
- `extract_ground_image()` calls `get_image()` + `decompress()` to get a CPU-readable copy of the texture. This is used by the grass planter (Part 12) to sample ground texture colors for per-blade color variation.

### Step 9: Add config factory methods

**Why:** When the terrain initializes chunks, it needs to pass a `TerrainConfig` and `GrassConfig` snapshot. These factory methods create those snapshots from current terrain settings. They're called once before chunk iteration loops.

**File:** `rust/src/terrain.rs` (continue in the `impl PixyTerrain` block)

```rust
    fn make_terrain_config(&self) -> TerrainConfig {
        TerrainConfig {
            dimensions: self.dimensions,
            cell_size: self.cell_size,
            blend_mode: self.blend_mode,
            use_ridge_texture: self.use_ridge_texture,
            ridge_threshold: self.ridge_threshold,
            extra_collision_layer: self.extra_collision_layer,
        }
    }

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
}
```

**What's happening:**
- `make_terrain_config()` captures only the fields that affect geometry (dimensions, cell size, blend mode, collision). This is a cheap Clone-able struct.
- `make_grass_config()` captures everything grass planters need: sprites, colors, toggles, wind settings, ground images for color sampling. The `grass_quad_mesh` is upcast from `QuadMesh` to `Mesh` for the generic grass planter interface.
- Both methods are called before chunk loops to avoid borrow conflicts: `self` is borrowed immutably once to create the config, then the config is passed by clone into each chunk.

## Verify

```bash
cd rust && cargo build
```

The terrain module now compiles with the full material pipeline. The module stubs for `texture_preset.rs` and `quick_paint.rs` still compile as-is. Part 10 adds chunk management (add/remove/edge copy).

## What You Learned

- **Deferred initialization**: `call_deferred()` schedules code for the next frame, ensuring the scene tree is fully constructed before accessing child nodes.
- **Borrow collector pattern**: Collect all field values into local variables before mutably borrowing a field. This satisfies the borrow checker when you need to read from multiple fields and write to one.
- **`set_shader_parameter()` API**: Takes `&str` name and `&Variant` value. Use `*name` to deref `&&str` from iterator, `.to_variant()` to convert typed values.
- **Default texture fallbacks**: Every texture slot has a fallback to prevent unbound samplers in the shader, which would produce white artifacts.
- **Config snapshot pattern**: Factory methods create lightweight Clone-able structs from terrain settings, passed to chunks/planters to break borrow cycles.

## Stubs Introduced

- [ ] `PixyTextureList` / `PixyTexturePreset` — minimal Resource stubs, full implementation in Part 11
- [ ] Chunk management methods (`add_new_chunk`, `remove_chunk`, etc.) — Part 10

## Stubs Resolved

- [x] `terrain` module (empty) — introduced in Part 01, now has full material pipeline
- [x] `texture_preset` module (empty) — introduced in Part 01, now has stub Resource types
