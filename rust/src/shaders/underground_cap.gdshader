shader_type spatial;
render_mode cull_front, depth_draw_always, diffuse_burley, specular_schlick_ggx;

// Clipping uniforms (shared with main shader)
uniform bool cross_section_enabled = false;
uniform vec3 clip_plane_position = vec3(0.0);
uniform vec3 clip_plane_normal = vec3(0.0, 1.0, 0.0);
uniform float clip_offset = 0.0;
uniform bool clip_camera_relative = false;

// Underground textures
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
varying vec3 world_normal;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
    // Bias to prevent z-fighting with exterior surface
    VERTEX -= NORMAL * 0.001;
}

vec4 triplanar_sample(sampler2D tex, vec3 pos, vec3 weights, float scale) {
    vec4 x = texture(tex, pos.zy * scale);
    vec4 y = texture(tex, pos.xz * scale);
    vec4 z = texture(tex, pos.xy * scale);
    return x * weights.x + y * weights.y + z * weights.z;
}

void fragment() {
    if (!cross_section_enabled) {
        discard;
    }

    vec3 effective_clip_pos = clip_plane_position;
    if (clip_camera_relative) {
        effective_clip_pos += CAMERA_POSITION_WORLD;
    }
    effective_clip_pos += normalize(clip_plane_normal) * clip_offset;

    vec3 clip_normal = normalize(clip_plane_normal);
    float dist = dot(world_pos - effective_clip_pos, clip_normal);
    if (dist < 0.0) {
        discard;
    }

    // Triplanar weights from world normal
    vec3 weights = pow(abs(world_normal), vec3(triplanar_sharpness));
    weights /= (weights.x + weights.y + weights.z);

    // Underground texture
    ALBEDO = triplanar_sample(underground_texture, world_pos, weights, underground_triplanar_scale).rgb;

    // Flip normal for correct lighting on back faces
    NORMAL = -NORMAL;

    // Normal map
    if (use_underground_normal_map) {
        vec3 n = triplanar_sample(underground_normal_texture, world_pos, weights, underground_triplanar_scale).rgb;
        NORMAL_MAP = n;
        NORMAL_MAP_DEPTH = -1.0;
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
