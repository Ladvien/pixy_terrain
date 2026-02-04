use godot::classes::mesh::PrimitiveType;
use godot::classes::rendering_server::ArrayType;
use godot::classes::{ArrayMesh, Material, MeshInstance3D, Node3D, Texture2D};
use godot::classes::{Shader, ShaderMaterial};
use godot::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::brush::{
    Brush, BrushAction, BrushFootprint, BrushMode, BrushPhase, BrushShape, FlattenDirection,
};
use crate::brush_preview::BrushPreview;
use crate::chunk::{ChunkCoord, MeshResult, DEFAULT_TEXTURE_COLOR};
use crate::chunk_manager::ChunkManager;
use crate::lod::LODConfig;
use crate::mesh_postprocess::{CombinedMesh, MeshPostProcessor};
use crate::mesh_worker::MeshWorkerPool;
use crate::noise_field::NoiseField;
use crate::terrain_modifications::ModificationLayer;
use crate::terrain_stability::{self, GravityResult};
use crate::texture_layer::TextureLayer;
use crate::undo::UndoHistory;
type VariantArray = Array<Variant>;

/// Render priority for stencil write pass (back faces mark stencil).
const STENCIL_WRITE_RENDER_PRIORITY: i32 = -1;
/// Render priority for main terrain pass (front faces).
const TERRAIN_RENDER_PRIORITY: i32 = 0;
/// Render priority for stencil cap pass (flat cap at clip plane).
const STENCIL_CAP_RENDER_PRIORITY: i32 = 1;
/// Minimum height delta to show height plane preview.
const MIN_HEIGHT_DELTA_FOR_PREVIEW: f32 = 0.001;

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

    /// Brush operating mode: 0 = Elevation, 1 = Texture, 2 = Flatten, 3 = Plateau, 4 = Smooth
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

    /// Step size for plateau brush mode (world units per discrete level)
    #[export]
    #[init(val = 4.0)]
    brush_step_size: f32,

    /// Brush feather: 0.0 = hard edge (pixel-art default), 1.0 = full smootherstep falloff
    #[export]
    #[init(val = 0.0)]
    brush_feather: f32,

    /// Flatten direction: 0 = Both, 1 = Up (remove above), 2 = Down (add below)
    #[export]
    #[init(val = 0)]
    brush_flatten_direction: i32,

    /// Enable gravity: unsupported floating terrain drops after brush commits
    #[export]
    #[init(val = false)]
    enable_gravity: bool,

    /// Minimum radius for curvature preview mesh (prevents tiny previews on small brushes)
    #[export]
    #[init(val = 3.0)]
    curvature_preview_min_radius: f32,

    /// Maximum radius for curvature preview mesh (prevents enormous previews on large brushes)
    #[export]
    #[init(val = 10.0)]
    curvature_preview_max_radius: f32,

    // ════════════════════════════════════════════════
    // Undo/Redo Settings
    // ════════════════════════════════════════════════
    #[export_group(name = "Undo/Redo")]
    /// Maximum number of undo steps to keep
    #[export]
    #[init(val = 50)]
    max_undo_steps: u32,

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

    /// Brush preview: footprint overlay mesh
    #[init(val = None)]
    preview_footprint: Option<Gd<MeshInstance3D>>,

    /// Brush preview: height plane mesh
    #[init(val = None)]
    preview_height_plane: Option<Gd<MeshInstance3D>>,

    /// Brush preview generator
    #[init(val = None)]
    brush_preview: Option<BrushPreview>,

    /// Undo/redo history for modification layer snapshots
    #[init(val = None)]
    undo_history: Option<UndoHistory>,

    /// Receiver for background gravity computation results
    #[init(val = None)]
    gravity_result_rx: Option<crossbeam::channel::Receiver<GravityResult>>,

    /// Whether a gravity computation is currently pending on a background thread
    #[init(val = false)]
    gravity_pending: bool,
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
        self.update_brush_preview();
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
            self.set_cross_section_params(&mut stencil_write_mat);

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
            terrain_mat
                .set_shader_parameter("triplanar_scale", &self.texture_uv_scale.x.to_variant());

            // Cross-section parameters for terrain pass
            self.set_cross_section_params(&mut terrain_mat);

            // ═══════════════════════════════════════════════════════════════════
            // Pass 3: Stencil Cap - flat cap at clip plane where stencil == 255
            // ═══════════════════════════════════════════════════════════════════
            let mut cap_shader = Shader::new_gd();
            cap_shader.set_code(include_str!("shaders/stencil_cap.gdshader"));
            let mut cap_mat = ShaderMaterial::new_gd();
            cap_mat.set_shader(&cap_shader);

            // Cross-section parameters for cap pass
            self.set_cross_section_params(&mut cap_mat);

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
                cap_mat
                    .set_shader_parameter("underground_roughness_texture", &roughness.to_variant());
                cap_mat.set_shader_parameter("use_underground_roughness_map", &true.to_variant());
            }

            // Underground AO map (optional)
            if let Some(ref ao) = self.underground_ao {
                cap_mat.set_shader_parameter("underground_ao_texture", &ao.to_variant());
                cap_mat.set_shader_parameter("use_underground_ao_map", &true.to_variant());
            }

            cap_mat.set_shader_parameter(
                "underground_triplanar_scale",
                &self.underground_uv_scale.to_variant(),
            );

            // ═══════════════════════════════════════════════════════════════════
            // Chain materials: stencil_write -> terrain -> stencil_cap
            // ═══════════════════════════════════════════════════════════════════
            // Set render priorities for correct pass ordering
            // NOTE: next_pass materials aren't necessarily rendered immediately after parent
            // Per Godot PR #80710: "Users have to rely entirely on render_priority to get correct sorting"
            stencil_write_mat.set_render_priority(STENCIL_WRITE_RENDER_PRIORITY);
            terrain_mat.set_render_priority(TERRAIN_RENDER_PRIORITY);
            cap_mat.set_render_priority(STENCIL_CAP_RENDER_PRIORITY);

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
        self.modification_layer = Some(Arc::new(ModificationLayer::new(
            resolution,
            self.voxel_size,
        )));
        self.texture_layer = Some(Arc::new(TextureLayer::new(resolution, self.voxel_size)));

        // Initialize brush with current settings
        let mut brush = Brush::new(self.voxel_size);
        self.sync_brush_settings(&mut brush);
        self.brush = Some(brush);

        self.undo_history = Some(UndoHistory::new(self.max_undo_steps.max(1) as usize));

        self.initialized = true;

        godot_print!(
            "PixyTerrain: Ready (seed={}, {} worker threads)",
            self.noise_seed,
            self.worker_pool.as_ref().map_or(0, |p| p.thread_count())
        );
    }

    fn update_terrain(&mut self) {
        // Poll for background gravity results (non-blocking)
        self.poll_gravity_result();

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

        let mesh = Self::build_array_mesh(&vertices, &normals, &indices, Some(&colors));

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
        godot_print!(
            "Camera position: [{:.1}, {:.1}, {:.1}]",
            cam_pos[0],
            cam_pos[1],
            cam_pos[2]
        );

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
                godot_print!(
                    "SDF inside terrain (Y=16): {:.2} (should be <0)",
                    sdf_inside
                );
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
        self.free_all_chunk_nodes();

        // 4. Clear pending uploads buffer and uploaded mesh storage
        self.pending_uploads.clear();
        self.uploaded_meshes.clear();

        // 5. Free preview nodes
        if let Some(mut node) = self.preview_footprint.take() {
            if node.is_instance_valid() {
                node.queue_free();
            }
        }
        if let Some(mut node) = self.preview_height_plane.take() {
            if node.is_instance_valid() {
                node.queue_free();
            }
        }
        self.brush_preview = None;

        // 6. Cancel pending gravity computation
        self.gravity_result_rx = None;
        self.gravity_pending = false;

        // 7. Clear undo history
        if let Some(ref mut history) = self.undo_history {
            history.clear();
        }

        // 8. Drop systems
        self.worker_pool = None;
        self.chunk_manager = None;
        self.noise_field = None;
        self.modification_layer = None;
        self.texture_layer = None;
        self.brush = None;
        self.undo_history = None;
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

        let processor =
            MeshPostProcessor::new(self.normal_angle_threshold, target);

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

        let processor =
            MeshPostProcessor::new(self.normal_angle_threshold, None);

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

        let processor =
            MeshPostProcessor::new(self.normal_angle_threshold, target);

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

        let processor =
            MeshPostProcessor::new(self.normal_angle_threshold, None);

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

        let processor =
            MeshPostProcessor::new(self.normal_angle_threshold, target);

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
        if let Some(mut brush) = self.brush.take() {
            self.sync_brush_settings(&mut brush);
            brush.begin_stroke(world_pos.x, world_pos.y, world_pos.z);
            self.brush = Some(brush);
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
    /// Returns the action to take (0=None, 1=BeginHeightAdjust, 2=CommitGeometry, 3=CommitTexture, 4=CommitFlatten, 5=CommitPlateau, 6=CommitSmooth, 7=BeginCurvatureAdjust, 8=CommitSlope)
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
                BrushAction::CommitFlatten => {
                    self.commit_flatten_modification();
                    4
                }
                BrushAction::CommitPlateau => {
                    self.commit_plateau_modification();
                    5
                }
                BrushAction::CommitSmooth => {
                    self.commit_smooth_modification();
                    6
                }
                BrushAction::BeginCurvatureAdjust => 7,
                BrushAction::CommitSlope => {
                    self.commit_slope_modification();
                    8
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

    /// Update curvature during geometry mode curvature adjustment
    #[func]
    pub fn brush_adjust_curvature(&mut self, screen_y: f32) {
        if let Some(ref mut brush) = self.brush {
            brush.adjust_curvature(screen_y);
        }
    }

    /// Get current brush curvature value
    #[func]
    pub fn get_brush_curvature(&self) -> f32 {
        self.brush.as_ref().map_or(0.0, |b| b.curvature)
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

    /// Get current brush phase (0=Idle, 1=PaintingArea, 2=AdjustingHeight, 3=Painting, 4=AdjustingCurvature)
    #[func]
    pub fn get_brush_phase(&self) -> i32 {
        if let Some(ref brush) = self.brush {
            match brush.phase {
                BrushPhase::Idle => 0,
                BrushPhase::PaintingArea => 1,
                BrushPhase::AdjustingHeight => 2,
                BrushPhase::Painting => 3,
                BrushPhase::AdjustingCurvature => 4,
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
        self.brush
            .as_ref()
            .map_or(0.0, |b| b.footprint.height_delta)
    }

    /// Clear all terrain modifications (brush edits)
    #[func]
    pub fn clear_modifications(&mut self) {
        // Create fresh layers
        let resolution = self.chunk_subdivisions.max(1) as u32;
        self.modification_layer = Some(Arc::new(ModificationLayer::new(
            resolution,
            self.voxel_size,
        )));
        self.texture_layer = Some(Arc::new(TextureLayer::new(resolution, self.voxel_size)));

        // Regenerate all chunks
        self.regenerate();
    }

    /// Get total number of terrain modifications
    #[func]
    pub fn get_modification_count(&self) -> i64 {
        self.modification_layer
            .as_ref()
            .map_or(0, |m| m.total_modifications()) as i64
    }

    /// Get total number of textured voxels
    #[func]
    pub fn get_texture_count(&self) -> i64 {
        self.texture_layer
            .as_ref()
            .map_or(0, |t| t.total_textured()) as i64
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Undo/Redo Methods
    // ═══════════════════════════════════════════════════════════════════════

    /// Apply an undo or redo history action and regenerate affected chunks.
    fn apply_history_action<F>(&mut self, action: F, label: &str) -> bool
    where
        F: FnOnce(&mut UndoHistory, Arc<ModificationLayer>) -> Option<Arc<ModificationLayer>>,
    {
        let current = match self.modification_layer {
            Some(ref layer) => Arc::clone(layer),
            None => return false,
        };
        let restored = match self.undo_history {
            Some(ref mut history) => action(history, current),
            None => return false,
        };
        if let Some(state) = restored {
            self.modification_layer = Some(state);
            self.regenerate_all_modified_chunks();
            if self.debug_logging {
                let (undo_n, redo_n) = self
                    .undo_history
                    .as_ref()
                    .map_or((0, 0), |h| (h.undo_count(), h.redo_count()));
                godot_print!(
                    "PixyTerrain: {} ({}+{} steps remaining)",
                    label,
                    undo_n,
                    redo_n
                );
            }
            true
        } else {
            false
        }
    }

    /// Undo the last terrain modification.
    /// Returns true if an undo was performed.
    #[func]
    pub fn undo(&mut self) -> bool {
        self.apply_history_action(|h, c| h.undo(c), "Undo")
    }

    /// Redo the last undone terrain modification.
    /// Returns true if a redo was performed.
    #[func]
    pub fn redo(&mut self) -> bool {
        self.apply_history_action(|h, c| h.redo(c), "Redo")
    }

    /// Check if undo is available
    #[func]
    pub fn can_undo(&self) -> bool {
        self.undo_history.as_ref().map_or(false, |h| h.can_undo())
    }

    /// Check if redo is available
    #[func]
    pub fn can_redo(&self) -> bool {
        self.undo_history.as_ref().map_or(false, |h| h.can_redo())
    }
}

impl PixyTerrain {
    /// Set cross-section shader parameters on a material.
    fn set_cross_section_params(&self, mat: &mut Gd<ShaderMaterial>) {
        mat.set_shader_parameter(
            "cross_section_enabled",
            &self.cross_section_enabled.to_variant(),
        );
        mat.set_shader_parameter(
            "clip_plane_position",
            &self.clip_plane_position.to_variant(),
        );
        mat.set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
        mat.set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
        mat.set_shader_parameter(
            "clip_camera_relative",
            &self.clip_camera_relative.to_variant(),
        );
    }

    /// Sync exported brush properties to a Brush instance.
    fn sync_brush_settings(&self, brush: &mut Brush) {
        brush.set_mode(match self.brush_mode {
            0 => BrushMode::Elevation,
            1 => BrushMode::Texture,
            2 => BrushMode::Flatten,
            3 => BrushMode::Plateau,
            4 => BrushMode::Smooth,
            5 => BrushMode::Slope,
            _ => BrushMode::Elevation,
        });
        brush.set_shape(if self.brush_shape == 0 {
            BrushShape::Square
        } else {
            BrushShape::Round
        });
        brush.set_size(self.brush_size);
        brush.set_strength(self.brush_strength);
        brush.set_selected_texture(self.selected_texture_index.max(0) as usize);
        brush.set_step_size(self.brush_step_size);
        brush.set_feather(self.brush_feather);
        brush.set_flatten_direction(match self.brush_flatten_direction {
            1 => FlattenDirection::Up,
            2 => FlattenDirection::Down,
            _ => FlattenDirection::Both,
        });
        let total_cells_x = (self.map_width_x.max(1) * self.chunk_subdivisions.max(1)) as i32;
        let total_cells_z = (self.map_width_z.max(1) * self.chunk_subdivisions.max(1)) as i32;
        brush.set_terrain_bounds(total_cells_x, total_cells_z);
    }

    /// Free all chunk mesh nodes and remove them from the scene tree.
    fn free_all_chunk_nodes(&mut self) {
        let nodes: Vec<_> = self.chunk_nodes.drain().map(|(_, node)| node).collect();
        for mut node in nodes {
            if node.is_instance_valid() {
                self.base_mut().remove_child(&node);
                node.queue_free();
            }
        }
    }

    /// Build a Godot ArrayMesh from packed vertex data.
    fn build_array_mesh(
        vertices: &PackedVector3Array,
        normals: &PackedVector3Array,
        indices: &PackedInt32Array,
        colors: Option<&PackedColorArray>,
    ) -> Gd<ArrayMesh> {
        let mut mesh = ArrayMesh::new_gd();
        let num_arrays = ArrayType::MAX.ord() as usize;
        let mut arrays: VariantArray = VariantArray::new();

        for i in 0..num_arrays {
            if i == ArrayType::VERTEX.ord() as usize {
                arrays.push(&vertices.to_variant());
            } else if i == ArrayType::NORMAL.ord() as usize {
                arrays.push(&normals.to_variant());
            } else if i == ArrayType::COLOR.ord() as usize {
                if let Some(c) = colors {
                    arrays.push(&c.to_variant());
                } else {
                    arrays.push(&Variant::nil());
                }
            } else if i == ArrayType::INDEX.ord() as usize {
                arrays.push(&indices.to_variant());
            } else {
                arrays.push(&Variant::nil());
            }
        }

        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        mesh
    }
}

impl PixyTerrain {
    /// Apply a combined/processed mesh to the scene, replacing all chunk meshes
    fn apply_combined_mesh(&mut self, mesh: Gd<ArrayMesh>, name: &str, combined: &CombinedMesh) {
        // Clear all existing chunk nodes (keep box geometry)
        self.free_all_chunk_nodes();

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
            colors: vec![DEFAULT_TEXTURE_COLOR; combined.vertices.len()],
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

        Self::build_array_mesh(&vertices, &normals, &indices, None)
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

    /// Generic SDF commit: push undo, apply closure, update layer, regenerate chunks, gravity.
    fn commit_sdf_modification<F>(&mut self, label: &str, apply_fn: F)
    where
        F: FnOnce(
            &Brush,
            &NoiseField,
            &ModificationLayer,
            &mut ModificationLayer,
        ) -> Vec<ChunkCoord>,
    {
        let Some(ref brush) = self.brush else { return };
        let Some(ref mod_layer) = self.modification_layer else {
            return;
        };
        let Some(ref noise) = self.noise_field else {
            return;
        };

        if let Some(ref mut history) = self.undo_history {
            history.push(Arc::clone(mod_layer));
        }

        let footprint = brush.footprint.clone();
        let existing = mod_layer.as_ref();
        let mut new_mod_layer = existing.clone();
        let affected_chunks = apply_fn(brush, noise, existing, &mut new_mod_layer);

        let mod_layer_for_gravity = new_mod_layer.clone();
        self.modification_layer = Some(Arc::new(new_mod_layer));
        self.mark_chunks_for_regeneration(&affected_chunks);
        self.dispatch_gravity_if_enabled(mod_layer_for_gravity, &footprint);

        if self.debug_logging {
            godot_print!(
                "PixyTerrain: Committed {} modification, {} chunks affected, {} total mods",
                label,
                affected_chunks.len(),
                self.modification_layer
                    .as_ref()
                    .map_or(0, |m| m.total_modifications())
            );
        }
    }

    fn commit_geometry_modification(&mut self) {
        self.commit_sdf_modification("geometry", |brush, noise, existing, new| {
            brush.apply_geometry(noise, existing, new)
        });
    }

    /// Commit texture modification from brush to texture layer
    fn commit_texture_modification(&mut self) {
        let Some(ref brush) = self.brush else { return };
        let Some(ref tex_layer) = self.texture_layer else {
            return;
        };

        if let (Some(ref mut history), Some(ref mod_layer)) =
            (&mut self.undo_history, &self.modification_layer)
        {
            history.push(Arc::clone(mod_layer));
        }

        let mut new_tex_layer = (**tex_layer).clone();
        let affected_chunks = brush.apply_texture(&mut new_tex_layer);
        self.texture_layer = Some(Arc::new(new_tex_layer));
        self.mark_chunks_for_regeneration(&affected_chunks);

        if self.debug_logging {
            godot_print!(
                "PixyTerrain: Committed texture modification, {} chunks affected, {} total textured",
                affected_chunks.len(),
                self.texture_layer.as_ref().map_or(0, |t| t.total_textured())
            );
        }
    }

    fn commit_flatten_modification(&mut self) {
        self.commit_sdf_modification("flatten", |brush, noise, existing, new| {
            brush.apply_flatten(noise, existing, new)
        });
    }

    fn commit_plateau_modification(&mut self) {
        self.commit_sdf_modification("plateau", |brush, noise, existing, new| {
            brush.apply_plateau(noise, existing, new)
        });
    }

    fn commit_smooth_modification(&mut self) {
        self.commit_sdf_modification("smooth", |brush, noise, existing, new| {
            brush.apply_smooth(noise, existing, new)
        });
    }

    fn commit_slope_modification(&mut self) {
        self.commit_sdf_modification("slope", |brush, noise, existing, new| {
            brush.apply_slope(noise, existing, new)
        });
    }

    /// Regenerate all chunks that could be affected by the current or previous modifications.
    /// Used after undo/redo to refresh the entire modified region.
    fn regenerate_all_modified_chunks(&mut self) {
        // Collect all chunk coords that have modifications in the current layer
        let mod_chunks: Vec<ChunkCoord> = self
            .modification_layer
            .as_ref()
            .map(|layer| layer.modified_chunks().cloned().collect())
            .unwrap_or_default();

        // Also regenerate all currently loaded chunks since undo may remove mods
        // from chunks that previously had them
        let loaded_chunks: Vec<ChunkCoord> = self.chunk_nodes.keys().cloned().collect();

        // Combine both sets
        let mut all_chunks: Vec<ChunkCoord> = mod_chunks;
        for coord in loaded_chunks {
            if !all_chunks.contains(&coord) {
                all_chunks.push(coord);
            }
        }

        self.mark_chunks_for_regeneration(&all_chunks);
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

    /// Ensure preview MeshInstance3D nodes and BrushPreview exist (lazy creation)
    fn ensure_preview_nodes(&mut self) {
        if self.brush_preview.is_none() {
            self.brush_preview = Some(BrushPreview::new());
        }

        if self.preview_footprint.is_none() {
            let mut node = MeshInstance3D::new_alloc();
            node.set_name("BrushPreviewFootprint");
            node.set_visible(false);
            self.base_mut().add_child(&node);
            self.preview_footprint = Some(node);
        }

        if self.preview_height_plane.is_none() {
            let mut node = MeshInstance3D::new_alloc();
            node.set_name("BrushPreviewHeightPlane");
            node.set_visible(false);
            self.base_mut().add_child(&node);
            self.preview_height_plane = Some(node);
        }
    }

    /// Update brush preview visualization based on current brush state
    fn update_brush_preview(&mut self) {
        let Some(ref brush) = self.brush else {
            self.hide_preview();
            return;
        };

        // Only show preview during active brush phases
        match brush.phase {
            BrushPhase::Idle => {
                self.hide_preview();
                return;
            }
            BrushPhase::PaintingArea
            | BrushPhase::AdjustingHeight
            | BrushPhase::AdjustingCurvature
            | BrushPhase::Painting => {}
        }

        if brush.footprint.is_empty() {
            self.hide_preview();
            return;
        }

        // Clone brush data we need before mutable borrows
        let brush_clone = brush.clone();

        self.ensure_preview_nodes();

        // Update footprint overlay (hidden during curvature adjustment to avoid occluding the dome)
        if brush_clone.phase == BrushPhase::AdjustingCurvature {
            if let Some(ref mut footprint_node) = self.preview_footprint {
                footprint_node.set_visible(false);
            }
        } else if let Some(ref mut preview) = self.brush_preview {
            preview.update_material(&brush_clone);

            if let Some(mesh) = preview.generate_mesh(&brush_clone) {
                if let Some(ref mut footprint_node) = self.preview_footprint {
                    footprint_node.set_mesh(&mesh);
                    if let Some(mat) = preview.get_material() {
                        footprint_node.set_surface_override_material(0, &mat.upcast::<Material>());
                    }
                    footprint_node.set_visible(true);
                }
            }
        }

        // Update height plane (during AdjustingHeight and AdjustingCurvature phases)
        let show_height_plane = (brush_clone.phase == BrushPhase::AdjustingHeight
            || brush_clone.phase == BrushPhase::AdjustingCurvature)
            && brush_clone.footprint.height_delta.abs() > MIN_HEIGHT_DELTA_FOR_PREVIEW;

        if show_height_plane {
            let fp = &brush_clone.footprint;
            let vs = brush_clone.voxel_size;
            let min_x = fp.min_x as f32 * vs;
            let max_x = (fp.max_x + 1) as f32 * vs;
            let min_z = fp.min_z as f32 * vs;
            let max_z = (fp.max_z + 1) as f32 * vs;
            let target_y = fp.base_y + fp.height_delta;
            let raising = fp.height_delta > 0.0;

            let plane_mesh = if brush_clone.phase == BrushPhase::AdjustingCurvature {
                // Clamp preview radius so it's visible on tiny brushes and not enormous on large ones
                let cx = (min_x + max_x) * 0.5;
                let cz = (min_z + max_z) * 0.5;
                let half_x = (max_x - min_x) * 0.5;
                let half_z = (max_z - min_z) * 0.5;
                let actual_radius = half_x.max(half_z);
                let clamped_radius = actual_radius.clamp(
                    self.curvature_preview_min_radius,
                    self.curvature_preview_max_radius,
                );
                let clamped_min_x = cx - clamped_radius;
                let clamped_max_x = cx + clamped_radius;
                let clamped_min_z = cz - clamped_radius;
                let clamped_max_z = cz + clamped_radius;

                BrushPreview::generate_curved_height_plane_mesh(
                    clamped_min_x,
                    clamped_max_x,
                    clamped_min_z,
                    clamped_max_z,
                    fp.base_y,
                    fp.height_delta,
                    brush_clone.curvature,
                    clamped_radius,
                    brush_clone.shape,
                )
            } else {
                BrushPreview::generate_height_plane_mesh(min_x, max_x, min_z, max_z, target_y)
            };
            let plane_mat = BrushPreview::create_height_plane_material(raising);

            if let Some(ref mut plane_node) = self.preview_height_plane {
                plane_node.set_mesh(&plane_mesh);
                plane_node.set_surface_override_material(0, &plane_mat.upcast::<Material>());
                plane_node.set_visible(true);
            }
        } else {
            if let Some(ref mut plane_node) = self.preview_height_plane {
                plane_node.set_visible(false);
            }
        }
    }

    /// Hide all preview nodes
    fn hide_preview(&mut self) {
        if let Some(ref mut node) = self.preview_footprint {
            node.set_visible(false);
        }
        if let Some(ref mut node) = self.preview_height_plane {
            node.set_visible(false);
        }
    }

    /// Dispatch gravity computation on a background thread if gravity is enabled.
    ///
    /// Cancels any previously pending gravity computation (the new brush commit
    /// supersedes it since it has a newer modification layer).
    fn dispatch_gravity_if_enabled(
        &mut self,
        mod_layer: ModificationLayer,
        footprint: &BrushFootprint,
    ) {
        if !self.enable_gravity {
            return;
        }

        let Some(ref noise_arc) = self.noise_field else {
            return;
        };

        // Cancel any pending gravity by dropping the old receiver
        self.gravity_result_rx = None;
        self.gravity_pending = false;

        let noise = Arc::clone(noise_arc);
        let footprint = footprint.clone();
        let voxel_size = self.voxel_size;
        let box_bounds = noise.get_box_bounds();
        let total_cells_x = (self.map_width_x.max(1) * self.chunk_subdivisions.max(1)) as i32;
        let total_cells_z = (self.map_width_z.max(1) * self.chunk_subdivisions.max(1)) as i32;

        let (tx, rx) = crossbeam::channel::bounded(1);
        self.gravity_result_rx = Some(rx);
        self.gravity_pending = true;

        std::thread::spawn(move || {
            let result = terrain_stability::compute_gravity(
                &noise,
                mod_layer,
                &footprint,
                voxel_size,
                box_bounds,
                total_cells_x,
                total_cells_z,
            );
            // If the receiver has been dropped (new brush commit), this send
            // silently fails — which is the desired behavior.
            let _ = tx.send(result);
        });
    }

    /// Poll for completed background gravity results (non-blocking).
    ///
    /// Called at the beginning of each frame in `update_terrain()`.
    fn poll_gravity_result(&mut self) {
        if !self.gravity_pending {
            return;
        }

        let result = if let Some(ref rx) = self.gravity_result_rx {
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(crossbeam::channel::TryRecvError::Empty) => None,
                Err(crossbeam::channel::TryRecvError::Disconnected) => {
                    // Thread finished but channel was disconnected (shouldn't happen)
                    self.gravity_pending = false;
                    self.gravity_result_rx = None;
                    None
                }
            }
        } else {
            self.gravity_pending = false;
            None
        };

        if let Some(gravity_result) = result {
            self.gravity_pending = false;
            self.gravity_result_rx = None;

            if gravity_result.components_dropped > 0 {
                // Swap in the gravity-updated modification layer
                self.modification_layer = Some(Arc::new(gravity_result.new_mod_layer));
                self.mark_chunks_for_regeneration(&gravity_result.affected_chunks);

                if self.debug_logging {
                    godot_print!(
                        "PixyTerrain: Gravity dropped {} of {} floating components, {} chunks affected",
                        gravity_result.components_dropped,
                        gravity_result.components_found,
                        gravity_result.affected_chunks.len()
                    );
                }
            }
        }
    }
}
