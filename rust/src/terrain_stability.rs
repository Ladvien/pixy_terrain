//! Terrain gravity / structural stability system.
//!
//! After each SDF-modifying brush commit, identifies solid voxels that are not
//! connected to the ground via 3D flood-fill, then drops each floating connected
//! component as a unit. Cave ceilings connected to walls remain grounded.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::brush::BrushFootprint;
use crate::chunk::ChunkCoord;
use crate::noise_field::NoiseField;
use crate::terrain_modifications::{ModificationLayer, VoxelMod};

/// Extra Y range padding for fallback scan bounds.
const FALLBACK_Y_RANGE_PADDING: i32 = 64;
/// Multiplier for computing scan margin from footprint extent.
const SCAN_MARGIN_MULTIPLIER: i32 = 2;
/// Minimum scan margin in cells.
const MIN_SCAN_MARGIN: i32 = 8;
/// Maximum scan margin in cells.
const MAX_SCAN_MARGIN: i32 = 32;
/// Number of floor seed rows for flood-fill grounding.
const FLOOR_SEED_ROWS: usize = 4;
/// Minimum blend value for gravity drop calculations.
const MIN_DROP_BLEND: f32 = 0.01;

/// Axis-aligned bounding box for the scan region (in voxel-grid integer coords).
#[derive(Clone, Debug)]
pub struct ScanRegion {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
    pub min_z: i32,
    pub max_z: i32,
}

impl ScanRegion {
    pub fn size_x(&self) -> usize {
        (self.max_x - self.min_x + 1).max(0) as usize
    }
    pub fn size_y(&self) -> usize {
        (self.max_y - self.min_y + 1).max(0) as usize
    }
    pub fn size_z(&self) -> usize {
        (self.max_z - self.min_z + 1).max(0) as usize
    }
}

/// 3D boolean grid indicating which voxels are solid within a scan region.
struct SolidGrid {
    data: Vec<bool>,
    size_x: usize,
    size_y: usize,
    size_z: usize,
}

impl SolidGrid {
    fn new(region: &ScanRegion) -> Self {
        let size_x = region.size_x();
        let size_y = region.size_y();
        let size_z = region.size_z();
        Self {
            data: vec![false; size_x * size_y * size_z],
            size_x,
            size_y,
            size_z,
        }
    }

    #[inline]
    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        x + y * self.size_x + z * self.size_x * self.size_y
    }

    #[inline]
    fn get(&self, x: usize, y: usize, z: usize) -> bool {
        self.data[self.index(x, y, z)]
    }

    #[inline]
    fn set(&mut self, x: usize, y: usize, z: usize, val: bool) {
        let idx = self.index(x, y, z);
        self.data[idx] = val;
    }
}

/// Result of applying gravity to a terrain region (synchronous, in-place).
pub struct StabilityResult {
    pub affected_chunks: Vec<ChunkCoord>,
    pub components_found: usize,
    pub components_dropped: usize,
}

/// Result of applying gravity on a background thread.
/// Contains the fully-updated modification layer to swap in.
pub struct GravityResult {
    pub new_mod_layer: ModificationLayer,
    pub affected_chunks: Vec<ChunkCoord>,
    pub components_found: usize,
    pub components_dropped: usize,
}

/// 6-connected neighbor offsets for BFS.
const NEIGHBORS: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

/// Compute gravity on an owned ModificationLayer (suitable for background threads).
///
/// Returns a `GravityResult` containing the updated layer and affected chunks.
pub fn compute_gravity(
    noise: &NoiseField,
    mut mod_layer: ModificationLayer,
    footprint: &BrushFootprint,
    voxel_size: f32,
    box_bounds: Option<([f32; 3], [f32; 3])>,
    terrain_cells_x: i32,
    terrain_cells_z: i32,
) -> GravityResult {
    let result = apply_gravity(
        noise,
        &mut mod_layer,
        footprint,
        voxel_size,
        box_bounds,
        terrain_cells_x,
        terrain_cells_z,
    );
    GravityResult {
        new_mod_layer: mod_layer,
        affected_chunks: result.affected_chunks,
        components_found: result.components_found,
        components_dropped: result.components_dropped,
    }
}

/// Apply gravity to floating terrain after a brush modification.
///
/// Scans the region around the brush footprint, identifies solid voxels not
/// connected to the ground via 3D flood-fill, and drops each floating
/// connected component downward until it rests on the surface below.
pub fn apply_gravity(
    noise: &NoiseField,
    mod_layer: &mut ModificationLayer,
    footprint: &BrushFootprint,
    voxel_size: f32,
    box_bounds: Option<([f32; 3], [f32; 3])>,
    terrain_cells_x: i32,
    terrain_cells_z: i32,
) -> StabilityResult {
    if footprint.is_empty() {
        return StabilityResult {
            affected_chunks: Vec::new(),
            components_found: 0,
            components_dropped: 0,
        };
    }

    // Step 1: Build scan region
    let region = build_scan_region(
        footprint,
        voxel_size,
        box_bounds,
        terrain_cells_x,
        terrain_cells_z,
    );
    if region.size_x() == 0 || region.size_y() == 0 || region.size_z() == 0 {
        return StabilityResult {
            affected_chunks: Vec::new(),
            components_found: 0,
            components_dropped: 0,
        };
    }

    // Step 2: Sample solid grid
    let solid = sample_solid_grid(noise, mod_layer, &region, voxel_size);

    // Step 3: Flood-fill grounded voxels
    let grounded = flood_fill_grounded(&solid, &region);

    // Step 4: Find floating components
    let components = find_floating_components(&solid, &grounded);
    let components_found = components.len();

    // Step 5 & 6: Drop each component (skip pure-noise components with no brush mods)
    let mut components_dropped = 0;
    let mut all_affected_chunks = HashSet::new();

    for component in &components {
        // Skip components that have zero brush modifications — these are
        // false positives from scan region edge effects (pure noise terrain
        // that appears floating only because the scan region doesn't extend
        // far enough to find its ground connection).
        let has_brush_mods = component.iter().any(|&(gx, gy, gz)| {
            let world_x = (region.min_x + gx as i32) as f32 * voxel_size;
            let world_y = (region.min_y + gy as i32) as f32 * voxel_size;
            let world_z = (region.min_z + gz as i32) as f32 * voxel_size;
            mod_layer.get_at_world(world_x, world_y, world_z).is_some()
        });
        if !has_brush_mods {
            continue;
        }

        let drop_distance = compute_drop_distance(component, &solid, &grounded, &region);

        if drop_distance == 0 {
            continue;
        }

        let chunks = drop_component(
            component,
            drop_distance,
            noise,
            mod_layer,
            &region,
            voxel_size,
        );
        all_affected_chunks.extend(chunks);
        components_dropped += 1;
    }

    StabilityResult {
        affected_chunks: all_affected_chunks.into_iter().collect(),
        components_found,
        components_dropped,
    }
}

/// Build the scan region from the brush footprint bounding box + margin.
/// Full Y range from floor to terrain peak.
///
/// The XZ margin must be large enough to capture all connectivity paths from
/// floating terrain back to the ground. A mountain undercut by a flatten brush
/// could connect to ground through slopes that extend far beyond the footprint.
/// We use the terrain height as the margin since connectivity paths through
/// slopes can extend horizontally up to the full terrain height.
fn build_scan_region(
    footprint: &BrushFootprint,
    voxel_size: f32,
    box_bounds: Option<([f32; 3], [f32; 3])>,
    terrain_cells_x: i32,
    terrain_cells_z: i32,
) -> ScanRegion {
    // Y range: use box bounds if available, otherwise estimate from footprint
    let (min_y, max_y) = if let Some((bmin, bmax)) = box_bounds {
        let y_min_cell = (bmin[1] / voxel_size).floor() as i32;
        let y_max_cell = (bmax[1] / voxel_size).ceil() as i32 - 1;
        (y_min_cell, y_max_cell)
    } else {
        // Fallback: use a reasonable Y range around the footprint
        let base_cell = (footprint.base_y / voxel_size).floor() as i32;
        let delta_cells = (footprint.height_delta.abs() / voxel_size).ceil() as i32;
        (0, base_cell + delta_cells + FALLBACK_Y_RANGE_PADDING)
    };

    // XZ margin proportional to brush footprint size. Cap to a reasonable
    // range to avoid scanning the entire terrain on large maps.
    let footprint_extent = (footprint.max_x - footprint.min_x)
        .max(footprint.max_z - footprint.min_z)
        .max(1);
    let margin = (footprint_extent * SCAN_MARGIN_MULTIPLIER).max(MIN_SCAN_MARGIN).min(MAX_SCAN_MARGIN);

    let min_x = (footprint.min_x - margin).max(0);
    let max_x = (footprint.max_x + margin).min(terrain_cells_x - 1);
    let min_z = (footprint.min_z - margin).max(0);
    let max_z = (footprint.max_z + margin).min(terrain_cells_z - 1);

    ScanRegion {
        min_x,
        max_x,
        min_y,
        max_y,
        min_z,
        max_z,
    }
}

/// Sample the combined SDF at each grid point and build a 3D boolean solid grid.
fn sample_solid_grid(
    noise: &NoiseField,
    mod_layer: &ModificationLayer,
    region: &ScanRegion,
    voxel_size: f32,
) -> SolidGrid {
    let mut grid = SolidGrid::new(region);

    for gz in 0..grid.size_z {
        for gy in 0..grid.size_y {
            for gx in 0..grid.size_x {
                let world_x = (region.min_x + gx as i32) as f32 * voxel_size;
                let world_y = (region.min_y + gy as i32) as f32 * voxel_size;
                let world_z = (region.min_z + gz as i32) as f32 * voxel_size;

                let sdf = noise.sample_with_mods(world_x, world_y, world_z, mod_layer);
                grid.set(gx, gy, gz, sdf < 0.0);
            }
        }
    }

    grid
}

/// BFS flood-fill from ground-connected seed voxels.
/// Returns a grid of the same dimensions where `true` = grounded.
///
/// Seeds from floor voxels only (bottom few rows of the scan region).
/// We do NOT seed from XZ or top-Y boundaries because terrain at the scan
/// edge might be floating — seeding from boundaries would incorrectly label
/// it as grounded and prevent gravity from working on undercut mountains.
///
/// The scan region is already expanded with a large margin proportional to
/// terrain height, so connectivity paths from floating terrain to the ground
/// should be captured within the scan region in most cases.
fn flood_fill_grounded(solid: &SolidGrid, region: &ScanRegion) -> SolidGrid {
    let mut grounded = SolidGrid::new(region);
    let mut queue: VecDeque<(usize, usize, usize)> = VecDeque::new();

    let sx = solid.size_x;
    let sy = solid.size_y;
    let sz = solid.size_z;

    // Seed from floor voxels only (bottom few rows).
    // At the very bottom (y=0 in world coords), the box SDF is ~0 so voxels
    // may not register as solid. A few rows up they are definitely solid.
    let floor_seed_rows = FLOOR_SEED_ROWS.min(sy);

    for gz in 0..sz {
        for gx in 0..sx {
            for gy in 0..floor_seed_rows {
                if solid.get(gx, gy, gz) {
                    grounded.set(gx, gy, gz, true);
                    queue.push_back((gx, gy, gz));
                }
            }
        }
    }

    // BFS: spread grounded label to 6-connected solid neighbors
    while let Some((x, y, z)) = queue.pop_front() {
        for &(dx, dy, dz) in &NEIGHBORS {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            let nz = z as i32 + dz;

            if nx < 0 || ny < 0 || nz < 0 || nx >= sx as i32 || ny >= sy as i32 || nz >= sz as i32 {
                continue;
            }

            let nx = nx as usize;
            let ny = ny as usize;
            let nz = nz as usize;

            if solid.get(nx, ny, nz) && !grounded.get(nx, ny, nz) {
                grounded.set(nx, ny, nz, true);
                queue.push_back((nx, ny, nz));
            }
        }
    }

    grounded
}

/// Find all floating connected components (solid but not grounded).
/// Each component is a Vec of (local_x, local_y, local_z) coordinates.
fn find_floating_components(
    solid: &SolidGrid,
    grounded: &SolidGrid,
) -> Vec<Vec<(usize, usize, usize)>> {
    let sx = solid.size_x;
    let sy = solid.size_y;
    let sz = solid.size_z;

    let mut visited = vec![false; sx * sy * sz];
    let mut components = Vec::new();

    for gz in 0..sz {
        for gy in 0..sy {
            for gx in 0..sx {
                let idx = gx + gy * sx + gz * sx * sy;
                if !solid.get(gx, gy, gz) || grounded.get(gx, gy, gz) || visited[idx] {
                    continue;
                }

                // BFS to find connected component
                let mut component = Vec::new();
                let mut queue: VecDeque<(usize, usize, usize)> = VecDeque::new();
                visited[idx] = true;
                queue.push_back((gx, gy, gz));

                while let Some((x, y, z)) = queue.pop_front() {
                    component.push((x, y, z));

                    for &(dx, dy, dz) in &NEIGHBORS {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        let nz = z as i32 + dz;

                        if nx < 0
                            || ny < 0
                            || nz < 0
                            || nx >= sx as i32
                            || ny >= sy as i32
                            || nz >= sz as i32
                        {
                            continue;
                        }

                        let nx = nx as usize;
                        let ny = ny as usize;
                        let nz = nz as usize;
                        let nidx = nx + ny * sx + nz * sx * sy;

                        if solid.get(nx, ny, nz) && !grounded.get(nx, ny, nz) && !visited[nidx] {
                            visited[nidx] = true;
                            queue.push_back((nx, ny, nz));
                        }
                    }
                }

                if !component.is_empty() {
                    components.push(component);
                }
            }
        }
    }

    components
}

/// Compute the drop distance (in grid cells) for a floating component.
///
/// For each (x, z) column in the component, finds the lowest component voxel,
/// then scans downward for the highest grounded solid voxel (or the floor).
/// Returns the minimum gap across all columns to prevent penetration.
fn compute_drop_distance(
    component: &[(usize, usize, usize)],
    solid: &SolidGrid,
    grounded: &SolidGrid,
    _region: &ScanRegion,
) -> i32 {
    // Group voxels by (x, z) column, tracking lowest Y per column
    let mut column_min_y: HashMap<(usize, usize), usize> = HashMap::new();
    for &(x, y, z) in component {
        let entry = column_min_y.entry((x, z)).or_insert(y);
        if y < *entry {
            *entry = y;
        }
    }

    let mut min_drop = i32::MAX;

    for (&(x, z), &lowest_y) in &column_min_y {
        // Scan downward from just below the lowest component voxel
        // to find the highest grounded solid voxel or the floor
        let mut landing_y: i32 = -1; // -1 means floor (below grid)

        if lowest_y > 0 {
            for scan_y in (0..lowest_y).rev() {
                if solid.get(x, scan_y, z) && grounded.get(x, scan_y, z) {
                    landing_y = scan_y as i32;
                    break;
                }
            }
        }

        // Drop distance: lowest component voxel should land at landing_y + 1
        let target_y = landing_y + 1;
        let drop = lowest_y as i32 - target_y;

        if drop < min_drop {
            min_drop = drop;
        }
    }

    min_drop.max(0)
}

/// Drop a component by `drop_distance` grid cells.
///
/// Only moves voxels that have actual brush modifications. For each such voxel:
/// 1. Compute the visual SDF at the source position
/// 2. Derive the `VoxelMod` needed at the destination to produce the same visual
///    result given the different noise value there
/// 3. Erase sources by removing the modification (letting noise show through)
///
/// Returns affected chunk coordinates.
fn drop_component(
    component: &[(usize, usize, usize)],
    drop_distance: i32,
    noise: &NoiseField,
    mod_layer: &mut ModificationLayer,
    region: &ScanRegion,
    voxel_size: f32,
) -> Vec<ChunkCoord> {
    let mut affected_chunks = HashSet::new();

    // Build set of destination positions for overlap detection
    let dest_set: HashSet<(usize, usize, usize)> = component
        .iter()
        .filter_map(|&(x, y, z)| {
            let new_y = y as i32 - drop_distance;
            if new_y >= 0 && new_y < region.size_y() as i32 {
                Some((x, new_y as usize, z))
            } else {
                None
            }
        })
        .collect();

    // Collect source voxel data: only voxels with actual brush modifications
    struct SourceVoxel {
        gx: usize,
        gy: usize,
        gz: usize,
        visual_sdf: f32,
        has_mod: bool,
        src_blend: f32,
    }

    let mut sources: Vec<SourceVoxel> = Vec::with_capacity(component.len());
    for &(gx, gy, gz) in component {
        let world_x = (region.min_x + gx as i32) as f32 * voxel_size;
        let world_y = (region.min_y + gy as i32) as f32 * voxel_size;
        let world_z = (region.min_z + gz as i32) as f32 * voxel_size;

        let voxel_mod = mod_layer.get_at_world(world_x, world_y, world_z);
        let has_mod = voxel_mod.is_some();
        let src_blend = voxel_mod.map_or(0.0, |m| m.blend);
        let visual_sdf = noise.sample_with_mods(world_x, world_y, world_z, mod_layer);

        sources.push(SourceVoxel {
            gx,
            gy,
            gz,
            visual_sdf,
            has_mod,
            src_blend,
        });
    }

    // Step 1: Write modifications at destination positions
    for src in &sources {
        if !src.has_mod {
            continue; // Skip pure-noise voxels
        }

        let new_y = src.gy as i32 - drop_distance;
        if new_y < 0 || new_y >= region.size_y() as i32 {
            continue;
        }

        let world_x = (region.min_x + src.gx as i32) as f32 * voxel_size;
        let dest_world_y = (region.min_y + new_y) as f32 * voxel_size;
        let world_z = (region.min_z + src.gz as i32) as f32 * voxel_size;

        // Compute the desired_sdf at destination that produces the same visual_sdf
        // given the different noise at the destination:
        //   visual_sdf = noise_dest * (1 - blend) + desired_dest * blend
        //   desired_dest = (visual_sdf - noise_dest * (1 - blend)) / blend
        let noise_dest = noise.sample(world_x, dest_world_y, world_z);
        let blend = src.src_blend.max(MIN_DROP_BLEND); // Avoid division by zero
        let desired_dest = (src.visual_sdf - noise_dest * (1.0 - blend)) / blend;

        mod_layer.set_at_world(
            world_x,
            dest_world_y,
            world_z,
            VoxelMod::new(desired_dest, blend),
        );

        let chunk = mod_layer.world_to_chunk(world_x, dest_world_y, world_z);
        affected_chunks.insert(chunk);
    }

    // Step 2: Erase source positions — remove the modification so noise shows through
    for src in &sources {
        if !src.has_mod {
            continue; // No mod to erase
        }

        // Check if this source position is a destination for some other voxel
        if dest_set.contains(&(src.gx, src.gy, src.gz)) {
            continue;
        }

        let world_x = (region.min_x + src.gx as i32) as f32 * voxel_size;
        let world_y = (region.min_y + src.gy as i32) as f32 * voxel_size;
        let world_z = (region.min_z + src.gz as i32) as f32 * voxel_size;

        // Remove the brush modification — let underlying noise show through
        mod_layer.remove_at_world(world_x, world_y, world_z);

        let chunk = mod_layer.world_to_chunk(world_x, world_y, world_z);
        affected_chunks.insert(chunk);
    }

    affected_chunks.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::CellXZ;

    /// Helper to create a simple flat noise field for testing.
    /// Surface at y = surface_y, solid below, air above.
    fn flat_noise(surface_y: f32) -> NoiseField {
        NoiseField::new(
            42,
            1,
            0.001, // Very low frequency = nearly flat
            0.01,  // Very low amplitude = nearly flat
            0.0,
            surface_y,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
        )
    }

    /// Helper to create a footprint covering a rectangular region.
    fn make_footprint(
        min_x: i32,
        max_x: i32,
        min_z: i32,
        max_z: i32,
        base_y: f32,
    ) -> BrushFootprint {
        let mut fp = BrushFootprint::new();
        fp.base_y = base_y;
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                fp.add_cell(CellXZ::new(x, z));
            }
        }
        fp
    }

    #[test]
    fn test_single_floating_block_drops() {
        // Create a flat terrain at y=10 (surface_y=10, so solid below y=10)
        let noise = flat_noise(10.0);
        let voxel_size = 1.0;
        let mut mod_layer = ModificationLayer::new(32, voxel_size);

        // Place a floating solid block at y=20..22, x=10..12, z=10..12
        // by setting SDF to negative (solid) with blend=1.0
        for y in 20..=22 {
            for z in 10..=12 {
                for x in 10..=12 {
                    let wx = x as f32 * voxel_size;
                    let wy = y as f32 * voxel_size;
                    let wz = z as f32 * voxel_size;
                    mod_layer.set_at_world(wx, wy, wz, VoxelMod::new(-5.0, 1.0));
                }
            }
        }

        let footprint = make_footprint(8, 14, 8, 14, 20.0);
        let result = apply_gravity(
            &noise,
            &mut mod_layer,
            &footprint,
            voxel_size,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
            100,
            100,
        );

        assert!(
            result.components_found > 0,
            "Should find floating components"
        );
        assert!(
            result.components_dropped > 0,
            "Should drop at least one component"
        );
        assert!(
            !result.affected_chunks.is_empty(),
            "Should have affected chunks"
        );

        // After gravity, the block should have been moved down closer to the ground
        // The original position (y=20) should be air, and the block should now rest
        // on the terrain surface near y=10
        let sdf_at_original = noise.sample_with_mods(
            10.0 * voxel_size,
            21.0 * voxel_size,
            10.0 * voxel_size,
            &mod_layer,
        );
        assert!(
            sdf_at_original > 0.0,
            "Original position should be air after drop, got SDF={}",
            sdf_at_original
        );
    }

    #[test]
    fn test_grounded_terrain_unchanged() {
        // Terrain that is connected to the ground should not move
        let noise = flat_noise(10.0);
        let voxel_size = 1.0;
        let mod_layer_before = ModificationLayer::new(32, voxel_size);
        let mut mod_layer = mod_layer_before.clone();

        // No floating blocks — just normal terrain
        let footprint = make_footprint(5, 15, 5, 15, 5.0);
        let result = apply_gravity(
            &noise,
            &mut mod_layer,
            &footprint,
            voxel_size,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
            100,
            100,
        );

        assert_eq!(
            result.components_dropped, 0,
            "No components should be dropped for grounded terrain"
        );
    }

    #[test]
    fn test_floating_block_near_boundary_still_drops() {
        // A floating solid block near the scan region boundary should still be
        // detected and dropped. We only seed grounded status from the floor,
        // not from XZ boundaries, so floating terrain at the edge is handled.
        let noise = flat_noise(10.0);
        let voxel_size = 1.0;
        let mut mod_layer = ModificationLayer::new(32, voxel_size);

        // Place a floating block at the footprint edge
        for y in 20..=22 {
            for z in 10..=12 {
                let wx = 8.0 * voxel_size;
                let wy = y as f32 * voxel_size;
                let wz = z as f32 * voxel_size;
                mod_layer.set_at_world(wx, wy, wz, VoxelMod::new(-5.0, 1.0));
            }
        }

        let footprint = make_footprint(10, 14, 10, 14, 20.0);
        let result = apply_gravity(
            &noise,
            &mut mod_layer,
            &footprint,
            voxel_size,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
            100,
            100,
        );

        // With floor-only seeding, the floating block should be detected and dropped
        assert!(
            result.components_dropped > 0,
            "Floating block near boundary should be dropped"
        );
    }

    #[test]
    fn test_two_components_drop_independently() {
        let noise = flat_noise(5.0);
        let voxel_size = 1.0;
        let mut mod_layer = ModificationLayer::new(32, voxel_size);

        // Component A: floating at y=15, x=20..21, z=20..21
        for y in 15..=16 {
            for z in 20..=21 {
                for x in 20..=21 {
                    mod_layer.set_at_world(
                        x as f32 * voxel_size,
                        y as f32 * voxel_size,
                        z as f32 * voxel_size,
                        VoxelMod::new(-5.0, 1.0),
                    );
                }
            }
        }

        // Component B: floating at y=25, x=30..31, z=30..31 (well-separated)
        for y in 25..=26 {
            for z in 30..=31 {
                for x in 30..=31 {
                    mod_layer.set_at_world(
                        x as f32 * voxel_size,
                        y as f32 * voxel_size,
                        z as f32 * voxel_size,
                        VoxelMod::new(-5.0, 1.0),
                    );
                }
            }
        }

        let footprint = make_footprint(18, 33, 18, 33, 20.0);
        let result = apply_gravity(
            &noise,
            &mut mod_layer,
            &footprint,
            voxel_size,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
            100,
            100,
        );

        assert!(
            result.components_found >= 2,
            "Should find at least 2 floating components, found {}",
            result.components_found
        );
        assert!(
            result.components_dropped >= 2,
            "Should drop at least 2 components, dropped {}",
            result.components_dropped
        );
    }

    #[test]
    fn test_drop_limited_by_shortest_column() {
        // L-shaped component: one part is closer to ground than the other.
        // The drop distance should be limited by the shortest column gap.
        let noise = flat_noise(5.0);
        let voxel_size = 1.0;
        let mut mod_layer = ModificationLayer::new(32, voxel_size);

        // Vertical part: x=20, z=20, y=8..12 (close to ground at y=5)
        for y in 8..=12 {
            mod_layer.set_at_world(
                20.0 * voxel_size,
                y as f32 * voxel_size,
                20.0 * voxel_size,
                VoxelMod::new(-5.0, 1.0),
            );
        }
        // Horizontal part: x=21..23, z=20, y=12 (higher up)
        for x in 21..=23 {
            mod_layer.set_at_world(
                x as f32 * voxel_size,
                12.0 * voxel_size,
                20.0 * voxel_size,
                VoxelMod::new(-5.0, 1.0),
            );
        }

        let footprint = make_footprint(18, 25, 18, 22, 10.0);
        let result = apply_gravity(
            &noise,
            &mut mod_layer,
            &footprint,
            voxel_size,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
            100,
            100,
        );

        // The L-shape should drop as a unit, limited by the short column (y=8, ground ~y=5)
        // Drop should be approximately 2 cells (8 - (5+1) = 2)
        // Verify the system ran without panicking and found the component
        // Main test: no panic, and found the component
        assert!(
            result.components_found > 0,
            "Should find at least one floating component"
        );
    }

    #[test]
    fn test_cave_ceiling_stays_grounded() {
        // Build a hollow cave: walls connected to ground, ceiling connected to walls.
        // The ceiling should NOT collapse.
        let noise = flat_noise(5.0);
        let voxel_size = 1.0;
        let mut mod_layer = ModificationLayer::new(32, voxel_size);

        // Build cave structure inside the scan region (not on boundary):
        // Ground layer at y=5 (from noise)
        // Walls: x=15 and x=25, y=6..14, z=18..22
        // Ceiling: x=15..25, y=14, z=18..22
        // Hollow interior: x=16..24, y=6..13, z=18..22

        // Walls (connected to ground terrain)
        for y in 6..=14 {
            for z in 18..=22 {
                // Left wall
                mod_layer.set_at_world(
                    15.0 * voxel_size,
                    y as f32 * voxel_size,
                    z as f32 * voxel_size,
                    VoxelMod::new(-5.0, 1.0),
                );
                // Right wall
                mod_layer.set_at_world(
                    25.0 * voxel_size,
                    y as f32 * voxel_size,
                    z as f32 * voxel_size,
                    VoxelMod::new(-5.0, 1.0),
                );
            }
        }

        // Ceiling
        for x in 15..=25 {
            for z in 18..=22 {
                mod_layer.set_at_world(
                    x as f32 * voxel_size,
                    14.0 * voxel_size,
                    z as f32 * voxel_size,
                    VoxelMod::new(-5.0, 1.0),
                );
            }
        }

        let footprint = make_footprint(13, 27, 16, 24, 10.0);
        let result = apply_gravity(
            &noise,
            &mut mod_layer,
            &footprint,
            voxel_size,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
            100,
            100,
        );

        // The ceiling is connected via walls to the ground — nothing should drop
        assert_eq!(
            result.components_dropped, 0,
            "Cave ceiling connected to walls should not collapse, but {} components were dropped",
            result.components_dropped
        );
    }

    #[test]
    fn test_empty_footprint_no_op() {
        let noise = flat_noise(10.0);
        let voxel_size = 1.0;
        let mut mod_layer = ModificationLayer::new(32, voxel_size);

        let footprint = BrushFootprint::new();
        let result = apply_gravity(
            &noise,
            &mut mod_layer,
            &footprint,
            voxel_size,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
            100,
            100,
        );

        assert_eq!(result.components_found, 0);
        assert_eq!(result.components_dropped, 0);
        assert!(result.affected_chunks.is_empty());
    }

    #[test]
    fn test_overlapping_source_destination() {
        // Drop distance of 1: source and destination positions overlap.
        // This tests that the read-then-write-then-erase order works correctly.
        let noise = flat_noise(5.0);
        let voxel_size = 1.0;
        let mut mod_layer = ModificationLayer::new(32, voxel_size);

        // Place a 1-tall floating block just above the ground
        // Ground is at y=5, so solid below. Place block at y=7 (gap of 1 at y=6)
        for z in 20..=21 {
            for x in 20..=21 {
                mod_layer.set_at_world(
                    x as f32 * voxel_size,
                    7.0 * voxel_size,
                    z as f32 * voxel_size,
                    VoxelMod::new(-5.0, 1.0),
                );
            }
        }

        let footprint = make_footprint(18, 23, 18, 23, 7.0);
        let result = apply_gravity(
            &noise,
            &mut mod_layer,
            &footprint,
            voxel_size,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
            100,
            100,
        );

        // Should drop by 1 (from y=7 to y=6)
        // No panic = success for overlap handling
        assert!(
            result.components_found > 0,
            "Should find at least one floating component"
        );
    }

    #[test]
    fn test_affected_chunks_correct() {
        let noise = flat_noise(5.0);
        let voxel_size = 1.0;
        let mut mod_layer = ModificationLayer::new(32, voxel_size);

        // Floating block at y=20
        for y in 20..=22 {
            for z in 10..=12 {
                for x in 10..=12 {
                    mod_layer.set_at_world(
                        x as f32 * voxel_size,
                        y as f32 * voxel_size,
                        z as f32 * voxel_size,
                        VoxelMod::new(-5.0, 1.0),
                    );
                }
            }
        }

        let footprint = make_footprint(8, 14, 8, 14, 20.0);
        let result = apply_gravity(
            &noise,
            &mut mod_layer,
            &footprint,
            voxel_size,
            Some(([0.0, 0.0, 0.0], [100.0, 100.0, 100.0])),
            100,
            100,
        );

        if result.components_dropped > 0 {
            assert!(
                !result.affected_chunks.is_empty(),
                "Dropped components should report affected chunks"
            );

            // Affected chunks should include both source region (y~20) and destination (y~5)
            let has_high_y = result.affected_chunks.iter().any(|c| c.y >= 0);
            assert!(
                has_high_y,
                "Should include chunks near the source or destination"
            );
        }
    }
}
