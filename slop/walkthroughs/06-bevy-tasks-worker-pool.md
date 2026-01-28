# Walkthrough 06: bevy_tasks Worker Pool

**Series:** Transvoxel Noise Terrain
**Status:** Planning
**Prerequisites:** Walkthrough 05 (transvoxel extraction)

## Goal

Create a parallel mesh generation system using bevy_tasks TaskPool with crossbeam channels for thread-safe communication to Godot's main thread.

## Acceptance Criteria

- [ ] `MeshWorkerPool` creates TaskPool with configurable thread count
- [ ] `MeshRequest` struct contains all data needed for mesh generation
- [ ] Worker threads produce `MeshResult` via crossbeam channels
- [ ] No Godot API calls in worker threads
- [ ] Bounded channels prevent memory overflow

## Why bevy_tasks?

From the research docs, bevy_tasks provides:
- **Proper thread parking** - threads sleep when idle (not busy-loop polling)
- **8-10x lower CPU usage** compared to rayon when idle
- **`scope()` API** - blocks until all spawned tasks complete
- **Lightweight** - only ~10 transitive dependencies

## Architecture

```
┌─────────────────────┐        ┌─────────────────────┐
│  Main Thread        │        │  Worker Threads     │
│  (Godot API OK)     │        │  (Pure Rust only)   │
├─────────────────────┤        ├─────────────────────┤
│                     │        │                     │
│  ChunkManager       │───────►│  TaskPool.scope()   │
│  sends MeshRequest  │ channel│  spawns tasks       │
│                     │        │                     │
│  _process() polls   │◄───────│  sends MeshResult   │
│  MeshResult         │ channel│  (no Gd<T> types!)  │
│                     │        │                     │
│  Creates ArrayMesh  │        │  extract_chunk_mesh │
│  (main thread safe) │        │  (pure computation) │
└─────────────────────┘        └─────────────────────┘
```

## Steps

### Step 1: Create mesh_worker.rs Module

**File:** `rust/src/mesh_worker.rs` (new file)

```rust
// Full path: rust/src/mesh_worker.rs

//! Parallel mesh generation using bevy_tasks.
//!
//! Worker threads perform pure Rust computation - no Godot API calls!
//! Results are sent via crossbeam channels to the main thread.

use bevy_tasks::{TaskPool, TaskPoolBuilder};
use crossbeam::channel::{bounded, Receiver, Sender, TrySendError};
use std::sync::Arc;

use crate::chunk::{ChunkCoord, MeshResult};
use crate::mesh_extraction::extract_chunk_mesh;
use crate::noise_field::NoiseField;

/// Request sent from main thread to workers
pub struct MeshRequest {
    /// Chunk to generate mesh for
    pub coord: ChunkCoord,
    /// LOD level for this chunk
    pub lod_level: u8,
    /// Transition side flags
    pub transition_sides: u8,
    /// Shared noise field (Arc for thread safety)
    pub noise_field: Arc<NoiseField>,
    /// Base voxel size at LOD 0
    pub base_voxel_size: f32,
    /// World size of each chunk
    pub chunk_size: f32,
}

/// Worker pool for parallel mesh generation
pub struct MeshWorkerPool {
    /// The bevy_tasks thread pool
    pool: TaskPool,
    /// Channel for sending requests to workers
    request_tx: Sender<MeshRequest>,
    /// Channel for receiving requests (workers pull from this)
    request_rx: Receiver<MeshRequest>,
    /// Channel for sending results back to main thread
    result_tx: Sender<MeshResult>,
    /// Channel for receiving results on main thread
    result_rx: Receiver<MeshResult>,
}

impl MeshWorkerPool {
    /// Create a new worker pool
    ///
    /// # Arguments
    /// * `num_threads` - Number of worker threads (0 = auto-detect)
    pub fn new(num_threads: usize) -> Self {
        let threads = if num_threads == 0 {
            // Use half of available cores, minimum 1
            (num_cpus::get() / 2).max(1)
        } else {
            num_threads
        };

        let pool = TaskPoolBuilder::new()
            .num_threads(threads)
            .thread_name("MeshWorker".into())
            .build();

        // Bounded channels prevent memory overflow
        // 64 requests/results in flight should be plenty
        let (request_tx, request_rx) = bounded(64);
        let (result_tx, result_rx) = bounded(64);

        Self {
            pool,
            request_tx,
            request_rx,
            result_tx,
            result_rx,
        }
    }

    /// Get a clone of the request sender (for ChunkManager to use)
    pub fn request_sender(&self) -> Sender<MeshRequest> {
        self.request_tx.clone()
    }

    /// Get a clone of the result receiver (for main thread to poll)
    pub fn result_receiver(&self) -> Receiver<MeshResult> {
        self.result_rx.clone()
    }

    /// Process pending mesh requests
    ///
    /// This should be called each frame. It drains the request queue
    /// and spawns parallel tasks for each request.
    pub fn process_requests(&self) {
        // Drain available requests
        let mut requests = Vec::new();
        while let Ok(req) = self.request_rx.try_recv() {
            requests.push(req);
        }

        if requests.is_empty() {
            return;
        }

        let result_tx = self.result_tx.clone();

        // Use scope() for bounded parallelism - blocks until all complete
        self.pool.scope(|scope| {
            for request in requests {
                let tx = result_tx.clone();
                scope.spawn(async move {
                    // Pure Rust computation - no Godot API!
                    let mesh = generate_mesh_for_request(&request);

                    // Send result (ignore if channel full)
                    let _ = tx.try_send(mesh);
                });
            }
        });
    }

    /// Get number of worker threads
    pub fn thread_count(&self) -> usize {
        self.pool.thread_num()
    }
}

/// Generate mesh for a single request (runs on worker thread)
///
/// IMPORTANT: This function must NOT call any Godot APIs!
fn generate_mesh_for_request(request: &MeshRequest) -> MeshResult {
    extract_chunk_mesh(
        &request.noise_field,
        request.coord,
        request.lod_level,
        request.base_voxel_size,
        request.chunk_size,
        request.transition_sides,
    )
}

impl Default for MeshWorkerPool {
    fn default() -> Self {
        Self::new(0) // Auto-detect threads
    }
}
```

### Step 2: Add Unit Tests

```rust
// Full path: rust/src/mesh_worker.rs (append)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise_field::NoiseField;
    use std::time::Duration;

    fn test_noise() -> Arc<NoiseField> {
        Arc::new(NoiseField::new(42, 4, 0.02, 10.0, 0.0))
    }

    #[test]
    fn test_worker_pool_creation() {
        let pool = MeshWorkerPool::new(2);
        assert_eq!(pool.thread_count(), 2);
    }

    #[test]
    fn test_worker_pool_default() {
        let pool = MeshWorkerPool::default();
        assert!(pool.thread_count() >= 1);
    }

    #[test]
    fn test_send_and_receive_mesh() {
        let pool = MeshWorkerPool::new(2);
        let noise = test_noise();

        // Send a request
        let request = MeshRequest {
            coord: ChunkCoord::new(0, 0, 0),
            lod_level: 0,
            transition_sides: 0,
            noise_field: noise,
            base_voxel_size: 1.0,
            chunk_size: 32.0,
        };

        pool.request_sender()
            .send(request)
            .expect("Should send request");

        // Process requests
        pool.process_requests();

        // Should receive result
        let result = pool
            .result_receiver()
            .recv_timeout(Duration::from_secs(5))
            .expect("Should receive result");

        assert_eq!(result.coord, ChunkCoord::new(0, 0, 0));
        assert_eq!(result.lod_level, 0);
    }

    #[test]
    fn test_multiple_requests_parallel() {
        let pool = MeshWorkerPool::new(4);
        let noise = test_noise();

        // Send multiple requests
        let coords = [
            ChunkCoord::new(0, 0, 0),
            ChunkCoord::new(1, 0, 0),
            ChunkCoord::new(0, 0, 1),
            ChunkCoord::new(1, 0, 1),
        ];

        for coord in &coords {
            let request = MeshRequest {
                coord: *coord,
                lod_level: 0,
                transition_sides: 0,
                noise_field: Arc::clone(&noise),
                base_voxel_size: 1.0,
                chunk_size: 32.0,
            };
            pool.request_sender().send(request).unwrap();
        }

        // Process all requests
        pool.process_requests();

        // Should receive all results
        let mut received = Vec::new();
        while let Ok(result) = pool.result_receiver().try_recv() {
            received.push(result.coord);
        }

        assert_eq!(received.len(), 4, "Should receive all 4 results");
    }

    #[test]
    fn test_bounded_channels_dont_block() {
        let pool = MeshWorkerPool::new(1);
        let noise = test_noise();

        // Try to overflow request channel (capacity = 64)
        let mut sent = 0;
        for i in 0..100 {
            let request = MeshRequest {
                coord: ChunkCoord::new(i, 0, 0),
                lod_level: 0,
                transition_sides: 0,
                noise_field: Arc::clone(&noise),
                base_voxel_size: 1.0,
                chunk_size: 32.0,
            };

            match pool.request_sender().try_send(request) {
                Ok(_) => sent += 1,
                Err(TrySendError::Full(_)) => break,
                Err(TrySendError::Disconnected(_)) => panic!("Channel disconnected"),
            }
        }

        // Should have sent up to channel capacity
        assert!(sent <= 64, "Should stop at channel capacity");
        assert!(sent > 0, "Should have sent some requests");
    }
}
```

### Step 3: Register Module in lib.rs

```rust
// Full path: rust/src/lib.rs

use godot::prelude::*;

mod chunk;
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
cd rust && cargo test mesh_worker
```

Expected: All worker pool tests pass.

## Verification Checklist

- [ ] Worker pool creates specified number of threads
- [ ] Requests can be sent via channel
- [ ] Results are received after processing
- [ ] Multiple requests process in parallel
- [ ] Bounded channels prevent overflow (don't block)

## Key Patterns

### Non-blocking Send/Receive
```rust
// Main thread: non-blocking poll
while let Ok(result) = result_rx.try_recv() {
    // Process result
}
```

### Scoped Parallelism
```rust
pool.scope(|s| {
    for item in items {
        s.spawn(async move {
            // Process item
        });
    }
}); // Blocks until all complete
```

### Thread Safety via Arc
```rust
let noise: Arc<NoiseField> = Arc::new(NoiseField::new(...));
// Can clone Arc safely across threads
let noise_clone = Arc::clone(&noise);
```

## What's Next

Walkthrough 07 creates the ChunkManager that coordinates LOD selection and mesh requests.
