# Using bevy_tasks for parallel mesh generation in Godot 4 with gdext

**bevy_tasks works as a standalone library** and offers an elegant solution for parallel voxel mesh generation without Rayon's CPU polling issues. The crate pulls in only `bevy_platform` for platform abstractions—not the full Bevy engine—and provides scoped parallelism with proper thread parking. For your transvoxel implementation, the `scope()` API enables borrowed data parallelism that blocks until completion, while the async-executor backend ensures **8-10x lower idle CPU usage** compared to Rayon.

## Standalone usage and dependency footprint

bevy_tasks was explicitly designed as "a lighter alternative to rayon" and can be used completely independently. Its Cargo dependencies include `async-executor`, `async-task`, `futures-lite`, `concurrent-queue`, and `crossbeam-queue`—notably **no bevy_ecs or bevy_app**. The total transitive dependency count is approximately 10 crates, comparable to crossbeam.

```toml
[dependencies]
bevy_tasks = { version = "0.15", default-features = true }
```

The key feature flags are `multi_threaded` for actual parallelism and `async_executor` for the executor backend. For your Godot project, you'll want both enabled (they're on by default). The crate provides three pre-configured task pools: `ComputeTaskPool` for same-frame work, `AsyncComputeTaskPool` for multi-frame operations, and `IoTaskPool` for I/O-bound tasks. For mesh generation that must complete within **sub-16ms budgets**, ComputeTaskPool is the right choice.

Creating a standalone pool is straightforward:

```rust
use bevy_tasks::{TaskPool, TaskPoolBuilder};

let mesh_pool = TaskPoolBuilder::new()
    .num_threads(4)  // Or use logical CPU count by default
    .thread_name("MeshWorker".into())
    .build();
```

## How the API compares to Rayon

The primary API difference lies in bevy_tasks' `scope()` function versus Rayon's `par_iter()`. While Rayon excels at data-parallel collection transformations, bevy_tasks provides explicit scoped parallelism that can borrow from the stack—critical for game engines where you often need to reference shared world data.

**Parallel map/collect pattern with bevy_tasks:**
```rust
use bevy_tasks::TaskPool;

fn generate_chunk_meshes(pool: &TaskPool, chunks: &[ChunkCoord], world: &WorldData) -> Vec<MeshData> {
    pool.scope(|s| {
        for coord in chunks {
            let coord = *coord;
            s.spawn(async move {
                transvoxel_extract(world, coord)  // Can borrow world!
            });
        }
    })  // Returns Vec<MeshData>, blocks until all complete
}
```

**Equivalent Rayon pattern:**
```rust
use rayon::prelude::*;

fn generate_chunk_meshes(chunks: &[ChunkCoord], world: &WorldData) -> Vec<MeshData> {
    chunks.par_iter()
        .map(|coord| transvoxel_extract(world, *coord))
        .collect()
}
```

bevy_tasks also provides `ParallelSlice` and `ParallelSliceMut` traits for Rayon-like operations:

```rust
use bevy_tasks::ParallelSlice;

let sums: Vec<f32> = vertex_data.par_chunk_map(&pool, 1024, |_, chunk| {
    chunk.iter().map(|v| v.magnitude()).sum()
});
```

**Task priorities and time budgeting are not supported**—the library "makes no attempt to ensure fairness or ordering of spawned tasks." This is intentional: game engines prioritize simplicity and predictable completion over scheduler fairness.

## Why bevy_tasks avoids Rayon's CPU polling problem

Rayon's worker threads use `thread::yield_now()` in busy loops while waiting for work, causing constant kernel `sched_yield` syscalls even when idle. Benchmarks from Bevy's Rayon-to-bevy_tasks migration showed this pattern consuming **900% CPU in debug mode** on a 32-core system running minimal workloads.

bevy_tasks solves this via `async-executor`, which uses the `parking` crate for proper thread synchronization. Threads **truly sleep** when no work is available—they park via OS-level primitives (futex on Linux) and only wake when:
- New work is spawned
- A waker signals task completion
- Explicit ticking occurs

The benchmark results from Bevy PR #384 demonstrated:
- **Debug mode**: 900% → 110% CPU usage
- **Release mode**: 450% → 40% CPU usage

This matters enormously for games: lower idle CPU means better battery life on laptops, reduced thermal throttling, and more headroom for other systems.

## Integration architecture for Godot and gdext

The critical constraint for gdext is that **all Godot API calls must happen on the main thread**. gdext validates thread access by default and will panic if you call Godot APIs from worker threads. This isn't a limitation of bevy_tasks—it's fundamental to Godot's design.

The safe pattern separates pure Rust computation from Godot integration:

```
┌─────────────────────────┐    crossbeam channel    ┌─────────────────────────┐
│   bevy_tasks workers    │ ────────────────────►   │   Godot main thread     │
│   Pure transvoxel mesh  │                         │   _process() callback   │
│   computation, no Gd<T> │                         │   Creates ArrayMesh     │
└─────────────────────────┘                         └─────────────────────────┘
```

**Implementation for gdext:**

```rust
use bevy_tasks::TaskPool;
use crossbeam::channel::{Sender, Receiver, bounded};
use godot::prelude::*;

struct MeshResult {
    coord: ChunkCoord,
    vertices: Vec<Vector3>,
    normals: Vec<Vector3>,
    indices: Vec<i32>,
}

#[derive(GodotClass)]
#[class(base=Node3D)]
struct VoxelTerrain {
    base: Base<Node3D>,
    pool: TaskPool,
    mesh_tx: Sender<MeshResult>,
    mesh_rx: Receiver<MeshResult>,
}

#[godot_api]
impl INode3D for VoxelTerrain {
    fn init(base: Base<Node3D>) -> Self {
        let (tx, rx) = bounded(64);
        Self {
            base,
            pool: TaskPool::new(),
            mesh_tx: tx,
            mesh_rx: rx,
        }
    }
    
    fn process(&mut self, _delta: f64) {
        // Poll completed meshes (non-blocking) - runs on main thread
        while let Ok(result) = self.mesh_rx.try_recv() {
            self.commit_mesh_to_godot(result);  // Safe: main thread
        }
    }
}

impl VoxelTerrain {
    fn request_chunk_mesh(&self, coord: ChunkCoord, world: Arc<WorldData>) {
        let tx = self.mesh_tx.clone();
        
        self.pool.spawn(async move {
            // Pure Rust computation - no Godot APIs
            let mesh = transvoxel_extract(&world, coord);
            let _ = tx.send(MeshResult {
                coord,
                vertices: mesh.vertices,
                normals: mesh.normals,
                indices: mesh.indices,
            });
        }).detach();
    }
    
    fn commit_mesh_to_godot(&mut self, result: MeshResult) {
        // Safe: called from _process on main thread
        let mut mesh = ArrayMesh::new_gd();
        let mut arrays = VariantArray::new();
        // ... populate arrays with result data
        mesh.add_surface_from_arrays(/* ... */);
        
        let mut instance = MeshInstance3D::new_alloc();
        instance.set_mesh(&mesh);
        self.base_mut().add_child(&instance);
    }
}
```

**Key safety rules:**
- Never store `Gd<T>` in worker threads
- Use `try_recv()` in `_process()` to avoid blocking the game loop
- Keep mesh generation deterministic for reproducibility
- Batch commits per frame to avoid hitches (e.g., max 4 chunks per frame)

## When to use alternatives

For your specific use case (parallel transvoxel mesh generation), **three options make sense**:

| Library | Best for | Overhead |
|---------|----------|----------|
| `std::thread::scope` | Simple scoped parallelism, no dependencies | Zero runtime overhead |
| `rayon` | Collection-parallel patterns like `par_iter()` | ~15 crates, excellent performance |
| `bevy_tasks` | Game-specific patterns, low idle CPU | ~10 crates, async-executor based |

**std::thread::scope** (Rust 1.63+) is the zero-dependency option:

```rust
use std::thread;

fn generate_meshes_sync(coords: &[ChunkCoord], world: &WorldData) -> Vec<MeshData> {
    thread::scope(|s| {
        coords.iter().map(|coord| {
            s.spawn(|| transvoxel_extract(world, *coord))
        }).collect::<Vec<_>>()
    }).into_iter().map(|h| h.join().unwrap()).collect()
}
```

**Rayon** remains excellent for pure data parallelism if you configure it properly:

```rust
use rayon::ThreadPoolBuilder;

// Configure once at startup
ThreadPoolBuilder::new()
    .num_threads(4)
    .build_global()
    .unwrap();

// Then use par_iter everywhere
let meshes: Vec<_> = chunks.par_iter()
    .map(|c| transvoxel_extract(world, *c))
    .collect();
```

**Avoid Tokio** for compute-bound work—benchmarks show significant latency overhead (**8-10µs additional**) and degraded performance under CPU contention. It's designed for I/O-bound async, not mesh generation.

## Synchronous completion and error handling

For frame-bounded work that must complete before continuing, bevy_tasks' `scope()` automatically blocks:

```rust
fn process_terrain_destruction(&self, affected_chunks: &[ChunkCoord]) {
    // This blocks until ALL chunks are remeshed
    let new_meshes = self.pool.scope(|s| {
        for coord in affected_chunks {
            s.spawn(async { self.regenerate_chunk(*coord) });
        }
    });
    
    // Safe to use new_meshes immediately
    self.apply_mesh_updates(new_meshes);
}
```

**Error handling pattern:**

```rust
fn try_generate_meshes(
    pool: &TaskPool,
    coords: &[ChunkCoord],
    world: &WorldData,
) -> Vec<Result<MeshData, MeshError>> {
    pool.scope(|s| {
        for coord in coords {
            let coord = *coord;
            s.spawn(async move {
                world.get_chunk(coord)
                    .ok_or(MeshError::ChunkNotLoaded)
                    .and_then(|data| transvoxel_extract(&data).map_err(MeshError::ExtractionFailed))
            });
        }
    })
}

// Partition successes and failures
let (meshes, errors): (Vec<_>, Vec<_>) = results.into_iter().partition_result();
```

## Practical recommendations for sub-16ms chunk processing

For real-time terrain destruction at 60fps, structure your system with these principles:

1. **Scope work to frame budget**: Use `pool.scope()` for synchronous completion, but limit chunks per frame
2. **Prioritize visible chunks**: Process player-facing chunks first; queue distant chunks for `AsyncComputeTaskPool`
3. **Pre-warm the thread pool**: Initialize `TaskPool` at startup to avoid first-frame allocation stalls
4. **Profile with Tracy or similar**: bevy_tasks integrates with `tracy` via the `trace` feature flag

```rust
// Budget-aware mesh processing
const MAX_CHUNKS_PER_FRAME: usize = 4;

fn process_frame(&mut self) {
    let urgent_chunks: Vec<_> = self.dirty_chunks
        .iter()
        .filter(|c| c.distance_to_player < IMMEDIATE_RADIUS)
        .take(MAX_CHUNKS_PER_FRAME)
        .collect();
    
    if !urgent_chunks.is_empty() {
        let meshes = self.pool.scope(|s| {
            for chunk in &urgent_chunks {
                s.spawn(async { self.remesh(*chunk) });
            }
        });
        self.apply_meshes(meshes);  // Must complete this frame
    }
    
    // Queue remaining for background processing
    self.queue_background_chunks();
}
```

The combination of bevy_tasks for parallel extraction, crossbeam channels for thread communication, and gdext's main-thread requirement creates a clean, safe architecture for high-performance voxel terrain in Godot 4.