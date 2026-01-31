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
        }
    }

    /// Sample the SDF at world position
    pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        let noise_value = self.fbm.get([x as f64, y as f64, z as f64]) as f32;
        y - self.height_offset - noise_value * self.amplitude
    }

    pub fn get_amplitude(&self) -> f32 {
        self.amplitude
    }
}

/// Thread-safe shared noise field for parallel mesh generation
pub type SharedNoiseField = Arc<NoiseField>;
