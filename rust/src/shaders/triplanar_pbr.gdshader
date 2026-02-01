shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;
// Write 0 to stencil wherever front faces render - clears the 255 written by back faces
stencil_mode write, compare_always, 0;

uniform sampler2D albedo_texture : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D normal_texture : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D roughness_texture : filter_linear_mipmap, repeat_enable;
uniform sampler2D ao_texture : filter_linear_mipmap, repeat_enable;

uniform float triplanar_scale = 1.0;
uniform float triplanar_sharpness = 4.0;
uniform bool use_normal_map = false;
uniform bool use_roughness_map = false;
uniform bool use_ao_map = false;

// Cross-section clipping
uniform bool cross_section_enabled = false;
uniform vec3 clip_plane_position = vec3(0.0, 0.0, 0.0);
uniform vec3 clip_plane_normal = vec3(0.0, 1.0, 0.0);
uniform float clip_offset = 0.0;
uniform bool clip_camera_relative = false;

varying vec3 world_pos;
varying vec3 world_normal;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

vec4 triplanar_sample(sampler2D tex, vec3 pos, vec3 weights, float scale) {
    vec4 x = texture(tex, pos.zy * scale);
    vec4 y = texture(tex, pos.xz * scale);
    vec4 z = texture(tex, pos.xy * scale);
    return x * weights.x + y * weights.y + z * weights.z;
}

void fragment() {
    // Cross-section clipping
    if (cross_section_enabled) {
        vec3 effective_clip_pos = clip_plane_position;
        if (clip_camera_relative) {
            effective_clip_pos += CAMERA_POSITION_WORLD;
        }
        effective_clip_pos += normalize(clip_plane_normal) * clip_offset;

        // Calculate signed distance from fragment to clip plane
        // Discard fragments below the clip plane (negative distance)
        float dist = dot(world_pos - effective_clip_pos, normalize(clip_plane_normal));
        if (dist < 0.0) {
            discard;
        }
    }

    // Calculate triplanar blend weights from normal
    vec3 weights = pow(abs(world_normal), vec3(triplanar_sharpness));
    weights /= (weights.x + weights.y + weights.z);

    // Front face - use terrain texture
    // (Back faces handled by underground_cap.gdshader via next_pass)
    ALBEDO = triplanar_sample(albedo_texture, world_pos, weights, triplanar_scale).rgb;

    // Normal map (optional)
    if (use_normal_map) {
        vec3 n = triplanar_sample(normal_texture, world_pos, weights, triplanar_scale).rgb;
        NORMAL_MAP = n;
    }

    // Roughness (optional)
    if (use_roughness_map) {
        ROUGHNESS = triplanar_sample(roughness_texture, world_pos, weights, triplanar_scale).r;
    } else {
        ROUGHNESS = 0.8;
    }

    // AO (optional)
    if (use_ao_map) {
        AO = triplanar_sample(ao_texture, world_pos, weights, triplanar_scale).r;
    }
}
