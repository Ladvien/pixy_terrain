// MIT License.
// Made by Dylearn — https://github.com/DylearnDev/Dylearn-3D-Pixel-Art-Grass-Demo
// Adapted for Pixy Terrain cloud shadow system

global uniform sampler2D cloud_noise; // Noise for cloud simulation
global uniform float cloud_scale; // Scale of cloud noise
global uniform float cloud_world_y; // Worldspace y height at which clouds are simulated to cast shadow from
global uniform float cloud_speed; // Speed of cloud movement
global uniform float cloud_contrast; // Steepness of noise pattern
global uniform float cloud_threshold; // Brightness of noise pattern
global uniform vec2 cloud_direction; // Direction of cloud movement
global uniform vec3 light_direction; // Directional light direction
global uniform float cloud_shadow_min; // Minimum light value clouds can cast
global uniform float cloud_diverge_angle; // Angle at which the two noises split from each other

vec2 rotate_vec2(vec2 vector, float angle) {
	float angle_deg = angle * (PI / 180.0);
	float rotated_x  = (vector.x * cos(angle_deg)) - (vector.y * sin(angle_deg));
	float rotated_y  = (vector.x * sin(angle_deg)) + (vector.y * cos(angle_deg));
	return vec2 (rotated_x, rotated_y);
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
