//! Brush system for terrain painting.
//!
//! Supports two modes:
//! - Elevation Mode: Two-phase painting (paint area → set height) for sculpting terrain
//! - Texture Mode: Paint different textures onto terrain surface

use std::collections::HashSet;

use crate::chunk::ChunkCoord;
use crate::noise_field::NoiseField;
use crate::terrain_modifications::{ModificationLayer, VoxelMod};
use crate::texture_layer::{TextureLayer, TextureWeights};

/// Brush operating mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BrushMode {
    /// Elevation sculpting mode: paint area, then adjust height
    #[default]
    Elevation,
    /// Texture painting mode: paint textures onto terrain surface
    Texture,
    /// Flatten mode: paint area, then flatten to click height on release
    Flatten,
    /// Plateau mode: paint area, then snap each cell to nearest step_size multiple on release
    Plateau,
    /// Smooth mode: Laplacian smoothing on the SDF field
    Smooth,
}

/// Direction constraint for flatten/plateau operations
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlattenDirection {
    /// Flatten both above and below the target plane
    #[default]
    Both,
    /// Only remove material above the target plane (make SDF more positive)
    Up,
    /// Only add material below the target plane (make SDF more negative)
    Down,
}

/// Shape of the brush footprint
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BrushShape {
    /// Square/rectangular brush
    #[default]
    Square,
    /// Circular brush
    Round,
}

/// Current phase of brush operation
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BrushPhase {
    /// No brush operation in progress
    #[default]
    Idle,
    /// Geometry mode: painting the area footprint (click + drag)
    PaintingArea,
    /// Geometry mode: adjusting height after area is defined (move up/down)
    AdjustingHeight,
    /// Texture mode: actively painting texture (click + drag)
    Painting,
}

/// A 2D cell position on the terrain (XZ plane)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellXZ {
    pub x: i32,
    pub z: i32,
}

impl CellXZ {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Convert to world position at a given Y height
    pub fn to_world(&self, y: f32, voxel_size: f32) -> (f32, f32, f32) {
        (
            self.x as f32 * voxel_size + voxel_size * 0.5,
            y,
            self.z as f32 * voxel_size + voxel_size * 0.5,
        )
    }
}

/// The footprint of a brush operation (set of affected XZ cells)
#[derive(Clone, Debug, Default)]
pub struct BrushFootprint {
    /// Set of affected XZ cells
    pub cells: HashSet<CellXZ>,
    /// Minimum bounds (for chunk tracking)
    pub min_x: i32,
    pub max_x: i32,
    pub min_z: i32,
    pub max_z: i32,
    /// Y level for geometry operations
    pub base_y: f32,
    /// Height delta for geometry operations
    pub height_delta: f32,
}

impl BrushFootprint {
    pub fn new() -> Self {
        Self {
            cells: HashSet::new(),
            min_x: i32::MAX,
            max_x: i32::MIN,
            min_z: i32::MAX,
            max_z: i32::MIN,
            base_y: 0.0,
            height_delta: 0.0,
        }
    }

    /// Add a cell to the footprint
    pub fn add_cell(&mut self, cell: CellXZ) {
        self.min_x = self.min_x.min(cell.x);
        self.max_x = self.max_x.max(cell.x);
        self.min_z = self.min_z.min(cell.z);
        self.max_z = self.max_z.max(cell.z);
        self.cells.insert(cell);
    }

    /// Check if the footprint contains a cell
    pub fn contains(&self, cell: &CellXZ) -> bool {
        self.cells.contains(cell)
    }

    /// Get all cells in the footprint
    pub fn iter(&self) -> impl Iterator<Item = &CellXZ> {
        self.cells.iter()
    }

    /// Get the number of cells in the footprint
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Check if the footprint is empty
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Clear the footprint
    pub fn clear(&mut self) {
        self.cells.clear();
        self.min_x = i32::MAX;
        self.max_x = i32::MIN;
        self.min_z = i32::MAX;
        self.max_z = i32::MIN;
        self.base_y = 0.0;
        self.height_delta = 0.0;
    }

    /// Compute the XZ centroid of the footprint in world coordinates
    pub fn compute_centroid(&self, voxel_size: f32) -> (f32, f32) {
        if self.cells.is_empty() {
            return (0.0, 0.0);
        }
        let (sum_x, sum_z) = self.cells.iter().fold((0.0f64, 0.0f64), |(sx, sz), cell| {
            let (wx, _, wz) = cell.to_world(0.0, voxel_size);
            (sx + wx as f64, sz + wz as f64)
        });
        let n = self.cells.len() as f64;
        ((sum_x / n) as f32, (sum_z / n) as f32)
    }

    /// Get affected chunk coordinates
    pub fn affected_chunks(&self, chunk_size: f32, voxel_size: f32) -> Vec<ChunkCoord> {
        if self.cells.is_empty() {
            return Vec::new();
        }

        let mut chunks = HashSet::new();
        let chunk_cells = (chunk_size / voxel_size) as i32;

        for cell in &self.cells {
            let chunk_x = cell.x.div_euclid(chunk_cells);
            let chunk_z = cell.z.div_euclid(chunk_cells);

            // Add chunks covering the full Y range of the modification
            // (base_y to base_y + height_delta, with voxel_size padding)
            let y_min = self.base_y + self.height_delta.min(0.0) - voxel_size;
            let y_max = self.base_y + self.height_delta.max(0.0) + voxel_size;
            let min_chunk_y = (y_min / chunk_size).floor() as i32;
            let max_chunk_y = (y_max / chunk_size).floor() as i32;
            for cy in (min_chunk_y - 1)..=(max_chunk_y + 1) {
                chunks.insert(ChunkCoord::new(chunk_x, cy, chunk_z));
            }
        }

        chunks.into_iter().collect()
    }

    /// Get affected chunk coordinates for an explicit Y range (used by flatten/plateau)
    pub fn affected_chunks_for_y_range(
        &self,
        chunk_size: f32,
        voxel_size: f32,
        y_min: f32,
        y_max: f32,
    ) -> Vec<ChunkCoord> {
        if self.cells.is_empty() {
            return Vec::new();
        }

        let mut chunks = HashSet::new();
        let chunk_cells = (chunk_size / voxel_size) as i32;

        let min_chunk_y = (y_min / chunk_size).floor() as i32;
        let max_chunk_y = (y_max / chunk_size).floor() as i32;

        for cell in &self.cells {
            let chunk_x = cell.x.div_euclid(chunk_cells);
            let chunk_z = cell.z.div_euclid(chunk_cells);

            for cy in (min_chunk_y - 1)..=(max_chunk_y + 1) {
                chunks.insert(ChunkCoord::new(chunk_x, cy, chunk_z));
            }
        }

        chunks.into_iter().collect()
    }
}

/// Brush state machine
#[derive(Clone, Debug)]
pub struct Brush {
    /// Current operating mode
    pub mode: BrushMode,
    /// Brush shape
    pub shape: BrushShape,
    /// Brush size (radius for round, half-size for square)
    pub size: f32,
    /// Brush strength (0-1, affects blend factor)
    pub strength: f32,
    /// Current phase of operation
    pub phase: BrushPhase,
    /// Current footprint being built/applied
    pub footprint: BrushFootprint,
    /// Selected texture index for texture mode
    pub selected_texture: usize,
    /// Starting mouse Y position for height adjustment
    pub height_adjust_start_y: f32,
    /// Sensitivity for height adjustment (world units per pixel)
    pub height_sensitivity: f32,
    /// Voxel size for coordinate calculations
    pub voxel_size: f32,
    /// Step size for plateau mode (world units per discrete level)
    pub step_size: f32,
    /// Feather: 0.0 = hard edge (pixel-art default), 1.0 = full smootherstep falloff
    pub feather: f32,
    /// Direction constraint for flatten/plateau operations
    pub flatten_direction: FlattenDirection,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            mode: BrushMode::Elevation,
            shape: BrushShape::Round,
            size: 5.0,
            strength: 1.0,
            phase: BrushPhase::Idle,
            footprint: BrushFootprint::new(),
            selected_texture: 0,
            height_adjust_start_y: 0.0,
            height_sensitivity: 0.1,
            voxel_size: 1.0,
            step_size: 4.0,
            feather: 0.0,
            flatten_direction: FlattenDirection::Both,
        }
    }
}

impl Brush {
    pub fn new(voxel_size: f32) -> Self {
        Self {
            voxel_size,
            ..Default::default()
        }
    }

    /// Check if brush is enabled (in a non-idle phase)
    pub fn is_active(&self) -> bool {
        self.phase != BrushPhase::Idle
    }

    /// Start a brush stroke at the given world position
    pub fn begin_stroke(&mut self, world_x: f32, world_y: f32, world_z: f32) {
        self.footprint.clear();
        self.footprint.base_y = world_y;

        match self.mode {
            BrushMode::Elevation => {
                self.phase = BrushPhase::PaintingArea;
                self.add_cells_at(world_x, world_z);
            }
            BrushMode::Texture | BrushMode::Flatten | BrushMode::Plateau | BrushMode::Smooth => {
                self.phase = BrushPhase::Painting;
                self.add_cells_at(world_x, world_z);
            }
        }
    }

    /// Continue a brush stroke (drag)
    pub fn continue_stroke(&mut self, world_x: f32, world_y: f32, world_z: f32) {
        match self.phase {
            BrushPhase::PaintingArea | BrushPhase::Painting => {
                self.add_cells_at(world_x, world_z);
            }
            BrushPhase::AdjustingHeight => {
                // Height is adjusted based on mouse Y movement
                let delta_y = world_y - self.height_adjust_start_y;
                self.footprint.height_delta = delta_y;
            }
            BrushPhase::Idle => {}
        }
    }

    /// End the current brush stroke phase
    pub fn end_stroke(&mut self, screen_y: f32) -> BrushAction {
        match self.phase {
            BrushPhase::PaintingArea => {
                // Transition to height adjustment phase
                self.phase = BrushPhase::AdjustingHeight;
                self.height_adjust_start_y = screen_y;
                self.footprint.height_delta = 0.0;
                BrushAction::BeginHeightAdjust
            }
            BrushPhase::AdjustingHeight => {
                // Commit the geometry modification
                self.phase = BrushPhase::Idle;
                BrushAction::CommitGeometry
            }
            BrushPhase::Painting => {
                self.phase = BrushPhase::Idle;
                match self.mode {
                    BrushMode::Texture => BrushAction::CommitTexture,
                    BrushMode::Flatten => BrushAction::CommitFlatten,
                    BrushMode::Plateau => BrushAction::CommitPlateau,
                    BrushMode::Smooth => BrushAction::CommitSmooth,
                    _ => BrushAction::CommitTexture,
                }
            }
            BrushPhase::Idle => BrushAction::None,
        }
    }

    /// Cancel the current brush operation
    pub fn cancel(&mut self) {
        self.footprint.clear();
        self.phase = BrushPhase::Idle;
    }

    /// Update height delta during adjustment phase
    pub fn adjust_height(&mut self, screen_y: f32) {
        if self.phase == BrushPhase::AdjustingHeight {
            let delta_pixels = screen_y - self.height_adjust_start_y;
            self.footprint.height_delta = -delta_pixels * self.height_sensitivity;
        }
    }

    /// Add cells at a world XZ position based on brush shape and size
    fn add_cells_at(&mut self, world_x: f32, world_z: f32) {
        let center_cell_x = (world_x / self.voxel_size).floor() as i32;
        let center_cell_z = (world_z / self.voxel_size).floor() as i32;
        let radius_cells = (self.size / self.voxel_size).ceil() as i32;

        match self.shape {
            BrushShape::Square => {
                for dz in -radius_cells..=radius_cells {
                    for dx in -radius_cells..=radius_cells {
                        self.footprint
                            .add_cell(CellXZ::new(center_cell_x + dx, center_cell_z + dz));
                    }
                }
            }
            BrushShape::Round => {
                let radius_sq = self.size * self.size;
                for dz in -radius_cells..=radius_cells {
                    for dx in -radius_cells..=radius_cells {
                        let dist_x = dx as f32 * self.voxel_size;
                        let dist_z = dz as f32 * self.voxel_size;
                        if dist_x * dist_x + dist_z * dist_z <= radius_sq {
                            self.footprint
                                .add_cell(CellXZ::new(center_cell_x + dx, center_cell_z + dz));
                        }
                    }
                }
            }
        }
    }

    /// Apply the current geometry modification to the modification layer
    pub fn apply_geometry(&self, mods: &mut ModificationLayer) -> Vec<ChunkCoord> {
        if self.footprint.is_empty() || self.footprint.height_delta.abs() < 0.001 {
            return Vec::new();
        }

        let base_y = self.footprint.base_y;
        let height_delta = self.footprint.height_delta;
        let (cx, cz) = self.footprint.compute_centroid(self.voxel_size);

        for cell in self.footprint.iter() {
            let (wx, _, wz) = cell.to_world(0.0, self.voxel_size);
            let falloff = self.compute_falloff(wx, wz, cx, cz);
            let cell_height = height_delta * falloff;

            let core_min = base_y + cell_height.min(0.0);
            let core_max = base_y + cell_height.max(0.0);
            let y_padding = 2.0 * self.voxel_size;

            // Apply modification at multiple Y levels around the affected area
            let y_min = (core_min - y_padding).floor();
            let y_max = (core_max + y_padding).ceil();
            let mut y = y_min;
            while y <= y_max {
                // Negate for SDF: positive height → negative SDF (adds material),
                // negative height → positive SDF (removes material)
                let y_falloff = self.compute_y_falloff(y, core_min, core_max, y_padding);
                let modification = VoxelMod::new(-cell_height * y_falloff, self.strength);
                mods.set_at_world(wx, y, wz, modification);
                y += self.voxel_size;
            }
        }

        self.footprint
            .affected_chunks(mods.chunk_size(), self.voxel_size)
    }

    /// Apply the current texture paint to the texture layer
    pub fn apply_texture(&self, textures: &mut TextureLayer) -> Vec<ChunkCoord> {
        if self.footprint.is_empty() {
            return Vec::new();
        }

        let weights = TextureWeights::single(self.selected_texture);
        let base_y = self.footprint.base_y;

        for cell in self.footprint.iter() {
            let (wx, _, wz) = cell.to_world(0.0, self.voxel_size);
            // Paint at the surface level
            textures.set_at_world(wx, base_y, wz, weights);
        }

        self.footprint
            .affected_chunks(textures.chunk_size(), self.voxel_size)
    }

    /// Get preview cells for visualization (world positions)
    pub fn get_preview_positions(&self, world_y: f32) -> Vec<(f32, f32, f32)> {
        self.footprint
            .iter()
            .map(|cell| cell.to_world(world_y, self.voxel_size))
            .collect()
    }

    /// Set brush mode
    pub fn set_mode(&mut self, mode: BrushMode) {
        if self.phase != BrushPhase::Idle {
            self.cancel();
        }
        self.mode = mode;
    }

    /// Set brush shape
    pub fn set_shape(&mut self, shape: BrushShape) {
        self.shape = shape;
    }

    /// Set brush size
    pub fn set_size(&mut self, size: f32) {
        self.size = size.max(self.voxel_size);
    }

    /// Set brush strength
    pub fn set_strength(&mut self, strength: f32) {
        self.strength = strength.clamp(0.0, 1.0);
    }

    /// Set selected texture for texture mode
    pub fn set_selected_texture(&mut self, index: usize) {
        self.selected_texture = index;
    }

    /// Set step size for plateau mode
    pub fn set_step_size(&mut self, step_size: f32) {
        self.step_size = step_size.max(0.1);
    }

    /// Set feather amount (0.0 = hard edge, 1.0 = full falloff)
    pub fn set_feather(&mut self, feather: f32) {
        self.feather = feather.clamp(0.0, 1.0);
    }

    /// Set flatten direction constraint
    pub fn set_flatten_direction(&mut self, direction: FlattenDirection) {
        self.flatten_direction = direction;
    }

    /// Quintic smootherstep (C² continuous — zero 1st and 2nd derivatives at 0 and 1).
    /// Avoids the ring/banding artifacts that linear or cubic smoothstep produce.
    pub fn smootherstep(t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }

    /// Compute feather falloff at a world XZ position relative to centroid.
    /// Returns 1.0 at center, falling to (1 - feather) at the brush edge.
    pub fn compute_falloff(&self, wx: f32, wz: f32, center_x: f32, center_z: f32) -> f32 {
        if self.feather <= 0.0 {
            return 1.0;
        }
        let dist = ((wx - center_x).powi(2) + (wz - center_z).powi(2)).sqrt();
        let ratio = (dist / self.size).clamp(0.0, 1.0);
        1.0 - self.feather * Self::smootherstep(ratio)
    }

    /// Compute Y-axis feather falloff.
    /// Returns 1.0 inside the core zone, tapering to (1 - feather) at the padding boundary.
    pub fn compute_y_falloff(&self, y: f32, core_min: f32, core_max: f32, padding: f32) -> f32 {
        if self.feather <= 0.0 || padding <= 0.0 {
            return 1.0;
        }
        // Distance from the core zone (0 if inside)
        let y_dist = if y < core_min {
            core_min - y
        } else if y > core_max {
            y - core_max
        } else {
            0.0
        };
        let ratio = (y_dist / padding).clamp(0.0, 1.0);
        1.0 - self.feather * Self::smootherstep(ratio)
    }

    /// Apply flatten modification: force terrain to a horizontal plane at base_y
    ///
    /// For each voxel in the footprint's Y range, computes the SDF delta needed
    /// to make the final SDF equal to `y - base_y` (a horizontal plane).
    pub fn apply_flatten(
        &self,
        noise: &NoiseField,
        existing_mods: &ModificationLayer,
        new_mods: &mut ModificationLayer,
    ) -> Vec<ChunkCoord> {
        if self.footprint.is_empty() {
            return Vec::new();
        }

        let base_y = self.footprint.base_y;
        let amplitude = noise.get_amplitude();
        let y_range = amplitude + 4.0 * self.voxel_size;
        let y_min = (base_y - y_range).floor();
        let y_max = (base_y + y_range).ceil();
        let (cx, cz) = self.footprint.compute_centroid(self.voxel_size);

        let core_min = base_y - self.voxel_size;
        let core_max = base_y + self.voxel_size;
        let y_padding = 4.0 * self.voxel_size;

        for cell in self.footprint.iter() {
            let (wx, _, wz) = cell.to_world(0.0, self.voxel_size);
            let falloff = self.compute_falloff(wx, wz, cx, cz);

            let mut y = y_min;
            while y <= y_max {
                let noise_base = noise.sample(wx, y, wz);
                let existing_mod_val = existing_mods.sample(wx, y, wz);
                let desired_sdf = y - base_y;
                let target_mod = desired_sdf - noise_base;
                let y_falloff = self.compute_y_falloff(y, core_min, core_max, y_padding);
                let blend = self.strength * falloff * y_falloff;
                let new_mod = existing_mod_val + (target_mod - existing_mod_val) * blend;

                // Apply direction filter
                let sdf_change = new_mod - existing_mod_val;
                let filtered_mod = match self.flatten_direction {
                    FlattenDirection::Both => new_mod,
                    FlattenDirection::Up => {
                        if sdf_change > 0.0 {
                            new_mod
                        } else {
                            existing_mod_val
                        }
                    }
                    FlattenDirection::Down => {
                        if sdf_change < 0.0 {
                            new_mod
                        } else {
                            existing_mod_val
                        }
                    }
                };

                if (filtered_mod - existing_mod_val).abs() > 0.0001 {
                    new_mods.set_at_world(wx, y, wz, VoxelMod::new(filtered_mod, 1.0));
                }

                y += self.voxel_size;
            }
        }

        self.footprint.affected_chunks_for_y_range(
            new_mods.chunk_size(),
            self.voxel_size,
            y_min,
            y_max,
        )
    }

    /// Apply plateau modification: snap each cell's surface to the nearest step_size multiple
    ///
    /// For each XZ cell, finds the current surface height via SDF zero-crossing search,
    /// rounds it to the nearest `step_size` multiple, then flattens to that height.
    pub fn apply_plateau(
        &self,
        noise: &NoiseField,
        existing_mods: &ModificationLayer,
        new_mods: &mut ModificationLayer,
    ) -> Vec<ChunkCoord> {
        if self.footprint.is_empty() {
            return Vec::new();
        }

        let base_y = self.footprint.base_y;
        let amplitude = noise.get_amplitude();
        let y_range = amplitude + 4.0 * self.voxel_size;
        let y_min = (base_y - y_range).floor();
        let y_max = (base_y + y_range).ceil();
        let (cx, cz) = self.footprint.compute_centroid(self.voxel_size);

        for cell in self.footprint.iter() {
            let (wx, _, wz) = cell.to_world(0.0, self.voxel_size);
            let falloff = self.compute_falloff(wx, wz, cx, cz);

            // Find the current surface height at this XZ column
            let surface_y =
                Self::find_surface_y(noise, existing_mods, wx, wz, y_min, y_max, self.voxel_size);

            // Snap to nearest step_size multiple
            let target_y = (surface_y / self.step_size).round() * self.step_size;

            let core_min = target_y - self.voxel_size;
            let core_max = target_y + self.voxel_size;
            let y_padding = 4.0 * self.voxel_size;

            // Apply flatten-style SDF override toward target_y
            let mut y = y_min;
            while y <= y_max {
                let noise_base = noise.sample(wx, y, wz);
                let existing_mod_val = existing_mods.sample(wx, y, wz);
                let desired_sdf = y - target_y;
                let target_mod = desired_sdf - noise_base;
                let y_falloff = self.compute_y_falloff(y, core_min, core_max, y_padding);
                let blend = self.strength * falloff * y_falloff;
                let new_mod = existing_mod_val + (target_mod - existing_mod_val) * blend;

                // Apply direction filter
                let sdf_change = new_mod - existing_mod_val;
                let filtered_mod = match self.flatten_direction {
                    FlattenDirection::Both => new_mod,
                    FlattenDirection::Up => {
                        if sdf_change > 0.0 {
                            new_mod
                        } else {
                            existing_mod_val
                        }
                    }
                    FlattenDirection::Down => {
                        if sdf_change < 0.0 {
                            new_mod
                        } else {
                            existing_mod_val
                        }
                    }
                };

                if (filtered_mod - existing_mod_val).abs() > 0.0001 {
                    new_mods.set_at_world(wx, y, wz, VoxelMod::new(filtered_mod, 1.0));
                }

                y += self.voxel_size;
            }
        }

        self.footprint.affected_chunks_for_y_range(
            new_mods.chunk_size(),
            self.voxel_size,
            y_min,
            y_max,
        )
    }

    /// Apply Laplacian smoothing to the SDF field.
    ///
    /// For each voxel in the footprint's Y range, samples the 6 face-adjacent neighbors,
    /// computes the Laplacian (neighbor average minus current value), and applies a
    /// weighted delta to smooth out sharp features.
    pub fn apply_smooth(
        &self,
        noise: &NoiseField,
        existing_mods: &ModificationLayer,
        new_mods: &mut ModificationLayer,
    ) -> Vec<ChunkCoord> {
        if self.footprint.is_empty() {
            return Vec::new();
        }

        let base_y = self.footprint.base_y;
        let amplitude = noise.get_amplitude();
        let y_range = amplitude + 4.0 * self.voxel_size;
        let y_min = (base_y - y_range).floor();
        let y_max = (base_y + y_range).ceil();
        let (cx, cz) = self.footprint.compute_centroid(self.voxel_size);

        let core_min = base_y - self.voxel_size;
        let core_max = base_y + self.voxel_size;
        let y_padding = 4.0 * self.voxel_size;

        let vs = self.voxel_size;

        for cell in self.footprint.iter() {
            let (wx, _, wz) = cell.to_world(0.0, vs);
            let falloff = self.compute_falloff(wx, wz, cx, cz);

            let mut y = y_min;
            while y <= y_max {
                let current_sdf = noise.sample_with_mods(wx, y, wz, existing_mods);

                // Sample 6 face-adjacent neighbors
                let n_xp = noise.sample_with_mods(wx + vs, y, wz, existing_mods);
                let n_xn = noise.sample_with_mods(wx - vs, y, wz, existing_mods);
                let n_yp = noise.sample_with_mods(wx, y + vs, wz, existing_mods);
                let n_yn = noise.sample_with_mods(wx, y - vs, wz, existing_mods);
                let n_zp = noise.sample_with_mods(wx, y, wz + vs, existing_mods);
                let n_zn = noise.sample_with_mods(wx, y, wz - vs, existing_mods);

                let neighbor_avg = (n_xp + n_xn + n_yp + n_yn + n_zp + n_zn) / 6.0;
                let laplacian = neighbor_avg - current_sdf;

                let y_falloff = self.compute_y_falloff(y, core_min, core_max, y_padding);
                let sdf_delta = laplacian * self.strength * falloff * y_falloff;

                if sdf_delta.abs() > 0.0001 {
                    new_mods.set_at_world(wx, y, wz, VoxelMod::new(sdf_delta, 1.0));
                }

                y += vs;
            }
        }

        self.footprint.affected_chunks_for_y_range(
            new_mods.chunk_size(),
            self.voxel_size,
            y_min,
            y_max,
        )
    }

    /// Find the surface Y position at a given XZ column by searching for SDF zero-crossing.
    ///
    /// Walks from y_max downward in voxel_size steps, finds the first positive→negative
    /// SDF transition, then bisects 8 iterations for sub-voxel accuracy.
    fn find_surface_y(
        noise: &NoiseField,
        mods: &ModificationLayer,
        wx: f32,
        wz: f32,
        y_min: f32,
        y_max: f32,
        voxel_size: f32,
    ) -> f32 {
        let mut prev_sdf = noise.sample_with_mods(wx, y_max, wz, mods);
        let mut y = y_max - voxel_size;

        // Walk downward looking for sign change (positive → negative = entering solid)
        while y >= y_min {
            let sdf = noise.sample_with_mods(wx, y, wz, mods);
            if prev_sdf >= 0.0 && sdf < 0.0 {
                // Found a crossing between y and y+voxel_size. Bisect for precision.
                let mut lo = y;
                let mut hi = y + voxel_size;
                for _ in 0..8 {
                    let mid = (lo + hi) * 0.5;
                    let mid_sdf = noise.sample_with_mods(wx, mid, wz, mods);
                    if mid_sdf < 0.0 {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                return (lo + hi) * 0.5;
            }
            prev_sdf = sdf;
            y -= voxel_size;
        }

        // No crossing found — fall back to base_y
        (y_min + y_max) * 0.5
    }
}

/// Action returned when ending a brush stroke
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushAction {
    /// No action needed
    None,
    /// Begin height adjustment phase (geometry mode)
    BeginHeightAdjust,
    /// Commit geometry modification
    CommitGeometry,
    /// Commit texture paint
    CommitTexture,
    /// Commit flatten modification
    CommitFlatten,
    /// Commit plateau modification
    CommitPlateau,
    /// Commit smooth modification
    CommitSmooth,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brush_default() {
        let brush = Brush::default();
        assert_eq!(brush.mode, BrushMode::Elevation);
        assert_eq!(brush.shape, BrushShape::Round);
        assert_eq!(brush.phase, BrushPhase::Idle);
        assert!(!brush.is_active());
    }

    #[test]
    fn test_brush_geometry_workflow() {
        let mut brush = Brush::new(1.0);
        brush.set_size(3.0);
        brush.set_shape(BrushShape::Square);

        // Begin stroke
        brush.begin_stroke(10.0, 5.0, 10.0);
        assert_eq!(brush.phase, BrushPhase::PaintingArea);
        assert!(brush.is_active());
        assert!(!brush.footprint.is_empty());

        // End first phase (transition to height adjust)
        let action = brush.end_stroke(100.0);
        assert_eq!(action, BrushAction::BeginHeightAdjust);
        assert_eq!(brush.phase, BrushPhase::AdjustingHeight);

        // Adjust height
        brush.adjust_height(50.0); // Move up 50 pixels
        assert!(brush.footprint.height_delta.abs() > 0.0);

        // End second phase (commit)
        let action = brush.end_stroke(50.0);
        assert_eq!(action, BrushAction::CommitGeometry);
        assert_eq!(brush.phase, BrushPhase::Idle);
    }

    #[test]
    fn test_brush_texture_workflow() {
        let mut brush = Brush::new(1.0);
        brush.set_mode(BrushMode::Texture);
        brush.set_size(2.0);
        brush.set_selected_texture(2);

        // Begin stroke
        brush.begin_stroke(5.0, 10.0, 5.0);
        assert_eq!(brush.phase, BrushPhase::Painting);

        // Continue stroke
        brush.continue_stroke(7.0, 10.0, 7.0);

        // End stroke (commit)
        let action = brush.end_stroke(100.0);
        assert_eq!(action, BrushAction::CommitTexture);
        assert_eq!(brush.phase, BrushPhase::Idle);
    }

    #[test]
    fn test_brush_cancel() {
        let mut brush = Brush::new(1.0);
        brush.begin_stroke(10.0, 5.0, 10.0);
        assert!(brush.is_active());

        brush.cancel();
        assert!(!brush.is_active());
        assert!(brush.footprint.is_empty());
    }

    #[test]
    fn test_brush_round_shape() {
        let mut brush = Brush::new(1.0);
        brush.set_shape(BrushShape::Round);
        brush.set_size(2.0);
        brush.begin_stroke(0.0, 0.0, 0.0);

        // Round brush should have fewer cells than square of same size
        let round_count = brush.footprint.len();

        brush.cancel();
        brush.set_shape(BrushShape::Square);
        brush.begin_stroke(0.0, 0.0, 0.0);
        let square_count = brush.footprint.len();

        assert!(round_count <= square_count);
    }

    #[test]
    fn test_footprint_affected_chunks() {
        let mut footprint = BrushFootprint::new();
        footprint.base_y = 16.0;

        // Add cells that span chunk boundaries
        footprint.add_cell(CellXZ::new(30, 30));
        footprint.add_cell(CellXZ::new(33, 33)); // Next chunk at resolution 32

        let chunks = footprint.affected_chunks(32.0, 1.0);
        assert!(chunks.len() >= 2, "Should affect multiple chunks");
    }

    #[test]
    fn test_brush_flatten_workflow() {
        let mut brush = Brush::new(1.0);
        brush.set_mode(BrushMode::Flatten);
        brush.set_size(2.0);

        // Begin stroke (goes to Painting phase for flatten)
        brush.begin_stroke(5.0, 10.0, 5.0);
        assert_eq!(brush.phase, BrushPhase::Painting);

        // Continue stroke
        brush.continue_stroke(7.0, 10.0, 7.0);

        // End stroke → CommitFlatten
        let action = brush.end_stroke(100.0);
        assert_eq!(action, BrushAction::CommitFlatten);
        assert_eq!(brush.phase, BrushPhase::Idle);
    }

    #[test]
    fn test_brush_plateau_workflow() {
        let mut brush = Brush::new(1.0);
        brush.set_mode(BrushMode::Plateau);
        brush.set_size(2.0);
        brush.set_step_size(4.0);

        // Begin stroke (goes to Painting phase for plateau)
        brush.begin_stroke(5.0, 10.0, 5.0);
        assert_eq!(brush.phase, BrushPhase::Painting);

        // Continue stroke
        brush.continue_stroke(7.0, 10.0, 7.0);

        // End stroke → CommitPlateau
        let action = brush.end_stroke(100.0);
        assert_eq!(action, BrushAction::CommitPlateau);
        assert_eq!(brush.phase, BrushPhase::Idle);
    }

    #[test]
    fn test_brush_step_size_clamped() {
        let mut brush = Brush::new(1.0);
        brush.set_step_size(0.0);
        assert!(brush.step_size >= 0.1);
        brush.set_step_size(-5.0);
        assert!(brush.step_size >= 0.1);
        brush.set_step_size(8.0);
        assert_eq!(brush.step_size, 8.0);
    }

    #[test]
    fn test_footprint_affected_chunks_for_y_range() {
        let mut footprint = BrushFootprint::new();
        footprint.add_cell(CellXZ::new(5, 5));

        let chunks = footprint.affected_chunks_for_y_range(32.0, 1.0, -10.0, 50.0);
        assert!(!chunks.is_empty(), "Should have affected chunks");
        // Y range -10 to 50 with chunk_size 32: chunks at Y=-1, 0, 1 (plus padding)
        assert!(chunks.len() >= 3, "Should span multiple Y chunks");
    }

    #[test]
    fn test_smootherstep_boundaries() {
        // C² continuous: zero derivatives at 0 and 1
        assert!((Brush::smootherstep(0.0)).abs() < 0.001);
        assert!((Brush::smootherstep(1.0) - 1.0).abs() < 0.001);
        assert!((Brush::smootherstep(0.5) - 0.5).abs() < 0.001); // symmetric midpoint
    }

    #[test]
    fn test_feather_zero_is_hard_edge() {
        let mut brush = Brush::new(1.0);
        brush.set_feather(0.0);
        // All cells should get falloff = 1.0 regardless of distance
        assert!((brush.compute_falloff(5.0, 0.0, 0.0, 0.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_feather_full_falloff() {
        let mut brush = Brush::new(1.0);
        brush.size = 10.0;
        brush.set_feather(1.0);
        // Center = full strength
        assert!((brush.compute_falloff(0.0, 0.0, 0.0, 0.0) - 1.0).abs() < 0.001);
        // Edge = zero strength
        assert!(brush.compute_falloff(10.0, 0.0, 0.0, 0.0) < 0.01);
    }

    #[test]
    fn test_feather_clamped() {
        let mut brush = Brush::new(1.0);
        brush.set_feather(-0.5);
        assert_eq!(brush.feather, 0.0);
        brush.set_feather(1.5);
        assert_eq!(brush.feather, 1.0);
        brush.set_feather(0.5);
        assert_eq!(brush.feather, 0.5);
    }

    #[test]
    fn test_y_falloff_inside_core() {
        let mut brush = Brush::new(1.0);
        brush.set_feather(1.0);
        // Inside core zone: falloff = 1.0
        assert!((brush.compute_y_falloff(5.0, 3.0, 7.0, 2.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_y_falloff_at_padding_edge() {
        let mut brush = Brush::new(1.0);
        brush.set_feather(1.0);
        // At the far edge of padding (core_max + padding): falloff ≈ 0.0
        assert!(brush.compute_y_falloff(9.0, 3.0, 7.0, 2.0) < 0.01);
        // At the far edge below (core_min - padding):
        assert!(brush.compute_y_falloff(1.0, 3.0, 7.0, 2.0) < 0.01);
    }

    #[test]
    fn test_y_falloff_zero_feather() {
        let mut brush = Brush::new(1.0);
        brush.set_feather(0.0);
        // No feather: always 1.0 regardless of Y position
        assert!((brush.compute_y_falloff(100.0, 3.0, 7.0, 2.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_brush_smooth_workflow() {
        let mut brush = Brush::new(1.0);
        brush.set_mode(BrushMode::Smooth);
        brush.set_size(2.0);

        // Begin stroke (goes to Painting phase for smooth)
        brush.begin_stroke(5.0, 10.0, 5.0);
        assert_eq!(brush.phase, BrushPhase::Painting);

        // Continue stroke
        brush.continue_stroke(7.0, 10.0, 7.0);

        // End stroke → CommitSmooth
        let action = brush.end_stroke(100.0);
        assert_eq!(action, BrushAction::CommitSmooth);
        assert_eq!(brush.phase, BrushPhase::Idle);
    }

    #[test]
    fn test_flatten_direction_default_is_both() {
        let brush = Brush::default();
        assert_eq!(brush.flatten_direction, FlattenDirection::Both);
    }

    #[test]
    fn test_flatten_direction_up() {
        let mut brush = Brush::new(1.0);
        brush.set_flatten_direction(FlattenDirection::Up);
        assert_eq!(brush.flatten_direction, FlattenDirection::Up);
    }
}
