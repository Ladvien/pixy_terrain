# Implementing a Stylized 3D Rendering Pipeline in Godot 4.5

A complete cell-shaded, outlined, pixelated rendering pipeline requires careful orchestration of **SubViewport-based low-resolution rendering**, **toon shader materials**, and **post-process edge detection**—combined with C# controller classes and Rust-powered procedural content generation. This guide covers Godot 4.3-4.6 with emphasis on 4.5, including critical technical changes like **reverse-Z depth buffers** that break older shaders.

## Cell and toon shading foundation

Godot 4's built-in `diffuse_toon` and `specular_toon` render modes provide the foundation for stylized rendering, creating hard lighting transitions rather than smooth gradients:

```glsl
shader_type spatial;
render_mode diffuse_toon, specular_toon;

uniform vec4 base_color : source_color = vec4(1.0);
uniform int bands : hint_range(1, 8) = 3;
uniform float wrap : hint_range(-2.0, 2.0) = 0.0;

void fragment() {
    ALBEDO = base_color.rgb;
}

void light() {
    float NdotL = dot(NORMAL, LIGHT);
    float diffuse_amount = NdotL + (ATTENUATION - 1.0) + wrap;
    
    // Create discrete lighting bands
    float cuts_inv = 1.0 / float(bands);
    float diffuse_stepped = clamp(diffuse_amount + mod(1.0 - diffuse_amount, cuts_inv), 0.0, 1.0);
    
    DIFFUSE_LIGHT += diffuse_stepped * ATTENUATION * LIGHT_COLOR * ALBEDO / PI;
}
```

The `light()` function executes **once per light per pixel**, enabling discrete band calculations. For rim lighting—strongest at silhouette edges where view and normal are nearly orthogonal—add:

```glsl
uniform float rim_width : hint_range(0.0, 16.0) = 8.0;
uniform vec4 rim_color : source_color = vec4(1.0);

// Inside light() function:
float NdotV = dot(NORMAL, VIEW);
float rim_light = pow(1.0 - NdotV, rim_width);
SPECULAR_LIGHT += rim_light * rim_color.rgb * LIGHT_COLOR / PI;
```

**Multiple light handling** requires careful consideration. The additive default (`DIFFUSE_LIGHT +=`) can over-brighten scenes; using `DIFFUSE_LIGHT = max(DIFFUSE_LIGHT, diffuse)` clamps to the brightest single light for more consistent toon aesthetics. The **Flexible Toon Shader** (Asset Library #1900) provides a production-ready implementation with configurable bands, color ramps, specular, and rim lighting.

## Post-process outline detection using depth and normals

The **High Quality Post Process Outline** shader by EMBYR combines depth discontinuities (silhouettes) and normal differences (inner edges) using Sobel-like edge detection. This runs as a full-screen spatial shader on a quad parented to the camera:

```glsl
shader_type spatial;
render_mode unshaded, blend_mix, depth_draw_never, depth_test_disabled;

uniform vec4 outlineColor : source_color = vec4(0.0, 0.0, 0.0, 0.78);
uniform float depth_threshold = 0.025;
uniform float normal_threshold : hint_range(0.0, 1.5) = 0.5;
uniform float max_thickness : hint_range(0.0, 5.0) = 1.3;

uniform sampler2D DEPTH_TEXTURE : hint_depth_texture, filter_linear, repeat_disable;
uniform sampler2D NORMAL_TEXTURE : hint_normal_roughness_texture, filter_linear, repeat_disable;

void vertex() {
    POSITION = vec4(VERTEX.xy, 1.0, 1.0);  // Critical for Godot 4.3+ reverse-Z
}

void fragment() {
    // Sample neighboring depths and normals
    vec2 texel = 1.0 / VIEWPORT_SIZE;
    float depth_center = texture(DEPTH_TEXTURE, SCREEN_UV).r;
    float depth_left = texture(DEPTH_TEXTURE, SCREEN_UV - vec2(texel.x, 0)).r;
    float depth_right = texture(DEPTH_TEXTURE, SCREEN_UV + vec2(texel.x, 0)).r;
    
    // Depth-based edges (silhouettes)
    float depth_diff = abs(depth_left - depth_center) + abs(depth_right - depth_center);
    float depthEdge = step(depth_threshold, depth_diff);
    
    // Normal-based edges (inner lines)
    vec3 normal_center = texture(NORMAL_TEXTURE, SCREEN_UV).xyz * 2.0 - 1.0;
    vec3 normal_left = texture(NORMAL_TEXTURE, SCREEN_UV - vec2(texel.x, 0)).xyz * 2.0 - 1.0;
    float normalEdge = step(normal_threshold, length(normal_center - normal_left));
    
    ALBEDO = outlineColor.rgb;
    ALPHA = max(depthEdge, normalEdge) * outlineColor.a;
}
```

**Setup requirements**: Create a MeshInstance3D with a **2×2 QuadMesh**, enable "Flip Faces," set Extra Cull Margin to maximum, parent it to Camera3D at position `(0, 0, -0.1)`. This positions the quad directly in front of the camera for full-screen coverage.

**Critical limitation**: The `hint_normal_roughness_texture` is **only available in Forward+ renderer**—it crashes on Mobile. For Mobile compatibility, reconstruct normals from the depth buffer using finite differences, though this produces lower quality results.

## CompositorEffects for compute-based post-processing

Godot 4.3 introduced **CompositorEffects** for GPU compute shader post-processing with fine-grained pipeline control. Unlike spatial shader quads, CompositorEffects can run at specific render stages (POST_OPAQUE, POST_SKY, POST_TRANSPARENT):

```gdscript
@tool
extends CompositorEffect
class_name OutlineCompositorEffect

func _init() -> void:
    effect_callback_type = EFFECT_CALLBACK_TYPE_POST_TRANSPARENT
    needs_normal_roughness = true  # Request normal buffer access

func _render_callback(effect_callback_type: int, render_data: RenderData) -> void:
    var render_scene_buffers := render_data.get_render_scene_buffers()
    if not render_scene_buffers:
        return
    
    var rd := RenderingServer.get_rendering_device()
    var internal_size := render_scene_buffers.get_internal_size()
    var color_image := render_scene_buffers.get_color_layer(0)
    
    # Dispatch compute shader with 16×16 workgroups
    rd.compute_list_begin()
    # ... bind uniforms, dispatch, end compute list
```

Attach CompositorEffects to Camera3D or WorldEnvironment via the `compositor` property. The **godot-distance-field-outlines** repository provides a complete CompositorEffect outline implementation with excellent documentation.

## SubViewport pixelation pipeline architecture

Low-resolution rendering uses a SubViewport to render at **320×180** or **480×270**, then upscales to display resolution. The node hierarchy controls effect ordering:

```
Root (Node2D)
├── SubViewportContainer (stretch=true, stretch_shrink=6)
│   └── SubViewport (size=320×180)
│       ├── Camera3D (with pixel snapping script)
│       ├── GameWorld (with toon shader materials)
│       └── PostProcessLayer (CanvasLayer, layer=1)
│           └── ColorRect (outline shader)
└── UILayer (CanvasLayer, layer=100)
    └── High-Resolution UI Elements
```

**SubViewport configuration**:
- **Size**: Target pixel resolution (320×180 scales 6× to 1080p)
- **Default Texture Filter**: `Nearest`
- **Snap 2D Transforms to Pixel**: Enabled
- **Snap 2D Vertices to Pixel**: Enabled

Camera pixel snapping prevents **swimming pixels**—subpixel camera movement causing sprites to jitter:

```gdscript
extends Camera3D

var virtual_position: Vector3 = Vector3.ZERO
var snap_delta: Vector3 = Vector3.ZERO
@export var target: Node3D
@export var smoothing_speed: float = 5.0

func _physics_process(delta: float) -> void:
    if target:
        virtual_position = virtual_position.lerp(target.global_position, smoothing_speed * delta)
        var snapped = virtual_position.snapped(Vector3.ONE * (1.0 / 160.0))  # Snap to pixel grid
        snap_delta = virtual_position - snapped
        global_position = snapped
```

For smooth scrolling despite snapping, offset the SubViewport's display sprite by `snap_delta * scale_factor` to achieve subpixel positioning at the display level while maintaining pixel-perfect rendering.

## Godot 4.5 Mono C# shader control patterns

C# provides clean shader parameter control through `ShaderMaterial.SetShaderParameter()`:

```csharp
public partial class ToonEffectController : Node3D
{
    private ShaderMaterial _toonMaterial;
    
    [Export] public int LightingBands { get; set; } = 3;
    [Export] public float RimWidth { get; set; } = 8.0f;
    [Export] public Color RimColor { get; set; } = Colors.White;
    
    public override void _Ready()
    {
        var mesh = GetNode<MeshInstance3D>("Mesh");
        _toonMaterial = (ShaderMaterial)mesh.GetSurfaceOverrideMaterial(0);
        ApplySettings();
    }
    
    public void ApplySettings()
    {
        _toonMaterial.SetShaderParameter("bands", LightingBands);
        _toonMaterial.SetShaderParameter("rim_width", RimWidth);
        _toonMaterial.SetShaderParameter("rim_color", RimColor);
    }
    
    public void UpdateRimInRealtime(float newWidth)
    {
        RimWidth = Mathf.Clamp(newWidth, 0f, 16f);
        _toonMaterial.SetShaderParameter("rim_width", RimWidth);
    }
}
```

**Type mappings**: `float`→`float`, `Vector2/3/4`→`vec2/3/4`, `Color`→`vec4` with `source_color`, `Texture2D`→`sampler2D`. For arrays, use `PackedFloat32Array` or `PackedVector3Array`. Note that `Vector4[]` requires conversion to `Godot.Collections.Array` due to Variant limitations.

For global shader uniforms affecting all materials:

```csharp
RenderingServer.GlobalShaderParameterSet("time_scale", timeScale);
RenderingServer.GlobalShaderParameterSet("player_position", playerPos);
```

## Rust GDExtension capabilities and boundaries

Rust GDExtension (gdext) **cannot write shaders** or access RenderingServer internals directly. Its role is **compute-heavy data generation** that feeds into shader uniforms:

```rust
use godot::prelude::*;
use godot::classes::{ShaderMaterial, PackedFloat32Array};

#[derive(GodotClass)]
#[class(base=Node3D)]
struct TerrainSimulation {
    base: Base<Node3D>,
    heightmap_data: Vec<f32>,
}

#[godot_api]
impl TerrainSimulation {
    #[func]
    fn compute_and_upload(&mut self, material: Gd<ShaderMaterial>) {
        // High-performance Rust computation
        for i in 0..self.heightmap_data.len() {
            self.heightmap_data[i] = self.sample_noise(i);
        }
        
        // Convert and pass to shader
        let packed = PackedFloat32Array::from_slice(&self.heightmap_data);
        material.set_shader_parameter("terrain_heights".into(), packed.to_variant());
    }
    
    #[func]
    fn generate_mesh_data(&self, width: i32, height: i32) -> Gd<ArrayMesh> {
        // Generate mesh vertices, normals, UVs in Rust
        // Return ArrayMesh for use with toon shader materials
        let mesh = ArrayMesh::new_gd();
        // ... mesh generation
        mesh
    }
}
```

**Rust can**: Generate mesh data, compute simulation/physics, process heightmaps, create procedural textures via `ImageTexture`, pass typed arrays to shaders. **Rust cannot**: Write shaders, replace rendering components, access direct GPU memory, compile shader code at runtime.

The interop pattern is: **Rust computes** → **C#/GDScript bridges** → **Shader consumes**. Call Rust methods from C# normally since gdext classes appear as native Godot types.

## Critical Godot 4.3+ technical changes

**Reverse-Z depth buffer** (Godot 4.3+) breaks older shaders. The near plane is now at depth **1.0** instead of 0.0, and far plane at **0.0**:

| Operation | Old Code | New Code (4.3+) |
|-----------|----------|-----------------|
| Vertex POSITION | `vec4(VERTEX, 1.0)` | `vec4(VERTEX.xy, 1.0, 1.0)` |
| Near plane depth | `DEPTH = 0.0` | `DEPTH = 1.0` |
| Depth comparison | `if (pos > depth)` | `if (pos < depth)` |

**TAA incompatibility**: Temporal Anti-Aliasing causes ghosting and outline instability with edge-detection shaders. **Use MSAA 2×-4× instead**—it preserves sharp toon edges without affecting alpha transparency. FXAA works on mobile/low-end but adds slight blur.

**Other 4.3+ changes**: `PI` is now built-in (remove manual definitions), color hints use `source_color` instead of `hint_color`, and `ATTENUATION` changed from `vec3` to `float`.

## Community implementations worth examining

The **Flexible Toon Shader** (Asset Library #1900, GitHub atzuk4451/FlexibleToonShaderGD-4.0) provides configurable bands, color ramps, rim lighting, and specular—ideal for production use. The **Complete Cel Shader** (eldskald/godot4-cel-shader) adds shader includes and global uniforms for project-wide settings.

For retro aesthetics, **Ultimate Retro Shader Collection** (Zorochase, Asset Library #2989) combines PSX vertex snapping, affine texture mapping, N64 three-point filtering, and dithering. **godot-psx-style-demo** (MenacingMecha) demonstrates authentic PS1 rendering with vertex jitter and limited color depth.

Outline implementations include **EMBYR's High Quality Post Process Outline** (godotshaders.com) and **godot-distance-field-outlines** (pink-arcana) for CompositorEffect-based distance field outlines with excellent learning resources.

## Complete pipeline integration example

The full rendering order: **SubViewport renders at 320×180** → **Toon materials on all meshes** → **Outline post-process inside SubViewport** → **Dithering/color reduction** → **Integer upscale to 1080p** → **High-res UI overlay**:

```gdscript
# Root scene setup
func _ready():
    # SubViewport settings
    $SubViewportContainer/SubViewport.size = Vector2i(320, 180)
    $SubViewportContainer/SubViewport.canvas_item_default_texture_filter = \
        SubViewport.DEFAULT_CANVAS_ITEM_TEXTURE_FILTER_NEAREST
    
    # Integer scaling (6× for 1080p)
    $SubViewportContainer.stretch = true
    $SubViewportContainer.stretch_shrink = 1  # Container handles scaling
    
    # Apply toon materials to all meshes
    for mesh in get_tree().get_nodes_in_group("toon_meshes"):
        mesh.material_override = preload("res://shaders/toon_material.tres")
    
    # Outline post-process applies inside SubViewport
    # UI renders at full resolution on separate CanvasLayer
```

For C# control:

```csharp
public partial class RenderPipelineController : Node
{
    [Export] public SubViewport GameViewport { get; set; }
    [Export] public ShaderMaterial OutlineShader { get; set; }
    [Export] public ShaderMaterial DitherShader { get; set; }
    
    public void SetOutlineThickness(float thickness)
    {
        OutlineShader.SetShaderParameter("max_thickness", thickness);
    }
    
    public void SetPixelResolution(Vector2I resolution)
    {
        GameViewport.Size = resolution;
    }
}
```

## Conclusion

Building this pipeline requires understanding the strict boundaries between rendering layers: **shaders handle GPU-side visual effects**, **C# orchestrates parameters and game logic**, and **Rust accelerates CPU-bound data generation**. The SubViewport approach for pixelation provides clean separation from post-processing while maintaining performance. Key gotchas include reverse-Z depth compatibility, TAA conflicts with outlines, and Forward+ requirements for normal buffer access. Start with the Flexible Toon Shader for materials, EMBYR's outline shader for edges, and the godot-smooth-pixel-camera-demo for camera snapping—these provide battle-tested foundations for the complete stylized pipeline.
