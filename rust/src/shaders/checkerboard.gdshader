shader_type spatial;

varying vec3 world_vertex;

uniform vec3 color_a : source_color = vec3(0.8, 0.8, 0.8);
uniform vec3 color_b : source_color = vec3(0.4, 0.4, 0.4);
uniform float scale : hint_range(0.1, 10) = 1.0;

void vertex() {
    world_vertex = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
    float checker = mod(floor(world_vertex.x * scale) + floor(world_vertex.y * scale) + floor(world_vertex.z * scale), 2.0) ;
    ALBEDO = mix(color_a, color_b, checker);
}