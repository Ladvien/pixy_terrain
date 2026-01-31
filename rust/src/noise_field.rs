use noise::{Fbm, MultiFractal, NoiseFn, Perlin, Seedable};
use std::sync::Arc;

/// Noise-based sign distance field for terrain generation
/// Negative = inside terrain (solid)
/// Positive = outside terrain (air)
/// Zero = surface
pub struct NoiseField {
    fbm: Fbm<Perlin>,
    amplitude: f32,
    height_offset: f32,
    floor_y: f32,
    boundary_offset: f32,
    box_min: Option<[f32; 3]>,
    box_max: Option<[f32; 3]>,
}

impl NoiseField {
    pub fn new(
        seed: u32,
        octaves: usize,
        frequency: f32,
        amplitude: f32,
        height_offset: f32,
        floor_y: f32,
        boundary_offset: f32,
    ) -> Self {
        let fbm = Fbm::<Perlin>::new(seed)
            .set_octaves(octaves)
            .set_frequency(frequency as f64)
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        Self {
            fbm,
            amplitude,
            height_offset,
            box_min: None,
            box_max: None,
            floor_y,
            boundary_offset,
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
    pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        // Return "air" outside XZ bounds
        if let (Some(min), Some(max)) = (&self.box_min, &self.box_max) {
            if x < min[0] || x > max[0] || z < min[2] || z > max[2] {
                return 1000.0; // Large positive = air
            }
        }
        self.sample_terrain_only(x, y, z)
    }

    pub fn get_amplitude(&self) -> f32 {
        self.amplitude
    }

    pub fn get_floor_y(&self) -> f32 {
        self.floor_y
    }

    pub fn get_boundary_offset(&self) -> f32 {
        self.boundary_offset
    }
}

/// Thread-safe shared noise field for parallel mesh generation
pub type SharedNoiseField = Arc<NoiseField>;
