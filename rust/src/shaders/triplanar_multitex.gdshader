shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;
// Write 0 to stencil wherever front faces render - clears the 255 written by back faces
stencil_mode write, compare_always, 0;

// Texture arrays for up to 4 terrain texture sets
uniform sampler2D albedo_texture_0 : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D albedo_texture_1 : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D albedo_texture_2 : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D albedo_texture_3 : source_color, filter_linear_mipmap, repeat_enable;

uniform sampler2D normal_texture_0 : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D normal_texture_1 : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D normal_texture_2 : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D normal_texture_3 : hint_normal, filter_linear_mipmap, repeat_enable;

uniform sampler2D roughness_texture_0 : filter_linear_mipmap, repeat_enable;
uniform sampler2D roughness_texture_1 : filter_linear_mipmap, repeat_enable;
uniform sampler2D roughness_texture_2 : filter_linear_mipmap, repeat_enable;
uniform sampler2D roughness_texture_3 : filter_linear_mipmap, repeat_enable;

// Texture enable flags
uniform bool use_texture_0 = true;
uniform bool use_texture_1 = false;
uniform bool use_texture_2 = false;
uniform bool use_texture_3 = false;

uniform bool use_normal_map_0 = false;
uniform bool use_normal_map_1 = false;
uniform bool use_normal_map_2 = false;
uniform bool use_normal_map_3 = false;

uniform bool use_roughness_map_0 = false;
uniform bool use_roughness_map_1 = false;
uniform bool use_roughness_map_2 = false;
uniform bool use_roughness_map_3 = false;

// Triplanar settings
uniform float triplanar_scale = 1.0;
uniform float triplanar_sharpness = 4.0;

// Cross-section clipping
uniform bool cross_section_enabled = false;
uniform vec3 clip_plane_position = vec3(0.0, 0.0, 0.0);
uniform vec3 clip_plane_normal = vec3(0.0, 1.0, 0.0);
uniform float clip_offset = 0.0;
uniform bool clip_camera_relative = false;

varying vec3 world_pos;
varying vec3 world_normal;
varying vec4 texture_weights; // From vertex color (RGBA = weights for 4 textures)

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);

    // Read texture weights from vertex color (set by mesh extraction)
    texture_weights = COLOR;
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
    vec3 tri_weights = pow(abs(world_normal), vec3(triplanar_sharpness));
    tri_weights /= (tri_weights.x + tri_weights.y + tri_weights.z);

    // Normalize texture weights to ensure they sum to 1.0
    vec4 tex_w = texture_weights;
    float weight_sum = tex_w.r + tex_w.g + tex_w.b + tex_w.a;
    if (weight_sum > 0.001) {
        tex_w /= weight_sum;
    } else {
        // Default to texture 0 if no weights
        tex_w = vec4(1.0, 0.0, 0.0, 0.0);
    }

    // Sample and blend albedo from all active textures
    vec3 final_albedo = vec3(0.0);
    vec3 final_normal = vec3(0.5, 0.5, 1.0); // Default flat normal
    float final_roughness = 0.8;

    // Texture 0
    if (use_texture_0 && tex_w.r > 0.001) {
        vec3 albedo_0 = triplanar_sample(albedo_texture_0, world_pos, tri_weights, triplanar_scale).rgb;
        final_albedo += albedo_0 * tex_w.r;

        if (use_normal_map_0) {
            vec3 normal_0 = triplanar_sample(normal_texture_0, world_pos, tri_weights, triplanar_scale).rgb;
            final_normal = mix(final_normal, normal_0, tex_w.r);
        }

        if (use_roughness_map_0) {
            float rough_0 = triplanar_sample(roughness_texture_0, world_pos, tri_weights, triplanar_scale).r;
            final_roughness = mix(final_roughness, rough_0, tex_w.r);
        }
    }

    // Texture 1
    if (use_texture_1 && tex_w.g > 0.001) {
        vec3 albedo_1 = triplanar_sample(albedo_texture_1, world_pos, tri_weights, triplanar_scale).rgb;
        final_albedo += albedo_1 * tex_w.g;

        if (use_normal_map_1) {
            vec3 normal_1 = triplanar_sample(normal_texture_1, world_pos, tri_weights, triplanar_scale).rgb;
            final_normal = mix(final_normal, normal_1, tex_w.g);
        }

        if (use_roughness_map_1) {
            float rough_1 = triplanar_sample(roughness_texture_1, world_pos, tri_weights, triplanar_scale).r;
            final_roughness = mix(final_roughness, rough_1, tex_w.g);
        }
    }

    // Texture 2
    if (use_texture_2 && tex_w.b > 0.001) {
        vec3 albedo_2 = triplanar_sample(albedo_texture_2, world_pos, tri_weights, triplanar_scale).rgb;
        final_albedo += albedo_2 * tex_w.b;

        if (use_normal_map_2) {
            vec3 normal_2 = triplanar_sample(normal_texture_2, world_pos, tri_weights, triplanar_scale).rgb;
            final_normal = mix(final_normal, normal_2, tex_w.b);
        }

        if (use_roughness_map_2) {
            float rough_2 = triplanar_sample(roughness_texture_2, world_pos, tri_weights, triplanar_scale).r;
            final_roughness = mix(final_roughness, rough_2, tex_w.b);
        }
    }

    // Texture 3
    if (use_texture_3 && tex_w.a > 0.001) {
        vec3 albedo_3 = triplanar_sample(albedo_texture_3, world_pos, tri_weights, triplanar_scale).rgb;
        final_albedo += albedo_3 * tex_w.a;

        if (use_normal_map_3) {
            vec3 normal_3 = triplanar_sample(normal_texture_3, world_pos, tri_weights, triplanar_scale).rgb;
            final_normal = mix(final_normal, normal_3, tex_w.a);
        }

        if (use_roughness_map_3) {
            float rough_3 = triplanar_sample(roughness_texture_3, world_pos, tri_weights, triplanar_scale).r;
            final_roughness = mix(final_roughness, rough_3, tex_w.a);
        }
    }

    ALBEDO = final_albedo;
    NORMAL_MAP = final_normal;
    ROUGHNESS = final_roughness;
}
