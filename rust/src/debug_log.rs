//! Debug logging infrastructure for terrain investigation
//!
//! Writes to `debug_terrain.log` in the working directory.
//! The log file is recreated on each `init_debug_log()` call.

use std::fs::File;
use std::io::Write;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref DEBUG_LOG: Mutex<Option<File>> = Mutex::new(None);
}

/// Log a debug message to the terrain debug log file
pub fn debug_log(msg: &str) {
    if let Ok(mut guard) = DEBUG_LOG.lock() {
        if let Some(ref mut file) = *guard {
            let _ = writeln!(file, "{}", msg);
            let _ = file.flush();
        }
    }
}

/// Initialize the debug log file (overwrites any existing log)
pub fn init_debug_log() {
    if let Ok(mut guard) = DEBUG_LOG.lock() {
        *guard = File::create("debug_terrain.log").ok();
        if let Some(ref mut file) = *guard {
            let _ = writeln!(file, "=== PIXY TERRAIN DEBUG LOG ===");
            let _ = writeln!(file, "Timestamp: {:?}", std::time::SystemTime::now());
            let _ = writeln!(file, "");
        }
    }
}

/// Statistics about normals in a mesh
#[derive(Debug)]
pub struct NormalStats {
    pub min_len: f32,
    pub max_len: f32,
    pub degenerate_count: usize,
}

/// Compute statistics about normal vectors
/// A normal is considered degenerate if its length is not close to 1.0
pub fn compute_normal_stats(normals: &[[f32; 3]]) -> NormalStats {
    let mut min_len = f32::MAX;
    let mut max_len = f32::MIN;
    let mut degenerate_count = 0;

    for n in normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        min_len = min_len.min(len);
        max_len = max_len.max(len);

        // A normalized normal should have length ~1.0
        // Consider degenerate if outside [0.99, 1.01] or NaN
        if len < 0.99 || len > 1.01 || len.is_nan() {
            degenerate_count += 1;
        }
    }

    if normals.is_empty() {
        min_len = 0.0;
        max_len = 0.0;
    }

    NormalStats {
        min_len,
        max_len,
        degenerate_count,
    }
}

/// Count vertices that appear at identical positions (within epsilon)
/// Returns the number of duplicate position groups found
pub fn count_duplicate_positions(vertices: &[[f32; 3]], epsilon: f32) -> usize {
    use std::collections::HashMap;

    // Quantize positions to grid cells for fast lookup
    let scale = 1.0 / epsilon;
    let mut position_counts: HashMap<(i32, i32, i32), usize> = HashMap::new();

    for v in vertices {
        let key = (
            (v[0] * scale).round() as i32,
            (v[1] * scale).round() as i32,
            (v[2] * scale).round() as i32,
        );
        *position_counts.entry(key).or_insert(0) += 1;
    }

    // Count positions with more than one vertex
    position_counts.values().filter(|&&count| count > 1).count()
}

/// Check if a position is near a box boundary
pub fn is_boundary_position(
    pos: [f32; 3],
    box_min: [f32; 3],
    box_max: [f32; 3],
    tolerance: f32,
) -> bool {
    // Check each axis for proximity to boundary
    (pos[0] - box_min[0]).abs() < tolerance
        || (pos[0] - box_max[0]).abs() < tolerance
        || (pos[1] - box_min[1]).abs() < tolerance
        || (pos[1] - box_max[1]).abs() < tolerance
        || (pos[2] - box_min[2]).abs() < tolerance
        || (pos[2] - box_max[2]).abs() < tolerance
}
