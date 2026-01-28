# Walkthrough 04: Noise Field and SDF

**Series:** Transvoxel Noise Terrain
**Status:** Planning
**Prerequisites:** Walkthrough 03 (LOD configuration)

## Goal

Create a noise-based Signed Distance Field (SDF) using the `noise` crate's Fbm (fractal Brownian motion) generator. The SDF produces terrain suitable for transvoxel extraction.

## Acceptance Criteria

- [ ] `NoiseField` struct with configurable seed, octaves, frequency, amplitude
- [ ] `sample(x, y, z)` returns SDF value (negative inside, positive outside)
- [ ] `SharedNoiseField` type alias for thread-safe sharing
- [ ] Unit tests verify SDF signs at expected positions

## Understanding SDF for Terrain

A Signed Distance Field returns:
- **Negative values** = inside solid terrain
- **Positive values** = outside (air)
- **Zero** = exactly on the surface

For terrain, we use the formula:
```
SDF = y - height_offset - noise(x, y, z) * amplitude
```

This creates terrain where:
- Below the noise surface → negative (solid)
- Above the noise surface → positive (air)
- At the surface → zero (where mesh generates)

## Steps

### Step 1: Create noise_field.rs Module

**File:** `rust/src/noise_field.rs` (new file)

```rust
// Full path: rust/src/noise_field.rs

//! Noise-based Signed Distance Field for terrain generation.
//!
//! The NoiseField produces SDF values where:
//! - Negative = inside solid terrain
//! - Positive = outside (air)
//! - Zero = surface (where mesh is generated)

use noise::{Fbm, MultiFractal, NoiseFn, Perlin, Seedable};
use std::sync::Arc;

/// Noise-based signed distance field for terrain generation
pub struct NoiseField {
    /// Fractal Brownian motion noise generator
    fbm: Fbm<Perlin>,
    /// Height variation multiplier
    amplitude: f32,
    /// Base terrain height (y = 0 is at this height when noise = 0)
    height_offset: f32,
}

impl NoiseField {
    /// Create a new noise field
    ///
    /// # Arguments
    /// * `seed` - Random seed for reproducible terrain
    /// * `octaves` - Number of noise layers (more = more detail)
    /// * `frequency` - Base frequency (higher = smaller features)
    /// * `amplitude` - Height variation scale
    /// * `height_offset` - Base terrain height
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
            .set_lacunarity(2.0)      // Frequency multiplier per octave
            .set_persistence(0.5);     // Amplitude multiplier per octave

        Self {
            fbm,
            amplitude,
            height_offset,
        }
    }

    /// Sample the SDF at a world position
    ///
    /// Returns:
    /// - Negative = inside terrain (solid)
    /// - Positive = outside terrain (air)
    /// - Zero = on surface
    pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        // Get noise value at this position
        let noise_value = self.fbm.get([x as f64, y as f64, z as f64]) as f32;

        // SDF formula: y - (height_offset + noise * amplitude)
        // When y < surface_height, result is negative (inside)
        // When y > surface_height, result is positive (outside)
        y - self.height_offset - noise_value * self.amplitude
    }

    /// Get the configured amplitude
    pub fn amplitude(&self) -> f32 {
        self.amplitude
    }

    /// Get the configured height offset
    pub fn height_offset(&self) -> f32 {
        self.height_offset
    }
}

/// Thread-safe shared noise field for parallel mesh generation
pub type SharedNoiseField = Arc<NoiseField>;

/// Create a thread-safe shared noise field
pub fn create_shared_noise_field(
    seed: u32,
    octaves: usize,
    frequency: f32,
    amplitude: f32,
    height_offset: f32,
) -> SharedNoiseField {
    Arc::new(NoiseField::new(seed, octaves, frequency, amplitude, height_offset))
}
```

### Step 2: Add Unit Tests

```rust
// Full path: rust/src/noise_field.rs (append)

#[cfg(test)]
mod tests {
    use super::*;

    fn test_field() -> NoiseField {
        // Create a simple test field
        // amplitude = 10, height_offset = 0
        // So surface is roughly at y = noise * 10
        NoiseField::new(42, 4, 0.02, 10.0, 0.0)
    }

    #[test]
    fn test_sdf_above_surface() {
        let field = test_field();
        // High above any possible terrain
        let sdf = field.sample(0.0, 100.0, 0.0);
        assert!(sdf > 0.0, "Should be positive (air) at y=100");
    }

    #[test]
    fn test_sdf_below_surface() {
        let field = test_field();
        // Deep below any possible terrain
        let sdf = field.sample(0.0, -100.0, 0.0);
        assert!(sdf < 0.0, "Should be negative (solid) at y=-100");
    }

    #[test]
    fn test_sdf_deterministic() {
        let field1 = NoiseField::new(42, 4, 0.02, 10.0, 0.0);
        let field2 = NoiseField::new(42, 4, 0.02, 10.0, 0.0);

        let v1 = field1.sample(10.0, 5.0, 10.0);
        let v2 = field2.sample(10.0, 5.0, 10.0);

        assert_eq!(v1, v2, "Same seed should produce same values");
    }

    #[test]
    fn test_different_seeds_differ() {
        let field1 = NoiseField::new(42, 4, 0.02, 10.0, 0.0);
        let field2 = NoiseField::new(99, 4, 0.02, 10.0, 0.0);

        let v1 = field1.sample(10.0, 5.0, 10.0);
        let v2 = field2.sample(10.0, 5.0, 10.0);

        assert_ne!(v1, v2, "Different seeds should produce different values");
    }

    #[test]
    fn test_height_offset() {
        // With height_offset = 50, surface moves up by 50
        let field = NoiseField::new(42, 1, 0.001, 1.0, 50.0);

        // At y = 40 (below offset), should be solid
        let below = field.sample(0.0, 40.0, 0.0);
        assert!(below < 0.0, "Should be solid below height_offset");

        // At y = 60 (above offset), should be air
        let above = field.sample(0.0, 60.0, 0.0);
        assert!(above > 0.0, "Should be air above height_offset");
    }

    #[test]
    fn test_amplitude_effect() {
        let low_amp = NoiseField::new(42, 4, 0.02, 1.0, 0.0);
        let high_amp = NoiseField::new(42, 4, 0.02, 100.0, 0.0);

        // Sample at same position
        let low_val = low_amp.sample(10.0, 0.0, 10.0);
        let high_val = high_amp.sample(10.0, 0.0, 10.0);

        // Higher amplitude should create larger variation
        // (both relative to y=0, but noise contribution differs)
        assert!(
            (high_val - low_val).abs() > 1.0,
            "Higher amplitude should create larger SDF difference"
        );
    }

    #[test]
    fn test_shared_noise_field() {
        let shared = create_shared_noise_field(42, 4, 0.02, 10.0, 0.0);

        // Can clone Arc for multiple threads
        let shared2 = Arc::clone(&shared);

        let v1 = shared.sample(5.0, 5.0, 5.0);
        let v2 = shared2.sample(5.0, 5.0, 5.0);

        assert_eq!(v1, v2);
    }
}
```

### Step 3: Register Module in lib.rs

```rust
// Full path: rust/src/lib.rs

use godot::prelude::*;

mod chunk;
mod lod;
mod noise_field;
mod terrain;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
```

### Step 4: Verify

```bash
cd rust && cargo test noise
```

Expected: All noise field tests pass.

## Verification Checklist

- [ ] `sample()` returns negative for points deep underground
- [ ] `sample()` returns positive for points high in the air
- [ ] Same seed produces identical values (deterministic)
- [ ] Different seeds produce different terrain
- [ ] `SharedNoiseField` can be cloned across threads

## Noise Parameters Explained

| Parameter | Effect | Typical Value |
|-----------|--------|---------------|
| `seed` | Unique terrain per value | 42 |
| `octaves` | Detail layers (4 = good detail) | 4 |
| `frequency` | Feature scale (0.02 = large hills) | 0.01-0.1 |
| `amplitude` | Height variation | 10-50 |
| `height_offset` | Base terrain height | 0.0 |

## What's Next

Walkthrough 05 implements transvoxel mesh extraction using this noise field.
