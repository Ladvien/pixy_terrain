# Walkthrough 08: Godot Integration

**Series:** Transvoxel Noise Terrain
**Status:** Planning
**Prerequisites:** Walkthrough 07 (chunk manager)

## Goal

Integrate all components into the PixyTerrain Godot node: initialize systems, process each frame, upload meshes to ArrayMesh, and manage MeshInstance3D children.

## Acceptance Criteria

- [ ] PixyTerrain exports noise and LOD parameters
- [ ] `_ready()` initializes worker pool, chunk manager, noise field
- [ ] `_process()` updates terrain based on camera position
- [ ] Meshes are uploaded to Godot as MeshInstance3D children
- [ ] Distant chunks are freed when camera moves
- [ ] `regenerate()` function rebuilds terrain

## Critical Constraint: Main Thread Only

All Godot API calls (creating nodes, setting meshes, adding children) MUST happen on the main thread. This is why we:
1. Generate mesh data on worker threads (pure Rust)
2. Send raw arrays via channels
3. Convert to Godot types in `_process()` (main thread)

## Steps

### Step 1: Update PixyTerrain Struct

**File:** `rust/src/terrain.rs` (replace entire file)

```rust
// Full path: rust/src/terrain.rs

//! PixyTerrain - Voxel terrain node using transvoxel meshing.
//!
//! This is the main Godot node that coordinates terrain generation.
//! All Godot API calls happen here on the main thread.

use godot::classes::mesh::PrimitiveType;
use godot::classes::rendering_server::ArrayType;
use godot::classes::{ArrayMesh, MeshInstance3D, Node3D};
use godot::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use crate::chunk::{ChunkCoord, MeshResult};
use crate::chunk_manager::ChunkManager;
use crate::lod::LODConfig;
use crate::mesh_worker::MeshWorkerPool;
use crate::noise_field::NoiseField;

type VariantArray = Array<Variant>;

/// Main terrain node - generates voxel terrain using transvoxel meshing
#[derive(GodotClass)]
#[class(base=Node3D, init, tool)]
pub struct PixyTerrain {
    base: Base<Node3D>,

    // === Noise Parameters ===
    /// Random seed for terrain generation
    #[export]
    #[init(val = 42)]
    noise_seed: u32,

    /// Number of noise octaves (more = more detail)
    #[export]
    #[init(val = 4)]
    noise_octaves: i32,

    /// Noise frequency (higher = smaller features)
    #[export]
    #[init(val = 0.02)]
    noise_frequency: f32,

    /// Height variation amplitude
    #[export]
    #[init(val = 32.0)]
    noise_amplitude: f32,

    /// Base terrain height
    #[export]
    #[init(val = 0.0)]
    height_offset: f32,

    // === LOD Parameters ===
    /// Size of each voxel at LOD 0
    #[export]
    #[init(val = 1.0)]
    voxel_size: f32,

    /// Distance threshold for LOD 0
    #[export]
    #[init(val = 64.0)]
    lod_base_distance: f32,

    /// Maximum LOD level
    #[export]
    #[init(val = 4)]
    max_lod_level: i32,

    /// Number of worker threads (0 = auto)
    #[export]
    #[init(val = 0)]
    worker_threads: i32,

    // === Debug ===
    /// Show debug wireframe
    #[export]
    #[init(val = false)]
    debug_wireframe: bool,

    // === Internal State (not exported) ===
    #[init(val = None)]
    worker_pool: Option<MeshWorkerPool>,

    #[init(val = None)]
    chunk_manager: Option<ChunkManager>,

    #[init(val = None)]
    noise_field: Option<Arc<NoiseField>>,

    /// Map of chunk coords to MeshInstance3D nodes
    #[init(val = HashMap::new())]
    chunk_nodes: HashMap<ChunkCoord, Gd<MeshInstance3D>>,

    /// Whether systems are initialized
    #[init(val = false)]
    initialized: bool,
}

#[godot_api]
impl INode3D for PixyTerrain {
    fn ready(&mut self) {
        godot_print!("PixyTerrain: Initializing...");
        self.initialize_systems();
    }

    fn process(&mut self, _delta: f64) {
        if !self.initialized {
            return;
        }
        self.update_terrain();
    }
}

impl PixyTerrain {
    /// Initialize all terrain systems
    fn initialize_systems(&mut self) {
        // Create noise field
        let noise = NoiseField::new(
            self.noise_seed,
            self.noise_octaves.max(1) as usize,
            self.noise_frequency,
            self.noise_amplitude,
            self.height_offset,
        );
        let noise_arc = Arc::new(noise);
        self.noise_field = Some(Arc::clone(&noise_arc));

        // Create worker pool
        let threads = if self.worker_threads <= 0 {
            0 // Auto-detect
        } else {
            self.worker_threads as usize
        };
        let worker_pool = MeshWorkerPool::new(threads);

        // Create chunk manager
        let lod_config = LODConfig::new(
            self.lod_base_distance,
            self.max_lod_level.max(0) as u8,
        );
        let chunk_manager = ChunkManager::new(
            lod_config,
            self.voxel_size,
            worker_pool.request_sender(),
            worker_pool.result_receiver(),
        );

        self.worker_pool = Some(worker_pool);
        self.chunk_manager = Some(chunk_manager);
        self.initialized = true;

        godot_print!(
            "PixyTerrain: Ready (seed={}, {} worker threads)",
            self.noise_seed,
            self.worker_pool.as_ref().map_or(0, |p| p.thread_count())
        );
    }

    /// Update terrain each frame
    fn update_terrain(&mut self) {
        // Get camera position
        let camera_pos = self.get_camera_position();

        // Process worker requests
        if let Some(ref pool) = self.worker_pool {
            pool.process_requests();
        }

        // Update chunk manager
        let ready_meshes = if let (Some(ref mut manager), Some(ref noise)) =
            (&mut self.chunk_manager, &self.noise_field)
        {
            manager.update(camera_pos, noise)
        } else {
            Vec::new()
        };

        // Upload ready meshes (main thread safe)
        for mesh_result in ready_meshes {
            self.upload_mesh_to_godot(mesh_result);
        }

        // Unload distant chunks
        self.unload_distant_chunks();
    }

    /// Get current camera position
    fn get_camera_position(&self) -> [f32; 3] {
        if let Some(viewport) = self.base().get_viewport() {
            if let Some(camera) = viewport.get_camera_3d() {
                let pos = camera.get_global_position();
                return [pos.x, pos.y, pos.z];
            }
        }
        // Fallback to node position
        let pos = self.base().get_global_position();
        [pos.x, pos.y, pos.z]
    }

    /// Upload mesh result to Godot (MAIN THREAD ONLY)
    fn upload_mesh_to_godot(&mut self, result: MeshResult) {
        if result.is_empty() {
            return;
        }

        // Convert to Godot types
        let vertices = PackedVector3Array::from(
            &result
                .vertices
                .iter()
                .map(|v| Vector3::new(v[0], v[1], v[2]))
                .collect::<Vec<_>>()[..],
        );

        let normals = PackedVector3Array::from(
            &result
                .normals
                .iter()
                .map(|n| Vector3::new(n[0], n[1], n[2]))
                .collect::<Vec<_>>()[..],
        );

        let indices = PackedInt32Array::from(&result.indices[..]);

        // Build ArrayMesh
        let mut mesh = ArrayMesh::new_gd();
        let num_arrays = ArrayType::MAX.ord() as usize;
        let mut arrays: VariantArray = VariantArray::new();

        for i in 0..num_arrays {
            if i == ArrayType::VERTEX.ord() as usize {
                arrays.push(&vertices.to_variant());
            } else if i == ArrayType::NORMAL.ord() as usize {
                arrays.push(&normals.to_variant());
            } else if i == ArrayType::INDEX.ord() as usize {
                arrays.push(&indices.to_variant());
            } else {
                arrays.push(&Variant::nil());
            }
        }

        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);

        // Remove old mesh instance if exists
        let coord = result.coord;
        if let Some(old_node) = self.chunk_nodes.remove(&coord) {
            old_node.queue_free();
        }

        // Create new mesh instance
        let mut instance = MeshInstance3D::new_alloc();
        instance.set_mesh(&mesh);
        instance.set_name(&format!(
            "Chunk_{}_{}_{}_LOD{}",
            coord.x, coord.y, coord.z, result.lod_level
        ));

        // Add as child
        self.base_mut().add_child(&instance);

        // Track the node
        let instance_id = instance.instance_id().to_i64();
        self.chunk_nodes.insert(coord, instance);

        // Mark active in manager
        if let Some(ref mut manager) = self.chunk_manager {
            manager.mark_chunk_active(&coord, instance_id);
        }
    }

    /// Unload chunks that are too far away
    fn unload_distant_chunks(&mut self) {
        let unload_list = if let Some(ref manager) = self.chunk_manager {
            manager.get_unload_candidates()
        } else {
            Vec::new()
        };

        for (coord, _) in unload_list {
            if let Some(node) = self.chunk_nodes.remove(&coord) {
                node.queue_free();
            }
            if let Some(ref mut manager) = self.chunk_manager {
                manager.remove_chunk(&coord);
            }
        }
    }
}

#[godot_api]
impl PixyTerrain {
    /// Regenerate terrain with current parameters
    #[func]
    fn regenerate(&mut self) {
        godot_print!("PixyTerrain: Regenerating...");

        // Clear existing chunks
        for (_, node) in self.chunk_nodes.drain() {
            node.queue_free();
        }

        // Reinitialize
        self.worker_pool = None;
        self.chunk_manager = None;
        self.noise_field = None;
        self.initialized = false;

        self.initialize_systems();
    }

    /// Clear all terrain
    #[func]
    fn clear(&mut self) {
        for (_, node) in self.chunk_nodes.drain() {
            node.queue_free();
        }
        self.chunk_manager = None;
        self.worker_pool = None;
        self.noise_field = None;
        self.initialized = false;
        godot_print!("PixyTerrain: Cleared");
    }

    /// Get number of loaded chunks
    #[func]
    fn get_chunk_count(&self) -> i32 {
        self.chunk_nodes.len() as i32
    }

    /// Get number of active (rendered) chunks
    #[func]
    fn get_active_chunk_count(&self) -> i32 {
        self.chunk_manager
            .as_ref()
            .map_or(0, |m| m.active_chunk_count()) as i32
    }

    /// Signal emitted when terrain is ready
    #[signal]
    fn terrain_ready();

    /// Signal emitted when a chunk is loaded
    #[signal]
    fn chunk_loaded(x: i32, y: i32, z: i32, lod: i32);
}
```

### Step 2: Verify Module Registration

**File:** `rust/src/lib.rs` (should already be correct)

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

### Step 3: Build and Test

```bash
cd rust && cargo build
```

Expected: Compiles without errors.

### Step 4: Test in Godot

1. Open the Godot project in `godot/`
2. Open the test scene (`scenes/test_scene.tscn`)
3. Select the PixyTerrain node
4. In the Inspector, verify all exports appear:
   - Noise Seed, Octaves, Frequency, Amplitude, Height Offset
   - Voxel Size, LOD Base Distance, Max LOD Level
   - Worker Threads, Debug Wireframe
5. Run the scene (F5)
6. Move the camera around
7. Verify:
   - Terrain meshes appear
   - Chunks load/unload as camera moves
   - No crashes or errors in output

### Step 5: Test regenerate()

In Godot:
1. Select PixyTerrain node
2. Change `noise_seed` to a different value
3. Call `regenerate()` from script or editor button
4. Verify terrain changes

## Verification Checklist

- [ ] `cargo build` succeeds
- [ ] All exports visible in Godot Inspector
- [ ] Terrain appears when scene runs
- [ ] Camera movement triggers chunk loading/unloading
- [ ] `regenerate()` rebuilds terrain
- [ ] `clear()` removes all chunks
- [ ] No errors in Godot output
- [ ] CPU usage low when camera stationary

## Debugging Tips

### No terrain appears?
- Check camera position - may be inside terrain (negative SDF)
- Try setting `height_offset = -50` to lower terrain
- Check Godot output for error messages

### Crashes on startup?
- Verify all Cargo.toml dependencies are correct
- Check `.gdextension` file points to correct library path
- Try `cargo build --release` for release build

### High CPU usage?
- This is expected briefly while chunks generate
- Should settle down when camera stops moving
- If persistent, check bevy_tasks is properly parking threads

### Chunks pop in suddenly?
- This is normal - no LOD blending implemented yet
- Future enhancement: geomorphing between LOD levels

## Complete File Structure

After all walkthroughs:
```
rust/
├── Cargo.toml           # Dependencies
└── src/
    ├── lib.rs           # Extension entry
    ├── chunk.rs         # ChunkCoord, MeshResult, ChunkState
    ├── lod.rs           # LODConfig
    ├── noise_field.rs   # NoiseField, NoiseDataField
    ├── mesh_extraction.rs # extract_chunk_mesh
    ├── mesh_worker.rs   # MeshWorkerPool
    ├── chunk_manager.rs # ChunkManager
    └── terrain.rs       # PixyTerrain node
```

## What's Next

The terrain system is now functional. Future enhancements could include:
- Terrain editing (set_voxel)
- Material/texture support
- Collision shapes
- Geomorphing for smooth LOD transitions
- Terrain streaming/serialization
