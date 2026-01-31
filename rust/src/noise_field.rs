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
    // Box bounds for SDF clipping
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
        }
    }
    /// Set box bounds for SDFK clipping (walls via SDF)
    pub fn set_box_bounds(&mut self, min: [f32; 3], max: [f32; 3]) {
        self.box_min = Some(min);
        self.box_max = Some(max);
    }

    /// Sample the SDF at world position
    pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        // Return "air" outside XZ bounds; creates walls via SDF
        if let (Some(min), Some(max)) = (&self.box_min, &self.box_max) {
            if x < min[0] || x > max[0] || z < min[2] || z > max[2] {
                return 1.0; // Outside = air
            }
        }

        let noise_value = self.fbm.get([x as f64, y as f64, z as f64]) as f32;
        y - self.height_offset - noise_value * self.amplitude
    }

    pub fn get_amplitude(&self) -> f32 {
        self.amplitude
    }
}

/// Thread-safe shared noise field for parallel mesh generation
pub type SharedNoiseField = Arc<NoiseField>;
