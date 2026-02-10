// Pixy Terrain — Grass billboard shader
//
// Renders MultiMesh grass sprites as camera-facing billboards.
// Each instance carries a ground color (INSTANCE_CUSTOM.rgb) and
// a sprite ID (INSTANCE_CUSTOM.a) to select from 6 grass textures.
// Supports wind scrolling or per-sprite pixel-art sway animation.

shader_type spatial;
render_mode diffuse_toon, depth_draw_opaque;

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

// --- Animation uniforms ---
group_uniforms Animation;
uniform sampler2D wind_texture : source_color;
uniform vec2 wind_direction = vec2(1.0, 1.0);
uniform float wind_scale = 0.1;
uniform float wind_speed = 1.0;
uniform bool animate_active = true;
uniform float fps : hint_range(0.0, 30.0, 1.0) = 0.0;
group_uniforms;

// --- Toon shading uniforms ---
group_uniforms Shading;
uniform vec4 shadow_color : source_color;
uniform int bands : hint_range(1, 10) = 5;
uniform float shadow_intensity : hint_range(-1.0, 0.5, 0.05) = 0.0;
group_uniforms;

// --- Varyings ---
varying vec3 model_origin;
varying vec3 vertex_normal;
varying vec3 instance_color;
varying float instance_sprite_id;
varying flat int instance_id;
varying float wind_color;

void vertex() {
	// --- Animation: only affects top vertices (Y > 0 = grass tips) ---
	if (animate_active && VERTEX.y > 0.0) {
		float left_to_right_mult = 0.2;

		if (fps > 0.0) {
			// Pixel-art sway: quantize time to discrete frames per sprite
			float seed = dot(vec2(VERTEX.xz), vec2(12.9898, 78.233)) + float(INSTANCE_ID);
			float time_offset = fract(sin(seed) * 43758.5453);
			float quantized_time = floor(TIME * fps * time_offset) / fps;
			float sway = sin(quantized_time) * left_to_right_mult;
			VERTEX.x += sway;
		}
		else {
			// Wind: scroll a noise texture across world space, bend grass by its value
			float wind_time = TIME * wind_speed;
			vec2 scroll = wind_time * wind_direction;
			vec3 world_pos = MODEL_MATRIX[3].xyz;
			vec2 wind_uv = (world_pos.xy * wind_scale) + scroll;
			float wind_value = texture(wind_texture, wind_uv).r;
			wind_color = wind_value;
			float wind = wind_value * left_to_right_mult;
			float bend_factor = clamp(VERTEX.y, 0.0, 1.0);
			VERTEX += vec3(wind, 0.0, 0.0) * bend_factor;
		}
	}

	// --- Spherical billboarding: rotate quad to face camera ---
	vec3 cam_z = normalize(CAMERA_DIRECTION_WORLD);
	vec3 cam_y = vec3(0.0, 1.0, 0.0);
	vec3 cam_x = normalize(cross(cam_y, cam_z));
	mat3 spherical_billboard = mat3(cam_x, cam_y, cam_z);
	vec3 billboarded_vertex = spherical_billboard * VERTEX;

	// Position in world: instance origin + billboarded local vertex
	vec3 instance_origin = (MODEL_MATRIX * vec4(0.0, 0.0, 0.0, 1.0)).xyz;
	vec3 world_position = instance_origin + billboarded_vertex;
	POSITION = PROJECTION_MATRIX * VIEW_MATRIX * vec4(world_position, 1.0);

	// Pass instance data to fragment
	model_origin = (MODELVIEW_MATRIX * vec4(0.0, 0.0, 0.0, 1.0)).xyz;
	instance_color = INSTANCE_CUSTOM.rgb;
	instance_sprite_id = INSTANCE_CUSTOM.a;
	instance_id = INSTANCE_ID;

	vec3 world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
	vertex_normal = world_normal;
}

// --- STUB fragment (replace as you type) ---
void fragment() {
     ALPHA_SCISSOR_THRESHOLD = 0.5;                                                                

    // Start with primary grass texture + color
    vec4 grass_tex = texture(grass_texture, UV);
    vec3 grass_color = use_base_color ? grass_base_color : instance_color * grass_base_color;

    // Select sprite variant by instance_sprite_id (0.0..1.0 mapped to slots 1-6)
    int tex_id = 1;
    if (instance_sprite_id > 0.9) {
            grass_color = use_base_color_6 ? grass_color_6 : instance_color * grass_color_6;
            tex_id = 6;
    }
    else if (instance_sprite_id > 0.7) {
            grass_color = use_base_color_5 ? grass_color_5 : instance_color * grass_color_5;
            tex_id = 5;
    }
    else if (instance_sprite_id > 0.5) {
            grass_color = use_base_color_4 ? grass_color_4 : instance_color * grass_color_4;
            tex_id = 4;
    }
    else if (instance_sprite_id > 0.3) {
            grass_color = use_base_color_3 ? grass_color_3 : instance_color * grass_color_3;
            tex_id = 3;
    }
    else if (instance_sprite_id > 0.1) {
            grass_color = use_base_color_2 ? grass_color_2 : instance_color * grass_color_2;
            tex_id = 2;
    }
    ALBEDO = grass_color;

    // Floor-only rendering: hide grass on walls (and adjust for merge rounds)
    float w_threshold = wall_threshold;
    if (is_merge_round && wall_threshold <= 0.0)
            w_threshold -= 1.5;
    if (dot(vertex_normal, vec3(0.0, 1.0, 0.0)) > w_threshold) {
            // Sample alpha from the correct sprite texture
            if (tex_id == 2) {
                    if (use_grass_tex_2) ALPHA = texture(grass_texture_2, UV).a;
                    else ALPHA = 0.0;
            }
            else if (tex_id == 3) {
                    if (use_grass_tex_3) ALPHA = texture(grass_texture_3, UV).a;
                    else ALPHA = 0.0;
            }
            else if (tex_id == 4) {
                    if (use_grass_tex_4) ALPHA = texture(grass_texture_4, UV).a;
                    else ALPHA = 0.0;
            }
            else if (tex_id == 5) {
                    if (use_grass_tex_5) ALPHA = texture(grass_texture_5, UV).a;
                    else ALPHA = 0.0;
            }
            else if (tex_id == 6) {
                    if (use_grass_tex_6) ALPHA = texture(grass_texture_6, UV).a;
                    else ALPHA = 0.0;
            }
            else
                    ALPHA = grass_tex.a;
    }
    else
            ALPHA = 0.0;

    // Hard discard for near-transparent pixels
    if (ALPHA < 0.1) discard;

    // Flat lighting: use instance origin so the whole billboard shades uniformly
    LIGHT_VERTEX = model_origin;
}

                                                                                                
// --- Toon light shader ---                                                                        
// Same cel-shading as terrain but uses ceil() instead of floor()
// for slightly brighter grass (ceil rounds up, floor rounds down).
void light() {
    float NdotL = dot(NORMAL, LIGHT);
    NdotL = clamp(NdotL, 0.0, 1.0);

    // Quantize with ceil (grass gets the higher band, slightly brighter than terrain)
    float stepped = ceil(NdotL * float(bands)) / float(bands);
    float toon_light = mix(shadow_intensity, 0.3, stepped);

    toon_light *= ATTENUATION;

    // Mix shadow color with light color (not albedo — albedo is applied by Godot)
    vec3 light_color = mix(shadow_color.rgb, LIGHT_COLOR.rgb, toon_light);

    DIFFUSE_LIGHT += light_color;
}