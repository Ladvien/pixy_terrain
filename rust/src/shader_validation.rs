/// Shader regression tests.
///
/// These tests read the actual .gdshader files from disk and verify critical
/// patterns haven't been accidentally reverted. Shaders are loaded at runtime
/// by Godot (not embedded in Rust), so these are the only automated guard
/// against shader regressions.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    /// Return absolute path to the terrain shader file.
    fn terrain_shader_path() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.join("../godot/addons/pixy_terrain/resources/shaders/mst_terrain.gdshader")
    }

    fn read_terrain_shader() -> String {
        let path = terrain_shader_path();
        fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("Failed to read terrain shader at {}: {}", path.display(), e)
        })
    }

    // ---------------------------------------------------------------
    // Reverse-Z depth bias regression guard
    //
    // Godot 4 Forward+ (Vulkan) uses reverse-Z depth buffer:
    //   near plane → z_ndc = 1.0 (larger z = closer to camera)
    //   far plane  → z_ndc = 0.0 (smaller z = farther from camera)
    //
    // To push walls *behind* floors, we must SUBTRACT from clip_pos.z,
    // which decreases z_ndc → farther from camera in reverse-Z.
    //
    // Using += would increase z_ndc → walls appear CLOSER to camera,
    // causing walls to visually overlay floors at cliff edges.
    // ---------------------------------------------------------------

    #[test]
    fn test_wall_depth_bias_subtracts_in_reverse_z() {
        let shader = read_terrain_shader();

        // The correct line: subtract bias to push walls farther in reverse-Z
        assert!(
            shader.contains("clip_pos.z -= wall_depth_bias * clip_pos.w"),
            "REGRESSION: wall depth bias must SUBTRACT (`-=`) in reverse-Z depth buffer. \
             Using `+=` causes walls to render in front of floors at cliff edges. \
             See mst_terrain.gdshader vertex() function."
        );

        // Guard against the specific wrong version being present
        assert!(
            !shader.contains("clip_pos.z += wall_depth_bias * clip_pos.w"),
            "REGRESSION: found `+=` for wall depth bias — this is WRONG for reverse-Z. \
             Godot 4 Forward+ uses reverse-Z (near=1.0, far=0.0). Adding to clip_pos.z \
             moves walls CLOSER to camera, not farther. Must use `-=`."
        );
    }

    #[test]
    fn test_depth_bias_only_applied_to_walls() {
        let shader = read_terrain_shader();

        // The bias must be gated by the floor flag so floors keep natural depth
        assert!(
            shader.contains("if (is_floor_flag < 0.5)"),
            "Wall depth bias must be conditional on is_floor_flag (CUSTOM1.b). \
             Floors (flag=1.0) should NOT be biased — only walls (flag=0.0)."
        );
    }

    #[test]
    fn test_floor_flag_read_from_custom1_b() {
        let shader = read_terrain_shader();

        // CUSTOM1.b carries the authoritative floor/wall flag set by Rust
        assert!(
            shader.contains("float is_floor_flag = CUSTOM1.b"),
            "Floor flag must be read from CUSTOM1.b (set by replay_geometry in chunk.rs). \
             Do not derive floor/wall classification from normals or other heuristics."
        );
    }

    #[test]
    fn test_wall_depth_bias_uniform_exists() {
        let shader = read_terrain_shader();

        assert!(
            shader.contains("uniform float wall_depth_bias"),
            "wall_depth_bias must be a uniform so it's tunable from the inspector."
        );
    }
}
