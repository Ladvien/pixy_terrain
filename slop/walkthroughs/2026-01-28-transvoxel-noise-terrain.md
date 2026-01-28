# Walkthrough: Transvoxel Noise Terrain Generation

**Date:** 2026-01-28
**Status:** Planning
**Checkpoint:** 0966cb24a77a064fffca36e89f9fb1a9cdf22eb6

## Goal

Implement 3D noise-based terrain generation using the transvoxel algorithm to create smooth, solid geometry meshes in Godot via GDExtension.

## Acceptance Criteria

- [ ] Terrain generates from 3D Perlin/Simplex noise producing an SDF (signed distance field)
- [ ] Transvoxel algorithm extracts smooth mesh from SDF data
- [ ] Mesh displays correctly in Godot with proper normals for lighting
- [ ] Exported parameters control noise octaves, frequency, amplitude, and threshold
- [ ] `regenerate()` function rebuilds terrain when parameters change

## Technical Approach

### Architecture

The system consists of three layers:

1. **Noise Layer** - Generates SDF values using 3D noise (noise crate with Fbm)
2. **Meshing Layer** - Converts SDF to triangles using transvoxel algorithm
3. **Godot Integration Layer** - Builds ArrayMesh and uploads to MeshInstance3D

Data flows: Noise Parameters → SDF Function → Transvoxel Extraction → Godot Mesh

### Key Decisions

- **Transvoxel over Marching Cubes**: Built-in LOD transition support, crack-free mesh boundaries
- **noise crate over fastnoise-lite**: Simpler API, sufficient for initial implementation
- **CPU meshing (not GPU)**: gdext doesn't support compute shaders; CPU is fast enough for 32³ chunks
- **Single-threaded first**: Rayon parallelism is an optimization for later
- **GenericMeshBuilder**: Use the crate's default builder, convert to Godot types afterward

### Dependencies

- `transvoxel = "0.5"` - Isosurface extraction with LOD transitions
- `noise = "0.9"` - Procedural noise generation

### Files to Create/Modify

- `rust/Cargo.toml`: Add transvoxel and noise dependencies
- `rust/src/terrain.rs`: Implement noise SDF and transvoxel mesh generation
- `rust/src/noise_field.rs`: (new) DataField implementation for noise-based SDF

## Build Order

1. **Dependencies**: Add crates to Cargo.toml and verify compilation
2. **Noise SDF**: Implement DataField trait with 3D noise function
3. **Mesh Extraction**: Call transvoxel extract_from_field and collect vertices/normals/indices
4. **Godot Conversion**: Convert rust vecs to PackedArrays and build ArrayMesh
5. **Parameter Exports**: Add noise controls as Godot exports
6. **Integration Test**: Verify mesh appears correctly in Godot editor

## Anticipated Challenges

- **Coordinate systems**: Transvoxel uses its own coordinate types; need careful conversion to Godot Vector3
- **Winding order**: Triangle indices may need reversal depending on coordinate handedness
- **Normals**: Transvoxel generates normals but they may need normalization or sign flipping
- **Threshold tuning**: The iso-surface threshold affects where the surface appears; 0.0 is typical for SDF

## Steps

### Step 1: Add Dependencies

**What you'll build:** Configure Cargo.toml with transvoxel and noise crates
**Key pattern:** Semantic versioning for crate compatibility

```toml
# Full path: rust/Cargo.toml
# Add under [dependencies]:
transvoxel = "0.5"
noise = "0.9"
```

**Verify:** `cargo check` completes without errors

---

### Step 2: Create NoiseField SDF

**What you'll build:** A struct implementing transvoxel's DataField trait
**Key pattern:** SDF returns negative inside, positive outside, zero at surface

```rust
// Full path: rust/src/noise_field.rs

// TODO: Create NoiseField struct with Fbm noise generator
// TODO: Implement DataField trait - sample() returns SDF value at position
// TODO: SDF formula: -position.y + noise.get([x, y, z]) * amplitude
```

**Verify:** Unit test that NoiseField returns expected values

---

### Step 3: Extract Mesh with Transvoxel

**What you'll build:** Function that calls extract_from_field and returns raw mesh data
**Key pattern:** Block defines region, GenericMeshBuilder collects output

```rust
// Full path: rust/src/terrain.rs

// TODO: Create Block with origin, size, subdivisions
// TODO: Call extract_from_field with NoiseField, block, threshold
// TODO: Collect positions, normals, triangle_indices from builder
```

**Verify:** Print vertex count to confirm extraction produced geometry

---

### Step 4: Convert to Godot Mesh

**What you'll build:** Transform transvoxel output into Godot ArrayMesh
**Key pattern:** PackedVector3Array for vertices/normals, PackedInt32Array for indices

```rust
// Full path: rust/src/terrain.rs (in generate_test_mesh or new function)

// TODO: Map transvoxel positions [f32;3] to Godot Vector3
// TODO: Map transvoxel normals to Vector3 (may need normalization)
// TODO: Convert indices to i32 for PackedInt32Array
// TODO: Build ArrayMesh with ARRAY_VERTEX, ARRAY_NORMAL, ARRAY_INDEX
```

**Verify:** Mesh visible in Godot, lit correctly (normals working)

---

### Step 5: Export Noise Parameters

**What you'll build:** GDExtension exports for controlling terrain generation
**Key pattern:** #[export] macro with sensible defaults

```rust
// Full path: rust/src/terrain.rs

// TODO: Add exports: noise_seed, noise_octaves, noise_frequency, noise_amplitude
// TODO: Add threshold export for iso-surface level
// TODO: Modify regenerate() to use these parameters
```

**Verify:** Changing parameters in Godot inspector updates terrain on regenerate()

---

### Step 6: Wire Up Ready and Regenerate

**What you'll build:** Complete lifecycle - generate on ready, regenerate on demand
**Key pattern:** Call new mesh generation from both ready() and regenerate()

```rust
// Full path: rust/src/terrain.rs

// TODO: Replace generate_test_mesh with generate_noise_terrain
// TODO: Call from ready() and regenerate()
// TODO: Add godot_print statements for debugging
```

**Verify:** Terrain appears when scene loads; regenerate() works from editor/script

---

## Known Dragons

*To be filled during proof phase*

## Session Log

- 2026-01-28 07:XX: Started planning
- 2026-01-28: Implementation proven: [to be updated]
- 2026-01-28: User began implementation: [to be updated]

## Bugs Encountered

*To be filled during implementation*
