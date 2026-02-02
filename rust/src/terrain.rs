use godot::classes::mesh::PrimitiveType;
use godot::classes::rendering_server::ArrayType;
use godot::classes::{ArrayMesh, Material, MeshInstance3D, Node3D, Texture2D};
use godot::classes::{Shader, ShaderMaterial};
use godot::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::chunk::{ChunkCoord, MeshResult};
use crate::chunk_manager::ChunkManager;
use crate::debug_log::{debug_log, init_debug_log};
use crate::lod::LODConfig;
use crate::mesh_postprocess::{CombinedMesh, MeshPostProcessor};
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

    /// Full material chain for individual chunks (stencil_write -> terrain -> stencil_cap)
    /// Used when cross-section is DISABLED
    #[init(val = None)]
    cached_material: Option<Gd<Material>>,

    /// Terrain-only material (no stencil passes)
    /// Used for chunks when cross-section is ENABLED - merged mesh handles stencil
    #[init(val = None)]
    cached_terrain_only_material: Option<Gd<Material>>,

    /// Stencil-only material chain (stencil_write -> stencil_cap, no terrain pass)
    /// Used for merged mesh to handle stencil cap rendering only
    #[init(val = None)]
    cached_stencil_only_material: Option<Gd<Material>>,

    /// Merged mesh instance used for stencil cap rendering when cross-section is enabled.
    /// This ensures all back faces render before any caps, fixing holes at chunk boundaries.
    #[init(val = None)]
    merged_stencil_mesh: Option<Gd<MeshInstance3D>>,

    /// Track whether merged mesh needs rebuild (chunks changed while cross-section enabled)
    #[init(val = false)]
    merged_mesh_dirty: bool,

    /// Cache the previous cross_section_enabled state to detect changes
    #[init(val = false)]
    prev_cross_section_enabled: bool,
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
        // Initialize debug logging (recreates log file each run)
        init_debug_log();
        debug_log("[initialize_systems] START");
        debug_log(&format!(
            "[initialize_systems] Config: map={}x{}x{}, voxel_size={}, chunk_subdivs={}",
            self.map_width_x,
            self.map_height_y,
            self.map_width_z,
            self.voxel_size,
            self.chunk_subdivisions
        ));
        debug_log(&format!(
            "[initialize_systems] Noise: seed={}, octaves={}, freq={}, amp={}, height_offset={}",
            self.noise_seed,
            self.noise_octaves,
            self.noise_frequency,
            self.noise_amplitude,
            self.height_offset
        ));
        debug_log(&format!(
            "[initialize_systems] LOD: base_dist={}, max_level={}, floor_y={}",
            self.lod_base_distance, self.max_lod_level, self.terrain_floor_y
        ));
        debug_log(&format!(
            "[initialize_systems] Cross-section: enabled={}, plane_pos={:?}, plane_normal={:?}",
            self.cross_section_enabled, self.clip_plane_position, self.clip_plane_normal
        ));

        // Create material based on texture selection
        if let Some(ref albedo) = self.terrain_albedo {
            debug_log("[initialize_systems] Creating stencil_write material...");
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
            stencil_write_mat.set_shader_parameter(
                "cross_section_enabled",
                &self.cross_section_enabled.to_variant(),
            );
            stencil_write_mat.set_shader_parameter(
                "clip_plane_position",
                &self.clip_plane_position.to_variant(),
            );
            stencil_write_mat
                .set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
            stencil_write_mat.set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
            stencil_write_mat.set_shader_parameter(
                "clip_camera_relative",
                &self.clip_camera_relative.to_variant(),
            );

            debug_log("[initialize_systems] Creating terrain material...");
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
            terrain_mat.set_shader_parameter(
                "cross_section_enabled",
                &self.cross_section_enabled.to_variant(),
            );
            terrain_mat.set_shader_parameter(
                "clip_plane_position",
                &self.clip_plane_position.to_variant(),
            );
            terrain_mat
                .set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
            terrain_mat.set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
            terrain_mat.set_shader_parameter(
                "clip_camera_relative",
                &self.clip_camera_relative.to_variant(),
            );

            debug_log("[initialize_systems] Creating stencil_cap material...");
            // ═══════════════════════════════════════════════════════════════════
            // Pass 3: Stencil Cap - flat cap at clip plane where stencil == 255
            // ═══════════════════════════════════════════════════════════════════
            let mut cap_shader = Shader::new_gd();
            cap_shader.set_code(include_str!("shaders/stencil_cap.gdshader"));
            let mut cap_mat = ShaderMaterial::new_gd();
            cap_mat.set_shader(&cap_shader);

            // Cross-section parameters for cap pass
            cap_mat.set_shader_parameter(
                "cross_section_enabled",
                &self.cross_section_enabled.to_variant(),
            );
            cap_mat.set_shader_parameter(
                "clip_plane_position",
                &self.clip_plane_position.to_variant(),
            );
            cap_mat.set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
            cap_mat.set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
            cap_mat.set_shader_parameter(
                "clip_camera_relative",
                &self.clip_camera_relative.to_variant(),
            );

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
            stencil_write_mat.set_render_priority(-1); // Render first (back faces mark stencil)
            terrain_mat.set_render_priority(0); // Render second (front faces clear stencil)
            cap_mat.set_render_priority(1); // Render last (cap where stencil == 255)

            terrain_mat.set_next_pass(&cap_mat.upcast::<Material>());
            stencil_write_mat.set_next_pass(&terrain_mat.upcast::<Material>());

            // Apply stencil_write_mat as the mesh material (it chains the others)
            self.cached_material = Some(stencil_write_mat.upcast::<Material>());
            debug_log("[initialize_systems] Material chain: priorities -1, 0, 1 (stencil_write -> terrain -> cap)");

            debug_log("[initialize_systems] Creating stencil-only material for merged mesh...");
            // ═══════════════════════════════════════════════════════════════════
            // Stencil-only material chain (for merged mesh: stencil_write -> stencil_cap)
            // This skips the terrain pass - chunks handle terrain rendering,
            // merged mesh handles stencil cap only to fix holes at chunk boundaries.
            // ═══════════════════════════════════════════════════════════════════
            let mut merged_stencil_write_shader = Shader::new_gd();
            merged_stencil_write_shader.set_code(include_str!("shaders/stencil_write.gdshader"));
            let mut merged_stencil_write_mat = ShaderMaterial::new_gd();
            merged_stencil_write_mat.set_shader(&merged_stencil_write_shader);

            // Cross-section parameters
            merged_stencil_write_mat.set_shader_parameter(
                "cross_section_enabled",
                &self.cross_section_enabled.to_variant(),
            );
            merged_stencil_write_mat.set_shader_parameter(
                "clip_plane_position",
                &self.clip_plane_position.to_variant(),
            );
            merged_stencil_write_mat
                .set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
            merged_stencil_write_mat
                .set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
            merged_stencil_write_mat.set_shader_parameter(
                "clip_camera_relative",
                &self.clip_camera_relative.to_variant(),
            );

            let mut merged_cap_shader = Shader::new_gd();
            merged_cap_shader.set_code(include_str!("shaders/stencil_cap.gdshader"));
            let mut merged_cap_mat = ShaderMaterial::new_gd();
            merged_cap_mat.set_shader(&merged_cap_shader);

            // Cross-section parameters for cap
            merged_cap_mat.set_shader_parameter(
                "cross_section_enabled",
                &self.cross_section_enabled.to_variant(),
            );
            merged_cap_mat.set_shader_parameter(
                "clip_plane_position",
                &self.clip_plane_position.to_variant(),
            );
            merged_cap_mat
                .set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
            merged_cap_mat.set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
            merged_cap_mat.set_shader_parameter(
                "clip_camera_relative",
                &self.clip_camera_relative.to_variant(),
            );

            // Underground textures for cap
            if let Some(ref underground) = self.underground_albedo {
                merged_cap_mat
                    .set_shader_parameter("underground_texture", &underground.to_variant());
            }
            if let Some(ref normal) = self.underground_normal {
                merged_cap_mat
                    .set_shader_parameter("underground_normal_texture", &normal.to_variant());
                merged_cap_mat
                    .set_shader_parameter("use_underground_normal_map", &true.to_variant());
            }
            if let Some(ref roughness) = self.underground_roughness {
                merged_cap_mat
                    .set_shader_parameter("underground_roughness_texture", &roughness.to_variant());
                merged_cap_mat
                    .set_shader_parameter("use_underground_roughness_map", &true.to_variant());
            }
            if let Some(ref ao) = self.underground_ao {
                merged_cap_mat.set_shader_parameter("underground_ao_texture", &ao.to_variant());
                merged_cap_mat.set_shader_parameter("use_underground_ao_map", &true.to_variant());
            }
            merged_cap_mat.set_shader_parameter(
                "underground_triplanar_scale",
                &self.underground_uv_scale.to_variant(),
            );

            // Chain: stencil_write -> stencil_cap (no terrain pass)
            // Render priorities: -2 for stencil write (before chunk stencil), 2 for cap (after chunk cap)
            merged_stencil_write_mat.set_render_priority(-2);
            merged_cap_mat.set_render_priority(2);
            merged_stencil_write_mat.set_next_pass(&merged_cap_mat.upcast::<Material>());

            self.cached_stencil_only_material = Some(merged_stencil_write_mat.upcast::<Material>());
            debug_log("[initialize_systems] Stencil-only material: priorities -2, 2 (stencil_write -> cap)");

            debug_log("[initialize_systems] Creating terrain-only material...");
            // ═══════════════════════════════════════════════════════════════════
            // Terrain-only material (no stencil passes)
            // Used for chunks when cross-section is ENABLED - merged mesh handles stencil
            // ═══════════════════════════════════════════════════════════════════
            let mut terrain_only_shader = Shader::new_gd();
            terrain_only_shader.set_code(include_str!("shaders/triplanar_pbr.gdshader"));
            let mut terrain_only_mat = ShaderMaterial::new_gd();
            terrain_only_mat.set_shader(&terrain_only_shader);

            // Copy all terrain texture settings
            terrain_only_mat.set_shader_parameter("albedo_texture", &albedo.to_variant());
            if let Some(ref normal) = self.terrain_normal {
                terrain_only_mat.set_shader_parameter("normal_texture", &normal.to_variant());
                terrain_only_mat.set_shader_parameter("use_normal_map", &true.to_variant());
            }
            if let Some(ref roughness) = self.terrain_roughness {
                terrain_only_mat.set_shader_parameter("roughness_texture", &roughness.to_variant());
                terrain_only_mat.set_shader_parameter("use_roughness_map", &true.to_variant());
            }
            if let Some(ref ao) = self.terrain_ao {
                terrain_only_mat.set_shader_parameter("ao_texture", &ao.to_variant());
                terrain_only_mat.set_shader_parameter("use_ao_map", &true.to_variant());
            }
            terrain_only_mat
                .set_shader_parameter("triplanar_scale", &self.texture_uv_scale.x.to_variant());

            // Cross-section parameters (for clip plane discard in terrain shader)
            terrain_only_mat.set_shader_parameter(
                "cross_section_enabled",
                &self.cross_section_enabled.to_variant(),
            );
            terrain_only_mat.set_shader_parameter(
                "clip_plane_position",
                &self.clip_plane_position.to_variant(),
            );
            terrain_only_mat
                .set_shader_parameter("clip_plane_normal", &self.clip_plane_normal.to_variant());
            terrain_only_mat.set_shader_parameter("clip_offset", &self.clip_offset.to_variant());
            terrain_only_mat.set_shader_parameter(
                "clip_camera_relative",
                &self.clip_camera_relative.to_variant(),
            );

            self.cached_terrain_only_material = Some(terrain_only_mat.upcast::<Material>());
            debug_log("[initialize_systems] All materials created successfully");
        } else {
            debug_log("[initialize_systems] No albedo texture - using debug checkerboard shader");
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
        // Box extension = chunk_size/2 puts surfaces in MIDDLE of guard chunks,
        // ensuring proper SDF zero-crossings for watertight corner geometry.
        let noise = NoiseField::with_box_extension(
            self.noise_seed,
            self.noise_octaves.max(1) as usize,
            self.noise_frequency,
            self.noise_amplitude,
            self.height_offset,
            self.terrain_floor_y,
            Some((box_min, box_max)),
            self.csg_blend_width,
            chunk_size / 2.0, // Extend box bounds by half chunk for corner geometry
        );

        let noise_arc = Arc::new(noise);
        self.noise_field = Some(Arc::clone(&noise_arc));
        debug_log(&format!(
            "[initialize_systems] Noise field created with CSG blend={}",
            self.csg_blend_width
        ));

        let threads = if self.worker_threads <= 0 {
            0
        } else {
            self.worker_threads as usize
        };
        let worker_pool = MeshWorkerPool::new(threads, self.channel_capacity as usize);
        let actual_threads = worker_pool.thread_count();
        debug_log(&format!(
            "[initialize_systems] Worker pool: {} threads (requested: {})",
            actual_threads, self.worker_threads
        ));

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
        debug_log(&format!(
            "[initialize_systems] Chunk manager created: map {}x{}x{}",
            self.map_width_x, self.map_height_y, self.map_width_z
        ));

        self.worker_pool = Some(worker_pool);
        self.chunk_manager = Some(chunk_manager);

        self.initialized = true;

        debug_log("[initialize_systems] END");
        debug_log("");

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

        // Track if chunks changed (for merged mesh update)
        let chunks_changed = !new_meshes.is_empty();

        // Log new meshes received (only when there are new meshes to avoid spam)
        if !new_meshes.is_empty() {
            debug_log(&format!(
                "[update_terrain] New meshes from workers: {}, camera=({:.1}, {:.1}, {:.1})",
                new_meshes.len(),
                camera_pos[0],
                camera_pos[1],
                camera_pos[2]
            ));
        }

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
        let mut uploaded_this_frame = false;
        for _ in 0..max_uploads {
            if let Some(mesh_result) = self.pending_uploads.pop_front() {
                // Store by coord - automatically replaces old LOD versions
                let coord = mesh_result.coord;
                self.uploaded_meshes.insert(coord, mesh_result.clone());
                self.upload_mesh_to_godot(mesh_result);
                self.meshes_uploaded += 1;
                uploaded_this_frame = true;
            } else {
                break;
            }
        }

        let unloaded = self.unload_distant_chunks();

        // Detect cross_section_enabled toggle
        let cross_section_toggled = self.cross_section_enabled != self.prev_cross_section_enabled;
        self.prev_cross_section_enabled = self.cross_section_enabled;

        if cross_section_toggled {
            debug_log(&format!(
                "[update_terrain] Cross-section TOGGLED: {} -> {}",
                !self.cross_section_enabled, self.cross_section_enabled
            ));
        }

        // Mark merged mesh dirty if chunks changed while cross-section enabled
        if self.cross_section_enabled && (chunks_changed || uploaded_this_frame || unloaded) {
            self.merged_mesh_dirty = true;
            debug_log(&format!(
                "[update_terrain] Merged mesh marked dirty: chunks_changed={}, uploaded={}, unloaded={}",
                chunks_changed, uploaded_this_frame, unloaded
            ));
        }

        // Update merged mesh for cross-section stencil cap
        if cross_section_toggled || self.merged_mesh_dirty {
            self.update_merged_stencil_mesh();
        }
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
            debug_log(&format!(
                "[upload_mesh_to_godot] SKIPPED empty mesh: ({}, {}, {}) LOD={}",
                result.coord.x, result.coord.y, result.coord.z, result.lod_level
            ));
            return;
        }

        let vert_count = result.vertices.len();
        let tri_count = result.indices.len() / 3;

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

        // Apply material based on cross-section state:
        // - Cross-section DISABLED: use full material chain (stencil + terrain + cap)
        // - Cross-section ENABLED: use terrain-only material (merged mesh handles stencil)
        let material_type = if self.cross_section_enabled {
            "terrain_only"
        } else {
            "full_chain"
        };
        let material = if self.cross_section_enabled {
            self.cached_terrain_only_material.as_ref()
        } else {
            self.cached_material.as_ref()
        };
        if let Some(mat) = material {
            instance.set_surface_override_material(0, mat);
        }

        debug_log(&format!(
            "[upload_mesh_to_godot] Chunk ({}, {}, {}) LOD={}: {} verts, {} tris, material={}",
            coord.x, coord.y, coord.z, result.lod_level, vert_count, tri_count, material_type
        ));

        self.base_mut().add_child(&instance);

        let instance_id = instance.instance_id().to_i64();
        self.chunk_nodes.insert(coord, instance);

        if let Some(ref mut manager) = self.chunk_manager {
            manager.mark_chunk_active(&coord, instance_id);
        }
    }

    /// Unload chunks that are too far from camera. Returns true if any chunks were unloaded.
    fn unload_distant_chunks(&mut self) -> bool {
        let unload_list = if let Some(ref manager) = self.chunk_manager {
            manager.get_unload_candidates()
        } else {
            Vec::new()
        };

        let unloaded = !unload_list.is_empty();

        if !unload_list.is_empty() {
            debug_log(&format!(
                "[unload_distant_chunks] Unloading {} chunks",
                unload_list.len()
            ));
        }

        for (coord, _) in &unload_list {
            debug_log(&format!(
                "[unload_distant_chunks] Unloading chunk ({}, {}, {})",
                coord.x, coord.y, coord.z
            ));
        }

        for (coord, _) in unload_list {
            if let Some(mut node) = self.chunk_nodes.remove(&coord) {
                node.queue_free();
            }
            // Also remove from uploaded_meshes so merged mesh stays in sync
            self.uploaded_meshes.remove(&coord);
            if let Some(ref mut manager) = self.chunk_manager {
                manager.remove_chunk(&coord);
            }
        }

        unloaded
    }

    /// Switch all chunk materials between full (stencil+terrain+cap) and terrain-only.
    /// Called when cross_section_enabled changes.
    fn update_chunk_materials(&mut self) {
        let material_type = if self.cross_section_enabled {
            "terrain_only"
        } else {
            "full_chain"
        };
        let material = if self.cross_section_enabled {
            self.cached_terrain_only_material.as_ref()
        } else {
            self.cached_material.as_ref()
        };

        debug_log(&format!(
            "[update_chunk_materials] Switching {} chunks to {} material",
            self.chunk_nodes.len(),
            material_type
        ));

        if let Some(mat) = material {
            for (_, instance) in self.chunk_nodes.iter_mut() {
                if instance.is_instance_valid() {
                    instance.set_surface_override_material(0, mat);
                }
            }
        }
    }

    /// Update merged mesh for cross-section stencil cap rendering.
    /// When cross_section_enabled, merges all chunk meshes into a single mesh
    /// with stencil-only material (stencil_write -> stencil_cap).
    /// This ensures all back faces from all chunks are written to stencil
    /// before any cap rendering, fixing holes at chunk boundaries.
    fn update_merged_stencil_mesh(&mut self) {
        debug_log(&format!(
            "[update_merged_stencil_mesh] cross_section={}, dirty={}",
            self.cross_section_enabled, self.merged_mesh_dirty
        ));

        // If cross-section disabled, clean up merged mesh and restore chunk materials
        if !self.cross_section_enabled {
            debug_log(
                "[update_merged_stencil_mesh] Cross-section DISABLED - cleaning up merged mesh",
            );
            if let Some(mut instance) = self.merged_stencil_mesh.take() {
                if instance.is_instance_valid() {
                    self.base_mut().remove_child(&instance);
                    instance.queue_free();
                }
            }
            // Restore full material chain to chunks
            self.update_chunk_materials();
            self.merged_mesh_dirty = false;
            return;
        }

        // Cross-section enabled - switch chunks to terrain-only material
        self.update_chunk_materials();

        // Collect all chunk meshes
        let chunks = self.collect_chunk_meshes();
        debug_log(&format!(
            "[update_merged_stencil_mesh] Collecting {} chunk meshes",
            chunks.len()
        ));

        if chunks.is_empty() {
            debug_log("[update_merged_stencil_mesh] No chunks to merge - cleaning up");
            // No chunks to merge - clean up any existing merged mesh
            if let Some(mut instance) = self.merged_stencil_mesh.take() {
                if instance.is_instance_valid() {
                    self.base_mut().remove_child(&instance);
                    instance.queue_free();
                }
            }
            self.merged_mesh_dirty = false;
            return;
        }

        // Log floor chunks (Y=-1) specifically
        let floor_chunks: Vec<_> = chunks.iter().filter(|c| c.coord.y == -1).collect();
        debug_log(&format!(
            "[update_merged_stencil_mesh] Floor chunks (Y=-1): {}",
            floor_chunks.len()
        ));

        // Merge chunks using MeshPostProcessor (just merge, no other processing)
        let processor =
            MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, None);
        let combined = processor.merge_chunks(&chunks);

        debug_log(&format!(
            "[update_merged_stencil_mesh] Merged result: {} verts, {} tris",
            combined.vertex_count(),
            combined.triangle_count()
        ));

        if combined.vertices.is_empty() {
            debug_log("[update_merged_stencil_mesh] Merged mesh is EMPTY - skipping");
            self.merged_mesh_dirty = false;
            return;
        }

        // Convert to Godot ArrayMesh
        let mesh = self.combined_mesh_to_godot(&combined);

        // Create or update merged mesh instance
        if let Some(ref mut instance) = self.merged_stencil_mesh {
            if instance.is_instance_valid() {
                // Update existing instance with new mesh
                debug_log("[update_merged_stencil_mesh] Updating existing merged mesh instance");
                instance.set_mesh(&mesh);
            } else {
                // Instance became invalid, create new one
                debug_log(
                    "[update_merged_stencil_mesh] Existing instance invalid - will create new",
                );
                self.merged_stencil_mesh = None;
            }
        }

        if self.merged_stencil_mesh.is_none() {
            debug_log("[update_merged_stencil_mesh] Creating NEW merged mesh instance");
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_mesh(&mesh);
            instance.set_name("MergedStencilMesh");

            // Apply stencil-only material (stencil_write -> stencil_cap, no terrain)
            if let Some(ref mat) = self.cached_stencil_only_material {
                instance.set_surface_override_material(0, mat);
                debug_log("[update_merged_stencil_mesh] Applied stencil-only material");
            } else {
                debug_log(
                    "[update_merged_stencil_mesh] WARNING: No stencil-only material available!",
                );
            }

            self.base_mut().add_child(&instance);
            self.merged_stencil_mesh = Some(instance);
        }

        self.merged_mesh_dirty = false;
        debug_log(&format!(
            "[update_merged_stencil_mesh] COMPLETE: {} chunks -> {} verts, {} tris",
            chunks.len(),
            combined.vertex_count(),
            combined.triangle_count()
        ));
        debug_log("");

        if self.debug_logging {
            godot_print!(
                "PixyTerrain: Updated merged stencil mesh - {} chunks, {} vertices, {} triangles",
                chunks.len(),
                combined.vertex_count(),
                combined.triangle_count()
            );
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

    /// Debug function: analyze floor chunks and geometry below Y=0
    #[func]
    pub fn debug_floor(&self) {
        godot_print!("=== PixyTerrain Floor Debug ===");
        godot_print!("Cross-section enabled: {}", self.cross_section_enabled);
        godot_print!("Merged mesh dirty: {}", self.merged_mesh_dirty);

        // Check merged stencil mesh
        if let Some(ref mesh_instance) = self.merged_stencil_mesh {
            if mesh_instance.is_instance_valid() {
                godot_print!("Merged stencil mesh: EXISTS and VALID");
                if let Some(mesh) = mesh_instance.get_mesh() {
                    let surface_count = mesh.get_surface_count();
                    godot_print!("  Surface count: {}", surface_count);
                }
            } else {
                godot_print!("Merged stencil mesh: EXISTS but INVALID");
            }
        } else {
            godot_print!("Merged stencil mesh: NONE");
        }

        // Analyze Y=-1 floor chunks
        godot_print!("\n--- Floor Chunks (Y=-1) ---");
        let mut floor_chunks = Vec::new();
        let mut total_floor_verts = 0;
        let mut total_floor_tris = 0;

        for (coord, mesh_result) in &self.uploaded_meshes {
            if coord.y == -1 {
                floor_chunks.push(*coord);
                total_floor_verts += mesh_result.vertices.len();
                total_floor_tris += mesh_result.indices.len() / 3;

                // Check for vertices below Y=0
                let below_y0: Vec<_> = mesh_result.vertices.iter().filter(|v| v[1] < 0.0).collect();

                godot_print!(
                    "  Chunk ({}, -1, {}): {} verts, {} tris, {} verts below Y=0",
                    coord.x,
                    coord.z,
                    mesh_result.vertices.len(),
                    mesh_result.indices.len() / 3,
                    below_y0.len()
                );

                // Find Y range of this chunk's vertices
                if !mesh_result.vertices.is_empty() {
                    let min_y = mesh_result
                        .vertices
                        .iter()
                        .map(|v| v[1])
                        .fold(f32::INFINITY, f32::min);
                    let max_y = mesh_result
                        .vertices
                        .iter()
                        .map(|v| v[1])
                        .fold(f32::NEG_INFINITY, f32::max);
                    godot_print!("    Y range: {:.2} to {:.2}", min_y, max_y);
                }
            }
        }

        godot_print!(
            "\nTotal floor chunks: {}, verts: {}, tris: {}",
            floor_chunks.len(),
            total_floor_verts,
            total_floor_tris
        );

        // Analyze chunks at Y=0 (first layer above floor)
        godot_print!("\n--- Ground Level Chunks (Y=0) ---");
        let mut y0_chunks = 0;
        let mut y0_verts = 0;
        let mut y0_tris = 0;

        for (coord, mesh_result) in &self.uploaded_meshes {
            if coord.y == 0 {
                y0_chunks += 1;
                y0_verts += mesh_result.vertices.len();
                y0_tris += mesh_result.indices.len() / 3;
            }
        }

        godot_print!(
            "Total Y=0 chunks: {}, verts: {}, tris: {}",
            y0_chunks,
            y0_verts,
            y0_tris
        );

        // Check for gaps in floor coverage
        godot_print!("\n--- Floor Coverage Analysis ---");
        let chunk_size = self.voxel_size * self.chunk_subdivisions as f32;
        godot_print!(
            "Expected floor chunks: X from -1 to {}, Z from -1 to {}",
            self.map_width_x,
            self.map_width_z
        );

        let mut missing_floor = Vec::new();
        for x in -1..=self.map_width_x {
            for z in -1..=self.map_width_z {
                let coord = crate::chunk::ChunkCoord::new(x, -1, z);
                if !self.uploaded_meshes.contains_key(&coord) {
                    missing_floor.push(coord);
                }
            }
        }

        if missing_floor.is_empty() {
            godot_print!("All floor chunks present!");
        } else {
            godot_print!("MISSING {} floor chunks:", missing_floor.len());
            for coord in missing_floor.iter().take(10) {
                godot_print!("  ({}, -1, {})", coord.x, coord.z);
            }
            if missing_floor.len() > 10 {
                godot_print!("  ... and {} more", missing_floor.len() - 10);
            }
        }
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

        // 4. Free merged stencil mesh if it exists
        if let Some(mut instance) = self.merged_stencil_mesh.take() {
            if instance.is_instance_valid() {
                self.base_mut().remove_child(&instance);
                instance.queue_free();
            }
        }
        self.merged_mesh_dirty = false;

        // 5. Clear pending uploads buffer and uploaded mesh storage
        self.pending_uploads.clear();
        self.uploaded_meshes.clear();

        // 6. Drop systems
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

        let processor =
            MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, target);

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
            MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, None);

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
            MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, target);

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
            MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, None);

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
            MeshPostProcessor::new(self.weld_epsilon, self.normal_angle_threshold, target);

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
}
