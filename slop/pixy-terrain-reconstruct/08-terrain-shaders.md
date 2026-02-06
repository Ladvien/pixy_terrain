# Pixy Terrain — Part 08: Terrain Shaders

**Series:** Reconstructing Pixy Terrain
**Part:** 08 of 18
**Previous:** 2026-02-06-chunk-mesh-generation-07.md
**Status:** Complete

## What We're Building

The four GLSL shaders that make the terrain visible: the main terrain shader (16-texture triplanar blending, toon lighting), the grass shader (billboarded sprites with wind animation), and two brush preview shaders. These are pure Godot shader files — no Rust code in this part.

## What You'll Have After This

A complete visual pipeline: terrain meshes render with toon-shaded, multi-texture blending; grass billboards wave in the wind; and the editor brush shows a translucent preview overlay. The shaders consume the vertex attributes that the Rust mesh generation (Part 07) produces.

## Prerequisites

- Part 07 completed (chunk mesh generation with CUSTOM0-2 vertex attributes)
- Understanding of Godot's shader language (similar to GLSL ES 3.0)

## Context: How Vertex Data Reaches the Shader

Before diving into the shaders, here's how the vertex attributes set in `replay_geometry()` (Part 07) map to shader inputs:

| SurfaceTool call | Shader built-in | Contains |
|---|---|---|
| `set_color()` | `COLOR` | Ground texture pair 0 (RGBA = which of 4 major groups) |
| `set_custom(0, ...)` | `CUSTOM0` | Ground texture pair 1 (RGBA = which slot within the group) |
| `set_custom(1, ...)` | `CUSTOM1` | Grass mask (R=mask, G=ridge flag) |
| `set_custom(2, ...)` | `CUSTOM2` | Material blend (R=packed mat IDs, G=mat_c, B=weight_a, A=weight_b) |
| `set_uv()` | `UV` | Standard texture coordinates (0-1 per cell) |
| `set_uv2()` | `UV2` | World-space position (floor) or world-offset (wall) |

The two vertex colors (`COLOR` × `CUSTOM0`) encode a 4×4 = 16-texture selection system. The dominant RGBA channel in each determines a texture index (see Part 03, `texture_index_to_colors()`).

## Steps

### Step 1: Create the terrain shader

**Why:** This is the visual core of the terrain. It decodes vertex color data into texture selections, blends up to 3 textures per cell, handles floor vs wall rendering differently (triplanar projection for walls), and applies toon-style stepped lighting.

**File:** `godot/resources/shaders/mst_terrain.gdshader`

```glsl
shader_type spatial;
render_mode diffuse_toon, depth_draw_opaque, alpha_to_coverage, cull_disabled;

group_uniforms Albedo;
uniform float wall_threshold : hint_range(0.0, 0.5) = 0.0;
uniform ivec3 chunk_size = ivec3(33, 32, 33);
uniform vec2 cell_size = vec2(2.0, 2.0);
uniform vec4 ground_albedo : source_color = vec4(0.392, 0.471, 0.318, 1.0);
uniform vec4 ground_albedo_2 : source_color = vec4(0.322, 0.482, 0.384, 1.0);
uniform vec4 ground_albedo_3 : source_color = vec4(0.373, 0.424, 0.294, 1.0);
uniform vec4 ground_albedo_4 : source_color = vec4(0.392, 0.475, 0.255, 1.0);
uniform vec4 ground_albedo_5 : source_color = vec4(0.29, 0.494, 0.365, 1.0);
uniform vec4 ground_albedo_6 : source_color = vec4(0.443, 0.447, 0.365, 1.0);
group_uniforms;

group_uniforms Blending;
uniform bool use_hard_textures = false;
uniform int blend_mode = 0;
uniform float blend_sharpness : hint_range(0.0, 10.0, 0.1) = 5.0;
uniform float blend_noise_scale : hint_range(0.0, 50.0, 1.0) = 10.0;
uniform float blend_noise_strength : hint_range(0.0, 1.0, 0.05) = 0.0;
group_uniforms;

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

group_uniforms Shading;
uniform vec4 shadow_color : source_color;
uniform int bands : hint_range(1, 10) = 5;
uniform float shadow_intensity : hint_range(-1.0, 0.5, 0.05) = 0.00;
group_uniforms;

// Shared varyings
varying vec3 vertex_normal;
varying vec4 custom1;
varying vec3 world_pos;

// Hard edges mode: flat material index (no interpolation)
varying flat int material_index;

// Soft blend mode: interpolated vertex colors
varying vec4 vc_color_0;
varying vec4 vc_color_1;

// Phantom fix: Per-cell material indices (flat = no GPU interpolation, supports 3 textures)
varying flat vec3 mat_indices; // mat_a, mat_b, mat_c (each /15)
varying vec2 mat_weights; // (weight_a, weight_b) - interpolated for smooth blending
varying float use_vertex_colors; // Flag: 1.0 = use vertex colors (boundary cells), 0.0 = use phantom fix
```

**What's happening — Render modes:**
- `diffuse_toon` enables the toon lighting model (stepped bands instead of smooth Lambert).
- `alpha_to_coverage` uses MSAA to smooth alpha-tested edges.
- `cull_disabled` ensures terrain is visible from both sides (important for wall geometry viewed from underneath).

**What's happening — Uniform naming convention:**
The 16 texture slots follow the naming pattern `vc_tex_XY` where X = dominant channel of COLOR (r/g/b/a) and Y = dominant channel of CUSTOM0 (r/g/b/a). So `vc_tex_rr` = texture slot 0 (both channels dominant red), `vc_tex_rg` = slot 1 (COLOR=red, CUSTOM0=green), etc.

**What's happening — Varyings:**
- `varying flat int material_index` — the `flat` qualifier disables GPU interpolation. Without it, the integer index would be interpolated across triangle faces, producing nonsense values like 2.7. The `flat` qualifier uses the provoking vertex's value for the entire triangle.
- `varying vec2 mat_weights` — intentionally NOT flat. Weights are interpolated by the GPU across triangle faces, producing smooth texture transitions.
- `varying float use_vertex_colors` — a flag that switches between two rendering paths: vertex-color blending (boundary cells with mixed heights) and phantom-fix blending (flat cells with up to 3 textures).

Now add the helper functions:

```glsl
float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	float a = hash(i);
	float b = hash(i + vec2(1.0, 0.0));
	float c = hash(i + vec2(0.0, 1.0));
	float d = hash(i + vec2(1.0, 1.0));
	return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

int get_material_index(vec4 vc_col_0, vec4 vc_col_1) {
	int index = 0;
	if (vc_col_0.r > 0.1) {
		if (vc_col_1.r > 0.1) index = 0;
		else if (vc_col_1.g > 0.1) index = 1;
		else if (vc_col_1.b > 0.1) index = 2;
		else if (vc_col_1.a > 0.1) index = 3;
	}
	else if (vc_col_0.g > 0.1) {
		if (vc_col_1.r > 0.1) index = 4;
		else if (vc_col_1.g > 0.1) index = 5;
		else if (vc_col_1.b > 0.1) index = 6;
		else if (vc_col_1.a > 0.1) index = 7;
	}
	else if (vc_col_0.b > 0.1) {
		if (vc_col_1.r > 0.1) index = 8;
		else if (vc_col_1.g > 0.1) index = 9;
		else if (vc_col_1.b > 0.1) index = 10;
		else if (vc_col_1.a > 0.1) index = 11;
	}
	else if (vc_col_0.a > 0.1) {
		if (vc_col_1.r > 0.1) index = 12;
		else if (vc_col_1.g > 0.1) index = 13;
		else if (vc_col_1.b > 0.1) index = 14;
		else if (vc_col_1.a > 0.1) index = 15;
	}
	return index;
}

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

vec4 sample_material_by_index(int index, vec2 base_uv) {
	float scale = 1.0;
	if (index != 15)
		scale = get_texture_scale(index);
	vec2 uv = base_uv * scale;
	vec4 result;
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

vec4 sample_wall_by_index(int index, vec2 uv) {
	return sample_material_by_index(index, uv);
}
```

**What's happening:**
- `get_material_index()` decodes the two vertex colors back into a 0-15 index. This is the inverse of `texture_index_to_colors()` from Part 03. The threshold `> 0.1` (not `> 0.5`) handles GPU interpolation bleed at triangle boundaries.
- `sample_material_by_index()` fetches a texture by index and multiplies by the corresponding ground albedo color. Slots 0-5 get tinted (these are the 6 "named" textures), slots 6-14 are untinted, slot 15 is the void/transparent texture.
- `filter_nearest` on all texture samplers ensures pixel art stays crisp.

Now add the weight blending functions:

```glsl
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

	float power = 2.0 + sharpness * 2.0;
	float total = 0.0;

	for (int i = 0; i < 16; i++) {
		weights[i] = pow(max(raw_weights[i], 0.0), power);
		total += weights[i];
	}

	if (total > 0.001) {
		for (int i = 0; i < 16; i++) {
			weights[i] /= total;
		}
	} else {
		float max_raw = 0.0;
		int max_idx = 0;
		for (int i = 0; i < 16; i++) {
			if (raw_weights[i] > max_raw) {
				max_raw = raw_weights[i];
				max_idx = i;
			}
		}
		for (int i = 0; i < 16; i++) {
			weights[i] = (i == max_idx) ? 1.0 : 0.0;
		}
	}
}

vec4 snap_to_dominant(vec4 c) {
	float max_val = max(max(c.r, c.g), max(c.b, c.a));
	if (max_val < 0.001) return vec4(1.0, 0.0, 0.0, 0.0);
	vec4 result = vec4(0.0);
	if (c.r >= max_val - 0.001) result.r = 1.0;
	else if (c.g >= max_val - 0.001) result.g = 1.0;
	else if (c.b >= max_val - 0.001) result.b = 1.0;
	else result.a = 1.0;
	return result;
}

vec4 blend_wall_materials(vec2 uv, vec4 vc0, vec4 vc1, float sharpness) {
	vec4 snapped_vc0 = snap_to_dominant(vc0);
	vec4 snapped_vc1 = snap_to_dominant(vc1);

	float weights[16];
	calculate_blend_weights(snapped_vc0, snapped_vc1, sharpness, weights);

	vec4 wall_color = vec4(0.0);
	float total_weight = 0.0;

	for (int i = 0; i < 16; i++) {
		if (weights[i] > 0.01) {
			wall_color += sample_wall_by_index(i, uv) * weights[i];
			total_weight += weights[i];
		}
	}

	if (total_weight > 0.001) {
		wall_color /= total_weight;
	}
	return wall_color;
}
```

**What's happening:**
- `calculate_blend_weights()` computes the product of each channel pair (`vc0.r * vc1.r`, `vc0.r * vc1.g`, etc.) to get 16 raw weights. A power function controlled by `sharpness` makes transitions sharper or smoother. Normalization ensures weights sum to 1.0.
- `snap_to_dominant()` fights GPU interpolation: when the GPU blends vertex colors across a triangle that spans floor and wall regions, you get intermediate values like (0.6, 0.4, 0.0, 0.0) instead of the intended (1.0, 0.0, 0.0, 0.0). Snapping forces back to the dominant channel. Without this, walls show bleeding artifacts from floor textures.
- Wall blending uses snapped colors while floor blending uses raw interpolated colors — this is intentional. Floors benefit from smooth transitions; walls need crisp material boundaries.

Now add the vertex and fragment shaders:

```glsl
void vertex() {
	vertex_normal = NORMAL;
	custom1 = CUSTOM1;
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;

	material_index = get_material_index(COLOR, CUSTOM0);

	vc_color_0 = COLOR;
	vc_color_1 = CUSTOM0;

	// CUSTOM2 decoding: material blend data
	float packed_mats = CUSTOM2.r * 255.0;
	mat_indices.x = mod(packed_mats, 16.0) / 15.0;  // mat_a
	mat_indices.y = floor(packed_mats / 16.0) / 15.0;  // mat_b
	mat_indices.z = CUSTOM2.g;  // mat_c (already /15)
	mat_weights = vec2(CUSTOM2.b, CUSTOM2.a);
	use_vertex_colors = CUSTOM2.a >= 1.5 ? 1.0 : 0.0;
}

void fragment() {
	ALPHA_SCISSOR_THRESHOLD = 0.5;
	vec2 tiling_factor_floor = vec2(1.0 / float(chunk_size.x), 1.0 / float(chunk_size.z));
	vec2 tiling_factor_wall = vec2(1.0 / float(chunk_size.x), 1.0 / float(chunk_size.y));
	float is_ridge = custom1.g;
	bool is_floor = dot(vertex_normal, vec3(0.0, 1.0, 0.0)) > wall_threshold && is_ridge < 0.5;

	if (is_floor) {
		vec2 floor_uv = UV2 * tiling_factor_floor;
		vec4 floor_color;

		if (use_hard_textures) {
			if (blend_mode == 2)
				floor_color = sample_material_by_index(material_index, floor_uv);
			else {
				int cell_dominant_mat = int(round(mat_indices.x * 15.0));
				floor_color = sample_material_by_index(cell_dominant_mat, floor_uv);
			}
		}
		else if (use_vertex_colors > 0.5) {
			float weights[16];
			float effective_sharpness = blend_sharpness;
			if (blend_noise_strength > 0.0) {
				float n = noise(world_pos.xz * blend_noise_scale);
				effective_sharpness = mix(blend_sharpness, blend_sharpness * (0.5 + n), blend_noise_strength);
			}
			calculate_blend_weights(vc_color_0, vc_color_1, effective_sharpness, weights);

			floor_color = vec4(0.0);
			for (int i = 0; i < 16; i++) {
				if (weights[i] > 0.01) {
					floor_color += sample_material_by_index(i, floor_uv) * weights[i];
				}
			}
		}
		else {
			int mat_a = int(round(mat_indices.x * 15.0));
			int mat_b = int(round(mat_indices.y * 15.0));
			int mat_c = int(round(mat_indices.z * 15.0));

			float weight_a = mat_weights.x;
			float weight_b = mat_weights.y;
			float weight_c = 1.0 - weight_a - weight_b;

			weight_a = max(0.0, weight_a);
			weight_b = max(0.0, weight_b);
			weight_c = max(0.0, weight_c);

			float weight_sum = weight_a + weight_b + weight_c;
			if (weight_sum > 0.001) {
				weight_a /= weight_sum;
				weight_b /= weight_sum;
				weight_c /= weight_sum;
			}

			if (blend_noise_strength > 0.0) {
				float n = noise(world_pos.xz * blend_noise_scale);
				float noise_offset = (n - 0.5) * blend_noise_strength;
				weight_a = clamp(weight_a + noise_offset, 0.0, 1.0);
				weight_b = clamp(weight_b - noise_offset * 0.5, 0.0, 1.0);
				weight_c = clamp(weight_c - noise_offset * 0.5, 0.0, 1.0);
				float total = weight_a + weight_b + weight_c;
				if (total > 0.001) {
					weight_a /= total;
					weight_b /= total;
					weight_c /= total;
				}
			}

			if (blend_sharpness > 0.0) {
				float power = 1.0 + blend_sharpness;
				weight_a = pow(weight_a, power);
				weight_b = pow(weight_b, power);
				weight_c = pow(weight_c, power);
				float total = weight_a + weight_b + weight_c;
				if (total > 0.001) {
					weight_a /= total;
					weight_b /= total;
					weight_c /= total;
				}
			}

			vec4 color_a = sample_material_by_index(mat_a, floor_uv);
			vec4 color_b = sample_material_by_index(mat_b, floor_uv);
			vec4 color_c = sample_material_by_index(mat_c, floor_uv);
			floor_color = color_a * weight_a + color_b * weight_b + color_c * weight_c;
		}

		ALBEDO = floor_color.rgb;
		ALPHA = floor_color.a;
	}
	else {
		// Wall rendering with triplanar projection
		vec4 vertex_pos = INV_VIEW_MATRIX * vec4(VERTEX, 1.0);
		vec3 abs_normal = abs(vertex_normal);

		vec3 tri_weights = vec3(abs_normal.x, 0.0, abs_normal.z);
		tri_weights /= (tri_weights.x + tri_weights.z);

		vec2 uv_x = (vertex_pos.zy / cell_size.yx) * tiling_factor_wall;
		vec2 uv_z = (vertex_pos.xy / cell_size) * tiling_factor_wall;

		vec4 wall_mat_x, wall_mat_z;

		float effective_sharpness = blend_sharpness;
		if (use_hard_textures) {
			wall_mat_x = blend_wall_materials(uv_x, vc_color_0, vc_color_1, effective_sharpness);
			wall_mat_z = blend_wall_materials(uv_z, vc_color_0, vc_color_1, effective_sharpness);
		} else {
			if (blend_noise_strength > 0.0) {
				float n = noise(world_pos.xz * blend_noise_scale);
				effective_sharpness = mix(blend_sharpness, blend_sharpness * (0.5 + n), blend_noise_strength);
			}
			wall_mat_x = blend_wall_materials(uv_x, vc_color_0, vc_color_1, effective_sharpness);
			wall_mat_z = blend_wall_materials(uv_z, vc_color_0, vc_color_1, effective_sharpness);
		}

		vec3 texture_x = wall_mat_x.rgb * tri_weights.x;
		vec3 texture_z = wall_mat_z.rgb * tri_weights.z;

		ALBEDO = texture_x + texture_z;
		ALPHA = wall_mat_x.a;
	}
}

void light() {
	float NdotL = dot(NORMAL, LIGHT);
	NdotL = clamp(NdotL, 0.0, 1.0);

	float stepped = ceil(NdotL * float(bands)) / float(bands);
	float toon_light = mix(shadow_intensity, 0.3, stepped);
	toon_light *= ATTENUATION;

	vec3 light_color = mix(shadow_color.rgb, LIGHT_COLOR.rgb, toon_light);
	DIFFUSE_LIGHT += light_color;
}
```

**What's happening — Floor rendering has 3 paths:**

1. **Hard textures** (`use_hard_textures = true`): Single texture per cell, no blending. Uses either per-vertex material index (blend_mode 2) or per-cell dominant material from CUSTOM2. Pixel-perfect boundaries, no transition zones.

2. **Vertex color blending** (`use_vertex_colors > 0.5`): For boundary cells (where heights differ across corners). Uses the full 16-weight blending system from `calculate_blend_weights()`. Most expensive path — 16 texture samples per pixel.

3. **Phantom fix blending** (default path): For flat cells. Uses 3 texture IDs + 2 weights from CUSTOM2. At most 3 texture samples per pixel. The weights are GPU-interpolated across the triangle for smooth transitions. This is the key optimization: most cells are flat and only need 3 textures, not 16.

**What's happening — Wall rendering:**
Walls use **biplanar projection** (not full triplanar — Y axis is omitted since walls are always vertical). Two projections: `uv_x` (from the ZY plane, for X-facing walls) and `uv_z` (from the XY plane, for Z-facing walls). The `tri_weights` blend between them based on normal direction. This prevents stretching on angled walls.

**What's happening — Toon lighting:**
The `light()` function quantizes the NdotL value into discrete steps: `ceil(NdotL * bands) / bands`. With `bands = 5`, you get 5 brightness levels instead of a smooth gradient. The `shadow_color` tints the dark regions (default black = pure shadow).

### Step 2: Create the grass shader

**Why:** Each grass blade is a QuadMesh rendered as a billboarded sprite. The shader handles spherical billboarding (always faces camera), wind animation, per-texture-slot sprite switching, and the same toon lighting as the terrain.

**File:** `godot/resources/shaders/mst_grass.gdshader`

```glsl
shader_type spatial;
render_mode diffuse_toon, depth_draw_opaque;

group_uniforms Albedo;
uniform sampler2D grass_texture : source_color, filter_nearest;
uniform vec3 grass_base_color : source_color = vec3(0.392, 0.471, 0.318);
uniform float wall_threshold : hint_range(0.0, 0.5) = 0.0;
uniform bool use_base_color = false;
uniform bool is_merge_round = false;
group_uniforms;

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

group_uniforms Animation;
uniform sampler2D wind_texture : source_color;
uniform vec2 wind_direction = vec2(1.0, 1.0);
uniform float wind_scale = 0.1;
uniform float wind_speed = 1.0;
uniform bool animate_active = true;
uniform float fps : hint_range(0.0, 30.0, 1.0)= 0.0;
group_uniforms;

group_uniforms Shading;
uniform vec4 shadow_color : source_color;
uniform int bands : hint_range(1, 10) = 5;
uniform float shadow_intensity : hint_range(-1.0, 0.5, 0.05) = 0.0;
group_uniforms;

varying vec3 model_origin;
varying vec3 vertex_normal;
varying vec3 instance_color;
varying float instance_sprite_id;
varying flat int instance_id;
varying float wind_color;

void vertex() {
	if (animate_active && VERTEX.y > 0.0) {
		float left_to_right_mult = 0.2;
		if (fps > 0.0) {
			float seed = dot(vec2(VERTEX.xz), vec2(12.9898, 78.233)) + float(INSTANCE_ID);
			float time_offset = fract(sin(seed) * 43758.5453);
			float quantized_time = floor(TIME * fps * time_offset) / fps;
			float sway = sin(quantized_time) * left_to_right_mult;
			VERTEX.x += sway;
		}
		else {
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

	vec3 cam_z = normalize(CAMERA_DIRECTION_WORLD);
	vec3 cam_y = vec3(0.0, 1.0, 0.0);
	vec3 cam_x = normalize(cross(cam_y, cam_z));
	mat3 spherical_billboard = mat3(cam_x, cam_y, cam_z);

	vec3 billboarded_vertex = spherical_billboard * VERTEX;

	vec3 instance_origin = (MODEL_MATRIX * vec4(0.0, 0.0, 0.0, 1.0)).xyz;
	vec3 world_position = instance_origin + billboarded_vertex;
	POSITION = PROJECTION_MATRIX * VIEW_MATRIX * vec4(world_position, 1.0);

	model_origin = (MODELVIEW_MATRIX * vec4(0.,0.,0.,1.)).xyz;
	instance_color = INSTANCE_CUSTOM.rgb;
	instance_sprite_id = INSTANCE_CUSTOM.a;
	instance_id = INSTANCE_ID;

	vec3 world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
	vertex_normal = world_normal;
}

void fragment() {
	ALPHA_SCISSOR_THRESHOLD = 0.5;
	vec4 grass_tex = texture(grass_texture, UV);
	vec3 grass_color = use_base_color ? grass_base_color : instance_color * grass_base_color;

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

	float w_threshold = wall_threshold;
	if (is_merge_round && wall_threshold <= 0.0)
		w_threshold -= 1.5;
	if (dot(vertex_normal, vec3(0.0, 1.0, 0.0)) > w_threshold) {
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

	if (ALPHA < 0.1) discard;

	LIGHT_VERTEX = model_origin;
}

void light() {
	float NdotL = dot(NORMAL, LIGHT);
	NdotL = clamp(NdotL, 0.0, 1.0);

	float stepped = ceil(NdotL * float(bands)) / float(bands);
	float toon_light = mix(shadow_intensity, 0.3, stepped);
	toon_light *= ATTENUATION;

	vec3 light_color = mix(shadow_color.rgb, LIGHT_COLOR.rgb, toon_light);
	DIFFUSE_LIGHT += light_color;
}
```

**What's happening — Billboarding:**
The vertex shader constructs a rotation matrix from the camera direction that keeps the quad always facing the camera (spherical billboard). `CAMERA_DIRECTION_WORLD` gives the camera's forward vector; we compute a right vector via cross product with world up, then use these three axes to rotate each vertex.

**What's happening — Two animation modes:**
1. **FPS mode** (`fps > 0`): Each blade sways independently at a quantized framerate. The `floor(TIME * fps) / fps` creates stepped animation — blades move at e.g. 8fps even when the game runs at 60fps. This gives a stop-motion pixel art look.
2. **Wind mode** (`fps = 0`): A noise texture scrolls across world space, creating wave-like wind patterns. `bend_factor = clamp(VERTEX.y, 0, 1)` ensures only the top of the blade bends — the base stays planted.

**What's happening — Per-texture grass:**
`INSTANCE_CUSTOM.a` carries the texture slot ID (encoded as 0.0-1.0 in 0.2 steps). The fragment shader selects which grass sprite to use based on thresholds (0.1, 0.3, 0.5, 0.7, 0.9). Each texture slot has its own `use_grass_tex_N` toggle — if false, ALPHA=0 hides grass for that texture type.

### Step 3: Create the round brush shader

**Why:** When the editor tool is active, a translucent circle follows the mouse cursor to show the brush area. This shader renders a QuadMesh with distance-based alpha, optionally showing a falloff preview using a curve texture.

**File:** `godot/resources/shaders/round_brush_radius_visual.gdshader`

```glsl
shader_type spatial;
render_mode unshaded, depth_test_disabled;

uniform sampler2D curve_texture : source_color;

uniform bool falloff_visible = false;

void fragment() {
	if (falloff_visible) {
		float t = 1.0 - distance(vec2(UV.x, UV.y), vec2(0.25, 0.75)) * 4.0;
		float sample = texture(curve_texture, vec2(clamp(t, 0.01, 0.99), 0)).r;
		ALPHA = clamp(sample, 0.0, 1.0) * 0.6;
	} else {
		ALPHA = 0.25;
	}
}
```

**What's happening:**
- `unshaded` + `depth_test_disabled` means this overlay renders without lighting and is always visible (even through terrain). This ensures the brush preview is always visible regardless of camera angle.
- When `falloff_visible` is true, the brush radius fades based on a curve texture — the same curve used for brush falloff in the editor.
- The center point `(0.25, 0.75)` is offset from the UV center `(0.5, 0.5)` because the brush mesh is positioned with a specific offset from the cursor.

### Step 4: Create the square brush shader

**Why:** Same purpose as the round brush, but for square brush mode. Uses Chebyshev distance (max of |x|, |y|) instead of Euclidean distance for a square falloff shape.

**File:** `godot/resources/shaders/square_brush_radius_visual.gdshader`

```glsl
shader_type spatial;
render_mode unshaded, depth_test_disabled;

uniform bool falloff_visible = false;

void fragment() {
	if (falloff_visible) {
		vec2 uv = (UV - 0.5) * 2.0;
		float t = max(abs(uv.x), abs(uv.y));
		float sample = smoothstep(1.0, 0.0, t);
		ALPHA = sample * 0.6;
	} else {
		ALPHA = 0.25;
	}
}
```

**What's happening:**
- `max(abs(uv.x), abs(uv.y))` is the Chebyshev distance, which produces a square distance field (equal distance at all points on a square perimeter). Compare with `distance()` which produces circular contours.
- `smoothstep(1.0, 0.0, t)` creates a smooth falloff from the center to the edges — fully opaque at center, transparent at edges.
- No curve texture needed here — the square brush uses a built-in smoothstep instead.

## Verify

The shader files are created in the Godot project and will be loaded at runtime by the Rust code (terrain.rs `ensure_terrain_material()` and `ensure_grass_material()`). No compilation step is needed — Godot compiles shaders on the fly when the project loads.

You can verify the shaders are syntactically correct by opening the Godot editor (`godot/project.godot`) and checking for shader errors in the Output panel.

## What You Learned

- **16-texture vertex color encoding**: Two RGBA vertex colors create a 4×4 = 16-texture selection grid. The shader decodes this by checking dominant channels.
- **Dual rendering paths**: Flat cells use efficient 3-texture blending via CUSTOM2 ("phantom fix"), while boundary cells fall back to full 16-weight vertex color blending. This optimization is critical for performance.
- **Triplanar wall projection**: Walls are textured by projecting from two axes (X and Z) and blending based on normal direction, preventing texture stretching on angled surfaces.
- **Spherical billboarding**: Grass quads always face the camera using a rotation matrix constructed from the camera direction and world up vector.
- **Quantized animation**: `floor(TIME * fps) / fps` creates stepped animation at a target framerate — a pixel art aesthetic choice.
- **Toon stepped lighting**: `ceil(NdotL * bands) / bands` quantizes the light-to-dark gradient into discrete bands.

## Stubs Introduced

- None (shader files are self-contained)

## Stubs Resolved

- None (shaders were not previously stubbed — they're new files)
