shader_type spatial;
// Pass 3: Stencil-masked cap rendering
// Only renders where stencil == 255 (exposed interior visible but no front face)
// Renders flat cap with underground texture using triplanar mapping
render_mode cull_disabled, depth_test_disabled, unshaded;
stencil_mode read, compare_equal, 255;

// Clipping uniforms (shared with main shader)
// Note: clip_plane_position, clip_offset, clip_camera_relative are set by Rust
// but not used here - the stencil test is sufficient for cap rendering
uniform bool cross_section_enabled = false;
uniform vec3 clip_plane_position = vec3(0.0);
uniform vec3 clip_plane_normal = vec3(0.0, 1.0, 0.0);
uniform float clip_offset = 0.0;
uniform bool clip_camera_relative = false;

// Underground textures for cap surface
uniform sampler2D underground_texture : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D underground_normal_texture : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D underground_roughness_texture : filter_linear_mipmap, repeat_enable;
uniform sampler2D underground_ao_texture : filter_linear_mipmap, repeat_enable;
uniform float underground_triplanar_scale = 1.0;
uniform float triplanar_sharpness = 4.0;
uniform bool use_underground_normal_map = false;
uniform bool use_underground_roughness_map = false;
uniform bool use_underground_ao_map = false;

varying vec3 world_pos;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

vec4 triplanar_sample(sampler2D tex, vec3 pos, vec3 weights, float scale) {
    vec4 x = texture(tex, pos.zy * scale);
    vec4 y = texture(tex, pos.xz * scale);
    vec4 z = texture(tex, pos.xy * scale);
    return x * weights.x + y * weights.y + z * weights.z;
}

void fragment() {
    // If cross-section disabled, don't render cap
    if (!cross_section_enabled) {
        discard;
    }

    // The stencil test (compare_equal 255) already ensures we only render
    // on exposed interior surfaces where back-faces are visible.
    // No clip plane discard needed here - that's handled by stencil_write.
    vec3 clip_normal = normalize(clip_plane_normal);

    // For the cap, we use the clip plane normal as the surface normal
    // This creates a flat cap perpendicular to the clip direction
    NORMAL = clip_normal;

    // Calculate triplanar weights from clip plane normal
    // This gives consistent texture mapping on the flat cap
    vec3 weights = pow(abs(clip_normal), vec3(triplanar_sharpness));
    weights /= (weights.x + weights.y + weights.z);

    // Sample underground texture using world position for continuity
    vec3 tex_color = triplanar_sample(underground_texture, world_pos, weights, underground_triplanar_scale).rgb;
    // Fallback to earthy brown if texture is unbound/black
    if (length(tex_color) < 0.01) {
        tex_color = vec3(0.4, 0.25, 0.1);
    }
    ALBEDO = tex_color;

    // Normal map (optional)
    if (use_underground_normal_map) {
        vec3 n = triplanar_sample(underground_normal_texture, world_pos, weights, underground_triplanar_scale).rgb;
        NORMAL_MAP = n;
    }

    // Roughness
    if (use_underground_roughness_map) {
        ROUGHNESS = triplanar_sample(underground_roughness_texture, world_pos, weights, underground_triplanar_scale).r;
    } else {
        ROUGHNESS = 0.8;
    }

    // AO
    if (use_underground_ao_map) {
        AO = triplanar_sample(underground_ao_texture, world_pos, weights, underground_triplanar_scale).r;
    }
}
