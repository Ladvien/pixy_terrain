use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Counter for boundary sample logging (to avoid excessive output)
static BOUNDARY_LOG_COUNT: AtomicUsize = AtomicUsize::new(0);
const MAX_BOUNDARY_LOGS: usize = 100;

/// Noise-based sign distance field for terrain generation
/// Negative = inside terrain (solid)
/// Positive = outside terrain (air)
/// Zero = surface
pub struct NoiseField {
    fbm: Fbm<Perlin>,
    amplitude: f32,
    height_offset: f32,
    floor_y: f32,
    box_min: Option<[f32; 3]>,
    box_max: Option<[f32; 3]>,
    /// Blend width for smooth CSG intersection (0 = hard max, >0 = smooth transition)
    /// Recommended: 1.0 to 2.0 * voxel_size for smooth normals at terrain-wall junctions
    csg_blend_width: f32,
    /// Extension applied to box bounds for SDF calculation.
    /// Set to chunk_size to ensure guard chunks have proper SDF zero-crossings.
    /// This extends the box surface INTO guard chunk regions so they generate real geometry.
    box_extension: f32,
}

impl NoiseField {
    pub fn new(
        seed: u32,
        octaves: usize,
        frequency: f32,
        amplitude: f32,
        height_offset: f32,
        floor_y: f32,
        box_bounds: Option<([f32; 3], [f32; 3])>,
    ) -> Self {
        Self::with_csg_blend(
            seed,
            octaves,
            frequency,
            amplitude,
            height_offset,
            floor_y,
            box_bounds,
            2.0, // Default blend width for smooth normals
        )
    }

    pub fn with_csg_blend(
        seed: u32,
        octaves: usize,
        frequency: f32,
        amplitude: f32,
        height_offset: f32,
        floor_y: f32,
        box_bounds: Option<([f32; 3], [f32; 3])>,
        csg_blend_width: f32,
    ) -> Self {
        Self::with_box_extension(
            seed,
            octaves,
            frequency,
            amplitude,
            height_offset,
            floor_y,
            box_bounds,
            csg_blend_width,
            0.0, // Default no extension
        )
    }

    /// Create noise field with extended box bounds for SDF calculation.
    /// The box_extension parameter extends the box surface into guard chunk regions,
    /// ensuring corner chunks generate proper geometry instead of degenerate triangles.
    pub fn with_box_extension(
        seed: u32,
        octaves: usize,
        frequency: f32,
        amplitude: f32,
        height_offset: f32,
        floor_y: f32,
        box_bounds: Option<([f32; 3], [f32; 3])>,
        csg_blend_width: f32,
        box_extension: f32,
    ) -> Self {
        let fbm = Fbm::<Perlin>::new(seed)
            .set_octaves(octaves)
            .set_frequency(frequency as f64)
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        let (box_min, box_max) = match box_bounds {
            Some((min, max)) => (Some(min), Some(max)),
            None => (None, None),
        };

        Self {
            fbm,
            amplitude,
            height_offset,
            floor_y,
            box_min,
            box_max,
            csg_blend_width,
            box_extension,
        }
    }

    pub fn get_box_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        match (&self.box_min, &self.box_max) {
            (Some(min), Some(max)) => Some((*min, *max)),
            _ => None,
        }
    }

    /// Check if a chunk coordinate represents a corner guard chunk
    /// (touches 3 boundaries: floor + 2 walls)
    pub fn is_corner_guard_chunk(&self, chunk_coord: [i32; 3], chunk_size: f32) -> bool {
        if let (Some(min), Some(max)) = (&self.box_min, &self.box_max) {
            let chunk_min = [
                chunk_coord[0] as f32 * chunk_size,
                chunk_coord[1] as f32 * chunk_size,
                chunk_coord[2] as f32 * chunk_size,
            ];
            let chunk_max = [
                (chunk_coord[0] + 1) as f32 * chunk_size,
                (chunk_coord[1] + 1) as f32 * chunk_size,
                (chunk_coord[2] + 1) as f32 * chunk_size,
            ];

            // Count how many boundaries this chunk touches
            let mut boundary_count = 0;

            // Check X boundaries
            if chunk_max[0] <= min[0] + 0.001 || chunk_min[0] >= max[0] - 0.001 {
                boundary_count += 1;
            }
            // Check Y boundaries
            if chunk_max[1] <= min[1] + 0.001 || chunk_min[1] >= max[1] - 0.001 {
                boundary_count += 1;
            }
            // Check Z boundaries
            if chunk_max[2] <= min[2] + 0.001 || chunk_min[2] >= max[2] - 0.001 {
                boundary_count += 1;
            }

            // Corner chunks touch 3 boundaries
            boundary_count >= 3
        } else {
            false
        }
    }

    /// Check if a chunk coordinate is a boundary/guard chunk
    /// (outside the main terrain volume, used for wall/floor generation)
    pub fn is_boundary_chunk(&self, chunk_coord: [i32; 3], chunk_size: f32) -> bool {
        if let (Some(min), Some(max)) = (&self.box_min, &self.box_max) {
            let chunk_min = [
                chunk_coord[0] as f32 * chunk_size,
                chunk_coord[1] as f32 * chunk_size,
                chunk_coord[2] as f32 * chunk_size,
            ];
            let chunk_max = [
                (chunk_coord[0] + 1) as f32 * chunk_size,
                (chunk_coord[1] + 1) as f32 * chunk_size,
                (chunk_coord[2] + 1) as f32 * chunk_size,
            ];

            // Chunk is a boundary chunk if any part of it is outside the box
            chunk_min[0] < min[0]
                || chunk_max[0] > max[0]
                || chunk_min[1] < min[1]
                || chunk_max[1] > max[1]
                || chunk_min[2] < min[2]
                || chunk_max[2] > max[2]
        } else {
            false
        }
    }

    /// Set box bounds for SDFK clipping (walls via SDF)
    pub fn set_box_bounds(&mut self, min: [f32; 3], max: [f32; 3]) {
        self.box_min = Some(min);
        self.box_max = Some(max);
    }
    /// 2D noise for heightmap terrain (no Y dependency in noise)
    pub fn sample_terrain_only(&self, x: f32, y: f32, z: f32) -> f32 {
        // 2D noise: only x and z, not y
        let noise_value = self.fbm.get([x as f64, z as f64]) as f32;
        let surface_height = self.floor_y + self.height_offset + noise_value * self.amplitude;
        y - surface_height
    }
    /// Sample the SDF at world position
    /// Uses CSG intersection to create watertight mesh when box bounds are set
    /// When csg_blend_width > 0, uses smooth_max for continuous gradients (better normals)
    pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        let terrain_sdf = self.sample_terrain_only(x, y, z);

        if let (Some(min), Some(max)) = (&self.box_min, &self.box_max) {
            // CSG intersection: solid where BOTH inside box AND below terrain
            // terrain_sdf: negative below surface (solid), positive above (air)
            // box_dist: negative inside box (solid), positive outside (air)
            // max(a, b) = intersection: solid only where BOTH are solid (negative)
            //
            // Extend box bounds to include guard chunk regions.
            // This brings the box surface INTO the corner chunks, creating proper
            // SDF zero-crossings so they generate real geometry instead of degenerate triangles.
            let ext = self.box_extension;
            let extended_min = [min[0] - ext, min[1] - ext, min[2] - ext];
            let extended_max = [max[0] + ext, max[1] + ext, max[2] + ext];

            let box_dist = Self::box_sdf([x, y, z], extended_min, extended_max);
            let result = Self::smooth_max(terrain_sdf, box_dist, self.csg_blend_width);

            // Debug logging for boundary positions (limited to avoid spam)
            if cfg!(debug_assertions) {
                let tolerance = 2.0;
                let is_boundary =
                    crate::debug_log::is_boundary_position([x, y, z], *min, *max, tolerance);
                if is_boundary {
                    let count = BOUNDARY_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
                    if count < MAX_BOUNDARY_LOGS {
                        crate::debug_log::debug_log(&format!(
                            "[noise_field] boundary sample at ({:.1}, {:.1}, {:.1}): terrain={:.3}, box={:.3}, final={:.3}",
                            x, y, z, terrain_sdf, box_dist, result
                        ));
                    }
                }
            }

            return result;
        }

        terrain_sdf
    }

    /// Smooth maximum for CSG intersection with continuous gradient
    /// When k <= 0, falls back to hard max(a, b)
    /// When k > 0, blends over a region of width k for C1 continuous gradients
    /// This eliminates normal artifacts at CSG intersection seams
    #[inline]
    fn smooth_max(a: f32, b: f32, k: f32) -> f32 {
        if k <= 0.0 {
            return a.max(b);
        }
        // Polynomial smooth max (faster than exp-based, still C1 continuous)
        let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
        // Blend values with correction term for exact result at edges
        a * (1.0 - h) + b * h + k * h * (1.0 - h)
    }

    pub fn get_amplitude(&self) -> f32 {
        self.amplitude
    }

    pub fn get_floor_y(&self) -> f32 {
        self.floor_y
    }

    pub fn get_csg_blend_width(&self) -> f32 {
        self.csg_blend_width
    }

    pub fn set_csg_blend_width(&mut self, width: f32) {
        self.csg_blend_width = width;
    }

    /// Signed distance function for an axis-aligned box
    /// Returns: negative inside, positive outside, zero on surface
    pub(crate) fn box_sdf(p: [f32; 3], min: [f32; 3], max: [f32; 3]) -> f32 {
        let center = [
            (min[0] + max[0]) / 2.0,
            (min[1] + max[1]) / 2.0,
            (min[2] + max[2]) / 2.0,
        ];
        let half = [
            (max[0] - min[0]) / 2.0,
            (max[1] - min[1]) / 2.0,
            (max[2] - min[2]) / 2.0,
        ];

        // Distance from point to box surface
        let q = [
            (p[0] - center[0]).abs() - half[0],
            (p[1] - center[1]).abs() - half[1],
            (p[2] - center[2]).abs() - half[2],
        ];

        // Outside distance (when any component is positive)
        let outside =
            (q[0].max(0.0).powi(2) + q[1].max(0.0).powi(2) + q[2].max(0.0).powi(2)).sqrt();
        // Inside distance (when all components are negative)
        let inside = q[0].max(q[1]).max(q[2]).min(0.0);

        outside + inside
    }
}

/// Thread-safe shared noise field for parallel mesh generation
pub type SharedNoiseField = Arc<NoiseField>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_sdf_on_surfaces() {
        let min = [0.0, 0.0, 0.0];
        let max = [100.0, 100.0, 100.0];

        // On floor (Y=0, inside XZ)
        assert!(
            (NoiseField::box_sdf([50.0, 0.0, 50.0], min, max)).abs() < 0.001,
            "Floor surface at Y=0 should have SDF ≈ 0"
        );

        // On walls
        assert!(
            (NoiseField::box_sdf([0.0, 50.0, 50.0], min, max)).abs() < 0.001,
            "X=0 wall should have SDF ≈ 0"
        );
        assert!(
            (NoiseField::box_sdf([100.0, 50.0, 50.0], min, max)).abs() < 0.001,
            "X=max wall should have SDF ≈ 0"
        );
        assert!(
            (NoiseField::box_sdf([50.0, 50.0, 0.0], min, max)).abs() < 0.001,
            "Z=0 wall should have SDF ≈ 0"
        );
        assert!(
            (NoiseField::box_sdf([50.0, 50.0, 100.0], min, max)).abs() < 0.001,
            "Z=max wall should have SDF ≈ 0"
        );

        // Inside box (negative)
        assert!(
            NoiseField::box_sdf([50.0, 50.0, 50.0], min, max) < 0.0,
            "Inside box should be negative (solid)"
        );

        // Outside box (positive)
        assert!(
            NoiseField::box_sdf([50.0, -10.0, 50.0], min, max) > 0.0,
            "Below box should be positive (air)"
        );
        assert!(
            NoiseField::box_sdf([-10.0, 50.0, 50.0], min, max) > 0.0,
            "Outside X=0 should be positive (air)"
        );
    }

    #[test]
    fn test_sdf_enclosure_floor_at_y0() {
        // Create noise with floor_y=50, so terrain surface is around Y=50
        // Box from Y=0 to Y=100 means floor should be at Y=0
        let noise = NoiseField::new(
            42,
            4,
            0.02,
            10.0, // amplitude
            0.0,  // height_offset
            50.0, // floor_y - terrain surface around Y=50
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
        );

        // At Y=0 inside XZ bounds: should be surface (SDF ≈ 0)
        // The box_sdf at Y=0 is 0, and terrain_sdf is negative (below terrain)
        // max(negative, 0) = 0, so floor surface is at Y=0
        let sdf_at_floor = noise.sample(50.0, 0.0, 50.0);
        assert!(
            sdf_at_floor.abs() < 0.1,
            "Floor should be at Y=0, got SDF={}",
            sdf_at_floor
        );

        // Just above Y=0: should be inside (SDF < 0)
        // box_sdf < 0 (inside box), terrain_sdf < 0 (below terrain surface)
        // max(negative, negative) = negative
        let sdf_above_floor = noise.sample(50.0, 1.0, 50.0);
        assert!(
            sdf_above_floor < 0.0,
            "Should be solid above floor, got SDF={}",
            sdf_above_floor
        );

        // Below Y=0: should be outside (SDF > 0)
        // box_sdf > 0 (outside box), terrain_sdf < 0 (below terrain surface)
        // max(negative, positive) = positive
        let sdf_below_floor = noise.sample(50.0, -1.0, 50.0);
        assert!(
            sdf_below_floor > 0.0,
            "Should be air below floor, got SDF={}",
            sdf_below_floor
        );
    }

    #[test]
    fn test_sdf_enclosure_walls() {
        // Create noise with floor_y=50, terrain around Y=50
        // Walls should be at X=0, X=100, Z=0, Z=100
        let noise = NoiseField::new(
            42,
            4,
            0.02,
            10.0,
            0.0,
            50.0,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
        );

        // Test at Y=25 (below terrain surface, inside box vertically)
        // At these positions, terrain_sdf is negative (solid)
        // box_sdf is 0 on the wall surfaces

        // At X=0 wall (inside Y and Z bounds, below terrain)
        let sdf_x0_wall = noise.sample(0.0, 25.0, 50.0);
        assert!(
            sdf_x0_wall.abs() < 0.1,
            "X=0 wall should be surface, got SDF={}",
            sdf_x0_wall
        );

        // At X=100 wall
        let sdf_xmax_wall = noise.sample(100.0, 25.0, 50.0);
        assert!(
            sdf_xmax_wall.abs() < 0.1,
            "X=max wall should be surface, got SDF={}",
            sdf_xmax_wall
        );

        // At Z=0 wall
        let sdf_z0_wall = noise.sample(50.0, 25.0, 0.0);
        assert!(
            sdf_z0_wall.abs() < 0.1,
            "Z=0 wall should be surface, got SDF={}",
            sdf_z0_wall
        );

        // At Z=100 wall
        let sdf_zmax_wall = noise.sample(50.0, 25.0, 100.0);
        assert!(
            sdf_zmax_wall.abs() < 0.1,
            "Z=max wall should be surface, got SDF={}",
            sdf_zmax_wall
        );

        // Test that just inside walls is solid
        let sdf_inside_x0 = noise.sample(1.0, 25.0, 50.0);
        assert!(
            sdf_inside_x0 < 0.0,
            "Inside X=0 wall should be solid, got SDF={}",
            sdf_inside_x0
        );

        // Test that just outside walls is air
        let sdf_outside_x0 = noise.sample(-1.0, 25.0, 50.0);
        assert!(
            sdf_outside_x0 > 0.0,
            "Outside X=0 wall should be air, got SDF={}",
            sdf_outside_x0
        );
    }

    #[test]
    fn test_mesh_has_floor_triangles() {
        use crate::chunk::ChunkCoord;
        use crate::mesh_extraction::extract_chunk_mesh;

        // Box bounds: floor at Y=0, walls at X/Z=0 and X/Z=32
        // Terrain surface around Y=50
        let noise = NoiseField::new(
            42,
            4,
            0.02,
            10.0,
            0.0,
            50.0, // terrain surface around Y=50
            Some(([0.0, 0.0, 0.0], [32.0, 64.0, 32.0])),
        );

        // KEY INSIGHT: Transvoxel needs voxels on BOTH sides of a surface to
        // generate geometry. The floor at Y=0 requires sampling Y < 0 to see
        // the sign change from + (air below floor) to - (solid above floor).
        //
        // Chunk (0,0,0) covers Y=0..32, only samples Y >= 0
        // Chunk (0,-1,0) covers Y=-32..0, samples both sides of the floor

        // Test 1: Terrain surface generates mesh in chunk (0,1,0)
        // This chunk covers Y=32..64, containing the terrain surface at ~Y=50
        let terrain_result = extract_chunk_mesh(&noise, ChunkCoord::new(0, 1, 0), 0, 1.0, 32.0, 0);
        assert!(
            !terrain_result.vertices.is_empty(),
            "Terrain surface chunk should have vertices"
        );

        // Test 2: Floor generates mesh in chunk (0,-1,0)
        // This chunk covers Y=-32..0, spanning across the floor at Y=0
        let floor_result = extract_chunk_mesh(&noise, ChunkCoord::new(0, -1, 0), 0, 1.0, 32.0, 0);
        assert!(
            !floor_result.vertices.is_empty(),
            "Floor chunk should have vertices when spanning Y=0"
        );

        // Verify floor vertices are near Y=0
        let max_y = floor_result
            .vertices
            .iter()
            .map(|v| v[1])
            .fold(f32::MIN, f32::max);
        assert!(
            max_y.abs() < 1.0,
            "Floor vertices should be near Y=0, max_y={}",
            max_y
        );

        // Test 3: Chunk (0,0,0) alone won't have floor (no Y < 0 samples)
        let no_floor_result = extract_chunk_mesh(&noise, ChunkCoord::new(0, 0, 0), 0, 1.0, 32.0, 0);
        // This chunk has no surface crossings (terrain is above, floor is at boundary)
        // It might be empty or have only wall geometry at X=0, Z=0
        let has_floor_in_chunk0 = no_floor_result.vertices.iter().any(|v| v[1].abs() < 1.0);
        assert!(
            !has_floor_in_chunk0,
            "Chunk (0,0,0) should not have floor vertices at Y=0"
        );
    }

    #[test]
    fn test_mesh_has_wall_triangles() {
        use crate::chunk::ChunkCoord;
        use crate::mesh_extraction::extract_chunk_mesh;

        // Box bounds: walls at X=0, X=32, Z=0, Z=32
        // Terrain surface around Y=50 (below terrain, so solid)
        let noise = NoiseField::new(
            42,
            4,
            0.02,
            10.0,
            0.0,
            50.0, // terrain surface around Y=50
            Some(([0.0, 0.0, 0.0], [32.0, 64.0, 32.0])),
        );

        // X=-1 chunk should capture X=0 wall
        // This chunk covers X=-32..0, spanning across the wall at X=0
        // At Y=1 we're in the solid region (below terrain, inside box)
        let wall_result = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(-1, 1, 0), // Y=1 to be in solid region
            0,
            1.0,
            32.0,
            0,
        );

        assert!(
            !wall_result.vertices.is_empty(),
            "X=-1 chunk should have wall vertices"
        );

        // Vertices should be near X=0
        let max_x = wall_result
            .vertices
            .iter()
            .map(|v| v[0])
            .fold(f32::MIN, f32::max);
        assert!(
            max_x.abs() < 1.0,
            "Wall vertices should be near X=0, max_x={}",
            max_x
        );
    }

    #[test]
    fn test_default_terrain_settings_generate_enclosure() {
        use crate::chunk::ChunkCoord;
        use crate::mesh_extraction::extract_chunk_mesh;

        // Match actual default terrain settings from terrain.rs
        let chunk_size = 32.0; // voxel_size(1.0) * chunk_subdivisions(32)
        let map_width = 10;
        let map_height = 4;
        let map_depth = 10;

        let box_min = [0.0, 0.0, 0.0];
        let box_max = [
            map_width as f32 * chunk_size,  // 320
            map_height as f32 * chunk_size, // 128
            map_depth as f32 * chunk_size,  // 320
        ];

        let noise = NoiseField::new(
            42,   // seed
            4,    // octaves
            0.02, // frequency
            32.0, // amplitude (default)
            0.0,  // height_offset (default)
            32.0, // terrain_floor_y (default)
            Some((box_min, box_max)),
        );

        // Debug: Check SDF values at key locations
        // At Y=16 (below terrain surface ~32), inside box, on X=0 wall
        let sdf_on_wall = noise.sample(0.0, 16.0, 160.0);
        assert!(
            sdf_on_wall.abs() < 1.0,
            "X=0 wall at Y=16 should be surface, got SDF={}",
            sdf_on_wall
        );

        // Just inside the wall (X=1)
        let sdf_inside_wall = noise.sample(1.0, 16.0, 160.0);
        assert!(
            sdf_inside_wall < 0.0,
            "Inside X=0 wall should be solid, got SDF={}",
            sdf_inside_wall
        );

        // Just outside the wall (X=-1)
        let sdf_outside_wall = noise.sample(-1.0, 16.0, 160.0);
        assert!(
            sdf_outside_wall > 0.0,
            "Outside X=0 wall should be air, got SDF={}",
            sdf_outside_wall
        );

        // Test floor at Y=0
        let sdf_on_floor = noise.sample(160.0, 0.0, 160.0);
        assert!(
            sdf_on_floor.abs() < 1.0,
            "Floor at Y=0 should be surface, got SDF={}",
            sdf_on_floor
        );

        // Now test actual mesh generation for X=-1 chunk
        // This chunk covers X=-32 to X=0, Y=0 to Y=32
        let wall_chunk = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(-1, 0, 5), // Y=0 chunk, Z=5 (middle of terrain)
            0,
            1.0,
            chunk_size,
            0,
        );

        assert!(
            !wall_chunk.vertices.is_empty(),
            "X=-1 guard chunk should generate wall geometry. \
             SDF at wall: {}, inside: {}, outside: {}",
            sdf_on_wall,
            sdf_inside_wall,
            sdf_outside_wall
        );

        // Test floor chunk Y=-1
        let floor_chunk = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(5, -1, 5), // X=5, Z=5 (middle of terrain)
            0,
            1.0,
            chunk_size,
            0,
        );

        assert!(
            !floor_chunk.vertices.is_empty(),
            "Y=-1 guard chunk should generate floor geometry"
        );
    }

    #[test]
    fn test_wall_chunk_at_various_y_levels() {
        use crate::chunk::ChunkCoord;
        use crate::mesh_extraction::extract_chunk_mesh;

        // Default terrain settings
        let chunk_size = 32.0;
        let box_max = [320.0, 128.0, 320.0];

        let noise = NoiseField::new(
            42,
            4,
            0.02,
            32.0,
            0.0,
            32.0,
            Some(([0.0, 0.0, 0.0], box_max)),
        );

        // Test wall chunks at different Y levels
        // terrain_floor_y=32, amplitude=32 means terrain surface ~32±32 = 0 to 64
        // Walls should exist where terrain is solid (below terrain surface)

        println!("\n=== Wall chunk analysis ===");
        println!("Terrain surface around Y=32 ± noise*32");
        println!("Box bounds: [0,0,0] to {:?}", box_max);

        for y in -1..=3i32 {
            let chunk = extract_chunk_mesh(
                &noise,
                ChunkCoord::new(-1, y, 5), // X=-1 guard chunk, Z=5 (middle)
                0,
                1.0,
                chunk_size,
                0,
            );

            let world_y_min = y as f32 * chunk_size;
            let world_y_max = (y + 1) as f32 * chunk_size;

            println!(
                "Chunk (-1, {}, 5) [Y={:.0}..{:.0}]: {} vertices, {} triangles",
                y,
                world_y_min,
                world_y_max,
                chunk.vertices.len(),
                chunk.indices.len() / 3
            );

            // Check SDF at chunk center on the wall
            let sample_y = (world_y_min + world_y_max) / 2.0;
            let sdf_on_wall = noise.sample(0.0, sample_y, 160.0);
            let sdf_inside = noise.sample(1.0, sample_y, 160.0);
            let sdf_outside = noise.sample(-1.0, sample_y, 160.0);

            println!(
                "  SDF at Y={:.0}: wall={:.2}, inside={:.2}, outside={:.2}",
                sample_y, sdf_on_wall, sdf_inside, sdf_outside
            );
        }

        // At least Y=0 chunk should have wall geometry (Y=0..32, below terrain surface)
        let y0_chunk = extract_chunk_mesh(&noise, ChunkCoord::new(-1, 0, 5), 0, 1.0, chunk_size, 0);
        assert!(
            !y0_chunk.vertices.is_empty(),
            "X=-1,Y=0 chunk should have wall vertices (terrain solid at Y<32)"
        );
    }

    #[test]
    fn test_corner_chunks_have_geometry() {
        // Verify that corner guard chunks produce mesh geometry
        // These are chunks that touch 3 boundaries (floor + 2 walls)
        use crate::chunk::ChunkCoord;
        use crate::mesh_extraction::extract_chunk_mesh;

        let chunk_size = 32.0;
        // Use with_box_extension to extend box bounds into corner chunks
        // Extension = chunk_size/2 puts the surface in the MIDDLE of guard chunks,
        // ensuring samples on both sides of the surface for proper zero-crossings.
        // (chunk_size would put surface at chunk corner = no zero-crossing)
        let noise = NoiseField::with_box_extension(
            42,
            4,
            0.02,
            32.0,
            0.0,
            32.0, // floor_y = 32, so terrain is solid below Y~32
            Some(([0.0, 0.0, 0.0], [320.0, 128.0, 320.0])),
            2.0,              // csg_blend_width
            chunk_size / 2.0, // box_extension = half chunk for proper zero-crossings
        );

        // Test all 4 floor-level corner chunks
        let corners = [
            ChunkCoord::new(-1, -1, -1), // Corner: X=0, Y=0, Z=0
            ChunkCoord::new(-1, -1, 10), // Corner: X=0, Y=0, Z=max
            ChunkCoord::new(10, -1, -1), // Corner: X=max, Y=0, Z=0
            ChunkCoord::new(10, -1, 10), // Corner: X=max, Y=0, Z=max
        ];

        println!("\n=== Corner chunk geometry test ===");
        for corner in &corners {
            let mesh = extract_chunk_mesh(&noise, *corner, 0, 1.0, chunk_size, 0);
            let vert_count = mesh.vertices.len();
            let tri_count = mesh.indices.len() / 3;

            println!(
                "Corner ({}, {}, {}): {} verts, {} tris",
                corner.x, corner.y, corner.z, vert_count, tri_count
            );

            // Corner chunks should produce meaningful geometry with extended box bounds.
            // A single degenerate triangle (3 verts, 1 tri) indicates the fix isn't working.
            // We require at least 3 triangles (9 indices) for watertight corner geometry.
            assert!(
                mesh.indices.len() >= 9,
                "Corner chunk {:?} produced {} triangles (need >= 3 for watertight). verts={}, tris={}",
                corner, tri_count, vert_count, tri_count
            );
        }
    }

    #[test]
    fn test_boundary_chunks_minimum_geometry() {
        // Verify that edge boundary chunks (not corners) produce sufficient geometry
        use crate::chunk::ChunkCoord;
        use crate::mesh_extraction::extract_chunk_mesh;

        let chunk_size = 32.0;
        let noise = NoiseField::new(
            42,
            4,
            0.02,
            32.0,
            0.0,
            32.0,
            Some(([0.0, 0.0, 0.0], [320.0, 128.0, 320.0])),
        );

        // Test edge chunks (touch 2 boundaries: floor + 1 wall)
        let edge_chunks = [
            ChunkCoord::new(-1, -1, 5), // X=-1 edge, floor
            ChunkCoord::new(10, -1, 5), // X=max edge, floor
            ChunkCoord::new(5, -1, -1), // Z=-1 edge, floor
            ChunkCoord::new(5, -1, 10), // Z=max edge, floor
        ];

        println!("\n=== Edge boundary chunk geometry test ===");
        for chunk in &edge_chunks {
            let mesh = extract_chunk_mesh(&noise, *chunk, 0, 1.0, chunk_size, 0);
            let vert_count = mesh.vertices.len();
            let tri_count = mesh.indices.len() / 3;

            println!(
                "Edge ({}, {}, {}): {} verts, {} tris",
                chunk.x, chunk.y, chunk.z, vert_count, tri_count
            );

            // Edge chunks should have more geometry than corners since they
            // span more of the boundary surface
            assert!(
                vert_count >= 6,
                "Edge chunk {:?} should have at least 6 vertices, got {}",
                chunk,
                vert_count
            );
        }
    }

    #[test]
    fn test_is_corner_guard_chunk() {
        let noise = NoiseField::new(
            42,
            4,
            0.02,
            32.0,
            0.0,
            32.0,
            Some(([0.0, 0.0, 0.0], [320.0, 128.0, 320.0])),
        );
        let chunk_size = 32.0;

        // These should be identified as corner chunks
        assert!(noise.is_corner_guard_chunk([-1, -1, -1], chunk_size));
        assert!(noise.is_corner_guard_chunk([-1, -1, 10], chunk_size));
        assert!(noise.is_corner_guard_chunk([10, -1, -1], chunk_size));
        assert!(noise.is_corner_guard_chunk([10, -1, 10], chunk_size));

        // These are edge chunks, not corners
        assert!(!noise.is_corner_guard_chunk([-1, -1, 5], chunk_size));
        assert!(!noise.is_corner_guard_chunk([5, -1, 5], chunk_size));

        // Interior chunks
        assert!(!noise.is_corner_guard_chunk([5, 0, 5], chunk_size));
    }

    #[test]
    fn test_is_boundary_chunk() {
        let noise = NoiseField::new(
            42,
            4,
            0.02,
            32.0,
            0.0,
            32.0,
            Some(([0.0, 0.0, 0.0], [320.0, 128.0, 320.0])),
        );
        let chunk_size = 32.0;

        // Guard chunks are boundary chunks
        assert!(noise.is_boundary_chunk([-1, 0, 5], chunk_size));
        assert!(noise.is_boundary_chunk([5, -1, 5], chunk_size));
        assert!(noise.is_boundary_chunk([5, 0, -1], chunk_size));

        // Interior chunks are not boundary chunks
        assert!(!noise.is_boundary_chunk([5, 1, 5], chunk_size));
        assert!(!noise.is_boundary_chunk([0, 0, 0], chunk_size));
    }

    #[test]
    fn test_box_extension_creates_zero_crossings() {
        // Verify that box_extension = chunk_size/2 creates proper SDF zero-crossings
        // in corner chunks, enabling transvoxel to generate geometry.
        let chunk_size = 32.0;

        // With extension=16 (half chunk), the surface at (-16,-16,-16) passes
        // through the center of corner chunk (-1,-1,-1), creating samples on
        // both sides of the surface.
        let noise = NoiseField::with_box_extension(
            42,
            4,
            0.02,
            32.0,
            0.0,
            32.0,
            Some(([0.0, 0.0, 0.0], [320.0, 128.0, 320.0])),
            2.0,              // csg_blend_width
            chunk_size / 2.0, // extension = 16 (half chunk)
        );

        // Sample at opposite ends of the corner chunk
        let at_corner = noise.sample(-32.0, -32.0, -32.0); // Outside extended box
        let at_origin = noise.sample(0.0, 0.0, 0.0); // Inside extended box

        // These should have opposite signs for a zero-crossing to exist
        assert!(
            at_corner > 0.0,
            "Corner (-32,-32,-32) should be outside (positive SDF), got {}",
            at_corner
        );
        assert!(
            at_origin < 0.0,
            "Origin (0,0,0) should be inside (negative SDF), got {}",
            at_origin
        );

        // The surface (SDF ≈ 0) should be around (-16,-16,-16)
        let at_center = noise.sample(-16.0, -16.0, -16.0);
        assert!(
            at_center.abs() < 1.0,
            "Center (-16,-16,-16) should be near surface (SDF ≈ 0), got {}",
            at_center
        );
    }
}
