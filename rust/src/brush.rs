//! Brush system for terrain painting.
//!
//! Supports two modes:
//! - Geometry Mode: Two-phase painting (paint area → set height) for sculpting terrain
//! - Texture Mode: Paint different textures onto terrain surface

use std::collections::HashSet;

use crate::chunk::ChunkCoord;
use crate::terrain_modifications::{ModificationLayer, VoxelMod};
use crate::texture_layer::{TextureLayer, TextureWeights};

/// Brush operating mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BrushMode {
    /// Geometry sculpting mode: paint area, then adjust height
    #[default]
    Geometry,
    /// Texture painting mode: paint textures onto terrain surface
    Texture,
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

            // Add chunks at multiple Y levels (terrain can span multiple Y chunks)
            // For now, add a reasonable range around base_y
            let base_chunk_y = (self.base_y / chunk_size).floor() as i32;
            for dy in -1..=2 {
                chunks.insert(ChunkCoord::new(chunk_x, base_chunk_y + dy, chunk_z));
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
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            mode: BrushMode::Geometry,
            shape: BrushShape::Round,
            size: 5.0,
            strength: 1.0,
            phase: BrushPhase::Idle,
            footprint: BrushFootprint::new(),
            selected_texture: 0,
            height_adjust_start_y: 0.0,
            height_sensitivity: 0.1,
            voxel_size: 1.0,
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
            BrushMode::Geometry => {
                self.phase = BrushPhase::PaintingArea;
                self.add_cells_at(world_x, world_z);
            }
            BrushMode::Texture => {
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
                // Commit the texture paint
                self.phase = BrushPhase::Idle;
                BrushAction::CommitTexture
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

        let modification = if self.footprint.height_delta > 0.0 {
            // Raising terrain: negative SDF delta (adds material)
            VoxelMod::new(-self.footprint.height_delta, self.strength)
        } else {
            // Lowering terrain: positive SDF delta (removes material)
            VoxelMod::new(-self.footprint.height_delta, self.strength)
        };

        let base_y = self.footprint.base_y;
        let height_delta = self.footprint.height_delta;

        for cell in self.footprint.iter() {
            // Apply modification at multiple Y levels around the affected area
            let y_min = (base_y + height_delta.min(0.0) - self.voxel_size).floor();
            let y_max = (base_y + height_delta.max(0.0) + self.voxel_size).ceil();
            let mut y = y_min;
            while y <= y_max {
                let (wx, _, wz) = cell.to_world(0.0, self.voxel_size);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brush_default() {
        let brush = Brush::default();
        assert_eq!(brush.mode, BrushMode::Geometry);
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
}
