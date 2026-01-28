# Walkthrough 07: Chunk Manager with LOD Selection

**Series:** Transvoxel Noise Terrain
**Status:** Planning
**Prerequisites:** Walkthrough 06 (worker pool)

## Goal

Create the ChunkManager that coordinates which chunks to load/unload based on camera position, selects appropriate LOD levels, and computes transition side flags for crack-free boundaries.

## Acceptance Criteria

- [ ] `ChunkManager` tracks all loaded chunks with their states
- [ ] `compute_desired_chunks()` returns chunks within view distance at correct LOD
- [ ] `compute_transition_sides()` sets flags when neighbor has higher detail
- [ ] `update()` sends mesh requests and polls results
- [ ] Distant chunks are marked for unload

## Transition Sides Explained

When two adjacent chunks have different LOD levels, the lower-detail chunk needs "transition cells" on the shared face to prevent cracks. The transvoxel algorithm handles this via `TransitionSide` flags.

```
Higher Detail (LOD 0)     Lower Detail (LOD 1)
┌─────────────────┐       ┌─────────────────┐
│                 │       │                 │
│   32x32 grid    │◄─────►│   16x16 grid    │
│                 │       │   + transition  │
│                 │       │     cells on    │
│                 │       │     left face   │
└─────────────────┘       └─────────────────┘
```

## Steps

### Step 1: Create chunk_manager.rs Module

**File:** `rust/src/chunk_manager.rs` (new file)

```rust
// Full path: rust/src/chunk_manager.rs

//! Chunk lifecycle management with LOD selection.
//!
//! The ChunkManager:
//! - Decides which chunks to load based on camera position
//! - Selects appropriate LOD level per chunk
//! - Computes transition side flags for seamless boundaries
//! - Coordinates with the worker pool for mesh generation

use std::collections::HashMap;
use std::sync::Arc;

use crossbeam::channel::{Receiver, Sender};

use crate::chunk::{Chunk, ChunkCoord, ChunkState, MeshResult};
use crate::lod::LODConfig;
use crate::mesh_worker::MeshRequest;
use crate::noise_field::NoiseField;

/// Manages chunk lifecycle and LOD selection
pub struct ChunkManager {
    /// All tracked chunks
    chunks: HashMap<ChunkCoord, Chunk>,
    /// LOD configuration
    lod_config: LODConfig,
    /// Base voxel size at LOD 0
    base_voxel_size: f32,
    /// Channel to send requests to workers
    request_tx: Sender<MeshRequest>,
    /// Channel to receive completed meshes
    result_rx: Receiver<MeshResult>,
    /// Current frame number (for LRU tracking)
    current_frame: u64,
    /// Maximum meshes to return per update
    max_results_per_update: usize,
}

impl ChunkManager {
    /// Create a new chunk manager
    pub fn new(
        lod_config: LODConfig,
        base_voxel_size: f32,
        request_tx: Sender<MeshRequest>,
        result_rx: Receiver<MeshResult>,
    ) -> Self {
        Self {
            chunks: HashMap::new(),
            lod_config,
            base_voxel_size,
            request_tx,
            result_rx,
            current_frame: 0,
            max_results_per_update: 8,
        }
    }

    /// Get chunk world size
    fn chunk_size(&self) -> f32 {
        self.base_voxel_size * self.lod_config.chunk_subdivisions as f32
    }

    /// Update chunks based on camera position
    ///
    /// Returns mesh results ready for upload to Godot
    pub fn update(
        &mut self,
        camera_pos: [f32; 3],
        noise_field: &Arc<NoiseField>,
    ) -> Vec<MeshResult> {
        self.current_frame += 1;

        // Determine which chunks should be loaded
        let desired = self.compute_desired_chunks(camera_pos);

        // Request new chunks or LOD changes
        for (coord, desired_lod) in &desired {
            self.ensure_chunk_requested(*coord, *desired_lod, noise_field);
        }

        // Mark distant chunks for unload
        self.mark_distant_for_unload(&desired);

        // Poll completed meshes (non-blocking)
        let mut results = Vec::new();
        while let Ok(result) = self.result_rx.try_recv() {
            // Update chunk state
            if let Some(chunk) = self.chunks.get_mut(&result.coord) {
                chunk.state = ChunkState::Ready;
                chunk.lod_level = result.lod_level;
            }
            results.push(result);

            // Limit results per frame
            if results.len() >= self.max_results_per_update {
                break;
            }
        }

        results
    }

    /// Compute which chunks should exist at what LOD level
    fn compute_desired_chunks(&self, camera_pos: [f32; 3]) -> HashMap<ChunkCoord, u8> {
        let mut desired = HashMap::new();
        let chunk_size = self.chunk_size();
        let view_distance = self.lod_config.max_view_distance();

        // Camera chunk position
        let cam_cx = (camera_pos[0] / chunk_size).floor() as i32;
        let cam_cy = (camera_pos[1] / chunk_size).floor() as i32;
        let cam_cz = (camera_pos[2] / chunk_size).floor() as i32;

        // View distance in chunks
        let view_chunks = (view_distance / chunk_size).ceil() as i32 + 1;

        // Iterate over potential chunks
        for dx in -view_chunks..=view_chunks {
            for dy in -view_chunks..=view_chunks {
                for dz in -view_chunks..=view_chunks {
                    let coord = ChunkCoord::new(cam_cx + dx, cam_cy + dy, cam_cz + dz);
                    let dist_sq = coord.distance_squared_to(camera_pos, chunk_size);
                    let distance = dist_sq.sqrt();

                    if distance <= view_distance {
                        let lod = self.lod_config.lod_for_distance(distance);
                        desired.insert(coord, lod);
                    }
                }
            }
        }

        desired
    }

    /// Ensure a chunk is requested at the correct LOD
    fn ensure_chunk_requested(
        &mut self,
        coord: ChunkCoord,
        desired_lod: u8,
        noise_field: &Arc<NoiseField>,
    ) {
        let needs_request = match self.chunks.get(&coord) {
            None => true, // New chunk
            Some(chunk) => {
                // Re-request if LOD changed and not already pending
                chunk.lod_level != desired_lod && chunk.state != ChunkState::Pending
            }
        };

        if needs_request {
            let transition_sides = self.compute_transition_sides(coord, desired_lod);

            let request = MeshRequest {
                coord,
                lod_level: desired_lod,
                transition_sides,
                noise_field: Arc::clone(noise_field),
                base_voxel_size: self.base_voxel_size,
                chunk_size: self.chunk_size(),
            };

            // Try to send (ignore if channel full)
            if self.request_tx.try_send(request).is_ok() {
                // Create or update chunk entry
                self.chunks
                    .entry(coord)
                    .and_modify(|c| {
                        c.state = ChunkState::Pending;
                        c.last_access_frame = self.current_frame;
                    })
                    .or_insert_with(|| {
                        let mut chunk = Chunk::new(coord, desired_lod);
                        chunk.state = ChunkState::Pending;
                        chunk.last_access_frame = self.current_frame;
                        chunk
                    });
            }
        } else if let Some(chunk) = self.chunks.get_mut(&coord) {
            // Update access time
            chunk.last_access_frame = self.current_frame;
        }
    }

    /// Compute TransitionSide flags for LOD boundaries
    ///
    /// A transition is needed on a face when the neighbor chunk
    /// has HIGHER detail (lower LOD number) than this chunk.
    fn compute_transition_sides(&self, coord: ChunkCoord, lod: u8) -> u8 {
        if lod == 0 {
            return 0; // Highest detail never needs transitions
        }

        let mut sides = 0u8;

        // Check each face's neighbor
        let neighbors = [
            (ChunkCoord::new(coord.x - 1, coord.y, coord.z), 0b000001), // LowX
            (ChunkCoord::new(coord.x + 1, coord.y, coord.z), 0b000010), // HighX
            (ChunkCoord::new(coord.x, coord.y - 1, coord.z), 0b000100), // LowY
            (ChunkCoord::new(coord.x, coord.y + 1, coord.z), 0b001000), // HighY
            (ChunkCoord::new(coord.x, coord.y, coord.z - 1), 0b010000), // LowZ
            (ChunkCoord::new(coord.x, coord.y, coord.z + 1), 0b100000), // HighZ
        ];

        for (neighbor_coord, flag) in neighbors {
            // Check if neighbor exists and has higher detail
            if let Some(neighbor) = self.chunks.get(&neighbor_coord) {
                if neighbor.lod_level < lod {
                    sides |= flag;
                }
            }
        }

        sides
    }

    /// Mark chunks outside view distance for unloading
    fn mark_distant_for_unload(&mut self, desired: &HashMap<ChunkCoord, u8>) {
        for (coord, chunk) in self.chunks.iter_mut() {
            if !desired.contains_key(coord) && chunk.state != ChunkState::Pending {
                chunk.state = ChunkState::MarkedForUnload;
            }
        }
    }

    /// Get chunks that should be unloaded
    pub fn get_unload_candidates(&self) -> Vec<(ChunkCoord, Option<i64>)> {
        self.chunks
            .iter()
            .filter(|(_, c)| c.state == ChunkState::MarkedForUnload)
            .map(|(coord, c)| (*coord, c.mesh_instance_id))
            .collect()
    }

    /// Remove a chunk from tracking
    pub fn remove_chunk(&mut self, coord: &ChunkCoord) {
        self.chunks.remove(coord);
    }

    /// Mark a chunk as active with its Godot instance ID
    pub fn mark_chunk_active(&mut self, coord: &ChunkCoord, instance_id: i64) {
        if let Some(chunk) = self.chunks.get_mut(coord) {
            chunk.state = ChunkState::Active;
            chunk.mesh_instance_id = Some(instance_id);
        }
    }

    /// Get number of tracked chunks
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Get number of active (rendered) chunks
    pub fn active_chunk_count(&self) -> usize {
        self.chunks
            .values()
            .filter(|c| c.state == ChunkState::Active)
            .count()
    }
}
```

### Step 2: Add Unit Tests

```rust
// Full path: rust/src/chunk_manager.rs (append)

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::channel::bounded;

    fn test_manager() -> (ChunkManager, Receiver<MeshRequest>, Sender<MeshResult>) {
        let (req_tx, req_rx) = bounded(64);
        let (res_tx, res_rx) = bounded(64);

        let config = LODConfig::new(64.0, 4);
        let manager = ChunkManager::new(config, 1.0, req_tx, res_rx);

        (manager, req_rx, res_tx)
    }

    #[test]
    fn test_compute_desired_chunks_at_origin() {
        let (manager, _, _) = test_manager();

        let desired = manager.compute_desired_chunks([0.0, 0.0, 0.0]);

        // Should include chunk at origin
        assert!(desired.contains_key(&ChunkCoord::new(0, 0, 0)));

        // Origin chunk should be LOD 0 (closest)
        assert_eq!(desired.get(&ChunkCoord::new(0, 0, 0)), Some(&0));
    }

    #[test]
    fn test_lod_increases_with_distance() {
        let (manager, _, _) = test_manager();

        // Camera at origin
        let desired = manager.compute_desired_chunks([0.0, 0.0, 0.0]);

        // Near chunk should be low LOD
        if let Some(&lod) = desired.get(&ChunkCoord::new(1, 0, 0)) {
            assert!(lod <= 1, "Near chunk should be LOD 0 or 1");
        }

        // Far chunk should be higher LOD
        // At chunk (10, 0, 0) with chunk_size 32, distance is ~320 units
        if let Some(&lod) = desired.get(&ChunkCoord::new(10, 0, 0)) {
            assert!(lod >= 2, "Far chunk should be LOD 2+, got {lod}");
        }
    }

    #[test]
    fn test_transition_sides_computation() {
        let (mut manager, _, _) = test_manager();

        // Add a high-detail neighbor
        let neighbor = Chunk::new(ChunkCoord::new(-1, 0, 0), 0); // LOD 0
        manager.chunks.insert(ChunkCoord::new(-1, 0, 0), neighbor);

        // Compute transitions for LOD 1 chunk at origin
        let sides = manager.compute_transition_sides(ChunkCoord::new(0, 0, 0), 1);

        // Should have transition on LowX face (neighbor at -1 is higher detail)
        assert!(sides & 0b000001 != 0, "Should have LowX transition");

        // Should NOT have transition on other faces
        assert!(sides & 0b000010 == 0, "Should not have HighX transition");
    }

    #[test]
    fn test_no_transition_at_lod_0() {
        let (manager, _, _) = test_manager();

        // LOD 0 never needs transitions (it's the highest detail)
        let sides = manager.compute_transition_sides(ChunkCoord::new(0, 0, 0), 0);
        assert_eq!(sides, 0);
    }

    #[test]
    fn test_chunk_request_sent() {
        let (mut manager, req_rx, _) = test_manager();
        let noise = Arc::new(crate::noise_field::NoiseField::new(42, 4, 0.02, 10.0, 0.0));

        // Update with camera at origin
        let _ = manager.update([0.0, 0.0, 0.0], &noise);

        // Should have sent some requests
        let mut request_count = 0;
        while req_rx.try_recv().is_ok() {
            request_count += 1;
        }

        assert!(request_count > 0, "Should have sent chunk requests");
    }

    #[test]
    fn test_result_updates_chunk_state() {
        let (mut manager, _, res_tx) = test_manager();
        let noise = Arc::new(crate::noise_field::NoiseField::new(42, 4, 0.02, 10.0, 0.0));

        // First update to create chunks
        let _ = manager.update([0.0, 0.0, 0.0], &noise);

        // Send a fake result
        let result = MeshResult {
            coord: ChunkCoord::new(0, 0, 0),
            lod_level: 0,
            vertices: vec![[0.0, 0.0, 0.0]],
            normals: vec![[0.0, 1.0, 0.0]],
            indices: vec![0],
            transition_sides: 0,
        };
        res_tx.send(result).unwrap();

        // Second update should receive it
        let results = manager.update([0.0, 0.0, 0.0], &noise);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].coord, ChunkCoord::new(0, 0, 0));
    }

    #[test]
    fn test_distant_chunks_marked_for_unload() {
        let (mut manager, _, _) = test_manager();
        let noise = Arc::new(crate::noise_field::NoiseField::new(42, 4, 0.02, 10.0, 0.0));

        // Load chunks at origin
        let _ = manager.update([0.0, 0.0, 0.0], &noise);

        // Mark one as active
        manager.mark_chunk_active(&ChunkCoord::new(0, 0, 0), 123);

        // Move camera very far away
        let _ = manager.update([10000.0, 0.0, 0.0], &noise);

        // Original chunk should be marked for unload
        let unload = manager.get_unload_candidates();
        let has_origin = unload.iter().any(|(c, _)| *c == ChunkCoord::new(0, 0, 0));
        assert!(has_origin, "Origin chunk should be marked for unload");
    }
}
```

### Step 3: Register Module in lib.rs

```rust
// Full path: rust/src/lib.rs

use godot::prelude::*;

mod chunk;
mod chunk_manager;
mod lod;
mod mesh_extraction;
mod mesh_worker;
mod noise_field;
mod terrain;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
```

### Step 4: Verify

```bash
cd rust && cargo test chunk_manager
```

Expected: All chunk manager tests pass.

## Verification Checklist

- [ ] Chunks near camera get low LOD numbers
- [ ] Chunks far from camera get high LOD numbers
- [ ] Transition sides computed when neighbor has higher detail
- [ ] LOD 0 chunks never have transitions
- [ ] Distant chunks marked for unload when camera moves

## What's Next

Walkthrough 08 integrates everything into the PixyTerrain Godot node.
