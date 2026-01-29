//! Parallel mesh generation using bevy_tasks.
//!
//! Worker threads perform pure Rust computation.
//! Results are sent via crossbean channels to the main thread.

use bevy_tasks::{TaskPool, TaskPoolBuilder};
use crossbeam::channel::{bounded, Receiver, Sender, TrySelectError};
use godot::prelude::godot_print;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::chunk::{ChunkCoord, MeshResult};
use crate::mesh_extraction::extract_chunk_mesh;
use crate::noise_field::NoiseField;

// Debug counters
static TASKS_SPAWNED: AtomicU64 = AtomicU64::new(0);
static TASKS_COMPLETED: AtomicU64 = AtomicU64::new(0);

// Track unique thread IDs used
use std::collections::HashSet;
use std::sync::Mutex;
static THREADS_USED: Mutex<Option<HashSet<std::thread::ThreadId>>> = Mutex::new(None);

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
    pub fn new(num_threads: usize, channel_capacity: usize) -> Self {
        let detected_cpus = num_cpus::get();
        let threads = if num_threads == 0 {
            ((detected_cpus * 3) / 4).max(2)
        } else {
            num_threads
        };

        #[cfg(not(test))]
        godot_print!(
            "[MeshWorker] CPU detection: {} cores, requested {} threads, using {} threads, channel capacity {}",
            detected_cpus,
            num_threads,
            threads,
            channel_capacity
        );

        let pool = TaskPoolBuilder::new()
            .num_threads(threads)
            .thread_name(String::from("MeshWorker"))
            .build();

        #[cfg(not(test))]
        godot_print!(
            "[MeshWorker] TaskPool created with {} threads, rayon has {} threads",
            pool.thread_num(),
            rayon::current_num_threads()
        );

        let (request_tx, request_rx) = bounded(channel_capacity);
        let (result_tx, result_rx) = bounded(channel_capacity);

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
        // Process batch_size requests at a time - use rayon's thread count
        let batch_size = rayon::current_num_threads().max(16);
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

        let count = batch.len();
        TASKS_SPAWNED.fetch_add(count as u64, Ordering::Relaxed);
        #[cfg(not(test))]
        godot_print!(
            "[MeshWorker] Processing {} of {} batch (rayon threads: {}, spawned: {}, completed: {})",
            count,
            batch_size,
            rayon::current_num_threads(),
            TASKS_SPAWNED.load(Ordering::Relaxed),
            TASKS_COMPLETED.load(Ordering::Relaxed)
        );

        let result_tx = self.result_tx.clone();

        // Reset thread tracking for this batch
        {
            let mut guard = THREADS_USED.lock().unwrap();
            *guard = Some(HashSet::new());
        }

        // Use rayon for true parallel processing
        rayon::scope(|scope| {
            for request in batch {
                let tx = result_tx.clone();
                scope.spawn(move |_| {
                    // Track which thread we're on
                    let thread_id = std::thread::current().id();
                    {
                        let mut guard = THREADS_USED.lock().unwrap();
                        if let Some(ref mut set) = *guard {
                            set.insert(thread_id);
                        }
                    }

                    let mesh = generate_mesh_for_request(&request);
                    TASKS_COMPLETED.fetch_add(1, Ordering::Relaxed);
                    let _ = tx.try_send(mesh);
                });
            }
        });

        // Report how many unique threads were used
        #[cfg(not(test))]
        {
            let guard = THREADS_USED.lock().unwrap();
            if let Some(ref set) = *guard {
                godot_print!("[MeshWorker] Batch used {} unique threads", set.len());
            }
        }
    }

    pub fn thread_count(&self) -> usize {
        self.pool.thread_num()
    }
}

impl Default for MeshWorkerPool {
    fn default() -> Self {
        Self::new(0, 256)
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
        use bevy_tasks::TaskPoolBuilder;

        // Test bevy_tasks directly first
        let direct_pool = TaskPoolBuilder::new()
            .num_threads(4)
            .thread_name(String::from("DirectTest"))
            .build();
        println!(
            "Direct TaskPool: requested 4 threads, got {} threads",
            direct_pool.thread_num()
        );

        // Now test through our wrapper
        let requested = 4;
        let pool = MeshWorkerPool::new(requested, 64);
        println!(
            "MeshWorkerPool: requested {} threads, got {} threads",
            requested,
            pool.thread_count()
        );

        // This assertion may fail - that's what we're investigating
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
