use std::collections::HashMap;

use crate::chunk::MeshResult;
use crate::debug_log::{count_duplicate_positions, debug_log};
use meshopt::SimplifyOptions;

/// Combined mesh data from multiple chunks
pub struct CombinedMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl CombinedMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

impl Default for CombinedMesh {
    fn default() -> Self {
        Self::new()
    }
}

/// Post-processor for mesh optimization and repair
pub struct MeshPostProcessor {
    pub weld_epsilon: f32,
    pub normal_angle_threshold: f32,
    pub target_triangle_count: Option<usize>,
}

impl MeshPostProcessor {
    pub fn new(
        weld_epsilon: f32,
        normal_angle_threshold: f32,
        target_triangle_count: Option<usize>,
    ) -> Self {
        Self {
            weld_epsilon,
            normal_angle_threshold,
            target_triangle_count,
        }
    }

    /// Merge multiple chunk meshes into a single combined mesh
    pub fn merge_chunks(&self, chunks: &[MeshResult]) -> CombinedMesh {
        debug_log(&format!("[merge_chunks] Merging {} chunks", chunks.len()));

        let mut combined = CombinedMesh::new();
        let mut skipped_triangles = 0;

        for chunk in chunks {
            let base_index = combined.vertices.len() as u32;
            let chunk_vertex_count = chunk.vertices.len() as i32;
            let chunk_tri_count = chunk.indices.len() / 3;

            // Copy vertices and normals
            combined.vertices.extend_from_slice(&chunk.vertices);
            combined.normals.extend_from_slice(&chunk.normals);

            // Offset and copy indices, validating triangles
            let mut chunk_skipped = 0;
            for tri in chunk.indices.chunks(3) {
                if tri.len() != 3 {
                    chunk_skipped += 1;
                    continue;
                }
                let (i0, i1, i2) = (tri[0], tri[1], tri[2]);

                // Skip entire triangle if any index is invalid
                if i0 < 0
                    || i0 >= chunk_vertex_count
                    || i1 < 0
                    || i1 >= chunk_vertex_count
                    || i2 < 0
                    || i2 >= chunk_vertex_count
                {
                    chunk_skipped += 1;
                    continue;
                }

                combined.indices.push(base_index + i0 as u32);
                combined.indices.push(base_index + i1 as u32);
                combined.indices.push(base_index + i2 as u32);
            }

            // Log each chunk being merged (especially floor chunks)
            if chunk.coord.y == -1 {
                debug_log(&format!(
                    "[merge_chunks]   Floor chunk ({}, -1, {}): {} verts, {} tris{}",
                    chunk.coord.x,
                    chunk.coord.z,
                    chunk.vertices.len(),
                    chunk_tri_count,
                    if chunk_skipped > 0 {
                        format!(" (skipped {} invalid)", chunk_skipped)
                    } else {
                        String::new()
                    }
                ));
            }

            skipped_triangles += chunk_skipped;
        }

        debug_log(&format!(
            "[merge_chunks] Final: {} verts, {} tris{}",
            combined.vertices.len(),
            combined.indices.len() / 3,
            if skipped_triangles > 0 {
                format!(" (skipped {} invalid tris total)", skipped_triangles)
            } else {
                String::new()
            }
        ));

        // Count duplicate positions for debugging
        if cfg!(debug_assertions) && !combined.vertices.is_empty() {
            let boundary_verts = count_duplicate_positions(&combined.vertices, self.weld_epsilon);
            if boundary_verts > 0 {
                debug_log(&format!(
                    "[merge_chunks] {} boundary vertices need cross-chunk normal averaging",
                    boundary_verts
                ));
            }
        }

        combined
    }

    /// Average normals for vertices at the same position across chunk boundaries
    /// Call this AFTER merge_chunks but BEFORE weld_vertices for best results
    pub fn average_boundary_normals(&self, mesh: &mut CombinedMesh) {
        if mesh.vertices.is_empty() || mesh.normals.is_empty() {
            return;
        }

        // Build spatial hash of vertex positions
        let scale = 1.0 / self.weld_epsilon;
        let mut position_to_indices: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();

        for (idx, v) in mesh.vertices.iter().enumerate() {
            let key = (
                (v[0] * scale).round() as i32,
                (v[1] * scale).round() as i32,
                (v[2] * scale).round() as i32,
            );
            position_to_indices.entry(key).or_default().push(idx);
        }

        // For each group of vertices at the same position, average their normals
        let mut averaged_count = 0;
        for indices in position_to_indices.values() {
            if indices.len() > 1 {
                // Accumulate normals from all vertices at this position
                let mut acc = [0.0f32; 3];
                for &idx in indices {
                    let n = mesh.normals[idx];
                    acc[0] += n[0];
                    acc[1] += n[1];
                    acc[2] += n[2];
                }

                // Normalize the accumulated result
                let averaged = normalize(acc);

                // Apply averaged normal to all vertices at this position
                for &idx in indices {
                    mesh.normals[idx] = averaged;
                }

                averaged_count += 1;
            }
        }

        if cfg!(debug_assertions) && averaged_count > 0 {
            debug_log(&format!(
                "[average_boundary_normals] Averaged normals at {} boundary positions",
                averaged_count
            ));
        }
    }

    /// Weld vertices that are within epsilon distance of each other
    /// Uses meshopt for efficient vertex deduplication
    pub fn weld_vertices(&self, mesh: &mut CombinedMesh) {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            return;
        }

        let vertex_count = mesh.vertices.len();

        // Validate indices before processing
        for &idx in &mesh.indices {
            if idx as usize >= vertex_count {
                // Invalid index found - abort to avoid panic
                return;
            }
        }

        // Convert to flat f32 array for meshopt
        let vertex_data: Vec<f32> = mesh
            .vertices
            .iter()
            .flat_map(|v| v.iter().copied())
            .collect();

        // Generate remap table using meshopt
        let mut remap: Vec<u32> = vec![0; vertex_count];

        let unique_count = unsafe {
            meshopt::ffi::meshopt_generateVertexRemap(
                remap.as_mut_ptr(),
                mesh.indices.as_ptr(),
                mesh.indices.len(),
                vertex_data.as_ptr() as *const std::ffi::c_void,
                vertex_count,
                std::mem::size_of::<[f32; 3]>(),
            )
        };

        // Safety check: if meshopt returned 0 unique vertices, don't modify the mesh
        if unique_count == 0 {
            return;
        }

        // Remap indices
        for idx in &mut mesh.indices {
            *idx = remap[*idx as usize];
        }

        // Create new vertex/normal arrays with only unique vertices
        // Accumulate normals for welded vertices instead of overwriting
        let mut new_vertices = vec![[0.0f32; 3]; unique_count];
        let mut normal_accum = vec![[0.0f32; 3]; unique_count];

        for (old_idx, &new_idx) in remap.iter().enumerate() {
            let idx = new_idx as usize;
            if idx < unique_count {
                new_vertices[idx] = mesh.vertices[old_idx];
                // Accumulate normals from all vertices that map to this unique vertex
                let n = mesh.normals[old_idx];
                normal_accum[idx][0] += n[0];
                normal_accum[idx][1] += n[1];
                normal_accum[idx][2] += n[2];
            }
        }

        // Normalize accumulated normals
        let new_normals: Vec<[f32; 3]> = normal_accum.iter().map(|n| normalize(*n)).collect();

        mesh.vertices = new_vertices;
        mesh.normals = new_normals;
    }

    /// Repair non-manifold edges and vertices
    /// Currently a placeholder - manifold repair is complex and may need
    /// additional library support
    pub fn repair_manifold(&self, _mesh: &mut CombinedMesh) {
        // TODO: Implement manifold repair
        // This would involve:
        // 1. Detecting non-manifold edges (edges shared by more than 2 triangles)
        // 2. Detecting non-manifold vertices (vertices where triangle fans don't form a disk)
        // 3. Splitting or removing problematic geometry
    }

    /// Recompute normals based on angle threshold
    /// Vertices with face angles below threshold get smooth normals,
    /// vertices with sharp angles get flat normals
    pub fn recompute_normals(&self, mesh: &mut CombinedMesh) {
        if mesh.indices.is_empty() || mesh.vertices.is_empty() {
            return;
        }

        // Validate indices don't exceed vertex count
        let max_idx = mesh.vertices.len() as u32;
        for &idx in &mesh.indices {
            if idx >= max_idx {
                // Invalid index - abort normal computation to avoid panic
                return;
            }
        }

        let threshold_cos = (self.normal_angle_threshold.to_radians()).cos();

        // First, compute face normals for all triangles
        // We store BOTH area-weighted (raw cross product) and normalized versions
        // Area-weighted is used for accumulation, normalized for angle comparisons
        let triangle_count = mesh.indices.len() / 3;
        let mut face_normals_weighted: Vec<[f32; 3]> = Vec::with_capacity(triangle_count);
        let mut face_normals_unit: Vec<[f32; 3]> = Vec::with_capacity(triangle_count);
        let mut face_areas: Vec<f32> = Vec::with_capacity(triangle_count);

        for i in 0..triangle_count {
            let i0 = mesh.indices[i * 3] as usize;
            let i1 = mesh.indices[i * 3 + 1] as usize;
            let i2 = mesh.indices[i * 3 + 2] as usize;

            let v0 = mesh.vertices[i0];
            let v1 = mesh.vertices[i1];
            let v2 = mesh.vertices[i2];

            let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

            let normal = cross(edge1, edge2);
            let area = length(normal) * 0.5; // Cross product magnitude = 2x triangle area
            face_normals_weighted.push(normal); // Keep area-weighted for accumulation
            face_normals_unit.push(normalize(normal)); // Normalized for angle comparison
            face_areas.push(area);
        }

        // Build vertex-to-face adjacency
        let mut vertex_faces: Vec<Vec<usize>> = vec![Vec::new(); mesh.vertices.len()];
        for (face_idx, chunk) in mesh.indices.chunks(3).enumerate() {
            for &vertex_idx in chunk {
                vertex_faces[vertex_idx as usize].push(face_idx);
            }
        }

        // Compute smooth normals per vertex
        let mut new_normals = vec![[0.0f32; 3]; mesh.vertices.len()];

        for (vertex_idx, faces) in vertex_faces.iter().enumerate() {
            if faces.is_empty() {
                continue;
            }

            let mut accumulated = [0.0f32; 3];

            // For each face, check if it's compatible with the majority of other faces
            // Only exclude faces that are sharp relative to ALL other faces
            // Track largest face for fallback
            let mut largest_face_idx = faces[0];
            let mut largest_face_area = face_areas[faces[0]];

            for &face_idx in faces {
                let face_normal_unit = face_normals_unit[face_idx];
                let face_normal_weighted = face_normals_weighted[face_idx];

                // Track largest face for fallback
                if face_areas[face_idx] > largest_face_area {
                    largest_face_area = face_areas[face_idx];
                    largest_face_idx = face_idx;
                }

                // Count how many other faces this face is smooth with
                let mut smooth_count = 0;
                let mut sharp_count = 0;
                for &other_face_idx in faces {
                    if face_idx != other_face_idx {
                        let other_normal = face_normals_unit[other_face_idx];
                        let d = dot(face_normal_unit, other_normal);
                        if d >= threshold_cos {
                            smooth_count += 1;
                        } else {
                            sharp_count += 1;
                        }
                    }
                }

                // Include this face in the average if:
                // - It's the only face (no neighbors)
                // - It's smooth with at least one other face
                // - It's smooth with the majority of faces (for complex vertices)
                let should_include = faces.len() == 1 || smooth_count > 0 || sharp_count == 0;

                if should_include {
                    // Use area-weighted normal (raw cross product) for accumulation
                    // Larger faces contribute more to the final normal
                    accumulated[0] += face_normal_weighted[0];
                    accumulated[1] += face_normal_weighted[1];
                    accumulated[2] += face_normal_weighted[2];
                }
            }

            // If nothing accumulated (all faces mutually sharp), use largest face normal
            // This is more stable than using arbitrary first face
            if accumulated[0] == 0.0 && accumulated[1] == 0.0 && accumulated[2] == 0.0 {
                accumulated = face_normals_unit[largest_face_idx];
            }

            new_normals[vertex_idx] = normalize(accumulated);
        }

        mesh.normals = new_normals;
    }

    /// Decimate mesh to target triangle count using meshopt
    pub fn decimate(&self, mesh: &mut CombinedMesh) {
        let Some(target) = self.target_triangle_count else {
            return;
        };

        if mesh.indices.is_empty() || target == 0 {
            return;
        }

        let before_tris = mesh.indices.len() / 3;
        let before_verts = mesh.vertices.len();

        debug_log(&format!(
            "[decimate] START: {} verts, {} tris -> target {} tris",
            before_verts, before_tris, target
        ));

        let target_indices = target * 3;
        if target_indices >= mesh.indices.len() {
            debug_log("[decimate] Already at or below target, skipping");
            return; // Already at or below target
        }

        // Convert vertices to flat array for meshopt
        let vertex_data: Vec<f32> = mesh
            .vertices
            .iter()
            .flat_map(|v| v.iter().copied())
            .collect();

        let mut new_indices: Vec<u32> = vec![0; mesh.indices.len()];

        // Use Prune to remove isolated/disconnected components created during simplification
        // Use LockBorder to preserve chunk boundary vertices for seamless terrain
        let options = (SimplifyOptions::Prune | SimplifyOptions::LockBorder).bits();

        let result_count = unsafe {
            meshopt::ffi::meshopt_simplify(
                new_indices.as_mut_ptr(),
                mesh.indices.as_ptr(),
                mesh.indices.len(),
                vertex_data.as_ptr(),
                mesh.vertices.len(),
                std::mem::size_of::<[f32; 3]>(),
                target_indices,
                1e-2, // target error
                options,
                std::ptr::null_mut(),
            )
        };

        debug_log(&format!(
            "[decimate] meshopt returned {} indices ({} tris)",
            result_count,
            result_count / 3
        ));

        new_indices.truncate(result_count);
        mesh.indices = new_indices;

        // Remove degenerate triangles created by decimation
        self.remove_degenerate_triangles(mesh);

        // Remove extremely thin/elongated triangles (aspect ratio check)
        self.remove_thin_triangles(mesh, 20.0); // Remove triangles with aspect ratio > 20

        // Remove small isolated triangle groups (stray geometry from aggressive decimation)
        self.remove_small_components(mesh, 10); // Pass 1: vertex-based (obvious islands)
        self.remove_weakly_connected_components(mesh, 5); // Pass 2: edge-based (pinch points)

        // Remove thin ribbon-like components (connected but very narrow strips)
        self.remove_thin_components(mesh, 0.1); // Remove components where min_extent/max_extent < 0.1

        // Remove unused vertices after decimation
        self.remove_unused_vertices(mesh);

        debug_log(&format!(
            "[decimate] FINAL: {} verts, {} tris (removed {} verts)",
            mesh.vertices.len(),
            mesh.indices.len() / 3,
            before_verts.saturating_sub(mesh.vertices.len())
        ));
    }

    /// Remove triangles with zero or near-zero area (degenerate triangles)
    fn remove_degenerate_triangles(&self, mesh: &mut CombinedMesh) {
        if mesh.indices.is_empty() {
            return;
        }

        let mut valid_indices = Vec::with_capacity(mesh.indices.len());
        let mut removed = 0;

        for tri in mesh.indices.chunks(3) {
            if tri.len() != 3 {
                removed += 1;
                continue;
            }

            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;

            // Bounds check
            if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
                removed += 1;
                continue;
            }

            let v0 = mesh.vertices[i0];
            let v1 = mesh.vertices[i1];
            let v2 = mesh.vertices[i2];

            // Check for zero-area triangle using cross product
            let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
            let cross_prod = cross(edge1, edge2);
            let area_sq = cross_prod[0] * cross_prod[0]
                + cross_prod[1] * cross_prod[1]
                + cross_prod[2] * cross_prod[2];

            // Area squared threshold - very small triangles are degenerate
            if area_sq > 1e-10 {
                valid_indices.extend_from_slice(tri);
            } else {
                removed += 1;
            }
        }

        if removed > 0 {
            debug_log(&format!(
                "[remove_degenerate] Removed {} degenerate triangles",
                removed
            ));
            mesh.indices = valid_indices;
        }
    }

    /// Remove small disconnected triangle groups (isolated geometry)
    /// Uses union-find to identify connected components, then removes components
    /// with fewer than `min_triangles` triangles
    fn remove_small_components(&self, mesh: &mut CombinedMesh, min_triangles: usize) {
        if mesh.indices.is_empty() {
            return;
        }

        let triangle_count = mesh.indices.len() / 3;
        if triangle_count <= min_triangles {
            return; // Don't remove if entire mesh is smaller than threshold
        }

        // Union-Find data structure for grouping connected triangles
        let mut parent: Vec<usize> = (0..triangle_count).collect();
        let mut rank: Vec<usize> = vec![0; triangle_count];

        // Find with path compression
        fn find(parent: &mut [usize], i: usize) -> usize {
            if parent[i] != i {
                parent[i] = find(parent, parent[i]);
            }
            parent[i]
        }

        // Union by rank
        fn union(parent: &mut [usize], rank: &mut [usize], a: usize, b: usize) {
            let ra = find(parent, a);
            let rb = find(parent, b);
            if ra != rb {
                if rank[ra] < rank[rb] {
                    parent[ra] = rb;
                } else if rank[ra] > rank[rb] {
                    parent[rb] = ra;
                } else {
                    parent[rb] = ra;
                    rank[ra] += 1;
                }
            }
        }

        // Build vertex -> triangles mapping
        let mut vertex_to_triangles: HashMap<u32, Vec<usize>> = HashMap::new();
        for (tri_idx, tri) in mesh.indices.chunks(3).enumerate() {
            for &v in tri {
                vertex_to_triangles.entry(v).or_default().push(tri_idx);
            }
        }

        // Union triangles that share vertices
        for triangles in vertex_to_triangles.values() {
            if triangles.len() > 1 {
                let first = triangles[0];
                for &other in &triangles[1..] {
                    union(&mut parent, &mut rank, first, other);
                }
            }
        }

        // Count triangles per component
        let mut component_sizes: HashMap<usize, usize> = HashMap::new();
        for i in 0..triangle_count {
            let root = find(&mut parent, i);
            *component_sizes.entry(root).or_default() += 1;
        }

        // Find the largest component (main mesh)
        let largest_component_size = *component_sizes.values().max().unwrap_or(&0);

        // Keep triangles from components >= min_triangles OR if it's the main component
        let mut valid_indices = Vec::with_capacity(mesh.indices.len());
        let mut removed_triangles = 0;

        for (tri_idx, tri) in mesh.indices.chunks(3).enumerate() {
            let root = find(&mut parent, tri_idx);
            let size = component_sizes[&root];

            // Keep if component is large enough or is the main mesh
            if size >= min_triangles || size == largest_component_size {
                valid_indices.extend_from_slice(tri);
            } else {
                removed_triangles += 1;
            }
        }

        // Count unique removed components
        let mut removed_component_roots: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        for i in 0..triangle_count {
            let root = find(&mut parent, i);
            let size = component_sizes[&root];
            if size < min_triangles && size != largest_component_size {
                removed_component_roots.insert(root);
            }
        }

        if removed_triangles > 0 {
            debug_log(&format!(
                "[remove_small_components] Removed {} triangles in {} isolated components (threshold: {} tris)",
                removed_triangles, removed_component_roots.len(), min_triangles
            ));
            mesh.indices = valid_indices;
        }
    }

    /// Remove components connected only by single vertices (not edges)
    /// Catches "pinch point" geometry from aggressive decimation
    fn remove_weakly_connected_components(&self, mesh: &mut CombinedMesh, min_triangles: usize) {
        if mesh.indices.is_empty() {
            return;
        }

        let triangle_count = mesh.indices.len() / 3;
        if triangle_count <= min_triangles {
            return;
        }

        // Build edge -> triangles mapping
        // Edge key: (smaller_index, larger_index) for canonical form
        let mut edge_to_triangles: HashMap<(u32, u32), Vec<usize>> = HashMap::new();

        for (tri_idx, tri) in mesh.indices.chunks(3).enumerate() {
            let (v0, v1, v2) = (tri[0], tri[1], tri[2]);

            // Add all three edges (canonical ordering)
            for &(a, b) in &[(v0, v1), (v1, v2), (v2, v0)] {
                let edge = (a.min(b), a.max(b));
                edge_to_triangles.entry(edge).or_default().push(tri_idx);
            }
        }

        // Union-Find: only union triangles sharing an EDGE
        let mut parent: Vec<usize> = (0..triangle_count).collect();
        let mut rank: Vec<usize> = vec![0; triangle_count];

        fn find(parent: &mut [usize], i: usize) -> usize {
            if parent[i] != i {
                parent[i] = find(parent, parent[i]);
            }
            parent[i]
        }

        fn union(parent: &mut [usize], rank: &mut [usize], a: usize, b: usize) {
            let (ra, rb) = (find(parent, a), find(parent, b));
            if ra != rb {
                if rank[ra] < rank[rb] {
                    parent[ra] = rb;
                } else if rank[ra] > rank[rb] {
                    parent[rb] = ra;
                } else {
                    parent[rb] = ra;
                    rank[ra] += 1;
                }
            }
        }

        // Union edge-connected triangles
        for triangles in edge_to_triangles.values() {
            if triangles.len() > 1 {
                let first = triangles[0];
                for &other in &triangles[1..] {
                    union(&mut parent, &mut rank, first, other);
                }
            }
        }

        // Count component sizes and find largest
        let mut component_sizes: HashMap<usize, usize> = HashMap::new();
        for i in 0..triangle_count {
            let root = find(&mut parent, i);
            *component_sizes.entry(root).or_default() += 1;
        }
        let largest = *component_sizes.values().max().unwrap_or(&0);

        // Keep triangles from large components
        let mut valid_indices = Vec::with_capacity(mesh.indices.len());
        let mut removed = 0;

        for (tri_idx, tri) in mesh.indices.chunks(3).enumerate() {
            let root = find(&mut parent, tri_idx);
            let size = component_sizes[&root];
            if size >= min_triangles || size == largest {
                valid_indices.extend_from_slice(tri);
            } else {
                removed += 1;
            }
        }

        if removed > 0 {
            debug_log(&format!(
                "[remove_weakly_connected] Removed {} edge-isolated triangles",
                removed
            ));
            mesh.indices = valid_indices;
        }
    }

    /// Remove triangles with extreme aspect ratios (very thin/elongated triangles)
    /// Aspect ratio = longest_edge / shortest_altitude
    fn remove_thin_triangles(&self, mesh: &mut CombinedMesh, max_aspect_ratio: f32) {
        if mesh.indices.is_empty() {
            return;
        }

        let mut valid_indices = Vec::with_capacity(mesh.indices.len());
        let mut removed = 0;

        for tri in mesh.indices.chunks(3) {
            if tri.len() != 3 {
                removed += 1;
                continue;
            }

            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;

            if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
                removed += 1;
                continue;
            }

            let v0 = mesh.vertices[i0];
            let v1 = mesh.vertices[i1];
            let v2 = mesh.vertices[i2];

            // Compute edge lengths
            let e0 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]]; // v0 -> v1
            let e1 = [v2[0] - v1[0], v2[1] - v1[1], v2[2] - v1[2]]; // v1 -> v2
            let e2 = [v0[0] - v2[0], v0[1] - v2[1], v0[2] - v2[2]]; // v2 -> v0

            let len0 = length(e0);
            let len1 = length(e1);
            let len2 = length(e2);

            let longest_edge = len0.max(len1).max(len2);

            // Compute area using cross product
            let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
            let cross_prod = cross(edge1, edge2);
            let area = length(cross_prod) * 0.5;

            // Shortest altitude = 2 * area / longest_edge
            // Aspect ratio = longest_edge / shortest_altitude = longest_edge^2 / (2 * area)
            let aspect_ratio = if area > 1e-10 {
                (longest_edge * longest_edge) / (2.0 * area)
            } else {
                f32::INFINITY // Degenerate triangle
            };

            if aspect_ratio <= max_aspect_ratio {
                valid_indices.extend_from_slice(tri);
            } else {
                removed += 1;
            }
        }

        if removed > 0 {
            debug_log(&format!(
                "[remove_thin_triangles] Removed {} thin triangles (aspect ratio > {})",
                removed, max_aspect_ratio
            ));
            mesh.indices = valid_indices;
        }
    }

    /// Remove edge-connected components that form thin ribbon-like strips
    /// Uses bounding box analysis: if min_extent / max_extent < threshold, remove
    fn remove_thin_components(&self, mesh: &mut CombinedMesh, min_ratio: f32) {
        if mesh.indices.is_empty() {
            return;
        }

        let triangle_count = mesh.indices.len() / 3;
        if triangle_count <= 1 {
            return;
        }

        // Build edge -> triangles mapping for edge-based connectivity
        let mut edge_to_triangles: HashMap<(u32, u32), Vec<usize>> = HashMap::new();

        for (tri_idx, tri) in mesh.indices.chunks(3).enumerate() {
            let (v0, v1, v2) = (tri[0], tri[1], tri[2]);
            for &(a, b) in &[(v0, v1), (v1, v2), (v2, v0)] {
                let edge = (a.min(b), a.max(b));
                edge_to_triangles.entry(edge).or_default().push(tri_idx);
            }
        }

        // Union-Find for edge-connected components
        let mut parent: Vec<usize> = (0..triangle_count).collect();
        let mut rank: Vec<usize> = vec![0; triangle_count];

        fn find(parent: &mut [usize], i: usize) -> usize {
            if parent[i] != i {
                parent[i] = find(parent, parent[i]);
            }
            parent[i]
        }

        fn union(parent: &mut [usize], rank: &mut [usize], a: usize, b: usize) {
            let (ra, rb) = (find(parent, a), find(parent, b));
            if ra != rb {
                if rank[ra] < rank[rb] {
                    parent[ra] = rb;
                } else if rank[ra] > rank[rb] {
                    parent[rb] = ra;
                } else {
                    parent[rb] = ra;
                    rank[ra] += 1;
                }
            }
        }

        for triangles in edge_to_triangles.values() {
            if triangles.len() > 1 {
                let first = triangles[0];
                for &other in &triangles[1..] {
                    union(&mut parent, &mut rank, first, other);
                }
            }
        }

        // Group triangles by component and compute bounding boxes
        let mut component_triangles: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..triangle_count {
            let root = find(&mut parent, i);
            component_triangles.entry(root).or_default().push(i);
        }

        // Find the largest component (main mesh - never remove it)
        let largest_component = component_triangles
            .iter()
            .max_by_key(|(_, tris)| tris.len())
            .map(|(root, _)| *root);

        // Compute bounding box for each component and determine if it's "thin"
        let mut thin_components: std::collections::HashSet<usize> =
            std::collections::HashSet::new();

        for (&root, tri_indices) in &component_triangles {
            // Never remove the largest component
            if Some(root) == largest_component {
                continue;
            }

            // Compute bounding box
            let mut min_bound = [f32::INFINITY; 3];
            let mut max_bound = [f32::NEG_INFINITY; 3];

            for &tri_idx in tri_indices {
                for i in 0..3 {
                    let vi = mesh.indices[tri_idx * 3 + i] as usize;
                    if vi < mesh.vertices.len() {
                        let v = mesh.vertices[vi];
                        for axis in 0..3 {
                            min_bound[axis] = min_bound[axis].min(v[axis]);
                            max_bound[axis] = max_bound[axis].max(v[axis]);
                        }
                    }
                }
            }

            // Compute extents
            let extents = [
                (max_bound[0] - min_bound[0]).max(0.001),
                (max_bound[1] - min_bound[1]).max(0.001),
                (max_bound[2] - min_bound[2]).max(0.001),
            ];

            let max_extent = extents[0].max(extents[1]).max(extents[2]);
            let min_extent = extents[0].min(extents[1]).min(extents[2]);

            let ratio = min_extent / max_extent;

            // If the component is very thin (ribbon-like), mark it for removal
            if ratio < min_ratio {
                thin_components.insert(root);
            }
        }

        if thin_components.is_empty() {
            return;
        }

        // Remove thin components
        let mut valid_indices = Vec::with_capacity(mesh.indices.len());
        let mut removed = 0;

        for (tri_idx, tri) in mesh.indices.chunks(3).enumerate() {
            let root = find(&mut parent, tri_idx);
            if !thin_components.contains(&root) {
                valid_indices.extend_from_slice(tri);
            } else {
                removed += 1;
            }
        }

        if removed > 0 {
            debug_log(&format!(
                "[remove_thin_components] Removed {} triangles in {} thin/ribbon components (ratio < {})",
                removed, thin_components.len(), min_ratio
            ));
            mesh.indices = valid_indices;
        }
    }

    /// Remove vertices that are no longer referenced by any index
    fn remove_unused_vertices(&self, mesh: &mut CombinedMesh) {
        let mut used = vec![false; mesh.vertices.len()];
        for &idx in &mesh.indices {
            used[idx as usize] = true;
        }

        let mut remap: Vec<u32> = vec![0; mesh.vertices.len()];
        let mut new_vertices = Vec::new();
        let mut new_normals = Vec::new();
        let mut new_index = 0u32;

        for (old_idx, &is_used) in used.iter().enumerate() {
            if is_used {
                remap[old_idx] = new_index;
                new_vertices.push(mesh.vertices[old_idx]);
                new_normals.push(mesh.normals[old_idx]);
                new_index += 1;
            }
        }

        for idx in &mut mesh.indices {
            *idx = remap[*idx as usize];
        }

        mesh.vertices = new_vertices;
        mesh.normals = new_normals;
    }

    /// Run full post-processing pipeline
    pub fn process(&self, chunks: &[MeshResult]) -> CombinedMesh {
        let mut mesh = self.merge_chunks(chunks);

        // Average normals at boundary positions BEFORE welding
        // This ensures cross-chunk normal continuity
        self.average_boundary_normals(&mut mesh);

        self.weld_vertices(&mut mesh);
        self.repair_manifold(&mut mesh);

        if self.target_triangle_count.is_some() {
            // When decimating, compute normals AFTER decimation
            // since decimation changes mesh topology
            self.decimate(&mut mesh);
            self.recompute_normals(&mut mesh);
            debug_log("[process] Recomputed normals after decimation");
        } else {
            self.recompute_normals(&mut mesh);
        }

        mesh
    }
}

// Vector math helpers
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Epsilon for near-zero length checks (appropriate for f32 precision)
const NORMAL_EPSILON: f32 = 1e-6;

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > NORMAL_EPSILON {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 1.0, 0.0] // Default up vector for degenerate normals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkCoord;

    fn create_simple_chunk(coord: ChunkCoord, offset: [f32; 3], normal: [f32; 3]) -> MeshResult {
        // Create a simple triangle with the specified offset and normal
        MeshResult {
            coord,
            lod_level: 0,
            vertices: vec![
                [offset[0], offset[1], offset[2]],
                [offset[0] + 1.0, offset[1], offset[2]],
                [offset[0], offset[1] + 1.0, offset[2]],
            ],
            normals: vec![normal, normal, normal],
            indices: vec![0, 1, 2],
            transition_sides: 0,
        }
    }

    #[test]
    fn test_merged_normals_are_averaged_at_boundaries() {
        // Create two chunks that share a boundary vertex at origin
        // Chunk 1: triangle with normal pointing +Z
        // Chunk 2: triangle with normal pointing -Z (opposite)
        // After averaging, the shared vertex should have a blended normal

        let chunk1 = MeshResult {
            coord: ChunkCoord::new(0, 0, 0),
            lod_level: 0,
            vertices: vec![
                [0.0, 0.0, 0.0], // Shared vertex
                [1.0, 0.0, 0.0], // Shared vertex
                [0.0, 1.0, 0.0],
            ],
            normals: vec![
                [0.0, 0.0, 1.0], // +Z normal
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            indices: vec![0, 1, 2],
            transition_sides: 0,
        };

        let chunk2 = MeshResult {
            coord: ChunkCoord::new(1, 0, 0),
            lod_level: 0,
            vertices: vec![
                [0.0, 0.0, 0.0], // Shared vertex (same position as chunk1)
                [1.0, 0.0, 0.0], // Shared vertex
                [0.0, 0.0, -1.0],
            ],
            normals: vec![
                [0.0, 0.0, -1.0], // -Z normal (opposite)
                [0.0, 0.0, -1.0],
                [0.0, 0.0, -1.0],
            ],
            indices: vec![0, 1, 2],
            transition_sides: 0,
        };

        let processor = MeshPostProcessor::new(0.001, 45.0, None);
        let mut merged = processor.merge_chunks(&[chunk1, chunk2]);

        // Before averaging, vertices at same position have different normals
        // After averaging, they should be blended

        processor.average_boundary_normals(&mut merged);

        // Find vertices at [0,0,0] - there should be 2 (one from each chunk)
        let origin_indices: Vec<usize> = merged
            .vertices
            .iter()
            .enumerate()
            .filter(|(_, v)| (v[0].abs() < 0.01) && (v[1].abs() < 0.01) && (v[2].abs() < 0.01))
            .map(|(i, _)| i)
            .collect();

        assert_eq!(origin_indices.len(), 2, "Should have 2 vertices at origin");

        // Both vertices at origin should now have the same averaged normal
        let n0 = merged.normals[origin_indices[0]];
        let n1 = merged.normals[origin_indices[1]];

        // The averaged normal of [0,0,1] and [0,0,-1] is [0,0,0], which normalizes to [0,1,0] (default)
        // OR they might cancel out to zero - check that they're at least equal
        let dot_product = n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2];
        assert!(
            dot_product > 0.99,
            "Boundary normals should be averaged to same value: {:?} vs {:?}",
            n0,
            n1
        );
    }

    #[test]
    fn test_no_degenerate_normals_after_merge() {
        // Merged mesh should have no zero-length or NaN normals
        let chunks = vec![
            create_simple_chunk(ChunkCoord::new(0, 0, 0), [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            create_simple_chunk(ChunkCoord::new(1, 0, 0), [32.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            create_simple_chunk(ChunkCoord::new(0, 0, 1), [0.0, 0.0, 32.0], [0.0, 1.0, 0.0]),
        ];

        let processor = MeshPostProcessor::new(0.001, 45.0, None);
        let merged = processor.process(&chunks);

        for (i, normal) in merged.normals.iter().enumerate() {
            let len = (normal[0].powi(2) + normal[1].powi(2) + normal[2].powi(2)).sqrt();
            assert!(
                len > 0.99 && len < 1.01,
                "Normal {} has invalid length {}: {:?}",
                i,
                len,
                normal
            );
            assert!(
                !normal[0].is_nan() && !normal[1].is_nan() && !normal[2].is_nan(),
                "Normal {} contains NaN: {:?}",
                i,
                normal
            );
        }
    }

    #[test]
    fn test_boundary_normal_averaging_preserves_non_boundary() {
        // Vertices that are NOT at boundaries should keep their original normals
        let chunk1 = MeshResult {
            coord: ChunkCoord::new(0, 0, 0),
            lod_level: 0,
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [5.0, 5.0, 5.0], // Non-boundary vertex
            ],
            normals: vec![
                [0.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0], // Unique normal
            ],
            indices: vec![0, 1, 2],
            transition_sides: 0,
        };

        let processor = MeshPostProcessor::new(0.001, 45.0, None);
        let mut merged = processor.merge_chunks(&[chunk1]);
        let original_unique_normal = merged.normals[3];

        processor.average_boundary_normals(&mut merged);

        // The unique vertex at [5,5,5] should keep its original normal
        assert_eq!(
            merged.normals[3], original_unique_normal,
            "Non-boundary vertex should preserve its normal"
        );
    }

    #[test]
    fn test_process_pipeline_produces_valid_mesh() {
        // The full process() pipeline should produce a valid mesh
        use crate::mesh_extraction::extract_chunk_mesh;
        use crate::noise_field::NoiseField;

        let noise = NoiseField::new(
            42,
            4,
            0.02,
            32.0,
            0.0,
            32.0,
            Some(([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])),
        );

        // Extract a few chunks
        let chunks: Vec<MeshResult> = vec![
            extract_chunk_mesh(&noise, ChunkCoord::new(0, 0, 0), 0, 1.0, 32.0, 0),
            extract_chunk_mesh(&noise, ChunkCoord::new(1, 0, 0), 0, 1.0, 32.0, 0),
            extract_chunk_mesh(&noise, ChunkCoord::new(0, 0, 1), 0, 1.0, 32.0, 0),
        ];

        let processor = MeshPostProcessor::new(0.001, 45.0, None);
        let combined = processor.process(&chunks);

        // Validate the output
        assert!(
            !combined.vertices.is_empty(),
            "Processed mesh should have vertices"
        );
        assert_eq!(
            combined.vertices.len(),
            combined.normals.len(),
            "Vertices and normals count should match"
        );
        assert!(
            combined.indices.len() % 3 == 0,
            "Index count should be multiple of 3"
        );

        // Check all indices are valid
        for &idx in &combined.indices {
            assert!(
                (idx as usize) < combined.vertices.len(),
                "Index {} exceeds vertex count {}",
                idx,
                combined.vertices.len()
            );
        }

        // Check all normals are normalized
        for (i, n) in combined.normals.iter().enumerate() {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!(
                len > 0.99 && len < 1.01,
                "Normal {} not normalized: len={}, {:?}",
                i,
                len,
                n
            );
        }
    }

    #[test]
    fn test_decimation_produces_valid_normals() {
        // Test that decimation followed by normal recomputation produces valid normals
        use crate::mesh_extraction::extract_chunk_mesh;
        use crate::noise_field::NoiseField;

        let noise = NoiseField::new(
            42,
            4,
            0.02,
            32.0,
            0.0,
            32.0,
            Some(([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])),
        );

        // Extract several chunks to get enough triangles for decimation
        let chunks: Vec<MeshResult> = vec![
            extract_chunk_mesh(&noise, ChunkCoord::new(0, 0, 0), 0, 1.0, 32.0, 0),
            extract_chunk_mesh(&noise, ChunkCoord::new(1, 0, 0), 0, 1.0, 32.0, 0),
            extract_chunk_mesh(&noise, ChunkCoord::new(0, 0, 1), 0, 1.0, 32.0, 0),
            extract_chunk_mesh(&noise, ChunkCoord::new(1, 0, 1), 0, 1.0, 32.0, 0),
        ];

        // Count total triangles in input
        let total_tris: usize = chunks.iter().map(|c| c.indices.len() / 3).sum();

        // Only run decimation test if we have enough triangles
        if total_tris > 100 {
            // Target 50% reduction
            let target = total_tris / 2;
            let processor = MeshPostProcessor::new(0.001, 45.0, Some(target));
            let combined = processor.process(&chunks);

            // Validate the decimated output
            assert!(
                !combined.vertices.is_empty(),
                "Decimated mesh should have vertices"
            );
            assert_eq!(
                combined.vertices.len(),
                combined.normals.len(),
                "Vertices and normals count should match after decimation"
            );
            assert!(
                combined.indices.len() % 3 == 0,
                "Index count should be multiple of 3 after decimation"
            );

            // Check all indices are valid
            for &idx in &combined.indices {
                assert!(
                    (idx as usize) < combined.vertices.len(),
                    "Index {} exceeds vertex count {} after decimation",
                    idx,
                    combined.vertices.len()
                );
            }

            // Check all normals are normalized (critical for this fix)
            for (i, n) in combined.normals.iter().enumerate() {
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                assert!(
                    len > 0.99 && len < 1.01,
                    "Normal {} not normalized after decimation: len={}, {:?}",
                    i,
                    len,
                    n
                );
                assert!(
                    !n[0].is_nan() && !n[1].is_nan() && !n[2].is_nan(),
                    "Normal {} contains NaN after decimation: {:?}",
                    i,
                    n
                );
            }

            // Verify decimation actually reduced triangles
            let final_tris = combined.indices.len() / 3;
            assert!(
                final_tris <= target + 10, // Allow small overshoot due to meshopt behavior
                "Decimation should reduce triangle count: {} -> {} (target {})",
                total_tris,
                final_tris,
                target
            );
        }
    }

    #[test]
    fn test_remove_degenerate_triangles() {
        // Test that degenerate triangles are removed
        let mut mesh = CombinedMesh {
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                // Degenerate: all three vertices on same line
                [2.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
                [4.0, 0.0, 0.0],
            ],
            normals: vec![
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            indices: vec![
                0, 1, 2, // Valid triangle
                3, 4, 5, // Degenerate triangle (collinear)
            ],
        };

        let processor = MeshPostProcessor::new(0.001, 45.0, None);
        processor.remove_degenerate_triangles(&mut mesh);

        // Should have removed the degenerate triangle
        assert_eq!(
            mesh.indices.len(),
            3,
            "Should have 1 triangle (3 indices) after removing degenerate"
        );
        assert_eq!(
            mesh.indices,
            vec![0, 1, 2],
            "Should keep only the valid triangle"
        );
    }
}
