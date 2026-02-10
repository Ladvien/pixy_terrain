// Pixy Terrain — Main terrain surface shader
//
// Original shader design by Yugen:
//   https://github.com/Yukitty/Yugens-Terrain-Authoring-Toolkit
//
// Decodes vertex color data into texture selections, blends up to 3 textures
// per cell (or 16 at boundaries), handles floor vs wall rendering with triplanar
// projection for walls, and applies toon-style stepped lighting.
//
// Vertex attribute pipeline (set by replay_geometry() in chunk.rs):
//   COLOR   → ground texture pair 0 (dominant RGBA channel = texture group)
//   CUSTOM0 → ground texture pair 1 (dominant RGBA channel = slot within group)
//   CUSTOM1 → grass mask (R=mask, G=ridge flag)
//   CUSTOM2 → material blend (R=packed mat IDs, G=mat_c, B=weight_a, A=weight_b)
//   UV      → per-cell texture coordinates (0-1)
//   UV2     → world-space position (floor) or world-offset (wall)

shader_type spatial;
// diffuse_toon: stepped lighting bands instead of smooth Lambert
// depth_draw_opaque: write depth buffer for proper occlusion
// cull_disabled: render both sides (walls visible from underneath)
render_mode diffuse_toon, depth_draw_opaque, cull_disabled;

// --- Albedo uniforms ---
// wall_threshold: dot(normal, up) cutoff separating floor from wall geometry
// chunk_size: grid dimensions for UV tiling (textures tile once per chunk)
// cell_size: world units per cell for wall UV projection
// ground_albedo_1-6: tint colors for the first 6 texture slots (color picker in inspector)
group_uniforms Albedo;
uniform float wall_threshold : hint_range(0.0, 0.5) = 0.0;
uniform ivec3 chunk_size = ivec3(33, 32, 33);
uniform vec2 cell_size = vec2(2.0, 2.0);
uniform vec4 ground_albedo   : source_color = vec4(0.392, 0.471, 0.318, 1.0);
uniform vec4 ground_albedo_2 : source_color = vec4(0.322, 0.482, 0.384, 1.0);
uniform vec4 ground_albedo_3 : source_color = vec4(0.373, 0.424, 0.294, 1.0);
uniform vec4 ground_albedo_4 : source_color = vec4(0.392, 0.475, 0.255, 1.0);
uniform vec4 ground_albedo_5 : source_color = vec4(0.29, 0.494, 0.365, 1.0);
uniform vec4 ground_albedo_6 : source_color = vec4(0.443, 0.447, 0.365, 1.0);
group_uniforms;

// --- Blending uniforms ---
// use_hard_textures: pixel-art mode — one texture per cell, no blending
// blend_mode: selects floor rendering path (0=auto, 2=per-vertex index)
// blend_sharpness: transition steepness (low=soft gradient, high=sharp boundary)
// blend_noise: adds procedural noise to blend edges for organic-looking transitions
group_uniforms Blending;
uniform bool use_hard_textures = false;
uniform int blend_mode = 0;
uniform float blend_sharpness : hint_range(0.0, 10.0, 0.1) = 5.0;
uniform float blend_noise_scale : hint_range(0.0, 50.0, 1.0) = 10.0;
uniform float blend_noise_strength : hint_range(0.0, 1.0, 0.05) = 0.0;
group_uniforms;

// --- Texture scale uniforms ---
// Per-texture UV multiplier: 2.0 = tile twice (smaller/more detailed), 0.5 = tile half (bigger)
// GLSL can't index uniforms by variable, so each slot needs its own uniform
group_uniforms Texture_Scales;
uniform float texture_scale_1 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_2 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_3 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_4 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_5 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_6 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_7 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_8 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_9 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_10 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_11 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_12 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_13 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_14 : hint_range(0.1, 20.0, 0.1) = 1.0;
uniform float texture_scale_15 : hint_range(0.1, 20.0, 0.1) = 1.0;
group_uniforms;

// --- 16 texture slots ---
// Naming: vc_tex_XY where X=dominant channel of COLOR, Y=dominant channel of CUSTOM0
// This forms a 4x4 grid:  rr=0, rg=1, rb=2, ra=3, gr=4, ... aa=15
// source_color: apply sRGB-to-linear conversion (correct color space)
// filter_nearest: no bilinear filtering (pixel art stays crisp)
group_uniforms Vertex_Colors;
uniform sampler2D vc_tex_rr : source_color, filter_nearest;
uniform sampler2D vc_tex_rg : source_color, filter_nearest;
uniform sampler2D vc_tex_rb : source_color, filter_nearest;
uniform sampler2D vc_tex_ra : source_color, filter_nearest;
uniform sampler2D vc_tex_gr : source_color, filter_nearest;
uniform sampler2D vc_tex_gg : source_color, filter_nearest;
uniform sampler2D vc_tex_gb : source_color, filter_nearest;
uniform sampler2D vc_tex_ga : source_color, filter_nearest;
uniform sampler2D vc_tex_br : source_color, filter_nearest;
uniform sampler2D vc_tex_bg : source_color, filter_nearest;
uniform sampler2D vc_tex_bb : source_color, filter_nearest;
uniform sampler2D vc_tex_ba : source_color, filter_nearest;
uniform sampler2D vc_tex_ar : source_color, filter_nearest;
uniform sampler2D vc_tex_ag : source_color, filter_nearest;
uniform sampler2D vc_tex_ab : source_color, filter_nearest;
uniform sampler2D vc_tex_aa : source_color, filter_nearest;
group_uniforms;

// --- Toon shading uniforms ---
// shadow_color: tint for shadowed regions (dark blue = cool shadows, brown = warm)
// bands: number of discrete brightness steps (5 = cel-shaded, 10 = nearly smooth)
// shadow_intensity: darkest shadow level (negative = extra dark, positive = lifted)
group_uniforms Shading;
uniform vec4 shadow_color : source_color;
uniform int bands : hint_range(1, 10) = 5;
uniform float shadow_intensity : hint_range(-1.0, 0.5, 0.05) = 0.00;
group_uniforms;

// --- Varyings: vertex → fragment data pipeline ---
// Standard (GPU-interpolated across triangle for smooth gradients):
varying vec3 vertex_normal;   // Surface normal
varying vec4 custom1;         // CUSTOM1 passthrough (R=grass mask, G=ridge flag)
varying vec3 world_pos;       // World-space position (for noise-based blend variation)
varying vec4 vc_color_0;      // COLOR passthrough (texture group selection)
varying vec4 vc_color_1;      // CUSTOM0 passthrough (texture slot selection)
varying vec2 mat_weights;     // Blend weights from CUSTOM2 (smooth across triangle)
varying float use_vertex_colors; // Flag: >0.5 = boundary cell, use 16-weight blending

// Flat (NOT interpolated — same value for every pixel in the triangle):
varying flat int material_index; // Decoded texture index (integer, would be nonsense if interpolated)
varying flat vec3 mat_indices;   // 3 packed material IDs from CUSTOM2 (discrete indices, not continuous)

// --- Constants ---
const int NUM_TEXTURES = 16;              // Total texture slots in the 4x4 vertex color grid
const int VOID_TEXTURE = 15;              // Slot 15 = transparent/void (no tint, no scale override)
const float EPSILON = 0.001;              // Near-zero: normalization guard, snap tolerance
const float CHANNEL_THRESHOLD = 0.1;      // Vertex color channel detection (low to survive GPU interpolation)
const float WEIGHT_SKIP_THRESHOLD = 0.01; // Skip texture samples below this weight (perf optimization)
const float SHARPNESS_BASE = 2.0;         // Base exponent for blend weight power curve
const float SHARPNESS_SCALE = 2.0;        // Multiplier: power = BASE + sharpness * SCALE
const float RIDGE_THRESHOLD = 0.5;        // CUSTOM1.g above this = ridge vertex (rendered as wall)
const float VERTEX_COLOR_FLAG = 1.5;      // CUSTOM2.a above this = use 16-weight vertex color blending
const float CUSTOM2_UNPACK = 255.0;       // CUSTOM2.r was packed as (mat_a + mat_b * 16) / 255
const float MAT_PACK_STRIDE = 16.0;       // Material B is packed at stride 16 within CUSTOM2.r
const float MAT_INDEX_SCALE = 15.0;       // Material indices normalized to 0..1 as index/15
const float TOON_LIGHT_MAX = 0.3;         // Brightest toon light step (mix target)

// --- Noise: procedural value noise for blend boundary variation ---

// Pseudo-random hash from 2D position. The magic constants are standard
// "good enough" shader hash values (not cryptographic, just visually random).
float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

// Smooth value noise via bilinear interpolation of hashed grid corners.
// Smoothstep (f*f*(3-2f)) removes visible grid lines at cell boundaries.
float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f); // smoothstep curve
	float a = hash(i);
	float b = hash(i + vec2(1.0, 0.0));
	float c = hash(i + vec2(0.0, 1.0));
	float d = hash(i + vec2(1.0, 1.0));
	return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// --- Material index decoder ---
// Inverse of texture_index_to_colors() from Rust (Part 03).
// Reads two vertex colors, finds dominant channel in each, returns texture index 0-15.
// Threshold is CHANNEL_THRESHOLD (0.1), not 0.5, because GPU interpolation creates
// intermediate values at triangle boundaries — first dominant channel always wins.
int get_material_index(vec4 vc_col_0, vec4 vc_col_1) {
	int index = 0;
	if (vc_col_0.r > CHANNEL_THRESHOLD) {
		if (vc_col_1.r > CHANNEL_THRESHOLD) index = 0;
		else if (vc_col_1.g > CHANNEL_THRESHOLD) index = 1;
		else if (vc_col_1.b > CHANNEL_THRESHOLD) index = 2;
		else if (vc_col_1.a > CHANNEL_THRESHOLD) index = 3;
	}
	else if (vc_col_0.g > CHANNEL_THRESHOLD) {
		if (vc_col_1.r > CHANNEL_THRESHOLD) index = 4;
		else if (vc_col_1.g > CHANNEL_THRESHOLD) index = 5;
		else if (vc_col_1.b > CHANNEL_THRESHOLD) index = 6;
		else if (vc_col_1.a > CHANNEL_THRESHOLD) index = 7;
	}
	else if (vc_col_0.b > CHANNEL_THRESHOLD) {
		if (vc_col_1.r > CHANNEL_THRESHOLD) index = 8;
		else if (vc_col_1.g > CHANNEL_THRESHOLD) index = 9;
		else if (vc_col_1.b > CHANNEL_THRESHOLD) index = 10;
		else if (vc_col_1.a > CHANNEL_THRESHOLD) index = 11;
	}
	else if (vc_col_0.a > CHANNEL_THRESHOLD) {
		if (vc_col_1.r > CHANNEL_THRESHOLD) index = 12;
		else if (vc_col_1.g > CHANNEL_THRESHOLD) index = 13;
		else if (vc_col_1.b > CHANNEL_THRESHOLD) index = 14;
		else if (vc_col_1.a > CHANNEL_THRESHOLD) index = 15;
	}
	return index;
}

// --- Texture scale lookup ---
// Maps texture index to its per-slot scale uniform.
// Must be a switch because GLSL can't index into uniforms by variable.
float get_texture_scale(int index) {
	switch(index) {
		case 0: return texture_scale_1;
		case 1: return texture_scale_2;
		case 2: return texture_scale_3;
		case 3: return texture_scale_4;
		case 4: return texture_scale_5;
		case 5: return texture_scale_6;
		case 6: return texture_scale_7;
		case 7: return texture_scale_8;
		case 8: return texture_scale_9;
		case 9: return texture_scale_10;
		case 10: return texture_scale_11;
		case 11: return texture_scale_12;
		case 12: return texture_scale_13;
		case 13: return texture_scale_14;
		case 14: return texture_scale_15;
		default: return 1.0;
	}
}

// --- Core texture sampler ---
// Fetches a texture by index, applies per-slot UV scaling.
// Slots 0-5: multiplied by ground_albedo tint (one grass texture, 6 color variations)
// Slots 6-14: untinted (render as-is from texture)
// Slot 15 (VOID_TEXTURE): transparent, no scale override
vec4 sample_material_by_index(int index, vec2 base_uv) {
	float scale = 1.0;
	if (index != VOID_TEXTURE)
		scale = get_texture_scale(index);
	vec2 uv = base_uv * scale;
	vec4 result = vec4(0.0);
	switch(index) {
		case 0: result = texture(vc_tex_rr, uv) * ground_albedo; break;
		case 1: result = texture(vc_tex_rg, uv) * ground_albedo_2; break;
		case 2: result = texture(vc_tex_rb, uv) * ground_albedo_3; break;
		case 3: result = texture(vc_tex_ra, uv) * ground_albedo_4; break;
		case 4: result = texture(vc_tex_gr, uv) * ground_albedo_5; break;
		case 5: result = texture(vc_tex_gg, uv) * ground_albedo_6; break;
		case 6: result = texture(vc_tex_gb, uv); break;
		case 7: result = texture(vc_tex_ga, uv); break;
		case 8: result = texture(vc_tex_br, uv); break;
		case 9: result = texture(vc_tex_bg, uv); break;
		case 10: result = texture(vc_tex_bb, uv); break;
		case 11: result = texture(vc_tex_ba, uv); break;
		case 12: result = texture(vc_tex_ar, uv); break;
		case 13: result = texture(vc_tex_ag, uv); break;
		case 14: result = texture(vc_tex_ab, uv); break;
		case 15: result = texture(vc_tex_aa, uv); break;
		default: result = texture(vc_tex_rr, uv) * ground_albedo; break;
	}
	return result;
}

// Wall texture sampler — separate function so wall rendering could be customized
// independently (different scale, tiling, etc.) without touching floor path.
vec4 sample_wall_by_index(int index, vec2 uv) {
	return sample_material_by_index(index, uv);
}

// --- 16-weight blend engine ---
// Computes blend weights for all 16 texture slots from two vertex colors.
// Used for boundary cells where multiple textures meet.
//
// Step 1: Raw weight = product of channel pairs (vc0.r*vc1.r, vc0.r*vc1.g, etc.)
//         GPU interpolation of vertex colors across the triangle creates the
//         intermediate values that drive smooth blending.
// Step 2: Power-sharpen weights. pow(weight, power) makes big weights bigger
//         and small weights smaller. sharpness=0 → power=2 (soft), sharpness=5 → power=12 (hard).
// Step 3: Normalize so weights sum to 1.0.
// Fallback: If all weights are near-zero after power (extreme sharpness), pick the
//           single largest raw weight to avoid divide-by-zero artifacts.
void calculate_blend_weights(vec4 vc0, vec4 vc1, float sharpness, out float weights[16]) {
	float raw_weights[16];
	raw_weights[0]  = vc0.r * vc1.r;
	raw_weights[1]  = vc0.r * vc1.g;
	raw_weights[2]  = vc0.r * vc1.b;
	raw_weights[3]  = vc0.r * vc1.a;
	raw_weights[4]  = vc0.g * vc1.r;
	raw_weights[5]  = vc0.g * vc1.g;
	raw_weights[6]  = vc0.g * vc1.b;
	raw_weights[7]  = vc0.g * vc1.a;
	raw_weights[8]  = vc0.b * vc1.r;
	raw_weights[9]  = vc0.b * vc1.g;
	raw_weights[10] = vc0.b * vc1.b;
	raw_weights[11] = vc0.b * vc1.a;
	raw_weights[12] = vc0.a * vc1.r;
	raw_weights[13] = vc0.a * vc1.g;
	raw_weights[14] = vc0.a * vc1.b;
	raw_weights[15] = vc0.a * vc1.a;

	float power = SHARPNESS_BASE + sharpness * SHARPNESS_SCALE;
	float total = 0.0;

	for (int i = 0; i < NUM_TEXTURES; i++) {
		weights[i] = pow(max(raw_weights[i], 0.0), power);
		total += weights[i];
	}

	if (total > EPSILON) {
		for (int i = 0; i < NUM_TEXTURES; i++) {
			weights[i] /= total;
		}
	} else {
		// Fallback: all weights crushed to zero by high sharpness.
		// Pick the single strongest raw weight instead of dividing by ~0.
		float max_raw = 0.0;
		int max_idx = 0;
		for (int i = 0; i < NUM_TEXTURES; i++) {
			if (raw_weights[i] > max_raw) {
				max_raw = raw_weights[i];
				max_idx = i;
			}
		}
		for (int i = 0; i < NUM_TEXTURES; i++) {
			weights[i] = (i == max_idx) ? 1.0 : 0.0;
		}
	}
}

// --- Wall snap helper ---
// Forces a vertex color to its single dominant channel.
// Fights GPU interpolation bleed: (0.6, 0.4, 0, 0) → (1, 0, 0, 0).
// Floors benefit from smooth interpolation; walls need crisp material boundaries
// because blurry transitions look like rendering bugs on vertical surfaces.
vec4 snap_to_dominant(vec4 c) {
	float max_val = max(max(c.r, c.g), max(c.b, c.a));
	if (max_val < EPSILON) return vec4(1.0, 0.0, 0.0, 0.0); // fallback to texture 0
	vec4 result = vec4(0.0);
	if (c.r >= max_val - EPSILON) result.r = 1.0;
	else if (c.g >= max_val - EPSILON) result.g = 1.0;
	else if (c.b >= max_val - EPSILON) result.b = 1.0;
	else result.a = 1.0;
	return result;
}

// --- Wall blend pipeline ---
// Like floor blending but with snapped vertex colors for crisp material boundaries.
// Samples only textures with meaningful weight (> WEIGHT_SKIP_THRESHOLD) to avoid
// wasting GPU cycles on texture fetches that contribute < 1% to the final color.
vec4 blend_wall_materials(vec2 uv, vec4 vc0, vec4 vc1, float sharpness) {
	vec4 snapped_vc0 = snap_to_dominant(vc0);
	vec4 snapped_vc1 = snap_to_dominant(vc1);

	float weights[16];
	calculate_blend_weights(snapped_vc0, snapped_vc1, sharpness, weights);

	vec4 wall_color = vec4(0.0);
	float total_weight = 0.0;

	for (int i = 0; i < NUM_TEXTURES; i++) {
		if (weights[i] > WEIGHT_SKIP_THRESHOLD) {
			wall_color += sample_material_by_index(i, uv) * weights[i];
			total_weight += weights[i];
		}
	}

	if (total_weight > EPSILON) {
		wall_color /= total_weight;
	}
	return wall_color;
}

// --- Vertex shader ---
// Unpacks vertex attributes into varyings for the fragment shader.
// Runs once per vertex; the GPU interpolates varyings across each triangle.
void vertex() {
	// Pass through surface normal, grass/ridge data, and world position
	vertex_normal = NORMAL;
	custom1 = CUSTOM1;
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;

	// Decode texture index from the two vertex colors (flat = no interpolation)
	material_index = get_material_index(COLOR, CUSTOM0);

	// Pass vertex colors through for the 16-weight blending path
	vc_color_0 = COLOR;
	vc_color_1 = CUSTOM0;

	// Unpack CUSTOM2: material blend data encoded by calculate_material_blend_data() in Rust
	// R channel = two material indices packed as (mat_a + mat_b * 16) / 255
	float packed_mats = CUSTOM2.r * CUSTOM2_UNPACK;
	mat_indices.x = mod(packed_mats, MAT_PACK_STRIDE) / MAT_INDEX_SCALE;  // mat_a
	mat_indices.y = floor(packed_mats / MAT_PACK_STRIDE) / MAT_INDEX_SCALE; // mat_b
	mat_indices.z = CUSTOM2.g;  // mat_c (already normalized to 0..1)

	// B and A channels = blend weights for 3-texture "phantom fix" path
	mat_weights = vec2(CUSTOM2.b, CUSTOM2.a);

	// Flag: CUSTOM2.a >= 1.5 means this is a boundary cell — use the expensive
	// 16-weight vertex color blending instead of the fast 3-texture path.
	// Rust sets weight_b to 2.0 for boundary cells; normal cells have weight_b < 1.0.
	use_vertex_colors = CUSTOM2.a >= VERTEX_COLOR_FLAG ? 1.0 : 0.0;
}

// --- Fragment shader ---
// Runs once per pixel. Decides floor vs wall, picks a rendering path, outputs color.
// Floor has 3 paths (fast→slow): hard textures, phantom fix (3 tex), vertex color (16 tex).
// Wall uses biplanar projection with snapped vertex colors for crisp boundaries.
void fragment() {
	// Pixels with alpha < 0.5 are discarded (makes void texture slot 15 invisible)
	ALPHA_SCISSOR_THRESHOLD = 0.5;

	// UV tiling: textures tile once across the full chunk dimensions
	vec2 tiling_factor_floor = vec2(1.0 / float(chunk_size.x), 1.0 / float(chunk_size.z));
	vec2 tiling_factor_wall = vec2(1.0 / float(chunk_size.x), 1.0 / float(chunk_size.y));

	// Floor vs wall classification:
	// Floor = normal points up (dot > threshold) AND not a ridge vertex
	float is_ridge = custom1.g;
	bool is_floor = dot(vertex_normal, vec3(0.0, 1.0, 0.0)) > wall_threshold && is_ridge < RIDGE_THRESHOLD;

	if (is_floor) {
		// Floor UV from world-space position (UV2), scaled to tile once per chunk
		vec2 floor_uv = UV2 * tiling_factor_floor;
		vec4 floor_color;

		// === PATH 1: Hard textures (cheapest) ===
		// One texture per cell, no blending. Pixel-perfect boundaries.
		if (use_hard_textures) {
			if (blend_mode == 2)
				// Per-vertex material index (flat varying = same for whole triangle)
				floor_color = sample_material_by_index(material_index, floor_uv);
			else
				// Per-cell dominant material from CUSTOM2 packed data
				floor_color = sample_material_by_index(int(round(mat_indices.x * MAT_INDEX_SCALE)), floor_uv);

		// === PATH 2: Vertex color blending (most expensive) ===
		// Boundary cells where heights differ across corners. Samples up to 16 textures.
		} else if (use_vertex_colors > 0.5) {
			float weights[16];
			float effective_sharpness = blend_sharpness;

			// Optional noise: vary sharpness per-pixel for organic blend edges
			if (blend_noise_strength > 0.0) {
				float n = noise(world_pos.xz * blend_noise_scale);
				effective_sharpness = mix(blend_sharpness, blend_sharpness * n, blend_noise_strength);
			}
			calculate_blend_weights(vc_color_0, vc_color_1, effective_sharpness, weights);

			// Accumulate weighted texture samples (skip near-zero weights for perf)
			floor_color = vec4(0.0);
			for (int i = 0; i < NUM_TEXTURES; i++) {
				if (weights[i] > WEIGHT_SKIP_THRESHOLD) {
					floor_color += sample_material_by_index(i, floor_uv) * weights[i];
				}
			}

		// === PATH 3: Phantom fix blending (default, fast) ===
		// Flat cells. Uses 3 texture IDs + 2 weights from CUSTOM2. Max 3 samples.
		} else {
			
			// Decode the 3 material indices back to integer texture slots
			int mat_a = int(round(mat_indices.x * MAT_INDEX_SCALE));
			int mat_b = int(round(mat_indices.y * MAT_INDEX_SCALE));
			int mat_c = int(round(mat_indices.z * MAT_INDEX_SCALE));

			// Weight C is derived: the three weights must sum to 1.0
			float weight_a = mat_weights.x;
			float weight_b = mat_weights.y;
			float weight_c = 1.0 - weight_a - weight_b;

			// Clamp and normalize (GPU interpolation can produce slightly negative values)
			weight_a = max(0.0, weight_a);
			weight_b = max(0.0, weight_b);
			weight_c = max(0.0, weight_c);
			float weight_sum = weight_a + weight_b + weight_c;
			if (weight_sum > EPSILON) {
				weight_a /= weight_sum;
				weight_b /= weight_sum;
				weight_c /= weight_sum;
			}

			// Optional noise: shift weights per-pixel for organic blend edges
			if (blend_noise_strength > 0.0) {
				float n = noise(world_pos.xz * blend_noise_scale);
				float noise_offset = (n - 0.5) * blend_noise_strength;
				weight_a = clamp(weight_a + noise_offset, 0.0, 1.0);
				weight_b = clamp(weight_b - noise_offset * 0.5, 0.0, 1.0);
				weight_c = clamp(weight_c - noise_offset * 0.5, 0.0, 1.0);
				float total = weight_a + weight_b + weight_c;
				if (total > EPSILON) {
					weight_a /= total;
					weight_b /= total;
					weight_c /= total;
				}
			}

			// Optional sharpness: power-curve weights for harder transitions
			if (blend_sharpness > 0.0) {
				float power = 1.0 + blend_sharpness;
				weight_a = pow(weight_a, power);
				weight_b = pow(weight_b, power);
				weight_c = pow(weight_c, power);
				float total = weight_a + weight_b + weight_c;
				if (total > EPSILON) {
					weight_a /= total;
					weight_b /= total;
					weight_c /= total;
				}				
			}

			// Sample 3 textures and blend by weight
			vec4 color_a = sample_material_by_index(mat_a, floor_uv);
			vec4 color_b = sample_material_by_index(mat_b, floor_uv);
			vec4 color_c = sample_material_by_index(mat_c, floor_uv);
			floor_color = color_a * weight_a + color_b * weight_b + color_c * weight_c;
		}

		ALBEDO = floor_color.rgb;
		ALPHA = floor_color.a;

	// === Wall rendering: biplanar projection ===
	// Project textures from X and Z axes, blend by normal direction.
	// Y axis omitted since walls are always vertical. Prevents stretching on angled walls.
	} else {
		// Convert view-space vertex to world-space for consistent UV projection
		vec4 vertex_pos = INV_VIEW_MATRIX * vec4(VERTEX, 1.0);
		vec3 abs_normal = abs(vertex_normal);

		// Biplanar weights: how much does this surface face X vs Z?
		vec3 tri_weights = vec3(abs_normal.x, 0.0, abs_normal.z);
		tri_weights /= (tri_weights.x + tri_weights.z);

		// Two UV projections: ZY plane (X-facing walls), XY plane (Z-facing walls)
		vec2 uv_x = (vertex_pos.zy / cell_size.yx) * tiling_factor_wall;
		vec2 uv_z = (vertex_pos.xy / cell_size) * tiling_factor_wall;

		vec4 wall_mat_x, wall_mat_z;

		// Wall blending uses snapped vertex colors for crisp material boundaries
		float effective_sharpness = blend_sharpness;
		if (use_hard_textures) {
			wall_mat_x = blend_wall_materials(uv_x, vc_color_0, vc_color_1, effective_sharpness);
			wall_mat_z = blend_wall_materials(uv_z, vc_color_0, vc_color_1, effective_sharpness);
		} else {
			// Optional noise: vary sharpness for organic wall blend edges
			if (blend_noise_strength > 0.0) {
				float n = noise(world_pos.xz * blend_noise_scale);
				effective_sharpness = mix(blend_sharpness, blend_sharpness * n, blend_noise_strength);
			}
			wall_mat_x = blend_wall_materials(uv_x, vc_color_0, vc_color_1, effective_sharpness);
			wall_mat_z = blend_wall_materials(uv_z, vc_color_0, vc_color_1, effective_sharpness);
		}

		// Blend the two projections by normal direction weight
		vec3 texture_x = wall_mat_x.rgb * tri_weights.x;
		vec3 texture_z = wall_mat_z.rgb * tri_weights.z;

		ALBEDO = texture_x + texture_z;
		ALPHA = wall_mat_x.a;
	}
}

// --- Toon light shader ---
// Called once per light per pixel. Replaces smooth PBR with discrete cel-shading bands.
void light() {
	// Raw light intensity: Lambert (normal · light_dir) scaled by attenuation + shadows
	float NdotL = dot(NORMAL, LIGHT) * ATTENUATION;

	// Quantize to discrete steps: 5 bands → snaps to 0.0, 0.2, 0.4, 0.6, 0.8
	float stepped = floor(NdotL * float(bands)) / float(bands);

	// Blend between tinted shadow and full albedo based on stepped brightness
	float light_amount = max(stepped + shadow_intensity, 0.0);
	vec3 lit_color = mix(shadow_color.rgb * ALBEDO, ALBEDO, light_amount);

	// Accumulate across lights; TOON_LIGHT_MAX caps each contribution to prevent overbrightening
	DIFFUSE_LIGHT += lit_color * LIGHT_COLOR * TOON_LIGHT_MAX;
}