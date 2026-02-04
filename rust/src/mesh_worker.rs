//! Parallel mesh generation worker pool.
//!
//! Worker threads perform pure Rust computation.
//! Results are sent via crossbeam channels to the main thread.

use crossbeam::channel::{bounded, Receiver, Sender};
use std::sync::Arc;

use crate::chunk::{ChunkCoord, MeshResult};
use crate::mesh_extraction::extract_chunk_mesh;
use crate::noise_field::NoiseField;
use crate::terrain_modifications::ModificationLayer;
use crate::texture_layer::TextureLayer;

/// Fraction of detected CPUs to use for mesh worker threads (numerator).
const THREAD_CPU_NUMERATOR: usize = 3;
/// Fraction of detected CPUs to use for mesh worker threads (denominator).
const THREAD_CPU_DENOMINATOR: usize = 4;
/// Minimum number of mesh worker threads.
const MIN_WORKER_THREADS: usize = 2;
/// Minimum batch size for processing mesh requests.
const MIN_BATCH_SIZE: usize = 16;
/// Default channel capacity for mesh request/result channels.
const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Request sent from main thread to workers
pub struct MeshRequest {
    pub coord: ChunkCoord,
    pub lod_level: u8,
    pub transition_sides: u8,
    pub noise_field: Arc<NoiseField>,
    pub base_voxel_size: f32,
    pub chunk_size: f32,
    /// Optional modification layer for terrain edits
    pub modifications: Option<Arc<ModificationLayer>>,
    /// Optional texture layer for multi-texture blending
    pub textures: Option<Arc<TextureLayer>>,
}

/// Worker pool for parallel mesh generation
pub struct MeshWorkerPool {
    thread_count: usize,
    request_tx: Sender<MeshRequest>,
    request_rx: Receiver<MeshRequest>,
    result_tx: Sender<MeshResult>,
    result_rx: Receiver<MeshResult>,
}

impl MeshWorkerPool {
    pub fn new(num_threads: usize, channel_capacity: usize) -> Self {
        let detected_cpus = num_cpus::get();
        let threads = if num_threads == 0 {
            ((detected_cpus * THREAD_CPU_NUMERATOR) / THREAD_CPU_DENOMINATOR)
                .max(MIN_WORKER_THREADS)
        } else {
            num_threads
        };

        let (request_tx, request_rx) = bounded(channel_capacity);
        let (result_tx, result_rx) = bounded(channel_capacity);

        Self {
            thread_count: threads,
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
        let batch_size = rayon::current_num_threads().max(MIN_BATCH_SIZE);
        let mut batch = Vec::with_capacity(batch_size);

        while batch.len() < batch_size {
            match self.request_rx.try_recv() {
                Ok(req) => batch.push(req),
                Err(_) => break,
            }
        }

        if batch.is_empty() {
            return;
        }

        let result_tx = self.result_tx.clone();

        rayon::scope(|scope| {
            for request in batch {
                let tx = result_tx.clone();
                scope.spawn(move |_| {
                    let mesh = generate_mesh_for_request(&request);
                    let _ = tx.try_send(mesh);
                });
            }
        });
    }

    pub fn thread_count(&self) -> usize {
        self.thread_count
    }

    pub fn shutdown(&mut self) {
        while self.request_rx.try_recv().is_ok() {}
        while self.result_rx.try_recv().is_ok() {}
    }
}

impl Default for MeshWorkerPool {
    fn default() -> Self {
        Self::new(0, DEFAULT_CHANNEL_CAPACITY)
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
        request.modifications.as_deref(),
        request.textures.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use crossbeam::channel::TrySendError;

    use super::*;
    use std::time::Duration;

    fn test_noise() -> Arc<NoiseField> {
        Arc::new(NoiseField::new(42, 4, 0.02, 10.0, 0.0, 32.0, None))
    }

    #[test]
    fn test_worker_pool_creation() {
        let pool = MeshWorkerPool::new(2, 64);
        assert!(pool.thread_count() >= 1);
    }

    #[test]
    fn test_worker_pool_default() {
        let pool = MeshWorkerPool::default();
        assert!(pool.thread_count() >= 1);
    }

    #[test]
    fn test_thread_count_matches_requested() {
        let requested = 4;
        let pool = MeshWorkerPool::new(requested, 64);

        assert_eq!(
            pool.thread_count(),
            requested,
            "Thread count should match requested"
        );
    }

    #[test]
    fn test_send_and_receive_mesh() {
        let pool = MeshWorkerPool::new(2, 64);
        let noise = test_noise();

        let request = MeshRequest {
            coord: ChunkCoord::new(0, 0, 0),
            lod_level: 0,
            transition_sides: 0,
            noise_field: noise,
            base_voxel_size: 1.0,
            chunk_size: 32.0,
            modifications: None,
            textures: None,
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
        let pool = MeshWorkerPool::new(4, 64);
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
                modifications: None,
                textures: None,
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
        let pool = MeshWorkerPool::new(1, 64);
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
                modifications: None,
                textures: None,
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
