use std::collections::HashMap;

use godot::prelude::*;

use super::types::CellGeometry;

/// Result of validating a single cell's geometry for watertightness.
pub struct ValidationResult {
    pub open_edges: Vec<(Vector3, Vector3)>,
    pub is_watertight: bool,
}

/// Canonical edge key using bit-exact float hashing.
/// Edges are ordered so (A,B) == (B,A).
#[derive(Hash, Eq, PartialEq)]
struct EdgeKey([u32; 6]); // [x1,y1,z1, x2,y2,z2] with v1 < v2 lexicographically

fn f32_to_bits(v: f32) -> u32 {
    v.to_bits()
}

fn vertex_to_bits(v: Vector3) -> [u32; 3] {
    [f32_to_bits(v.x), f32_to_bits(v.y), f32_to_bits(v.z)]
}

fn make_edge_key(a: Vector3, b: Vector3) -> EdgeKey {
    let ba = vertex_to_bits(a);
    let bb = vertex_to_bits(b);
    // Lexicographic ordering on bit representations
    if ba < bb {
        EdgeKey([ba[0], ba[1], ba[2], bb[0], bb[1], bb[2]])
    } else {
        EdgeKey([bb[0], bb[1], bb[2], ba[0], ba[1], ba[2]])
    }
}

/// Check if an edge lies on the cell perimeter (both vertices share the same boundary).
fn is_boundary_edge(
    a: Vector3,
    b: Vector3,
    min_x: f32,
    max_x: f32,
    min_z: f32,
    max_z: f32,
) -> bool {
    let eps = 1e-5;

    // Both on left boundary (x == min_x)
    if (a.x - min_x).abs() < eps && (b.x - min_x).abs() < eps {
        return true;
    }
    // Both on right boundary (x == max_x)
    if (a.x - max_x).abs() < eps && (b.x - max_x).abs() < eps {
        return true;
    }
    // Both on top boundary (z == min_z)
    if (a.z - min_z).abs() < eps && (b.z - min_z).abs() < eps {
        return true;
    }
    // Both on bottom boundary (z == max_z)
    if (a.z - max_z).abs() < eps && (b.z - max_z).abs() < eps {
        return true;
    }
    false
}

/// Validate that a cell's geometry is watertight (no internal open edges).
///
/// Extracts triangle edges, counts occurrences. Internal edges must appear exactly 2 times.
/// Boundary edges (on cell perimeter) are excluded since adjacent cells provide matching triangles.
pub fn validate_cell_watertight(
    geo: &CellGeometry,
    cell_x: i32,
    cell_z: i32,
    cell_size: Vector2,
) -> ValidationResult {
    let min_x = cell_x as f32 * cell_size.x;
    let max_x = (cell_x + 1) as f32 * cell_size.x;
    let min_z = cell_z as f32 * cell_size.y;
    let max_z = (cell_z + 1) as f32 * cell_size.y;

    // Count edge occurrences
    let mut edge_counts: HashMap<EdgeKey, (Vector3, Vector3, u32)> = HashMap::new();
    let tri_count = geo.verts.len() / 3;

    for tri in 0..tri_count {
        let i = tri * 3;
        let v0 = geo.verts[i];
        let v1 = geo.verts[i + 1];
        let v2 = geo.verts[i + 2];

        for (a, b) in [(v0, v1), (v1, v2), (v2, v0)] {
            let key = make_edge_key(a, b);
            edge_counts
                .entry(key)
                .and_modify(|e| e.2 += 1)
                .or_insert((a, b, 1));
        }
    }

    // Find open internal edges
    let mut open_edges = Vec::new();
    for (_key, (a, b, count)) in &edge_counts {
        if *count == 1 && !is_boundary_edge(*a, *b, min_x, max_x, min_z, max_z) {
            open_edges.push((*a, *b));
        }
    }

    let is_watertight = open_edges.is_empty();
    ValidationResult {
        open_edges,
        is_watertight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marching_squares::cases::generate_cell;
    use crate::marching_squares::cell_context::*;
    use crate::marching_squares::types::*;

    fn default_context() -> CellContext {
        CellContext::test_default(3, 3)
    }

    fn validate_case(heights: [f32; 4], label: &str) {
        let mut ctx = default_context();
        ctx.heights = heights;
        let mut geo = CellGeometry::default();
        generate_cell(&mut ctx, &mut geo);

        let result = validate_cell_watertight(&geo, 0, 0, ctx.config.cell_size);
        if !result.is_watertight {
            for (a, b) in &result.open_edges {
                eprintln!(
                    "  {} open edge: ({:.3},{:.3},{:.3}) - ({:.3},{:.3},{:.3})",
                    label, a.x, a.y, a.z, b.x, b.y, b.z
                );
            }
        }
        assert!(
            result.is_watertight,
            "{}: found {} open internal edges",
            label,
            result.open_edges.len()
        );
    }

    /// Rotate a height array by `n` positions (simulates cell rotation).
    /// Heights are [A, B, D, C]. Rotation maps A->B->D->C->A.
    fn rotate_heights(heights: [f32; 4], n: usize) -> [f32; 4] {
        let mut h = heights;
        for _ in 0..n {
            h = [h[3], h[0], h[1], h[2]]; // C->A, A->B, B->D, D->C
        }
        h
    }

    #[test]
    fn test_case_0_full_floor() {
        validate_case([5.0, 5.0, 5.0, 5.0], "case 0");
    }

    #[test]
    fn test_case_1_outer_corner() {
        validate_case([5.0, 3.0, 3.0, 3.0], "case 1");
    }

    #[test]
    fn test_case_2_edge() {
        validate_case([5.0, 5.0, 3.0, 3.0], "case 2");
    }

    #[test]
    fn test_case_3_edge_a_outer() {
        validate_case([7.0, 5.0, 3.0, 3.0], "case 3");
    }

    #[test]
    fn test_case_4_edge_b_outer() {
        validate_case([5.0, 7.0, 3.0, 3.0], "case 4");
    }

    #[test]
    fn test_case_5_double_inner() {
        validate_case([0.0, 5.0, 0.0, 5.0], "case 5");
    }

    #[test]
    fn test_case_5_5_double_inner_asymmetric() {
        validate_case([0.0, 7.0, 0.0, 5.0], "case 5.5");
    }

    #[test]
    fn test_case_6_inner_corner() {
        validate_case([3.0, 5.0, 5.0, 5.0], "case 6");
    }

    #[test]
    fn test_case_7_inner_asymmetric_bd() {
        validate_case([0.0, 5.0, 5.0, 7.0], "case 7");
    }

    #[test]
    fn test_case_8_inner_asymmetric_cd() {
        validate_case([0.0, 7.0, 5.0, 5.0], "case 8");
    }

    #[test]
    fn test_case_9_inner_diagonal_outer() {
        // A lowest, D highest, B≈C middle merged
        validate_case([0.0, 5.0, 8.0, 5.5], "case 9");
    }

    #[test]
    fn test_case_10_inner_corner_edge_bd() {
        validate_case([0.0, 5.0, 7.0, 5.0], "case 10");
    }

    #[test]
    fn test_case_11_inner_corner_edge_cd() {
        // A low, D higher than B, CD connected: A=0, B=5, D=8, C=8
        validate_case([0.0, 5.0, 8.0, 8.0], "case 11");
    }

    #[test]
    fn test_case_12_spiral_clockwise() {
        validate_case([0.0, 3.0, 5.0, 8.0], "case 12");
    }

    #[test]
    fn test_case_13_spiral_counter() {
        validate_case([0.0, 8.0, 5.0, 3.0], "case 13");
    }

    #[test]
    fn test_case_14_staircase_abcd() {
        validate_case([0.0, 3.0, 9.0, 6.0], "case 14");
    }

    #[test]
    fn test_case_15_staircase_acbd() {
        validate_case([0.0, 6.0, 9.0, 3.0], "case 15");
    }

    #[test]
    fn test_case_17_outer_diagonal_inner() {
        validate_case([9.0, 5.0, 0.0, 5.0], "case 17");
    }

    #[test]
    fn test_case_18_outer_diagonal_outer() {
        // A highest, B≈C merged, D lowest: A=10, B=5, D=0, C=5.5
        validate_case([10.0, 5.0, 0.0, 5.5], "case 18");
    }

    #[test]
    fn test_case_19_outer_partial_edge_b() {
        validate_case([9.0, 6.0, 3.0, 3.0], "case 19");
    }

    #[test]
    fn test_case_20_outer_partial_edge_c() {
        validate_case([9.0, 3.0, 3.0, 6.0], "case 20");
    }

    #[test]
    fn test_case_21_outer_inner_composite() {
        // A higher than B, B≈C merged, D lowest: A=7, B=5.5, D=0, C=6
        validate_case([7.0, 5.5, 0.0, 6.0], "case 21");
    }

    #[test]
    fn test_case_22_single_wall_ac() {
        validate_case([5.0, 4.5, 4.5, 3.0], "case 22");
    }

    #[test]
    fn test_case_23_single_wall_bd() {
        validate_case([4.5, 5.0, 3.0, 4.5], "case 23");
    }

    #[test]
    fn test_all_cases_all_rotations() {
        let cases: Vec<(&str, [f32; 4])> = vec![
            ("case 0", [5.0, 5.0, 5.0, 5.0]),
            ("case 1", [5.0, 3.0, 3.0, 3.0]),
            ("case 2", [5.0, 5.0, 3.0, 3.0]),
            ("case 3", [7.0, 5.0, 3.0, 3.0]),
            ("case 4", [5.0, 7.0, 3.0, 3.0]),
            ("case 5", [0.0, 5.0, 0.0, 5.0]),
            ("case 5.5", [0.0, 7.0, 0.0, 5.0]),
            ("case 6", [3.0, 5.0, 5.0, 5.0]),
            ("case 7", [0.0, 5.0, 5.0, 7.0]),
            ("case 8", [0.0, 7.0, 5.0, 5.0]),
            ("case 9", [0.0, 5.0, 8.0, 5.5]),
            ("case 10", [0.0, 5.0, 7.0, 5.0]),
            ("case 11", [0.0, 5.0, 8.0, 8.0]),
            ("case 12", [0.0, 3.0, 5.0, 8.0]),
            ("case 13", [0.0, 8.0, 5.0, 3.0]),
            ("case 14", [0.0, 3.0, 9.0, 6.0]),
            ("case 15", [0.0, 6.0, 9.0, 3.0]),
            ("case 17", [9.0, 5.0, 0.0, 5.0]),
            ("case 18", [10.0, 5.0, 0.0, 5.5]),
            ("case 19", [9.0, 6.0, 3.0, 3.0]),
            ("case 20", [9.0, 3.0, 3.0, 6.0]),
            ("case 21", [7.0, 5.5, 0.0, 6.0]),
            ("case 22", [5.0, 4.5, 4.5, 3.0]),
            ("case 23", [4.5, 5.0, 3.0, 4.5]),
        ];

        for (label, heights) in &cases {
            for rot in 0..4 {
                let rotated = rotate_heights(*heights, rot);
                let name = format!("{} rot{}", label, rot);
                validate_case(rotated, &name);
            }
        }
    }

    #[test]
    fn test_brute_force_all_heights() {
        let height_values = [0.0, 2.0, 3.0, 5.0, 6.0, 8.0, 10.0];
        let failures = run_brute_force(&height_values);
        assert_brute_force_results(&failures);
    }

    #[test]
    fn test_brute_force_near_threshold() {
        // Test values near the merge_threshold (1.3) to catch boundary gaps
        let height_values = [0.0, 1.0, 1.3, 1.4, 2.6, 3.0, 4.3, 5.0, 6.3, 8.0];
        let failures = run_brute_force(&height_values);
        assert_brute_force_results(&failures);
    }

    #[test]
    fn test_brute_force_ridge_close_to_ridge() {
        // Heights simulating ridges close together (user-reported scenario)
        let height_values = [0.0, 1.5, 3.0, 4.0, 4.5, 5.0, 6.0, 7.0, 7.5, 9.0];
        let failures = run_brute_force(&height_values);
        assert_brute_force_results(&failures);
    }

    fn run_brute_force(height_values: &[f32]) -> Vec<([f32; 4], Vec<(Vector3, Vector3)>)> {
        let mut failures = Vec::new();

        for &a in height_values {
            for &b in height_values {
                for &c in height_values {
                    for &d in height_values {
                        let heights = [a, b, c, d]; // [A, B, D, C]
                        let mut ctx = default_context();
                        ctx.heights = heights;
                        let mut geo = CellGeometry::default();
                        generate_cell(&mut ctx, &mut geo);

                        let result = validate_cell_watertight(&geo, 0, 0, ctx.config.cell_size);
                        if !result.is_watertight {
                            failures.push((heights, result.open_edges.clone()));
                        }
                    }
                }
            }
        }
        failures
    }

    #[test]
    fn test_brute_force_dense() {
        // Dense set: 0 to 10 in steps of 1, plus half-steps near threshold
        let height_values = [
            0.0, 0.5, 1.0, 1.3, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5,
            8.0, 9.0, 10.0,
        ];
        let failures = run_brute_force(&height_values);
        assert_brute_force_results(&failures);
    }

    /// Validate a combined 2-cell mesh for watertightness.
    /// Generates two horizontally adjacent cells, combines their geometry,
    /// and checks for open internal edges (excluding the outer perimeter).
    fn validate_cell_pair_watertight(
        left_heights: [f32; 4],
        right_heights: [f32; 4],
    ) -> ValidationResult {
        let cell_size = Vector2::new(2.0, 2.0);

        let mut left_ctx = default_context();
        left_ctx.heights = left_heights;
        let mut left_geo = CellGeometry::default();
        generate_cell(&mut left_ctx, &mut left_geo);

        let mut right_ctx = default_context();
        right_ctx.heights = right_heights;
        right_ctx.cell_coords = Vector2i::new(1, 0);
        let mut right_geo = CellGeometry::default();
        generate_cell(&mut right_ctx, &mut right_geo);

        // Combine geometry
        let mut combined = CellGeometry::default();
        combined.verts.extend_from_slice(&left_geo.verts);
        combined.verts.extend_from_slice(&right_geo.verts);

        // Validate combined: outer boundary is 2-cell block perimeter
        // x: 0 to 4, z: 0 to 2
        let min_x = 0.0;
        let max_x = 2.0 * cell_size.x; // 4.0
        let min_z = 0.0;
        let max_z = cell_size.y; // 2.0

        let mut edge_counts: HashMap<EdgeKey, (Vector3, Vector3, u32)> = HashMap::new();
        let tri_count = combined.verts.len() / 3;

        for tri in 0..tri_count {
            let i = tri * 3;
            let v0 = combined.verts[i];
            let v1 = combined.verts[i + 1];
            let v2 = combined.verts[i + 2];

            for (a, b) in [(v0, v1), (v1, v2), (v2, v0)] {
                let key = make_edge_key(a, b);
                edge_counts
                    .entry(key)
                    .and_modify(|e| e.2 += 1)
                    .or_insert((a, b, 1));
            }
        }

        let mut open_edges = Vec::new();
        for (_key, (a, b, count)) in &edge_counts {
            if *count == 1 && !is_boundary_edge(*a, *b, min_x, max_x, min_z, max_z) {
                open_edges.push((*a, *b));
            }
        }

        ValidationResult {
            is_watertight: open_edges.is_empty(),
            open_edges,
        }
    }

    /// Cross-cell boundary matching test.
    /// Tests that adjacent cells produce matching edges along their shared boundary
    /// by combining their meshes and checking for open internal edges.
    #[test]
    fn test_cross_cell_boundary_matching() {
        let height_values = [0.0, 2.0, 5.0, 8.0, 10.0];
        let mut failures = Vec::new();

        for &la in &height_values {
            for &lb in &height_values {
                for &ld in &height_values {
                    for &lc in &height_values {
                        let left = [la, lb, ld, lc];
                        // Right cell shares B,D as A,C: right = [B, newB, newD, D]
                        for &rb in &height_values {
                            for &rd in &height_values {
                                let right = [lb, rb, rd, ld];
                                let result = validate_cell_pair_watertight(left, right);
                                if !result.is_watertight {
                                    failures.push((left, right, result.open_edges.len()));
                                }
                            }
                        }
                    }
                }
            }
        }

        if !failures.is_empty() {
            eprintln!("\n=== CROSS-CELL FAILURES ({}) ===", failures.len());
            for (l, r, count) in failures.iter().take(20) {
                eprintln!(
                    "L[{:.0},{:.0},{:.0},{:.0}] R[{:.0},{:.0},{:.0},{:.0}]: {} open edges",
                    l[0], l[1], l[2], l[3], r[0], r[1], r[2], r[3], count
                );
            }
            panic!("{} cross-cell pairs have open edges", failures.len());
        }
    }

    /// Identify which case a cell hits (returns case name and rotation).
    fn identify_case(heights: [f32; 4], merge_threshold: f32) -> String {
        let is_higher = |a: f32, b: f32| a - b > merge_threshold;
        let is_lower = |a: f32, b: f32| a - b < -merge_threshold;
        let is_merged = |a: f32, b: f32| (a - b).abs() < merge_threshold;

        for rotation in 0..4u8 {
            let ay = heights[rotation as usize % 4];
            let by = heights[(rotation as usize + 1) % 4];
            let dy = heights[(rotation as usize + 2) % 4];
            let cy = heights[(rotation as usize + 3) % 4];
            let ab = is_merged(ay, by);
            let bd = is_merged(by, dy);
            let cd = is_merged(cy, dy);
            let ac = is_merged(ay, cy);

            // Case 0 check (only at rotation 0)
            if rotation == 0 && ab && bd && cd && ac {
                return "C0r0".to_string();
            }

            if is_higher(ay, by) && is_higher(ay, cy) && bd && cd {
                return format!("C1r{}", rotation);
            }
            if is_higher(ay, cy) && is_higher(by, dy) && bd && cd {
                return format!("C2r{}", rotation);
            }
            if is_higher(ay, by) && is_higher(ay, cy) && is_higher(by, dy) && cd {
                return format!("C3r{}", rotation);
            }
            if is_higher(by, ay) && is_higher(ay, cy) && is_higher(by, dy) && cd {
                return format!("C4r{}", rotation);
            }
            if is_lower(ay, by)
                && is_lower(ay, cy)
                && is_lower(dy, by)
                && is_lower(dy, cy)
                && is_merged(by, cy)
            {
                return format!("C5r{}", rotation);
            }
            if is_lower(ay, by)
                && is_lower(ay, cy)
                && is_lower(dy, by)
                && is_lower(dy, cy)
                && is_higher(by, cy)
            {
                return format!("C5.5r{}", rotation);
            }
            if is_lower(ay, by) && is_lower(ay, cy) && bd && cd {
                return format!("C6r{}", rotation);
            }
            if is_lower(ay, by) && is_lower(ay, cy) && bd && !cd && is_higher(cy, dy) {
                return format!("C7r{}", rotation);
            }
            if is_lower(ay, by) && is_lower(ay, cy) && !bd && cd && is_higher(by, dy) {
                return format!("C8r{}", rotation);
            }
            if is_lower(ay, by)
                && is_lower(ay, cy)
                && !bd
                && !cd
                && is_higher(by, dy)
                && is_higher(cy, dy)
                && is_merged(by, cy)
            {
                return format!("C9r{}", rotation);
            }
            if is_lower(ay, by) && is_lower(ay, cy) && is_higher(dy, cy) && bd {
                return format!("C10r{}", rotation);
            }
            if is_lower(ay, by) && is_lower(ay, cy) && is_higher(dy, by) && cd {
                return format!("C11r{}", rotation);
            }
            if is_lower(ay, by) && is_lower(by, dy) && is_lower(dy, cy) && is_higher(cy, ay) {
                return format!("C12r{}", rotation);
            }
            if is_lower(ay, cy) && is_lower(cy, dy) && is_lower(dy, by) && is_higher(by, ay) {
                return format!("C13r{}", rotation);
            }
            if is_lower(ay, by) && is_lower(by, cy) && is_lower(cy, dy) {
                return format!("C14r{}", rotation);
            }
            if is_lower(ay, cy) && is_lower(cy, by) && is_lower(by, dy) {
                return format!("C15r{}", rotation);
            }
            if is_higher(ay, cy) && is_merged(ay, by) && is_merged(cy, dy) && ab && cd {
                return format!("C16r{}", rotation);
            }
            if is_higher(ay, by)
                && is_higher(ay, cy)
                && !bd
                && !cd
                && is_lower(dy, by)
                && is_lower(dy, cy)
            {
                return format!("C17r{}", rotation);
            }
            if is_higher(ay, by)
                && is_higher(ay, cy)
                && is_merged(by, cy)
                && is_higher(by, dy)
                && is_higher(cy, dy)
            {
                return format!("C18r{}", rotation);
            }
            if is_higher(ay, by) && is_higher(ay, cy) && is_higher(by, cy) && !cd {
                return format!("C19r{}", rotation);
            }
            if is_higher(ay, by) && is_higher(ay, cy) && is_higher(cy, by) && !bd {
                return format!("C20r{}", rotation);
            }
            if is_higher(ay, by) && is_merged(by, cy) && !bd && is_lower(dy, by) {
                return format!("C21r{}", rotation);
            }
            if ab && bd && cd && !ac && is_higher(ay, cy) {
                return format!("C22r{}", rotation);
            }
            if ab && ac && cd && !bd && is_higher(by, dy) {
                return format!("C23r{}", rotation);
            }
        }
        "NONE".to_string()
    }

    /// Check if a single cell's boundary edges are canonical.
    /// Returns non-canonical edges grouped by boundary name.
    fn check_canonical_boundaries(
        heights: [f32; 4],
        threshold: f32,
    ) -> Vec<(String, Vector3, Vector3)> {
        let _cell_size = Vector2::new(2.0, 2.0);
        let mut ctx = default_context();
        ctx.heights = heights;
        ctx.config.merge_threshold = threshold;
        let mut geo = CellGeometry::default();
        generate_cell(&mut ctx, &mut geo);

        let mut non_canonical = Vec::new();

        // Boundaries in world coords for cell at (0,0) with cell_size=2:
        // AB: z=0, x from 0 to 2. Corners: A(0,h[0],0), B(2,h[1],0)
        // BD: x=2, z from 0 to 2. Corners: B(2,h[1],0), D(2,h[2],2)
        // CD: z=2, x from 0 to 2. Corners: C(0,h[3],2), D(2,h[2],2)
        // AC: x=0, z from 0 to 2. Corners: A(0,h[0],0), C(0,h[3],2)
        let boundaries = [
            ("AB", 0.0_f32, 2.0_f32, true, heights[0], heights[1]), // z=0, x:0→2, h1=A, h2=B
            ("BD", 0.0, 2.0, false, heights[1], heights[2]),        // x=2, z:0→2, h1=B, h2=D
            ("CD", 0.0, 2.0, true, heights[3], heights[2]),         // z=2, x:0→2, h1=C, h2=D
            ("AC", 0.0, 2.0, false, heights[0], heights[3]),        // x=0, z:0→2, h1=A, h2=C
        ];

        let tri_count = geo.verts.len() / 3;
        for (name, _coord_min, _coord_max, is_z_const, h1, h2) in &boundaries {
            let (fixed_val, is_x_fixed) = match *name {
                "AB" => (0.0, false), // z=0 fixed
                "BD" => (2.0, true),  // x=2 fixed
                "CD" => (2.0, false), // z=2 fixed
                "AC" => (0.0, true),  // x=0 fixed
                _ => unreachable!(),
            };

            // Collect boundary edges (both orientations, count them)
            let mut boundary_edges: HashMap<EdgeKey, (Vector3, Vector3, u32)> = HashMap::new();
            for tri in 0..tri_count {
                let i = tri * 3;
                let v0 = geo.verts[i];
                let v1 = geo.verts[i + 1];
                let v2 = geo.verts[i + 2];
                for (a, b) in [(v0, v1), (v1, v2), (v2, v0)] {
                    let on_boundary = if is_x_fixed {
                        (a.x - fixed_val).abs() < 1e-4 && (b.x - fixed_val).abs() < 1e-4
                    } else {
                        (a.z - fixed_val).abs() < 1e-4 && (b.z - fixed_val).abs() < 1e-4
                    };
                    if on_boundary {
                        let key = make_edge_key(a, b);
                        boundary_edges
                            .entry(key)
                            .and_modify(|e| e.2 += 1)
                            .or_insert((a, b, 1));
                    }
                }
            }

            // Build expected canonical edges for this boundary
            let is_merged = (h1 - h2).abs() < threshold;
            let _mid_t = 1.0; // midpoint in world coords = 1.0 (cell_size * 0.5)

            // World coordinates of corners and midpoint
            let (c1, c2, mid_upper, mid_lower) = if *is_z_const {
                // z is fixed, varying coord is x
                let z = fixed_val;
                let x1 = if *name == "CD" { 0.0 } else { 0.0 }; // always 0 for start
                let x2 = 2.0; // always 2 for end
                let xm = 1.0;
                if is_merged {
                    let mid_y = h1 + (h2 - h1) * 0.5;
                    (
                        Vector3::new(x1, *h1, z),
                        Vector3::new(x2, *h2, z),
                        Vector3::new(xm, mid_y, z),
                        Vector3::new(xm, mid_y, z),
                    )
                } else {
                    let upper = h1.max(*h2);
                    let lower = h1.min(*h2);
                    (
                        Vector3::new(x1, *h1, z),
                        Vector3::new(x2, *h2, z),
                        Vector3::new(xm, upper, z),
                        Vector3::new(xm, lower, z),
                    )
                }
            } else {
                // x is fixed, varying coord is z
                let x = fixed_val;
                let z1 = 0.0;
                let z2 = 2.0;
                let zm = 1.0;
                if is_merged {
                    let mid_y = h1 + (h2 - h1) * 0.5;
                    (
                        Vector3::new(x, *h1, z1),
                        Vector3::new(x, *h2, z2),
                        Vector3::new(x, mid_y, zm),
                        Vector3::new(x, mid_y, zm),
                    )
                } else {
                    let upper = h1.max(*h2);
                    let lower = h1.min(*h2);
                    (
                        Vector3::new(x, *h1, z1),
                        Vector3::new(x, *h2, z2),
                        Vector3::new(x, upper, zm),
                        Vector3::new(x, lower, zm),
                    )
                }
            };

            // Canonical edges for this boundary:
            let mut canonical = Vec::new();
            if is_merged {
                // corner1 → mid, mid → corner2
                canonical.push(make_edge_key(c1, mid_upper));
                canonical.push(make_edge_key(mid_upper, c2));
            } else {
                // corner1 side half-edge, wall, corner2 side half-edge
                if *h1 >= *h2 {
                    // h1 is upper: c1→mid_upper, wall, mid_lower→c2
                    canonical.push(make_edge_key(c1, mid_upper));
                    canonical.push(make_edge_key(mid_lower, mid_upper));
                    canonical.push(make_edge_key(mid_lower, c2));
                } else {
                    // h2 is upper: c1→mid_lower, wall, mid_upper→c2
                    canonical.push(make_edge_key(c1, mid_lower));
                    canonical.push(make_edge_key(mid_lower, mid_upper));
                    canonical.push(make_edge_key(mid_upper, c2));
                }
            }

            // Check each boundary edge with count=1 against canonical set
            for (_key, (a, b, count)) in &boundary_edges {
                if *count == 1 {
                    let key = make_edge_key(*a, *b);
                    if !canonical.contains(&key) {
                        non_canonical.push((name.to_string(), *a, *b));
                    }
                }
            }
        }

        non_canonical
    }

    /// Identify which cases produce non-canonical boundary edges.
    #[test]
    fn test_single_cell_canonical_boundaries() {
        let height_values = [0.0_f32, 2.0, 5.0, 8.0];
        let threshold = 1.3;
        let mut case_failures: HashMap<String, usize> = HashMap::new();
        let mut case_samples: HashMap<String, Vec<([f32; 4], Vec<(String, Vector3, Vector3)>)>> =
            HashMap::new();
        let mut total = 0;

        for &a in &height_values {
            for &b in &height_values {
                for &d in &height_values {
                    for &c in &height_values {
                        let heights = [a, b, d, c];
                        let bad = check_canonical_boundaries(heights, threshold);
                        if !bad.is_empty() {
                            total += 1;
                            let case_name = identify_case(heights, threshold);
                            *case_failures.entry(case_name.clone()).or_insert(0) += 1;
                            let samples = case_samples.entry(case_name).or_default();
                            if samples.len() < 2 {
                                samples.push((heights, bad));
                            }
                        }
                    }
                }
            }
        }

        eprintln!(
            "\n=== NON-CANONICAL BOUNDARY EDGES BY CASE ({} total cells) ===",
            total
        );
        let mut sorted: Vec<_> = case_failures.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (case_name, count) in &sorted {
            eprintln!("  {:>4} x {}", count, case_name);
            if let Some(samples) = case_samples.get(*case_name) {
                for (h, bad_edges) in samples.iter().take(1) {
                    eprintln!(
                        "         [{:.0},{:.0},{:.0},{:.0}]:",
                        h[0], h[1], h[2], h[3]
                    );
                    for (bname, a, b) in bad_edges.iter().take(4) {
                        eprintln!(
                            "           {} ({:.1},{:.1},{:.1})-({:.1},{:.1},{:.1})",
                            bname, a.x, a.y, a.z, b.x, b.y, b.z
                        );
                    }
                }
            }
        }
        assert_eq!(
            total, 0,
            "{} cells have non-canonical boundary edges",
            total
        );
    }

    /// Debug test: identify which case pairs cause the most cross-cell failures.
    #[test]
    fn test_cross_cell_debug_specific() {
        let height_values = [0.0_f32, 2.0, 5.0, 8.0];
        let mut case_pair_counts: HashMap<String, usize> = HashMap::new();
        let mut total_failures = 0;
        let threshold = 1.3;

        // Collect samples per case pair (up to 3 each)
        let mut case_pair_samples: HashMap<
            String,
            Vec<([f32; 4], [f32; 4], Vec<(Vector3, Vector3)>)>,
        > = HashMap::new();

        for &la in &height_values {
            for &lb in &height_values {
                for &ld in &height_values {
                    for &lc in &height_values {
                        let left = [la, lb, ld, lc];
                        for &rb in &height_values {
                            for &rd in &height_values {
                                let right = [lb, rb, rd, ld];
                                let result = validate_cell_pair_watertight(left, right);
                                if !result.is_watertight {
                                    total_failures += 1;
                                    let lc_name = identify_case(left, threshold);
                                    let rc_name = identify_case(right, threshold);
                                    let pair = format!("{} | {}", lc_name, rc_name);
                                    *case_pair_counts.entry(pair.clone()).or_insert(0) += 1;
                                    let samples = case_pair_samples.entry(pair).or_default();
                                    if samples.len() < 2 {
                                        samples.push((left, right, result.open_edges.clone()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        eprintln!(
            "\n=== CASE PAIR FAILURE SUMMARY ({} total) ===",
            total_failures
        );
        let mut sorted: Vec<_> = case_pair_counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (pair, count) in sorted.iter().take(30) {
            eprintln!("  {:>5} x {}", count, pair);
            if let Some(samples) = case_pair_samples.get(*pair) {
                for (left, right, edges) in samples.iter().take(1) {
                    eprintln!(
                        "         L[{:.0},{:.0},{:.0},{:.0}] R[{:.0},{:.0},{:.0},{:.0}]:",
                        left[0], left[1], left[2], left[3], right[0], right[1], right[2], right[3],
                    );
                    for (a, b) in edges {
                        let on_shared = (a.x - 2.0).abs() < 0.01 && (b.x - 2.0).abs() < 0.01;
                        if on_shared {
                            eprintln!(
                                "           ({:.1},{:.1},{:.1})-({:.1},{:.1},{:.1})",
                                a.x, a.y, a.z, b.x, b.y, b.z
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(
            total_failures, 0,
            "{} cross-cell pairs have open edges",
            total_failures
        );
    }

    /// Near-threshold cross-cell boundary matching test.
    /// Uses heights that produce mixed merged/walled boundaries within the same cell pair,
    /// which the original test (using well-separated heights) never exercises.
    #[test]
    fn test_cross_cell_near_threshold() {
        let height_values = [0.0_f32, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];
        let mut failures = Vec::new();

        for &la in &height_values {
            for &lb in &height_values {
                for &ld in &height_values {
                    for &lc in &height_values {
                        let left = [la, lb, ld, lc];
                        // Right cell shares B,D as A,C: right = [B, newB, newD, D]
                        for &rb in &height_values {
                            for &rd in &height_values {
                                let right = [lb, rb, rd, ld];
                                let result = validate_cell_pair_watertight(left, right);
                                if !result.is_watertight {
                                    failures.push((left, right, result.open_edges.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }

        if !failures.is_empty() {
            eprintln!(
                "\n=== NEAR-THRESHOLD CROSS-CELL FAILURES ({}) ===",
                failures.len()
            );
            let threshold = 1.3;
            for (l, r, edges) in failures.iter().take(30) {
                let lc_name = identify_case(*l, threshold);
                let rc_name = identify_case(*r, threshold);
                eprintln!(
                    "L[{:.0},{:.0},{:.0},{:.0}] R[{:.0},{:.0},{:.0},{:.0}] {} | {}: {} open edges",
                    l[0],
                    l[1],
                    l[2],
                    l[3],
                    r[0],
                    r[1],
                    r[2],
                    r[3],
                    lc_name,
                    rc_name,
                    edges.len()
                );
                for (a, b) in edges.iter().take(4) {
                    eprintln!(
                        "  ({:.3},{:.3},{:.3})-({:.3},{:.3},{:.3})",
                        a.x, a.y, a.z, b.x, b.y, b.z
                    );
                }
            }
            panic!(
                "{} near-threshold cross-cell pairs have open edges",
                failures.len()
            );
        }
    }

    /// Validate a combined 2-cell mesh for watertightness (vertical adjacency).
    /// Generates two vertically adjacent cells (top and bottom), combines their geometry,
    /// and checks for open internal edges (excluding the outer perimeter).
    fn validate_cell_pair_vertical(
        top_heights: [f32; 4],
        bottom_heights: [f32; 4],
    ) -> ValidationResult {
        let cell_size = Vector2::new(2.0, 2.0);

        let mut top_ctx = default_context();
        top_ctx.heights = top_heights;
        let mut top_geo = CellGeometry::default();
        generate_cell(&mut top_ctx, &mut top_geo);

        let mut bottom_ctx = default_context();
        bottom_ctx.heights = bottom_heights;
        bottom_ctx.cell_coords = Vector2i::new(0, 1);
        let mut bottom_geo = CellGeometry::default();
        generate_cell(&mut bottom_ctx, &mut bottom_geo);

        // Combine geometry
        let mut combined = CellGeometry::default();
        combined.verts.extend_from_slice(&top_geo.verts);
        combined.verts.extend_from_slice(&bottom_geo.verts);

        // Validate combined: outer boundary is 2-cell block perimeter
        // x: 0 to 2, z: 0 to 4
        let min_x = 0.0;
        let max_x = cell_size.x; // 2.0
        let min_z = 0.0;
        let max_z = 2.0 * cell_size.y; // 4.0

        let mut edge_counts: HashMap<EdgeKey, (Vector3, Vector3, u32)> = HashMap::new();
        let tri_count = combined.verts.len() / 3;

        for tri in 0..tri_count {
            let i = tri * 3;
            let v0 = combined.verts[i];
            let v1 = combined.verts[i + 1];
            let v2 = combined.verts[i + 2];

            for (a, b) in [(v0, v1), (v1, v2), (v2, v0)] {
                let key = make_edge_key(a, b);
                edge_counts
                    .entry(key)
                    .and_modify(|e| e.2 += 1)
                    .or_insert((a, b, 1));
            }
        }

        let mut open_edges = Vec::new();
        for (_key, (a, b, count)) in &edge_counts {
            if *count == 1 && !is_boundary_edge(*a, *b, min_x, max_x, min_z, max_z) {
                open_edges.push((*a, *b));
            }
        }

        ValidationResult {
            is_watertight: open_edges.is_empty(),
            open_edges,
        }
    }

    /// Vertical cross-cell boundary matching test.
    /// Tests that top-bottom adjacent cells produce matching edges along their shared
    /// CD/AB boundary. Bottom cell's A=top's C, bottom cell's B=top's D.
    #[test]
    fn test_cross_cell_vertical_matching() {
        let height_values = [0.0_f32, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];
        let mut failures = Vec::new();

        for &ta in &height_values {
            for &tb in &height_values {
                for &td in &height_values {
                    for &tc in &height_values {
                        let top = [ta, tb, td, tc];
                        // Bottom cell shares C,D as A,B: bottom = [C, D, newD, newC]
                        for &bd in &height_values {
                            for &bc in &height_values {
                                let bottom = [tc, td, bd, bc];
                                let result = validate_cell_pair_vertical(top, bottom);
                                if !result.is_watertight {
                                    failures.push((top, bottom, result.open_edges.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }

        if !failures.is_empty() {
            eprintln!(
                "\n=== VERTICAL CROSS-CELL FAILURES ({}) ===",
                failures.len()
            );
            let threshold = 1.3;
            for (t, b, edges) in failures.iter().take(30) {
                let tc_name = identify_case(*t, threshold);
                let bc_name = identify_case(*b, threshold);
                eprintln!(
                    "T[{:.0},{:.0},{:.0},{:.0}] B[{:.0},{:.0},{:.0},{:.0}] {} | {}: {} open edges",
                    t[0],
                    t[1],
                    t[2],
                    t[3],
                    b[0],
                    b[1],
                    b[2],
                    b[3],
                    tc_name,
                    bc_name,
                    edges.len()
                );
                for (a, b) in edges.iter().take(4) {
                    eprintln!(
                        "  ({:.3},{:.3},{:.3})-({:.3},{:.3},{:.3})",
                        a.x, a.y, a.z, b.x, b.y, b.z
                    );
                }
            }
            panic!(
                "{} vertical cross-cell pairs have open edges",
                failures.len()
            );
        }
    }

    fn assert_brute_force_results(failures: &[([f32; 4], Vec<(Vector3, Vector3)>)]) {
        if !failures.is_empty() {
            eprintln!("\n=== BRUTE FORCE FAILURES ({}) ===", failures.len());
            for (h, edges) in failures.iter().take(50) {
                eprintln!(
                    "FAIL [{:.1}, {:.1}, {:.1}, {:.1}]: {} open edges",
                    h[0],
                    h[1],
                    h[2],
                    h[3],
                    edges.len()
                );
                for (a, b) in edges {
                    eprintln!(
                        "  edge: ({:.3},{:.3},{:.3}) - ({:.3},{:.3},{:.3})",
                        a.x, a.y, a.z, b.x, b.y, b.z
                    );
                }
            }
            if failures.len() > 50 {
                eprintln!("... and {} more", failures.len() - 50);
            }
            panic!("{} height combinations have open edges", failures.len());
        }
    }
}
