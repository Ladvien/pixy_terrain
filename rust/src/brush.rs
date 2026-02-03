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
    /// Geometry mode: adjusting curvature after height is set (move up/down for dome/bowl)
    AdjustingCurvature,
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
    /// Curvature amount: -1.0 (bowl) to 1.0 (dome), 0.0 = flat
    pub curvature: f32,
    /// Starting mouse Y position for curvature adjustment
    pub curvature_adjust_start_y: f32,
    /// Maximum cell X index (exclusive) for terrain bounds clamping
    pub max_cell_x: i32,
    /// Maximum cell Z index (exclusive) for terrain bounds clamping
    pub max_cell_z: i32,
    /// World-space XZ centers of each brush stamp (polyline for stroke path)
    pub stroke_centers: Vec<(f32, f32)>,
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
            curvature: 0.0,
            curvature_adjust_start_y: 0.0,
            max_cell_x: i32::MAX,
            max_cell_z: i32::MAX,
            stroke_centers: Vec::new(),
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
        self.stroke_centers.clear();
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
            BrushPhase::AdjustingCurvature => {
                // Curvature is adjusted via adjust_curvature(), not world position
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
                // Transition to curvature adjustment phase
                self.phase = BrushPhase::AdjustingCurvature;
                self.curvature_adjust_start_y = screen_y;
                self.curvature = 0.0;
                BrushAction::BeginCurvatureAdjust
            }
            BrushPhase::AdjustingCurvature => {
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
        self.stroke_centers.clear();
        self.phase = BrushPhase::Idle;
        self.curvature = 0.0;
    }

    /// Update height delta during adjustment phase
    pub fn adjust_height(&mut self, screen_y: f32) {
        if self.phase == BrushPhase::AdjustingHeight {
            let delta_pixels = screen_y - self.height_adjust_start_y;
            self.footprint.height_delta = -delta_pixels * self.height_sensitivity;
        }
    }

    /// Update curvature during curvature adjustment phase
    pub fn adjust_curvature(&mut self, screen_y: f32) {
        if self.phase == BrushPhase::AdjustingCurvature {
            let delta_pixels = screen_y - self.curvature_adjust_start_y;
            // Down = positive curvature (dome), up = negative (bowl)
            self.curvature = (delta_pixels * self.height_sensitivity * 0.5).clamp(-1.0, 1.0);
        }
    }

    /// Add cells at a world XZ position based on brush shape and size
    fn add_cells_at(&mut self, world_x: f32, world_z: f32) {
        self.stroke_centers.push((world_x, world_z));
        let center_cell_x = (world_x / self.voxel_size).floor() as i32;
        let center_cell_z = (world_z / self.voxel_size).floor() as i32;
        let radius_cells = (self.size / self.voxel_size).ceil() as i32;

        match self.shape {
            BrushShape::Square => {
                for dz in -radius_cells..=radius_cells {
                    for dx in -radius_cells..=radius_cells {
                        let cx = center_cell_x + dx;
                        let cz = center_cell_z + dz;
                        if cx < 0 || cx >= self.max_cell_x || cz < 0 || cz >= self.max_cell_z {
                            continue;
                        }
                        self.footprint.add_cell(CellXZ::new(cx, cz));
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
                            let cx = center_cell_x + dx;
                            let cz = center_cell_z + dz;
                            if cx < 0
                                || cx >= self.max_cell_x
                                || cz < 0
                                || cz >= self.max_cell_z
                            {
                                continue;
                            }
                            self.footprint.add_cell(CellXZ::new(cx, cz));
                        }
                    }
                }
            }
        }
    }

    /// Apply the current geometry modification to the modification layer.
    ///
    /// Uses absolute SDF mode: stores `desired_sdf` (the target SDF value) with a
    /// blend factor. During mesh generation, the final SDF is computed as:
    ///   `noise * (1 - blend) + desired_sdf * blend`
    ///
    /// This eliminates interpolation artifacts because `desired_sdf = y - target_y`
    /// is linear in y, and trilinear interpolation of a linear function is exact.
    pub fn apply_geometry(
        &self,
        noise: &NoiseField,
        existing_mods: &ModificationLayer,
        new_mods: &mut ModificationLayer,
    ) -> Vec<ChunkCoord> {
        if self.footprint.is_empty() || self.footprint.height_delta.abs() < 0.001 {
            return Vec::new();
        }

        let base_y = self.footprint.base_y;
        let height_delta = self.footprint.height_delta;
        let floor_y = noise.get_floor_y();
        let height_offset = noise.get_height_offset();
        // Use effective amplitude (accounts for FBM octave summation) for terrain_peak
        // to prevent fragment artifacts from unmodified noise above the brush zone.
        // Keep raw amplitude for terrain_trough to avoid unnecessary downward expansion.
        let terrain_peak = floor_y + height_offset + noise.get_effective_amplitude();
        let terrain_trough = floor_y + height_offset - noise.get_amplitude();
        let target_max = base_y + height_delta.max(0.0);
        let target_min = base_y + height_delta.min(0.0);
        let padding = 4.0 * self.voxel_size;
        let (y_min, y_max) = Self::clamp_y_range(
            (terrain_trough.min(target_min) - padding).floor(),
            (terrain_peak.max(target_max) + padding).ceil(),
            noise,
        );
        if y_min >= y_max {
            return Vec::new();
        }

        for cell in self.footprint.iter() {
            let (wx, _, wz) = cell.to_world(0.0, self.voxel_size);
            // Grid-aligned position for noise sampling (matches mesh generation)
            let grid_x = cell.x as f32 * self.voxel_size;
            let grid_z = cell.z as f32 * self.voxel_size;
            let falloff = self.compute_stroke_falloff(wx, wz);
            let curvature_factor = if self.curvature.abs() > 0.001 {
                let dist = self.min_distance_to_stroke(wx, wz);
                let ratio = (dist / self.size).clamp(0.0, 1.0);
                if self.curvature > 0.0 {
                    // Dome: 1.0 at center, (1-curvature) at edge
                    1.0 - self.curvature * Self::smootherstep(ratio)
                } else {
                    // Bowl: (1+curvature) at center, 1.0 at edge
                    1.0 + self.curvature * (1.0 - Self::smootherstep(ratio))
                }
            } else {
                1.0
            };

            let raised_y = base_y + height_delta * curvature_factor;
            let original_surface_y = Self::find_surface_y(
                noise, existing_mods, wx, wz, y_min, y_max, self.voxel_size,
            );
            let target_y = original_surface_y + (raised_y - original_surface_y) * falloff;

            let mut y = y_min;
            while y <= y_max {
                let desired_sdf = y - target_y;
                let new_blend = self.strength;

                // Read existing modification state
                let (existing_desired, existing_blend) =
                    match existing_mods.get_at_world(grid_x, y, grid_z) {
                        Some(m) => (m.desired_sdf, m.blend),
                        None => (noise.sample(grid_x, y, grid_z), 0.0),
                    };

                // Alpha-composite: layer new brush on top of existing
                let combined_blend =
                    1.0 - (1.0 - existing_blend) * (1.0 - new_blend);

                if combined_blend > 0.0001 {
                    let combined_desired = (existing_desired * existing_blend * (1.0 - new_blend)
                        + desired_sdf * new_blend)
                        / combined_blend;

                    let changed = (combined_desired - existing_desired).abs() > 0.0001
                        || (combined_blend - existing_blend).abs() > 0.0001;
                    if changed {
                        new_mods.set_at_world(
                            grid_x,
                            y,
                            grid_z,
                            VoxelMod::new(combined_desired, combined_blend),
                        );
                    }
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

    /// Set terrain bounds for XZ cell clamping.
    /// Cells outside [0, max_cell_x) x [0, max_cell_z) will be rejected.
    pub fn set_terrain_bounds(&mut self, max_cell_x: i32, max_cell_z: i32) {
        self.max_cell_x = max_cell_x;
        self.max_cell_z = max_cell_z;
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

    /// Squared distance from point (px, pz) to line segment (ax, az)–(bx, bz).
    fn point_to_segment_dist_sq(px: f32, pz: f32, ax: f32, az: f32, bx: f32, bz: f32) -> f32 {
        let dx = bx - ax;
        let dz = bz - az;
        let len_sq = dx * dx + dz * dz;
        if len_sq < 1e-12 {
            // Degenerate segment (a == b): point distance
            return (px - ax).powi(2) + (pz - az).powi(2);
        }
        // Project point onto line, clamp to [0,1]
        let t = ((px - ax) * dx + (pz - az) * dz) / len_sq;
        let t = t.clamp(0.0, 1.0);
        let proj_x = ax + t * dx;
        let proj_z = az + t * dz;
        (px - proj_x).powi(2) + (pz - proj_z).powi(2)
    }

    /// Minimum distance from point (px, pz) to the stroke polyline.
    /// With a single center, degenerates to point distance (preserving dome for single-click).
    pub fn min_distance_to_stroke(&self, px: f32, pz: f32) -> f32 {
        let centers = &self.stroke_centers;
        if centers.is_empty() {
            return f32::MAX;
        }
        if centers.len() == 1 {
            let (cx, cz) = centers[0];
            return ((px - cx).powi(2) + (pz - cz).powi(2)).sqrt();
        }
        let mut min_sq = f32::MAX;
        for i in 0..centers.len() - 1 {
            let (ax, az) = centers[i];
            let (bx, bz) = centers[i + 1];
            let d_sq = Self::point_to_segment_dist_sq(px, pz, ax, az, bx, bz);
            if d_sq < min_sq {
                min_sq = d_sq;
            }
        }
        min_sq.sqrt()
    }

    /// Compute feather falloff using distance to the stroke polyline.
    /// Same logic as `compute_falloff` but measures distance to the nearest
    /// stroke segment instead of to a single centroid.
    pub fn compute_stroke_falloff(&self, wx: f32, wz: f32) -> f32 {
        if self.feather <= 0.0 {
            return 1.0;
        }
        let dist = self.min_distance_to_stroke(wx, wz);
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

    /// Apply flatten modification: force terrain to a horizontal plane at base_y.
    ///
    /// Uses absolute SDF mode with direction filtering. The desired SDF for a flat
    /// plane is `y - base_y`, which interpolates exactly (linear in y).
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
        let floor_y = noise.get_floor_y();
        let height_offset = noise.get_height_offset();
        let terrain_peak = floor_y + height_offset + noise.get_effective_amplitude();
        let terrain_trough = floor_y + height_offset - noise.get_amplitude();
        let padding = 4.0 * self.voxel_size;
        let (y_min, y_max) = Self::clamp_y_range(
            (terrain_trough.min(base_y) - padding).floor(),
            (terrain_peak.max(base_y) + padding).ceil(),
            noise,
        );
        if y_min >= y_max {
            return Vec::new();
        }
        for cell in self.footprint.iter() {
            let (wx, _, wz) = cell.to_world(0.0, self.voxel_size);
            let falloff = self.compute_stroke_falloff(wx, wz);
            let original_surface_y = Self::find_surface_y(
                noise, existing_mods, wx, wz, y_min, y_max, self.voxel_size,
            );
            let target_y = original_surface_y + (base_y - original_surface_y) * falloff;

            let mut y = y_min;
            while y <= y_max {
                let desired_sdf = y - target_y;
                let new_blend = self.strength;

                let noise_val = noise.sample(wx, y, wz);

                // Read existing modification state
                let (existing_desired, existing_blend) =
                    match existing_mods.get_at_world(wx, y, wz) {
                        Some(m) => (m.desired_sdf, m.blend),
                        None => (noise_val, 0.0),
                    };

                // Alpha-composite
                let combined_blend =
                    1.0 - (1.0 - existing_blend) * (1.0 - new_blend);

                if combined_blend > 0.0001 {
                    let combined_desired =
                        (existing_desired * existing_blend * (1.0 - new_blend)
                            + desired_sdf * new_blend)
                            / combined_blend;

                    // Direction filter: compare effective SDF values
                    let current_sdf =
                        noise_val * (1.0 - existing_blend) + existing_desired * existing_blend;
                    let new_sdf =
                        noise_val * (1.0 - combined_blend) + combined_desired * combined_blend;
                    let sdf_change = new_sdf - current_sdf;

                    let (final_desired, final_blend) = match self.flatten_direction {
                        FlattenDirection::Both => (combined_desired, combined_blend),
                        FlattenDirection::Up => {
                            // Only remove material (make SDF more positive)
                            if sdf_change > 0.0 {
                                (combined_desired, combined_blend)
                            } else {
                                (existing_desired, existing_blend)
                            }
                        }
                        FlattenDirection::Down => {
                            // Only add material (make SDF more negative)
                            if sdf_change < 0.0 {
                                (combined_desired, combined_blend)
                            } else {
                                (existing_desired, existing_blend)
                            }
                        }
                    };

                    let changed = (final_desired - existing_desired).abs() > 0.0001
                        || (final_blend - existing_blend).abs() > 0.0001;
                    if changed {
                        new_mods.set_at_world(wx, y, wz, VoxelMod::new(final_desired, final_blend));
                    }
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

    /// Apply plateau modification: snap each cell's surface to the nearest step_size multiple.
    ///
    /// Uses absolute SDF mode. For each XZ cell, finds the current surface height,
    /// rounds to nearest `step_size` multiple, then stores `desired_sdf = y - target_y`.
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
        let floor_y = noise.get_floor_y();
        let height_offset = noise.get_height_offset();
        let terrain_peak = floor_y + height_offset + noise.get_effective_amplitude();
        let terrain_trough = floor_y + height_offset - noise.get_amplitude();
        let padding = 4.0 * self.voxel_size;
        let (y_min, y_max) = Self::clamp_y_range(
            (terrain_trough.min(base_y) - padding).floor(),
            (terrain_peak.max(base_y) + padding).ceil(),
            noise,
        );
        if y_min >= y_max {
            return Vec::new();
        }
        for cell in self.footprint.iter() {
            let (wx, _, wz) = cell.to_world(0.0, self.voxel_size);
            let falloff = self.compute_stroke_falloff(wx, wz);

            // Find the current surface height at this XZ column
            let original_surface_y =
                Self::find_surface_y(noise, existing_mods, wx, wz, y_min, y_max, self.voxel_size);

            // Snap to nearest step_size multiple
            let snapped_y = (original_surface_y / self.step_size).round() * self.step_size;
            let target_y = original_surface_y + (snapped_y - original_surface_y) * falloff;

            let mut y = y_min;
            while y <= y_max {
                let desired_sdf = y - target_y;
                let new_blend = self.strength;

                let noise_val = noise.sample(wx, y, wz);

                // Read existing modification state
                let (existing_desired, existing_blend) =
                    match existing_mods.get_at_world(wx, y, wz) {
                        Some(m) => (m.desired_sdf, m.blend),
                        None => (noise_val, 0.0),
                    };

                // Alpha-composite
                let combined_blend =
                    1.0 - (1.0 - existing_blend) * (1.0 - new_blend);

                if combined_blend > 0.0001 {
                    let combined_desired =
                        (existing_desired * existing_blend * (1.0 - new_blend)
                            + desired_sdf * new_blend)
                            / combined_blend;

                    // Direction filter: compare effective SDF values
                    let current_sdf =
                        noise_val * (1.0 - existing_blend) + existing_desired * existing_blend;
                    let new_sdf =
                        noise_val * (1.0 - combined_blend) + combined_desired * combined_blend;
                    let sdf_change = new_sdf - current_sdf;

                    let (final_desired, final_blend) = match self.flatten_direction {
                        FlattenDirection::Both => (combined_desired, combined_blend),
                        FlattenDirection::Up => {
                            if sdf_change > 0.0 {
                                (combined_desired, combined_blend)
                            } else {
                                (existing_desired, existing_blend)
                            }
                        }
                        FlattenDirection::Down => {
                            if sdf_change < 0.0 {
                                (combined_desired, combined_blend)
                            } else {
                                (existing_desired, existing_blend)
                            }
                        }
                    };

                    let changed = (final_desired - existing_desired).abs() > 0.0001
                        || (final_blend - existing_blend).abs() > 0.0001;
                    if changed {
                        new_mods.set_at_world(wx, y, wz, VoxelMod::new(final_desired, final_blend));
                    }
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
    /// For each voxel, samples the 6 face-adjacent neighbors, computes the Laplacian,
    /// and stores the smoothed result as the desired SDF with blend=1.0.
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
        let floor_y = noise.get_floor_y();
        let height_offset = noise.get_height_offset();
        let terrain_peak = floor_y + height_offset + noise.get_effective_amplitude();
        let terrain_trough = floor_y + height_offset - noise.get_amplitude();
        let padding = 4.0 * self.voxel_size;
        let (y_min, y_max) = Self::clamp_y_range(
            (terrain_trough.min(base_y) - padding).floor(),
            (terrain_peak.max(base_y) + padding).ceil(),
            noise,
        );
        if y_min >= y_max {
            return Vec::new();
        }
        let core_min = base_y - self.voxel_size;
        let core_max = base_y + self.voxel_size;
        let y_padding = 4.0 * self.voxel_size;

        let vs = self.voxel_size;

        for cell in self.footprint.iter() {
            let (wx, _, wz) = cell.to_world(0.0, vs);
            let falloff = self.compute_stroke_falloff(wx, wz);

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
                let smooth_amount = self.strength * falloff * y_falloff;

                // The smoothed SDF is the current value moved toward the neighbor average
                let smoothed_sdf = current_sdf + laplacian * smooth_amount;

                // Read existing modification state
                let (existing_desired, existing_blend) =
                    match existing_mods.get_at_world(wx, y, wz) {
                        Some(m) => (m.desired_sdf, m.blend),
                        None => (noise.sample(wx, y, wz), 0.0),
                    };

                // For smooth, the result IS the desired value — store with blend=1.0
                // because we've already computed the final combined SDF we want.
                let changed = (smoothed_sdf - existing_desired).abs() > 0.0001
                    || (1.0 - existing_blend).abs() > 0.0001;
                if changed && smooth_amount > 0.0001 {
                    new_mods.set_at_world(wx, y, wz, VoxelMod::new(smoothed_sdf, 1.0));
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

    /// Clamp Y iteration range to terrain box bounds.
    /// Returns (clamped_y_min, clamped_y_max).
    fn clamp_y_range(y_min: f32, y_max: f32, noise: &NoiseField) -> (f32, f32) {
        if let Some((box_min, box_max)) = noise.get_box_bounds() {
            (y_min.max(box_min[1]), y_max.min(box_max[1]))
        } else {
            (y_min, y_max)
        }
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
    /// Begin curvature adjustment phase (geometry mode)
    BeginCurvatureAdjust,
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

        // End second phase (transition to curvature adjust)
        let action = brush.end_stroke(50.0);
        assert_eq!(action, BrushAction::BeginCurvatureAdjust);
        assert_eq!(brush.phase, BrushPhase::AdjustingCurvature);

        // Adjust curvature
        brush.adjust_curvature(60.0); // Move down 10 pixels → dome
        assert!(brush.curvature.abs() > 0.0);

        // End third phase (commit)
        let action = brush.end_stroke(60.0);
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

    #[test]
    fn test_elevation_large_height_delta_reaches_target() {
        // Regression test: elevation brush with large height_delta should produce
        // SDF modifications that define a surface at the expected target height,
        // not capped by the Y iteration range.
        let voxel_size = 1.0;
        let amplitude = 2.0;
        let height_delta = 10.0;

        // Create a flat noise field (amplitude=2 but we use a known seed)
        let noise = NoiseField::new(
            42,    // seed
            1,     // octaves
            0.01,  // frequency (low = nearly flat)
            amplitude,
            0.0,   // height_offset
            0.0,   // floor_y
            None,  // no box bounds
        );

        let existing_mods = ModificationLayer::new(32, voxel_size);
        let mut new_mods = ModificationLayer::new(32, voxel_size);

        // Set up brush with a single cell, large height_delta
        let mut brush = Brush::new(voxel_size);
        brush.set_mode(BrushMode::Elevation);
        brush.set_shape(BrushShape::Square);
        brush.set_size(1.0);
        brush.set_strength(1.0);
        brush.set_feather(0.0);

        brush.footprint.clear();
        brush.stroke_centers.clear();
        brush.footprint.add_cell(CellXZ::new(5, 5));
        // Single stamp center at cell (5,5) world position
        let (cwx, _, cwz) = CellXZ::new(5, 5).to_world(0.0, voxel_size);
        brush.stroke_centers.push((cwx, cwz));
        brush.footprint.base_y = 0.0;
        brush.footprint.height_delta = height_delta;

        let chunks = brush.apply_geometry(&noise, &existing_mods, &mut new_mods);
        assert!(!chunks.is_empty(), "Should affect at least one chunk");

        // Verify the SDF field has a zero-crossing near target_y.
        // At target_y, desired_sdf = 0 (surface). Check that modifications were
        // written at Y levels near target_y by sampling the combined SDF.
        let grid_x = 5.0 * voxel_size;
        let grid_z = 5.0 * voxel_size;

        // Find the surface in the modified field
        let y_search_max = height_delta + amplitude + 6.0;
        let y_search_min = -amplitude - 6.0;
        let surface_y = Brush::find_surface_y(
            &noise,
            &new_mods,
            grid_x,
            grid_z,
            y_search_min,
            y_search_max,
            voxel_size,
        );

        // The surface should be near base_y + height_delta (within a voxel)
        let expected_surface = height_delta; // base_y=0 + height_delta
        let error = (surface_y - expected_surface).abs();
        assert!(
            error < 2.0 * voxel_size,
            "Surface at {surface_y} should be near expected {expected_surface} (error={error})"
        );
    }

    #[test]
    fn test_elevation_no_fragments_above_target() {
        // Demonstrate that the elevation brush's y_range can be too small,
        // leaving unmodified noise above the brush zone that creates
        // floating fragment geometry.
        //
        // The brush computes:
        //   y_range = amplitude + |height_delta| + 4 * voxel_size
        //   y_max   = ceil(base_y + y_range)
        //
        // Critically, y_max depends on base_y (the Y where the user clicked),
        // NOT on floor_y. If base_y is below the terrain peaks, the brush
        // writes modifications only up to y_max, but at Y levels above y_max
        // the unmodified noise can still be solid (negative SDF). This creates
        // a spurious surface crossing: the last modified level has positive
        // SDF (air via desired_sdf = y - target_y) while the first unmodified
        // level reverts to negative noise SDF (solid terrain).
        //
        // With default noise (amplitude=32, floor_y=32, seed=42):
        //   Peak terrain surface ~= floor_y + amplitude * 0.82 ~= 58
        //   base_y = 10 (user clicked on a low wall or valley)
        //   y_max  = ceil(10 + 32 + 10 + 4) = 56
        //   At Y=57..58, no mods exist, but noise is solid -> fragment!
        //
        // This test should FAIL to prove the bug exists.

        let voxel_size: f32 = 1.0;
        let amplitude: f32 = 32.0;
        let floor_y: f32 = 32.0;
        let height_delta: f32 = 10.0;
        // base_y below the terrain peaks triggers the y_range gap.
        let base_y: f32 = 10.0;

        // Default terrain noise settings (no box bounds to avoid CSG effects).
        let noise = NoiseField::new(
            42, 4, 0.02, amplitude, 0.0, floor_y, None,
        );

        let y_range = amplitude + height_delta.abs() + 4.0 * voxel_size;
        let brush_y_max = (base_y + y_range).ceil();

        // Find columns where terrain is still solid at brush_y_max.
        // These are the columns where the bug manifests.
        let scan_extent = 320;
        let mut high_columns: Vec<CellXZ> = Vec::new();
        for cz in 0..scan_extent {
            for cx in 0..scan_extent {
                let gx = cx as f32 * voxel_size;
                let gz = cz as f32 * voxel_size;
                if noise.sample(gx, brush_y_max, gz) < 0.0 {
                    high_columns.push(CellXZ::new(cx, cz));
                }
            }
        }

        assert!(
            !high_columns.is_empty(),
            "Test precondition failed: no columns have terrain above brush y_max={brush_y_max}."
        );

        // Apply elevation brush to the high-terrain columns.
        let test_columns: Vec<CellXZ> = high_columns.iter().copied().take(100).collect();

        let existing_mods = ModificationLayer::new(32, voxel_size);
        let mut new_mods = ModificationLayer::new(32, voxel_size);

        let mut brush = Brush::new(voxel_size);
        brush.set_mode(BrushMode::Elevation);
        brush.set_shape(BrushShape::Square);
        brush.set_size(3.0);
        brush.set_strength(1.0);
        brush.set_feather(0.0);

        brush.footprint.clear();
        brush.stroke_centers.clear();
        brush.footprint.base_y = base_y;
        brush.footprint.height_delta = height_delta;
        for cell in &test_columns {
            brush.footprint.add_cell(*cell);
            let (wx, _, wz) = cell.to_world(0.0, voxel_size);
            brush.stroke_centers.push((wx, wz));
        }

        let chunks = brush.apply_geometry(&noise, &existing_mods, &mut new_mods);
        assert!(!chunks.is_empty(), "Should affect at least one chunk");

        let target_y = base_y + height_delta;

        // Scan every XZ column in the footprint for spurious surface crossings
        // above target_y. The invariant: once the combined SDF becomes positive
        // (air) above target_y, it must STAY positive all the way up. A sign
        // flip back to negative means a floating fragment surface.
        let mut fragment_columns: Vec<(i32, i32, f32, f32)> = Vec::new();
        let scan_max: f32 = floor_y + amplitude * 2.0;

        for cell in brush.footprint.iter() {
            let grid_x = cell.x as f32 * voxel_size;
            let grid_z = cell.z as f32 * voxel_size;

            let mut found_air = false;
            let mut y = target_y;

            while y <= scan_max {
                let sdf = noise.sample_with_mods(grid_x, y, grid_z, &new_mods);

                if sdf > 0.0 {
                    found_air = true;
                } else if found_air && sdf < 0.0 {
                    // Fragment: air-to-solid transition above target_y
                    fragment_columns.push((cell.x, cell.z, y, sdf));
                    break;
                }

                y += voxel_size;
            }
        }

        assert!(
            fragment_columns.is_empty(),
            "Found {} columns with fragment surfaces above target_y={target_y}.\n\
             Brush y_range={y_range} produces y_max={brush_y_max}, but terrain extends \n\
             above that. Unmodified noise above y_max creates spurious solid regions.\n\
             First few fragments (cell_x, cell_z, fragment_y, sdf): {:?}",
            fragment_columns.len(),
            &fragment_columns[..fragment_columns.len().min(10)]
        );
    }

    #[test]
    fn test_elevation_feathering_no_fragments() {
        // With feather > 0, the old code used y_falloff to reduce blend far from
        // the target surface, allowing unmodified noise to persist and create
        // floating fragment geometry. The fix uses interpolated target_y with
        // full blend so noise is always fully overridden.
        let voxel_size: f32 = 1.0;
        let amplitude: f32 = 32.0;
        let floor_y: f32 = 32.0;
        let height_delta: f32 = 10.0;
        let base_y: f32 = 10.0;

        let noise = NoiseField::new(42, 4, 0.02, amplitude, 0.0, floor_y, None);

        let existing_mods = ModificationLayer::new(32, voxel_size);
        let mut new_mods = ModificationLayer::new(32, voxel_size);

        let mut brush = Brush::new(voxel_size);
        brush.set_mode(BrushMode::Elevation);
        brush.set_shape(BrushShape::Round);
        brush.set_size(8.0);
        brush.set_strength(1.0);
        brush.set_feather(1.0); // Full feathering — the scenario that caused fragments

        // Build footprint centered at (50, 50)
        brush.footprint.clear();
        brush.stroke_centers.clear();
        brush.footprint.base_y = base_y;
        brush.footprint.height_delta = height_delta;
        let center = 50;
        // Single stamp center at (50, 50) world position
        let (sc_x, _, sc_z) = CellXZ::new(center, center).to_world(0.0, voxel_size);
        brush.stroke_centers.push((sc_x, sc_z));
        let radius_cells = (brush.size / voxel_size).ceil() as i32;
        let radius_sq = brush.size * brush.size;
        for dz in -radius_cells..=radius_cells {
            for dx in -radius_cells..=radius_cells {
                let dist_x = dx as f32 * voxel_size;
                let dist_z = dz as f32 * voxel_size;
                if dist_x * dist_x + dist_z * dist_z <= radius_sq {
                    brush
                        .footprint
                        .add_cell(CellXZ::new(center + dx, center + dz));
                }
            }
        }

        let chunks = brush.apply_geometry(&noise, &existing_mods, &mut new_mods);
        assert!(!chunks.is_empty(), "Should affect at least one chunk");

        let target_y = base_y + height_delta;
        let scan_max: f32 = floor_y + amplitude * 2.0;

        // Check every column: once SDF is positive (air) above target_y,
        // it must stay positive. A flip back to negative means a fragment.
        let mut fragment_columns: Vec<(i32, i32, f32, f32)> = Vec::new();

        for cell in brush.footprint.iter() {
            let grid_x = cell.x as f32 * voxel_size;
            let grid_z = cell.z as f32 * voxel_size;

            let mut found_air = false;
            let mut y = target_y;

            while y <= scan_max {
                let sdf = noise.sample_with_mods(grid_x, y, grid_z, &new_mods);

                if sdf > 0.0 {
                    found_air = true;
                } else if found_air && sdf < 0.0 {
                    fragment_columns.push((cell.x, cell.z, y, sdf));
                    break;
                }

                y += voxel_size;
            }
        }

        assert!(
            fragment_columns.is_empty(),
            "Found {} columns with fragment surfaces above target_y={target_y} with feather=1.0.\n\
             Feathering should interpolate target_y, not reduce blend.\n\
             First few fragments (cell_x, cell_z, fragment_y, sdf): {:?}",
            fragment_columns.len(),
            &fragment_columns[..fragment_columns.len().min(10)]
        );
    }

    #[test]
    fn test_xz_bounds_clamp_at_boundary() {
        let mut brush = Brush::new(1.0);
        brush.set_shape(BrushShape::Square);
        brush.set_size(1.0);
        brush.set_terrain_bounds(10, 10);

        // Stroke at the boundary edge — cells at x=10 and z=10 should be excluded
        brush.begin_stroke(9.5, 0.0, 9.5);
        for cell in brush.footprint.iter() {
            assert!(
                cell.x >= 0 && cell.x < 10,
                "Cell x={} should be in [0, 10)",
                cell.x
            );
            assert!(
                cell.z >= 0 && cell.z < 10,
                "Cell z={} should be in [0, 10)",
                cell.z
            );
        }
    }

    #[test]
    fn test_xz_bounds_clamp_negative() {
        let mut brush = Brush::new(1.0);
        brush.set_shape(BrushShape::Square);
        brush.set_size(2.0);
        brush.set_terrain_bounds(10, 10);

        // Stroke near origin — negative cells should be excluded
        brush.begin_stroke(0.5, 0.0, 0.5);
        for cell in brush.footprint.iter() {
            assert!(cell.x >= 0, "Cell x={} should be >= 0", cell.x);
            assert!(cell.z >= 0, "Cell z={} should be >= 0", cell.z);
        }
    }

    #[test]
    fn test_xz_bounds_fully_outside_produces_empty_footprint() {
        let mut brush = Brush::new(1.0);
        brush.set_shape(BrushShape::Square);
        brush.set_size(1.0);
        brush.set_terrain_bounds(10, 10);

        // Stroke completely outside the terrain
        brush.begin_stroke(20.0, 0.0, 20.0);
        assert!(
            brush.footprint.is_empty(),
            "Footprint should be empty when painting outside bounds, got {} cells",
            brush.footprint.len()
        );
    }

    #[test]
    fn test_xz_bounds_round_brush() {
        let mut brush = Brush::new(1.0);
        brush.set_shape(BrushShape::Round);
        brush.set_size(3.0);
        brush.set_terrain_bounds(10, 10);

        // Stroke near boundary with round brush
        brush.begin_stroke(9.5, 0.0, 9.5);
        for cell in brush.footprint.iter() {
            assert!(
                cell.x >= 0 && cell.x < 10,
                "Round brush cell x={} should be in [0, 10)",
                cell.x
            );
            assert!(
                cell.z >= 0 && cell.z < 10,
                "Round brush cell z={} should be in [0, 10)",
                cell.z
            );
        }
    }

    #[test]
    fn test_y_clamp_with_box_bounds() {
        // Create noise with box bounds from Y=0 to Y=100
        let noise = NoiseField::new(
            42,
            1,
            0.01,
            10.0,
            0.0,
            50.0,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
        );

        // Unclamped range extends well beyond [0, 100]
        let raw_min = -20.0;
        let raw_max = 120.0;
        let (clamped_min, clamped_max) = Brush::clamp_y_range(raw_min, raw_max, &noise);
        assert!(
            clamped_min >= 0.0,
            "Clamped y_min should be >= box_min[1]=0, got {}",
            clamped_min
        );
        assert!(
            clamped_max <= 100.0,
            "Clamped y_max should be <= box_max[1]=100, got {}",
            clamped_max
        );
    }

    #[test]
    fn test_y_clamp_no_box_bounds() {
        // Without box bounds, range should be unchanged
        let noise = NoiseField::new(42, 1, 0.01, 10.0, 0.0, 50.0, None);

        let (clamped_min, clamped_max) = Brush::clamp_y_range(-20.0, 120.0, &noise);
        assert_eq!(clamped_min, -20.0);
        assert_eq!(clamped_max, 120.0);
    }

    #[test]
    fn test_min_distance_single_center() {
        // With one center, min_distance_to_stroke degenerates to point distance
        let mut brush = Brush::new(1.0);
        brush.stroke_centers.push((10.0, 20.0));

        let dist = brush.min_distance_to_stroke(13.0, 24.0);
        let expected = ((3.0f32).powi(2) + (4.0f32).powi(2)).sqrt(); // 5.0
        assert!((dist - expected).abs() < 0.001, "dist={dist}, expected={expected}");
    }

    #[test]
    fn test_min_distance_to_segment() {
        let mut brush = Brush::new(1.0);
        // Horizontal segment from (0,0) to (10,0)
        brush.stroke_centers.push((0.0, 0.0));
        brush.stroke_centers.push((10.0, 0.0));

        // Perpendicular distance from (5, 3) to segment = 3.0
        let dist = brush.min_distance_to_stroke(5.0, 3.0);
        assert!((dist - 3.0).abs() < 0.001, "perpendicular dist={dist}");

        // Beyond the end: (12, 0) → distance to endpoint (10, 0) = 2.0
        let dist2 = brush.min_distance_to_stroke(12.0, 0.0);
        assert!((dist2 - 2.0).abs() < 0.001, "beyond-end dist={dist2}");

        // Before the start: (-3, 0) → distance to endpoint (0, 0) = 3.0
        let dist3 = brush.min_distance_to_stroke(-3.0, 0.0);
        assert!((dist3 - 3.0).abs() < 0.001, "before-start dist={dist3}");
    }

    #[test]
    fn test_stroke_falloff_uniform_along_path() {
        // Points directly on a long stroke path should all get falloff=1.0
        let mut brush = Brush::new(1.0);
        brush.size = 5.0;
        brush.set_feather(1.0);

        // Long horizontal stroke
        brush.stroke_centers.push((0.0, 0.0));
        brush.stroke_centers.push((50.0, 0.0));

        // Points on the path should have distance 0 → falloff 1.0
        for x in [0.0, 10.0, 25.0, 40.0, 50.0] {
            let falloff = brush.compute_stroke_falloff(x, 0.0);
            assert!(
                (falloff - 1.0).abs() < 0.001,
                "On-path point ({x}, 0) should have falloff=1.0, got {falloff}"
            );
        }

        // Points perpendicular at brush-size distance should have near-zero falloff
        for x in [10.0, 25.0, 40.0] {
            let falloff = brush.compute_stroke_falloff(x, 5.0);
            assert!(
                falloff < 0.01,
                "Edge-perpendicular point ({x}, 5) should have near-zero falloff, got {falloff}"
            );
        }
    }

    #[test]
    fn test_stroke_centers_cleared_on_begin() {
        let mut brush = Brush::new(1.0);
        brush.set_size(2.0);
        brush.set_shape(BrushShape::Square);

        // First stroke
        brush.begin_stroke(10.0, 5.0, 10.0);
        brush.continue_stroke(20.0, 5.0, 20.0);
        assert!(brush.stroke_centers.len() >= 2);

        // Cancel and begin new stroke — centers should be cleared
        brush.cancel();
        assert!(brush.stroke_centers.is_empty(), "cancel should clear stroke_centers");

        brush.begin_stroke(30.0, 5.0, 30.0);
        assert_eq!(brush.stroke_centers.len(), 1, "begin_stroke should start fresh");
    }

    #[test]
    fn test_elevation_ridge_not_dome() {
        // A long elevation stroke should produce uniform surface height along the
        // path, not a dome that tapers at the ends.
        let voxel_size = 1.0;
        let noise = NoiseField::new(42, 1, 0.01, 2.0, 0.0, 0.0, None);

        let existing_mods = ModificationLayer::new(32, voxel_size);
        let mut new_mods = ModificationLayer::new(32, voxel_size);

        let mut brush = Brush::new(voxel_size);
        brush.set_mode(BrushMode::Elevation);
        brush.set_shape(BrushShape::Round);
        brush.set_size(3.0);
        brush.set_strength(1.0);
        brush.set_feather(0.5);

        // Simulate a long horizontal drag from x=10 to x=40 at z=20
        brush.footprint.clear();
        brush.stroke_centers.clear();
        let z = 20;
        for x in 10..=40 {
            // Record a stamp center every few cells (simulating mouse movement)
            if x % 3 == 0 || x == 10 || x == 40 {
                let wx = x as f32 * voxel_size + voxel_size * 0.5;
                let wz = z as f32 * voxel_size + voxel_size * 0.5;
                brush.stroke_centers.push((wx, wz));
            }
            // Add cells within brush radius of the path
            let radius_cells = (brush.size / voxel_size).ceil() as i32;
            for dz in -radius_cells..=radius_cells {
                let dist_z = dz as f32 * voxel_size;
                if dist_z.abs() <= brush.size {
                    brush.footprint.add_cell(CellXZ::new(x, z + dz));
                }
            }
        }
        brush.footprint.base_y = 0.0;
        brush.footprint.height_delta = 8.0;

        let chunks = brush.apply_geometry(&noise, &existing_mods, &mut new_mods);
        assert!(!chunks.is_empty());

        // Sample surface heights along the center of the path (z=20)
        let y_search_min = -10.0;
        let y_search_max = 20.0;
        let mut surface_heights = Vec::new();
        for x in [12, 20, 25, 30, 38] {
            let gx = x as f32 * voxel_size;
            let gz = z as f32 * voxel_size;
            let sy = Brush::find_surface_y(
                &noise, &new_mods, gx, gz, y_search_min, y_search_max, voxel_size,
            );
            surface_heights.push((x, sy));
        }

        // All points along the center of the path should have similar heights.
        // With centroid-based falloff, end points would be much lower than the center.
        let min_h = surface_heights.iter().map(|(_, h)| *h).fold(f32::MAX, f32::min);
        let max_h = surface_heights.iter().map(|(_, h)| *h).fold(f32::MIN, f32::max);
        let range = max_h - min_h;
        assert!(
            range < 2.0 * voxel_size,
            "Surface heights along path center should be uniform.\n\
             Heights: {:?}\n\
             Range: {range} (should be < {})",
            surface_heights,
            2.0 * voxel_size,
        );
    }
}
