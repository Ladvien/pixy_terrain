// Editor plugin — implemented in Parts 15-17
// Stub types for gizmo compilation (Part 14)
use std::collections::HashMap;

use godot::classes::{EditorPlugin, IEditorPlugin};
use godot::prelude::*;

use crate::gizmo::GizmoState;

/// Tool modes for the terrain editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainToolMode {
    Brush,
    Level,
    Smooth,
    Bridge,
    GrassMask,
    VertexPaint,
    DebugBrush,
    ChunkManagement,
    TerrainSettings,
}

/// Brush shape types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushType {
    Round,
    Square,
}

#[derive(GodotClass)]
#[class(base=EditorPlugin, init, tool)]
pub struct PixyTerrainPlugin {
    base: Base<EditorPlugin>,

    // Brush state (stubs — will be fully implemented in Parts 15-17)
    pub current_tool_mode: TerrainToolMode,
    pub brush_type: BrushType,
    pub brush_position: Vector3,
    pub brush_size: f32,
    pub terrain_hovered: bool,
    pub flatten: bool,
    pub draw_height: f32,
    pub is_setting: bool,
    pub draw_height_set: bool,
    pub is_drawing: bool,
    pub current_draw_pattern: HashMap<[i32; 2], HashMap<[i32; 2], f32>>,
}

impl Default for TerrainToolMode {
    fn default() -> Self {
        TerrainToolMode::Brush
    }
}

impl Default for BrushType {
    fn default() -> Self {
        BrushType::Round
    }
}

#[godot_api]
impl IEditorPlugin for PixyTerrainPlugin {}

#[godot_api]
impl PixyTerrainPlugin {}

impl PixyTerrainPlugin {
    /// Build a GizmoState snapshot from current brush state.
    pub fn get_gizmo_state(&self) -> GizmoState {
        GizmoState {
            mode: self.current_tool_mode,
            brush_type: self.brush_type,
            brush_position: self.brush_position,
            brush_size: self.brush_size,
            terrain_hovered: self.terrain_hovered,
            flatten: self.flatten,
            draw_height: self.draw_height,
            draw_pattern: self.current_draw_pattern.clone(),
            is_setting: self.is_setting,
            draw_height_set: self.draw_height_set,
            is_drawing: self.is_drawing,
        }
    }
}
