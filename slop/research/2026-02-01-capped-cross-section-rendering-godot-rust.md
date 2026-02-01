# Capped cross-section rendering in Godot 4 with Rust integration

**Proper capped cross-sections—where sliced geometry shows filled surfaces rather than hollow interiors—require either Godot 4.5's new stencil buffer support or SDF-integrated mesh generation for terrain.** The fundamental challenge is that simple clip-plane discard exposes back-faces but doesn't create a true filled cap surface. For SDF-based terrain like transvoxel/marching cubes, the most elegant solution is generating cap polygons during mesh extraction since the SDF already contains the interior/exterior information needed. This report covers shader implementations, Rust mesh-slicing libraries for GDExtension, and specific techniques for "ant farm" style terrain visualization.

## Godot 4.5 brings native stencil buffer support

Godot 4.5 (released September 2025) introduced native stencil buffer access via PR #80710, which is the key enabling technology for proper cap rendering. The shader API uses a new `stencil_mode` directive:

```gdshader
stencil_mode write_only, compare_always, 255;
stencil_mode read_only, compare_equal, 1;
```

Available stencil modes include `WRITE_DEPTH_FAIL`, `READ_ONLY`, `READ_WRITE`, `WRITE_ONLY`, and `DISABLED`. Compare operations support `COMPARE_LESS`, `COMPARE_EQUAL`, `COMPARE_GREATER`, `COMPARE_NOT_EQUAL`, and their variants. The reference value accepts integers **0-255**. For pre-4.5 versions, no native stencil access exists—workarounds require separate Viewports with mask textures or depth buffer tricks with significant limitations.

The [Unity3DCrossSectionShader](https://github.com/Dandarawy/Unity3DCrossSectionShader) (590+ stars) documents the standard stencil approach: render the mesh two-sided, have back-facing triangles write 255 to the stencil buffer, then render the cutting plane only where stencil equals 255. This same technique now works directly in Godot 4.5.

## Two-pass rendering with next_pass works in all Godot 4 versions

For Godot 4.0-4.4 (or simpler implementations in 4.5), the `next_pass` material property enables multi-pass rendering without stencil operations. The first pass renders back-faces as the cap color, the second pass renders the normal surface:

```gdshader
// Pass 1: Cap surface (set as main material)
shader_type spatial;
render_mode cull_front, unshaded, depth_draw_alpha_prepass;

uniform vec3 clip_plane_normal = vec3(0.0, 1.0, 0.0);
uniform float clip_plane_distance = 0.0;
uniform vec4 cap_color : source_color = vec4(0.6, 0.3, 0.1, 1.0);
varying vec3 world_pos;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    VERTEX += NORMAL * 0.001; // Bias prevents z-fighting
}

void fragment() {
    float dist = dot(world_pos, normalize(clip_plane_normal)) - clip_plane_distance;
    if (dist > 0.0) { ALBEDO = cap_color.rgb; }
    else { discard; }
}
```

```gdshader
// Pass 2: Normal surface (set as next_pass of first material)
shader_type spatial;
render_mode cull_back, diffuse_burley, specular_schlick_ggx;

uniform vec3 clip_plane_normal = vec3(0.0, 1.0, 0.0);
uniform float clip_plane_distance = 0.0;
uniform sampler2D albedo_texture : source_color;
varying vec3 world_pos;

void vertex() { world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz; }

void fragment() {
    float dist = dot(world_pos, normalize(clip_plane_normal)) - clip_plane_distance;
    if (dist > 0.0) { ALBEDO = texture(albedo_texture, UV).rgb; }
    else { discard; }
}
```

The key `render_mode` settings are `cull_front` (renders back-faces for the interior cap) and `cull_back` (normal front-face rendering). This approach shows internal mesh geometry rather than a perfectly flat cap surface, which works well for terrain but may cause artifacts on complex mechanical models.

## Rust crates for mesh slicing and GDExtension integration

The **csgrs** crate ([crates.io](https://crates.io/crates/csgrs), [GitHub](https://github.com/timschmidt/csgrs)) provides the most complete solution for cap generation, offering `Mesh::slice(plane)` to slice meshes and return cross-section polygons with built-in triangulation. It also converts SDFs to meshes via marching cubes using `Mesh::from_sdf()` and integrates directly with Parry3d through `to_trimesh()`.

**Parry3d** ([crates.io](https://crates.io/crates/parry3d)) offers lower-level mesh operations including `TriMesh::split_trimesh()` for splitting meshes by planes into two parts, and `intersection_with_plane()` for extracting contour polylines suitable for cap polygon generation. The crate includes Sutherland-Hodgman clipping via `query::clip::clip_halfspace_polygon`.

For SDF operations, **sdfu** ([crates.io](https://crates.io/crates/sdfu)) provides analytic SDF primitives and combination operations (union, intersection, subtraction, smooth blending), while **mesh_to_sdf** ([crates.io](https://crates.io/crates/mesh_to_sdf)) converts triangle meshes to SDF grids using BVH or R-tree acceleration.

GDExtension integration via [gdext](https://github.com/godot-rust/gdext) requires marshaling mesh data through Godot's `SurfaceTool` or `ArrayMesh`:

```rust
use godot::prelude::*;
use godot::classes::{SurfaceTool, ArrayMesh, Mesh};

fn create_cap_mesh(vertices: &[Vector3], normals: &[Vector3]) -> Gd<ArrayMesh> {
    let mut st = SurfaceTool::new_gd();
    st.begin(Mesh::PRIMITIVE_TRIANGLES);
    for i in 0..vertices.len() {
        st.set_normal(normals[i]);
        st.add_vertex(vertices[i]);
    }
    st.generate_tangents();
    st.commit().unwrap()
}
```

A recommended Cargo.toml includes: `parry3d = "0.26"`, `csgrs = "0.20"`, `sdfu = "0.4"`, `mesh_to_sdf = { version = "0.2", features = ["nalgebra"] }`, and `nalgebra = "0.33"`.

## SDF-integrated mesh generation is optimal for terrain

Since transvoxel/marching cubes terrain is already generated from an SDF, the most elegant capped cross-section approach modifies the mesh generation pass to treat the clip plane as an additional boundary. The SDF provides three critical pieces of information: the surface boundary (where SDF = 0), whether a point is inside solid terrain (SDF < 0), and the surface gradient for cap normals.

During marching cubes, classify voxel corners with both the SDF and clip plane:

```cpp
for each voxel cell:
    for each corner:
        float sdf = sample_terrain_sdf(corner_position);
        float clip_dist = dot(corner_position, clip_normal) - clip_offset;
        
        // Treat clipped region as "outside" regardless of SDF
        if (clip_dist < 0.0) corner_inside = false;
        else corner_inside = (sdf < 0.0);
    
    // Standard marching cubes lookup with modified corner states
    // This naturally generates cap geometry at clip boundary
```

Cap triangles get their normals from the clip plane normal (for flat caps) or the SDF gradient (for caps that follow terrain contours). UVs can be calculated by projecting world positions onto the clip plane.

For **real-time clip plane movement** where regenerating the mesh is too expensive, use a shader-only approach: render the mesh with `cull_disabled` or two passes, apply interior shading to back-faces, and optionally sample an SDF texture in the shader for precise boundary determination. This trades cap quality (potential edge artifacts) for interactivity.

## Terrain-specific "ant farm" visualization techniques

For underground terrain visualization showing solid earth with visible tunnels, three approaches work effectively:

**Depth-slab rendering** clips geometry outside a viewing slab and renders back-faces of caves with a "dirt interior" color. This creates the classic terrarium side-view effect:

```gdshader
uniform float slab_front = -1.0;
uniform float slab_back = 1.0;
uniform vec4 interior_color : source_color = vec4(0.4, 0.25, 0.1, 1.0);

void fragment() {
    float depth = world_pos.z;
    if (depth < slab_front || depth > slab_back) discard;
    
    // Back-faces (inside caves) get interior shading
    if (!FRONT_FACING) {
        ALBEDO = interior_color.rgb;
        NORMAL = -NORMAL; // Flip normal for correct lighting
    }
}
```

**CSG-based cutaway** subtracts a viewing box from the terrain SDF during mesh generation. This produces clean edges but requires mesh regeneration when the view changes.

**Dual-material terrain** marks cap/interior triangles with a secondary material index during mesh generation, allowing different textures for the exposed earth cross-section versus the natural surface. This integrates well with triplanar texturing for the normal terrain surface while applying a separate rock/dirt texture to cut faces.

## Stencil-based cap rendering workflow for complex meshes

For non-terrain meshes requiring precise filled caps (like architectural cutaways), the full stencil approach in Godot 4.5 follows this algorithm:

- **Pass 1**: Render back-faces only (`cull_front`), write to stencil (`stencil_mode write_only, compare_always, 255`), disable color output. This marks pixels where the mesh interior is visible.
- **Pass 2**: Render front-faces normally with clipping. Where front-faces cover back-faces, the stencil gets overwritten.
- **Pass 3**: Render a quad at the clip plane position with stencil test (`stencil_mode read_only, compare_equal, 255`). The cap appears only where back-faces are visible but front-faces aren't.

The [alanlawrance/cross-section-shader](https://github.com/alanlawrance/cross-section-shader) Unity URP implementation documents this technique in detail, and the concepts transfer directly to Godot's shader language.

## Implementation decision matrix

| Factor | Mesh-Generation Caps | Shader-Only Caps |
|--------|---------------------|------------------|
| **Performance** | Better for static clips | Better for dynamic clips |
| **Cap quality** | Clean polygonal edges | Potential edge artifacts |
| **Complexity** | Higher CPU load | Higher GPU load |
| **LOD support** | Natural with Transvoxel | Requires careful handling |
| **Godot version** | Works in all 4.x | Stencil needs 4.5+ |

## Conclusion

For SDF-based terrain visualization, **integrating cap generation into the marching cubes pass** offers the most elegant solution since no additional computation is needed—the SDF already encodes inside/outside and provides surface normals. The Rust crates **csgrs** and **parry3d** provide production-ready mesh slicing and can integrate with Godot via gdext. For shader-only approaches, Godot 4.5's stencil buffer support finally enables the standard two-pass technique used in Unity for years, while the `next_pass` back-face rendering approach works in all Godot 4 versions with acceptable results for terrain. No complete out-of-the-box solution exists on godotshaders.com, but the techniques documented here provide a clear implementation path.
