pub struct LODConfig {
    pub base_distance: f32,
    pub max_lod: u8,
    pub chunk_subdivisions: u32,
}

impl LODConfig {
    pub fn new(base_distance: f32, max_lod: u8, chunk_subdivisions: u32) -> Self {
        Self {
            base_distance,
            max_lod,
            chunk_subdivisions,
        }
    }

    pub fn lod_for_distance(&self, distance: f32) -> u8 {
        if distance <= 0.0 || distance <= self.base_distance {
            return 0;
        }
        let level = (distance / self.base_distance).log2().floor() as i32;
        (level.max(0) as u8).min(self.max_lod)
    }

    pub fn max_view_distance(&self) -> f32 {
        self.base_distance * (1 << self.max_lod) as f32
    }

    pub fn voxel_size_at_lod(&self, base_voxel_size: f32, lod: u8) -> f32 {
        base_voxel_size * (1 << lod) as f32
    }
}
