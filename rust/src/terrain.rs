use godot::classes::mesh::PrimitiveType;
use godot::classes::rendering_server::ArrayType;
use godot::classes::{ArrayMesh, Material, MeshInstance3D, Node3D, Texture2D};
use godot::classes::{Shader, ShaderMaterial};
use godot::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::brush::{Brush, BrushAction, BrushMode, BrushPhase, BrushShape};
use crate::chunk::{ChunkCoord, MeshResult};
use crate::chunk_manager::ChunkManager;
use crate::lod::LODConfig;
use crate::mesh_postprocess::{CombinedMesh, MeshPostProcessor};
use crate::mesh_worker::MeshWorkerPool;
use crate::noise_field::NoiseField;
use crate::terrain_modifications::ModificationLayer;
use crate::texture_layer::TextureLayer;
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

    /// CSG blend width for smooth normals at terrain-wall junctions
    /// Higher values = smoother normals but more rounding at corners
    /// Recommended: 1.0 to 2.0 * voxel_size. Set to 0 for hard CSG (old behavior)
    #[export]
    #[init(val = 2.0)]
    csg_blend_width: f32,

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
    // Brush Settings
    // ════════════════════════════════════════════════
    #[export_group(name = "Brush Settings")]
    /// Enable brush painting mode
    #[export]
    #[init(val = false)]
    brush_enabled: bool,

    /// Brush operating mode: 0 = Geometry, 1 = Texture
    #[export]
    #[init(val = 0)]
    brush_mode: i32,

    /// Brush shape: 0 = Square, 1 = Round
    #[export]
    #[init(val = 1)]
    brush_shape: i32,

    /// Brush size in world units
    #[export]
    #[init(val = 5.0)]
    brush_size: f32,

    /// Brush strength (0-1)
    #[export]
    #[init(val = 1.0)]
    brush_strength: f32,

    /// Selected texture index for texture painting (0-3)
    #[export]
    #[init(val = 0)]
    selected_texture_index: i32,

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

    // ════════════════════════════════════════════════
    // Texture
    // ════════════════════════════════════════════════
    #[export_group(name = "Texture")]
    #[export]
    #[init(val = None)]
    terrain_albedo: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = None)]
    terrain_normal: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = None)]
    terrain_roughness: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = None)]
    terrain_ao: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = Vector3::new(1.0, 1.0, 1.0))]
    texture_uv_scale: Vector3,

    // ═════════════════════════════════════════════════
    // Cross-Section
    // ═════════════════════════════════════════════════
    #[export_group(name = "Cross-Section")]
    #[export]
    #[init(val = false)]
    cross_section_enabled: bool,

    #[export]
    #[init(val = None)]
    underground_albedo: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = None)]
    underground_normal: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = None)]
    underground_roughness: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = None)]
    underground_ao: Option<Gd<Texture2D>>,

    #[export]
    #[init(val = 1.0)]
    underground_uv_scale: f32,

    #[export]
    #[init(val = Vector3::new(0.0, 0.0, 0.0))]
    clip_plane_position: Vector3,

    #[export]
    #[init(val = Vector3::new(0.0, 1.0, 0.0))]
    clip_plane_normal: Vector3,

    #[export]
    #[init(val = 0.0)]
    clip_offset: f32,

    #[export]
    #[init(val = false)]
    clip_camera_relative: bool,

    // ═════════════════════════════════════════════════
    // Internal State (not exported)
    // ═════════════════════════════════════════════════
    #[init(val = None)]
    worker_pool: Option<MeshWorkerPool>,

    #[init(val = VecDeque::new())]
    pending_uploads: VecDeque<MeshResult>,

    /// Storage for uploaded mesh data by chunk coordinate (for post-processing)
    /// Using HashMap ensures only the current LOD version is kept per chunk
    #[init(val = HashMap::new())]
    uploaded_meshes: HashMap<ChunkCoord, MeshResult>,

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
    cached_material: Option<Gd<Material>>,

    /// Terrain modification layer (sparse storage of brush edits)
    #[init(val = None)]
    modification_layer: Option<Arc<ModificationLayer>>,

    /// Texture layer for multi-texture painting
    #[init(val = None)]
    texture_layer: Option<Arc<TextureLayer>>,

    /// Brush state machine
    #[init(val = None)]
    brush: Option<Brush>,
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
        // Create material based on texture selection
        if let Some(ref albedo) = self.terrain_albedo {
            // Three-pass stencil buffer rendering for proper capped cross-sections:
            // Pass 1 (stencil_write): Back faces mark stencil buffer where interior is visible
            // Pass 2 (terrain): Front faces render normally with clip plane discard
            // Pass 3 (stencil_cap): Flat cap renders only where stencil == 255

            // ═══════════════════════════════════════════════════════════════════
            // Pass 1: Stencil Write - marks back faces in stencil buffer
            // ═══════════════════════════════════════════════════════════════════
            let mut stencil_write_shader = Shader::new_gd();
            stencil_write_shader.set_code(include_str!("shaders/stencil_write.gdshader"));
            let mut stencil_write_mat = ShaderMaterial::new_gd();
            stencil_write_mat.set_shader(&stencil_write_shader);

            // Cross-section parameters for stencil write pass
            stencil_write_mat.set_shader_parameter("cross_section_enabled", &self.cross_section_enabled.to_variant());
            stencil_write_mat.set_shader_parameter("clip_plane_position", &self.clip_plane_position.to_variant());
            stencil_write_mat.set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
            stencil_write_mat.set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
            stencil_write_mat.set_shader_parameter("clip_camera_relative", &self.clip_camera_relative.to_variant());

            // ═══════════════════════════════════════════════════════════════════
            // Pass 2: Main Terrain - front faces with triplanar PBR texturing
            // ═══════════════════════════════════════════════════════════════════
            let mut terrain_shader = Shader::new_gd();
            terrain_shader.set_code(include_str!("shaders/triplanar_pbr.gdshader"));
            let mut terrain_mat = ShaderMaterial::new_gd();
            terrain_mat.set_shader(&terrain_shader);

            // Set albedo texture
            terrain_mat.set_shader_parameter("albedo_texture", &albedo.to_variant());

            // Normal map (optional)
            if let Some(ref normal) = self.terrain_normal {
                terrain_mat.set_shader_parameter("normal_texture", &normal.to_variant());
                terrain_mat.set_shader_parameter("use_normal_map", &true.to_variant());
            }

            // Roughness map (optional)
            if let Some(ref roughness) = self.terrain_roughness {
                terrain_mat.set_shader_parameter("roughness_texture", &roughness.to_variant());
                terrain_mat.set_shader_parameter("use_roughness_map", &true.to_variant());
            }

            // Ambient Occlusion map (optional)
            if let Some(ref ao) = self.terrain_ao {
                terrain_mat.set_shader_parameter("ao_texture", &ao.to_variant());
                terrain_mat.set_shader_parameter("use_ao_map", &true.to_variant());
            }

            // UV scale becomes triplanar scale (use X component for uniform scaling)
            terrain_mat.set_shader_parameter("triplanar_scale", &self.texture_uv_scale.x.to_variant());

            // Cross-section parameters for terrain pass
            terrain_mat.set_shader_parameter("cross_section_enabled", &self.cross_section_enabled.to_variant());
            terrain_mat.set_shader_parameter("clip_plane_position", &self.clip_plane_position.to_variant());
            terrain_mat.set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
            terrain_mat.set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
            terrain_mat.set_shader_parameter("clip_camera_relative", &self.clip_camera_relative.to_variant());

            // ═══════════════════════════════════════════════════════════════════
            // Pass 3: Stencil Cap - flat cap at clip plane where stencil == 255
            // ═══════════════════════════════════════════════════════════════════
            let mut cap_shader = Shader::new_gd();
            cap_shader.set_code(include_str!("shaders/stencil_cap.gdshader"));
            let mut cap_mat = ShaderMaterial::new_gd();
            cap_mat.set_shader(&cap_shader);

            // Cross-section parameters for cap pass
            cap_mat.set_shader_parameter("cross_section_enabled", &self.cross_section_enabled.to_variant());
            cap_mat.set_shader_parameter("clip_plane_position", &self.clip_plane_position.to_variant());
            cap_mat.set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
            cap_mat.set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
            cap_mat.set_shader_parameter("clip_camera_relative", &self.clip_camera_relative.to_variant());

            // Underground texture (required for cap)
            if let Some(ref underground) = self.underground_albedo {
                cap_mat.set_shader_parameter("underground_texture", &underground.to_variant());
            }

            // Underground normal map (optional)
            if let Some(ref normal) = self.underground_normal {
                cap_mat.set_shader_parameter("underground_normal_texture", &normal.to_variant());
                cap_mat.set_shader_parameter("use_underground_normal_map", &true.to_variant());
            }

            // Underground roughness map (optional)
            if let Some(ref roughness) = self.underground_roughness {
                cap_mat.set_shader_parameter("underground_roughness_texture", &roughness.to_variant());
                cap_mat.set_shader_parameter("use_underground_roughness_map", &true.to_variant());
            }

            // Underground AO map (optional)
            if let Some(ref ao) = self.underground_ao {
                cap_mat.set_shader_parameter("underground_ao_texture", &ao.to_variant());
                cap_mat.set_shader_parameter("use_underground_ao_map", &true.to_variant());
            }

            cap_mat.set_shader_parameter("underground_triplanar_scale", &self.underground_uv_scale.to_variant());

            // ═══════════════════════════════════════════════════════════════════
            // Chain materials: stencil_write -> terrain -> stencil_cap
            // ═══════════════════════════════════════════════════════════════════
            // Set render priorities for correct pass ordering
            // NOTE: next_pass materials aren't necessarily rendered immediately after parent
            // Per Godot PR #80710: "Users have to rely entirely on render_priority to get correct sorting"
            stencil_write_mat.set_render_priority(-1);  // Render first (back faces mark stencil)
            terrain_mat.set_render_priority(0);          // Render second (front faces clear stencil)
            cap_mat.set_render_priority(1);              // Render last (cap where stencil == 255)

            terrain_mat.set_next_pass(&cap_mat.upcast::<Material>());
            stencil_write_mat.set_next_pass(&terrain_mat.upcast::<Material>());

            // Apply stencil_write_mat as the mesh material (it chains the others)
            self.cached_material = Some(stencil_write_mat.upcast::<Material>());
        } else {
            // No texture - use debug checkerboard shader
            let mut shader = Shader::new_gd();
            shader.set_code(include_str!("shaders/checkerboard.gdshader"));
            let mut debug_material = ShaderMaterial::new_gd();
            debug_material.set_shader(&shader);
            self.cached_material = Some(debug_material.upcast::<Material>());
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
        // Uses smooth CSG intersection for artifact-free normals at terrain-wall junctions
        let noise = NoiseField::with_csg_blend(
            self.noise_seed,
            self.noise_octaves.max(1) as usize,
            self.noise_frequency,
            self.noise_amplitude,
            self.height_offset,
            self.terrain_floor_y,
            Some((box_min, box_max)),
            self.csg_blend_width,
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

        // Create modification and texture layers
        let resolution = self.chunk_subdivisions.max(1) as u32;
        self.modification_layer = Some(Arc::new(ModificationLayer::new(resolution, self.voxel_size)));
        self.texture_layer = Some(Arc::new(TextureLayer::new(resolution, self.voxel_size)));

        // Initialize brush with current settings
        let mut brush = Brush::new(self.voxel_size);
        brush.set_mode(if self.brush_mode == 0 { BrushMode::Geometry } else { BrushMode::Texture });
        brush.set_shape(if self.brush_shape == 0 { BrushShape::Square } else { BrushShape::Round });
        brush.set_size(self.brush_size);
        brush.set_strength(self.brush_strength);
        brush.set_selected_texture(self.selected_texture_index.max(0) as usize);
        self.brush = Some(brush);

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
            manager.update_with_layers(
                camera_pos,
                noise,
                self.modification_layer.as_ref(),
                self.texture_layer.as_ref(),
            )
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

        // Upload limited number per frame (from front, oldest first)
        let max_uploads = self.max_uploads_per_frame.max(1) as usize;
        for _ in 0..max_uploads {
            if let Some(mesh_result) = self.pending_uploads.pop_front() {
                // Store by coord - automatically replaces old LOD versions
                let coord = mesh_result.coord;
                self.uploaded_meshes.insert(coord, mesh_result.clone());
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

        // Vertex colors for texture blending (RGBA = texture weights)
        let colors = PackedColorArray::from(
            &result
                .colors
                .iter()
                .map(|c| Color::from_rgba(c[0], c[1], c[2], c[3]))
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
            } else if i == ArrayType::COLOR.ord() as usize {
                arrays.push(&colors.to_variant());
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
        self.modification_layer = None;
        self.texture_layer = None;
        self.brush = None;
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
        self.apply_combined_mesh(mesh.clone(), "MergedTerrain", &combined);

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

        godot_print!(
            "PixyTerrain: weld_seams - {} chunks, {} total vertices before processing",
            chunks.len(),
            chunks.iter().map(|c| c.vertices.len()).sum::<usize>()
        );

        if chunks.is_empty() {
            godot_print!("PixyTerrain: WARNING - No chunks to process! uploaded_meshes is empty.");
            return;
        }

        let processor = MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, None);

        let mut combined = processor.merge_chunks(&chunks);
        godot_print!(
            "PixyTerrain: After merge - {} vertices, {} triangles",
            combined.vertex_count(),
            combined.triangle_count()
        );

        processor.weld_vertices(&mut combined);
        godot_print!(
            "PixyTerrain: After weld - {} vertices, {} triangles",
            combined.vertex_count(),
            combined.triangle_count()
        );

        if combined.vertices.is_empty() {
            godot_print!("PixyTerrain: WARNING - Combined mesh is empty after welding!");
            return;
        }

        let mesh = self.combined_mesh_to_godot(&combined);
        self.apply_combined_mesh(mesh, "WeldedTerrain", &combined);

        godot_print!(
            "PixyTerrain: Welded seams - final {} vertices, {} triangles",
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
        self.apply_combined_mesh(mesh.clone(), "DecimatedTerrain", &combined);

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

        godot_print!(
            "PixyTerrain: recompute_normals - {} chunks, {} total vertices before processing",
            chunks.len(),
            chunks.iter().map(|c| c.vertices.len()).sum::<usize>()
        );

        if chunks.is_empty() {
            godot_print!("PixyTerrain: WARNING - No chunks to process! uploaded_meshes is empty.");
            return;
        }

        let processor = MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, None);

        let mut combined = processor.merge_chunks(&chunks);
        godot_print!(
            "PixyTerrain: After merge - {} vertices, {} triangles",
            combined.vertex_count(),
            combined.triangle_count()
        );

        processor.weld_vertices(&mut combined);
        godot_print!(
            "PixyTerrain: After weld - {} vertices, {} triangles",
            combined.vertex_count(),
            combined.triangle_count()
        );

        processor.recompute_normals(&mut combined);
        godot_print!(
            "PixyTerrain: After normals - {} vertices, {} triangles",
            combined.vertex_count(),
            combined.triangle_count()
        );

        if combined.vertices.is_empty() {
            godot_print!("PixyTerrain: WARNING - Combined mesh is empty after processing!");
            return;
        }

        let mesh = self.combined_mesh_to_godot(&combined);
        self.apply_combined_mesh(mesh, "NormalsRecomputed", &combined);

        godot_print!(
            "PixyTerrain: Recomputed normals - final {} vertices, {} triangles",
            combined.vertex_count(),
            combined.triangle_count()
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

    // ═══════════════════════════════════════════════════════════════════════
    // Brush Methods
    // ═══════════════════════════════════════════════════════════════════════

    /// Begin a brush stroke at the given world position
    /// Returns true if brush stroke started successfully
    #[func]
    pub fn brush_begin(&mut self, world_pos: Vector3) -> bool {
        if !self.brush_enabled || !self.initialized {
            return false;
        }

        // Update brush settings from exports
        if let Some(ref mut brush) = self.brush {
            brush.set_mode(if self.brush_mode == 0 { BrushMode::Geometry } else { BrushMode::Texture });
            brush.set_shape(if self.brush_shape == 0 { BrushShape::Square } else { BrushShape::Round });
            brush.set_size(self.brush_size);
            brush.set_strength(self.brush_strength);
            brush.set_selected_texture(self.selected_texture_index.max(0) as usize);

            brush.begin_stroke(world_pos.x, world_pos.y, world_pos.z);
            true
        } else {
            false
        }
    }

    /// Continue a brush stroke (during drag)
    #[func]
    pub fn brush_continue(&mut self, world_pos: Vector3) {
        if let Some(ref mut brush) = self.brush {
            brush.continue_stroke(world_pos.x, world_pos.y, world_pos.z);
        }
    }

    /// End the current brush stroke phase
    /// Returns the action to take (0=None, 1=BeginHeightAdjust, 2=CommitGeometry, 3=CommitTexture)
    #[func]
    pub fn brush_end(&mut self, screen_y: f32) -> i32 {
        if let Some(ref mut brush) = self.brush {
            let action = brush.end_stroke(screen_y);
            match action {
                BrushAction::None => 0,
                BrushAction::BeginHeightAdjust => 1,
                BrushAction::CommitGeometry => {
                    self.commit_geometry_modification();
                    2
                }
                BrushAction::CommitTexture => {
                    self.commit_texture_modification();
                    3
                }
            }
        } else {
            0
        }
    }

    /// Update height delta during geometry mode height adjustment
    #[func]
    pub fn brush_adjust_height(&mut self, screen_y: f32) {
        if let Some(ref mut brush) = self.brush {
            brush.adjust_height(screen_y);
        }
    }

    /// Cancel the current brush operation
    #[func]
    pub fn brush_cancel(&mut self) {
        if let Some(ref mut brush) = self.brush {
            brush.cancel();
        }
    }

    /// Check if brush is currently active (in a non-idle phase)
    #[func]
    pub fn is_brush_active(&self) -> bool {
        self.brush.as_ref().map_or(false, |b| b.is_active())
    }

    /// Get current brush phase (0=Idle, 1=PaintingArea, 2=AdjustingHeight, 3=Painting)
    #[func]
    pub fn get_brush_phase(&self) -> i32 {
        if let Some(ref brush) = self.brush {
            match brush.phase {
                BrushPhase::Idle => 0,
                BrushPhase::PaintingArea => 1,
                BrushPhase::AdjustingHeight => 2,
                BrushPhase::Painting => 3,
            }
        } else {
            0
        }
    }

    /// Get brush footprint preview positions as PackedVector3Array
    #[func]
    pub fn get_brush_preview_positions(&self, world_y: f32) -> PackedVector3Array {
        if let Some(ref brush) = self.brush {
            let positions = brush.get_preview_positions(world_y);
            PackedVector3Array::from(
                &positions
                    .iter()
                    .map(|(x, y, z)| Vector3::new(*x, *y, *z))
                    .collect::<Vec<_>>()[..],
            )
        } else {
            PackedVector3Array::new()
        }
    }

    /// Get current brush height delta (for geometry mode preview)
    #[func]
    pub fn get_brush_height_delta(&self) -> f32 {
        self.brush.as_ref().map_or(0.0, |b| b.footprint.height_delta)
    }

    /// Clear all terrain modifications (brush edits)
    #[func]
    pub fn clear_modifications(&mut self) {
        // Create fresh layers
        let resolution = self.chunk_subdivisions.max(1) as u32;
        self.modification_layer = Some(Arc::new(ModificationLayer::new(resolution, self.voxel_size)));
        self.texture_layer = Some(Arc::new(TextureLayer::new(resolution, self.voxel_size)));

        // Regenerate all chunks
        self.regenerate();
    }

    /// Get total number of terrain modifications
    #[func]
    pub fn get_modification_count(&self) -> i64 {
        self.modification_layer.as_ref().map_or(0, |m| m.total_modifications()) as i64
    }

    /// Get total number of textured voxels
    #[func]
    pub fn get_texture_count(&self) -> i64 {
        self.texture_layer.as_ref().map_or(0, |t| t.total_textured()) as i64
    }
}

impl PixyTerrain {
    /// Apply a combined/processed mesh to the scene, replacing all chunk meshes
    fn apply_combined_mesh(&mut self, mesh: Gd<ArrayMesh>, name: &str, combined: &CombinedMesh) {
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

        // Clear pending uploads
        self.pending_uploads.clear();

        // Store the combined mesh back as a single MeshResult so subsequent
        // post-processing operations can chain (e.g., Decimate then Recompute Normals)
        self.uploaded_meshes.clear();
        let mesh_result = MeshResult {
            coord: combined_coord,
            lod_level: 0,
            vertices: combined.vertices.clone(),
            normals: combined.normals.clone(),
            indices: combined.indices.iter().map(|&i| i as i32).collect(),
            transition_sides: 0,
            colors: vec![[1.0, 0.0, 0.0, 0.0]; combined.vertices.len()],
        };
        self.uploaded_meshes.insert(combined_coord, mesh_result);
    }

    /// Collect mesh data from all uploaded chunks
    fn collect_chunk_meshes(&self) -> Vec<MeshResult> {
        // Use uploaded_meshes HashMap which stores only current LOD per chunk
        self.uploaded_meshes.values().cloned().collect()
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

    /// Commit geometry modification from brush to modification layer
    fn commit_geometry_modification(&mut self) {
        let Some(ref brush) = self.brush else { return };
        let Some(ref mod_layer) = self.modification_layer else { return };

        // Clone the mod layer to get a mutable version
        let mut new_mod_layer = (**mod_layer).clone();
        let affected_chunks = brush.apply_geometry(&mut new_mod_layer);

        // Store the updated layer
        self.modification_layer = Some(Arc::new(new_mod_layer));

        // Mark affected chunks for regeneration
        self.mark_chunks_for_regeneration(&affected_chunks);

        if self.debug_logging {
            godot_print!(
                "PixyTerrain: Committed geometry modification, {} chunks affected, {} total mods",
                affected_chunks.len(),
                self.modification_layer.as_ref().map_or(0, |m| m.total_modifications())
            );
        }
    }

    /// Commit texture modification from brush to texture layer
    fn commit_texture_modification(&mut self) {
        let Some(ref brush) = self.brush else { return };
        let Some(ref tex_layer) = self.texture_layer else { return };

        // Clone the texture layer to get a mutable version
        let mut new_tex_layer = (**tex_layer).clone();
        let affected_chunks = brush.apply_texture(&mut new_tex_layer);

        // Store the updated layer
        self.texture_layer = Some(Arc::new(new_tex_layer));

        // Mark affected chunks for regeneration
        self.mark_chunks_for_regeneration(&affected_chunks);

        if self.debug_logging {
            godot_print!(
                "PixyTerrain: Committed texture modification, {} chunks affected, {} total textured",
                affected_chunks.len(),
                self.texture_layer.as_ref().map_or(0, |t| t.total_textured())
            );
        }
    }

    /// Mark chunks for regeneration after brush modifications
    fn mark_chunks_for_regeneration(&mut self, chunks: &[ChunkCoord]) {
        if chunks.is_empty() {
            return;
        }

        // Remove affected chunk nodes to force re-upload
        for coord in chunks {
            if let Some(mut node) = self.chunk_nodes.remove(coord) {
                node.queue_free();
            }
            self.uploaded_meshes.remove(coord);
        }

        // Mark chunks dirty in the chunk manager
        if let Some(ref mut manager) = self.chunk_manager {
            manager.mark_chunks_dirty(chunks);
        }
    }
}
