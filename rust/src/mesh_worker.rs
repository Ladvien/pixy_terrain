//! Parallel mesh generation using bevy_tasks.
//!
//! Worker threads perform pure Rust computation.
//! Results are sent via crossbean channels to the main thread.

use bevy_tasks::{TaskPool, TaskPoolBuilder};
use crossbeam::channel::{bounded, Receiver, Sender, TrySelectError};
use std::sync::Arc;

use crate::chunk::{ChunkCoord, MeshResult};
use crate::mesh_extraction::extract_chunk_mesh;
use crate::noise_field::NoiseField;

/// Request sent from main thread to workers
pub struct MeshRequest {
    pub coord: ChunkCoord,
    pub lod_level: u8,
    pub transition_sides: u8,
    pub noise_field: Arc<NoiseField>,
    pub base_voxel_size: f32,
    pub chunk_size: f32,
}

/// Worker pool for parallel mesh generation
pub struct MeshWorkerPool {
    pool: TaskPool,
    request_tx: Sender<MeshRequest>,
    request_rx: Receiver<MeshRequest>,
    result_tx: Sender<MeshResult>,
    result_rx: Receiver<MeshResult>,
}

impl MeshWorkerPool {
    pub fn new(num_threads: usize) -> Self {
        let threads = if num_threads == 0 {
            (num_cpus::get() / 2).max(1)
        } else {
            num_threads
        };

        let pool = TaskPoolBuilder::new()
            .num_threads(threads)
            .thread_name(String::from("MeshWorker"))
            .build();

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

    pub fn request_sender(&self) -> Sender<MeshRequest> {
        self.request_tx.clone()
    }

    pub fn result_receiver(&self) -> Receiver<MeshResult> {
        self.result_rx.clone()
    }

    pub fn process_requests(&self) {
        let mut requests = Vec::new();
        while let Ok(req) = self.request_rx.try_recv() {
            requests.push(req)
        }

        if requests.is_empty() {
            return;
        }
        for request in requests {
            let tx = self.result_tx.clone();
            self.pool
                .spawn(async move {
                    let mesh = generate_mesh_for_request(&request);
                    let _ = tx.try_send(mesh);
                })
                .detach();
        }
    }

    pub fn thread_count(&self) -> usize {
        self.pool.thread_num()
    }
}

impl Default for MeshWorkerPool {
    fn default() -> Self {
        Self::new(0)
    }
}

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

#[cfg(test)]
mod tests {
    use crossbeam::channel::TrySendError;

    use super::*;
    use std::time::Duration;

    fn test_noise() -> Arc<NoiseField> {
        Arc::new(NoiseField::new(42, 4, 0.02, 10.0, 0.0))
    }

    #[test]
    fn test_worker_pool_creation() {
        let pool = MeshWorkerPool::new(2);
        assert!(pool.thread_count() >= 1);
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

        pool.process_requests();

        let result = pool
            .result_receiver()
            .recv_timeout(Duration::from_secs(5))
            .expect("Should receive result");

        assert_eq!(result.coord, ChunkCoord::new(0, 0, 0));
    }

    #[test]
    fn test_multiple_requests_parallel() {
        let pool = MeshWorkerPool::new(4);
        let noise = test_noise();

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

        pool.process_requests();

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

        assert!(sent <= 64, "Should stop at channel capacity");
        assert!(sent > 0, "Should have sent some requests");
    }
}
