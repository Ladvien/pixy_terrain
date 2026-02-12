
// Pixy Terrain — Square brush radius overlay
//
// Editor-only: renders a transparent square over the terrain to show brush size.
// When falloff_visible is true, uses smoothstep from edges inward to visualize
// falloff. Otherwise flat 25% alpha.

shader_type spatial;
render_mode unshaded, depth_test_disabled;

uniform bool falloff_visible = false;

void fragment() {
    if (falloff_visible) {
            // Remap UV from 0..1 to -1..1 (centered)
            vec2 uv = (UV - 0.5) * 2.0;
            // Chebyshev distance: max of |x|, |y| — gives square shape
            float t = max(abs(uv.x), abs(uv.y));
            // Smoothstep: 1.0 at center, fades to 0.0 at edges
            float sample = smoothstep(1.0, 0.0, t);
            ALPHA = sample * 0.6;
    } else {
            ALPHA = 0.25;
    }
}