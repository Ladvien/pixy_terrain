// Pixy Terrain — Grass cross-mesh shader
//
// Renders MultiMesh grass sprites on 3-quad cross-meshes (star pattern).
// Each instance carries a ground color (INSTANCE_CUSTOM.rgb) and
// a sprite ID (INSTANCE_CUSTOM.a) to select from 6 grass textures.
//
// Original 6-texture grass pipeline by Yugen:
//   https://github.com/Yukitty/Yugens-Terrain-Authoring-Toolkit
//
// Wind system, character displacement, toon lighting, and cloud shadows
// adapted from Dylearn 3D Pixel Art Grass Demo (MIT License):
//   https://github.com/DylearnDev/Dylearn-3D-Pixel-Art-Grass-Demo

shader_type spatial;
render_mode diffuse_toon, depth_draw_opaque, cull_disabled;

// --- Cloud shadow system (global uniforms set by PixyEnvironment) ---
global uniform sampler2D cloud_noise;
global uniform float cloud_scale;
global uniform float cloud_world_y;
global uniform float cloud_speed;
global uniform float cloud_contrast;
global uniform float cloud_threshold;
global uniform vec2 cloud_direction;
global uniform vec3 light_direction;
global uniform float cloud_shadow_min;
global uniform float cloud_diverge_angle;

// --- Wind system (global uniforms set by PixyEnvironment) ---
global uniform sampler2D wind_noise;
global uniform float wind_noise_threshold;
global uniform float wind_noise_scale;
global uniform float wind_noise_speed;
global uniform vec2 wind_noise_direction;
global uniform float wind_diverge_angle;

vec2 rotate_vec2(vec2 vector, float angle) {
	float angle_deg = angle * (PI / 180.0);
	float rotated_x  = (vector.x * cos(angle_deg)) - (vector.y * sin(angle_deg));
	float rotated_y  = (vector.x * sin(angle_deg)) + (vector.y * cos(angle_deg));
	return vec2(rotated_x, rotated_y);
}

float get_cloud_noise(vec3 world_pos) {
	float t = (cloud_world_y - world_pos.y) / light_direction.y;
	vec3 hit_pos = world_pos + t * light_direction;
	float inv_scale = 1.0 / cloud_scale;

	// Non-repeating clouds via dual-noise scroll
	vec2 cloud_direction1 = rotate_vec2(cloud_direction, cloud_diverge_angle);
	vec2 cloud_direction2 = rotate_vec2(cloud_direction, -cloud_diverge_angle);
	vec2 cloud_time_direction1 = TIME * cloud_speed * normalize(cloud_direction1);
	vec2 cloud_time_direction2 = TIME * cloud_speed * normalize(cloud_direction2);
	float cloud_sample1 = texture(cloud_noise, hit_pos.xz * inv_scale + cloud_time_direction1).r;
	float cloud_sample2 = texture(cloud_noise, hit_pos.xz * (inv_scale * 0.8) + (cloud_time_direction2 * 0.89 * PI / 3.0)).r;
	float cloud_sample = (cloud_sample1 * cloud_sample2);
	float light_value = clamp(cloud_sample + cloud_threshold, 0.0, 1.0);

	light_value = (light_value - 0.5) * cloud_contrast + 0.5;
	light_value = clamp(light_value + cloud_threshold, cloud_shadow_min, 1.0);
	return light_value;
}

float get_wind_noise(vec3 world_pos, float time) {
	vec2 dir1 = rotate_vec2(wind_noise_direction, wind_diverge_angle);
	vec2 dir2 = rotate_vec2(wind_noise_direction, -wind_diverge_angle);
	vec2 time_dir1 = time * wind_noise_speed * normalize(dir1);
	vec2 time_dir2 = time * wind_noise_speed * normalize(dir2);
	float s1 = texture(wind_noise, world_pos.xz * wind_noise_scale + time_dir1).r;
	float s2 = texture(wind_noise, world_pos.xz * (wind_noise_scale * 0.8) + (time_dir2 * 0.89 * PI / 3.0)).r;
	float combined = s1 * s2;
	combined = clamp(combined + wind_noise_threshold, 0.0, 1.0);
	return (combined - 0.5) * 2.0;
}

// --- Primary grass texture + color ---
group_uniforms Albedo;
uniform sampler2D grass_texture : source_color, filter_nearest;
uniform vec3 grass_base_color : source_color = vec3(0.392, 0.471, 0.318);
uniform float wall_threshold : hint_range(0.0, 0.5) = 0.0;
uniform bool use_base_color = false;
uniform bool is_merge_round = false;
group_uniforms;

// --- Extra grass textures ---
group_uniforms extra_grass;
uniform sampler2D grass_texture_2 : source_color, filter_nearest;
uniform bool use_grass_tex_2 = true;
uniform bool use_base_color_2 = false;
uniform vec3 grass_color_2 : source_color = vec3(0.322, 0.482, 0.384);
uniform sampler2D grass_texture_3 : source_color, filter_nearest;
uniform bool use_grass_tex_3 = true;
uniform bool use_base_color_3 = false;
uniform vec3 grass_color_3 : source_color = vec3(0.373, 0.424, 0.294);
uniform sampler2D grass_texture_4 : source_color, filter_nearest;
uniform bool use_grass_tex_4 = true;
uniform bool use_base_color_4 = false;
uniform vec3 grass_color_4 : source_color = vec3(0.392, 0.475, 0.255);
uniform sampler2D grass_texture_5 : source_color, filter_nearest;
uniform bool use_grass_tex_5 = true;
uniform bool use_base_color_5 = false;
uniform vec3 grass_color_5 : source_color = vec3(0.29, 0.494, 0.365);
uniform sampler2D grass_texture_6 : source_color, filter_nearest;
uniform bool use_grass_tex_6 = true;
uniform bool use_base_color_6 = false;
uniform vec3 grass_color_6 : source_color = vec3(0.443, 0.447, 0.365);
group_uniforms;

// --- Animation uniforms (Dylearn-based) ---
group_uniforms Animation;
uniform float framerate = 5.0;
uniform bool quantised = true;
	group_uniforms Animation.world_space;
uniform bool world_space_sway = true;
uniform float world_sway_angle : hint_range(0.0, 180.0) = 60.0;
	group_uniforms Animation.view_space;
uniform bool view_space_sway = true;
uniform float view_sway_speed : hint_range(0.0, 5.0) = 0.1;
uniform float view_sway_angle : hint_range(0.0, 45.0) = 10.0;
	group_uniforms character_displacement;
uniform bool character_displacement = true;
uniform float player_displacement_angle_z : hint_range(0.0, 360.0, 0.1) = 45.0;
uniform float player_displacement_angle_x : hint_range(0.0, 360.0, 0.1) = 45.0;
uniform float radius_exponent : hint_range(0.0, 10.0, 0.001) = 1.0;
const int character_count = 64;
uniform vec4[character_count] character_positions;

// --- Toon lighting uniforms (Dylearn-based) ---
// --- Cross-section uniforms ---
group_uniforms Cross_Section;
uniform bool cross_section_enabled = false;
uniform vec3 clip_origin = vec3(0.0, 100.0, 0.0);
group_uniforms;

group_uniforms Lighting;
uniform int cuts : hint_range(1, 8) = 3;
uniform float wrap : hint_range(-2.0, 2.0) = 0.0;
uniform float steepness : hint_range(1.0, 8.0) = 1.0;
uniform float threshold_gradient_size = 0.2;
uniform vec4 shadow_color : source_color;
group_uniforms;

// --- Varyings ---
varying vec3 model_origin;
varying vec3 object_origin;
varying vec3 world_pos;
varying vec3 instance_color;
varying float instance_sprite_id;
varying float wind_noise_sample;

// --- Helper functions ---
float location_seed(vec2 location) {
	return fract(sin(dot(location, vec2(12.9898, 78.233))) * 43758.5453123);
}

float random(float n) {
	return fract(sin(n * 12.9898) * 43758.5453);
}

mat4 rotateAroundAxis(vec3 axis, float angle) {
	axis = normalize(axis);
	float c = cos(angle);
	float s = sin(angle);
	float oc = 1.0 - c;
	return mat4(
		vec4(oc * axis.x * axis.x + c,
			oc * axis.x * axis.y - axis.z * s,
			oc * axis.z * axis.x + axis.y * s,
			0.0),
		vec4(oc * axis.x * axis.y + axis.z * s,
			oc * axis.y * axis.y + c,
			oc * axis.y * axis.z - axis.x * s,
			0.0),
		vec4(oc * axis.z * axis.x - axis.y * s,
			oc * axis.y * axis.z + axis.x * s,
			oc * axis.z * axis.z + c,
			0.0),
		vec4(0.0, 0.0, 0.0, 1.0)
	);
}

mat4 view_space_rotate(float angle) {
	return mat4(
		vec4(cos(angle), -sin(angle), 0.0, 0.0),
		vec4(sin(angle), cos(angle), 0.0, 0.0),
		vec4(0.0, 0.0, 1.0, 0.0),
		vec4(0.0, 0.0, 0.0, 1.0)
	);
}

void vertex() {
	// Instance data from MultiMesh
	instance_color = INSTANCE_CUSTOM.rgb;
	instance_sprite_id = INSTANCE_CUSTOM.a;

	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	object_origin = (MODEL_MATRIX * vec4(0.0, 0.0, 0.0, 1.0)).xyz;

	// Per-instance phase offset for quantized time
	float seed = 10.0 * location_seed(object_origin.xy);
	float frametime = 1.0 / framerate;
	float phase = mod(seed, frametime);

	float time = TIME;
	if (quantised) {
		time += phase;
		time = round(time * framerate) / framerate;
	}

	// --- Wind sampling (from pixy_environment) ---
	wind_noise_sample = get_wind_noise(object_origin, time);

	// --- World-space sway (wind) ---
	float world_sway = wind_noise_sample * radians(world_sway_angle);
	vec3 wind_world_axis = (vec4(wind_noise_direction.y, 0.0, -wind_noise_direction.x, 1.0) * INV_VIEW_MATRIX).xyz;
	mat4 world_rotation = rotateAroundAxis(wind_world_axis, world_sway);

	// --- View-space sway (gentle idle) ---
	float model_sway = sin((time + seed) * view_sway_speed * 2.0 * PI) * radians(view_sway_angle);
	mat4 model_rotation = view_space_rotate(model_sway);

	// --- Character displacement ---
	float character_displacement_x = 0.0;
	float character_displacement_y = 0.0;
	for (int i = 0; i < character_count; i++) {
		float distance = length(character_positions[i].xyz - object_origin) / character_positions[i].w;
		float displacement_strength = pow(1.0 - distance, radius_exponent);

		if (displacement_strength > 0.0) {
			displacement_strength = clamp(displacement_strength, 0.0, 1.0);
			vec3 character_to_grass = object_origin - character_positions[i].xyz;
			float view_y_dot = dot(normalize(CAMERA_DIRECTION_WORLD.xz), normalize(character_to_grass.xz));
			vec3 perpendicular_camera_direction_world = vec3(CAMERA_DIRECTION_WORLD.z, 0.0, -CAMERA_DIRECTION_WORLD.x);
			float view_x_dot = dot(normalize(perpendicular_camera_direction_world.xz), normalize(character_to_grass.xz));
			character_displacement_x += displacement_strength * view_x_dot;
			character_displacement_y += displacement_strength * view_y_dot;
		}
	}
	character_displacement_x = clamp(character_displacement_x, -1.0, 1.0);
	character_displacement_y = clamp(character_displacement_y, -1.0, 1.0);

	mat4 player_displacement_rotation =
		rotateAroundAxis(vec3(0.0, 0.0, 1.0), character_displacement_x * radians(player_displacement_angle_z))
		* rotateAroundAxis(vec3(1.0, 0.0, 0.0), character_displacement_y * radians(player_displacement_angle_x));

	// --- Apply rotations conditionally ---
	if (view_space_sway) {
		VERTEX = (model_rotation * vec4(VERTEX, 1.0)).xyz;
	}
	if (character_displacement) {
		VERTEX = (player_displacement_rotation * vec4(VERTEX, 1.0)).xyz;
	}
	if (world_space_sway) {
		VERTEX = (world_rotation * vec4(VERTEX, 1.0)).xyz;
	}

	// --- Per-instance random Y rotation (breaks visual uniformity) ---
	float y_seed = location_seed(object_origin.xz);
	float y_angle = y_seed * 6.28318;
	VERTEX = (rotateAroundAxis(vec3(0.0, 1.0, 0.0), y_angle) * vec4(VERTEX, 1.0)).xyz;
	model_origin = (MODELVIEW_MATRIX * vec4(0.0, 0.0, 0.0, 1.0)).xyz;
}

void fragment() {
	vec2 uv = UV;

	// 6-slot texture selection from instance_sprite_id (0.0..1.0 mapped to slots 1-6)
	vec4 grass_tex = texture(grass_texture, uv);
	vec3 grass_color = use_base_color ? grass_base_color : instance_color * grass_base_color;
	int tex_id = 1;

	if (instance_sprite_id > 0.9) {
		grass_color = use_base_color_6 ? grass_color_6 : instance_color * grass_color_6;
		tex_id = 6;
	} else if (instance_sprite_id > 0.7) {
		grass_color = use_base_color_5 ? grass_color_5 : instance_color * grass_color_5;
		tex_id = 5;
	} else if (instance_sprite_id > 0.5) {
		grass_color = use_base_color_4 ? grass_color_4 : instance_color * grass_color_4;
		tex_id = 4;
	} else if (instance_sprite_id > 0.3) {
		grass_color = use_base_color_3 ? grass_color_3 : instance_color * grass_color_3;
		tex_id = 3;
	} else if (instance_sprite_id > 0.1) {
		grass_color = use_base_color_2 ? grass_color_2 : instance_color * grass_color_2;
		tex_id = 2;
	}

	ALBEDO = grass_color;

	// Sample alpha from the correct sprite texture
	if (tex_id == 2) {
		ALPHA = use_grass_tex_2 ? texture(grass_texture_2, uv).a : 0.0;
	} else if (tex_id == 3) {
		ALPHA = use_grass_tex_3 ? texture(grass_texture_3, uv).a : 0.0;
	} else if (tex_id == 4) {
		ALPHA = use_grass_tex_4 ? texture(grass_texture_4, uv).a : 0.0;
	} else if (tex_id == 5) {
		ALPHA = use_grass_tex_5 ? texture(grass_texture_5, uv).a : 0.0;
	} else if (tex_id == 6) {
		ALPHA = use_grass_tex_6 ? texture(grass_texture_6, uv).a : 0.0;
	} else {
		ALPHA = grass_tex.a;
	}

	// Strict alpha cutoff (Dylearn approach)
	ALPHA_SCISSOR_THRESHOLD = 0.5;

	// Cross-section: discard grass above the player from the camera's perspective
	if (cross_section_enabled) {
		vec3 cam_up = normalize(INV_VIEW_MATRIX[1].xyz);
		if (dot(world_pos - clip_origin, cam_up) > 0.0) {
			discard;
		}
	}

	// Flat lighting: use instance origin so the whole cross-mesh shades uniformly
	LIGHT_VERTEX = model_origin;
}

// --- Hybrid toon light shader (Dylearn-based) ---
void light() {
	float NdotL = dot(NORMAL, LIGHT);
	float diffuse_amount = NdotL + (ATTENUATION - 1.0) + wrap;
	diffuse_amount *= steepness;

	// Cloud shadows on directional light
	if (LIGHT_IS_DIRECTIONAL) {
		vec3 vertex_world_pos = (INV_VIEW_MATRIX * vec4(model_origin, 1.0)).xyz;
		float light_value = get_cloud_noise(vertex_world_pos);
		diffuse_amount = min(diffuse_amount, light_value);
	}

	// Toon banding with smooth threshold gradients
	float cuts_inv = 1.0 / float(cuts);
	float cut = cuts_inv;
	float original_index = ceil(diffuse_amount * float(cuts));
	float original_stepped = clamp(original_index * cut, 0.0, 1.0);
	float diffuse_stepped = clamp(diffuse_amount + mod(1.0 - diffuse_amount, cuts_inv), 0.0, 1.0);

	if (threshold_gradient_size > 0.0) {
		float nearest_k = floor(diffuse_amount / cut + 0.5);
		float threshold = nearest_k * cut;

		if (nearest_k >= 0.0 && nearest_k <= float(cuts)) {
			float halfWidth = 0.5 * cut * threshold_gradient_size;
			float low  = max(0.0, threshold - halfWidth);
			float high = min(1.0, threshold + halfWidth);

			float blend = 0.0;
			if (high > low) {
				blend = smoothstep(low, high, diffuse_amount);
			} else {
				blend = step(threshold, diffuse_amount);
			}

			float leftValue  = threshold;
			float rightValue = min(threshold + (cut * 1.0), 1.0);
			diffuse_stepped = mix(leftValue, rightValue, blend);
			diffuse_stepped = clamp(diffuse_stepped, 0.0, 1.0);
		} else {
			diffuse_stepped = original_stepped;
		}
	}

	vec3 diffuse = ALBEDO.rgb * LIGHT_COLOR / PI;
	diffuse *= diffuse_stepped;
	DIFFUSE_LIGHT = max(DIFFUSE_LIGHT, diffuse);
}
