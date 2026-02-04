use crate::chunk::MeshResult;

const DECIMATION_TARGET_ERROR: f32 = 1e-2;

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
    pub normal_angle_threshold: f32,
    pub target_triangle_count: Option<usize>,
}

impl MeshPostProcessor {
    pub fn new(
        normal_angle_threshold: f32,
        target_triangle_count: Option<usize>,
    ) -> Self {
        Self {
            normal_angle_threshold,
            target_triangle_count,
        }
    }

    /// Merge multiple chunk meshes into a single combined mesh
    pub fn merge_chunks(&self, chunks: &[MeshResult]) -> CombinedMesh {
        let mut combined = CombinedMesh::new();

        for chunk in chunks {
            let base_index = combined.vertices.len() as u32;
            let chunk_vertex_count = chunk.vertices.len() as i32;

            // Copy vertices and normals
            combined.vertices.extend_from_slice(&chunk.vertices);
            combined.normals.extend_from_slice(&chunk.normals);

            // Offset and copy indices, validating triangles
            for tri in chunk.indices.chunks(3) {
                if tri.len() != 3 {
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
                    continue;
                }

                combined.indices.push(base_index + i0 as u32);
                combined.indices.push(base_index + i1 as u32);
                combined.indices.push(base_index + i2 as u32);
            }
        }

        combined
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

        let target_indices = target * 3;
        if target_indices >= mesh.indices.len() {
            return; // Already at or below target
        }

        // Convert vertices to flat array for meshopt
        let vertex_data: Vec<f32> = mesh
            .vertices
            .iter()
            .flat_map(|v| v.iter().copied())
            .collect();

        let mut new_indices: Vec<u32> = vec![0; mesh.indices.len()];

        let result_count = unsafe {
            meshopt::ffi::meshopt_simplify(
                new_indices.as_mut_ptr(),
                mesh.indices.as_ptr(),
                mesh.indices.len(),
                vertex_data.as_ptr(),
                mesh.vertices.len(),
                std::mem::size_of::<[f32; 3]>(),
                target_indices,
                DECIMATION_TARGET_ERROR,
                0,    // options
                std::ptr::null_mut(),
            )
        };

        new_indices.truncate(result_count);
        mesh.indices = new_indices;

        // Remove unused vertices after decimation
        self.remove_unused_vertices(mesh);
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

        self.weld_vertices(&mut mesh);
        self.repair_manifold(&mut mesh);
        self.recompute_normals(&mut mesh);

        if self.target_triangle_count.is_some() {
            self.decimate(&mut mesh);
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
