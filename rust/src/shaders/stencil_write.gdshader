shader_type spatial;
// Pass 1: Back-face stencil marker
// Renders back faces only, writes 255 to stencil buffer
// Does not write color - only marks where mesh interior is visible
render_mode cull_front, depth_draw_always, unshaded;
stencil_mode write, compare_always, 255;

// Clipping uniforms (shared with main shader)
uniform bool cross_section_enabled = false;
uniform vec3 clip_plane_position = vec3(0.0);
uniform vec3 clip_plane_normal = vec3(0.0, 1.0, 0.0);
uniform float clip_offset = 0.0;
uniform bool clip_camera_relative = false;

varying vec3 world_pos;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    // Slight bias to prevent z-fighting
    VERTEX -= NORMAL * 0.001;
}

void fragment() {
    // If cross-section disabled, don't render anything
    if (!cross_section_enabled) {
        discard;
    }

    // Calculate effective clip plane position
    vec3 effective_clip_pos = clip_plane_position;
    if (clip_camera_relative) {
        effective_clip_pos += CAMERA_POSITION_WORLD;
    }
    effective_clip_pos += normalize(clip_plane_normal) * clip_offset;

    // Discard fragments on the hidden side of the clip plane
    float dist = dot(world_pos - effective_clip_pos, normalize(clip_plane_normal));
    if (dist < 0.0) {
        discard;
    }

    // Don't write color - stencil_mode directive handles writing 255
    // Set alpha to 0 to prevent any color contribution
    ALPHA = 0.0;
}
