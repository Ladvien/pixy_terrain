# Texturing transvoxel terrain for real-time destructible voxel games

World-space triplanar mapping combined with vertex-encoded material weights represents the state-of-the-art approach for texturing smooth voxel terrain like Astroneer's. This technique eliminates UV coordinate dependencies entirely, enabling seamless texturing across dynamically generated mesh topology, LOD transitions, and runtime terrain destruction. The core architecture involves storing **4 material indices and blend weights per vertex**, sampling from a **Texture2DArray** via triplanar projection, and using **height-based blending** for natural material interpenetration at boundaries.

Commercial games like Teardown, Deep Rock Galactic, and No Man's Sky all converge on similar solutions: triplanar projection for UV-free texturing, indexed material systems with texture arrays, and SDF-based material layering for destruction. For a Blender-to-Godot pipeline, the recommended workflow bakes material IDs and blend weights to vertex colors in Blender, exports via GLTF with custom attributes, then uses a custom spatial shader in Godot that implements triplanar sampling with slope-based and height-based material blending.

---

## Triplanar mapping eliminates UV dependency for procedural meshes

Triplanar mapping projects textures from three orthogonal axes (X, Y, Z) and blends them based on surface normal orientation. This solves the fundamental problem of voxel terrain: marching cubes and transvoxel algorithms produce meshes with arbitrary vertex positions that cannot be meaningfully UV-unwrapped.

The core algorithm calculates blend weights from the absolute normal components, applies a **sharpness power function** (typically 4-8) to create cleaner transitions, then samples the texture three times using world-space coordinates:

```glsl
vec4 triplanarSample(sampler2D tex, vec3 worldPos, vec3 normal, float scale, float sharpness) {
    vec3 weights = pow(abs(normal), vec3(sharpness));
    weights /= (weights.x + weights.y + weights.z);
    
    vec4 xProj = texture(tex, worldPos.zy / scale);
    vec4 yProj = texture(tex, worldPos.xz / scale);
    vec4 zProj = texture(tex, worldPos.xy / scale);
    
    return xProj * weights.x + yProj * weights.y + zProj * weights.z;
}
```

**Performance optimization** is critical since triplanar triples texture samples. The primary techniques include:

- **Conditional sampling**: Skip projections where weight < 0.01 (reduces average from 3 to ~1.5 samples)
- **Biplanar mapping**: Inigo Quilez's technique uses only the two most significant axes (2 samples instead of 3)
- **Dithered triplanar**: Sample only one projection per pixel, randomized, relying on TAA to converge (1 sample)
- **Channel packing**: Combine Metallic/AO/Roughness into single MOR texture, reducing total samples by 50%

For normal mapping without tangent space, use **Reoriented Normal Mapping (RNM) blend** which properly transforms detail normals for each projection axis before blending. This approach, documented by Ben Golus, produces correct lighting across all surface orientations.

---

## Texture tiling artifacts require multi-scale solutions

Visible texture repetition is the primary quality issue with triplanar mapping. Four complementary techniques address this:

**Hash-based variation** (Inigo Quilez) applies per-tile random offsets and mirroring using a hash function. This requires `textureGrad()` to preserve mipmap derivatives across transformed UVs:

```glsl
vec4 textureNoTile(sampler2D tex, vec2 uv) {
    ivec2 iuv = ivec2(floor(uv));
    vec2 fuv = fract(uv);
    
    vec4 ofa = hash4(iuv + ivec2(0,0));
    // Generate random transforms for 4 surrounding tiles
    // Sample with preserved derivatives
    // Smooth blend at tile boundaries
}
```

**Stochastic texturing** (Heitz & Deliot, Unity Labs 2019) uses histogram-preserving blending on a triangular grid. This works excellently for natural materials like rock and dirt but poorly for geometric patterns.

**Macro variation maps** overlay a very low-frequency color texture (sampled at 0.001× scale) that breaks up large-scale repetition without adding detail texture samples.

**Multi-scale blending** combines detail, mid, and macro textures at different scales (1×, 4×, 16×), using height values from each to create smooth transitions.

---

## Multi-material blending with height maps creates natural transitions

Simple linear blending between materials produces unrealistic results—sand doesn't gradually fade into rock, it fills the cracks. **Height-based blending** uses heightmap data to determine which material "wins" at each pixel:

```glsl
float heightBlend(float h1, float w1, float h2, float w2, float depth) {
    float ma = max(h1 + w1, h2 + w2) - depth;
    float b1 = max(h1 + w1 - ma, 0.0);
    float b2 = max(h2 + w2 - ma, 0.0);
    return b1 / (b1 + b2 + 0.0001);
}
```

The `depth` parameter (typically **0.1-0.3**) controls transition sharpness. Store height information in the alpha channel of albedo textures or as a dedicated heightmap array layer.

**Slope-based material assignment** calculates surface steepness from `1.0 - worldNormal.y` and assigns materials accordingly: grass on flat surfaces (slope < 0.2), rock on steep surfaces (slope > 0.7), with smooth transitions between. This requires no additional voxel data—it's computed entirely from mesh normals.

**Curvature-based weathering** detects convex surfaces (ridges) and concave surfaces (crevices) to apply context-appropriate materials. Convex areas show worn/exposed rock, while concave areas accumulate dirt and moss. For voxel terrain, compute curvature during mesh generation and store it in vertex color alpha, or use screen-space techniques like SSAO derivatives.

---

## Vertex color encoding supports 4+ materials per triangle

For smooth voxel terrain, material data must be encoded per-vertex since there are no stable UVs. The recommended encoding uses **packed RGBA channels**:

```
R: Material 0 weight (grass)
G: Material 1 weight (dirt)  
B: Material 2 weight (rock)
A: Material 3 weight OR curvature data
```

For more than 4 materials, use **custom vertex attributes** with packed material indices:

```glsl
// CUSTOM1.x: 4 texture indices packed as bytes
// CUSTOM1.y: 4 texture weights packed as bytes
uvec4 unpackIndices(float packed) {
    uint data = floatBitsToUint(packed);
    return uvec4(
        data & 0xFFu,
        (data >> 8u) & 0xFFu,
        (data >> 16u) & 0xFFu,
        (data >> 24u) & 0xFFu
    );
}
```

**Texture2DArray** is strongly preferred over texture atlases for multi-material terrain. Arrays provide native UV wrapping, no mipmap bleeding, and support up to **256-2048 layers** depending on GPU. Atlas approaches require manual `fract()` operations and padding pixels between tiles.

---

## Transvoxel LOD transitions require world-space solutions

The Transvoxel algorithm (Eric Lengyel, 2010) inserts **transition cells** at boundaries between different LOD levels, preventing geometric cracks. However, texture coordinates remain problematic because transition cells have asymmetric vertex densities.

World-space triplanar mapping **completely solves** transvoxel texture continuity because:
- No UV coordinates are needed
- All chunks share the same world coordinate system
- Texture mapping is inherently LOD-invariant
- Chunk boundaries are seamless by design

For **vertex morphing** (smooth LOD transitions), Zylann's godot_voxel stores secondary vertex positions in `CUSTOM0`:

```glsl
vec3 get_transvoxel_position(vec3 vertex_pos, vec4 fdata) {
    int idata = floatBitsToInt(fdata.a);
    float s = get_transvoxel_secondary_factor(idata);
    vec3 secondary = fdata.xyz;
    return mix(vertex_pos, secondary, s);
}
```

**Normal generation** should use central differences on the SDF (6 samples) or the more efficient tetrahedron technique (4 samples):

```glsl
vec3 computeNormalTetra(vec3 p, float h) {
    vec2 k = vec2(1,-1);
    return normalize(
        k.xyy * sdf(p + k.xyy*h) +
        k.yyx * sdf(p + k.yyx*h) +
        k.yxy * sdf(p + k.yxy*h) +
        k.xxx * sdf(p + k.xxx*h)
    );
}
```

Use **16-bit SDF precision** rather than 8-bit to maintain gradient quality at larger LOD scales.

---

## Dynamic destruction reveals interior materials via SDF depth layers

For Astroneer-style terrain destruction that exposes "interior" materials, implement **SDF-based material layering**:

```glsl
// Material layers based on distance from original surface
const MaterialLayer layers[3] = MaterialLayer[3](
    MaterialLayer(0u, 0.0),    // Grass at surface
    MaterialLayer(1u, -0.5),   // Dirt 0.5 units below
    MaterialLayer(2u, -2.0)    // Rock 2+ units below
);

vec4 getLayeredMaterial(float sdfValue, vec3 worldPos, vec3 normal) {
    uint layerIndex = 0u;
    for (int i = 0; i < 3; i++) {
        if (sdfValue < layers[i].depthThreshold) {
            layerIndex = layers[i].materialIndex;
        }
    }
    return triplanarSampleArray(worldPos, normal, layerIndex);
}
```

**Fresh vs weathered surfaces** can track "time since exposure" or use procedural methods based on SDF gradient steepness—sharp cuts (high gradient magnitude) indicate recent destruction:

```glsl
float cutSharpness = clamp(abs(dFdx(sdfValue)) + abs(dFdy(sdfValue)), 0.0, 1.0);
float freshness = cutSharpness * (1.0 - timeSinceExposure / 5.0);
```

**Procedural damage patterns** add noise-based variation to destruction edges, creating irregular rather than perfectly spherical cuts:

```glsl
float damageNoise = snoise(worldPos * 8.0);
float transitionWidth = 0.3 + damageNoise * 0.1;
float edgeFactor = smoothstep(-transitionWidth, transitionWidth, sdf);
```

For **real-time material updates**, use compute shaders with dirty region tracking—only update chunks where destruction occurred.

---

## Commercial games validate triplanar + indexed material architectures

**Astroneer** uses Unreal Engine 4 with JIT-compiled terrain shader graphs. Its visual style relies on **flat shading** with simple textures, biome-based color systems (107 terrain colors), and 4 hardness tiers with distinct visual textures. The key insight: stylized aesthetics reduce texturing complexity while maintaining visual appeal.

**Deep Rock Galactic** implements Marching Cubes with **chunking, LOD, and caching**. Procedural cave generation combines hand-crafted "shapes" that are distributed and combined algorithmically. The destruction system uses identical tech for level generation and player drilling—when drilling, "it's the same tech as building the level."

**Teardown** represents the most technically sophisticated voxel renderer analyzed. Key techniques:
- **8-bit indexed color palette** per object (256 materials, 1 byte per voxel)
- **Ray-marched fragment shaders** through 3D textures with MIP-map skip traversal
- **Volumetric shadow mapping** at 3504×200×3000 voxels (~262MB, 1-bit per voxel)
- **Heavy temporal denoising** with stochastic sampling and motion-vector reprojection

**No Man's Sky** demonstrates continuous planet-scale generation with seamless space-to-surface transitions. The philosophy of "augmenting artists rather than replacing them" means procedural systems respect hand-authored content—a relevant principle for addon design.

---

## Godot 4 implementation requires Texture2DArray and custom vertex attributes

A complete Godot 4 terrain shader combining triplanar mapping, slope-based blending, and vertex color materials:

```gdshader
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2DArray albedo_array : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2DArray normal_array : hint_normal, filter_linear_mipmap, repeat_enable;
uniform float triplanar_scale = 1.0;
uniform float triplanar_sharpness = 4.0;
uniform float slope_threshold : hint_range(0.0, 1.0) = 0.7;

varying vec3 world_pos;
varying vec3 world_normal;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

vec4 triplanar_sample_array(sampler2DArray tex, vec3 pos, vec3 weights, int layer) {
    vec4 x = texture(tex, vec3(pos.zy * triplanar_scale, float(layer)));
    vec4 y = texture(tex, vec3(pos.xz * triplanar_scale, float(layer)));
    vec4 z = texture(tex, vec3(pos.xy * triplanar_scale, float(layer)));
    return x * weights.x + y * weights.y + z * weights.z;
}

void fragment() {
    // Triplanar blend weights
    vec3 weights = pow(abs(world_normal), vec3(triplanar_sharpness));
    weights /= (weights.x + weights.y + weights.z);
    
    // Slope-based material selection
    float slope = 1.0 - world_normal.y;
    float slope_blend = smoothstep(slope_threshold - 0.1, slope_threshold + 0.1, slope);
    
    // Material weights from vertex colors (R=grass, G=dirt, B=rock, A=snow)
    vec4 mat_weights = COLOR;
    mat_weights.b = max(mat_weights.b, slope_blend); // Override rock on slopes
    mat_weights /= max(dot(mat_weights, vec4(1.0)), 0.001);
    
    // Sample and blend materials
    vec4 grass = triplanar_sample_array(albedo_array, world_pos, weights, 0);
    vec4 dirt = triplanar_sample_array(albedo_array, world_pos, weights, 1);
    vec4 rock = triplanar_sample_array(albedo_array, world_pos, weights, 2);
    vec4 snow = triplanar_sample_array(albedo_array, world_pos, weights, 3);
    
    ALBEDO = grass.rgb * mat_weights.r + dirt.rgb * mat_weights.g + 
             rock.rgb * mat_weights.b + snow.rgb * mat_weights.a;
}
```

**Compute shaders** enable GPU-side material assignment when voxel data changes:

```glsl
#[compute]
#version 450

layout(local_size_x = 8, local_size_y = 8, local_size_z = 8) in;

layout(std430, binding = 0) buffer VoxelData { float sdf[]; };
layout(std430, binding = 1) buffer MaterialData { uvec2 materials[]; };

uniform vec3 destruction_center;
uniform float destruction_radius;

void main() {
    uvec3 gid = gl_GlobalInvocationID;
    uint idx = gid.x + gid.y * chunk_size + gid.z * chunk_size * chunk_size;
    
    vec3 world_pos = chunk_origin + vec3(gid);
    float dist = distance(world_pos, destruction_center);
    
    if (dist < destruction_radius) {
        float old_sdf = sdf[idx];
        sdf[idx] = max(old_sdf, dist - destruction_radius);
        
        // Mark newly exposed surfaces with interior material
        if (old_sdf < 0.0 && sdf[idx] >= 0.0) {
            materials[idx].x |= 0x10000u; // Set "exposed" flag
        }
    }
}
```

---

## Blender baking pipeline exports material data via vertex colors

Since Blender 2.92, native baking to vertex colors is supported. Configure Cycles baking with `target = 'VERTEX_COLORS'`:

```python
import bpy
import bmesh

def bake_terrain_material_data(terrain_obj):
    mesh = terrain_obj.data
    
    # Create vertex color layers
    mesh.color_attributes.new(name="_MaterialWeights", type='BYTE_COLOR', domain='CORNER')
    mesh.color_attributes.new(name="_TerrainData", type='BYTE_COLOR', domain='CORNER')
    
    bm = bmesh.new()
    bm.from_mesh(mesh)
    
    mat_layer = bm.loops.layers.color.get("_MaterialWeights")
    data_layer = bm.loops.layers.color.get("_TerrainData")
    
    for face in bm.faces:
        for loop in face.loops:
            vert = loop.vert
            
            # Calculate material weights based on position/normal
            height = vert.co.z
            slope = 1.0 - abs(vert.normal.z)
            
            grass = max(0, 1.0 - slope * 2 - height * 0.1)
            rock = min(1, slope * 2)
            snow = max(0, (height - 5.0) * 0.2)
            dirt = max(0, 1.0 - grass - rock - snow)
            
            loop[mat_layer] = (grass, dirt, rock, snow)
            
            # Curvature/AO in secondary layer
            # (compute actual curvature with neighbor sampling)
            loop[data_layer] = (0.5, 0.5, 0.5, 1.0)
    
    bm.to_mesh(mesh)
    bm.free()
```

**GLTF export** requires custom attributes to have underscore prefixes (`_MaterialWeights`) for spec compliance. Vertex colors connected to material nodes export automatically; custom attributes require `export_attributes = True`.

For **procedural Blender materials that need Godot equivalents**, use Material Maker (RodZilla's open-source tool) which creates procedural materials as GLSL shader graphs directly portable to Godot. Alternatively, bake procedural materials to textures via Cycles.

---

## Academic techniques offer advanced anti-tiling and synthesis methods

**Wang Tiles** (Cohen et al., SIGGRAPH 2003) provide non-periodic tiling with minimal tile sets. An 8-tile set with 2-color edges covers all valid configurations. GPU implementation packs tiles into a single texture with hardware filtering working across boundaries. This approach is particularly effective for terrain textures, reducing visible repetition without stochastic sampling overhead.

**Wave Function Collapse** adapts well to terrain generation with slope-based features rather than raw heights. Recent work (arXiv 2024) uses SRTM elevation data as input patterns. WFC is computationally expensive so must run offline or during loading rather than real-time.

**Global SDF volumes** (Flax Engine approach) rasterize scene geometry into cascaded SDF textures—4 cascades with higher precision nearby, ~200m total coverage. This enables GI, particle collisions, and procedural effects including material transitions at destruction boundaries.

**GigaVoxels** (Crassin et al., INRIA) pioneered sparse voxel octrees with GPU raycasting. The octree + bricks representation with 3D texture atlases scales to billions of voxels. While complex to implement, this approach informs modern virtual geometry systems.

---

## Progressive disclosure balances simplicity with power-user control

For addon UX, implement a tiered parameter system:

**Preset level** (beginners): Single dropdown selecting terrain style (grassland, desert, cave, alien). Each preset configures all underlying parameters.

**Basic level** (intermediate): Expose 4-8 key parameters—texture scale, blend sharpness, slope threshold, material assignments. Group in collapsible panel.

**Advanced level** (experts): Full access to all parameters including triplanar sharpness exponent, height blend depth, noise octaves, LOD fade distances. Hidden by default behind "Show Advanced" toggle.

Tooltips should explain both what a parameter does and why users might adjust it. For example: "Blend Sharpness (4-16): Controls how quickly textures transition between projection axes. Higher values create sharper transitions suitable for blocky styles; lower values create smoother blends for organic terrain."

## Conclusion

The optimal architecture for Astroneer-style destructible voxel terrain combines **Transvoxel mesh generation**, **world-space triplanar mapping**, **Texture2DArray materials** with vertex-encoded blend weights, and **SDF-based material layering** for destruction effects. This approach scales from stylized low-poly aesthetics to realistic PBR terrain, supports runtime modification, and maintains visual consistency across LOD transitions.

Key implementation priorities: start with basic triplanar + slope-based blending (immediate visual results), add height-based material transitions (natural appearance), implement vertex color material encoding (multi-material support), then add destruction-aware material layers (Astroneer-style digging). Performance optimization via conditional sampling and LOD-based quality reduction ensures real-time framerates even with complex terrain.