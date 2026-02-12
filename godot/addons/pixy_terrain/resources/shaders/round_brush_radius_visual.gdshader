// Pixy Terrain — Round brush radius overlay
//
// Editor-only: renders a transparent circle over the terrain to show brush size.
// When falloff_visible is true, samples a curve texture to visualize brush
// falloff (center=opaque, edges=fading). Otherwise flat 25% alpha.

shader_type spatial;
// unshaded: no lighting (pure overlay color)
// depth_test_disabled: always visible, even behind terrain geometry
render_mode unshaded, depth_test_disabled;

// Curve texture: 1D gradient encoding the brush falloff curve (from editor)
uniform sampler2D curve_texture : source_color;
// Toggle: show falloff gradient or just flat transparency
uniform bool falloff_visible = false;

void fragment() {
    if (falloff_visible) {
            // Distance from UV center (0.25, 0.75) scaled to fill the quad
            // This maps the circular brush radius to the mesh UV space
            float t = 1.0 - distance(vec2(UV.x, UV.y), vec2(0.25, 0.75)) * 4.0;
            // Sample the falloff curve (clamped to avoid edge artifacts)
            float sample = texture(curve_texture, vec2(clamp(t, 0.01, 0.99), 0)).r;
            ALPHA = clamp(sample, 0.0, 1.0) * 0.6;
    } else {
            // Flat overlay: 25% transparent
            ALPHA = 0.25;
    }
}