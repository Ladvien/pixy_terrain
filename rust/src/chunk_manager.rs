//! Chunk lifecycle management with LOD selection

use std::collections::HashMap;
use std::sync::Arc;

use crossbeam::channel::{Receiver, Sender};

use crate::chunk::{Chunk, ChunkCoord, ChunkState, MeshResult};
use crate::lod::LODConfig;
use crate::mesh_worker::MeshRequest;
use crate::noise_field::NoiseField;
use crate::terrain_modifications::ModificationLayer;
use crate::texture_layer::TextureLayer;

/// Maximum number of mesh results to collect per update cycle.
const DEFAULT_MAX_RESULTS_PER_UPDATE: usize = 64;

pub struct ChunkManager {
    chunks: HashMap<ChunkCoord, Chunk>,
    lod_config: LODConfig,
    base_voxel_size: f32,
    request_tx: Sender<MeshRequest>,
    result_rx: Receiver<MeshResult>,
    current_frame: u64,
    max_results_per_update: usize,
    map_width: i32,
    map_height: i32,
    map_depth: i32,
}

impl ChunkManager {
    pub fn new(
        lod_config: LODConfig,
        base_voxel_size: f32,
        request_tx: Sender<MeshRequest>,
        result_rx: Receiver<MeshResult>,
        map_width: i32,
        map_height: i32,
        map_depth: i32,
    ) -> Self {
        Self {
            chunks: HashMap::new(),
            lod_config,
            base_voxel_size,
            request_tx,
            result_rx,
            current_frame: 0,
            max_results_per_update: DEFAULT_MAX_RESULTS_PER_UPDATE,
            map_width,
            map_height,
            map_depth,
        }
    }

    fn chunk_size(&self) -> f32 {
        self.base_voxel_size * self.lod_config.chunk_subdivisions as f32
    }

    /// Update with optional modification and texture layers
    pub fn update_with_layers(
        &mut self,
        camera_pos: [f32; 3],
        noise_field: &Arc<NoiseField>,
        modifications: Option<&Arc<ModificationLayer>>,
        textures: Option<&Arc<TextureLayer>>,
    ) -> Vec<MeshResult> {
        self.current_frame += 1;
        let desired = self.compute_desired_chunks(camera_pos);

        for (coord, desired_lod) in &desired {
            self.ensure_chunk_requested(
                *coord,
                *desired_lod,
                noise_field,
                &desired,
                modifications,
                textures,
            );
        }

        self.mark_distant_for_unload(&desired);

        let mut results = Vec::new();
        while let Ok(result) = self.result_rx.try_recv() {
            if let Some(chunk) = self.chunks.get_mut(&result.coord) {
                chunk.state = ChunkState::Ready;
                chunk.lod_level = result.lod_level;
            }
            results.push(result);

            if results.len() >= self.max_results_per_update {
                break;
            }
        }

        results
    }

    fn compute_desired_chunks(&self, camera_pos: [f32; 3]) -> HashMap<ChunkCoord, u8> {
        let mut desired = HashMap::new();
        let chunk_size = self.chunk_size();
        let view_distance = self.lod_config.max_view_distance();

        let cam_cx = (camera_pos[0] / chunk_size).floor() as i32;
        let cam_cy = (camera_pos[1] / chunk_size).floor() as i32;
        let cam_cz = (camera_pos[2] / chunk_size).floor() as i32;

        let view_chunks = (view_distance / chunk_size).ceil() as i32 + 1;

        for dx in -view_chunks..=view_chunks {
            for dy in -view_chunks..=view_chunks {
                for dz in -view_chunks..=view_chunks {
                    let coord = ChunkCoord::new(cam_cx + dx, cam_cy + dy, cam_cz + dz);

                    // Boundary checks: allow guard chunks on all boundaries for wall generation
                    // Transvoxel needs samples on BOTH sides of surfaces to generate geometry.
                    //
                    // For a box from [0,0,0] to [max], we need:
                    // - X=-1 chunk to capture X=0 wall (samples X from -chunk_size to 0)
                    // - X=map_width chunk to capture X=max wall (samples X from max to max+chunk_size)
                    // - Same logic for Y and Z
                    if coord.x < -1 || coord.x > self.map_width {
                        continue;
                    }
                    if coord.z < -1 || coord.z > self.map_depth {
                        continue;
                    }
                    if coord.y < -1 || coord.y >= self.map_height {
                        continue;
                    }

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

    fn ensure_chunk_requested(
        &mut self,
        coord: ChunkCoord,
        desired_lod: u8,
        noise_field: &Arc<NoiseField>,
        desired: &HashMap<ChunkCoord, u8>,
        modifications: Option<&Arc<ModificationLayer>>,
        textures: Option<&Arc<TextureLayer>>,
    ) -> bool {
        let needs_request = match self.chunks.get(&coord) {
            Some(chunk) => {
                chunk.state == ChunkState::Unloaded
                    || (chunk.lod_level != desired_lod && chunk.state != ChunkState::Pending)
            }
            None => true,
        };

        if needs_request {
            let transition_sides = self.compute_transition_sides(coord, desired_lod, desired);

            let request = MeshRequest {
                coord,
                lod_level: desired_lod,
                transition_sides,
                noise_field: Arc::clone(noise_field),
                base_voxel_size: self.base_voxel_size,
                chunk_size: self.chunk_size(),
                modifications: modifications.map(Arc::clone),
                textures: textures.map(Arc::clone),
            };

            if self.request_tx.try_send(request).is_ok() {
                self.chunks
                    .entry(coord)
                    .and_modify(|c| {
                        c.state = ChunkState::Pending;
                        c.last_access_frame = self.current_frame;
                    })
                    .or_insert_with(|| {
                        let mut chunk = Chunk::new(desired_lod);
                        chunk.state = ChunkState::Pending;
                        chunk.last_access_frame = self.current_frame;
                        chunk
                    });
                return true;
            }
        } else if let Some(chunk) = self.chunks.get_mut(&coord) {
            chunk.last_access_frame = self.current_frame;
        }
        false
    }

    fn compute_transition_sides(
        &self,
        coord: ChunkCoord,
        lod: u8,
        desired: &HashMap<ChunkCoord, u8>,
    ) -> u8 {
        if lod == 0 {
            return 0;
        }

        let mut sides = 0u8;
        let neighbors = [
            (ChunkCoord::new(coord.x - 1, coord.y, coord.z), 0b000001), // LowX
            (ChunkCoord::new(coord.x + 1, coord.y, coord.z), 0b000010), // HighX
            (ChunkCoord::new(coord.x, coord.y - 1, coord.z), 0b000100), // LowY
            (ChunkCoord::new(coord.x, coord.y + 1, coord.z), 0b001000), // HighY
            (ChunkCoord::new(coord.x, coord.y, coord.z - 1), 0b010000), // LowZ
            (ChunkCoord::new(coord.x, coord.y, coord.z + 1), 0b100000), // HighZ
        ];

        for (neigbor_coord, flag) in neighbors {
            if let Some(&neighbor_lod) = desired.get(&neigbor_coord) {
                if neighbor_lod < lod {
                    sides |= flag;
                }
            }
        }

        sides
    }

    fn mark_distant_for_unload(&mut self, desired: &HashMap<ChunkCoord, u8>) {
        for (coord, chunk) in self.chunks.iter_mut() {
            if !desired.contains_key(coord) && chunk.state != ChunkState::Pending {
                chunk.state = ChunkState::MarkedForUnload;
            }
        }
    }

    pub fn get_unload_candidates(&self) -> Vec<(ChunkCoord, Option<i64>)> {
        self.chunks
            .iter()
            .filter(|(_, c)| c.state == ChunkState::MarkedForUnload)
            .map(|(coord, c)| (*coord, c.mesh_instance_id))
            .collect()
    }

    pub fn remove_chunk(&mut self, coord: &ChunkCoord) {
        self.chunks.remove(coord);
    }

    pub fn mark_chunk_active(&mut self, coord: &ChunkCoord, instance_id: i64) {
        if let Some(chunk) = self.chunks.get_mut(coord) {
            chunk.state = ChunkState::Active;
            chunk.mesh_instance_id = Some(instance_id);
        }
    }

    pub fn clear_all_chunks(&mut self) {
        self.chunks.clear();
    }

    /// Mark chunks as needing regeneration (e.g., after terrain modifications)
    /// Returns the number of chunks marked dirty
    pub fn mark_chunks_dirty(&mut self, coords: &[ChunkCoord]) -> usize {
        let mut count = 0;
        for coord in coords {
            if let Some(chunk) = self.chunks.get_mut(coord) {
                // Reset to trigger re-request on next update
                if chunk.state == ChunkState::Active || chunk.state == ChunkState::Ready {
                    chunk.state = ChunkState::Unloaded;
                    count += 1;
                }
            }
        }
        count
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::channel::bounded;

    fn test_manager() -> (ChunkManager, Receiver<MeshRequest>, Sender<MeshResult>) {
        let (req_tx, req_rx) = bounded(64);
        let (res_tx, res_rx) = bounded(64);

        let config = LODConfig::new(64.0, 4, 32);
        let manager = ChunkManager::new(config, 1.0, req_tx, res_rx, 10, 4, 10);

        (manager, req_rx, res_tx)
    }

    fn test_noise() -> Arc<crate::noise_field::NoiseField> {
        Arc::new(crate::noise_field::NoiseField::new(
            42, 4, 0.02, 10.0, 0.0, 32.0, None,
        ))
    }

    #[test]
    fn test_compute_desired_chunks_at_origin() {
        let (manager, _, _) = test_manager();

        let desired = manager.compute_desired_chunks([0.0, 0.0, 0.0]);

        assert!(desired.contains_key(&ChunkCoord::new(0, 0, 0)));
        assert_eq!(desired.get(&ChunkCoord::new(0, 0, 0)), Some(&0));
    }

    #[test]
    fn test_lod_increases_with_distance() {
        let (manager, _, _) = test_manager();

        let desired = manager.compute_desired_chunks([0.0, 0.0, 0.0]);

        if let Some(&lod) = desired.get(&ChunkCoord::new(1, 0, 0)) {
            assert!(lod <= 1, "Near chunk should be LOD 0 or 1");
        }

        if let Some(&lod) = desired.get(&ChunkCoord::new(10, 0, 0)) {
            assert!(lod >= 2, "Far chunk should be LOD 2+, got {lod}");
        }
    }

    #[test]
    fn test_transition_sides_computation() {
        let (mut manager, _, _) = test_manager();

        let neighbor = Chunk::new(0);
        manager.chunks.insert(ChunkCoord::new(-1, 0, 0), neighbor);

        // Create a desired map with LOD levels for testing
        let mut desired: HashMap<ChunkCoord, u8> = HashMap::new();
        desired.insert(ChunkCoord::new(0, 0, 0), 1);
        desired.insert(ChunkCoord::new(-1, 0, 0), 0);

        let sides = manager.compute_transition_sides(ChunkCoord::new(0, 0, 0), 1, &desired);

        assert!(sides & 0b000001 != 0, "Should have LowX transition");
        assert!(sides & 0b000010 == 0, "Should not have HighX transition");
    }

    #[test]
    fn test_no_transition_at_lod_0() {
        let (manager, _, _) = test_manager();

        let desired: HashMap<ChunkCoord, u8> = HashMap::new();
        let sides = manager.compute_transition_sides(ChunkCoord::new(0, 0, 0), 0, &desired);
        assert_eq!(sides, 0);
    }

    #[test]
    fn test_chunk_request_sent() {
        let (mut manager, req_rx, _) = test_manager();
        let noise = test_noise();

        let _ = manager.update_with_layers([0.0, 0.0, 0.0], &noise, None, None);

        let mut request_count = 0;
        while req_rx.try_recv().is_ok() {
            request_count += 1;
        }

        assert!(request_count > 0, "Should have sent chunk requests");
    }

    #[test]
    fn test_result_updates_chunk_state() {
        let (mut manager, _, res_tx) = test_manager();
        let noise = test_noise();

        let _ = manager.update_with_layers([0.0, 0.0, 0.0], &noise, None, None);

        let result = MeshResult {
            coord: ChunkCoord::new(0, 0, 0),
            lod_level: 0,
            vertices: vec![[0.0, 0.0, 0.0]],
            normals: vec![[0.0, 1.0, 0.0]],
            indices: vec![0],
            colors: vec![[1.0, 0.0, 0.0, 0.0]],
        };
        res_tx.send(result).unwrap();

        let results = manager.update_with_layers([0.0, 0.0, 0.0], &noise, None, None);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].coord, ChunkCoord::new(0, 0, 0));
    }

    #[test]
    fn test_distant_chunks_marked_for_unload() {
        // Use smaller config to avoid channel overflow
        let (req_tx, _req_rx) = bounded(64);
        let (res_tx, res_rx) = bounded(64);
        let config = LODConfig::new(32.0, 2, 32); // Smaller view distance
        let mut manager = ChunkManager::new(config, 1.0, req_tx, res_rx, 10, 4, 10);
        let noise = test_noise();

        // Load chunks at origin
        let _ = manager.update_with_layers([0.0, 0.0, 0.0], &noise, None, None);

        // Manually insert and mark origin chunk as active (bypass channel limits)
        manager.chunks.insert(
            ChunkCoord::new(0, 0, 0),
            Chunk::new(0),
        );
        manager.mark_chunk_active(&ChunkCoord::new(0, 0, 0), 123);

        // Move camera very far away
        let _ = manager.update_with_layers([10000.0, 0.0, 0.0], &noise, None, None);

        // Original chunk should be marked for unload
        let unload = manager.get_unload_candidates();
        let has_origin = unload.iter().any(|(c, _)| *c == ChunkCoord::new(0, 0, 0));
        assert!(has_origin, "Origin chunk should be marked for unload");
    }

    #[test]
    fn test_floor_chunks_generated() {
        // Test that Y=-1 chunks are included for floor surface generation
        // The box SDF floor is at Y=0, transvoxel needs samples on both sides
        let (req_tx, _req_rx) = bounded(128);
        let (_res_tx, res_rx) = bounded(64);
        let config = LODConfig::new(64.0, 4, 32);
        let manager = ChunkManager::new(config, 1.0, req_tx, res_rx, 4, 4, 4);

        // Camera at Y=16 (middle of chunk 0)
        let desired = manager.compute_desired_chunks([16.0, 16.0, 16.0]);

        // Should include Y=-1 chunk for floor
        assert!(
            desired.contains_key(&ChunkCoord::new(0, -1, 0)),
            "Should include Y=-1 chunk for floor surface"
        );

        // Should also include Y=0 through Y=map_height-1
        assert!(
            desired.contains_key(&ChunkCoord::new(0, 0, 0)),
            "Should include Y=0 chunk"
        );
    }

    #[test]
    fn test_wall_chunks_generated() {
        // Test that guard chunks are generated for all wall boundaries
        // Transvoxel needs samples on BOTH sides of surfaces to generate geometry
        let (req_tx, _req_rx) = bounded(256);
        let (_res_tx, res_rx) = bounded(64);
        let config = LODConfig::new(128.0, 4, 32); // Large view distance
        let manager = ChunkManager::new(config, 1.0, req_tx, res_rx, 4, 4, 4);

        // Camera at center of terrain
        let desired = manager.compute_desired_chunks([64.0, 64.0, 64.0]);

        // Should include guard chunks for all walls
        assert!(
            desired.contains_key(&ChunkCoord::new(-1, 0, 0)),
            "Should include X=-1 chunk for X=0 wall"
        );
        assert!(
            desired.contains_key(&ChunkCoord::new(0, 0, -1)),
            "Should include Z=-1 chunk for Z=0 wall"
        );
        assert!(
            desired.contains_key(&ChunkCoord::new(4, 0, 0)), // map_width=4
            "Should include X=map_width chunk for X=max wall"
        );
        assert!(
            desired.contains_key(&ChunkCoord::new(0, 0, 4)), // map_depth=4
            "Should include Z=map_depth chunk for Z=max wall"
        );
    }

    #[test]
    fn test_default_terrain_chunks() {
        // Test with EXACT default terrain settings from terrain.rs
        let (req_tx, _req_rx) = bounded(512);
        let (_res_tx, res_rx) = bounded(64);

        // Default settings:
        // lod_base_distance = 64.0, max_lod_level = 4, chunk_subdivisions = 32
        // map_width_x = 10, map_height_y = 4, map_width_z = 10
        // voxel_size = 1.0
        let config = LODConfig::new(64.0, 4, 32);
        let manager = ChunkManager::new(config, 1.0, req_tx, res_rx, 10, 4, 10);

        // chunk_size = 1.0 * 32 = 32
        // map bounds: [0,0,0] to [320, 128, 320]

        // Test with camera at terrain center
        let center_cam = [160.0, 64.0, 160.0];
        let desired = manager.compute_desired_chunks(center_cam);

        println!("\n=== Default terrain chunk analysis ===");
        println!("Camera at {:?}", center_cam);
        println!("Total desired chunks: {}", desired.len());

        // Count boundary chunks
        let x_neg1: Vec<_> = desired.keys().filter(|c| c.x == -1).collect();
        let z_neg1: Vec<_> = desired.keys().filter(|c| c.z == -1).collect();
        let y_neg1: Vec<_> = desired.keys().filter(|c| c.y == -1).collect();
        let x_max: Vec<_> = desired.keys().filter(|c| c.x == 10).collect();
        let z_max: Vec<_> = desired.keys().filter(|c| c.z == 10).collect();

        println!("X=-1 chunks (wall): {}", x_neg1.len());
        println!("Z=-1 chunks (wall): {}", z_neg1.len());
        println!("Y=-1 chunks (floor): {}", y_neg1.len());
        println!("X=10 chunks (wall): {}", x_max.len());
        println!("Z=10 chunks (wall): {}", z_max.len());

        // All boundary chunks should be present (view distance is 512 > terrain size 320)
        assert!(!x_neg1.is_empty(), "Should have X=-1 wall chunks");
        assert!(!z_neg1.is_empty(), "Should have Z=-1 wall chunks");
        assert!(!y_neg1.is_empty(), "Should have Y=-1 floor chunks");
        assert!(!x_max.is_empty(), "Should have X=max wall chunks");
        assert!(!z_max.is_empty(), "Should have Z=max wall chunks");
    }
}
