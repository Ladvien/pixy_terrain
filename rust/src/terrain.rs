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

/// Main terrain editor node - displays voxel-based terrain using transvoxel meshing
#[derive(GodotClass)]
#[class(base=Node3D, init, tool)]
pub struct PixyTerrain {
    base: Base<Node3D>,

    #[export]
    #[init(val = 42)]
    noise_seed: u32,

    #[export]
    #[init(val = 4)]
    noise_octaves: i32,

    #[export]
    #[init(val = 0.02)]
    noise_frequency: f32,

    #[export]
    #[init(val = 32.0)]
    noise_amplitude: f32,

    #[export]
    #[init(val = 0.0)]
    height_offset: f32,

    #[export]
    #[init(val = 1.0)]
    voxel_size: f32,

    #[export]
    #[init(val = 64.0)]
    lod_base_distance: f32,

    #[export]
    #[init(val = 4)]
    max_lod_level: i32,

    #[export]
    #[init(val = 0)]
    worker_threads: i32,

    #[export]
    #[init(val = false)]
    debug_wireframe: bool,

    // Parallelization
    #[init(val = None)]
    worker_pool: Option<MeshWorkerPool>,

    #[export]
    #[init(val = 256)]
    channel_capacity: i32,

    #[init(val = None)]
    chunk_manager: Option<ChunkManager>,

    #[init(val = None)]
    noise_field: Option<Arc<NoiseField>>,

    #[init(val = HashMap::new())]
    chunk_nodes: HashMap<ChunkCoord, Gd<MeshInstance3D>>,

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
    fn initialize_systems(&mut self) {
        let noise = NoiseField::new(
            self.noise_seed,
            self.noise_octaves.max(1) as usize,
            self.noise_frequency,
            self.noise_amplitude,
            self.height_offset,
        );
        let noise_arc = Arc::new(noise);
        self.noise_field = Some(Arc::clone(&noise_arc));

        let threads = if self.worker_threads <= 0 {
            0
        } else {
            self.worker_threads as usize
        };
        let worker_pool = MeshWorkerPool::new(threads, self.channel_capacity as usize);

        let lod_config = LODConfig::new(self.lod_base_distance, self.max_lod_level.max(0) as u8);
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

    fn update_terrain(&mut self) {
        let camera_pos = self.get_camera_position();

        if let Some(ref pool) = self.worker_pool {
            pool.process_requests();
        }

        let ready_meshes = if let (Some(ref mut manager), Some(ref noise)) =
            (&mut self.chunk_manager, &self.noise_field)
        {
            manager.update(camera_pos, noise)
        } else {
            Vec::new()
        };

        for mesh_result in ready_meshes {
            self.upload_mesh_to_godot(mesh_result);
        }

        self.unload_distant_chunks();
    }

    fn get_camera_position(&self) -> [f32; 3] {
        if let Some(viewport) = self.base().get_viewport() {
            if let Some(camera) = viewport.get_camera_3d() {
                let pos = camera.get_global_position();
                return [pos.x, pos.y, pos.z];
            }
        }
        let pos = self.base().get_global_position();
        [pos.x, pos.y, pos.z]
    }

    fn upload_mesh_to_godot(&mut self, result: MeshResult) {
        if result.is_empty() {
            return;
        }

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

        let coord = result.coord;
        if let Some(mut old_node) = self.chunk_nodes.remove(&coord) {
            old_node.queue_free();
        }

        let mut instance = MeshInstance3D::new_alloc();
        instance.set_mesh(&mesh);
        instance.set_name(&format!(
            "Chunk_{}_{}_{}_LOD{}",
            coord.x, coord.y, coord.z, result.lod_level
        ));

        self.base_mut().add_child(&instance);

        let instance_id = instance.instance_id().to_i64();
        self.chunk_nodes.insert(coord, instance);

        if let Some(ref mut manager) = self.chunk_manager {
            manager.mark_chunk_active(&coord, instance_id);
        }
    }

    fn unload_distant_chunks(&mut self) {
        let unload_list = if let Some(ref manager) = self.chunk_manager {
            manager.get_unload_candidates()
        } else {
            Vec::new()
        };

        for (coord, _) in unload_list {
            if let Some(mut node) = self.chunk_nodes.remove(&coord) {
                node.queue_free();
            }
            if let Some(ref mut manager) = self.chunk_manager {
                manager.remove_chunk(&coord);
            }
        }
    }
}
