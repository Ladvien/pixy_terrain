use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use std::sync::Arc;

use crate::terrain_modifications::ModificationLayer;

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
        }
    }

    pub fn get_box_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        match (&self.box_min, &self.box_max) {
            (Some(min), Some(max)) => Some((*min, *max)),
            _ => None,
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
            let box_dist = Self::box_sdf([x, y, z], *min, *max);
            return Self::smooth_max(terrain_sdf, box_dist, self.csg_blend_width);
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

    /// Sample the SDF with terrain modifications applied.
    /// This is used during mesh generation to incorporate brush edits.
    ///
    /// The modification layer is sampled using trilinear interpolation and
    /// the result is added to the base SDF value.
    pub fn sample_with_mods(&self, x: f32, y: f32, z: f32, mods: &ModificationLayer) -> f32 {
        let base_sdf = self.sample(x, y, z);
        let mod_delta = mods.sample(x, y, z);
        base_sdf + mod_delta
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
        let outside = (q[0].max(0.0).powi(2) + q[1].max(0.0).powi(2) + q[2].max(0.0).powi(2)).sqrt();
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
        let terrain_result = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(0, 1, 0),
            0,
            1.0,
            32.0,
            0,
            None,
            None,
        );
        assert!(
            !terrain_result.vertices.is_empty(),
            "Terrain surface chunk should have vertices"
        );

        // Test 2: Floor generates mesh in chunk (0,-1,0)
        // This chunk covers Y=-32..0, spanning across the floor at Y=0
        let floor_result = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(0, -1, 0),
            0,
            1.0,
            32.0,
            0,
            None,
            None,
        );
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
        let no_floor_result = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(0, 0, 0),
            0,
            1.0,
            32.0,
            0,
            None,
            None,
        );
        // This chunk has no surface crossings (terrain is above, floor is at boundary)
        // It might be empty or have only wall geometry at X=0, Z=0
        let has_floor_in_chunk0 = no_floor_result
            .vertices
            .iter()
            .any(|v| v[1].abs() < 1.0);
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
            None,
            None,
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
            map_width as f32 * chunk_size,   // 320
            map_height as f32 * chunk_size,  // 128
            map_depth as f32 * chunk_size,   // 320
        ];

        let noise = NoiseField::new(
            42,                    // seed
            4,                     // octaves
            0.02,                  // frequency
            32.0,                  // amplitude (default)
            0.0,                   // height_offset (default)
            32.0,                  // terrain_floor_y (default)
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
            None,
            None,
        );

        assert!(
            !wall_chunk.vertices.is_empty(),
            "X=-1 guard chunk should generate wall geometry. \
             SDF at wall: {}, inside: {}, outside: {}",
            sdf_on_wall, sdf_inside_wall, sdf_outside_wall
        );

        // Test floor chunk Y=-1
        let floor_chunk = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(5, -1, 5), // X=5, Z=5 (middle of terrain)
            0,
            1.0,
            chunk_size,
            0,
            None,
            None,
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
            42, 4, 0.02, 32.0, 0.0, 32.0,
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
                0, 1.0, chunk_size, 0,
                None, None,
            );

            let world_y_min = y as f32 * chunk_size;
            let world_y_max = (y + 1) as f32 * chunk_size;

            println!(
                "Chunk (-1, {}, 5) [Y={:.0}..{:.0}]: {} vertices, {} triangles",
                y, world_y_min, world_y_max,
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
        let y0_chunk = extract_chunk_mesh(
            &noise,
            ChunkCoord::new(-1, 0, 5),
            0, 1.0, chunk_size, 0,
            None, None,
        );
        assert!(
            !y0_chunk.vertices.is_empty(),
            "X=-1,Y=0 chunk should have wall vertices (terrain solid at Y<32)"
        );
    }
}
