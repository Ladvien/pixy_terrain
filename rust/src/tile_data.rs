use std::collections::HashMap;

/// Represents a single tile in the 3D grid
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileData {
    /// Index into the tile atlas (0 = empty/air)
    pub tile_id: u16,
    /// Rotation around Y axis: 0=0°, 1=90°, 2=180°, 3=270°
    pub rotation: u8,
    /// Bit flags for flipping: bit 0 = flip X, bit 1 = flip Y, bit 2 = flip Z
    pub flip_flags: u8,
}

impl TileData {
    pub const FLIP_X: u8 = 0b001;
    pub const FLIP_Y: u8 = 0b010;
    pub const FLIP_Z: u8 = 0b100;

    pub fn new(tile_id: u16) -> Self {
        Self {
            tile_id,
            rotation: 0,
            flip_flags: 0,
        }
    }

    pub fn with_rotation(mut self, rotation: u8) -> Self {
        self.rotation = rotation % 4;
        self
    }

    pub fn with_flip(mut self, flip_flags: u8) -> Self {
        self.flip_flags = flip_flags & 0b111;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.tile_id == 0
    }

    pub fn flip_x(&self) -> bool {
        self.flip_flags & Self::FLIP_X != 0
    }

    pub fn flip_y(&self) -> bool {
        self.flip_flags & Self::FLIP_Y != 0
    }

    pub fn flip_z(&self) -> bool {
        self.flip_flags & Self::FLIP_Z != 0
    }
}

/// Grid position as a tuple for HashMap key
pub type GridPos = (i32, i32, i32);

/// Sparse 3D grid storing tiles using a HashMap
/// Only non-empty tiles are stored, making this efficient for sparse data
#[derive(Default)]
pub struct TileGrid {
    tiles: HashMap<GridPos, TileData>,
}

impl TileGrid {
    pub fn new() -> Self {
        Self {
            tiles: HashMap::new(),
        }
    }

    /// Set a tile at the given position
    /// If tile_id is 0, removes the tile (air/empty)
    pub fn set_tile(&mut self, x: i32, y: i32, z: i32, tile: TileData) {
        if tile.is_empty() {
            self.tiles.remove(&(x, y, z));
        } else {
            self.tiles.insert((x, y, z), tile);
        }
    }

    /// Get a tile at the given position
    /// Returns None if no tile exists (empty/air)
    pub fn get_tile(&self, x: i32, y: i32, z: i32) -> Option<&TileData> {
        self.tiles.get(&(x, y, z))
    }

    /// Remove a tile at the given position
    pub fn remove_tile(&mut self, x: i32, y: i32, z: i32) -> Option<TileData> {
        self.tiles.remove(&(x, y, z))
    }

    /// Clear all tiles
    pub fn clear(&mut self) {
        self.tiles.clear();
    }

    /// Get the number of non-empty tiles
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Check if a position has a tile
    pub fn has_tile(&self, x: i32, y: i32, z: i32) -> bool {
        self.tiles.contains_key(&(x, y, z))
    }

    /// Iterate over all tiles and their positions
    pub fn iter(&self) -> impl Iterator<Item = (&GridPos, &TileData)> {
        self.tiles.iter()
    }

    /// Get bounding box of all tiles: (min_pos, max_pos)
    /// Returns None if grid is empty
    pub fn bounds(&self) -> Option<(GridPos, GridPos)> {
        if self.tiles.is_empty() {
            return None;
        }

        let mut min = (i32::MAX, i32::MAX, i32::MAX);
        let mut max = (i32::MIN, i32::MIN, i32::MIN);

        for &(x, y, z) in self.tiles.keys() {
            min.0 = min.0.min(x);
            min.1 = min.1.min(y);
            min.2 = min.2.min(z);
            max.0 = max.0.max(x);
            max.1 = max.1.max(y);
            max.2 = max.2.max(z);
        }

        Some((min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_data_empty() {
        let tile = TileData::default();
        assert!(tile.is_empty());

        let tile = TileData::new(1);
        assert!(!tile.is_empty());
    }

    #[test]
    fn test_tile_data_rotation() {
        let tile = TileData::new(1).with_rotation(5); // 5 % 4 = 1
        assert_eq!(tile.rotation, 1);
    }

    #[test]
    fn test_tile_data_flip() {
        let tile = TileData::new(1).with_flip(TileData::FLIP_X | TileData::FLIP_Z);
        assert!(tile.flip_x());
        assert!(!tile.flip_y());
        assert!(tile.flip_z());
    }

    #[test]
    fn test_grid_set_get() {
        let mut grid = TileGrid::new();
        let tile = TileData::new(42);

        grid.set_tile(1, 2, 3, tile);
        assert_eq!(grid.get_tile(1, 2, 3), Some(&tile));
        assert_eq!(grid.get_tile(0, 0, 0), None);
    }

    #[test]
    fn test_grid_remove_on_empty() {
        let mut grid = TileGrid::new();
        grid.set_tile(1, 2, 3, TileData::new(42));
        assert_eq!(grid.tile_count(), 1);

        // Setting to empty tile removes it
        grid.set_tile(1, 2, 3, TileData::default());
        assert_eq!(grid.tile_count(), 0);
    }

    #[test]
    fn test_grid_bounds() {
        let mut grid = TileGrid::new();
        assert!(grid.bounds().is_none());

        grid.set_tile(-5, 0, 10, TileData::new(1));
        grid.set_tile(3, 7, -2, TileData::new(2));

        let (min, max) = grid.bounds().unwrap();
        assert_eq!(min, (-5, 0, -2));
        assert_eq!(max, (3, 7, 10));
    }
}
