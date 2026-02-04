use std::collections::HashMap;

use godot::classes::{
    EditorNode3DGizmo, EditorNode3DGizmoPlugin, IEditorNode3DGizmoPlugin, StandardMaterial3D,
};
use godot::prelude::*;

use crate::editor_plugin::{BrushType, PixyTerrainPlugin, TerrainToolMode};
use crate::terrain::PixyTerrain;

/// Gizmo plugin for PixyTerrain: brush preview, chunk grid overlay, draw pattern visualization.
/// Port of Yugen's MarchingSquaresTerrainGizmoPlugin + MarchingSquaresTerrainGizmo.
#[derive(GodotClass)]
#[class(base=EditorNode3DGizmoPlugin, init, tool)]
pub struct PixyTerrainGizmoPlugin {
    base: Base<EditorNode3DGizmoPlugin>,

    /// Cached reference to the editor plugin for reading brush state.
    pub plugin_ref: Option<Gd<PixyTerrainPlugin>>,
}

#[godot_api]
impl IEditorNode3DGizmoPlugin for PixyTerrainGizmoPlugin {
    fn get_gizmo_name(&self) -> GString {
        "PixyTerrain".into()
    }

    fn has_gizmo(&self, node: Option<Gd<godot::classes::Node3D>>) -> bool {
        node.is_some_and(|n| n.is_class("PixyTerrain"))
    }

    fn redraw(&mut self, gizmo: Option<Gd<EditorNode3DGizmo>>) {
        let Some(mut gizmo) = gizmo else {
            return;
        };
        gizmo.clear();

        let Some(node) = gizmo.get_node_3d() else {
            return;
        };

        let Ok(terrain) = node.clone().try_cast::<PixyTerrain>() else {
            return;
        };

        let Some(ref plugin) = self.plugin_ref else {
            return;
        };

        if !plugin.is_instance_valid() {
            return;
        }

        let plugin_bind = plugin.bind();
        let state = plugin_bind.get_gizmo_state();
        drop(plugin_bind);

        let t = terrain.bind();
        let dim = t.dimensions;
        let cell_size = t.cell_size;
        let chunk_keys = t.get_chunk_keys();

        let chunk_width = (dim.x - 1) as f32 * cell_size.x;
        let chunk_depth = (dim.z - 1) as f32 * cell_size.y;

        // Collect chunk existence for closure use (avoid holding terrain borrow)
        let mut existing_chunks: Vec<[i32; 2]> = Vec::new();
        for i in 0..chunk_keys.len() {
            let ck = chunk_keys[i];
            existing_chunks.push([ck.x as i32, ck.y as i32]);
        }

        let has_chunk_fn = |x: i32, z: i32| -> bool { existing_chunks.contains(&[x, z]) };

        // ── Chunk management lines ──
        let addchunk_mat = self.base_mut().get_material("addchunk");
        let removechunk_mat = self.base_mut().get_material("removechunk");

        if state.mode == TerrainToolMode::ChunkManagement {
            for &[cx, cz] in &existing_chunks {
                // Draw borders for existing chunk (red X)
                draw_chunk_lines(
                    &mut gizmo,
                    cx,
                    cz,
                    chunk_width,
                    chunk_depth,
                    &has_chunk_fn,
                    &removechunk_mat,
                    true,
                );

                // Draw borders for adjacent empty chunks (green +)
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let ax = cx + dx;
                    let az = cz + dz;
                    if !has_chunk_fn(ax, az) {
                        draw_chunk_lines(
                            &mut gizmo,
                            ax,
                            az,
                            chunk_width,
                            chunk_depth,
                            &has_chunk_fn,
                            &addchunk_mat,
                            false,
                        );
                    }
                }
            }
        }

        // ── Draw pattern visualization ──
        let pattern_mat = self.base_mut().get_material("brush_pattern");

        if !state.draw_pattern.is_empty() {
            let mut lines = PackedVector3Array::new();

            for (chunk_key, cells) in &state.draw_pattern {
                for (cell_key, sample) in cells {
                    let world_x = (chunk_key[0] * (dim.x - 1) + cell_key[0]) as f32 * cell_size.x;
                    let world_z = (chunk_key[1] * (dim.z - 1) + cell_key[1]) as f32 * cell_size.y;

                    let y = if state.flatten {
                        state.draw_height
                    } else if let Some(chunk) = t.get_chunk(chunk_key[0], chunk_key[1]) {
                        chunk
                            .bind()
                            .get_height(Vector2i::new(cell_key[0], cell_key[1]))
                    } else {
                        0.0
                    };

                    let half = *sample * cell_size.x * 0.4;
                    let center = Vector3::new(world_x, y + 0.05, world_z);

                    // Draw a small square for each pattern cell
                    lines.push(center + Vector3::new(-half, 0.0, -half));
                    lines.push(center + Vector3::new(half, 0.0, -half));
                    lines.push(center + Vector3::new(half, 0.0, -half));
                    lines.push(center + Vector3::new(half, 0.0, half));
                    lines.push(center + Vector3::new(half, 0.0, half));
                    lines.push(center + Vector3::new(-half, 0.0, half));
                    lines.push(center + Vector3::new(-half, 0.0, half));
                    lines.push(center + Vector3::new(-half, 0.0, -half));
                }
            }

            if !lines.is_empty() {
                if let Some(ref mat) = pattern_mat {
                    gizmo.add_lines(&lines, &mat.clone().upcast::<godot::classes::Material>());
                }
            }
        }

        // ── Brush circle/square visualization ──
        let brush_mat = self.base_mut().get_material("brush");

        if state.terrain_hovered {
            let pos = state.brush_position;
            let half = state.brush_size / 2.0;
            let mut brush_lines = PackedVector3Array::new();

            match state.brush_type {
                BrushType::Round => {
                    let segments = 32;
                    for i in 0..segments {
                        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
                        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
                        brush_lines.push(Vector3::new(
                            pos.x + half * a0.cos(),
                            pos.y + 0.1,
                            pos.z + half * a0.sin(),
                        ));
                        brush_lines.push(Vector3::new(
                            pos.x + half * a1.cos(),
                            pos.y + 0.1,
                            pos.z + half * a1.sin(),
                        ));
                    }
                }
                BrushType::Square => {
                    let y = pos.y + 0.1;
                    brush_lines.push(Vector3::new(pos.x - half, y, pos.z - half));
                    brush_lines.push(Vector3::new(pos.x + half, y, pos.z - half));
                    brush_lines.push(Vector3::new(pos.x + half, y, pos.z - half));
                    brush_lines.push(Vector3::new(pos.x + half, y, pos.z + half));
                    brush_lines.push(Vector3::new(pos.x + half, y, pos.z + half));
                    brush_lines.push(Vector3::new(pos.x - half, y, pos.z + half));
                    brush_lines.push(Vector3::new(pos.x - half, y, pos.z + half));
                    brush_lines.push(Vector3::new(pos.x - half, y, pos.z - half));
                }
            }

            if !brush_lines.is_empty() {
                if let Some(ref mat) = brush_mat {
                    gizmo.add_lines(
                        &brush_lines,
                        &mat.clone().upcast::<godot::classes::Material>(),
                    );
                }
            }
        }

        drop(t);
    }

    fn get_priority(&self) -> i32 {
        -1
    }
}

impl PixyTerrainGizmoPlugin {
    pub fn create_materials(&mut self) {
        self.base_mut()
            .create_material("brush", Color::from_rgba(1.0, 1.0, 1.0, 0.5));
        self.base_mut()
            .create_material("brush_pattern", Color::from_rgba(0.7, 0.7, 0.7, 0.5));
        self.base_mut()
            .create_material("removechunk", Color::from_rgba(1.0, 0.0, 0.0, 0.5));
        self.base_mut()
            .create_material("addchunk", Color::from_rgba(0.0, 1.0, 0.0, 0.5));
        self.base_mut().create_handle_material("handles");
    }
}

/// Draw chunk border lines (free function to avoid borrow issues with self).
#[allow(clippy::too_many_arguments)]
fn draw_chunk_lines(
    gizmo: &mut Gd<EditorNode3DGizmo>,
    chunk_x: i32,
    chunk_z: i32,
    chunk_width: f32,
    chunk_depth: f32,
    has_chunk: &dyn Fn(i32, i32) -> bool,
    material: &Option<Gd<StandardMaterial3D>>,
    is_remove: bool,
) {
    let x0 = chunk_x as f32 * chunk_width;
    let z0 = chunk_z as f32 * chunk_depth;
    let x1 = x0 + chunk_width;
    let z1 = z0 + chunk_depth;

    let mut lines = PackedVector3Array::new();

    if !has_chunk(chunk_x, chunk_z - 1) {
        lines.push(Vector3::new(x0, 0.0, z0));
        lines.push(Vector3::new(x1, 0.0, z0));
    }
    if !has_chunk(chunk_x + 1, chunk_z) {
        lines.push(Vector3::new(x1, 0.0, z0));
        lines.push(Vector3::new(x1, 0.0, z1));
    }
    if !has_chunk(chunk_x, chunk_z + 1) {
        lines.push(Vector3::new(x1, 0.0, z1));
        lines.push(Vector3::new(x0, 0.0, z1));
    }
    if !has_chunk(chunk_x - 1, chunk_z) {
        lines.push(Vector3::new(x0, 0.0, z1));
        lines.push(Vector3::new(x0, 0.0, z0));
    }

    if is_remove {
        lines.push(Vector3::new(x0, 0.0, z0));
        lines.push(Vector3::new(x1, 0.0, z1));
        lines.push(Vector3::new(x1, 0.0, z0));
        lines.push(Vector3::new(x0, 0.0, z1));
    } else {
        let mx = (x0 + x1) / 2.0;
        let mz = (z0 + z1) / 2.0;
        let qw = chunk_width * 0.25;
        let qd = chunk_depth * 0.25;
        lines.push(Vector3::new(mx - qw, 0.0, mz));
        lines.push(Vector3::new(mx + qw, 0.0, mz));
        lines.push(Vector3::new(mx, 0.0, mz - qd));
        lines.push(Vector3::new(mx, 0.0, mz + qd));
    }

    if !lines.is_empty() {
        if let Some(ref mat) = material {
            gizmo.add_lines(&lines, &mat.clone().upcast::<godot::classes::Material>());
        }
    }
}

/// State snapshot passed from editor plugin to gizmo plugin.
pub struct GizmoState {
    pub mode: TerrainToolMode,
    pub brush_type: BrushType,
    pub brush_position: Vector3,
    pub brush_size: f32,
    pub terrain_hovered: bool,
    pub flatten: bool,
    pub draw_height: f32,
    pub draw_pattern: HashMap<[i32; 2], HashMap<[i32; 2], f32>>,
}

/// Initialize gizmo materials. Must be called after construction.
pub fn init_gizmo_plugin(plugin: &mut Gd<PixyTerrainGizmoPlugin>) {
    plugin.bind_mut().create_materials();
}
