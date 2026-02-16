// Pixy Terrain — Flower shader
// Adapted from Eduardo Schildt's 3D Pixel Art Vegetation (MIT License)
// Uses pixy's wind and cloud shadow systems + character displacement

shader_type spatial;
render_mode cull_disabled, specular_disabled;

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
	float rotated_x = (vector.x * cos(angle_deg)) - (vector.y * sin(angle_deg));
	float rotated_y = (vector.x * sin(angle_deg)) + (vector.y * cos(angle_deg));
	return vec2(rotated_x, rotated_y);
}

float get_cloud_noise(vec3 world_pos) {
	float t = (cloud_world_y - world_pos.y) / light_direction.y;
	vec3 hit_pos = world_pos + t * light_direction;
	float inv_scale = 1.0 / cloud_scale;

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

// --- Flower uniforms ---
uniform sampler2D flower_texture : source_color, filter_nearest;
uniform float flower_bendiness = 1.0;
uniform float sway_back : hint_range(0.0, 1.0, 0.01) = 1.0;
uniform float light_steps = 3.0;

// --- Character displacement ---
uniform bool character_displacement = true;
uniform float player_displacement_angle_z : hint_range(0.0, 360.0, 0.1) = 45.0;
uniform float player_displacement_angle_x : hint_range(0.0, 360.0, 0.1) = 45.0;
uniform float radius_exponent : hint_range(0.0, 10.0, 0.001) = 1.0;
const int character_count = 64;
uniform vec4[character_count] character_positions;

varying vec3 node_pos_view;
varying vec3 node_pos_world;
varying vec4 world_pos;

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

void vertex() {
	world_pos = MODEL_MATRIX * vec4(VERTEX, 1.0);
	node_pos_view = NODE_POSITION_VIEW;
	node_pos_world = NODE_POSITION_WORLD;

	// --- Wind animation (pixy system) ---
	float wind = get_wind_noise(world_pos.xyz, TIME);
	float wind_intensity = wind * flower_bendiness;

	// Wind direction in model space
	vec4 direction = inverse(MODEL_MATRIX) * vec4(wind_noise_direction.x, 0.0, wind_noise_direction.y, 0.0);

	// Gravity bend + sway (adapted from Eduardo)
	vec3 noise_vec = vec3(wind);
	noise_vec = noise_vec * (1.0 + sway_back) - sway_back;
	vec3 displace = (noise_vec * direction.xyz - noise_vec * noise_vec * vec3(0.0, 1.0, 0.0) * flower_bendiness);
	VERTEX += displace * wind_intensity;

	// --- Character displacement ---
	if (character_displacement) {
		vec3 object_origin = (MODEL_MATRIX * vec4(0.0, 0.0, 0.0, 1.0)).xyz;
		float char_disp_x = 0.0;
		float char_disp_y = 0.0;
		for (int i = 0; i < character_count; i++) {
			float distance = length(character_positions[i].xyz - object_origin) / character_positions[i].w;
			float strength = pow(1.0 - distance, radius_exponent);

			if (strength > 0.0) {
				strength = clamp(strength, 0.0, 1.0);
				vec3 char_to_flower = object_origin - character_positions[i].xyz;
				float view_y_dot = dot(normalize(CAMERA_DIRECTION_WORLD.xz), normalize(char_to_flower.xz));
				vec3 perp_cam = vec3(CAMERA_DIRECTION_WORLD.z, 0.0, -CAMERA_DIRECTION_WORLD.x);
				float view_x_dot = dot(normalize(perp_cam.xz), normalize(char_to_flower.xz));
				char_disp_x += strength * view_x_dot;
				char_disp_y += strength * view_y_dot;
			}
		}
		char_disp_x = clamp(char_disp_x, -1.0, 1.0);
		char_disp_y = clamp(char_disp_y, -1.0, 1.0);

		mat4 disp_rot =
			rotateAroundAxis(vec3(0.0, 0.0, 1.0), char_disp_x * radians(player_displacement_angle_z))
			* rotateAroundAxis(vec3(1.0, 0.0, 0.0), char_disp_y * radians(player_displacement_angle_x));
		VERTEX = (disp_rot * vec4(VERTEX, 1.0)).xyz;
	}
}

void fragment() {
	vec4 color = texture(flower_texture, UV);

	ALBEDO = color.rgb;
	ALPHA = color.a;
	ALPHA_SCISSOR_THRESHOLD = 0.75;

	SPECULAR = 0.0;
	ROUGHNESS = 0.0; // Outline mask (0.0 = receives outlines)
	LIGHT_VERTEX = node_pos_view; // Shade entire mesh by object position
}

void light() {
	// Cel-shaded diffuse
	float light = clamp(dot(NORMAL, LIGHT), 0.0, 1.0);
	light = round(light * light_steps) / light_steps;

	// Cloud shadow attenuation
	float cloud_shadow = get_cloud_noise(world_pos.xyz);
	light *= cloud_shadow;

	DIFFUSE_LIGHT += light * (LIGHT_COLOR / PI) * ATTENUATION;
}
