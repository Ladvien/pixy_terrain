# Implementing capped cross-section rendering in Godot 4.6 with Rust GDExtension

A **capped cross-section** (or "filled cross-section") renders terrain as a solid mass that's been sliced clean — the cut face shows a flat, opaque, textured interior surface. Think of slicing through a block of earth: you see the terrain surface above the cut, and where it's been sliced you see a solid dirt/rock interior, not hollow geometry or back-faces.

This "ant farm" aesthetic requires two rendering components:
1. **Terrain clipping** — Fragments beyond the clip plane are discarded
2. **Interior cap** — A flat quad at the clip plane, textured with interior material, rendered only where terrain actually exists

Godot 4.5+ introduces native stencil buffer support through `stencil_mode`, enabling pixel-perfect masking of the cap surface. Combined with gdext's `ImmediateMesh` for per-frame quad updates and global shader uniforms for synchronization, you can build a robust capped cross-section system.

---

## Core concept: Why stencil masking is required

Simply placing a textured quad at the clip plane doesn't work — it would cover the entire plane, including empty space where there's no terrain. The cap must only render where terrain interior is visible through the cut.

**The solution:** Terrain back-faces (the inside surfaces exposed by clipping) write to the stencil buffer. The cap quad then reads the stencil and only renders where stencil was written — exactly where terrain interior exists.

---

## Stencil buffer masking with Godot 4.5's stencil_mode directive

The `stencil_mode` directive syntax: `stencil_mode [flags], [compare_function], [reference_value]`. 

Available flags:
- **write** — Write reference value on depth pass
- **write_depth_fail** — Write on depth fail
- **read** — Enable stencil testing

Compare functions mirror OpenGL: `compare_always`, `compare_equal`, `compare_not_equal`, `compare_greater`, etc. Reference values range **0-255**, but avoid 0 since the buffer initializes to zeros each frame.

### Back-face stencil writer (marks where interior is visible)

```glsl
// back_face_stencil.shader
shader_type spatial;
render_mode cull_front, depth_draw_never, unshaded;
stencil_mode write, compare_always, 1;

global uniform vec3 clip_plane_position;
global uniform vec3 clip_plane_normal;

void fragment() {
    vec3 world_pos = (INV_VIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;
    float dist = dot(world_pos - clip_plane_position, clip_plane_normal);
    if (dist < 0.0) discard;  // Only mark back-faces above clip plane
    ALPHA = 0.0;  // Invisible — only writes stencil
}
```

### Interior cap shader (solid surface where stencil was written)

```glsl
// interior_cap.shader
shader_type spatial;
render_mode depth_draw_never;
stencil_mode read, compare_equal, 1;

uniform sampler2D interior_texture : source_color;
uniform float uv_scale = 0.1;

global uniform vec3 clip_plane_position;
global uniform vec3 clip_plane_normal;

varying vec3 world_pos;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

vec2 project_to_plane_uv(vec3 point, vec3 origin, vec3 normal) {
    vec3 up = abs(normal.y) < 0.99 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
    vec3 tangent = normalize(cross(up, normal));
    vec3 bitangent = cross(normal, tangent);
    vec3 offset = point - origin;
    return vec2(dot(offset, tangent), dot(offset, bitangent));
}

void fragment() {
    vec2 uv = project_to_plane_uv(world_pos, clip_plane_position, clip_plane_normal) * uv_scale;
    ALBEDO = texture(interior_texture, uv).rgb;
    ALPHA = 0.999;  // Force alpha pass for stencil read to work
}
```

**Critical gotcha**: Stencil read only works in the transparent/alpha pass. Use `ALPHA = 0.999` to force materials into that pass while appearing fully opaque.

---

## Depth-based masking (Godot 4.3-4.4 fallback)

If targeting Godot 4.3-4.4 or preferring depth over stencil, read the depth buffer using `hint_depth_texture`. Godot 4.3+ uses **reverse Z** (near=1.0, far=0.0), which reduces z-fighting but requires attention when comparing depths.

```glsl
// interior_cap_depth.shader - Depth-based masking (works in Godot 4.3+)
shader_type spatial;
render_mode unshaded, cull_disabled;

uniform sampler2D depth_texture : hint_depth_texture, repeat_disable, filter_nearest;
uniform sampler2D interior_texture : source_color;
uniform float uv_scale = 0.1;

void fragment() {
    float terrain_depth = textureLod(depth_texture, SCREEN_UV, 0.0).r;
    
    // In reverse Z: values near 0.0 = far plane (nothing drawn)
    if (terrain_depth < 0.01) {
        discard;  // No terrain was rendered here
    }
    
    // ... UV projection and texturing same as stencil version
    ALBEDO = texture(interior_texture, uv).rgb;
}
```

| Approach | Precision | Complexity | Compatibility |
|----------|-----------|------------|---------------|
| **Stencil** | Pixel-perfect | Moderate | Godot 4.5+ only |
| **Depth** | Good (potential edge artifacts) | Simple | Godot 4.0+ |

Stencil is preferred — it explicitly marks "terrain interior visible here" rather than inferring from depth values.

---

## Creating the cap quad from Rust gdext

**ImmediateMesh** is optimal for simple geometry updated every frame. For a 6-vertex quad, overhead is negligible.

```rust
use godot::prelude::*;
use godot::classes::{ImmediateMesh, MeshInstance3D, IMeshInstance3D};
use godot::classes::mesh::PrimitiveType;

#[derive(GodotClass)]
#[class(base=MeshInstance3D)]
pub struct InteriorCap {
    base: Base<MeshInstance3D>,
    mesh: Gd<ImmediateMesh>,
    
    #[export]
    quad_size: f32,
    
    #[export]
    clip_distance: f32,
}

#[godot_api]
impl IMeshInstance3D for InteriorCap {
    fn init(base: Base<MeshInstance3D>) -> Self {
        Self {
            base,
            mesh: ImmediateMesh::new_gd(),
            quad_size: 50.0,
            clip_distance: 10.0,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_mesh(&self.mesh);
        self.rebuild_quad();
    }

    fn process(&mut self, _delta: f64) {
        self.update_from_camera();
    }
}

#[godot_api]
impl InteriorCap {
    fn rebuild_quad(&mut self) {
        let half = self.quad_size / 2.0;
        let normal = Vector3::new(0.0, 0.0, -1.0);  // Faces camera

        self.mesh.clear_surfaces();
        self.mesh.surface_begin(PrimitiveType::TRIANGLES);

        let verts = [
            (Vector3::new(-half, -half, 0.0), Vector2::new(0.0, 1.0)),
            (Vector3::new(half, -half, 0.0), Vector2::new(1.0, 1.0)),
            (Vector3::new(-half, half, 0.0), Vector2::new(0.0, 0.0)),
            (Vector3::new(half, half, 0.0), Vector2::new(1.0, 0.0)),
        ];

        // Two triangles
        for &idx in &[0usize, 2, 1, 1, 2, 3] {
            self.mesh.surface_set_normal(normal);
            self.mesh.surface_set_uv(verts[idx].1);
            self.mesh.surface_add_vertex(verts[idx].0);
        }

        self.mesh.surface_end();
    }

    fn update_from_camera(&mut self) {
        if let Some(viewport) = self.base().get_viewport() {
            if let Some(camera) = viewport.get_camera_3d() {
                let cam_transform = camera.get_global_transform();
                let cam_forward = -cam_transform.basis.col_c();
                
                let quad_pos = cam_transform.origin + cam_forward * self.clip_distance;
                
                self.base_mut().set_global_position(quad_pos);
                self.base_mut().look_at(cam_transform.origin, Vector3::UP);
            }
        }
    }
}
```

The quad is oversized and relies entirely on stencil masking to constrain where it renders.

---

## Preventing z-fighting at the clip boundary

The cap quad sits exactly at the clip plane, which can cause z-fighting with terrain edges. **Vertex offset toward camera** is the most reliable fix:

```glsl
void vertex() {
    vec3 world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
    vec3 dir_to_cam = normalize(CAMERA_POSITION_WORLD - world_position);
    world_position += dir_to_cam * 0.01;  // 1cm offset toward camera
    VERTEX = (inverse(MODEL_MATRIX) * vec4(world_position, 1.0)).xyz;
}
```

A **0.5-1cm offset** typically eliminates z-fighting without visible gaps at terrain scales.

---

## Synchronizing shader uniforms from Rust

**Global shader uniforms** keep terrain and cap shaders synchronized:

```rust
use godot::classes::{RenderingServer};
use godot::classes::rendering_server::GlobalShaderParameterType;

fn sync_clip_plane(&self) {
    let rs = RenderingServer::singleton();
    rs.global_shader_parameter_set("clip_plane_position", &self.clip_position.to_variant());
    rs.global_shader_parameter_set("clip_plane_normal", &self.clip_normal.to_variant());
}
```

In shaders:

```glsl
global uniform vec3 clip_plane_position;
global uniform vec3 clip_plane_normal;
```

Both terrain clip shader and cap shader read identical values automatically.

---

## Render order: Three-pass pipeline

| Pass | Material | render_priority | Purpose |
|------|----------|-----------------|---------|
| 1 | Terrain front faces | -1 | Render visible terrain, discard below clip plane |
| 2 | Terrain back-face stencil | 0 | Write stencil=1 where interior visible |
| 3 | Interior cap quad | 1 | Render solid interior only where stencil=1 |

**render_priority**: Lower values render first. Set in Rust:

```rust
terrain_material.set_render_priority(-1);
stencil_writer_material.set_render_priority(0);
cap_material.set_render_priority(1);
```

**next_pass** chains the stencil writer to the terrain material:

```rust
terrain_material.set_next_pass(&stencil_writer_material);
```

---

## Complete terrain shader with clipping

```glsl
// terrain_clipped.shader
shader_type spatial;

global uniform vec3 clip_plane_position;
global uniform vec3 clip_plane_normal;

varying vec3 world_pos;

void vertex() {
    world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
    // Clip fragments below the plane
    if (dot(world_pos - clip_plane_position, clip_plane_normal) < 0.0) {
        discard;
    }
    
    // Normal terrain shading...
    ALBEDO = terrain_color;
}
```

---

## Summary

The capped cross-section ("ant farm" effect) requires:

1. **Terrain shader** clips geometry at the plane via `discard`
2. **Back-face stencil pass** marks where terrain interior is visible (stencil=1)
3. **Interior cap quad** renders solid textured surface only where stencil=1

Key implementation details:
- Use stencil reference ≥1 (buffer initializes to 0)
- Cap shader needs `ALPHA = 0.999` to enter alpha pass for stencil read
- Offset cap quad 0.5-1cm toward camera to prevent z-fighting
- Global uniforms synchronize clip plane params across all shaders
- `render_priority` controls pass order: lower values render first

For Godot 4.3-4.4, depth-buffer masking via `hint_depth_texture` provides a workable fallback with minor edge artifacts.
