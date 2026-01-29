//! Chunk lifecycle management with LOD selection

use std::collections::HashMap;
use std::sync::Arc;

use crossbeam::channel::{Receiver, Sender};

use crate::chunk::{self, Chunk, ChunkCoord, ChunkState, MeshResult};
use crate::lod::LODConfig;
use crate::mesh_worker::MeshRequest;
use crate::noise_field::NoiseField;

pub struct ChunkManager {
    chunks: HashMap<ChunkCoord, Chunk>,
    lod_config: LODConfig,
    base_voxel_size: f32,
    request_tx: Sender<MeshRequest>,
    result_rx: Receiver<MeshResult>,
    current_frame: u64,
    max_results_per_update: usize,
}

impl ChunkManager {
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
            max_results_per_update: 16,
        }
    }

    fn chunk_size(&self) -> f32 {
        self.base_voxel_size * self.lod_config.chunk_subdivisions as f32
    }

    pub fn update(
        &mut self,
        camera_pos: [f32; 3],
        noise_field: &Arc<NoiseField>,
    ) -> Vec<MeshResult> {
        self.current_frame += 1;
        let desired = self.compute_desired_chunks(camera_pos);

        for (coord, desired_lod) in &desired {
            self.ensure_chunk_requested(*coord, *desired_lod, noise_field);
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
    ) {
        let needs_request = match self.chunks.get(&coord) {
            Some(chunk) => chunk.lod_level != desired_lod && chunk.state != ChunkState::Pending,
            None => true,
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

            if self.request_tx.try_send(request).is_ok() {
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
            chunk.last_access_frame = self.current_frame;
        }
    }

    fn compute_transition_sides(&self, coord: ChunkCoord, lod: u8) -> u8 {
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
            if let Some(neighbor) = self.chunks.get(&neigbor_coord) {
                if neighbor.lod_level < lod {
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

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn active_chunk_count(&self) -> usize {
        self.chunks
            .values()
            .filter(|c| c.state == ChunkState::Active)
            .count()
    }
}

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

        let neighbor = Chunk::new(ChunkCoord::new(-1, 0, 0), 0);
        manager.chunks.insert(ChunkCoord::new(-1, 0, 0), neighbor);

        let sides = manager.compute_transition_sides(ChunkCoord::new(0, 0, 0), 1);

        assert!(sides & 0b000001 != 0, "Should have LowX transition");
        assert!(sides & 0b000010 == 0, "Should not have HighX transition");
    }

    #[test]
    fn test_no_transition_at_lod_0() {
        let (manager, _, _) = test_manager();

        let sides = manager.compute_transition_sides(ChunkCoord::new(0, 0, 0), 0);
        assert_eq!(sides, 0);
    }

    #[test]
    fn test_chunk_request_sent() {
        let (mut manager, req_rx, _) = test_manager();
        let noise = Arc::new(crate::noise_field::NoiseField::new(42, 4, 0.02, 10.0, 0.0));

        let _ = manager.update([0.0, 0.0, 0.0], &noise);

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

        let _ = manager.update([0.0, 0.0, 0.0], &noise);

        let result = MeshResult {
            coord: ChunkCoord::new(0, 0, 0),
            lod_level: 0,
            vertices: vec![[0.0, 0.0, 0.0]],
            normals: vec![[0.0, 1.0, 0.0]],
            indices: vec![0],
            transition_sides: 0,
        };
        res_tx.send(result).unwrap();

        let results = manager.update([0.0, 0.0, 0.0], &noise);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].coord, ChunkCoord::new(0, 0, 0));
    }

    #[test]
    fn test_distant_chunks_marked_for_unload() {
        // Use smaller config to avoid channel overflow
        let (req_tx, _req_rx) = bounded(64);
        let (res_tx, res_rx) = bounded(64);
        let config = LODConfig::new(32.0, 2); // Smaller view distance
        let mut manager = ChunkManager::new(config, 1.0, req_tx, res_rx);
        let noise = Arc::new(crate::noise_field::NoiseField::new(42, 4, 0.02, 10.0, 0.0));

        // Load chunks at origin
        let _ = manager.update([0.0, 0.0, 0.0], &noise);

        // Manually insert and mark origin chunk as active (bypass channel limits)
        manager.chunks.insert(
            ChunkCoord::new(0, 0, 0),
            Chunk::new(ChunkCoord::new(0, 0, 0), 0),
        );
        manager.mark_chunk_active(&ChunkCoord::new(0, 0, 0), 123);

        // Move camera very far away
        let _ = manager.update([10000.0, 0.0, 0.0], &noise);

        // Original chunk should be marked for unload
        let unload = manager.get_unload_candidates();
        let has_origin = unload.iter().any(|(c, _)| *c == ChunkCoord::new(0, 0, 0));
        assert!(has_origin, "Origin chunk should be marked for unload");
    }
}
