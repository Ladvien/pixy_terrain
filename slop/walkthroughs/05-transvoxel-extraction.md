# Walkthrough 05: Transvoxel Mesh Extraction

**Series:** Transvoxel Noise Terrain
**Status:** Planning
**Prerequisites:** Walkthrough 04 (noise field)

## Goal

Implement the `DataField` trait from the transvoxel crate and create a mesh extraction function that converts our noise SDF into triangle meshes.

## Acceptance Criteria

- [ ] `NoiseDataField` implements transvoxel's `DataField` trait
- [ ] `extract_chunk_mesh()` function produces vertices, normals, indices
- [ ] Meshes have world-space positions (not local)
- [ ] Unit test generates non-empty mesh for a chunk at terrain surface

## Understanding Transvoxel API

The transvoxel crate requires:
1. A `DataField` that provides `size()` and `sample(x, y, z)` methods
2. A `Block` defining the region to extract
3. Optional `TransitionSide` flags for LOD boundaries
4. A `MeshBuilder` to collect output (we use `GenericMeshBuilder`)

## Steps

### Step 1: Add DataField Implementation to noise_field.rs

**File:** `rust/src/noise_field.rs` (append to existing file)

```rust
// Full path: rust/src/noise_field.rs (append after existing code)

use transvoxel::prelude::*;

/// Adapter implementing transvoxel's DataField trait for NoiseField
///
/// This wraps a NoiseField and translates grid coordinates to world positions.
pub struct NoiseDataField<'a> {
    /// Reference to the noise field
    noise: &'a NoiseField,
    /// World-space origin of this chunk
    origin: [f32; 3],
    /// Voxel spacing in world units
    voxel_size: f32,
    /// Number of subdivisions (typically 32)
    subdivisions: u32,
}

impl<'a> NoiseDataField<'a> {
    /// Create a new data field adapter
    ///
    /// # Arguments
    /// * `noise` - The noise field to sample
    /// * `origin` - World-space position of chunk corner
    /// * `voxel_size` - Distance between voxels
    /// * `subdivisions` - Grid resolution (typically 32)
    pub fn new(
        noise: &'a NoiseField,
        origin: [f32; 3],
        voxel_size: f32,
        subdivisions: u32,
    ) -> Self {
        Self {
            noise,
            origin,
            voxel_size,
            subdivisions,
        }
    }

    /// Convert grid coordinate to world position
    fn grid_to_world(&self, x: u32, y: u32, z: u32) -> [f32; 3] {
        [
            self.origin[0] + x as f32 * self.voxel_size,
            self.origin[1] + y as f32 * self.voxel_size,
            self.origin[2] + z as f32 * self.voxel_size,
        ]
    }
}

/// Marker type for transvoxel voxel data
#[derive(Clone, Copy, Default)]
pub struct TerrainVoxel {
    pub density: f32,
}

impl VoxelData for TerrainVoxel {
    type Density = f32;

    fn density(&self) -> Self::Density {
        self.density
    }
}

impl<'a> DataField<TerrainVoxel, u32> for NoiseDataField<'a> {
    fn size(&self) -> [u32; 3] {
        // Need subdivisions + 1 samples to define subdivisions cells
        let s = self.subdivisions + 1;
        [s, s, s]
    }

    fn sample(&self, x: u32, y: u32, z: u32) -> TerrainVoxel {
        let [wx, wy, wz] = self.grid_to_world(x, y, z);
        TerrainVoxel {
            density: self.noise.sample(wx, wy, wz),
        }
    }
}
```

### Step 2: Create mesh_extraction.rs Module

**File:** `rust/src/mesh_extraction.rs` (new file)

```rust
// Full path: rust/src/mesh_extraction.rs

//! Mesh extraction using the transvoxel algorithm.
//!
//! This module converts noise SDF data into triangle meshes.

use crate::chunk::{ChunkCoord, MeshResult};
use crate::noise_field::{NoiseDataField, NoiseField, TerrainVoxel};
use transvoxel::prelude::*;

/// Extract mesh for a single chunk
///
/// # Arguments
/// * `noise` - The noise field to sample
/// * `coord` - Chunk coordinate
/// * `lod_level` - LOD level (affects voxel size)
/// * `base_voxel_size` - Voxel size at LOD 0
/// * `chunk_size` - World size of chunk
/// * `transition_sides` - Bit flags for LOD transition faces
///
/// # Returns
/// MeshResult with vertices, normals, and indices in world space
pub fn extract_chunk_mesh(
    noise: &NoiseField,
    coord: ChunkCoord,
    lod_level: u8,
    base_voxel_size: f32,
    chunk_size: f32,
    transition_sides: u8,
) -> MeshResult {
    // Calculate voxel size for this LOD level
    let voxel_size = base_voxel_size * (1 << lod_level) as f32;

    // Get world-space origin of this chunk
    let origin = coord.to_world_position(chunk_size);

    // Subdivisions (typically 32)
    let subdivisions = 32u32;

    // Create data field adapter
    let data_field = NoiseDataField::new(noise, origin, voxel_size, subdivisions);

    // Create transvoxel block (local coordinates)
    let block = Block::new(
        [0.0, 0.0, 0.0],  // Local origin
        chunk_size,        // Size
        subdivisions,      // Subdivisions
    );

    // Convert transition flags
    let transitions = transition_sides_from_u8(transition_sides);

    // Extract mesh
    let builder: GenericMeshBuilder<TerrainVoxel, f32> = extract_from_field(
        &data_field,
        FieldCaching::CacheNothing,  // Can use CacheAll for better perf
        block,
        transitions,
        0.0,  // Threshold (isosurface at SDF = 0)
        GenericMeshBuilder::new(),
    );

    // Get raw mesh data
    let positions = builder.positions();
    let normals = builder.normals();
    let indices = builder.triangle_indices();

    // Convert to world-space positions and normalize normals
    let vertices: Vec<[f32; 3]> = positions
        .iter()
        .map(|p| {
            [
                p[0] + origin[0],
                p[1] + origin[1],
                p[2] + origin[2],
            ]
        })
        .collect();

    let normals: Vec<[f32; 3]> = normals
        .iter()
        .map(|n| normalize_normal(*n))
        .collect();

    let indices: Vec<i32> = indices.iter().map(|&i| i as i32).collect();

    MeshResult {
        coord,
        lod_level,
        vertices,
        normals,
        indices,
        transition_sides,
    }
}

/// Normalize a normal vector
fn normalize_normal(n: [f32; 3]) -> [f32; 3] {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0001 {
        [n[0] / len, n[1] / len, n[2] / len]
    } else {
        [0.0, 1.0, 0.0] // Default up normal
    }
}

/// Convert u8 bit flags to TransitionSides
fn transition_sides_from_u8(flags: u8) -> TransitionSides {
    let mut sides = TransitionSides::empty();

    if flags & 0b000001 != 0 {
        sides |= TransitionSide::LowX;
    }
    if flags & 0b000010 != 0 {
        sides |= TransitionSide::HighX;
    }
    if flags & 0b000100 != 0 {
        sides |= TransitionSide::LowY;
    }
    if flags & 0b001000 != 0 {
        sides |= TransitionSide::HighY;
    }
    if flags & 0b010000 != 0 {
        sides |= TransitionSide::LowZ;
    }
    if flags & 0b100000 != 0 {
        sides |= TransitionSide::HighZ;
    }

    sides
}

/// Bit flag constants for transition sides
pub mod transition_flags {
    pub const LOW_X: u8 = 0b000001;
    pub const HIGH_X: u8 = 0b000010;
    pub const LOW_Y: u8 = 0b000100;
    pub const HIGH_Y: u8 = 0b001000;
    pub const LOW_Z: u8 = 0b010000;
    pub const HIGH_Z: u8 = 0b100000;
    pub const NONE: u8 = 0;
}
```

### Step 3: Add Unit Tests

```rust
// Full path: rust/src/mesh_extraction.rs (append)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise_field::NoiseField;

    fn test_noise() -> NoiseField {
        // Create terrain that crosses y=0
        // height_offset = 0, amplitude = 10
        // So surface is at y = noise * 10 (roughly -10 to +10)
        NoiseField::new(42, 4, 0.02, 10.0, 0.0)
    }

    #[test]
    fn test_extract_chunk_at_surface() {
        let noise = test_noise();

        // Chunk at y=0 should intersect surface
        let result = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(0, 0, 0),
            0,     // LOD 0
            1.0,   // voxel size
            32.0,  // chunk size
            0,     // no transitions
        );

        assert!(
            !result.is_empty(),
            "Chunk at y=0 should have geometry (surface intersects)"
        );
        assert!(
            result.triangle_count() > 0,
            "Should have triangles"
        );
        assert_eq!(
            result.vertices.len(),
            result.normals.len(),
            "Should have matching vertex and normal counts"
        );
    }

    #[test]
    fn test_extract_chunk_above_surface() {
        let noise = test_noise();

        // Chunk high above surface should be empty (all air)
        let result = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(0, 10, 0),  // y=10 chunks up = 320 units
            0,
            1.0,
            32.0,
            0,
        );

        assert!(
            result.is_empty(),
            "Chunk far above surface should be empty"
        );
    }

    #[test]
    fn test_extract_chunk_below_surface() {
        let noise = test_noise();

        // Chunk deep below surface should be empty (all solid)
        let result = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(0, -10, 0),  // y=-10 chunks down
            0,
            1.0,
            32.0,
            0,
        );

        assert!(
            result.is_empty(),
            "Chunk far below surface should be empty (all solid)"
        );
    }

    #[test]
    fn test_world_space_positions() {
        let noise = test_noise();

        let result = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(1, 0, 1),  // Offset chunk
            0,
            1.0,
            32.0,
            0,
        );

        if !result.is_empty() {
            // All vertices should be in chunk's world space
            for v in &result.vertices {
                assert!(
                    v[0] >= 32.0 && v[0] <= 64.0,
                    "X should be in chunk 1 range [32, 64]"
                );
                assert!(
                    v[2] >= 32.0 && v[2] <= 64.0,
                    "Z should be in chunk 1 range [32, 64]"
                );
            }
        }
    }

    #[test]
    fn test_normals_are_normalized() {
        let noise = test_noise();

        let result = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(0, 0, 0),
            0,
            1.0,
            32.0,
            0,
        );

        for n in &result.normals {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "Normal should be unit length, got {len}"
            );
        }
    }

    #[test]
    fn test_transition_flags_conversion() {
        use transition_flags::*;

        let sides = transition_sides_from_u8(LOW_X | HIGH_Z);
        assert!(sides.contains(TransitionSide::LowX));
        assert!(!sides.contains(TransitionSide::HighX));
        assert!(sides.contains(TransitionSide::HighZ));
    }
}
```

### Step 4: Register Module in lib.rs

```rust
// Full path: rust/src/lib.rs

use godot::prelude::*;

mod chunk;
mod lod;
mod mesh_extraction;
mod noise_field;
mod terrain;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
```

### Step 5: Verify

```bash
cd rust && cargo test mesh_extraction
```

Expected: All extraction tests pass.

## Verification Checklist

- [ ] Chunk at surface level produces non-empty mesh
- [ ] Chunk far above surface is empty (all air)
- [ ] Chunk far below surface is empty (all solid)
- [ ] Vertices are in world space (not local)
- [ ] Normals are unit length

## Key Insight: Why Empty Chunks?

Transvoxel only generates triangles where the surface intersects voxels. If all voxels in a chunk are either:
- All positive (air) - no surface
- All negative (solid) - no surface

Then the mesh is empty. This is correct behavior and saves memory/draw calls.

## What's Next

Walkthrough 06 creates the bevy_tasks worker pool for parallel mesh generation.
