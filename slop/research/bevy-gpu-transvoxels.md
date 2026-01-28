# GPU transvoxel destructible terrain in Bevy 0.18: Technical documentation

Building a GPU-based transvoxel destructible terrain system requires integrating multiple complex subsystems: compute shader mesh generation, SDF storage and modification, LOD management, and procedural generation. **Bevy 0.18** (released January 13, 2026) provides the necessary wgpu-based compute pipeline APIs, while Eric Lengyel's **Transvoxel algorithm** handles seamless LOD transitions. This documentation compiles authoritative sources, direct repository links, and academic papers for implementing each component.

---

## Bevy 0.18 render and compute APIs

Bevy 0.18 introduced significant changes to the render pipeline that affect compute shader implementation. The **`RenderPipelineDescriptor`** and **`ComputePipelineDescriptor`** now hold `BindGroupLayoutDescriptor` instead of `BindGroupLayout`—descriptors create layouts when first needed by a pipeline. Additionally, `BindGroupLayout` labels are no longer optional; you must implement `fn label() -> &'static str` in `AsBindGroup`.

### Core documentation links

| Resource | URL |
|----------|-----|
| Official Release Notes | https://bevy.org/news/bevy-0-18/ |
| Migration Guide 0.17→0.18 | https://bevy.org/learn/migration-guides/0-17-to-0-18/ |
| bevy::render module | https://docs.rs/bevy/latest/bevy/render/index.html |
| bevy::render::render_resource | https://docs.rs/bevy/latest/bevy/render/render_resource/index.html |
| ComputePipelineDescriptor | https://docs.rs/bevy/latest/bevy/render/render_resource/struct.ComputePipelineDescriptor.html |
| PipelineCache | https://docs.rs/bevy/latest/bevy/render/render_resource/struct.PipelineCache.html |
| RenderDevice | https://docs.rs/bevy/latest/bevy/render/renderer/struct.RenderDevice.html |
| Render Graph API | https://docs.rs/bevy/latest/bevy/render/render_graph/index.html |
| Node Trait | https://docs.rs/bevy/latest/bevy/render/render_graph/trait.Node.html |

### Official compute shader examples

The **Game of Life compute example** at https://github.com/bevyengine/bevy/blob/main/examples/shader/compute_shader_game_of_life.rs demonstrates the complete pattern for `RenderGraph` nodes, `ComputePassDescriptor` usage, and bind group management. The **custom vertex attribute example** at https://github.com/bevyengine/bevy/blob/main/examples/shader/custom_vertex_attribute.rs shows how to define custom mesh vertex attributes for generated geometry.

### Critical 0.18 mesh API changes

The `Mesh` struct now includes `asset_usage: RenderAssetUsages` and `enable_raytracing: bool` fields. New `try_*` methods (e.g., `try_insert_attribute()`) return `Result<..., MeshAccessError>` for `RenderAssetUsages::RENDER_WORLD`-only meshes. Bevy 0.18 automatically updates `Aabb` for modified meshes; use the `NoAutoAabb` component to disable this behavior for GPU-generated meshes.

### wgpu documentation

Bevy 0.18 uses **wgpu 27.x**. Essential documentation:
- wgpu crate docs: https://docs.rs/wgpu/
- Learn wgpu tutorial: https://sotrh.github.io/learn-wgpu/beginner/tutorial3-pipeline/
- WebGPU compute fundamentals: https://webgpufundamentals.org/webgpu/lessons/webgpu-compute-shaders.html

---

## Eric Lengyel's transvoxel algorithm

The Transvoxel algorithm solves the critical problem of **seamless LOD transitions** for isosurface extraction. Regular marching cubes creates visible seams between chunks at different resolutions; transition cells connect meshes at 2:1 resolution boundaries using 9 high-resolution samples plus 4 low-resolution samples per cell.

### Primary authoritative sources

| Resource | URL |
|----------|-----|
| Official Transvoxel Website | https://transvoxel.org/ |
| PhD Dissertation (9 MB PDF) | https://transvoxel.org/Lengyel-VoxelTerrain.pdf |
| Visual Poster | https://transvoxel.org/Transvoxel.pdf |
| Official Lookup Tables (C++) | https://github.com/EricLengyel/Transvoxel |
| Published Paper DOI | 10.1080/2151237X.2011.563682 |

The dissertation "Voxel-Based Terrain for Real-Time Virtual Simulations" contains the complete theoretical basis, with Tables 3.1-3.2 covering Modified Marching Cubes equivalence classes and Tables 4.1-4.8 covering transition cell equivalence classes. The algorithm is **patent-free**.

### Lookup table structure

The official `Transvoxel.cpp` at https://github.com/EricLengyel/Transvoxel contains:
- `regularCellClass[256]` — Maps 8-bit Marching Cubes case to equivalence class (0-15)
- `regularCellData[16]` — Vertex count, triangle count, triangulation for regular cells
- `regularVertexData[256][12]` — Edge intersection data for each case
- `transitionCellClass[512]` — Maps 9-bit transition case to equivalence class (0-55)
- `transitionCellData[56]` — Geometry data for transition cells (up to 36 vertices)
- `transitionVertexData[512][12]` — Transition cell vertex positions

### Rust implementations

| Crate | Repository | docs.rs |
|-------|------------|---------|
| **transvoxel** (v2.0.0) | https://github.com/Gnurfos/transvoxel_rs | https://docs.rs/transvoxel |
| transvoxel-data (tables only) | https://github.com/TheGreatB3/transvoxel-data-rs | — |

The `transvoxel` crate provides `extract_from_field()` and `extract()` functions, a `TransitionSide` enum for LOD boundaries (LowX, HighX, LowY, HighY, LowZ, HighZ), and includes Bevy integration examples.

### Marching cubes foundation

Paul Bourke's reference at https://paulbourke.net/geometry/polygonise/ provides the foundational `edgeTable[256]` and `triTable[256][16]` that Transvoxel extends. Source code: https://paulbourke.net/geometry/polygonise/marchingsource.cpp

---

## GPU sparse voxel data structures

Efficient GPU voxel storage requires specialized data structures that balance memory efficiency with random access performance. Three approaches dominate: **Sparse Voxel Octrees (SVO)**, **brick-based structures**, and **spatial hashing**.

### Foundational papers

| Paper | Authors | URL |
|-------|---------|-----|
| **Efficient Sparse Voxel Octrees** | Laine & Karras (NVIDIA, 2010) | https://research.nvidia.com/sites/default/files/pubs/2010-02_Efficient-Sparse-Voxel/laine2010i3d_paper.pdf |
| Technical Report (extended) | Laine & Karras | https://research.nvidia.com/sites/default/files/pubs/2010-02_Efficient-Sparse-Voxel/laine2010tr1_paper.pdf |
| **GigaVoxels** | Crassin et al. (INRIA, 2009) | https://maverick.inria.fr/Publications/2009/CNLE09/CNLE09.pdf |
| GigaVoxels SIGGRAPH Slides | — | https://maverick.inria.fr/Publications/2009/CNLSE09/GigaVoxels_Siggraph09_Slides.pdf |
| **Voxel Hashing** | Nießner et al. (2013) | https://niessnerlab.org/papers/2013/4hashing/niessner2013hashing.pdf |
| Perfect Spatial Hashing | Lefebvre & Hoppe | https://hhoppe.com/perfecthash.pdf |
| ASH: Parallel Spatial Hashing | CMU (2023) | https://arxiv.org/abs/2110.00511 |

The Laine & Karras paper introduces **compact SVO storage** with contour information, efficient ray casting, and normal compression. GigaVoxels establishes the **N³-tree + brick pool** paradigm with ray-guided streaming. Voxel Hashing enables real-time reconstruction using hash maps that map integer world coordinates to **8³ voxel blocks** containing TSDF values.

### NVIDIA GVDB Voxels library

NVIDIA's official sparse voxel library provides production-ready GPU voxel management:
- Repository: https://github.com/NVIDIA/gvdb-voxels
- Programming Guide: https://github.com/NVIDIA/gvdb-voxels/blob/master/GVDB_Programming_Guide_1.1.pdf
- GTC Presentation: https://on-demand.gputechconf.com/gtc/2017/presentation/s7424-rama-hoetzlein-introduction-and-techniques-nvidia-gvdb-voxels.pdf

GVDB uses **indexed memory pooling** for dynamic topology on GPU, hierarchical traversal for raytracing, and OpenVDB compatibility. Apache 2.0 licensed.

---

## Real-time SDF modification and CSG

Destructible terrain requires efficient GPU-based SDF modification. The authoritative reference for SDF primitives and CSG operations is **Inigo Quilez's distance functions collection** at https://iquilezles.org/articles/distfunctions/.

### Core CSG operations (GLSL)

```glsl
// Union (exact)
float opUnion(float d1, float d2) { return min(d1, d2); }

// Subtraction (bound)
float opSubtraction(float d1, float d2) { return max(-d1, d2); }

// Intersection (bound)
float opIntersection(float d1, float d2) { return max(d1, d2); }

// Smooth Union (for terrain blending)
float opSmoothUnion(float d1, float d2, float k) {
    float h = clamp(0.5 + 0.5*(d2-d1)/k, 0.0, 1.0);
    return mix(d2, d1, h) - k*h*(1.0-h);
}
```

### Additional SDF resources

- hg_sdf Library (Mercury): https://mercury.sexy/hg_sdf/
- GPU Gems 3, Chapter 34 (SDF generation): https://developer.nvidia.com/gpugems/gpugems3/part-v-physics-simulation/chapter-34-signed-distance-fields-using-single-pass-gpu
- Curated SDF resources: https://github.com/CedricGuillemet/SDF

---

## GPU mesh generation with compute shaders

Running Transvoxel on GPU compute shaders requires solving the **variable output geometry** problem—each cell can produce 0-5 triangles, making buffer allocation non-trivial.

### Prefix sum and stream compaction

| Resource | URL |
|----------|-----|
| GPU Gems 3 Chapter 39 (foundational) | https://developer.nvidia.com/gpugems/gpugems3/part-vi-gpu-computing/chapter-39-parallel-prefix-sum-scan-cuda |
| Decoupled Look-back (state-of-art) | https://research.nvidia.com/publication/single-pass-parallel-prefix-scan-decoupled-look-back |
| Raph Levien Vulkan prefix sum | https://raphlinus.github.io/gpu/2020/04/30/prefix-sum.html |
| Portable prefix sum | https://raphlinus.github.io/gpu/2021/11/17/prefix-sum-portable.html |
| **GPUPrefixSums** (multi-platform) | https://github.com/b0nes164/GPUPrefixSums |
| Stream Compaction (Chalmers) | https://www.cse.chalmers.se/~uffe/streamcompaction.pdf |

The **GPUPrefixSums** repository is particularly valuable—it implements Decoupled Fallback for devices without forward progress guarantees, supports CUDA, D3D12, Unity, and **WGPU**, and works with all wave/subgroup sizes [4, 128].

### GPU marching cubes pipeline

The standard approach uses two compute passes:
1. **Pass 1**: Classify cells, count triangles per cell
2. **Prefix Sum**: Calculate output offsets
3. **Pass 2**: Generate triangles at calculated offsets
4. **Indirect Draw**: Render generated geometry

### WebGPU marching cubes implementation

Will Usher's detailed WebGPU implementation at https://www.willusher.io/graphics/2024/04/22/webgpu-marching-cubes/ demonstrates parallel scan for output allocation in a wgpu-compatible context.

### Indirect drawing best practices

Critical insight from Toji's guide at https://toji.dev/webgpu-best-practices/indirect-draws.html: batch indirect draws into the same GPUBuffer to avoid massive validation overhead (**3ms→10μs** with 412 draws).

Additional resources:
- wgpu RenderPass docs: https://docs.rs/wgpu/latest/wgpu/struct.RenderPass.html
- Vulkan indirect drawing guide: https://vkguide.dev/docs/gpudriven/draw_indirect/
- Compute-to-render patterns: https://toji.dev/webgpu-best-practices/compute-vertex-data.html

---

## Procedural noise and terrain density functions

### FastNoise libraries

| Resource | URL |
|----------|-----|
| FastNoise2 (C++, SIMD) | https://github.com/Auburn/FastNoise2 |
| FastNoise2 Wiki | https://github.com/Auburn/FastNoise2/wiki |
| **fastnoise-lite** (Rust) | https://crates.io/crates/fastnoise-lite |
| fastnoise-lite docs | https://docs.rs/fastnoise-lite/latest/fastnoise_lite/ |
| noise-rs (alternative) | https://docs.rs/noise |

FastNoise2 provides a modular node graph supporting OpenSimplex2, Perlin, Value, and Cellular noise with built-in fBm, Ridged, and PingPong fractals plus domain warping. The `fastnoise-lite` Rust crate is the official port with `no_std` support.

### GPU noise implementations

| Resource | URL |
|----------|-----|
| Stefan Gustavson "Simplex Noise Demystified" | https://cgvr.cs.uni-bremen.de/teaching/cg_literatur/simplexnoise.pdf |
| Gustavson implementations | https://github.com/stegu/perlin-noise |
| **webgl-noise** (GLSL) | https://github.com/ashima/webgl-noise |
| "Efficient computational noise in GLSL" | https://arxiv.org/abs/1204.1461 |
| bevy_shader_utils (WGSL) | https://crates.io/crates/bevy_shader_utils |
| WebGPU-Lab (WGSL examples) | https://github.com/s-macke/WebGPU-Lab |

The **webgl-noise** library provides `snoise(vec3 x)` for 3D simplex noise at https://github.com/ashima/webgl-noise/blob/master/src/noise3D.glsl—this can be translated to WGSL. The `bevy_shader_utils` crate offers WGSL noise functions directly importable as `#import bevy_shader_utils::simplex_noise_3d::simplex_noise_3d`.

### Terrain density function patterns

**GPU Gems 3 Chapter 1** at https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu provides the authoritative density function approach:

```hlsl
// Basic terrain with ground plane
float density = -ws.y;

// Add 3D noise for features
density += noiseVol.Sample(ws).x;

// Multiple octaves for detail
for (int i = 0; i < 9; i++) {
    density += amplitude * noiseVol.Sample(ws * frequency).x;
    frequency *= 1.97; // slightly off 2.0 to reduce repetition
    amplitude *= 0.5;
}

// Domain warping for caves/overhangs
float3 warp = noiseVol.Sample(ws * 0.004).xyz;
ws += warp * 8;

// Hard floor (sediment deposition)
float hard_floor_y = -13;
density += saturate((hard_floor_y - ws.y) * 3) * 40;
```

Additional resources:
- Inigo Quilez fBm: https://iquilezles.org/articles/fbm/
- Domain Warping: https://iquilezles.org/articles/warp/
- Musgrave terrain paper: https://www.classes.cs.uchicago.edu/archive/2015/fall/23700-1/final-project/MusgraveTerrain00.pdf
- Procedural cave generation: http://julian.togelius.com/Mark2015Procedural.pdf

---

## Triplanar PBR material mapping

Triplanar mapping eliminates UV stretching on steep surfaces by projecting textures from three orthogonal planes and blending based on surface normal.

### Mathematical basis

From **GPU Gems 3 Chapter 1** and Ben Golus's authoritative implementation:

```glsl
// UV coordinate generation
vec2 coord1 = position.yz * texScale;  // X-axis projection
vec2 coord2 = position.zx * texScale;  // Y-axis projection
vec2 coord3 = position.xy * texScale;  // Z-axis projection

// Blend weight calculation
vec3 blend_weights = abs(normal);
blend_weights = (blend_weights - 0.2) * 7;  // Tighten blending zone
blend_weights = max(blend_weights, 0);
blend_weights /= (blend_weights.x + blend_weights.y + blend_weights.z);
```

### Implementation resources

| Resource | URL |
|----------|-----|
| Ben Golus triplanar normals | https://bgolus.medium.com/normal-mapping-for-a-triplanar-shader-10bf39dca05a |
| Ben Golus repository | https://github.com/bgolus/Normal-Mapping-for-a-Triplanar-Shader |
| Catlike Coding tutorial | https://catlikecoding.com/unity/tutorials/advanced-rendering/triplanar-mapping/ |
| GLSL reference gist | https://gist.github.com/patriciogonzalezvivo/20263fe85d52705e4530 |
| Biplanar optimization (Quilez) | https://www.shadertoy.com/view/3ddfDj |

Ben Golus's work covers three normal blending techniques: **UDN Blend** (fastest), **Whiteout Blend** (more accurate), and **Reoriented Normal Mapping** (ground truth).

### Bevy custom materials

| Resource | URL |
|----------|-----|
| Material Trait | https://docs.rs/bevy/latest/bevy/pbr/trait.Material.html |
| Extended Material example | https://bevy.org/examples/shaders/extended-material/ |
| Custom shader material | https://bevy.org/examples/shaders/shader-material/ |

Use `ExtendedMaterial<StandardMaterial, MyExtension>` to extend Bevy's PBR while adding triplanar sampling. Material uniforms use **group 2**, with binding indices starting at 100+ for extensions.

---

## Reference implementations and projects

### Rust/Bevy voxel projects

| Project | URL | Assessment |
|---------|-----|------------|
| **bevy_voxel_world** | https://github.com/splashdust/bevy_voxel_world | ⭐⭐⭐⭐⭐ Best-maintained, Bevy 0.17 compatible, production-ready |
| **dust-engine** | https://github.com/dust-engine/dust | ⭐⭐⭐⭐ Cutting-edge Vulkan raytracing, requires RTX |
| **voxelis** | https://github.com/WildPixelGames/voxelis | ⭐⭐⭐⭐ Pure Rust SVO DAG, 99.999% compression |
| vx_bevy | https://github.com/Game4all/vx_bevy | ⭐⭐⭐ Minecraft-style learning reference |

### Core Rust meshing crates

| Crate | URL | Use Case |
|-------|-----|----------|
| **block-mesh** | https://crates.io/crates/block-mesh | Greedy meshing, 40M quads/sec |
| **fast-surface-nets** | https://crates.io/crates/fast-surface-nets | SDF meshing, 20M triangles/sec |
| **transvoxel** | https://crates.io/crates/transvoxel | LOD transitions |
| **binary-greedy-meshing** | https://crates.io/crates/binary-greedy-meshing | 30x faster than block-mesh |

### GPU terrain references (other engines)

| Project | URL | Value |
|---------|-----|-------|
| **Tuntenfisch/Voxels** (Unity) | https://github.com/Tuntenfisch/Voxels | GPU Dual Contouring with multi-material, HLSL reference |
| Dreams SIGGRAPH 2015 | https://advances.realtimerendering.com/s2015/AlexEvans_SIGGRAPH-2015-sml.pdf | Point cloud SDF rendering architecture |
| GPU Gems 3 Ch. 1 | https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu | Foundational GPU terrain concepts |

---

## Conclusion

This documentation provides the authoritative foundation for implementing GPU-based transvoxel destructible terrain in Bevy 0.18. The **core technical stack** consists of Bevy's compute pipeline APIs wrapping wgpu 27.x, the Transvoxel algorithm from Eric Lengyel's `transvoxel.org` resources, GPU prefix sum from the GPUPrefixSums repository for variable geometry output, and fastnoise-lite for procedural density functions. 

Key implementation priorities should be: (1) establish the compute-to-render pipeline using Bevy's `RenderGraph` and `PipelineCache`, (2) implement prefix sum for triangle count allocation, (3) port the Transvoxel lookup tables to WGSL, and (4) integrate triplanar PBR sampling via `ExtendedMaterial`. The `transvoxel` Rust crate provides a working CPU reference that can guide GPU shader translation, while the Tuntenfisch/Voxels Unity project demonstrates the complete GPU dual contouring pipeline in HLSL that can inform WGSL architecture.