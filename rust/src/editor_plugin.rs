use std::collections::HashMap;

use godot::classes::editor_plugin::AfterGuiInput;
use godot::classes::editor_plugin::CustomControlContainer;
use godot::classes::{
    Button, ButtonGroup, Camera3D, CenterContainer, CheckBox, ColorPickerButton, EditorPlugin,
    EditorResourcePicker, HBoxContainer, HSeparator, HSlider, IEditorPlugin, Input, InputEvent,
    InputEventKey, InputEventMouseButton, InputEventMouseMotion, Label, MarginContainer,
    OptionButton, PhysicsRayQueryParameters3D, ScrollContainer, SpinBox, VBoxContainer,
};
use godot::prelude::*;

use crate::chunk::PixyTerrainChunk;
use crate::gizmo::{self, GizmoState, PixyTerrainGizmoPlugin};
use crate::marching_squares;
use crate::quick_paint::PixyQuickPaint;
use crate::terrain::PixyTerrain;

/// Minimum width of the toolbar panel.
const TOOLBAR_MIN_WIDTH: f32 = 140.0;
/// Padding around toolbar content.
const TOOLBAR_MARGIN: i32 = 8;
/// Vertical separation between toolbar items.
const TOOLBAR_SEPARATION: i32 = 4;
/// Minimum button size for toolbar buttons.
const BUTTON_MIN_WIDTH: f32 = 100.0;
/// Minimum button height for toolbar buttons.
const BUTTON_MIN_HEIGHT: f32 = 28.0;
/// Maximum brush size.
const MAX_BRUSH_SIZE: f32 = 50.0;
/// Minimum brush size.
const MIN_BRUSH_SIZE: f32 = 1.0;
/// Scroll wheel brush size step.
const BRUSH_SIZE_STEP: f32 = 0.5;

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Replicates Godot's @GlobalScope.ease() function.
/// See: https://docs.godotengine.org/en/stable/classes/class_%40globalscope.html#class-globalscope-method-ease
fn godot_ease(x: f32, curve: f32) -> f32 {
    if x < 0.0 {
        return 0.0;
    }
    if x > 1.0 {
        return 1.0;
    }
    if curve > 0.0 {
        if curve < 1.0 {
            1.0 - (1.0 - x).powf(1.0 / curve)
        } else {
            x.powf(curve)
        }
    } else if curve < 0.0 {
        if x < 0.5 {
            (2.0 * x).powf(-curve) * 0.5
        } else {
            (1.0 - (2.0 * (1.0 - x)).powf(-curve)) * 0.5 + 0.5
        }
    } else {
        0.0
    }
}

// ═══════════════════════════════════════════
// Enums
// ═══════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainToolMode {
    #[default]
    Brush = 0,
    Level = 1,
    Smooth = 2,
    Bridge = 3,
    GrassMask = 4,
    VertexPaint = 5,
    DebugBrush = 6,
    ChunkManagement = 7,
    TerrainSettings = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushType {
    #[default]
    Round = 0,
    Square = 1,
}

// ═══════════════════════════════════════════
// Plugin Struct
// ═══════════════════════════════════════════

#[derive(GodotClass)]
#[class(tool, init, base=EditorPlugin)]
pub struct PixyTerrainPlugin {
    base: Base<EditorPlugin>,

    // UI state
    #[init(val = None)]
    current_terrain: Option<Gd<Node>>,
    #[init(val = None)]
    margin_container: Option<Gd<MarginContainer>>,
    #[init(val = None)]
    toolbar: Option<Gd<VBoxContainer>>,
    #[init(val = None)]
    generate_button: Option<Gd<Button>>,
    #[init(val = None)]
    clear_button: Option<Gd<Button>>,
    #[init(val = Vec::new())]
    tool_buttons: Vec<Gd<Button>>,
    #[init(val = None)]
    attributes_container: Option<Gd<ScrollContainer>>,
    #[init(val = None)]
    attributes_hbox: Option<Gd<HBoxContainer>>,
    #[init(val = false)]
    is_modifying: bool,

    // Tool mode
    #[init(val = TerrainToolMode::Brush)]
    mode: TerrainToolMode,
    #[init(val = BrushType::Round)]
    brush_type: BrushType,
    #[init(val = 15.0)]
    brush_size: f32,
    #[init(val = 1.0)]
    strength: f32,
    /// Target height for Level mode.
    #[init(val = 0.0)]
    height: f32,
    #[init(val = true)]
    flatten: bool,
    #[init(val = true)]
    falloff: bool,
    /// Ease value for bridge mode (-1.0 = no ease).
    #[init(val = -1.0)]
    ease_value: f32,
    #[init(val = false)]
    should_mask_grass: bool,

    // Vertex paint state
    #[init(val = 0)]
    vertex_color_idx: i32,
    #[init(val = Color::from_rgba(1.0, 0.0, 0.0, 0.0))]
    vertex_color_0: Color,
    #[init(val = Color::from_rgba(1.0, 0.0, 0.0, 0.0))]
    vertex_color_1: Color,
    #[init(val = false)]
    paint_walls_mode: bool,

    // Drawing state
    #[init(val = Vector3::ZERO)]
    brush_position: Vector3,
    #[init(val = false)]
    terrain_hovered: bool,
    #[init(val = HashMap::new())]
    current_draw_pattern: HashMap<[i32; 2], HashMap<[i32; 2], f32>>,
    #[init(val = false)]
    is_drawing: bool,
    #[init(val = false)]
    draw_height_set: bool,
    #[init(val = 0.0)]
    draw_height: f32,
    #[init(val = false)]
    is_setting: bool,

    // Gizmo plugin
    #[init(val = None)]
    gizmo_plugin: Option<Gd<PixyTerrainGizmoPlugin>>,

    // Right-side texture settings panel
    #[init(val = None)]
    texture_panel: Option<Gd<ScrollContainer>>,

    // Bridge state
    #[init(val = false)]
    is_making_bridge: bool,
    #[init(val = Vector3::ZERO)]
    bridge_start_pos: Vector3,
    #[init(val = Vector3::ZERO)]
    base_position: Vector3,
    /// Chunk where bridge started (for cross-chunk offset calculation).
    #[init(val = Vector2i::ZERO)]
    bridge_start_chunk: Vector2i,

    // QuickPaint presets
    #[init(val = Vec::new())]
    quick_paint_presets: Vec<Gd<PixyQuickPaint>>,
    #[init(val = None)]
    current_quick_paint: Option<Gd<PixyQuickPaint>>,

    // Chunk management state
    #[init(val = None)]
    selected_chunk_coords: Option<Vector2i>,
}

// ═══════════════════════════════════════════
// IEditorPlugin Implementation
// ═══════════════════════════════════════════

#[godot_api]
impl IEditorPlugin for PixyTerrainPlugin {
    fn enter_tree(&mut self) {
        godot_print!("PixyTerrainPlugin: enter_tree called");

        let mut margin_container = MarginContainer::new_alloc();
        margin_container.set_name("PixyTerrainMargin");
        margin_container.set_visible(false);
        margin_container.set_custom_minimum_size(Vector2::new(TOOLBAR_MIN_WIDTH, 0.0));
        margin_container.add_theme_constant_override("margin_top", TOOLBAR_MARGIN);
        margin_container.add_theme_constant_override("margin_left", TOOLBAR_MARGIN);
        margin_container.add_theme_constant_override("margin_right", TOOLBAR_MARGIN);
        margin_container.add_theme_constant_override("margin_bottom", TOOLBAR_MARGIN);

        let mut toolbar = VBoxContainer::new_alloc();
        toolbar.set_name("PixyTerrainToolbar");
        toolbar.add_theme_constant_override("separation", TOOLBAR_SEPARATION);

        // Generation Section
        let mut gen_label = Label::new_alloc();
        gen_label.set_text("Generation");
        toolbar.add_child(&gen_label);

        let mut generate_button = Button::new_alloc();
        generate_button.set_text("Generate (G)");
        generate_button.set_custom_minimum_size(Vector2::new(BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT));

        let mut clear_button = Button::new_alloc();
        clear_button.set_text("Clear (C)");
        clear_button.set_custom_minimum_size(Vector2::new(BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT));

        toolbar.add_child(&generate_button);
        toolbar.add_child(&clear_button);

        // ── Tool Mode Buttons ──
        let sep = HSeparator::new_alloc();
        toolbar.add_child(&sep);

        let mut tools_label = Label::new_alloc();
        tools_label.set_text("Tools");
        toolbar.add_child(&tools_label);

        let button_group = ButtonGroup::new_gd();
        let tool_labels = [
            "Brush",
            "Level",
            "Smooth",
            "Bridge",
            "Grass Mask",
            "Vertex Paint",
            "Debug",
            "Chunks",
            "Settings",
        ];

        let plugin_ref = self.to_gd();
        let mut tool_buttons: Vec<Gd<Button>> = Vec::new();

        for (i, label) in tool_labels.iter().enumerate() {
            // Add separators before visual, utility, and settings groups
            if i == 4 || i == 6 || i == 7 {
                let group_sep = HSeparator::new_alloc();
                toolbar.add_child(&group_sep);
            }

            let mut btn = Button::new_alloc();
            btn.set_text(*label);
            btn.set_toggle_mode(true);
            btn.set_button_group(&button_group);
            btn.set_custom_minimum_size(Vector2::new(BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT));

            let callable = Callable::from_object_method(&plugin_ref, "on_tool_button_toggled")
                .bindv(&varray![i as i32]);
            btn.connect("toggled", &callable);

            toolbar.add_child(&btn);
            tool_buttons.push(btn);
        }

        // Pre-press Brush button
        if let Some(first_btn) = tool_buttons.first_mut() {
            first_btn.set_pressed(true);
        }

        margin_container.add_child(&toolbar);

        // Connect generation signals
        generate_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_generate_pressed"),
        );
        clear_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_clear_pressed"),
        );

        self.base_mut().add_control_to_container(
            CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT,
            &margin_container,
        );

        // ── Bottom Attributes Panel ──
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_name("PixyTerrainAttributes");
        scroll.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        scroll.set_vertical_scroll_mode(godot::classes::scroll_container::ScrollMode::DISABLED);
        scroll.set_visible(false);

        let hbox = HBoxContainer::new_alloc();
        scroll.add_child(&hbox);

        self.base_mut()
            .add_control_to_container(CustomControlContainer::SPATIAL_EDITOR_BOTTOM, &scroll);

        self.attributes_container = Some(scroll);
        self.attributes_hbox = Some(hbox);
        self.margin_container = Some(margin_container);
        self.toolbar = Some(toolbar);
        self.generate_button = Some(generate_button);
        self.clear_button = Some(clear_button);
        self.tool_buttons = tool_buttons;

        // Register gizmo plugin
        let mut gizmo_plugin = Gd::<PixyTerrainGizmoPlugin>::default();
        gizmo::init_gizmo_plugin(&mut gizmo_plugin);
        gizmo_plugin.bind_mut().plugin_ref = Some(self.to_gd());
        self.base_mut().add_node_3d_gizmo_plugin(&gizmo_plugin);
        self.gizmo_plugin = Some(gizmo_plugin);

        // ── Right-Side Texture Settings Panel ──
        let mut tex_scroll = ScrollContainer::new_alloc();
        tex_scroll.set_name("PixyTerrainTextureSettings");
        tex_scroll.set_custom_minimum_size(Vector2::new(220.0, 0.0));
        tex_scroll.set_visible(false);

        self.base_mut().add_control_to_container(
            CustomControlContainer::SPATIAL_EDITOR_SIDE_RIGHT,
            &tex_scroll,
        );
        self.texture_panel = Some(tex_scroll);

        godot_print!("PixyTerrainPlugin: toolbar added");
    }

    fn exit_tree(&mut self) {
        // Unregister gizmo plugin
        if let Some(gizmo_plugin) = self.gizmo_plugin.take() {
            self.base_mut().remove_node_3d_gizmo_plugin(&gizmo_plugin);
        }

        self.generate_button = None;
        self.clear_button = None;
        self.tool_buttons.clear();
        self.toolbar = None;
        self.attributes_hbox = None;

        if let Some(mut scroll) = self.attributes_container.take() {
            self.base_mut().remove_control_from_container(
                CustomControlContainer::SPATIAL_EDITOR_BOTTOM,
                &scroll,
            );
            scroll.queue_free();
        }

        if let Some(mut margin) = self.margin_container.take() {
            self.base_mut().remove_control_from_container(
                CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT,
                &margin,
            );
            margin.queue_free();
        }

        if let Some(mut tex_panel) = self.texture_panel.take() {
            self.base_mut().remove_control_from_container(
                CustomControlContainer::SPATIAL_EDITOR_SIDE_RIGHT,
                &tex_panel,
            );
            tex_panel.queue_free();
        }
    }

    fn handles(&self, object: Gd<Object>) -> bool {
        object.get_class() == "PixyTerrain"
    }

    fn edit(&mut self, object: Option<Gd<Object>>) {
        if let Some(obj) = object {
            if let Ok(node) = obj.try_cast::<Node>() {
                self.current_terrain = Some(node);
                self.set_ui_visible(true);
                return;
            }
        }
        self.set_ui_visible(false);
        self.current_draw_pattern.clear();
        self.is_drawing = false;
        self.draw_height_set = false;
    }

    fn make_visible(&mut self, visible: bool) {
        if !visible && self.is_modifying {
            return;
        }
        self.set_ui_visible(visible);
        if !visible {
            self.current_terrain = None;
        }
    }

    fn forward_3d_gui_input(
        &mut self,
        camera: Option<Gd<Camera3D>>,
        event: Option<Gd<InputEvent>>,
    ) -> i32 {
        let Some(event) = event else {
            return AfterGuiInput::PASS.ord();
        };

        // Keyboard shortcuts for Generate / Clear
        if let Ok(key_event) = event.clone().try_cast::<InputEventKey>() {
            if key_event.is_pressed() && !key_event.is_echo() {
                match key_event.get_keycode() {
                    godot::global::Key::G => {
                        self.do_generate();
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::C => {
                        self.do_clear();
                        return AfterGuiInput::STOP.ord();
                    }
                    _ => {}
                }
            }
        }

        // Only handle mouse events from here
        let Some(camera) = camera else {
            return AfterGuiInput::PASS.ord();
        };

        let Some(terrain_node) = self
            .current_terrain
            .as_ref()
            .filter(|t| t.is_instance_valid())
            .cloned()
        else {
            return AfterGuiInput::PASS.ord();
        };

        // Get mouse position from event
        let mouse_pos;
        let is_button_event;
        let is_motion_event;

        if let Ok(btn) = event.clone().try_cast::<InputEventMouseButton>() {
            mouse_pos = btn.get_position();
            is_button_event = true;
            is_motion_event = false;
        } else if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
            mouse_pos = motion.get_position();
            is_button_event = false;
            is_motion_event = true;
        } else {
            return AfterGuiInput::PASS.ord();
        }

        let terrain_gd: Gd<Node3D> = terrain_node.clone().cast();

        // Compute ray
        let ray_origin = camera.project_ray_origin(mouse_pos);
        let ray_dir = camera.project_ray_normal(mouse_pos);

        let input = Input::singleton();
        let shift_held = input.is_key_pressed(godot::global::Key::SHIFT);
        let alt_held = input.is_key_pressed(godot::global::Key::ALT);
        let ctrl_held = input.is_key_pressed(godot::global::Key::CTRL);

        // Get terrain dimensions
        let terrain: Gd<PixyTerrain> = terrain_node.clone().cast();
        let (dim, cell_size) = {
            let t = terrain.bind();
            (t.dimensions, t.cell_size)
        };

        // ── Brush/drawing tool modes ──
        let is_draw_mode = matches!(
            self.mode,
            TerrainToolMode::Brush
                | TerrainToolMode::Level
                | TerrainToolMode::Smooth
                | TerrainToolMode::Bridge
                | TerrainToolMode::GrassMask
                | TerrainToolMode::VertexPaint
                | TerrainToolMode::DebugBrush
        );

        if is_draw_mode {
            self.terrain_hovered = false;
            let mut draw_position: Option<Vector3> = None;

            // Raycast strategy depends on current state
            if self.is_setting && self.draw_height_set {
                // Setting mode: vertical plane through base_position
                let terrain_transform = terrain_gd.get_global_transform();
                let local_ray_dir = terrain_transform.basis.inverse() * ray_dir;
                let set_normal = Vector3::new(local_ray_dir.x, 0.0, local_ray_dir.z).normalized();
                if set_normal.length() > 0.001 {
                    let d = set_normal.dot(self.base_position);
                    let set_plane = Plane::new(set_normal, d);
                    let local_origin = terrain_gd.to_local(ray_origin);
                    if let Some(pos) = set_plane.intersect_ray(local_origin, local_ray_dir) {
                        self.brush_position = pos;
                    }
                }
            } else if !self.current_draw_pattern.is_empty() && self.flatten {
                // Flatten mode: horizontal plane at draw_height
                let chunk_plane = Plane::new(Vector3::UP, self.draw_height);
                if let Some(world_pos) = chunk_plane.intersect_ray(ray_origin, ray_dir) {
                    draw_position = Some(terrain_gd.to_local(world_pos));
                }
            } else {
                // Default: physics raycast
                if let Some(mut world) = camera.get_world_3d() {
                    if let Some(mut space) = world.get_direct_space_state() {
                        let ray_end = ray_origin + ray_dir * 10000.0;
                        let query = PhysicsRayQueryParameters3D::create_ex(ray_origin, ray_end)
                            .collision_mask(16)
                            .done()
                            .unwrap();
                        let result = space.intersect_ray(&query);
                        if !result.is_empty() {
                            if let Some(pos_variant) = result.get("position") {
                                let world_pos: Vector3 = pos_variant.to();
                                draw_position = Some(terrain_gd.to_local(world_pos));
                            }
                        }
                    }
                }
            }

            let draw_area_hovered = draw_position.is_some();
            if let Some(pos) = draw_position {
                self.terrain_hovered = true;
                // Don't overwrite brush_position when in setting mode (already set above)
                if !(self.is_setting && self.draw_height_set) {
                    self.brush_position = pos;
                }
            }

            // ALT to clear pattern (unless setting)
            if alt_held && !self.is_setting {
                self.current_draw_pattern.clear();
            }

            // ── Mouse button handling ──
            if is_button_event {
                let btn: Gd<InputEventMouseButton> = event.clone().cast();
                if btn.get_button_index() == godot::global::MouseButton::LEFT {
                    if btn.is_pressed() && draw_area_hovered {
                        self.draw_height_set = false;

                        // Mode-specific press initialization
                        if self.mode == TerrainToolMode::Bridge && !self.is_making_bridge {
                            self.flatten = false;
                            self.is_making_bridge = true;
                            self.bridge_start_pos = self.brush_position;
                            // Capture chunk where bridge started for cross-chunk offset
                            let chunk_width = (dim.x - 1) as f32 * cell_size.x;
                            let chunk_depth = (dim.z - 1) as f32 * cell_size.y;
                            self.bridge_start_chunk = Vector2i::new(
                                (self.brush_position.x / chunk_width).floor() as i32,
                                (self.brush_position.z / chunk_depth).floor() as i32,
                            );
                        }
                        if self.mode == TerrainToolMode::Smooth && !self.falloff {
                            self.falloff = true;
                        }
                        if matches!(
                            self.mode,
                            TerrainToolMode::GrassMask | TerrainToolMode::DebugBrush
                        ) && self.falloff
                        {
                            self.falloff = false;
                        }
                        if matches!(
                            self.mode,
                            TerrainToolMode::GrassMask
                                | TerrainToolMode::VertexPaint
                                | TerrainToolMode::DebugBrush
                        ) && self.flatten
                        {
                            self.flatten = false;
                        }

                        if self.mode == TerrainToolMode::Level && ctrl_held {
                            // Ctrl+click in Level mode: set target height from click pos
                            self.height = self.brush_position.y;
                        } else if shift_held {
                            // Shift+click: enter drawing mode
                            self.is_drawing = true;
                        } else {
                            // Normal click: enter setting mode
                            self.is_setting = true;
                            if !self.flatten {
                                self.draw_height = self.brush_position.y;
                            }
                        }

                        // Initialize draw_height_set (moved from gizmo)
                        self.initialize_draw_state(&terrain, dim, cell_size);

                        // Build initial pattern
                        if self.is_drawing {
                            self.build_draw_pattern(&terrain, dim, cell_size);
                        }
                    } else if !btn.is_pressed() {
                        // Mouse button released
                        if self.is_making_bridge {
                            self.is_making_bridge = false;
                        }
                        if self.is_drawing {
                            self.is_drawing = false;
                            if matches!(
                                self.mode,
                                TerrainToolMode::GrassMask
                                    | TerrainToolMode::Level
                                    | TerrainToolMode::Bridge
                                    | TerrainToolMode::DebugBrush
                            ) {
                                self.draw_pattern(&terrain, dim, cell_size);
                                self.current_draw_pattern.clear();
                            }
                            if matches!(
                                self.mode,
                                TerrainToolMode::Smooth | TerrainToolMode::VertexPaint
                            ) {
                                self.current_draw_pattern.clear();
                            }
                        }
                        if self.is_setting {
                            self.is_setting = false;
                            self.draw_pattern(&terrain, dim, cell_size);
                            if shift_held {
                                self.draw_height = self.brush_position.y;
                            } else {
                                self.current_draw_pattern.clear();
                            }
                        }
                    }
                    return AfterGuiInput::STOP.ord();
                }

                // Shift+scroll wheel: adjust brush size
                if shift_held {
                    let button_idx = btn.get_button_index();
                    let factor = if btn.get_factor() != 0.0 {
                        btn.get_factor()
                    } else {
                        1.0
                    };
                    if button_idx == godot::global::MouseButton::WHEEL_UP {
                        self.brush_size =
                            (self.brush_size + BRUSH_SIZE_STEP * factor).min(MAX_BRUSH_SIZE);
                        return AfterGuiInput::STOP.ord();
                    } else if button_idx == godot::global::MouseButton::WHEEL_DOWN {
                        self.brush_size =
                            (self.brush_size - BRUSH_SIZE_STEP * factor).max(MIN_BRUSH_SIZE);
                        return AfterGuiInput::STOP.ord();
                    }
                }
            }

            // ── Mouse motion while drawing ──
            if is_motion_event && draw_area_hovered && self.is_drawing {
                self.build_draw_pattern(&terrain, dim, cell_size);

                // Continuous modes: apply immediately
                if matches!(
                    self.mode,
                    TerrainToolMode::Smooth
                        | TerrainToolMode::VertexPaint
                        | TerrainToolMode::GrassMask
                ) {
                    self.draw_pattern(&terrain, dim, cell_size);
                    self.current_draw_pattern.clear();
                }
            }

            // Trigger gizmo redraw so brush visualization updates
            self.update_gizmos();

            return AfterGuiInput::PASS.ord();
        }

        // ── Chunk Management mode ──
        if self.mode == TerrainToolMode::ChunkManagement {
            let chunk_plane = Plane::new(Vector3::UP, 0.0);
            if let Some(intersection) = chunk_plane.intersect_ray(ray_origin, ray_dir) {
                let chunk_width = (dim.x - 1) as f32 * cell_size.x;
                let chunk_depth = (dim.z - 1) as f32 * cell_size.y;
                let chunk_x = (intersection.x / chunk_width).floor() as i32;
                let chunk_z = (intersection.z / chunk_depth).floor() as i32;

                if is_button_event {
                    let btn: Gd<InputEventMouseButton> = event.clone().cast();
                    if btn.is_pressed()
                        && btn.get_button_index() == godot::global::MouseButton::LEFT
                    {
                        let has = terrain.bind().has_chunk(chunk_x, chunk_z);

                        if has {
                            // Remove existing chunk
                            self.register_chunk_undo_redo(
                                &terrain_node,
                                chunk_x,
                                chunk_z,
                                "remove chunk",
                                true,
                            );
                            return AfterGuiInput::STOP.ord();
                        } else {
                            // Add new chunk if adjacent to existing
                            let t = terrain.bind();
                            let can_add = t.get_chunk_keys().is_empty()
                                || t.has_chunk(chunk_x - 1, chunk_z)
                                || t.has_chunk(chunk_x + 1, chunk_z)
                                || t.has_chunk(chunk_x, chunk_z - 1)
                                || t.has_chunk(chunk_x, chunk_z + 1);
                            drop(t);

                            if can_add {
                                self.register_chunk_undo_redo(
                                    &terrain_node,
                                    chunk_x,
                                    chunk_z,
                                    "add chunk",
                                    false,
                                );
                                return AfterGuiInput::STOP.ord();
                            }
                        }
                    }
                }
            }

            // Consume left clicks in chunk management mode
            if is_button_event {
                let btn: Gd<InputEventMouseButton> = event.clone().cast();
                if btn.is_pressed() && btn.get_button_index() == godot::global::MouseButton::LEFT {
                    return AfterGuiInput::STOP.ord();
                }
            }
        }

        AfterGuiInput::PASS.ord()
    }
}

// ═══════════════════════════════════════════
// #[func] Methods (callable from GDScript / undo-redo)
// ═══════════════════════════════════════════

#[godot_api]
impl PixyTerrainPlugin {
    #[func]
    fn on_generate_pressed(&mut self) {
        self.do_generate();
    }

    #[func]
    fn on_clear_pressed(&mut self) {
        self.do_clear();
    }

    /// Called when a tool mode toggle button is pressed.
    /// Godot passes signal args first (pressed: bool), then bound args (tool_index: i32).
    #[func]
    fn on_tool_button_toggled(&mut self, pressed: bool, tool_index: i32) {
        if !pressed {
            return;
        }
        self.mode = match tool_index {
            0 => TerrainToolMode::Brush,
            1 => TerrainToolMode::Level,
            2 => TerrainToolMode::Smooth,
            3 => TerrainToolMode::Bridge,
            4 => TerrainToolMode::GrassMask,
            5 => TerrainToolMode::VertexPaint,
            6 => TerrainToolMode::DebugBrush,
            7 => TerrainToolMode::ChunkManagement,
            8 => TerrainToolMode::TerrainSettings,
            _ => TerrainToolMode::Brush,
        };
        self.rebuild_attributes();
    }

    /// Called when an attribute control value changes.
    /// Godot passes signal args first (value: Variant), then bound args (setting_name: GString).
    #[func]
    fn on_attribute_changed(&mut self, value: Variant, setting_name: GString) {
        match setting_name.to_string().as_str() {
            "brush_type" => {
                let idx: i64 = value.to();
                self.brush_type = if idx == 0 {
                    BrushType::Round
                } else {
                    BrushType::Square
                };
            }
            "size" => {
                self.brush_size = value.to::<f64>() as f32;
            }
            "strength" => {
                self.strength = value.to::<f64>() as f32;
            }
            "height" => {
                self.height = value.to::<f64>() as f32;
            }
            "flatten" => {
                self.flatten = value.to();
            }
            "falloff" => {
                self.falloff = value.to();
            }
            "ease_value" => {
                self.ease_value = value.to::<f64>() as f32;
            }
            "mask_mode" => {
                self.should_mask_grass = value.to();
            }
            "material" => {
                let idx: i64 = value.to();
                self.set_vertex_colors(idx as i32);
            }
            "paint_walls" => {
                self.paint_walls_mode = value.to();
            }
            "quick_paint" => {
                let idx: i64 = value.to();
                if idx == 0 {
                    // "None" option
                    self.current_quick_paint = None;
                } else {
                    let preset_idx = (idx - 1) as usize;
                    self.current_quick_paint = self.quick_paint_presets.get(preset_idx).cloned();
                }
            }
            "chunk_select" => {
                // Update selected chunk from dropdown index
                if let Some(ref terrain) = self.current_terrain {
                    if terrain.is_instance_valid() {
                        let t: Gd<PixyTerrain> = terrain.clone().cast();
                        let keys = t.bind().get_chunk_keys();
                        let idx = value.to::<i64>() as usize;
                        if idx < keys.len() {
                            let k = keys[idx];
                            self.selected_chunk_coords =
                                Some(Vector2i::new(k.x as i32, k.y as i32));
                            // Rebuild to update merge mode display
                            self.rebuild_attributes();
                        }
                    }
                }
            }
            "chunk_merge_mode" => {
                // Set merge mode on selected chunk
                if let Some(ref terrain) = self.current_terrain {
                    if terrain.is_instance_valid() {
                        let t: Gd<PixyTerrain> = terrain.clone().cast();
                        if let Some(sel) = self.selected_chunk_coords {
                            if let Some(mut chunk) = t.bind().get_chunk(sel.x, sel.y) {
                                chunk.bind_mut().merge_mode = value.to::<i64>() as i32;
                                chunk.bind_mut().regenerate_mesh();
                            }
                        }
                    }
                }
            }
            // ── Terrain Settings ──
            "dim_x"
            | "dim_z"
            | "dim_y"
            | "cell_size_x"
            | "cell_size_z"
            | "blend_mode"
            | "wall_threshold"
            | "ridge_threshold"
            | "ledge_threshold"
            | "merge_mode"
            | "grass_subdivisions"
            | "grass_size_x"
            | "grass_size_y"
            | "default_wall_texture"
            | "blend_sharpness"
            | "blend_noise_scale"
            | "blend_noise_strength"
            | "animation_fps"
            | "use_ridge_texture"
            | "extra_collision_layer" => {
                self.apply_terrain_setting(setting_name.to_string().as_str(), &value);
            }
            // ── Texture Panel Settings ──
            name if name.starts_with("tex_scale_")
                || name.starts_with("tex_has_grass_")
                || name.starts_with("ground_color_") =>
            {
                self.apply_terrain_setting(name, &value);
            }
            _ => {}
        }
    }

    /// Apply a composite pattern action. Called by undo/redo.
    /// `patterns` is a VarDictionary with keys: "height", "color_0", "color_1",
    /// "wall_color_0", "wall_color_1", "grass_mask".
    /// Each value is Dict<Vector2i(chunk), Dict<Vector2i(cell), value>>.
    #[func]
    fn apply_composite_pattern_action(&mut self, terrain: Gd<Node>, patterns: VarDictionary) {
        let terrain: Gd<PixyTerrain> = terrain.cast();
        let mut affected_chunks: HashMap<[i32; 2], Gd<PixyTerrainChunk>> = HashMap::new();

        // Apply order: wall_color_0, wall_color_1, height, grass_mask, color_0, color_1
        let keys_in_order = [
            "wall_color_0",
            "wall_color_1",
            "height",
            "grass_mask",
            "color_0",
            "color_1",
        ];

        for &key in &keys_in_order {
            let Some(outer_variant) = patterns.get(key) else {
                continue;
            };
            let outer_dict: VarDictionary = outer_variant.to();

            for (chunk_key, chunk_value) in outer_dict.iter_shared() {
                let chunk_coords: Vector2i = chunk_key.to();
                let cell_dict: VarDictionary = chunk_value.to();

                let chunk = terrain.bind().get_chunk(chunk_coords.x, chunk_coords.y);
                let Some(mut chunk) = chunk else {
                    continue;
                };

                affected_chunks
                    .entry([chunk_coords.x, chunk_coords.y])
                    .or_insert_with(|| chunk.clone());

                for (cell_key, cell_value) in cell_dict.iter_shared() {
                    let cell: Vector2i = cell_key.to();
                    let mut c = chunk.bind_mut();
                    match key {
                        "height" => {
                            let h: f32 = cell_value.to();
                            c.draw_height(cell.x, cell.y, h);
                        }
                        "color_0" => {
                            let color: Color = cell_value.to();
                            c.draw_color_0(cell.x, cell.y, color);
                        }
                        "color_1" => {
                            let color: Color = cell_value.to();
                            c.draw_color_1(cell.x, cell.y, color);
                        }
                        "wall_color_0" => {
                            let color: Color = cell_value.to();
                            c.draw_wall_color_0(cell.x, cell.y, color);
                        }
                        "wall_color_1" => {
                            let color: Color = cell_value.to();
                            c.draw_wall_color_1(cell.x, cell.y, color);
                        }
                        "grass_mask" => {
                            let mask: Color = cell_value.to();
                            c.draw_grass_mask(cell.x, cell.y, mask);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Regenerate mesh once per affected chunk
        for (_, mut chunk) in affected_chunks {
            chunk.bind_mut().regenerate_mesh();
        }
    }

    /// Called when a texture resource is changed via EditorResourcePicker.
    /// Godot passes signal args first (resource), then bound args (setting_name).
    #[func]
    fn on_texture_resource_changed(&mut self, resource: Variant, setting_name: GString) {
        let Some(ref terrain_node) = self.current_terrain else {
            return;
        };
        if !terrain_node.is_instance_valid() {
            return;
        }
        let mut terrain: Gd<PixyTerrain> = terrain_node.clone().cast();

        let name = setting_name.to_string();
        let tex: Option<Gd<godot::classes::Texture2D>> = if resource.is_nil() {
            None
        } else {
            Some(resource.to())
        };

        {
            let mut t = terrain.bind_mut();

            if let Some(slot_str) = name.strip_prefix("ground_tex_") {
                let slot: i32 = slot_str.parse().unwrap_or(1);
                match slot {
                    1 => t.ground_texture = tex,
                    2 => t.texture_2 = tex,
                    3 => t.texture_3 = tex,
                    4 => t.texture_4 = tex,
                    5 => t.texture_5 = tex,
                    6 => t.texture_6 = tex,
                    7 => t.texture_7 = tex,
                    8 => t.texture_8 = tex,
                    9 => t.texture_9 = tex,
                    10 => t.texture_10 = tex,
                    11 => t.texture_11 = tex,
                    12 => t.texture_12 = tex,
                    13 => t.texture_13 = tex,
                    14 => t.texture_14 = tex,
                    15 => t.texture_15 = tex,
                    _ => {}
                }
            } else if let Some(slot_str) = name.strip_prefix("grass_sprite_") {
                let slot: i32 = slot_str.parse().unwrap_or(1);
                match slot {
                    1 => t.grass_sprite = tex,
                    2 => t.grass_sprite_tex_2 = tex,
                    3 => t.grass_sprite_tex_3 = tex,
                    4 => t.grass_sprite_tex_4 = tex,
                    5 => t.grass_sprite_tex_5 = tex,
                    6 => t.grass_sprite_tex_6 = tex,
                    _ => {}
                }
            }
        }

        // Sync shader uniforms
        terrain.bind_mut().force_batch_update();
    }
}

// ═══════════════════════════════════════════
// Private Implementation
// ═══════════════════════════════════════════

impl PixyTerrainPlugin {
    fn set_ui_visible(&mut self, visible: bool) {
        if let Some(ref mut margin) = self.margin_container {
            margin.set_visible(visible);
        }
        if let Some(ref mut scroll) = self.attributes_container {
            scroll.set_visible(visible);
        }
        if let Some(ref mut tex_panel) = self.texture_panel {
            tex_panel.set_visible(visible);
        }
        if visible {
            self.rebuild_attributes();
            self.rebuild_texture_panel();
        }
    }

    /// Rebuild the bottom attributes panel controls based on the current tool mode.
    fn rebuild_attributes(&mut self) {
        // Clear existing children
        if let Some(ref mut hbox) = self.attributes_hbox {
            // Remove all children
            let count = hbox.get_child_count();
            for i in (0..count).rev() {
                if let Some(mut child) = hbox.get_child(i) {
                    hbox.remove_child(&child);
                    child.queue_free();
                }
            }
        }

        let plugin_ref = self.to_gd();

        match self.mode {
            TerrainToolMode::Brush => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_checkbox_attribute("flatten", "Flatten", self.flatten, &plugin_ref);
                self.add_checkbox_attribute("falloff", "Falloff", self.falloff, &plugin_ref);
                self.add_quick_paint_dropdown(&plugin_ref);
            }
            TerrainToolMode::Level => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "height",
                    "Height",
                    -50.0,
                    50.0,
                    0.1,
                    self.height as f64,
                    &plugin_ref,
                );
                self.add_checkbox_attribute("falloff", "Falloff", self.falloff, &plugin_ref);
                self.add_quick_paint_dropdown(&plugin_ref);
            }
            TerrainToolMode::Smooth => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "strength",
                    "Strength",
                    0.1,
                    10.0,
                    0.1,
                    self.strength as f64,
                    &plugin_ref,
                );
                self.add_quick_paint_dropdown(&plugin_ref);
            }
            TerrainToolMode::Bridge => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "ease_value",
                    "Ease",
                    -5.0,
                    5.0,
                    0.1,
                    self.ease_value as f64,
                    &plugin_ref,
                );
                self.add_quick_paint_dropdown(&plugin_ref);
            }
            TerrainToolMode::GrassMask => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                self.add_checkbox_attribute(
                    "mask_mode",
                    "Mask",
                    self.should_mask_grass,
                    &plugin_ref,
                );
            }
            TerrainToolMode::VertexPaint => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
                let mat_options: Vec<&str> = (0..15)
                    .map(|i| match i {
                        0 => "Tex 0",
                        1 => "Tex 1",
                        2 => "Tex 2",
                        3 => "Tex 3",
                        4 => "Tex 4",
                        5 => "Tex 5",
                        6 => "Tex 6",
                        7 => "Tex 7",
                        8 => "Tex 8",
                        9 => "Tex 9",
                        10 => "Tex 10",
                        11 => "Tex 11",
                        12 => "Tex 12",
                        13 => "Tex 13",
                        14 => "Tex 14",
                        _ => "Wall",
                    })
                    .collect();
                self.add_option_attribute(
                    "material",
                    "Material",
                    &mat_options,
                    self.vertex_color_idx as i64,
                    &plugin_ref,
                );
                self.add_checkbox_attribute(
                    "paint_walls",
                    "Paint Walls",
                    self.paint_walls_mode,
                    &plugin_ref,
                );
            }
            TerrainToolMode::DebugBrush => {
                self.add_option_attribute(
                    "brush_type",
                    "Brush",
                    &["Round", "Square"],
                    self.brush_type as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "size",
                    "Size",
                    MIN_BRUSH_SIZE as f64,
                    MAX_BRUSH_SIZE as f64,
                    BRUSH_SIZE_STEP as f64,
                    self.brush_size as f64,
                    &plugin_ref,
                );
            }
            TerrainToolMode::ChunkManagement => {
                // Chunk selector and merge mode selector
                if let Some(ref terrain) = self.current_terrain {
                    if terrain.is_instance_valid() {
                        let t: Gd<PixyTerrain> = terrain.clone().cast();
                        let keys = t.bind().get_chunk_keys();

                        if !keys.is_empty() {
                            // Build chunk selection options
                            let mut chunk_options: Vec<String> = Vec::new();
                            for i in 0..keys.len() {
                                let k = keys[i];
                                chunk_options
                                    .push(format!("Chunk ({}, {})", k.x as i32, k.y as i32));
                            }
                            let chunk_refs: Vec<&str> =
                                chunk_options.iter().map(|s| s.as_str()).collect();

                            // Determine current selection
                            let current_idx = if let Some(sel) = self.selected_chunk_coords {
                                (0..keys.len())
                                    .find(|&i| {
                                        let k = keys[i];
                                        k.x as i32 == sel.x && k.y as i32 == sel.y
                                    })
                                    .unwrap_or(0)
                            } else {
                                0
                            };

                            self.add_option_attribute(
                                "chunk_select",
                                "Chunk",
                                &chunk_refs,
                                current_idx as i64,
                                &plugin_ref,
                            );

                            // Get selected chunk's merge mode
                            let sel_coords = if let Some(sel) = self.selected_chunk_coords {
                                sel
                            } else if !keys.is_empty() {
                                let k = keys[0];
                                Vector2i::new(k.x as i32, k.y as i32)
                            } else {
                                Vector2i::ZERO
                            };

                            let merge_mode = if let Some(chunk) =
                                t.bind().get_chunk(sel_coords.x, sel_coords.y)
                            {
                                chunk.bind().merge_mode
                            } else {
                                1 // Default to Polyhedron
                            };

                            self.add_option_attribute(
                                "chunk_merge_mode",
                                "Merge",
                                &[
                                    "Cubic",
                                    "Polyhedron",
                                    "RoundedPoly",
                                    "SemiRound",
                                    "Spherical",
                                ],
                                merge_mode as i64,
                                &plugin_ref,
                            );
                        }
                    }
                }
            }
            TerrainToolMode::TerrainSettings => {
                // Read current terrain values for display
                let (
                    dims,
                    cell_sz,
                    blend,
                    wall_th,
                    ridge_th,
                    ledge_th,
                    merge,
                    grass_sub,
                    grass_sz,
                    def_wall,
                    blend_sharp,
                    blend_ns,
                    blend_nstr,
                    anim_fps,
                    use_ridge_tex,
                    extra_coll,
                ) = if let Some(ref terrain) = self.current_terrain {
                    if terrain.is_instance_valid() {
                        let t: Gd<PixyTerrain> = terrain.clone().cast();
                        let tb = t.bind();
                        (
                            tb.dimensions,
                            tb.cell_size,
                            tb.blend_mode,
                            tb.wall_threshold,
                            tb.ridge_threshold,
                            tb.ledge_threshold,
                            tb.merge_mode,
                            tb.grass_subdivisions,
                            tb.grass_size,
                            tb.default_wall_texture,
                            tb.blend_sharpness,
                            tb.blend_noise_scale,
                            tb.blend_noise_strength,
                            tb.animation_fps,
                            tb.use_ridge_texture,
                            tb.extra_collision_layer,
                        )
                    } else {
                        return;
                    }
                } else {
                    return;
                };

                self.add_spinbox_attribute(
                    "dim_x",
                    "Dim X",
                    3.0,
                    129.0,
                    1.0,
                    dims.x as f64,
                    &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "dim_z",
                    "Dim Z",
                    3.0,
                    129.0,
                    1.0,
                    dims.z as f64,
                    &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "dim_y",
                    "Height",
                    1.0,
                    256.0,
                    1.0,
                    dims.y as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "cell_size_x",
                    "Cell X",
                    0.1,
                    10.0,
                    0.1,
                    cell_sz.x as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "cell_size_z",
                    "Cell Z",
                    0.1,
                    10.0,
                    0.1,
                    cell_sz.y as f64,
                    &plugin_ref,
                );
                self.add_option_attribute(
                    "blend_mode",
                    "Blend",
                    &["Smooth", "Hard", "Hard Blend"],
                    blend as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "wall_threshold",
                    "Wall Thresh",
                    0.0,
                    0.5,
                    0.01,
                    wall_th as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "ridge_threshold",
                    "Ridge Thresh",
                    0.0,
                    1.0,
                    0.01,
                    ridge_th as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "ledge_threshold",
                    "Ledge Thresh",
                    0.0,
                    1.0,
                    0.01,
                    ledge_th as f64,
                    &plugin_ref,
                );
                self.add_option_attribute(
                    "merge_mode",
                    "Merge",
                    &[
                        "Cubic",
                        "Polyhedron",
                        "RoundedPoly",
                        "SemiRound",
                        "Spherical",
                    ],
                    merge as i64,
                    &plugin_ref,
                );
                self.add_checkbox_attribute(
                    "use_ridge_texture",
                    "Ridge Tex",
                    use_ridge_tex,
                    &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "grass_subdivisions",
                    "Grass Subs",
                    1.0,
                    10.0,
                    1.0,
                    grass_sub as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "grass_size_x",
                    "Grass W",
                    0.1,
                    5.0,
                    0.1,
                    grass_sz.x as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "grass_size_y",
                    "Grass H",
                    0.1,
                    5.0,
                    0.1,
                    grass_sz.y as f64,
                    &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "animation_fps",
                    "Anim FPS",
                    0.0,
                    60.0,
                    1.0,
                    anim_fps as f64,
                    &plugin_ref,
                );
                self.add_spinbox_attribute(
                    "default_wall_texture",
                    "Wall Tex",
                    0.0,
                    15.0,
                    1.0,
                    def_wall as f64,
                    &plugin_ref,
                );
                // Extra collision layer: options 9-32 (stored as absolute layer number)
                let coll_options: Vec<&str> = (9..=32)
                    .map(|i| match i {
                        9 => "Layer 9",
                        10 => "Layer 10",
                        11 => "Layer 11",
                        12 => "Layer 12",
                        13 => "Layer 13",
                        14 => "Layer 14",
                        15 => "Layer 15",
                        16 => "Layer 16",
                        17 => "Layer 17",
                        18 => "Layer 18",
                        19 => "Layer 19",
                        20 => "Layer 20",
                        21 => "Layer 21",
                        22 => "Layer 22",
                        23 => "Layer 23",
                        24 => "Layer 24",
                        25 => "Layer 25",
                        26 => "Layer 26",
                        27 => "Layer 27",
                        28 => "Layer 28",
                        29 => "Layer 29",
                        30 => "Layer 30",
                        31 => "Layer 31",
                        32 => "Layer 32",
                        _ => "Layer 9",
                    })
                    .collect();
                self.add_option_attribute(
                    "extra_collision_layer",
                    "Coll Layer",
                    &coll_options,
                    (extra_coll - 9).max(0) as i64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "blend_sharpness",
                    "Blend Sharp",
                    0.0,
                    20.0,
                    0.1,
                    blend_sharp as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "blend_noise_scale",
                    "Noise Scale",
                    0.0,
                    50.0,
                    0.1,
                    blend_ns as f64,
                    &plugin_ref,
                );
                self.add_slider_attribute(
                    "blend_noise_strength",
                    "Noise Str",
                    0.0,
                    5.0,
                    0.01,
                    blend_nstr as f64,
                    &plugin_ref,
                );
            }
        }
    }

    /// Add an HSlider attribute control to the bottom attributes panel.
    #[allow(clippy::too_many_arguments)]
    fn add_slider_attribute(
        &mut self,
        name: &str,
        label_text: &str,
        min: f64,
        max: f64,
        step: f64,
        current: f64,
        plugin_ref: &Gd<PixyTerrainPlugin>,
    ) {
        let Some(ref mut hbox) = self.attributes_hbox else {
            return;
        };

        let mut center = CenterContainer::new_alloc();
        center.set_custom_minimum_size(Vector2::new(160.0, 36.0));

        let mut vbox = VBoxContainer::new_alloc();
        vbox.add_theme_constant_override("separation", 0);

        let mut label = Label::new_alloc();
        label.set_text(&format!("{label_text}: {current:.1}"));
        label.set_name(&format!("{name}_label"));

        let mut slider = HSlider::new_alloc();
        slider.set_min(min);
        slider.set_max(max);
        slider.set_step(step);
        slider.set_value(current);
        slider.set_custom_minimum_size(Vector2::new(140.0, 0.0));

        let callable = Callable::from_object_method(plugin_ref, "on_attribute_changed")
            .bindv(&varray![name.to_variant()]);
        slider.connect("value_changed", &callable);

        vbox.add_child(&label);
        vbox.add_child(&slider);
        center.add_child(&vbox);
        hbox.add_child(&center);
    }

    /// Add a CheckBox attribute control to the bottom attributes panel.
    fn add_checkbox_attribute(
        &mut self,
        name: &str,
        label_text: &str,
        current: bool,
        plugin_ref: &Gd<PixyTerrainPlugin>,
    ) {
        let Some(ref mut hbox) = self.attributes_hbox else {
            return;
        };

        let mut center = CenterContainer::new_alloc();
        center.set_custom_minimum_size(Vector2::new(100.0, 36.0));

        let mut checkbox = CheckBox::new_alloc();
        checkbox.set_text(label_text);
        checkbox.set_pressed(current);

        let callable = Callable::from_object_method(plugin_ref, "on_attribute_changed")
            .bindv(&varray![name.to_variant()]);
        checkbox.connect("toggled", &callable);

        center.add_child(&checkbox);
        hbox.add_child(&center);
    }

    /// Add an OptionButton attribute control to the bottom attributes panel.
    fn add_option_attribute(
        &mut self,
        name: &str,
        label_text: &str,
        options: &[&str],
        current_index: i64,
        plugin_ref: &Gd<PixyTerrainPlugin>,
    ) {
        let Some(ref mut hbox) = self.attributes_hbox else {
            return;
        };

        let mut center = CenterContainer::new_alloc();
        center.set_custom_minimum_size(Vector2::new(120.0, 36.0));

        let mut vbox = VBoxContainer::new_alloc();
        vbox.add_theme_constant_override("separation", 0);

        let mut label = Label::new_alloc();
        label.set_text(label_text);

        let mut option_btn = OptionButton::new_alloc();
        for opt in options {
            option_btn.add_item(*opt);
        }
        option_btn.select(current_index as i32);

        let callable = Callable::from_object_method(plugin_ref, "on_attribute_changed")
            .bindv(&varray![name.to_variant()]);
        option_btn.connect("item_selected", &callable);

        vbox.add_child(&label);
        vbox.add_child(&option_btn);
        center.add_child(&vbox);
        hbox.add_child(&center);
    }

    /// Add a SpinBox attribute control to the bottom attributes panel.
    #[allow(clippy::too_many_arguments)]
    fn add_spinbox_attribute(
        &mut self,
        name: &str,
        label_text: &str,
        min: f64,
        max: f64,
        step: f64,
        current: f64,
        plugin_ref: &Gd<PixyTerrainPlugin>,
    ) {
        let Some(ref mut hbox) = self.attributes_hbox else {
            return;
        };

        let mut center = CenterContainer::new_alloc();
        center.set_custom_minimum_size(Vector2::new(120.0, 36.0));

        let mut vbox = VBoxContainer::new_alloc();
        vbox.add_theme_constant_override("separation", 0);

        let mut label = Label::new_alloc();
        label.set_text(label_text);

        let mut spin = SpinBox::new_alloc();
        spin.set_min(min);
        spin.set_max(max);
        spin.set_step(step);
        spin.set_value(current);
        spin.set_custom_minimum_size(Vector2::new(80.0, 0.0));

        let callable = Callable::from_object_method(plugin_ref, "on_attribute_changed")
            .bindv(&varray![name.to_variant()]);
        spin.connect("value_changed", &callable);

        vbox.add_child(&label);
        vbox.add_child(&spin);
        center.add_child(&vbox);
        hbox.add_child(&center);
    }

    /// Add a QuickPaint dropdown to the attributes panel.
    fn add_quick_paint_dropdown(&mut self, plugin_ref: &Gd<PixyTerrainPlugin>) {
        // Build options: "None" + names of all presets
        let mut options: Vec<&str> = vec!["None"];
        let preset_names: Vec<String> = self
            .quick_paint_presets
            .iter()
            .map(|p| p.bind().paint_name.to_string())
            .collect();
        let preset_refs: Vec<&str> = preset_names.iter().map(|s| s.as_str()).collect();
        options.extend(preset_refs);

        let current_idx = if self.current_quick_paint.is_some() {
            // Find which preset is active
            if let Some(ref active) = self.current_quick_paint {
                self.quick_paint_presets
                    .iter()
                    .position(|p| p.instance_id() == active.instance_id())
                    .map(|i| (i + 1) as i64)
                    .unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };

        self.add_option_attribute(
            "quick_paint",
            "QuickPaint",
            &options,
            current_idx,
            plugin_ref,
        );
    }

    /// Apply a terrain setting change to the current terrain and update the shader.
    fn apply_terrain_setting(&mut self, name: &str, value: &Variant) {
        let Some(ref terrain_node) = self.current_terrain else {
            return;
        };
        if !terrain_node.is_instance_valid() {
            return;
        }
        let mut terrain: Gd<PixyTerrain> = terrain_node.clone().cast();

        {
            let mut t = terrain.bind_mut();
            match name {
                "dim_x" => {
                    let v = value.to::<f64>() as i32;
                    t.dimensions = Vector3i::new(v, t.dimensions.y, t.dimensions.z);
                }
                "dim_z" => {
                    let v = value.to::<f64>() as i32;
                    t.dimensions = Vector3i::new(t.dimensions.x, t.dimensions.y, v);
                }
                "dim_y" => {
                    let v = value.to::<f64>() as i32;
                    t.dimensions = Vector3i::new(t.dimensions.x, v, t.dimensions.z);
                }
                "cell_size_x" => {
                    let v = value.to::<f64>() as f32;
                    t.cell_size = Vector2::new(v, t.cell_size.y);
                }
                "cell_size_z" => {
                    let v = value.to::<f64>() as f32;
                    t.cell_size = Vector2::new(t.cell_size.x, v);
                }
                "blend_mode" => {
                    t.blend_mode = value.to::<i64>() as i32;
                }
                "wall_threshold" => {
                    t.wall_threshold = value.to::<f64>() as f32;
                }
                "ridge_threshold" => {
                    t.ridge_threshold = value.to::<f64>() as f32;
                }
                "ledge_threshold" => {
                    t.ledge_threshold = value.to::<f64>() as f32;
                }
                "merge_mode" => {
                    t.merge_mode = value.to::<i64>() as i32;
                }
                "grass_subdivisions" => {
                    t.grass_subdivisions = value.to::<f64>() as i32;
                }
                "grass_size_x" => {
                    let v = value.to::<f64>() as f32;
                    t.grass_size = Vector2::new(v, t.grass_size.y);
                }
                "grass_size_y" => {
                    let v = value.to::<f64>() as f32;
                    t.grass_size = Vector2::new(t.grass_size.x, v);
                }
                "default_wall_texture" => {
                    t.default_wall_texture = value.to::<f64>() as i32;
                }
                "blend_sharpness" => {
                    t.blend_sharpness = value.to::<f64>() as f32;
                }
                "blend_noise_scale" => {
                    t.blend_noise_scale = value.to::<f64>() as f32;
                }
                "blend_noise_strength" => {
                    t.blend_noise_strength = value.to::<f64>() as f32;
                }
                "animation_fps" => {
                    t.animation_fps = value.to::<f64>() as i32;
                }
                "use_ridge_texture" => {
                    t.use_ridge_texture = value.to();
                }
                "extra_collision_layer" => {
                    // Value is index 0-23, convert to layer 9-32
                    t.extra_collision_layer = value.to::<i64>() as i32 + 9;
                }
                _ if name.starts_with("tex_scale_") => {
                    let slot: usize = name["tex_scale_".len()..].parse().unwrap_or(1);
                    let v = value.to::<f64>() as f32;
                    match slot {
                        1 => t.texture_scale_1 = v,
                        2 => t.texture_scale_2 = v,
                        3 => t.texture_scale_3 = v,
                        4 => t.texture_scale_4 = v,
                        5 => t.texture_scale_5 = v,
                        6 => t.texture_scale_6 = v,
                        7 => t.texture_scale_7 = v,
                        8 => t.texture_scale_8 = v,
                        9 => t.texture_scale_9 = v,
                        10 => t.texture_scale_10 = v,
                        11 => t.texture_scale_11 = v,
                        12 => t.texture_scale_12 = v,
                        13 => t.texture_scale_13 = v,
                        14 => t.texture_scale_14 = v,
                        15 => t.texture_scale_15 = v,
                        _ => {}
                    }
                }
                _ if name.starts_with("tex_has_grass_") => {
                    let slot: usize = name["tex_has_grass_".len()..].parse().unwrap_or(2);
                    let v: bool = value.to();
                    match slot {
                        2 => t.tex2_has_grass = v,
                        3 => t.tex3_has_grass = v,
                        4 => t.tex4_has_grass = v,
                        5 => t.tex5_has_grass = v,
                        6 => t.tex6_has_grass = v,
                        _ => {}
                    }
                }
                _ if name.starts_with("ground_color_") => {
                    let slot: usize = name["ground_color_".len()..].parse().unwrap_or(1);
                    let v: Color = value.to();
                    match slot {
                        1 => t.ground_color = v,
                        2 => t.ground_color_2 = v,
                        3 => t.ground_color_3 = v,
                        4 => t.ground_color_4 = v,
                        5 => t.ground_color_5 = v,
                        6 => t.ground_color_6 = v,
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Sync shader uniforms after changing any terrain setting
        terrain.bind_mut().force_batch_update();
    }

    /// Rebuild the right-side texture settings panel content.
    fn rebuild_texture_panel(&mut self) {
        // Clear existing children (borrow scroll, then release)
        if let Some(ref mut scroll) = self.texture_panel {
            let count = scroll.get_child_count();
            for i in (0..count).rev() {
                if let Some(mut child) = scroll.get_child(i) {
                    scroll.remove_child(&child);
                    child.queue_free();
                }
            }
        } else {
            return;
        }

        // Only populate when a terrain is selected
        let terrain_node = match self.current_terrain {
            Some(ref t) if t.is_instance_valid() => t.clone(),
            _ => return,
        };

        let terrain: Gd<PixyTerrain> = terrain_node.cast();
        let t = terrain.bind();

        // Read current values
        let scales = [
            t.texture_scale_1,
            t.texture_scale_2,
            t.texture_scale_3,
            t.texture_scale_4,
            t.texture_scale_5,
            t.texture_scale_6,
            t.texture_scale_7,
            t.texture_scale_8,
            t.texture_scale_9,
            t.texture_scale_10,
            t.texture_scale_11,
            t.texture_scale_12,
            t.texture_scale_13,
            t.texture_scale_14,
            t.texture_scale_15,
        ];
        let has_grass = [
            true, // tex1 always has grass
            t.tex2_has_grass,
            t.tex3_has_grass,
            t.tex4_has_grass,
            t.tex5_has_grass,
            t.tex6_has_grass,
        ];
        let ground_colors = [
            t.ground_color,
            t.ground_color_2,
            t.ground_color_3,
            t.ground_color_4,
            t.ground_color_5,
            t.ground_color_6,
        ];
        // Collect ground textures (15 slots)
        let ground_textures: [Option<Gd<godot::classes::Texture2D>>; 15] = [
            t.ground_texture.clone(),
            t.texture_2.clone(),
            t.texture_3.clone(),
            t.texture_4.clone(),
            t.texture_5.clone(),
            t.texture_6.clone(),
            t.texture_7.clone(),
            t.texture_8.clone(),
            t.texture_9.clone(),
            t.texture_10.clone(),
            t.texture_11.clone(),
            t.texture_12.clone(),
            t.texture_13.clone(),
            t.texture_14.clone(),
            t.texture_15.clone(),
        ];
        // Collect grass sprites (6 slots)
        let grass_sprites: [Option<Gd<godot::classes::Texture2D>>; 6] = [
            t.grass_sprite.clone(),
            t.grass_sprite_tex_2.clone(),
            t.grass_sprite_tex_3.clone(),
            t.grass_sprite_tex_4.clone(),
            t.grass_sprite_tex_5.clone(),
            t.grass_sprite_tex_6.clone(),
        ];
        drop(t);

        let plugin_ref = self.to_gd();

        let mut vbox = VBoxContainer::new_alloc();
        vbox.set_name("TextureSettingsVBox");
        vbox.add_theme_constant_override("separation", 6);

        let mut header = Label::new_alloc();
        header.set_text("Texture Settings");
        vbox.add_child(&header);

        // 15 texture slots
        for slot in 1..=15i32 {
            let sep = HSeparator::new_alloc();
            vbox.add_child(&sep);

            let mut slot_label = Label::new_alloc();
            slot_label.set_text(&format!("Texture {slot}"));
            vbox.add_child(&slot_label);

            // Ground texture picker
            let tex_name = format!("ground_tex_{slot}");
            let mut tex_label = Label::new_alloc();
            tex_label.set_text("Ground Texture");

            let mut tex_picker = EditorResourcePicker::new_alloc();
            tex_picker.set_base_type("Texture2D");
            if let Some(ref tex) = ground_textures[(slot - 1) as usize] {
                tex_picker.set_edited_resource(tex);
            }
            tex_picker.set_custom_minimum_size(Vector2::new(180.0, 28.0));

            let callable = Callable::from_object_method(&plugin_ref, "on_texture_resource_changed")
                .bindv(&varray![tex_name.to_variant()]);
            tex_picker.connect("resource_changed", &callable);

            vbox.add_child(&tex_label);
            vbox.add_child(&tex_picker);

            // UV scale slider
            let scale_name = format!("tex_scale_{slot}");
            let mut scale_label = Label::new_alloc();
            scale_label.set_text(&format!("Scale: {:.1}", scales[(slot - 1) as usize]));
            scale_label.set_name(&format!("{scale_name}_label"));

            let mut scale_slider = HSlider::new_alloc();
            scale_slider.set_min(0.1);
            scale_slider.set_max(40.0);
            scale_slider.set_step(0.1);
            scale_slider.set_value(scales[(slot - 1) as usize] as f64);
            scale_slider.set_custom_minimum_size(Vector2::new(180.0, 0.0));

            let callable = Callable::from_object_method(&plugin_ref, "on_attribute_changed")
                .bindv(&varray![scale_name.to_variant()]);
            scale_slider.connect("value_changed", &callable);

            vbox.add_child(&scale_label);
            vbox.add_child(&scale_slider);

            // Ground color picker (slots 1-6 only)
            if slot <= 6 {
                let color_name = format!("ground_color_{slot}");
                let mut color_label = Label::new_alloc();
                color_label.set_text("Ground Color");

                let mut color_picker = ColorPickerButton::new_alloc();
                color_picker.set_pick_color(ground_colors[(slot - 1) as usize]);
                color_picker.set_custom_minimum_size(Vector2::new(180.0, 28.0));

                let callable = Callable::from_object_method(&plugin_ref, "on_attribute_changed")
                    .bindv(&varray![color_name.to_variant()]);
                color_picker.connect("color_changed", &callable);

                vbox.add_child(&color_label);
                vbox.add_child(&color_picker);

                // Grass sprite picker
                let sprite_name = format!("grass_sprite_{slot}");
                let mut sprite_label = Label::new_alloc();
                sprite_label.set_text("Grass Sprite");

                let mut sprite_picker = EditorResourcePicker::new_alloc();
                sprite_picker.set_base_type("Texture2D");
                if let Some(ref tex) = grass_sprites[(slot - 1) as usize] {
                    sprite_picker.set_edited_resource(tex);
                }
                sprite_picker.set_custom_minimum_size(Vector2::new(180.0, 28.0));

                let callable =
                    Callable::from_object_method(&plugin_ref, "on_texture_resource_changed")
                        .bindv(&varray![sprite_name.to_variant()]);
                sprite_picker.connect("resource_changed", &callable);

                vbox.add_child(&sprite_label);
                vbox.add_child(&sprite_picker);
            }

            // Has grass checkbox (slots 2-6 only, slot 1 always has grass)
            if (2..=6).contains(&slot) {
                let grass_name = format!("tex_has_grass_{slot}");
                let mut grass_cb = CheckBox::new_alloc();
                grass_cb.set_text("Has Grass");
                grass_cb.set_pressed(has_grass[(slot - 1) as usize]);

                let callable = Callable::from_object_method(&plugin_ref, "on_attribute_changed")
                    .bindv(&varray![grass_name.to_variant()]);
                grass_cb.connect("toggled", &callable);

                vbox.add_child(&grass_cb);
            }
        }

        // Re-borrow scroll to add the completed vbox
        if let Some(ref mut scroll) = self.texture_panel {
            scroll.add_child(&vbox);
        }
    }

    /// Return a snapshot of the current brush/drawing state for gizmo rendering.
    pub fn get_gizmo_state(&self) -> GizmoState {
        GizmoState {
            mode: self.mode,
            brush_type: self.brush_type,
            brush_position: self.brush_position,
            brush_size: self.brush_size,
            terrain_hovered: self.terrain_hovered,
            flatten: self.flatten,
            draw_height: self.draw_height,
            draw_pattern: self.current_draw_pattern.clone(),
        }
    }

    /// Trigger a gizmo redraw on the terrain node.
    fn update_gizmos(&self) {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                let mut terrain_3d: Gd<godot::classes::Node3D> = terrain.clone().cast();
                terrain_3d.update_gizmos();
            }
        }
    }

    fn call_terrain_method(&mut self, method_name: &str) {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                let mut terrain_clone = terrain.clone();
                if terrain_clone.has_method(method_name) {
                    self.is_modifying = true;
                    terrain_clone.call(method_name, &[]);
                    self.is_modifying = false;
                }
            }
        }
    }

    fn do_generate(&mut self) {
        self.call_terrain_method("regenerate");
    }

    fn do_clear(&mut self) {
        self.call_terrain_method("clear");
    }

    /// Set vertex colors from a texture index (0-15).
    fn set_vertex_colors(&mut self, idx: i32) {
        let (c0, c1) = marching_squares::texture_index_to_colors(idx);
        self.vertex_color_0 = c0;
        self.vertex_color_1 = c1;
        self.vertex_color_idx = idx;
    }

    /// Initialize draw_height_set state (logic moved from Yugen's gizmo _redraw).
    fn initialize_draw_state(
        &mut self,
        terrain: &Gd<PixyTerrain>,
        dim: Vector3i,
        cell_size: Vector2,
    ) {
        if self.is_setting && !self.draw_height_set {
            self.draw_height_set = true;

            // Check if clicked cell is in the current pattern
            let pos = self.brush_position;
            let chunk_width = (dim.x - 1) as f32 * cell_size.x;
            let chunk_depth = (dim.z - 1) as f32 * cell_size.y;
            let cursor_chunk_x = (pos.x / chunk_width).floor() as i32;
            let cursor_chunk_z = (pos.z / chunk_depth).floor() as i32;

            let cursor_cell_x = ((pos.x + cell_size.x / 2.0) / cell_size.x
                - cursor_chunk_x as f32 * (dim.x - 1) as f32)
                .floor() as i32;
            let cursor_cell_z = ((pos.z + cell_size.y / 2.0) / cell_size.y
                - cursor_chunk_z as f32 * (dim.z - 1) as f32)
                .floor() as i32;

            let in_pattern = self
                .current_draw_pattern
                .get(&[cursor_chunk_x, cursor_chunk_z])
                .and_then(|cells| cells.get(&[cursor_cell_x, cursor_cell_z]))
                .is_some();

            let alt_held = Input::singleton().is_key_pressed(godot::global::Key::ALT);

            if !in_pattern && !alt_held {
                // Not on existing pattern → clear and switch to drawing
                self.current_draw_pattern.clear();
                self.is_setting = false;
                self.is_drawing = true;
                self.draw_height = pos.y;
            } else {
                // On existing pattern → drag mode
                if alt_held {
                    // ALT: only drag the clicked cell
                    let chunk_key = [cursor_chunk_x, cursor_chunk_z];
                    let cell_key = [cursor_cell_x, cursor_cell_z];
                    self.current_draw_pattern.clear();

                    if let Some(chunk) = terrain.bind().get_chunk(cursor_chunk_x, cursor_chunk_z) {
                        let h = chunk
                            .bind()
                            .get_height(Vector2i::new(cursor_cell_x, cursor_cell_z));
                        let mut cells = HashMap::new();
                        cells.insert(cell_key, h as f64 as f32);
                        self.current_draw_pattern.insert(chunk_key, cells);
                    }
                    self.draw_height = pos.y;
                }
                self.base_position = pos;
            }
        }

        if self.is_drawing && !self.draw_height_set {
            self.draw_height_set = true;
            self.draw_height = self.brush_position.y;
        }
    }

    /// Build the draw pattern based on current brush position and size.
    fn build_draw_pattern(&mut self, terrain: &Gd<PixyTerrain>, dim: Vector3i, cell_size: Vector2) {
        let pos = self.brush_position;

        // Compute brush bounding box (with cell_size offset matching Yugen)
        let pos_tl = Vector2::new(
            pos.x + cell_size.x - self.brush_size / 2.0,
            pos.z + cell_size.y - self.brush_size / 2.0,
        );
        let pos_br = Vector2::new(
            pos.x + cell_size.x + self.brush_size / 2.0,
            pos.z + cell_size.y + self.brush_size / 2.0,
        );

        let chunk_width = (dim.x - 1) as f32 * cell_size.x;
        let chunk_depth = (dim.z - 1) as f32 * cell_size.y;

        let chunk_tl_x = (pos_tl.x / chunk_width).floor() as i32;
        let chunk_tl_z = (pos_tl.y / chunk_depth).floor() as i32;
        let chunk_br_x = (pos_br.x / chunk_width).floor() as i32;
        let chunk_br_z = (pos_br.y / chunk_depth).floor() as i32;

        let x_tl = (pos_tl.x / cell_size.x - chunk_tl_x as f32 * (dim.x - 1) as f32).floor() as i32;
        let z_tl = (pos_tl.y / cell_size.y - chunk_tl_z as f32 * (dim.z - 1) as f32).floor() as i32;
        let x_br = (pos_br.x / cell_size.x - chunk_br_x as f32 * (dim.x - 1) as f32).floor() as i32;
        let z_br = (pos_br.y / cell_size.y - chunk_br_z as f32 * (dim.z - 1) as f32).floor() as i32;

        // Max distance for brush type
        let half = self.brush_size / 2.0;
        let max_distance = match self.brush_type {
            BrushType::Round => half * half,
            BrushType::Square => half * half * 2.0,
        };

        for chunk_z in chunk_tl_z..=chunk_br_z {
            for chunk_x in chunk_tl_x..=chunk_br_x {
                if !terrain.bind().has_chunk(chunk_x, chunk_z) {
                    continue;
                }

                let x_min = if chunk_x == chunk_tl_x { x_tl } else { 0 };
                let x_max = if chunk_x == chunk_br_x { x_br } else { dim.x };
                let z_min = if chunk_z == chunk_tl_z { z_tl } else { 0 };
                let z_max = if chunk_z == chunk_br_z { z_br } else { dim.z };

                for z in z_min..z_max {
                    for x in x_min..x_max {
                        let world_x = (chunk_x * (dim.x - 1) + x) as f32 * cell_size.x;
                        let world_z = (chunk_z * (dim.z - 1) + z) as f32 * cell_size.y;

                        let dist_sq = (pos.x - world_x) * (pos.x - world_x)
                            + (pos.z - world_z) * (pos.z - world_z);

                        if dist_sq > max_distance {
                            continue;
                        }

                        let sample = if self.falloff {
                            let t = match self.brush_type {
                                BrushType::Round => {
                                    ((max_distance - dist_sq) / max_distance).clamp(0.0, 1.0)
                                }
                                BrushType::Square => {
                                    let local_x = world_x - pos.x;
                                    let local_z = world_z - pos.z;
                                    let uv_x = local_x / (self.brush_size * 0.5);
                                    let uv_z = local_z / (self.brush_size * 0.5);
                                    let d = uv_x.abs().max(uv_z.abs());
                                    1.0 - d.clamp(0.2, 1.0)
                                }
                            };
                            let t = t.clamp(0.001, 0.999);
                            // Smoothstep falloff (approximation of Yugen's Curve resource)
                            t * t * (3.0 - 2.0 * t)
                        } else {
                            1.0
                        };

                        // Accumulate: keep max sample per cell
                        let chunk_key = [chunk_x, chunk_z];
                        let cell_key = [x, z];
                        let cell_entry = self
                            .current_draw_pattern
                            .entry(chunk_key)
                            .or_default()
                            .entry(cell_key)
                            .or_insert(0.0);
                        if sample > *cell_entry {
                            *cell_entry = sample;
                        }
                    }
                }
            }
        }
    }

    /// Apply the current draw pattern to the terrain.
    #[allow(clippy::type_complexity)]
    fn draw_pattern(&mut self, terrain: &Gd<PixyTerrain>, dim: Vector3i, cell_size: Vector2) {
        if self.current_draw_pattern.is_empty() {
            return;
        }

        // Snapshot the pattern (avoid borrow issues)
        let pattern_snapshot: Vec<([i32; 2], Vec<([i32; 2], f32)>)> = self
            .current_draw_pattern
            .iter()
            .map(|(k, v)| (*k, v.iter().map(|(ck, cv)| (*ck, *cv)).collect()))
            .collect();

        // Phase 1: Compute do/undo values per cell
        let mut do_height = VarDictionary::new();
        let mut undo_height = VarDictionary::new();
        let mut do_color_0 = VarDictionary::new();
        let mut undo_color_0 = VarDictionary::new();
        let mut do_color_1 = VarDictionary::new();
        let mut undo_color_1 = VarDictionary::new();
        let mut do_wall_color_0 = VarDictionary::new();
        let mut undo_wall_color_0 = VarDictionary::new();
        let mut do_wall_color_1 = VarDictionary::new();
        let mut undo_wall_color_1 = VarDictionary::new();
        let mut do_grass_mask = VarDictionary::new();
        let mut undo_grass_mask = VarDictionary::new();

        let mut first_chunk: Option<[i32; 2]> = None;

        for (chunk_key, cells) in &pattern_snapshot {
            if first_chunk.is_none() {
                first_chunk = Some(*chunk_key);
            }

            let chunk_coords = Vector2i::new(chunk_key[0], chunk_key[1]);
            let chunk = terrain.bind().get_chunk(chunk_key[0], chunk_key[1]);
            let Some(chunk) = chunk else { continue };

            match self.mode {
                TerrainToolMode::Smooth => {
                    // Compute average height across cells in this chunk
                    let avg_height = {
                        let c = chunk.bind();
                        let sum: f32 = cells
                            .iter()
                            .map(|(ck, _)| c.get_height(Vector2i::new(ck[0], ck[1])))
                            .sum();
                        sum / cells.len().max(1) as f32
                    };

                    let mut do_chunk = VarDictionary::new();
                    let mut undo_chunk = VarDictionary::new();

                    for &(cell_key, sample) in cells {
                        let sample = sample.clamp(0.001, 0.999);
                        let cell_coords = Vector2i::new(cell_key[0], cell_key[1]);
                        let old_h = chunk.bind().get_height(cell_coords);
                        let f = sample * self.strength;
                        let new_h = lerp_f32(old_h, avg_height, f);
                        do_chunk.set(cell_coords, new_h);
                        undo_chunk.set(cell_coords, old_h);
                    }

                    do_height.set(chunk_coords, do_chunk);
                    undo_height.set(chunk_coords, undo_chunk);
                }

                TerrainToolMode::DebugBrush => {
                    for &(cell_key, _) in cells {
                        let c = chunk.bind();
                        let cell_coords = Vector2i::new(cell_key[0], cell_key[1]);
                        let h = c.get_height(cell_coords);
                        let col0 = c.get_color_0(cell_key[0], cell_key[1]);
                        let col1 = c.get_color_1(cell_key[0], cell_key[1]);
                        godot_print!(
                            "DEBUG: chunk ({},{}), cell ({},{}), h={:.3}, c0={:?}, c1={:?}",
                            chunk_key[0],
                            chunk_key[1],
                            cell_key[0],
                            cell_key[1],
                            h,
                            col0,
                            col1
                        );
                    }
                    return; // Debug mode doesn't apply changes
                }

                _ => {
                    let mut do_chunk = VarDictionary::new();
                    let mut undo_chunk = VarDictionary::new();
                    let mut do_chunk_cc = VarDictionary::new();
                    let mut undo_chunk_cc = VarDictionary::new();

                    for &(cell_key, sample) in cells {
                        let sample = sample.clamp(0.001, 0.999);
                        let cell_coords = Vector2i::new(cell_key[0], cell_key[1]);

                        match self.mode {
                            TerrainToolMode::GrassMask => {
                                let old = chunk.bind().get_grass_mask_at(cell_key[0], cell_key[1]);
                                let new_mask = if self.should_mask_grass {
                                    Color::from_rgba(0.0, 0.0, 0.0, 0.0)
                                } else {
                                    Color::from_rgba(1.0, 0.0, 0.0, 0.0)
                                };
                                do_chunk.set(cell_coords, new_mask);
                                undo_chunk.set(cell_coords, old);
                            }

                            TerrainToolMode::Level => {
                                let old_h = chunk.bind().get_height(cell_coords);
                                let new_h = lerp_f32(old_h, self.height, sample);
                                do_chunk.set(cell_coords, new_h);
                                undo_chunk.set(cell_coords, old_h);
                            }

                            TerrainToolMode::Bridge => {
                                let b_end =
                                    Vector2::new(self.brush_position.x, self.brush_position.z);
                                let b_start =
                                    Vector2::new(self.bridge_start_pos.x, self.bridge_start_pos.z);
                                let bridge_length = b_end.distance_to(b_start);

                                if bridge_length < 0.5 || cells.len() < 3 {
                                    continue;
                                }

                                // Cell to world-space (using correct stride)
                                let mut global_x =
                                    (chunk_key[0] * (dim.x - 1) + cell_key[0]) as f32 * cell_size.x;
                                let global_z =
                                    (chunk_key[1] * (dim.z - 1) + cell_key[1]) as f32 * cell_size.y;

                                // Cross-chunk offset correction for bridge interpolation
                                // When drawing to a different chunk, adjust the X coordinate
                                // to account for chunk boundary alignment
                                if chunk_key[0] != self.bridge_start_chunk.x {
                                    global_x += (self.bridge_start_chunk.x - chunk_key[0]) as f32
                                        * 2.0
                                        * cell_size.x;
                                }

                                let global_cell = Vector2::new(global_x, global_z);

                                let bridge_dir = (b_end - b_start) / bridge_length;
                                let cell_vec = global_cell - b_start;
                                let linear_offset = cell_vec.dot(bridge_dir);
                                let mut progress = (linear_offset / bridge_length).clamp(0.0, 1.0);

                                if self.ease_value != -1.0 {
                                    progress = godot_ease(progress, self.ease_value);
                                }

                                let bridge_height = lerp_f32(
                                    self.bridge_start_pos.y,
                                    self.brush_position.y,
                                    progress,
                                );

                                let old_h = chunk.bind().get_height(cell_coords);
                                do_chunk.set(cell_coords, bridge_height);
                                undo_chunk.set(cell_coords, old_h);
                            }

                            TerrainToolMode::VertexPaint => {
                                if self.paint_walls_mode {
                                    let old_c0 =
                                        chunk.bind().get_wall_color_0(cell_key[0], cell_key[1]);
                                    let old_c1 =
                                        chunk.bind().get_wall_color_1(cell_key[0], cell_key[1]);
                                    do_chunk.set(cell_coords, self.vertex_color_0);
                                    undo_chunk.set(cell_coords, old_c0);
                                    do_chunk_cc.set(cell_coords, self.vertex_color_1);
                                    undo_chunk_cc.set(cell_coords, old_c1);
                                } else {
                                    let old_c0 = chunk.bind().get_color_0(cell_key[0], cell_key[1]);
                                    let old_c1 = chunk.bind().get_color_1(cell_key[0], cell_key[1]);
                                    do_chunk.set(cell_coords, self.vertex_color_0);
                                    undo_chunk.set(cell_coords, old_c0);
                                    do_chunk_cc.set(cell_coords, self.vertex_color_1);
                                    undo_chunk_cc.set(cell_coords, old_c1);
                                }
                            }

                            // Brush (default height tool)
                            _ => {
                                let old_h = chunk.bind().get_height(cell_coords);
                                let new_h = if self.flatten {
                                    lerp_f32(old_h, self.brush_position.y, sample)
                                } else {
                                    let height_diff = self.brush_position.y - self.draw_height;
                                    old_h + height_diff * sample
                                };
                                do_chunk.set(cell_coords, new_h);
                                undo_chunk.set(cell_coords, old_h);
                            }
                        }
                    }

                    // Store in appropriate dictionaries
                    match self.mode {
                        TerrainToolMode::GrassMask => {
                            do_grass_mask.set(chunk_coords, do_chunk);
                            undo_grass_mask.set(chunk_coords, undo_chunk);
                        }
                        TerrainToolMode::VertexPaint => {
                            let (key0, key1) = if self.paint_walls_mode {
                                ("wall_color_0", "wall_color_1")
                            } else {
                                ("color_0", "color_1")
                            };
                            // Store in wall/color dicts based on mode
                            if self.paint_walls_mode {
                                do_wall_color_0.set(chunk_coords, do_chunk);
                                undo_wall_color_0.set(chunk_coords, undo_chunk);
                                do_wall_color_1.set(chunk_coords, do_chunk_cc);
                                undo_wall_color_1.set(chunk_coords, undo_chunk_cc);
                            } else {
                                do_color_0.set(chunk_coords, do_chunk);
                                undo_color_0.set(chunk_coords, undo_chunk);
                                do_color_1.set(chunk_coords, do_chunk_cc);
                                undo_color_1.set(chunk_coords, undo_chunk_cc);
                            }
                            let _ = (key0, key1); // used for action name
                        }
                        _ => {
                            // Height modes: Brush, Level, Bridge
                            do_height.set(chunk_coords, do_chunk);
                            undo_height.set(chunk_coords, undo_chunk);
                        }
                    }
                }
            }
        }

        // Phase 1.5: QuickPaint — apply wall, ground, and grass patterns alongside height
        if let Some(ref qp) = self.current_quick_paint {
            let qp_bind = qp.bind();
            let wall_slot = qp_bind.wall_texture_slot;
            let ground_slot = qp_bind.ground_texture_slot;
            let has_grass = qp_bind.has_grass;
            drop(qp_bind);

            let (wall_c0, wall_c1) = marching_squares::texture_index_to_colors(wall_slot);
            let (ground_c0, ground_c1) = marching_squares::texture_index_to_colors(ground_slot);

            for (chunk_key, cells) in &pattern_snapshot {
                let chunk_coords = Vector2i::new(chunk_key[0], chunk_key[1]);
                let chunk = terrain.bind().get_chunk(chunk_key[0], chunk_key[1]);
                let Some(chunk) = chunk else { continue };

                // Wall colors
                let mut do_wc0_chunk = VarDictionary::new();
                let mut undo_wc0_chunk = VarDictionary::new();
                let mut do_wc1_chunk = VarDictionary::new();
                let mut undo_wc1_chunk = VarDictionary::new();
                // Ground colors
                let mut do_gc0_chunk = VarDictionary::new();
                let mut undo_gc0_chunk = VarDictionary::new();
                let mut do_gc1_chunk = VarDictionary::new();
                let mut undo_gc1_chunk = VarDictionary::new();
                // Grass mask
                let mut do_gm_chunk = VarDictionary::new();
                let mut undo_gm_chunk = VarDictionary::new();

                for &(cell_key, _) in cells {
                    let cell = Vector2i::new(cell_key[0], cell_key[1]);
                    let c = chunk.bind();

                    // Wall colors
                    undo_wc0_chunk.set(cell, c.get_wall_color_0(cell_key[0], cell_key[1]));
                    undo_wc1_chunk.set(cell, c.get_wall_color_1(cell_key[0], cell_key[1]));
                    do_wc0_chunk.set(cell, wall_c0);
                    do_wc1_chunk.set(cell, wall_c1);

                    // Ground colors
                    undo_gc0_chunk.set(cell, c.get_color_0(cell_key[0], cell_key[1]));
                    undo_gc1_chunk.set(cell, c.get_color_1(cell_key[0], cell_key[1]));
                    do_gc0_chunk.set(cell, ground_c0);
                    do_gc1_chunk.set(cell, ground_c1);

                    // Grass mask
                    undo_gm_chunk.set(cell, c.get_grass_mask_at(cell_key[0], cell_key[1]));
                    if has_grass {
                        do_gm_chunk.set(cell, Color::from_rgba(1.0, 1.0, 0.0, 0.0));
                    } else {
                        do_gm_chunk.set(cell, Color::from_rgba(0.0, 0.0, 0.0, 0.0));
                    }
                }

                // Merge into existing dicts (QuickPaint wall colors)
                do_wall_color_0.set(chunk_coords, do_wc0_chunk);
                undo_wall_color_0.set(chunk_coords, undo_wc0_chunk);
                do_wall_color_1.set(chunk_coords, do_wc1_chunk);
                undo_wall_color_1.set(chunk_coords, undo_wc1_chunk);

                // Ground colors
                do_color_0.set(chunk_coords, do_gc0_chunk);
                undo_color_0.set(chunk_coords, undo_gc0_chunk);
                do_color_1.set(chunk_coords, do_gc1_chunk);
                undo_color_1.set(chunk_coords, undo_gc1_chunk);

                // Grass mask
                do_grass_mask.set(chunk_coords, do_gm_chunk);
                undo_grass_mask.set(chunk_coords, undo_gm_chunk);
            }

            // Expand wall colors to adjacent ±1 cells for QuickPaint
            let cells_to_expand: Vec<([i32; 2], [i32; 2])> = pattern_snapshot
                .iter()
                .flat_map(|(ck, cells)| cells.iter().map(move |(cell, _)| (*ck, *cell)))
                .collect();

            for (chunk_key, cell_key) in &cells_to_expand {
                let chunk_coords = Vector2i::new(chunk_key[0], chunk_key[1]);
                for dx in -1i32..=1 {
                    for dz in -1i32..=1 {
                        if dx == 0 && dz == 0 {
                            continue;
                        }

                        let mut adj_x = cell_key[0] + dx;
                        let mut adj_z = cell_key[1] + dz;
                        let mut adj_chunk = chunk_coords;

                        if adj_x < 0 {
                            adj_chunk.x -= 1;
                            adj_x = dim.x - 1;
                        } else if adj_x >= dim.x {
                            adj_chunk.x += 1;
                            adj_x = 0;
                        }
                        if adj_z < 0 {
                            adj_chunk.y -= 1;
                            adj_z = dim.z - 1;
                        } else if adj_z >= dim.z {
                            adj_chunk.y += 1;
                            adj_z = 0;
                        }

                        if !terrain.bind().has_chunk(adj_chunk.x, adj_chunk.y) {
                            continue;
                        }

                        let adj_cell = Vector2i::new(adj_x, adj_z);

                        // Skip if already in wall pattern
                        if let Some(existing) = do_wall_color_0.get(adj_chunk) {
                            let d: VarDictionary = existing.to();
                            if d.contains_key(adj_cell) {
                                continue;
                            }
                        }

                        let adj_chunk_gd = terrain.bind().get_chunk(adj_chunk.x, adj_chunk.y);
                        let Some(adj_chunk_gd) = adj_chunk_gd else {
                            continue;
                        };

                        let old_wc0 = adj_chunk_gd.bind().get_wall_color_0(adj_x, adj_z);
                        let old_wc1 = adj_chunk_gd.bind().get_wall_color_1(adj_x, adj_z);

                        let mut do_chunk_0: VarDictionary =
                            do_wall_color_0.get_or_nil(adj_chunk).to();
                        do_chunk_0.set(adj_cell, wall_c0);
                        do_wall_color_0.set(adj_chunk, do_chunk_0);

                        let mut undo_chunk_0: VarDictionary =
                            undo_wall_color_0.get_or_nil(adj_chunk).to();
                        undo_chunk_0.set(adj_cell, old_wc0);
                        undo_wall_color_0.set(adj_chunk, undo_chunk_0);

                        let mut do_chunk_1: VarDictionary =
                            do_wall_color_1.get_or_nil(adj_chunk).to();
                        do_chunk_1.set(adj_cell, wall_c1);
                        do_wall_color_1.set(adj_chunk, do_chunk_1);

                        let mut undo_chunk_1: VarDictionary =
                            undo_wall_color_1.get_or_nil(adj_chunk).to();
                        undo_chunk_1.set(adj_cell, old_wc1);
                        undo_wall_color_1.set(adj_chunk, undo_chunk_1);
                    }
                }
            }
        }

        // Phase 2: Cross-chunk edge propagation
        self.propagate_cross_chunk_edges(
            terrain,
            &pattern_snapshot,
            dim,
            &mut do_height,
            &mut undo_height,
            &mut do_color_0,
            &mut undo_color_0,
            &mut do_color_1,
            &mut undo_color_1,
            &mut do_wall_color_0,
            &mut undo_wall_color_0,
            &mut do_wall_color_1,
            &mut undo_wall_color_1,
            &mut do_grass_mask,
            &mut undo_grass_mask,
        );

        // Phase 3: Wall color expansion for height modes (Brush, Level, Smooth, Bridge)
        // Skip when QuickPaint is active — it handles its own wall colors
        if self.current_quick_paint.is_none()
            && matches!(
                self.mode,
                TerrainToolMode::Brush
                    | TerrainToolMode::Level
                    | TerrainToolMode::Smooth
                    | TerrainToolMode::Bridge
            )
        {
            self.expand_wall_colors(
                terrain,
                dim,
                &do_height,
                &mut do_wall_color_0,
                &mut undo_wall_color_0,
                &mut do_wall_color_1,
                &mut undo_wall_color_1,
            );
        }

        // Phase 4: Build composite dictionaries and register undo/redo
        let mut do_patterns = VarDictionary::new();
        let mut undo_patterns = VarDictionary::new();

        if !do_height.is_empty() {
            do_patterns.set("height", do_height);
            undo_patterns.set("height", undo_height);
        }
        if !do_wall_color_0.is_empty() {
            do_patterns.set("wall_color_0", do_wall_color_0);
            undo_patterns.set("wall_color_0", undo_wall_color_0);
        }
        if !do_wall_color_1.is_empty() {
            do_patterns.set("wall_color_1", do_wall_color_1);
            undo_patterns.set("wall_color_1", undo_wall_color_1);
        }
        if !do_grass_mask.is_empty() {
            do_patterns.set("grass_mask", do_grass_mask);
            undo_patterns.set("grass_mask", undo_grass_mask);
        }
        if !do_color_0.is_empty() {
            do_patterns.set("color_0", do_color_0);
            undo_patterns.set("color_0", undo_color_0);
        }
        if !do_color_1.is_empty() {
            do_patterns.set("color_1", do_color_1);
            undo_patterns.set("color_1", undo_color_1);
        }

        if do_patterns.is_empty() {
            return;
        }

        let action_name = match self.mode {
            TerrainToolMode::Brush => "terrain brush",
            TerrainToolMode::Level => "terrain level",
            TerrainToolMode::Smooth => "terrain smooth",
            TerrainToolMode::Bridge => "terrain bridge",
            TerrainToolMode::GrassMask => "terrain grass mask",
            TerrainToolMode::VertexPaint => {
                if self.paint_walls_mode {
                    "terrain wall paint"
                } else {
                    "terrain vertex paint"
                }
            }
            _ => "terrain draw",
        };

        let terrain_node = self.current_terrain.as_ref().unwrap().clone();
        self.register_undo_redo(action_name, &terrain_node, do_patterns, undo_patterns);
    }

    /// Propagate draw values to adjacent chunk edges for seamless borders.
    /// Uses mode-specific dictionary routing to avoid mutable aliasing issues.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn propagate_cross_chunk_edges(
        &self,
        terrain: &Gd<PixyTerrain>,
        pattern_snapshot: &[([i32; 2], Vec<([i32; 2], f32)>)],
        dim: Vector3i,
        do_height: &mut VarDictionary,
        undo_height: &mut VarDictionary,
        do_color_0: &mut VarDictionary,
        undo_color_0: &mut VarDictionary,
        do_color_1: &mut VarDictionary,
        undo_color_1: &mut VarDictionary,
        do_wall_color_0: &mut VarDictionary,
        undo_wall_color_0: &mut VarDictionary,
        do_wall_color_1: &mut VarDictionary,
        undo_wall_color_1: &mut VarDictionary,
        do_grass_mask: &mut VarDictionary,
        undo_grass_mask: &mut VarDictionary,
    ) {
        // Collect edges to propagate, then apply (avoids holding borrows across iterations)
        struct EdgeEntry {
            src_chunk: Vector2i,
            src_cell: Vector2i,
            adj_chunk: Vector2i,
            adj_cell: Vector2i,
        }

        let mut edges: Vec<EdgeEntry> = Vec::new();

        for (chunk_key, cells) in pattern_snapshot {
            for &(cell_key, sample) in cells {
                let sample = sample.clamp(0.001, 0.999);

                for cx in -1i32..=1 {
                    for cz in -1i32..=1 {
                        if cx == 0 && cz == 0 {
                            continue;
                        }

                        let adj_chunk = [chunk_key[0] + cx, chunk_key[1] + cz];
                        if !terrain.bind().has_chunk(adj_chunk[0], adj_chunk[1]) {
                            continue;
                        }

                        let mut x = cell_key[0];
                        let mut z = cell_key[1];

                        if cx == -1 {
                            if x == 0 {
                                x = dim.x - 1;
                            } else {
                                continue;
                            }
                        } else if cx == 1 {
                            if x == dim.x - 1 {
                                x = 0;
                            } else {
                                continue;
                            }
                        }
                        if cz == -1 {
                            if z == 0 {
                                z = dim.z - 1;
                            } else {
                                continue;
                            }
                        } else if cz == 1 {
                            if z == dim.z - 1 {
                                z = 0;
                            } else {
                                continue;
                            }
                        }

                        let existing_higher = self
                            .current_draw_pattern
                            .get(&adj_chunk)
                            .and_then(|cells| cells.get(&[x, z]))
                            .is_some_and(|&s| s > sample);

                        if existing_higher {
                            continue;
                        }

                        edges.push(EdgeEntry {
                            src_chunk: Vector2i::new(chunk_key[0], chunk_key[1]),
                            src_cell: Vector2i::new(cell_key[0], cell_key[1]),
                            adj_chunk: Vector2i::new(adj_chunk[0], adj_chunk[1]),
                            adj_cell: Vector2i::new(x, z),
                        });
                    }
                }
            }
        }

        // Apply collected edges to the appropriate dictionaries
        for edge in &edges {
            let adj_chunk_gd = terrain.bind().get_chunk(edge.adj_chunk.x, edge.adj_chunk.y);

            match self.mode {
                TerrainToolMode::GrassMask => {
                    Self::copy_dict_entry(
                        do_grass_mask,
                        edge.src_chunk,
                        edge.src_cell,
                        edge.adj_chunk,
                        edge.adj_cell,
                    );
                    if let Some(adj) = &adj_chunk_gd {
                        let restore = adj
                            .bind()
                            .get_grass_mask_at(edge.adj_cell.x, edge.adj_cell.y);
                        Self::set_nested_dict(
                            undo_grass_mask,
                            edge.adj_chunk,
                            edge.adj_cell,
                            restore.to_variant(),
                        );
                    }
                }
                TerrainToolMode::VertexPaint if self.paint_walls_mode => {
                    Self::copy_dict_entry(
                        do_wall_color_0,
                        edge.src_chunk,
                        edge.src_cell,
                        edge.adj_chunk,
                        edge.adj_cell,
                    );
                    Self::copy_dict_entry(
                        do_wall_color_1,
                        edge.src_chunk,
                        edge.src_cell,
                        edge.adj_chunk,
                        edge.adj_cell,
                    );
                    if let Some(adj) = &adj_chunk_gd {
                        Self::set_nested_dict(
                            undo_wall_color_0,
                            edge.adj_chunk,
                            edge.adj_cell,
                            adj.bind()
                                .get_wall_color_0(edge.adj_cell.x, edge.adj_cell.y)
                                .to_variant(),
                        );
                        Self::set_nested_dict(
                            undo_wall_color_1,
                            edge.adj_chunk,
                            edge.adj_cell,
                            adj.bind()
                                .get_wall_color_1(edge.adj_cell.x, edge.adj_cell.y)
                                .to_variant(),
                        );
                    }
                }
                TerrainToolMode::VertexPaint => {
                    Self::copy_dict_entry(
                        do_color_0,
                        edge.src_chunk,
                        edge.src_cell,
                        edge.adj_chunk,
                        edge.adj_cell,
                    );
                    Self::copy_dict_entry(
                        do_color_1,
                        edge.src_chunk,
                        edge.src_cell,
                        edge.adj_chunk,
                        edge.adj_cell,
                    );
                    if let Some(adj) = &adj_chunk_gd {
                        Self::set_nested_dict(
                            undo_color_0,
                            edge.adj_chunk,
                            edge.adj_cell,
                            adj.bind()
                                .get_color_0(edge.adj_cell.x, edge.adj_cell.y)
                                .to_variant(),
                        );
                        Self::set_nested_dict(
                            undo_color_1,
                            edge.adj_chunk,
                            edge.adj_cell,
                            adj.bind()
                                .get_color_1(edge.adj_cell.x, edge.adj_cell.y)
                                .to_variant(),
                        );
                    }
                }
                _ => {
                    // Height modes
                    Self::copy_dict_entry(
                        do_height,
                        edge.src_chunk,
                        edge.src_cell,
                        edge.adj_chunk,
                        edge.adj_cell,
                    );
                    if let Some(adj) = &adj_chunk_gd {
                        let restore = adj.bind().get_height(edge.adj_cell);
                        Self::set_nested_dict(
                            undo_height,
                            edge.adj_chunk,
                            edge.adj_cell,
                            restore.to_variant(),
                        );
                    }
                }
            }
        }
    }

    /// Copy a value from src nested dict entry to adj nested dict entry.
    fn copy_dict_entry(
        dict: &mut VarDictionary,
        src_chunk: Vector2i,
        src_cell: Vector2i,
        adj_chunk: Vector2i,
        adj_cell: Vector2i,
    ) {
        if let Some(src_outer) = dict.get(src_chunk) {
            let src_dict: VarDictionary = src_outer.to();
            if let Some(val) = src_dict.get(src_cell) {
                let mut adj_dict: VarDictionary = dict.get_or_nil(adj_chunk).to();
                adj_dict.set(adj_cell, val);
                dict.set(adj_chunk, adj_dict);
            }
        }
    }

    /// Set a value in a nested VarDictionary (outer[chunk][cell] = value).
    fn set_nested_dict(dict: &mut VarDictionary, chunk: Vector2i, cell: Vector2i, value: Variant) {
        let mut inner: VarDictionary = dict.get_or_nil(chunk).to();
        inner.set(cell, value);
        dict.set(chunk, inner);
    }

    /// Expand wall colors to adjacent cells for height modification modes.
    /// Ensures uniform wall color around height-modified cells.
    #[allow(clippy::too_many_arguments)]
    fn expand_wall_colors(
        &mut self,
        terrain: &Gd<PixyTerrain>,
        dim: Vector3i,
        height_pattern: &VarDictionary,
        do_wall_0: &mut VarDictionary,
        undo_wall_0: &mut VarDictionary,
        do_wall_1: &mut VarDictionary,
        undo_wall_1: &mut VarDictionary,
    ) {
        // Set vertex colors from terrain's default wall texture
        let default_wall_tex = terrain.bind().default_wall_texture;
        self.set_vertex_colors(default_wall_tex);

        let vc0 = self.vertex_color_0;
        let vc1 = self.vertex_color_1;

        // Collect all cells in the height pattern, then expand ±1
        let mut cells_to_process: Vec<(Vector2i, Vector2i)> = Vec::new(); // (chunk, cell)

        for (chunk_key, chunk_value) in height_pattern.iter_shared() {
            let chunk_coords: Vector2i = chunk_key.to();
            let cell_dict: VarDictionary = chunk_value.to();
            for (cell_key, _) in cell_dict.iter_shared() {
                let cell_coords: Vector2i = cell_key.to();
                cells_to_process.push((chunk_coords, cell_coords));
            }
        }

        for (chunk_coords, cell_coords) in &cells_to_process {
            for dx in -1i32..=1 {
                for dz in -1i32..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }

                    let mut adj_x = cell_coords.x + dx;
                    let mut adj_z = cell_coords.y + dz;
                    let mut adj_chunk = *chunk_coords;

                    // Handle chunk boundary crossings
                    if adj_x < 0 {
                        adj_chunk.x -= 1;
                        adj_x = dim.x - 1;
                    } else if adj_x >= dim.x {
                        adj_chunk.x += 1;
                        adj_x = 0;
                    }
                    if adj_z < 0 {
                        adj_chunk.y -= 1;
                        adj_z = dim.z - 1;
                    } else if adj_z >= dim.z {
                        adj_chunk.y += 1;
                        adj_z = 0;
                    }

                    if !terrain.bind().has_chunk(adj_chunk.x, adj_chunk.y) {
                        continue;
                    }

                    let adj_cell = Vector2i::new(adj_x, adj_z);

                    // Skip if already in wall pattern
                    if let Some(existing) = do_wall_0.get(adj_chunk) {
                        let d: VarDictionary = existing.to();
                        if d.contains_key(adj_cell) {
                            continue;
                        }
                    }

                    let adj_chunk_gd = terrain.bind().get_chunk(adj_chunk.x, adj_chunk.y);
                    let Some(adj_chunk_gd) = adj_chunk_gd else {
                        continue;
                    };

                    // Get current values for undo
                    let old_wc0 = adj_chunk_gd.bind().get_wall_color_0(adj_x, adj_z);
                    let old_wc1 = adj_chunk_gd.bind().get_wall_color_1(adj_x, adj_z);

                    // Set do values
                    let mut do_chunk_0: VarDictionary = do_wall_0.get_or_nil(adj_chunk).to();
                    do_chunk_0.set(adj_cell, vc0);
                    do_wall_0.set(adj_chunk, do_chunk_0);

                    let mut undo_chunk_0: VarDictionary = undo_wall_0.get_or_nil(adj_chunk).to();
                    undo_chunk_0.set(adj_cell, old_wc0);
                    undo_wall_0.set(adj_chunk, undo_chunk_0);

                    let mut do_chunk_1: VarDictionary = do_wall_1.get_or_nil(adj_chunk).to();
                    do_chunk_1.set(adj_cell, vc1);
                    do_wall_1.set(adj_chunk, do_chunk_1);

                    let mut undo_chunk_1: VarDictionary = undo_wall_1.get_or_nil(adj_chunk).to();
                    undo_chunk_1.set(adj_cell, old_wc1);
                    undo_wall_1.set(adj_chunk, undo_chunk_1);
                }
            }
        }
    }

    /// Register an undo/redo action for a composite pattern operation.
    fn register_undo_redo(
        &mut self,
        action_name: &str,
        terrain_node: &Gd<Node>,
        do_patterns: VarDictionary,
        undo_patterns: VarDictionary,
    ) {
        let Some(mut undo_redo) = self.base_mut().get_undo_redo() else {
            godot_warn!("No EditorUndoRedoManager available");
            return;
        };

        let plugin_gd = self.to_gd();

        undo_redo.create_action(action_name);
        undo_redo.add_do_method(
            &plugin_gd,
            "apply_composite_pattern_action",
            &[terrain_node.to_variant(), do_patterns.to_variant()],
        );
        undo_redo.add_undo_method(
            &plugin_gd,
            "apply_composite_pattern_action",
            &[terrain_node.to_variant(), undo_patterns.to_variant()],
        );
        undo_redo.commit_action();
    }

    /// Register undo/redo for chunk add/remove operations.
    fn register_chunk_undo_redo(
        &mut self,
        terrain_node: &Gd<Node>,
        chunk_x: i32,
        chunk_z: i32,
        action_name: &str,
        is_remove: bool,
    ) {
        let Some(mut undo_redo) = self.base_mut().get_undo_redo() else {
            godot_warn!("No EditorUndoRedoManager available");
            return;
        };

        let terrain_clone = terrain_node.clone();

        if is_remove {
            undo_redo.create_action(action_name);
            undo_redo.add_do_method(
                &terrain_clone,
                "remove_chunk_from_tree",
                &[chunk_x.to_variant(), chunk_z.to_variant()],
            );
            undo_redo.add_undo_method(
                &terrain_clone,
                "add_new_chunk",
                &[chunk_x.to_variant(), chunk_z.to_variant()],
            );
            undo_redo.commit_action();
        } else {
            undo_redo.create_action(action_name);
            undo_redo.add_do_method(
                &terrain_clone,
                "add_new_chunk",
                &[chunk_x.to_variant(), chunk_z.to_variant()],
            );
            undo_redo.add_undo_method(
                &terrain_clone,
                "remove_chunk",
                &[chunk_x.to_variant(), chunk_z.to_variant()],
            );
            undo_redo.commit_action();
        }
    }
}
