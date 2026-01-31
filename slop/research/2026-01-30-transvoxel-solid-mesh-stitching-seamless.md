# Seamless mesh stitching between transvoxel terrain and solid geometry

The fundamental rule for watertight mesh connections is unforgiving: **vertices must align exactly with other vertices—never with edges or faces**. When transvoxel terrain (which positions vertices via density field interpolation) meets solid box geometry (with axis-aligned, binary vertex positions), the two vertex positioning systems will never align perfectly at boundaries. This report provides comprehensive techniques for eliminating seams, gaps, and visual artifacts at these challenging intersections.

The most robust solution is to **avoid the stitching problem entirely** by representing all geometry—including walls and structures—as signed distance field (SDF) operations, allowing the transvoxel mesher to generate everything seamlessly. When separate geometry is unavoidable, transition cells, degenerate triangles, skirt meshes, and constrained vertex positioning offer practical alternatives, each with specific tradeoffs.

## Why micro-seams occur at mesh boundaries

The root cause of micro-seams is mathematical: **independently generated meshes cannot share topology without explicit coordination**. When rasterizing edges, GPUs use line-drawing algorithms that treat each edge independently. Two edges that *should* be coincident but don't share vertices are rasterized separately, and floating-point rounding in transformation matrices causes them to cover different pixels, creating visible gaps.

**T-junction artifacts** are particularly insidious. When vertex B lies on edge AC without being part of that edge's triangle structure, the rasterizer has no knowledge that B should coincide with AC. The edge equation for AC evaluates differently than "is this point inside the triangle using B?", producing flickering dots, thin background bleed-through, and "sparkles" that appear and disappear as the camera moves.

The **vertex positioning mismatch** between marching cubes interpolation and axis-aligned geometry is fundamental:

- **Marching Cubes linear interpolation** calculates `t = (isolevel - d1)/(d2 - d1)` and positions vertices at `vertex1 + t × (vertex2 - vertex1)`, producing arbitrary floating-point positions along edges based on density value ratios
- **Axis-aligned vertices** occupy grid positions like `(i, j, k) × voxelSize` with exact integer ratios
- **Binary search positioning** (used in some systems) stops when tolerance is met, producing positions that depend on iteration count and field shape

These systems have different mathematical bases, different error characteristics, and will never produce identical positions except by coincidence. Even when two chunks compute a vertex that should match, floating-point arithmetic through different code paths yields values like **(1.0000001, 2.0, 3.0)** versus **(0.9999999, 2.0, 3.0)**—imperceptibly different mathematically but visible as cracks during rasterization.

## The transvoxel algorithm and its transition cells

Eric Lengyel's **Transvoxel Algorithm** (2009) solves LOD-to-LOD stitching within voxel terrain systems by introducing **transition cells**—special structures that connect meshes at different resolutions. Rather than attempting to patch arbitrary cracks, the algorithm considers **9 samples from the high-resolution boundary face**, creating **512 possible cases** (2⁹) that fall into **73 equivalence classes** through the action of the dihedral group D₄.

The transition cell structure divides into two parts: the left part contains the full-resolution face with 9 samples triangulated using only those samples, while the right part uses conventional Modified Marching Cubes. The 13 total sample locations include 9 on the full-resolution face (4 corners plus 5 midpoints) and 4 on the half-resolution face sharing values with corresponding corners.

**Vertex positioning at LOD boundaries** uses a recursive algorithm:

1. Examine the voxel sample at the midpoint of each active edge
2. If a transition occurs, it happens in only one half of the edge at double resolution
3. Recurse down until reaching the highest resolution
4. Apply interpolation at the highest-resolution sub-edge endpoints

This guarantees **perfect vertex alignment across all LOD levels** within the same voxel system. The vertex position formula uses 8-bit fixed-point interpolation:

```cpp
t = d1 / (d1 - d0)                    // Interpolation parameter
Q = t × P0 + (1-t) × P1               // Vertex position (257 possible positions per edge)
```

**What transvoxel solves versus what it doesn't**: The algorithm handles LOD-to-LOD stitching beautifully—no cracks, holes, or seams at LOD boundaries. However, it does not address **external geometry stitching**. Connecting voxel terrain to non-voxel geometry (buildings, hand-modeled assets, solid boxes) falls outside its design scope. It also requires exactly **2:1 resolution ratios** between adjacent blocks.

## Stitching techniques for different mesh types

### Transition cells and their implementation

Transition cells work by inserting pre-computed triangle patterns at boundaries. The godot_voxel implementation uses a **transition cell scale of 0.25** and generates 15×15×1 transition cells on each of 6 faces:

```cpp
// Border offset calculation for LOD transitions
Vector3f get_border_offset(const Vector3f pos_scaled, const int lod_index, 
    const Vector3i block_size_non_scaled) {
    // When transition meshes are inserted between blocks of different LOD,
    // we need to make space for them.
}
```

Transition meshes contain a `CUSTOM0` attribute with vertex displacement data, and shaders use a `u_transition_mask` bitmask (6 bits for ±X, ±Y, ±Z neighbors) to determine which transition geometries to activate.

### Degenerate triangles for T-junction elimination

**Degenerate triangles** have zero area—typically when two or more vertices share the same position. GPUs automatically cull these at no rendering cost, making them useful for stitching:

```cpp
// High LOD vertices: A -- B -- C
// Low LOD vertices:  A ------- C
// Solution: Move B to C, creating degenerate triangle

// Original: Triangle ABD + Triangle BCD
// After B→C: Triangle ACD (normal) + Triangle CCD (degenerate, culled)
```

From the GameDev.net discussion: "Say you move B to overlap with C. The triangle formed by C-C-E ends up having zero area and disappearing... but the A-B-D and B-E-D triangles will fill the gap."

### Skirt meshes hide gaps with geometry overlap

Skirt meshes are triangle strips that hang **vertically downward** from patch edges:

```cpp
for (vertex in edgeVertices) {
    vec3 top = vertex.position;
    vec3 bottom = vec3(top.x, top.y - skirtHeight, top.z);  // Same XZ, lowered Y
    addVertex(top, vertex.normal, vertex.uv);
    addVertex(bottom, vertex.normal, vertex.uv);
}
```

A "short skirt at base" extends just enough to cover the maximum possible gap: `skirtHeight = maxLODError + safetyMargin`. This approach works regardless of neighbor LOD and requires no mesh modification, but causes texture stretching and can be visible from certain angles.

### Constrained vertex positioning on boundaries

Force boundary vertices to lie on positions that match adjacent geometry:

```cpp
// Ensure boundary vertices snap to power-of-2 aligned positions
if (isOnBoundary(vertex)) {
    float lodScale = pow(2, lodLevel);
    vertex.x = floor(vertex.x / lodScale) * lodScale;
    vertex.z = floor(vertex.z / lodScale) * lodScale;
}
```

For matching lower LOD at boundaries:
```cpp
if (neighborLOD < myLOD && isOnSharedEdge(vertex)) {
    float neighborGridSize = chunkSize / pow(2, neighborLOD);
    vertex.position = snapToGrid(vertex.position, neighborGridSize);
}
```

## The SDF-based approach eliminates stitching entirely

The most robust solution represents **everything as signed distance fields** and lets a single mesher generate all surfaces:

```glsl
// Box SDF
float sdBox(vec3 p, vec3 b) {
    vec3 d = abs(p) - b;
    return min(max(d.x, max(d.y, d.z)), 0.0) + length(max(d, 0.0));
}

// Combine terrain with building
float combined = min(terrain_sdf, sdBox(p - buildingPosition, buildingSize));
```

When you compute `min(terrain_sdf(p), building_sdf(p))`:
1. Both SDFs are evaluated at the **same point**
2. The mesher sees a single, consistent scalar value
3. There's no mesh topology to match—just a unified field

The "return air outside bounds" technique makes structures blend naturally with terrain:

```glsl
float buildingSDF(vec3 p) {
    if (outsideBoundingBox(p, buildingBounds)) {
        return 9999.0;  // Large positive = empty space
    }
    return actualBuildingSDF(p);
}
```

Outside the building bounds, terrain generates normally. Inside, the building SDF takes over. At the boundary, the mesher creates a smooth transition.

**Smooth blending operations** (from Inigo Quilez) create organic transitions:

```glsl
float smoothUnion(float d1, float d2, float k) {
    float h = clamp(0.5 + 0.5*(d1-d2)/k, 0.0, 1.0);
    return mix(d1, d2, h) - k*h*(1.0-h);
}
```

### SDF tradeoffs versus separate mesh geometry

| Aspect | SDF-Generated | Separate Mesh |
|--------|---------------|---------------|
| **Seams** | None (unified generation) | Guaranteed at intersection |
| **Sharp corners** | Require high resolution | Perfect edges |
| **Texturing** | Triplanar/procedural only | Standard UV mapping |
| **Performance** | Higher meshing cost | Lower meshing, separate draws |
| **Modification** | Easy (change SDF params) | Requires mesh editing |

**When SDF works well**: Organic/natural forms, rounded architecture, large-scale features, seamless terrain integration. **When separate geometry is needed**: Pixel-perfect sharp edges, complex UV mapping, fine detail below voxel resolution, thin shells/fences (SDFs struggle with thin features), performance-critical objects.

## Visual artifact prevention at seams

### Lighting seams from mismatched normals

Include neighbor vertices in normal calculation by using **margin data**. From GPU Gems 3: "Enlarge your density volume slightly and use the extra space to generate density values a bit beyond the borders of your block." For a 32³ voxel block, generate density values for a 44³ volume (6-voxel margin per edge).

Gradient-based normal computation that spans boundaries:

```glsl
float d = 1.0 / voxels_per_block;
grad.x = density_vol.Sample(uvw + float3(d, 0, 0)) - density_vol.Sample(uvw + float3(-d, 0, 0));
grad.y = density_vol.Sample(uvw + float3(0, d, 0)) - density_vol.Sample(uvw + float3(0, -d, 0));
grad.z = density_vol.Sample(uvw + float3(0, 0, d)) - density_vol.Sample(uvw + float3(0, 0, -d));
output.wsNormal = -normalize(grad);
```

### Texture coordinate continuity

**Triplanar texturing** eliminates explicit UV coordinates entirely:

```glsl
blend_weights = abs(N_orig.xyz);
blend_weights = (blend_weights - 0.2) * 7;
// Sample from YZ, XZ, XY planes and blend based on normal direction
```

For chunked textures with mipmapping: "Add borders of 2^N where N is the number of mip-levels not including the full-sized texture, and fill this border with edge-pixels from the texture it's supposed to tile into."

### Z-fighting and shadow acne

**Density function erosion** prevents z-fighting between LODs: "Generate the lower-LOD blocks by using a small negative bias in the density function. This isotropically 'erodes' the blocks... higher-LOD chunks will usually encase the lower-LOD chunks."

**Reverse Z-buffer** provides better precision: "Floating-point numbers have much more precision when closer to 0... switching to a logarithmic Z-buffer, reversing Z" is used in AAA games.

### LOD popping with geomorphing

Store both current position and parent (lower LOD) position per vertex, blend based on distance:

```glsl
vec3 finalPos = mix(originalPos, parentPos, lodBlendFactor);
```

From GPU Gems 3: "Draw both LODs during the transition period. Draw the low LOD first and slowly alpha-fade the higher-LOD block in, or out, over some short period of time."

## Game engine implementations

**godot_voxel** implements full Transvoxel with 16³ voxel blocks embedded in 19³ buffers (3-voxel margin for neighbors). The `VoxelMesherTransvoxel` processes 15³ cells for main mesh and generates transition cells on 6 faces. LOD fading via `lod_fade_duration` smooths transitions.

**Unity implementations** typically use the overlap/padding method (1-2 voxel padding of neighbor data) or skirts. Terraxel-Unity implements full Transvoxel; Tuntenfisch/Voxels provides GPU dual contouring with skirts at highest LOD along boundaries.

**PolyVox** uses a critical rule: quads only generate on lower x/y/z faces of regions, with upper faces relying on adjacent region mesh. When modifying boundary voxels, adjacent regions must be re-extracted.

## Implementation strategy recommendation

For **transvoxel terrain meeting solid box geometry**, the recommended approach:

1. **Primary solution**: Represent walls/structures as SDF primitives and let transvoxel generate all geometry. Sharp corners can be achieved with high-enough resolution or boolean SDF operations.

2. **Hybrid approach**: Use SDF for terrain integration (visible seams) and switch to mesh-based rendering for object details (precision matters, seams hidden inside geometry).

3. **Fallback techniques**: When separate meshes are unavoidable, use short skirts (extend downward by maximum error margin), ensure boundary vertices snap to shared grid positions, and recalculate normals using neighbor data.

4. **For heightfield terrain specifically**: Constrain LOD difference to max 1 between neighbors, generate edge vertices at lower LOD, use geomorphing for smooth transitions.

The key insight from the GameDev.net discussion remains paramount: attempts to align vertex B with edge ZW via averaging (`B.Y = (Z.Y + W.Y) / 2`) will fail in the general case. The vertex will be "ever-so-slightly above or below the LOD edge," and the only reliable solutions are **vertex-to-vertex alignment** (make B coincide with Z or W, creating a degenerate triangle) or **physical overlap** (skirts, margin geometry that lets the z-buffer resolve coincident surfaces).

## Key resources and implementations

- **Transvoxel Algorithm**: Eric Lengyel's PhD dissertation at transvoxel.org, lookup tables at github.com/EricLengyel/Transvoxel
- **GPU Gems 3, Chapter 1**: "Generating Complex Procedural Terrains Using the GPU" by Ryan Geiss—covers margin data, triplanar texturing, density erosion
- **GPU Gems 2, Chapter 2**: "Terrain Rendering Using GPU-Based Geometry Clipmaps"—transition regions with geometry/texture morphing
- **godot_voxel**: github.com/Zylann/godot_voxel with full transvoxel mesher implementation
- **SDF operations**: Inigo Quilez's comprehensive SDF reference at iquilezles.org
- **Surface Nets**: 0fps.net "Smooth Voxel Terrain" series covering boundary handling