# Complete camera-terrain solutions for Godot 4 procedural games

Both problems—seeing through single-sided terrain when cameras clip, and revealing players behind mountains—can be solved with a combination of **shader-based double-sided rendering** and **dithered cutaway effects**. The solutions below are production-ready for Godot 4.3+ with orthographic cameras and pixel art aesthetics.

## Problem 1: Making single-sided terrain solid from all angles

Standard marching cubes/squares produces open surfaces, not enclosed volumes. When cameras penetrate terrain, players see void because backfaces don't exist. You have two complementary solutions: **generate watertight geometry** or **render existing geometry double-sided**.

### The shader approach handles most cases

The simplest production fix uses `cull_disabled` with proper normal flipping. This costs roughly **2x fillrate** but requires zero geometry changes and integrates cleanly with existing toon shaders:

```glsl
// terrain_double_sided.gdshader
shader_type spatial;
render_mode cull_disabled, depth_draw_opaque;

uniform sampler2D albedo_texture : source_color, filter_nearest;
uniform vec4 albedo_color : source_color = vec4(1.0);
uniform vec3 backface_tint : source_color = vec3(0.7, 0.7, 0.8);

// Toon shading parameters
uniform float toon_ramp_offset : hint_range(0.0, 1.0) = 0.5;
uniform float toon_ramp_smoothness : hint_range(0.0, 1.0) = 0.05;
uniform vec3 shadow_tint : source_color = vec3(0.5, 0.4, 0.6);

void fragment() {
    vec4 tex_color = texture(albedo_texture, UV) * albedo_color;
    
    // Critical: flip normals on backfaces for correct lighting
    if (!FRONT_FACING) {
        NORMAL = -NORMAL;
        ALBEDO = tex_color.rgb * backface_tint;  // Darken backfaces slightly
    } else {
        ALBEDO = tex_color.rgb;
    }
}

void light() {
    // Toon shading works correctly on both sides after normal flip
    float ndotl = dot(NORMAL, LIGHT) * 0.5 + 0.5;
    float toon_ramp = smoothstep(toon_ramp_offset, toon_ramp_offset + toon_ramp_smoothness, ndotl);
    toon_ramp *= ATTENUATION;
    
    vec3 lit = LIGHT_COLOR * toon_ramp;
    vec3 shadow = ALBEDO * shadow_tint;
    
    DIFFUSE_LIGHT += clamp(lit - shadow, vec3(0.0), vec3(1.0));
    SPECULAR_LIGHT = shadow;
}
```

The `FRONT_FACING` built-in returns `false` when rendering backfaces, allowing proper normal inversion. Without this fix, backfaces appear unlit because normals point away from the light. Set `GeometryInstance3D.cast_shadow` to `SHADOW_CASTING_SETTING_DOUBLE_SIDED` on your terrain mesh for correct shadow casting.

### When you need actual watertight geometry

For physics collisions, raycasting, or extreme camera angles, you'll need closed meshes. The **skirts technique** is the standard solution—extend vertical geometry downward from chunk edges:

```gdscript
# Add to your marching squares mesh generation
func add_mesh_skirt(surface_tool: SurfaceTool, boundary_vertices: Array, skirt_depth: float):
    """Generate vertical walls extending downward from boundary edges"""
    for i in range(boundary_vertices.size()):
        var v1: Vector3 = boundary_vertices[i]
        var v2: Vector3 = boundary_vertices[(i + 1) % boundary_vertices.size()]
        
        # Bottom vertices projected straight down
        var v1_bottom = Vector3(v1.x, v1.y - skirt_depth, v1.z)
        var v2_bottom = Vector3(v2.x, v2.y - skirt_depth, v2.z)
        
        # Calculate outward-facing normal for the wall
        var edge = (v2 - v1).normalized()
        var wall_normal = edge.cross(Vector3.DOWN).normalized()
        
        # Add quad (two triangles) for this wall segment
        surface_tool.set_normal(wall_normal)
        surface_tool.add_vertex(v1)
        surface_tool.add_vertex(v1_bottom)
        surface_tool.add_vertex(v2_bottom)
        
        surface_tool.add_vertex(v1)
        surface_tool.add_vertex(v2_bottom)
        surface_tool.add_vertex(v2)
```

For chunk-based terrain, **sample neighboring chunk data** when meshing boundaries. Store a 1-voxel padding border around each chunk that duplicates data from neighbors—this ensures marching cubes generates matching vertices at seams.

### Algorithm-level alternatives

**Marching Tetrahedra** guarantees manifold (watertight) output by subdividing each cube into 6 tetrahedra, eliminating the ambiguous cases that cause holes in standard marching cubes. The tradeoff is **4x more triangles**.

**Dual Contouring** places one vertex per cell (using QEF minimization) and naturally produces closed meshes with better sharp feature preservation. However, it can create non-manifold vertices where thin features pass through the same cell.

Minecraft and Valheim don't actually use marching cubes—Minecraft uses simple block-face culling (only render faces adjacent to air), while Valheim uses heightmap terrain where the 2D-to-3D extrusion is inherently closed.

## Problem 2: Revealing players behind terrain with dithered cutaway

Isometric games like Diablo and Hades solve occlusion differently. Diablo uses **chain-based wall transparency** triggered when walls form complete enclosures. Hades solves it through **level design**—chambers minimize occlusion by construction. TUNIC deliberately hides secrets using orthographic projection.

For procedural terrain, the most pixel-art-friendly solution is **Bayer matrix dithering** with world-space distance calculations. Unlike alpha blending, dithering is opaque rendering—it writes to depth correctly, needs no sorting, and works with deferred shading.

### Complete cutaway shader with Bayer dithering

```glsl
// terrain_cutaway_dither.gdshader
shader_type spatial;
render_mode cull_disabled, depth_draw_opaque;

uniform sampler2D albedo_texture : source_color, filter_nearest;
uniform vec4 albedo_color : source_color = vec4(1.0);

// Cutaway parameters
uniform vec3 player_position = vec3(0.0);
uniform float cutaway_radius : hint_range(0.0, 20.0) = 5.0;      // Outer edge (fully opaque)
uniform float cutaway_inner : hint_range(0.0, 20.0) = 2.0;       // Inner edge (fully transparent)
uniform float dither_scale : hint_range(1.0, 8.0) = 1.0;         // 1.0 for pixel art
uniform bool use_isometric_distance = true;                       // Flatten Y for isometric

// Backface handling
uniform vec3 backface_tint : source_color = vec3(0.7);

varying vec3 world_position;

// 4x4 Bayer matrix provides 17 discrete transparency levels
// Structured pattern complements pixel art aesthetics
float bayer_4x4(ivec2 pos) {
    int x = pos.x % 4;
    int y = pos.y % 4;
    
    float matrix[16] = float[16](
         0.0/16.0,  8.0/16.0,  2.0/16.0, 10.0/16.0,
        12.0/16.0,  4.0/16.0, 14.0/16.0,  6.0/16.0,
         3.0/16.0, 11.0/16.0,  1.0/16.0,  9.0/16.0,
        15.0/16.0,  7.0/16.0, 13.0/16.0,  5.0/16.0
    );
    
    return matrix[y * 4 + x];
}

// 8x8 Bayer for smoother gradients (65 levels)
float bayer_8x8(ivec2 pos) {
    int x = pos.x % 8;
    int y = pos.y % 8;
    
    float matrix[64] = float[64](
         0.0, 32.0,  8.0, 40.0,  2.0, 34.0, 10.0, 42.0,
        48.0, 16.0, 56.0, 24.0, 50.0, 18.0, 58.0, 26.0,
        12.0, 44.0,  4.0, 36.0, 14.0, 46.0,  6.0, 38.0,
        60.0, 28.0, 52.0, 20.0, 62.0, 30.0, 54.0, 22.0,
         3.0, 35.0, 11.0, 43.0,  1.0, 33.0,  9.0, 41.0,
        51.0, 19.0, 59.0, 27.0, 49.0, 17.0, 57.0, 25.0,
        15.0, 47.0,  7.0, 39.0, 13.0, 45.0,  5.0, 37.0,
        63.0, 31.0, 55.0, 23.0, 61.0, 29.0, 53.0, 21.0
    ) / 64.0;
    
    return matrix[y * 8 + x];
}

void vertex() {
    world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
    vec4 tex_color = texture(albedo_texture, UV) * albedo_color;
    
    // Handle backface normals
    if (!FRONT_FACING) {
        NORMAL = -NORMAL;
        ALBEDO = tex_color.rgb * backface_tint;
    } else {
        ALBEDO = tex_color.rgb;
    }
    
    // Calculate distance to player
    float dist;
    if (use_isometric_distance) {
        // XZ plane distance works better for isometric cameras
        dist = length(world_position.xz - player_position.xz);
    } else {
        dist = length(world_position - player_position);
    }
    
    // Create fade gradient: 0.0 = cut away, 1.0 = fully visible
    float fade = clamp(
        (dist - cutaway_inner) / max(cutaway_radius - cutaway_inner, 0.001),
        0.0, 1.0
    );
    
    // Get screen pixel for dither pattern
    ivec2 screen_pixel = ivec2(FRAGCOORD.xy / dither_scale);
    float threshold = bayer_4x4(screen_pixel);
    
    // Discard pixels based on dither pattern
    if (fade < threshold) {
        discard;
    }
}
```

### GDScript controller for shader uniforms

For a single terrain mesh, set parameters directly on the material:

```gdscript
# terrain_cutaway_controller.gd
extends Node3D

@export var player: Node3D
@export var terrain: MeshInstance3D
@export var cutaway_radius: float = 5.0
@export var cutaway_inner_ratio: float = 0.4

var _material: ShaderMaterial

func _ready() -> void:
    _material = terrain.material_override as ShaderMaterial
    if _material == null:
        _material = terrain.get_surface_override_material(0) as ShaderMaterial

func _process(_delta: float) -> void:
    if _material and player:
        _material.set_shader_parameter("player_position", player.global_position)
        _material.set_shader_parameter("cutaway_radius", cutaway_radius)
        _material.set_shader_parameter("cutaway_inner", cutaway_radius * cutaway_inner_ratio)
```

### Global uniforms for multiple terrain chunks

When your terrain consists of many mesh instances, use **global shader uniforms** to update all materials with a single call. First, define globals in Project Settings → Globals → Shader Globals:

| Name | Type | Default |
|------|------|---------|
| player_pos | vec3 | (0, 0, 0) |
| camera_pos | vec3 | (0, 0, 0) |

Then update via RenderingServer from an autoload:

```gdscript
# shader_globals.gd (autoload)
extends Node

@export var player: Node3D
@export var camera: Camera3D

func _ready() -> void:
    # Add globals if they don't exist
    _ensure_global("player_pos", RenderingServer.GLOBAL_VAR_TYPE_VEC3)
    _ensure_global("camera_pos", RenderingServer.GLOBAL_VAR_TYPE_VEC3)

func _process(_delta: float) -> void:
    if player:
        RenderingServer.global_shader_parameter_set("player_pos", player.global_position)
    if camera:
        RenderingServer.global_shader_parameter_set("camera_pos", camera.global_position)

func _ensure_global(name: String, type: int) -> void:
    if RenderingServer.global_shader_parameter_get(name) == null:
        RenderingServer.global_shader_parameter_add(name, type, Vector3.ZERO)
```

Update the shader to use globals:

```glsl
// Replace the uniform line with:
global uniform vec3 player_pos;

// And reference player_pos instead of player_position in fragment()
```

### Rust (gdext) alternative for position updates

If you're using Rust with godot-rust, the equivalent controller:

```rust
use godot::prelude::*;
use godot::engine::{Node3D, MeshInstance3D, ShaderMaterial, RenderingServer};

#[derive(GodotClass)]
#[class(base=Node3D)]
struct TerrainCutaway {
    #[export]
    player: Option<Gd<Node3D>>,
    #[export]
    terrain: Option<Gd<MeshInstance3D>>,
    #[export]
    cutaway_radius: f32,
    
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for TerrainCutaway {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            player: None,
            terrain: None,
            cutaway_radius: 5.0,
            base,
        }
    }

    fn process(&mut self, _delta: f64) {
        let (Some(player), Some(terrain)) = (&self.player, &self.terrain) else {
            return;
        };
        
        let player_pos = player.get_global_position();
        
        // Using global shader parameters
        let mut rs = RenderingServer::singleton();
        rs.global_shader_parameter_set("player_pos".into(), player_pos.to_variant());
    }
}
```

## Performance and visual tuning

The **dither_scale** uniform controls the dithering pattern size. At `1.0`, each screen pixel gets its own threshold—ideal for pixel art rendered to a low-resolution SubViewport. Increase to `2.0` or `4.0` for higher-resolution rendering where you want larger visible dither blocks.

**Bayer 4×4 vs 8×8**: The 4×4 matrix provides 17 opacity levels with a smaller repeating pattern (more retro). The 8×8 provides 65 levels with smoother gradients but a larger visible tile. For chunky pixel art, 4×4 typically looks better.

**Blue noise dithering** produces a more organic, less structured pattern but requires a texture lookup. Games like Return of the Obra Dinn use Bayer for characters (to make them stand out) and blue noise for environments.

The double-sided rendering costs roughly **2× fragment shader cost** but zero additional geometry memory. If you're vertex-bound rather than fillrate-bound, this is essentially free. For very complex terrain meshes where fillrate matters, consider generating actual backface geometry only for chunks near the camera.

## Combining both solutions

The complete terrain shader handles both problems—double-sided rendering for camera clipping and dithered cutaway for occlusion:

```glsl
// terrain_complete.gdshader
shader_type spatial;
render_mode cull_disabled, depth_draw_opaque;

// Texturing
uniform sampler2D albedo_texture : source_color, filter_nearest;
uniform vec4 albedo_color : source_color = vec4(1.0);
uniform vec3 backface_tint : source_color = vec3(0.7, 0.7, 0.8);

// Toon shading
uniform float toon_threshold : hint_range(0.0, 1.0) = 0.5;
uniform float toon_smoothness : hint_range(0.0, 1.0) = 0.05;
uniform vec3 shadow_color : source_color = vec3(0.5, 0.4, 0.6);

// Cutaway (use globals for multi-chunk terrain)
global uniform vec3 player_pos;
uniform float cutaway_radius : hint_range(0.0, 20.0) = 5.0;
uniform float cutaway_inner : hint_range(0.0, 20.0) = 2.0;
uniform float dither_scale : hint_range(1.0, 8.0) = 1.0;

varying vec3 world_position;

float bayer_4x4(ivec2 pos) {
    int x = pos.x % 4;
    int y = pos.y % 4;
    float m[16] = float[16](
        0.0/16.0, 8.0/16.0, 2.0/16.0, 10.0/16.0,
        12.0/16.0, 4.0/16.0, 14.0/16.0, 6.0/16.0,
        3.0/16.0, 11.0/16.0, 1.0/16.0, 9.0/16.0,
        15.0/16.0, 7.0/16.0, 13.0/16.0, 5.0/16.0
    );
    return m[y * 4 + x];
}

void vertex() {
    world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
    vec4 tex = texture(albedo_texture, UV) * albedo_color;
    
    // Backface normal correction
    if (!FRONT_FACING) {
        NORMAL = -NORMAL;
        ALBEDO = tex.rgb * backface_tint;
    } else {
        ALBEDO = tex.rgb;
    }
    
    // Dithered cutaway around player (XZ distance for isometric)
    float dist = length(world_position.xz - player_pos.xz);
    float fade = clamp((dist - cutaway_inner) / max(cutaway_radius - cutaway_inner, 0.001), 0.0, 1.0);
    
    ivec2 px = ivec2(FRAGCOORD.xy / dither_scale);
    if (fade < bayer_4x4(px)) {
        discard;
    }
}

void light() {
    float ndotl = dot(NORMAL, LIGHT) * 0.5 + 0.5;
    float ramp = smoothstep(toon_threshold, toon_threshold + toon_smoothness, ndotl) * ATTENUATION;
    
    vec3 lit = LIGHT_COLOR * ramp;
    vec3 shadow = ALBEDO * shadow_color;
    
    DIFFUSE_LIGHT += clamp(lit - shadow, vec3(0.0), vec3(1.0));
    SPECULAR_LIGHT = shadow;
}
```

This unified shader provides toon-shaded terrain that renders correctly when cameras penetrate the surface and creates a dithered visibility window around the player. The XZ-plane distance calculation works well for isometric cameras where vertical separation shouldn't affect the cutaway circle's apparent size.

## Godot 4.3+ reverse-Z considerations

Godot 4.3 implemented **reverse-Z depth buffering** where near=1.0 and far=0.0. This doesn't affect the cutaway logic since we calculate distances in world space. However, if you add depth-buffer-based occlusion detection (comparing `FRAGCOORD.z` against sampled depth), flip your comparisons or transform through `INV_PROJECTION_MATRIX` to handle reverse-Z automatically.