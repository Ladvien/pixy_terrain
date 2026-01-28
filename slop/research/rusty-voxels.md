# Building a real-time destructible voxel terrain system in Godot 4 with Rust

A **1km³ smooth voxel world** with real-time destruction at 60fps is achievable using Rust + gdext, but requires careful architecture choices: Transvoxel meshing with **32³ chunks**, sparse SDF storage, time-budgeted mesh updates (**2-4ms per frame**), and a hybrid collision system combining trimesh for player physics with SDF raycasting for projectiles. The Rust crate ecosystem provides mature tools—`transvoxel` for LOD-aware meshing, `sdfu` for smooth boolean operations, and `fast-surface-nets` for high-throughput mesh generation at **~20M triangles/second**. Multiplayer terrain sync works best with server-authoritative edit actions (~20 bytes each) using LZ4 compression, achieving sub-kilobyte updates even during heavy digging.

## Recommended Rust crate stack

The Rust voxel ecosystem has matured significantly, with several crates providing production-ready components. The **`transvoxel`** crate (v2.0.0) handles LOD-aware meshing with crack-free transition cells—essential for your 1km world. It exposes `Block`, `TransitionSide`, and `GenericMeshBuilder` types, generating positions, normals, and triangle indices directly. The crate requires you to implement `DataField` for your SDF function and optionally cache voxel data via `VoxelVecBlock` for **5-6× speedup** on re-queries.

For SDF primitives and boolean operations, **`sdfu`** (v0.3.0) provides an elegant composable API. Its smooth subtraction formula—`h = clamp(0.5 - 0.5*(d2+d1)/k, 0.0, 1.0); return mix(d2, -d1, h) + k*h*(1.0-h)`—creates natural-looking dug surfaces. The softness parameter `k` between **0.1-0.3** produces subtle smoothing ideal for terrain digging. Operations are commutative, meaning edit order doesn't affect results.

| Crate | Purpose | Performance | Status |
|-------|---------|-------------|--------|
| `transvoxel` 2.0 | LOD meshing | ~0.5ms per 32³ chunk | Active |
| `sdfu` 0.3 | SDF primitives | Zero-cost abstractions | Stable |
| `fast-surface-nets` 0.2 | High-throughput meshing | 20M tris/sec | Active |
| `rayon` 1.8 | Parallel chunk processing | Near-linear scaling | Mature |
| `glam` 0.25 | SIMD math | 4-8× vs scalar | Standard |

**Avoid** the `building-blocks` crate—it was archived in November 2023. Its successor crates (`fast-surface-nets`, `block-mesh`, `ndshape`) provide the same functionality with active maintenance.

## SDF architecture for bounded excavation

Your "enclosed volume" approach maps directly to SDF intersection operations. The terrain function becomes `max(solid_box_SDF(p), -edits_SDF(p))`, where the solid box represents your initial 1km³ play space and edits accumulate as smooth-subtracted primitives. The negation inverts each edit, making its interior become exterior, while `max()` performs the boolean intersection.

**Edit storage presents three viable strategies.** Analytical storage keeps edit primitives (spheres, capsules) and evaluates them at runtime—infinite precision, minimal storage, but **O(n)** per sample. Sampled storage writes final SDF values to a grid after each edit—fast queries, but **64KB per 32³ chunk** at 16-bit precision. The hybrid approach stores a coarse grid plus recent analytical edits, periodically "baking" edits into the grid when count exceeds a threshold (typically 50-100 edits per chunk).

For sparse regions, hash maps outperform octrees for scattered edits across your 1km world. Use `HashMap<ChunkCoord, ChunkData>` with 64-bit Morton-encoded keys for spatial locality. Only chunks containing surface geometry or edits consume memory—**empty space costs nothing**.

Material IDs require special handling at boundaries. Store a **uint8 material ID alongside each SDF sample** (2 bytes total per voxel). Critical insight: **never interpolate material IDs like SDF values**. Instead, use hard material transitions with duplicated vertices at boundaries, or compute materials procedurally from world position in the fragment shader (height-based strata, noise-driven ore veins).

## Transvoxel meshing with LOD transitions

Transvoxel extends marching cubes with **transition cells** that bridge different LOD levels without cracks. Regular cells use 8 corner samples; transition cells use **13 samples** (9 on high-res face, 4 on low-res). Pre-computed lookup tables handle all 512 transition configurations, requiring only local voxel data per cell—essential for real-time re-triangulation during destruction.

**Chunk sizing at 32³** balances your requirements optimally. For a 1000m world at 1m resolution, 32³ chunks yield ~30,517 chunks total—manageable draw call count with instancing. Memory per chunk: **64KB** for 16-bit SDF + material, fitting comfortably in L2 cache. Smaller 16³ chunks require 244,000 chunks; larger 64³ chunks reduce flexibility for incremental updates.

LOD selection uses the formula `LOD_level = floor(log2(distance / base_lod_distance))`. A typical cascade:

- **LOD0**: 0-100m, full 1m resolution
- **LOD1**: 100-200m, effective 2m resolution  
- **LOD2**: 200-400m, effective 4m resolution
- **LOD3**: 400-800m, effective 8m resolution

Transition cells automatically stitch adjacent LOD levels. Visual popping can be reduced with hysteresis (different thresholds for increasing vs. decreasing detail) and mesh cross-fading during transitions.

**Normal calculation** via central differences provides best quality: sample SDF at `p ± (h,0,0)`, `p ± (0,h,0)`, `p ± (0,0,h)`, normalize the result. Use distance-adaptive epsilon `h = 0.0001 * chunk_lod_scale`. The tetrahedron technique achieves comparable quality with only 4 SDF evaluations instead of 6.

## Achieving 60fps with time-budgeted updates

At 60fps, your **16.67ms frame budget** must accommodate rendering, physics, game logic, and terrain updates. Allocate **2-4ms** (12-24% of frame) for terrain operations. With optimized meshing at ~0.5ms per 32³ chunk, you can update **4-8 chunks per frame**.

The threading architecture separates concerns across CPU cores:

```
Main Thread (Godot-bound):
├── Input, game logic
├── Priority queue management  
├── Poll mesh results (non-blocking)
└── Upload ≤N meshes until time budget exhausted

Worker Pool (Rayon, 4-8 threads):
├── Mesh generation (work-stealing)
├── SDF evaluation for edits
├── Chunk data decompression
└── Send completed meshes via channel
```

**Priority queuing** determines update order: player's current chunk first, then chunks the player is moving toward, then recently modified chunks, then background loading. Double-buffer mesh data—display the previous mesh while regenerating the new one, then swap pointers atomically.

gdext threading requires the `experimental-threads` feature and careful attention to Godot's main-thread requirements. Perform all heavy computation in pure Rust threads (no Godot API calls), then marshal results to the main thread for `ArrayMesh` creation and physics shape registration. The pattern: `std::thread::spawn` or `rayon::par_iter` for mesh generation, `std::sync::mpsc` channels for result delivery, `call_deferred` for Godot API calls.

**GPU compute** in Godot 4.5/4.6 remains limited. While `RenderingDevice` supports storage buffers and GLSL compute shaders, you cannot bind compute output directly to vertex shaders—a full CPU roundtrip is required via `buffer_get_data()`. This negates GPU meshing benefits when collision data is needed. Use GPU compute selectively for SDF evaluation and noise generation; keep meshing on CPU.

## Multiplayer terrain synchronization

Server-authoritative architecture provides the cleanest multiplayer experience. Clients send edit requests; the server validates, applies, assigns sequence numbers, and broadcasts to affected viewers. For subtractive operations (digging), **last-writer-wins** with timestamps works because removing voxels is commutative—order doesn't matter.

**Edit packets** should be compact. A sphere dig operation requires: position (3×float16 = 6 bytes), radius (float16 = 2 bytes), material (uint8 = 1 byte), timestamp (uint32 = 4 bytes)—totaling ~14 bytes. Batch multiple edits per packet for header amortization. At 1 Mbps upload, you can sustain **6,000+ edits per second** uncompressed.

| Compression | Speed | Ratio | Use Case |
|-------------|-------|-------|----------|
| LZ4 | 500+ MB/s | 2:1 | Real-time edit packets |
| Zstd-3 | ~200 MB/s | 3.7:1 | Chunk streaming |
| RLE | Very fast | 8:1 to 40:1 | Homogeneous terrain |

**Optimistic local prediction** maintains responsiveness: apply edits locally immediately, send request to server, reconcile on server response. Since terrain edits are primarily subtractive and commutative, conflicts are rare and resolve cleanly. Store a local edit history with timestamps; if server correction arrives, discard conflicting edits and re-apply non-conflicting ones.

Interest management prevents bandwidth explosion. Track per-player view areas using `VoxelViewer` patterns; only sync terrain within each player's radius. Give clients a slightly larger view distance than the server to avoid visual holes at the boundary.

## Hybrid collision strategy

**Trimesh collision** via Godot's `ConcavePolygonShape3D` provides accurate player physics but incurs significant regeneration cost. Generate collision meshes only for chunks within player interaction range (typically 3-5 chunks radius). Use simplified meshes—decimate with `meshopt` to 50% triangle count, accepting minor collision inaccuracy for 2× faster updates.

**SDF raycasting** handles projectiles and line-of-sight queries without mesh overhead. Sphere-trace against your SDF data: advance by `sdf_value` at each step until the distance approaches zero. For a 64-iteration limit, typical rays resolve in 10-20 steps. Sphere sweeps offset the query by sphere radius.

```rust
fn sdf_raycast(origin: Vec3, dir: Vec3, max_dist: f32) -> Option<(Vec3, f32)> {
    let mut t = 0.0;
    for _ in 0..64 {
        let d = sample_sdf(origin + dir * t);
        if d < 0.001 { return Some((origin + dir * t, t)); }
        t += d;
        if t > max_dist { break; }
    }
    None
}
```

**Time-budget collision updates** using a priority queue: player's current chunk (immediate), adjacent chunks in movement direction, chunks with active physics objects, then background rebuilds. Allocate **1-2ms per frame** for collision shape updates. `PhysicsServer3D` calls must execute on the main thread—queue shapes for registration rather than blocking workers.

## Material strata and triplanar rendering

Store material layers procedurally based on world Y-coordinate for consistent strata: **0-10m** topsoil, **10-50m** rock, **50-100m** iron ore layer, etc. Override with per-voxel material IDs only where the player has explicitly painted or where generation requires variation (caves, veins). This hybrid approach minimizes storage while maintaining artistic control.

Vertex attributes for smooth terrain require **position** (vec3), **normal** (vec3 or 2-byte octahedral), and **material index** (uint8)—approximately 15-27 bytes per vertex. Pass material indices to the fragment shader; sample texture arrays with `texture(materialArray, vec3(uv, materialIndex))`.

**Triplanar mapping** eliminates UV seams on cut surfaces. Sample each material texture three times using world-space XY, XZ, YZ coordinates, then blend based on surface normal magnitude per axis. Cost: 3× texture samples, but essential for procedural terrain.

```glsl
vec3 triplanar_blend(vec3 normal) {
    vec3 blend = abs(normal);
    blend = pow(blend, vec3(4.0));
    return blend / (blend.x + blend.y + blend.z);
}

vec4 sample_triplanar(sampler2DArray tex, vec3 world_pos, vec3 normal, float layer) {
    vec3 blend = triplanar_blend(normal);
    vec4 x = texture(tex, vec3(world_pos.yz, layer));
    vec4 y = texture(tex, vec3(world_pos.xz, layer));
    vec4 z = texture(tex, vec3(world_pos.xy, layer));
    return x * blend.x + y * blend.y + z * blend.z;
}
```

## Memory budget and sparse storage

Raw storage for 1km³ at various resolutions reveals why sparse storage is mandatory:

| Resolution | Voxels | Raw SDF | With Materials |
|------------|--------|---------|----------------|
| 1.0m | 10⁹ | 1 GB | 2 GB |
| 0.5m | 8×10⁹ | 8 GB | 16 GB |
| 0.25m | 64×10⁹ | 64 GB | 128 GB |

**Sparse voxel storage** exploits the fact that only surfaces and edited regions need data. Use `HashMap<ChunkCoord, ChunkData>` for O(1) chunk lookup; most chunks remain empty. RLE compression on serialization achieves **8:1 to 40:1** ratios for terrain data. For a typical 1km world with ~10% surface coverage, actual memory usage drops to **100-200MB** for 1m resolution.

**Save/load strategy**: persist only edit deltas against procedural generation. Store the world seed plus a list of edited chunks with their RLE-compressed voxel modifications. On load, regenerate base terrain from seed, then apply stored edits. This keeps save files **~1-10MB** even for heavily modified worlds.

## gdext integration patterns for high-performance mesh transfer

Creating meshes from Rust requires converting to Godot's packed array types. Batch all vertex data preparation on the Rust side before a single transfer:

```rust
// Prepare all data in Rust first
let positions: Vec<Vector3> = mesh_buffer.positions.iter()
    .map(|p| Vector3::new(p.x, p.y, p.z)).collect();
let indices: Vec<i32> = mesh_buffer.indices.iter()
    .map(|&i| i as i32).collect();

// Single transfer to Godot
let mut arrays = VariantArray::new();
arrays.resize(Mesh::ARRAY_MAX as usize);
arrays.set(Mesh::ARRAY_VERTEX as usize, PackedVector3Array::from(&positions[..]).to_variant());
arrays.set(Mesh::ARRAY_INDEX as usize, PackedInt32Array::from(&indices[..]).to_variant());

array_mesh.add_surface_from_arrays(Mesh::PRIMITIVE_TRIANGLES, arrays);
```

**Threading constraints**: gdext validates thread IDs and will panic if Godot APIs are called from worker threads without `experimental-threads`. The safe pattern: perform all mesh generation in pure Rust threads, send results via `mpsc::channel`, poll on main thread, create Godot objects there. Use `bytemuck` for zero-copy struct-to-bytes conversion when preparing buffer data.

## Implementation phases

**Phase 1 (Weeks 1-3): Core infrastructure.** Implement chunk storage with `HashMap<ChunkCoord, ChunkData>`, basic SDF evaluation using `sdfu` primitives, and single-threaded mesh generation with `transvoxel`. Target: static terrain rendering at 60fps.

**Phase 2 (Weeks 4-6): Real-time destruction.** Add sphere/capsule edit operations with smooth subtraction, implement dirty chunk tracking, create worker thread pool with Rayon, add time-budgeted mesh upload. Target: single-player destruction at 60fps with 4-8 chunk updates per frame.

**Phase 3 (Weeks 7-9): LOD system.** Implement distance-based LOD selection, enable Transvoxel transition cells, add chunk streaming for distant terrain, optimize memory with sparse storage. Target: full 1km world visible with LOD transitions.

**Phase 4 (Weeks 10-12): Multiplayer foundation.** Create server-authoritative edit system, implement edit serialization with LZ4 compression, add interest management and chunk streaming to joining players. Target: 4+ players with synchronized terrain.

**Phase 5 (Weeks 13-15): Polish and collision.** Implement hybrid collision (trimesh + SDF raycasting), add material strata system with triplanar shaders, optimize based on profiling. Target: production-ready with stable 60fps.

## Conclusion

This architecture leverages Rust's performance characteristics—zero-cost abstractions, fearless concurrency, SIMD via `glam`—while respecting Godot 4's threading constraints. The key insights: **32³ chunks** provide optimal cache behavior and update granularity; **Transvoxel** is essential for crack-free LOD in large worlds; **server-authoritative edits** with commutative operations simplify multiplayer; **hybrid collision** avoids trimesh regeneration for most queries.

Zylann's godot_voxel demonstrates this architecture works at scale with 4,850+ commits and active users. Deep Rock Galactic proves the gameplay model is commercially viable. Your Rust implementation can exceed their C++/Unreal performance through careful memory layout and Rayon's work-stealing parallelism—targeting **~0.5ms per chunk** versus their reported 4-938ms variance.