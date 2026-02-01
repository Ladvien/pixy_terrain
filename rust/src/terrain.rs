use godot::classes::mesh::PrimitiveType;
use godot::classes::rendering_server::ArrayType;
use godot::classes::{ArrayMesh, MeshInstance3D, Node3D};
use godot::classes::{Shader, ShaderMaterial};
use godot::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::chunk::{ChunkCoord, MeshResult};
use crate::chunk_manager::ChunkManager;
use crate::lod::LODConfig;
use crate::mesh_postprocess::MeshPostProcessor;
use crate::mesh_worker::MeshWorkerPool;
use crate::noise_field::NoiseField;
type VariantArray = Array<Variant>;

/// Main terrain editor node - displays voxel-based terrain using transvoxel meshing
#[derive(GodotClass)]
#[class(base=Node3D, init, tool)]
pub struct PixyTerrain {
    base: Base<Node3D>,

    // ═════════════════════════════════════════════════
    // Map Settings
    // ═════════════════════════════════════════════════
    #[export_group(name = "Map Settings")]
    #[export]
    #[init(val = 10)]
    map_width_x: i32,

    #[export]
    #[init(val = 4)]
    map_height_y: i32,

    #[export]
    #[init(val = 10)]
    map_width_z: i32,

    #[export]
    #[init(val = 1.0)]
    voxel_size: f32,

    // ═════════════════════════════════════════════
    // Terrain Generation
    // ═════════════════════════════════════════════
    #[export_group(name = "Terrain Generation")]
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

    // ══════════════════════════════════════════════
    // LOD Settings
    // ══════════════════════════════════════════════
    #[export_group(name = "LOD Settings")]
    #[export]
    #[init(val = 64.0)]
    lod_base_distance: f32,

    #[export]
    #[init(val = 4)]
    max_lod_level: i32,

    #[export]
    #[init(val = 32)]
    chunk_subdivisions: i32,

    // ═══════════════════════════════════════════════
    // Parallelization
    // ═══════════════════════════════════════════════
    #[export_group(name = "Parallelization")]
    #[export]
    #[init(val = 0)]
    worker_threads: i32,

    #[export]
    #[init(val = 256)]
    channel_capacity: i32,

    #[export]
    #[init(val = 8)]
    max_uploads_per_frame: i32,

    #[export]
    #[init(val = 512)]
    max_pending_uploads: i32,

    // ══════════════════════════════════════════════
    // Terrain Floor
    // ══════════════════════════════════════════════
    #[export_group(name = "Terrain Floor")]
    #[export]
    #[init(val = 32.0)]
    terrain_floor_y: f32,

    // ══════════════════════════════════════════════════
    // Mesh Post-Processing
    // ══════════════════════════════════════════════════
    #[export_group(name = "Mesh Post-Processing")]
    #[export]
    #[init(val = 0.001)]
    weld_epsilon: f32,

    #[export]
    #[init(val = 45.0)]
    normal_angle_threshold: f32,

    #[export]
    #[init(val = 0)]
    decimation_target_triangles: i32,

    #[export]
    #[init(val = false)]
    auto_process_on_export: bool,

    // ════════════════════════════════════════════════
    // Debug
    // ════════════════════════════════════════════════
    #[export_group(name = "Debug")]
    #[export]
    #[init(val = false)]
    debug_wireframe: bool,

    #[export]
    #[init(val = false)]
    debug_logging: bool,

    #[export]
    #[init(val = false)]
    debug_material: bool,

    // ═════════════════════════════════════════════════
    // Internal State (not exported)
    // ═════════════════════════════════════════════════
    #[init(val = None)]
    worker_pool: Option<MeshWorkerPool>,

    #[init(val = VecDeque::new())]
    pending_uploads: VecDeque<MeshResult>,

    /// Storage for all uploaded mesh data (for post-processing)
    #[init(val = Vec::new())]
    uploaded_meshes: Vec<MeshResult>,

    #[init(val = 0)]
    meshes_dropped: u64,

    #[init(val = 0)]
    meshes_uploaded: u64,

    #[init(val = None)]
    chunk_manager: Option<ChunkManager>,

    #[init(val = None)]
    noise_field: Option<Arc<NoiseField>>,

    #[init(val = HashMap::new())]
    chunk_nodes: HashMap<ChunkCoord, Gd<MeshInstance3D>>,

    #[init(val = false)]
    initialized: bool,

    #[init(val = None)]
    cached_material: Option<Gd<ShaderMaterial>>,
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
        // Create debug material
        if self.debug_material {
            let mut shader = Shader::new_gd();
            shader.set_code(include_str!("shaders/checkerboard.gdshader"));
            let mut debug_material = ShaderMaterial::new_gd();
            debug_material.set_shader(&shader);
            self.cached_material = Some(debug_material);
        } else {
            self.cached_material = None;
        }

        let chunk_size = self.voxel_size * self.chunk_subdivisions as f32;

        // Box bounds at origin (no offset) for SDF enclosure
        let box_min = [0.0, 0.0, 0.0];
        let box_max = [
            self.map_width_x.max(1) as f32 * chunk_size,
            self.map_height_y.max(1) as f32 * chunk_size,
            self.map_width_z.max(1) as f32 * chunk_size,
        ];

        // Create noise field with SDF enclosure for watertight mesh generation
        let noise = NoiseField::new(
            self.noise_seed,
            self.noise_octaves.max(1) as usize,
            self.noise_frequency,
            self.noise_amplitude,
            self.height_offset,
            self.terrain_floor_y,
            Some((box_min, box_max)),
        );

        let noise_arc = Arc::new(noise);
        self.noise_field = Some(Arc::clone(&noise_arc));

        let threads = if self.worker_threads <= 0 {
            0
        } else {
            self.worker_threads as usize
        };
        let worker_pool = MeshWorkerPool::new(threads, self.channel_capacity as usize);

        let lod_config = LODConfig::new(
            self.lod_base_distance,
            self.max_lod_level.max(0) as u8,
            self.chunk_subdivisions.max(1) as u32,
        );
        let chunk_manager = ChunkManager::new(
            lod_config,
            self.voxel_size,
            worker_pool.request_sender(),
            worker_pool.result_receiver(),
            self.map_width_x,
            self.map_height_y,
            self.map_width_z,
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

        // Collect new meshes from workers
        let new_meshes = if let (Some(ref mut manager), Some(ref noise)) =
            (&mut self.chunk_manager, &self.noise_field)
        {
            manager.update(camera_pos, noise)
        } else {
            Vec::new()
        };

        // Add to back of queue (FIFO)
        for mesh in new_meshes {
            if self.pending_uploads.len() < self.max_pending_uploads as usize {
                self.pending_uploads.push_back(mesh);
            } else {
                // Buffer full, drop oldest to make room
                if let Some(_old) = self.pending_uploads.pop_front() {
                    self.meshes_dropped += 1;
                }
                self.pending_uploads.push_back(mesh);
            }
        }

        // Upload limited number per from (from front, oldest firsst)
        let max_uploads = self.max_uploads_per_frame.max(1) as usize;
        for _ in 0..max_uploads {
            if let Some(mesh_result) = self.pending_uploads.pop_front() {
                // Store a copy for post-processing before uploading
                self.uploaded_meshes.push(mesh_result.clone());
                self.upload_mesh_to_godot(mesh_result);
                self.meshes_uploaded += 1;
            } else {
                break;
            }
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

        // Apply debug material if cached
        if let Some(ref mat) = self.cached_material {
            instance.set_surface_override_material(0, mat);
        }

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

#[godot_api]
impl PixyTerrain {
    #[func]
    pub fn regenerate(&mut self) {
        self.clear();
        self.initialize_systems();
    }

    /// Debug function: print info about loaded chunks including boundary chunks
    #[func]
    pub fn debug_chunks(&self) {
        godot_print!("=== PixyTerrain Chunk Debug ===");
        godot_print!("Chunk count: {}", self.chunk_nodes.len());

        // Count boundary chunks
        let mut x_neg1 = 0;
        let mut z_neg1 = 0;
        let mut y_neg1 = 0;
        let mut x_max = 0;
        let mut z_max = 0;

        for (coord, _) in &self.chunk_nodes {
            if coord.x == -1 {
                x_neg1 += 1;
            }
            if coord.z == -1 {
                z_neg1 += 1;
            }
            if coord.y == -1 {
                y_neg1 += 1;
            }
            if coord.x == self.map_width_x {
                x_max += 1;
            }
            if coord.z == self.map_width_z {
                z_max += 1;
            }
        }

        godot_print!("Boundary chunks:");
        godot_print!("  X=-1 (wall): {}", x_neg1);
        godot_print!("  Z=-1 (wall): {}", z_neg1);
        godot_print!("  Y=-1 (floor): {}", y_neg1);
        godot_print!("  X=max (wall): {}", x_max);
        godot_print!("  Z=max (wall): {}", z_max);

        let cam_pos = self.get_camera_position();
        godot_print!("Camera position: [{:.1}, {:.1}, {:.1}]", cam_pos[0], cam_pos[1], cam_pos[2]);

        let chunk_size = self.voxel_size * self.chunk_subdivisions as f32;
        godot_print!("Chunk size: {}", chunk_size);
        godot_print!(
            "Map bounds: [0,0,0] to [{}, {}, {}]",
            self.map_width_x as f32 * chunk_size,
            self.map_height_y as f32 * chunk_size,
            self.map_width_z as f32 * chunk_size
        );

        // Check if SDF enclosure is active
        if let Some(ref noise) = self.noise_field {
            let bounds = noise.get_box_bounds();
            godot_print!("SDF box bounds: {:?}", bounds);

            if bounds.is_some() {
                // Sample SDF at key points to verify enclosure
                let mid_x = self.map_width_x as f32 * chunk_size / 2.0;
                let mid_z = self.map_width_z as f32 * chunk_size / 2.0;

                // Test floor at Y=0
                let sdf_floor = noise.sample(mid_x, 0.0, mid_z);
                godot_print!("SDF at floor (Y=0): {:.2} (should be ~0)", sdf_floor);

                // Test X=0 wall at Y=16 (below terrain)
                let sdf_wall_x0 = noise.sample(0.0, 16.0, mid_z);
                godot_print!("SDF at X=0 wall (Y=16): {:.2} (should be ~0)", sdf_wall_x0);

                // Test inside terrain (should be negative)
                let sdf_inside = noise.sample(mid_x, 16.0, mid_z);
                godot_print!("SDF inside terrain (Y=16): {:.2} (should be <0)", sdf_inside);
            } else {
                godot_print!("WARNING: Box bounds are None! Walls/floor won't generate.");
            }
        } else {
            godot_print!("WARNING: NoiseField is None!");
        }

        godot_print!("Pending uploads: {}", self.pending_uploads.len());
        godot_print!("Meshes uploaded: {}", self.meshes_uploaded);
        godot_print!("Meshes dropped: {}", self.meshes_dropped);
    }

    #[func]
    pub fn clear(&mut self) {
        // 1. Stop worker pool first (prevent new results)
        if let Some(ref mut pool) = self.worker_pool {
            pool.shutdown()
        }

        // 2. Clear chunk manager's internal state
        if let Some(ref mut manager) = self.chunk_manager {
            manager.clear_all_chunks();
        }

        // 3. Free chunk mesh nodes
        let nodes: Vec<_> = self.chunk_nodes.drain().map(|(_, node)| node).collect();
        for mut node in nodes {
            if node.is_instance_valid() {
                self.base_mut().remove_child(&node);
                node.queue_free();
            }
        }

        // 4. Clear pending uploads buffer and uploaded mesh storage
        self.pending_uploads.clear();
        self.uploaded_meshes.clear();

        // 5. Drop systems
        self.worker_pool = None;
        self.chunk_manager = None;
        self.noise_field = None;
        self.initialized = false;
    }

    /// Merge all chunks into single watertight mesh and apply to scene
    /// Returns the combined ArrayMesh ready for use or export
    #[func]
    pub fn merge_and_export(&mut self) -> Gd<ArrayMesh> {
        let chunks = self.collect_chunk_meshes();

        let target = if self.decimation_target_triangles > 0 {
            Some(self.decimation_target_triangles as usize)
        } else {
            None
        };

        let processor = MeshPostProcessor::new(
            self.weld_epsilon,
            self.normal_angle_threshold,
            target,
        );

        let combined = processor.process(&chunks);
        let mesh = self.combined_mesh_to_godot(&combined);

        // Apply to scene
        self.apply_combined_mesh(mesh.clone(), "MergedTerrain");

        godot_print!(
            "PixyTerrain: Merged {} chunks - {} vertices, {} triangles",
            chunks.len(),
            combined.vertex_count(),
            combined.triangle_count()
        );

        mesh
    }

    /// Weld vertices at seams and apply to scene
    #[func]
    pub fn weld_seams(&mut self) {
        let chunks = self.collect_chunk_meshes();

        let processor = MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, None);

        let mut combined = processor.merge_chunks(&chunks);
        processor.weld_vertices(&mut combined);

        let mesh = self.combined_mesh_to_godot(&combined);
        self.apply_combined_mesh(mesh, "WeldedTerrain");

        godot_print!(
            "PixyTerrain: Welded seams - {} vertices, {} triangles",
            combined.vertex_count(),
            combined.triangle_count()
        );
    }

    /// Decimate mesh to target triangle count and apply to scene
    #[func]
    pub fn decimate_mesh(&mut self, target_triangles: i32) -> Gd<ArrayMesh> {
        let chunks = self.collect_chunk_meshes();

        let target = if target_triangles > 0 {
            Some(target_triangles as usize)
        } else {
            None
        };

        let processor = MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, target);

        let combined = processor.process(&chunks);
        let mesh = self.combined_mesh_to_godot(&combined);

        // Apply to scene
        self.apply_combined_mesh(mesh.clone(), "DecimatedTerrain");

        godot_print!(
            "PixyTerrain: Decimated to {} triangles ({} vertices)",
            combined.triangle_count(),
            combined.vertex_count()
        );

        mesh
    }

    /// Recompute normals with angle threshold and apply to scene
    #[func]
    pub fn recompute_normals(&mut self) {
        let chunks = self.collect_chunk_meshes();

        let processor = MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, None);

        let mut combined = processor.merge_chunks(&chunks);
        processor.weld_vertices(&mut combined);
        processor.recompute_normals(&mut combined);

        let mesh = self.combined_mesh_to_godot(&combined);
        self.apply_combined_mesh(mesh, "NormalsRecomputed");

        godot_print!(
            "PixyTerrain: Recomputed normals for {} vertices",
            combined.vertex_count()
        );
    }

    /// Export watertight mesh to OBJ file
    #[func]
    pub fn export_mesh(&mut self, path: GString) -> bool {
        let chunks = self.collect_chunk_meshes();

        let target = if self.decimation_target_triangles > 0 {
            Some(self.decimation_target_triangles as usize)
        } else {
            None
        };

        let processor = MeshPostProcessor::new(
            self.weld_epsilon,
            self.normal_angle_threshold,
            target,
        );

        let combined = processor.process(&chunks);

        // Export to OBJ format
        let path_str = path.to_string();
        match self.write_obj(&combined, &path_str) {
            Ok(()) => {
                godot_print!("PixyTerrain: Exported mesh to {}", path_str);
                true
            }
            Err(e) => {
                godot_error!("PixyTerrain: Failed to export mesh: {}", e);
                false
            }
        }
    }

    /// Get statistics about the current mesh
    #[func]
    pub fn get_mesh_stats(&self) -> godot::prelude::VarDictionary {
        let mut dict = godot::prelude::VarDictionary::new();

        let mut total_vertices = 0usize;
        let mut total_triangles = 0usize;

        for result in self.pending_uploads.iter() {
            total_vertices += result.vertices.len();
            total_triangles += result.indices.len() / 3;
        }

        dict.set("chunk_count", self.chunk_nodes.len() as i32);
        dict.set("pending_uploads", self.pending_uploads.len() as i32);
        dict.set("pending_vertices", total_vertices as i64);
        dict.set("pending_triangles", total_triangles as i64);
        dict.set("meshes_uploaded", self.meshes_uploaded as i64);
        dict.set("meshes_dropped", self.meshes_dropped as i64);

        dict
    }
}

impl PixyTerrain {
    /// Apply a combined/processed mesh to the scene, replacing all chunk meshes
    fn apply_combined_mesh(&mut self, mesh: Gd<ArrayMesh>, name: &str) {
        // Clear all existing chunk nodes (keep box geometry)
        let nodes: Vec<_> = self.chunk_nodes.drain().map(|(_, node)| node).collect();
        for mut node in nodes {
            if node.is_instance_valid() {
                self.base_mut().remove_child(&node);
                node.queue_free();
            }
        }

        // Create new mesh instance for the combined mesh
        let mut instance = MeshInstance3D::new_alloc();
        instance.set_mesh(&mesh);
        instance.set_name(name);

        // Apply material if cached
        if let Some(ref mat) = self.cached_material {
            instance.set_surface_override_material(0, mat);
        }

        self.base_mut().add_child(&instance);

        // Store in chunk_nodes with a special coord so it can be cleared on regenerate
        let combined_coord = ChunkCoord { x: 0, y: 0, z: 0 };
        self.chunk_nodes.insert(combined_coord, instance);

        // Clear mesh storage since we've merged everything
        self.pending_uploads.clear();
        self.uploaded_meshes.clear();
    }

    /// Collect mesh data from all uploaded chunks
    fn collect_chunk_meshes(&self) -> Vec<MeshResult> {
        // Use uploaded_meshes which stores all mesh data after upload
        self.uploaded_meshes.iter().cloned().collect()
    }

    /// Convert CombinedMesh to Godot ArrayMesh
    fn combined_mesh_to_godot(
        &self,
        combined: &crate::mesh_postprocess::CombinedMesh,
    ) -> Gd<ArrayMesh> {
        let vertices = PackedVector3Array::from(
            &combined
                .vertices
                .iter()
                .map(|v| Vector3::new(v[0], v[1], v[2]))
                .collect::<Vec<_>>()[..],
        );

        let normals = PackedVector3Array::from(
            &combined
                .normals
                .iter()
                .map(|n| Vector3::new(n[0], n[1], n[2]))
                .collect::<Vec<_>>()[..],
        );

        let indices = PackedInt32Array::from(
            &combined
                .indices
                .iter()
                .map(|&i| i as i32)
                .collect::<Vec<_>>()[..],
        );

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
        mesh
    }

    /// Write mesh to OBJ file format
    fn write_obj(
        &self,
        mesh: &crate::mesh_postprocess::CombinedMesh,
        path: &str,
    ) -> Result<(), std::io::Error> {
        use std::fs::File;
        use std::io::Write;

        let mut file = File::create(path)?;

        writeln!(file, "# Exported from PixyTerrain")?;
        writeln!(file, "# Vertices: {}", mesh.vertices.len())?;
        writeln!(file, "# Triangles: {}", mesh.indices.len() / 3)?;
        writeln!(file)?;

        // Write vertices
        for v in &mesh.vertices {
            writeln!(file, "v {} {} {}", v[0], v[1], v[2])?;
        }

        writeln!(file)?;

        // Write normals
        for n in &mesh.normals {
            writeln!(file, "vn {} {} {}", n[0], n[1], n[2])?;
        }

        writeln!(file)?;

        // Write faces (OBJ uses 1-based indices)
        for chunk in mesh.indices.chunks(3) {
            if chunk.len() == 3 {
                writeln!(
                    file,
                    "f {}//{} {}//{} {}//{}",
                    chunk[0] + 1,
                    chunk[0] + 1,
                    chunk[1] + 1,
                    chunk[1] + 1,
                    chunk[2] + 1,
                    chunk[2] + 1
                )?;
            }
        }

        Ok(())
    }
}
