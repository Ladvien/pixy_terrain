# Walkthrough 02: Core Data Structures

**Series:** Transvoxel Noise Terrain
**Status:** Planning
**Prerequisites:** Walkthrough 01 (dependencies)

## Goal

Create the fundamental data structures for chunk management: ChunkCoord, MeshResult, ChunkState, and Chunk.

## Acceptance Criteria

- [ ] `ChunkCoord` struct with coordinate conversion methods
- [ ] `MeshResult` struct using only Rust primitive types (no Godot types!)
- [ ] `ChunkState` enum for lifecycle tracking
- [ ] `Chunk` struct combining state and metadata
- [ ] Unit tests pass

## Why No Godot Types in MeshResult?

gdext validates thread IDs and will panic if Godot APIs are called from worker threads. By keeping mesh data as primitive Rust types (`Vec<[f32; 3]>`, `Vec<i32>`), worker threads can safely produce mesh data. Conversion to Godot's `PackedVector3Array` happens only on the main thread.

## Steps

### Step 1: Create chunk.rs Module

**File:** `rust/src/chunk.rs` (new file)

```rust
// Full path: rust/src/chunk.rs

//! Core data structures for chunk management.
//!
//! IMPORTANT: MeshResult uses only Rust primitive types to ensure
//! thread-safety. Godot type conversion happens on main thread only.

/// Integer coordinate identifying a chunk in the world grid
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Convert chunk coordinate to world position (corner of chunk)
    pub fn to_world_position(&self, chunk_size: f32) -> [f32; 3] {
        [
            self.x as f32 * chunk_size,
            self.y as f32 * chunk_size,
            self.z as f32 * chunk_size,
        ]
    }

    /// Get chunk center in world space
    pub fn to_world_center(&self, chunk_size: f32) -> [f32; 3] {
        [
            (self.x as f32 + 0.5) * chunk_size,
            (self.y as f32 + 0.5) * chunk_size,
            (self.z as f32 + 0.5) * chunk_size,
        ]
    }

    /// Distance squared from chunk center to a world position
    pub fn distance_squared_to(&self, pos: [f32; 3], chunk_size: f32) -> f32 {
        let center = self.to_world_center(chunk_size);
        let dx = center[0] - pos[0];
        let dy = center[1] - pos[1];
        let dz = center[2] - pos[2];
        dx * dx + dy * dy + dz * dz
    }
}
```

### Step 2: Add MeshResult

```rust
// Full path: rust/src/chunk.rs (append)

/// Mesh data produced by worker threads.
///
/// Uses only Rust primitive types - NO Godot types!
/// This allows safe generation on worker threads.
pub struct MeshResult {
    /// Which chunk this mesh belongs to
    pub coord: ChunkCoord,
    /// LOD level used for generation
    pub lod_level: u8,
    /// Vertex positions in world space
    pub vertices: Vec<[f32; 3]>,
    /// Vertex normals (normalized)
    pub normals: Vec<[f32; 3]>,
    /// Triangle indices
    pub indices: Vec<i32>,
    /// Transition sides used (bit flags)
    pub transition_sides: u8,
}

impl MeshResult {
    /// Check if mesh is empty (no geometry generated)
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Number of triangles in this mesh
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Number of vertices in this mesh
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
}
```

### Step 3: Add ChunkState and Chunk

```rust
// Full path: rust/src/chunk.rs (append)

/// Lifecycle state of a chunk
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkState {
    /// Not yet requested for meshing
    Unloaded,
    /// Mesh generation in progress on worker thread
    Pending,
    /// Mesh ready, waiting for upload to Godot
    Ready,
    /// Mesh uploaded, actively rendering
    Active,
    /// Too far from camera, marked for removal
    MarkedForUnload,
}

/// Chunk metadata stored by ChunkManager
pub struct Chunk {
    pub coord: ChunkCoord,
    pub state: ChunkState,
    pub lod_level: u8,
    /// Godot instance ID of MeshInstance3D (if Active)
    pub mesh_instance_id: Option<i64>,
    /// Frame number of last access (for LRU)
    pub last_access_frame: u64,
}

impl Chunk {
    pub fn new(coord: ChunkCoord, lod_level: u8) -> Self {
        Self {
            coord,
            state: ChunkState::Unloaded,
            lod_level,
            mesh_instance_id: None,
            last_access_frame: 0,
        }
    }
}
```

### Step 4: Add Unit Tests

```rust
// Full path: rust/src/chunk.rs (append)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_coord_to_world() {
        let coord = ChunkCoord::new(1, 2, 3);
        let pos = coord.to_world_position(32.0);
        assert_eq!(pos, [32.0, 64.0, 96.0]);
    }

    #[test]
    fn test_chunk_coord_center() {
        let coord = ChunkCoord::new(0, 0, 0);
        let center = coord.to_world_center(32.0);
        assert_eq!(center, [16.0, 16.0, 16.0]);
    }

    #[test]
    fn test_chunk_distance() {
        let coord = ChunkCoord::new(0, 0, 0);
        let dist_sq = coord.distance_squared_to([16.0, 16.0, 16.0], 32.0);
        assert!(dist_sq < 0.001); // Should be at center
    }

    #[test]
    fn test_mesh_result_empty() {
        let result = MeshResult {
            coord: ChunkCoord::new(0, 0, 0),
            lod_level: 0,
            vertices: vec![],
            normals: vec![],
            indices: vec![],
            transition_sides: 0,
        };
        assert!(result.is_empty());
        assert_eq!(result.triangle_count(), 0);
    }

    #[test]
    fn test_mesh_result_with_data() {
        let result = MeshResult {
            coord: ChunkCoord::new(0, 0, 0),
            lod_level: 0,
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            indices: vec![0, 1, 2],
            transition_sides: 0,
        };
        assert!(!result.is_empty());
        assert_eq!(result.triangle_count(), 1);
        assert_eq!(result.vertex_count(), 3);
    }
}
```

### Step 5: Register Module in lib.rs

```rust
// Full path: rust/src/lib.rs

use godot::prelude::*;

mod chunk;
mod terrain;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
```

### Step 6: Verify

```bash
cd rust && cargo test
```

Expected: All tests pass.

```bash
cargo build
```

Expected: Compiles without errors.

## Verification Checklist

- [ ] `ChunkCoord::to_world_position` converts correctly
- [ ] `ChunkCoord::distance_squared_to` calculates distance
- [ ] `MeshResult` contains no Godot types
- [ ] All unit tests pass
- [ ] Module registered in lib.rs

## What's Next

Walkthrough 03 adds LOD configuration with distance-based level selection.
