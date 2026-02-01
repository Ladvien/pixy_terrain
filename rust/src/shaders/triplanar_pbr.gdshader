shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2D albedo_texture : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D normal_texture : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D roughness_texture : filter_linear_mipmap, repeat_enable;
uniform sampler2D ao_texture : filter_linear_mipmap, repeat_enable;

uniform float triplanar_scale = 1.0;
uniform float triplanar_sharpness = 4.0;
uniform bool use_normal_map = false;
uniform bool use_roughness_map = false;
uniform bool use_ao_map = false;

varying vec3 world_pos;
varying vec3 world_normal;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

vec4 triplanar_sample(sampler2D tex, vec3 pos, vec3 weights) {
    vec4 x = texture(tex, pos.zy * triplanar_scale);
    vec4 y = texture(tex, pos.xz * triplanar_scale);
    vec4 z = texture(tex, pos.xy * triplanar_scale);
    return x * weights.x + y * weights.y + z * weights.z;
}

void fragment() {
    // Calculate triplanar blend weights from normal
    vec3 weights = pow(abs(world_normal), vec3(triplanar_sharpness));
    weights /= (weights.x + weights.y + weights.z);

    // Albedo
    ALBEDO = triplanar_sample(albedo_texture, world_pos, weights).rgb;

    // Normal map (optional)
    if (use_normal_map) {
        vec3 n = triplanar_sample(normal_texture, world_pos, weights).rgb;
        NORMAL_MAP = n;
    }

    // Roughness (optional)
    if (use_roughness_map) {
        ROUGHNESS = triplanar_sample(roughness_texture, world_pos, weights).r;
    } else {
        ROUGHNESS = 0.8;
    }

    // AO (optional)
    if (use_ao_map) {
        AO = triplanar_sample(ao_texture, world_pos, weights).r;
    }
}
