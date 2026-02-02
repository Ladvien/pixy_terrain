use transvoxel::extraction::extract_from_field;
use transvoxel::generic_mesh::GenericMeshBuilder;
use transvoxel::transition_sides::{TransitionSide, TransitionSides};
use transvoxel::voxel_source::{Block, BlockDims};

use crate::chunk::{ChunkCoord, MeshResult};
use crate::debug_log::{compute_normal_stats, debug_log};
use crate::noise_field::NoiseField;

/// Extract mesh for a single chunk
pub fn extract_chunk_mesh(
    noise: &NoiseField,
    coord: ChunkCoord,
    lod_level: u8,
    base_voxel_size: f32,
    chunk_size: f32,
    transition_sides: u8,
) -> MeshResult {
    let origin = coord.to_world_position(chunk_size);
    let voxel_size = base_voxel_size * (1 << lod_level) as f32;
    let subdivisions = (chunk_size / voxel_size) as usize;

    // Block defines hte world region to extract
    let block = Block {
        dims: BlockDims {
            base: [origin[0], origin[1], origin[2]],
            size: chunk_size,
        },
        subdivisions,
    };

    let transitions = transition_sides_from_u8(transition_sides);

    // Closure implements DataField automatically
    let mut field = |x: f32, y: f32, z: f32| -> f32 { noise.sample(x, y, z) };

    let builder = extract_from_field(
        &mut field,
        &block,
        0.0_f32,
        transitions,
        GenericMeshBuilder::new(),
    );

    let mesh = builder.build();

    let vertices: Vec<[f32; 3]> = mesh
        .positions
        .chunks(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();

    // Epsilon for near-zero length checks (consistent with mesh_postprocess.rs)
    const NORMAL_EPSILON: f32 = 1e-6;

    let normals: Vec<[f32; 3]> = mesh
        .normals
        .chunks(3)
        .map(|c| {
            let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            if len > NORMAL_EPSILON {
                [c[0] / len, c[1] / len, c[2] / len]
            } else {
                [0.0, 1.0, 0.0]
            }
        })
        .collect();

    let indices: Vec<i32> = mesh.triangle_indices.iter().map(|&i| i as i32).collect();

    let vert_count = vertices.len();
    let tri_count = indices.len() / 3;

    // Log normal statistics for debugging (especially for boundary chunks)
    if cfg!(debug_assertions) && !normals.is_empty() {
        let normal_stats = compute_normal_stats(&normals);
        if normal_stats.degenerate_count > 0 || coord.y == -1 || coord.x == -1 || coord.z == -1 {
            debug_log(&format!(
                "[extract_chunk_mesh] ({}, {}, {}) normals: min_len={:.3}, max_len={:.3}, degenerate={}",
                coord.x, coord.y, coord.z,
                normal_stats.min_len, normal_stats.max_len, normal_stats.degenerate_count
            ));
        }
    }

    // Log extraction results for floor chunks or empty meshes
    if coord.y == -1 || vert_count == 0 {
        debug_log(&format!(
            "[extract_chunk_mesh] ({}, {}, {}) LOD={}: origin=({:.1}, {:.1}, {:.1}), subdivs={}, verts={}, tris={}{}",
            coord.x, coord.y, coord.z, lod_level,
            origin[0], origin[1], origin[2], subdivisions,
            vert_count, tri_count,
            if vert_count == 0 { " [EMPTY]" } else { "" }
        ));
    }

    MeshResult {
        coord,
        lod_level,
        vertices,
        normals,
        indices,
        transition_sides,
    }
}

fn transition_sides_from_u8(flags: u8) -> TransitionSides {
    let mut sides = TransitionSides::empty();
    if flags & 0b000001 != 0 {
        sides |= TransitionSide::LowX;
    }
    if flags & 0b000010 != 0 {
        sides |= TransitionSide::HighX;
    }
    if flags & 0b000100 != 0 {
        sides |= TransitionSide::LowY;
    }
    if flags & 0b001000 != 0 {
        sides |= TransitionSide::HighY;
    }
    if flags & 0b010000 != 0 {
        sides |= TransitionSide::LowZ;
    }
    if flags & 0b100000 != 0 {
        sides |= TransitionSide::HighZ;
    }
    sides
}
