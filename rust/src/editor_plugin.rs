use std::collections::HashMap;

use godot::classes::editor_plugin::AfterGuiInput;
use godot::classes::editor_plugin::CustomControlContainer;
use godot::classes::{
    Button, ButtonGroup, Camera3D, CenterContainer, CheckBox, ColorPickerButton, EditorPlugin,
    EditorResourcePicker, HBoxContainer, HSeparator, HSlider, IEditorPlugin, Input, InputEvent,
    InputEventKey, InputEventMouseButton, InputEventMouseMotion, Label, MarginContainer,
    OptionButton, PhysicsRayQueryParameters3D, ScrollContainer, SpinBox, StaticBody3D,
    VBoxContainer,
};
use godot::prelude::*;

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

// =======================================
// Enums
// =======================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainToolMode {
    #[default]
    Height = 0,
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

// =======================================
// Plugin Struct
// =======================================

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
    #[init(val = TerrainToolMode::Height)]
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
    /// Original click position for height drag calculations (two-clickworkflow).
    #[init(val = Vector3::ZERO)]
    setting_start_position: Vector3,

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

    // Collision debug toggle
    #[init(val = false)]
    show_collision_wireframes: bool,
    #[init(val = None)]
    collision_toggle_button: Option<Gd<CheckBox>>,

    // Chunk management state
    #[init(val = None)]
    selected_chunk_coords: Option<Vector2i>,
}

// =======================================
// IEditorPlugin Implementation
// =======================================
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

        // -- Tool Mode Buttons --
        let sep = HSeparator::new_alloc();
        toolbar.add_child(&sep);

        let mut tools_label = Label::new_alloc();
        tools_label.set_text("Tools");
        toolbar.add_child(&tools_label);

        let button_group = ButtonGroup::new_gd();
        let tool_labels = [
            "Height",
            "Level",
            "Smooth",
            "Bridge",
            "Grass Mask",
            "Vertex Paint",
            "Debug",
            "Chunks",
            "Settings",
        ];
        let tool_tooltips = [
            "Height Tool\n\nElevate or lower terrain
  height.\n\n[Shortcuts]\n\
               \u{2022} Click+Drag: Set height by dragging up/down\n\
               \u{2022} Shift+Click+Drag: Paint selection continuously\n\
               \u{2022} Shift+Scroll: Adjust brush size\n\
               \u{2022} Alt: Clear current selection",
            "Level Tool\n\nSet terrain to a specific
  height.\n\n[Shortcuts]\n\
               \u{2022} Ctrl+Click: Sample height from terrain\n\
               \u{2022} Shift+Click+Drag: Paint at set height",
            "Smooth Tool\n\nSmooth out rough terrain
  areas.\n\n[Shortcuts]\n\
               \u{2022} Shift+Click+Drag: Smooth terrain",
            "Bridge Tool\n\nCreate slopes between two
  points.\n\n[Shortcuts]\n\
               \u{2022} Click start, drag to end\n\u{2022} Ease controls slope
   curve",
            "Grass Mask Tool\n\nEnable/disable grass on
  terrain.\n\n[Shortcuts]\n\
               \u{2022} Click to toggle grass mask",
            "Vertex Paint Tool\n\nPaint texture materials on
  terrain.\n\n[Shortcuts]\n\
               \u{2022} Select material slot first\n\
               \u{2022} Paint Walls: toggle wall vs floor painting",
            "Debug Brush\n\nPrint cell data to console.\n\nUseful for
  debugging terrain data.",
            "Chunk Management\n\nAdd/remove terrain
  chunks.\n\n[Shortcuts]\n\
               \u{2022} Click empty area: Add chunk (if adjacent)\n\
               \u{2022} Click existing chunk: Remove chunk",
            "Terrain Settings\n\nAdjust global terrain parameters.\n\n\
               Dimensions, cell size, blend mode, etc.",
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
            btn.set_tooltip_text(tool_tooltips[i]);
            btn.set_toggle_mode(true);
            btn.set_button_group(&button_group);
            btn.set_custom_minimum_size(Vector2::new(BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT));

            let callable = Callable::from_object_method(&plugin_ref, "on_tool_button_toggled")
                .bindv(&varray![i as i32]);
            btn.connect("toggled", &callable);

            toolbar.add_child(&btn);
            tool_buttons.push(btn);
        }

        // -- Debug Options --
        let debug_sep = HSeparator::new_alloc();
        toolbar.add_child(&debug_sep);

        let mut collision_toggle = CheckBox::new_alloc();
        collision_toggle.set_text("Show Colliders");
        collision_toggle.set_tooltip_text(
            "Toggle collision wireframe
  visibility",
        );
        collision_toggle.set_pressed(false);

        collision_toggle.set_custom_minimum_size(Vector2::new(BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT));
        let collision_callable =
            Callable::from_object_method(&plugin_ref, "on_collision_toggle_changed");
        collision_toggle.connect("toggled", &collision_callable);
        toolbar.add_child(&collision_toggle);

        // Pre-press Brush button (deferred to avoid triggering signal during enter_tree)
        if let Some(first_btn) = tool_buttons.first_mut() {
            first_btn.call_deferred("set_pressed", &[true.to_variant()]);
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

        // -- Bottom Attributes Panel --
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
        self.collision_toggle_button = Some(collision_toggle);

        // Register gizmo plugin
        let mut gizmo_plugin = Gd::<PixyTerrainGizmoPlugin>::default();
        gizmo::init_gizmo_plugin(&mut gizmo_plugin);
        gizmo_plugin.bind_mut().plugin_ref = Some(self.to_gd());
        self.base_mut().add_node_3d_gizmo_plugin(&gizmo_plugin);
        self.gizmo_plugin = Some(gizmo_plugin);

        // -- Right-Side Texture Settings Panel --
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
        self.collision_toggle_button = None;
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
                self.base_mut()
                    .call_deferred("apply_collision_visibility_deferred", &[]);
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

        // -- Brush/drawing tool modes --
        let is_draw_mode = matches!(
            self.mode,
            TerrainToolMode::Height
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
                // Strategy 1: Setting mode - vertical plane through base_position
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
                // Strategy 2: Flatten mode - horizontal plane at draw_height
                let chunk_plane = Plane::new(Vector3::UP, self.draw_height);
                if let Some(world_pos) = chunk_plane.intersect_ray(ray_origin, ray_dir) {
                    draw_position = Some(terrain_gd.to_local(world_pos));
                }
            } else if self.is_drawing && self.mode == TerrainToolMode::Level {
                // Strategy 3: Level drawing mode - horizontal plane at target height
                let level_plane = Plane::new(Vector3::UP, self.height);
                if let Some(world_pos) = level_plane.intersect_ray(ray_origin, ray_dir) {
                    draw_position = Some(terrain_gd.to_local(world_pos));
                }
            } else {
                // Strategy 4: Default - physics raycast
                if let Some(mut world) = camera.get_world_3d() {
                    if let Some(mut space) = world.get_direct_space_state() {
                        let ray_end = ray_origin + ray_dir * 10000.0;
                        let query = PhysicsRayQueryParameters3D::create_ex(ray_origin, ray_end)
                            .collision_mask(1 << 16)
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
                if !(self.is_setting && self.draw_height_set) {
                    self.brush_position = pos;
                }
            }

            // ALT to clear pattern (unless setting)
            if alt_held && !self.is_setting {
                self.current_draw_pattern.clear();
            }

            // -- Mouse button handling --
            if is_button_event {
                let btn: Gd<InputEventMouseButton> = event.clone().cast();
                if btn.get_button_index() == godot::global::MouseButton::LEFT {
                    // Second click while in height adjustment mode -> apply and reset
                    if btn.is_pressed() && self.is_setting && self.draw_height_set {
                        self.draw_pattern(&terrain, dim, cell_size);
                        self.is_setting = false;
                        self.draw_height_set = false;
                        self.current_draw_pattern.clear();
                        return AfterGuiInput::STOP.ord();
                    }

                    if btn.is_pressed() && draw_area_hovered {
                        // Mode-specific press initialization
                        if self.mode == TerrainToolMode::Bridge && !self.is_making_bridge {
                            self.flatten = false;
                            self.is_making_bridge = true;
                            self.bridge_start_pos = self.brush_position;
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
                        } else if matches!(
                            self.mode,
                            TerrainToolMode::Level
                                | TerrainToolMode::Smooth
                                | TerrainToolMode::GrassMask
                                | TerrainToolMode::VertexPaint
                        ) {
                            // Level/Smooth/GrassMask/VertexPaint: simple click-drag-release
                            self.is_drawing = true;
                        } else {
                            // Normal click: enter setting mode (two-click workflow)
                            self.is_setting = true;
                            if !self.flatten {
                                self.draw_height = self.brush_position.y;
                            }
                        }

                        // Initialize draw state
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
                            self.draw_height_set = false;
                        }
                        // Two-click workflow: release enters height adjustment mode
                        if self.is_setting && !self.draw_height_set {
                            self.draw_height_set = true;
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
                        self.sync_brush_size_slider();
                        return AfterGuiInput::STOP.ord();
                    } else if button_idx == godot::global::MouseButton::WHEEL_DOWN {
                        self.brush_size =
                            (self.brush_size - BRUSH_SIZE_STEP * factor).max(MIN_BRUSH_SIZE);
                        self.sync_brush_size_slider();
                        return AfterGuiInput::STOP.ord();
                    }
                }
            }

            // -- Mouse motion during paint phase --
            if is_motion_event && self.is_setting && !self.draw_height_set && draw_area_hovered {
                self.build_draw_pattern(&terrain, dim, cell_size);
            }

            // -- Mouse motion in height adjustment mode --
            // brush_position.y already updated by vertical plane raycast above

            // -- Mouse motion while drawing (shift+drag mode) --
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

        // -- Chunk Management mode --
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

// =======================================
// #[func] Methods (callable from GDScript / undo-redo)
// =======================================

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

    #[func]
    fn on_collision_toggle_changed(&mut self, pressed: bool) {
        self.show_collision_wireframes = pressed;
        self.apply_collision_visibility_to_all_chunks();
    }

    #[func]
    fn apply_collision_visibility_deferred(&self) {
        self.apply_collision_visibility_to_all_chunks();
    }

    /// Deferred rebuild of attributes panel - safe to call to_gd() here.
    #[func]
    fn _rebuild_attributes_deferred(&mut self) {
        let plugin_ref = self.to_gd();
        self.rebuild_attributes_impl(plugin_ref);
    }

    /// Deferred rebuild of texture panel - safe to call to_gd() here.
    #[func]
    fn _rebuild_texture_panel_deferred(&mut self) {
        let plugin_ref = self.to_gd();
        self.rebuild_texture_panel_impl(plugin_ref);
    }

    /// Called when a tool mode toggle button is pressed.
    #[func]
    fn on_tool_button_toggled(&mut self, pressed: bool, tool_index: i32) {
        if !pressed {
            return;
        }
        self.mode = match tool_index {
            0 => TerrainToolMode::Height,
            1 => TerrainToolMode::Level,
            2 => TerrainToolMode::Smooth,
            3 => TerrainToolMode::Bridge,
            4 => TerrainToolMode::GrassMask,
            5 => TerrainToolMode::VertexPaint,
            6 => TerrainToolMode::DebugBrush,
            7 => TerrainToolMode::ChunkManagement,
            8 => TerrainToolMode::TerrainSettings,
            _ => TerrainToolMode::Height,
        };
        // Use call_deferred to avoid borrow conflict from signal dispatch
        self.base_mut()
            .call_deferred("_rebuild_attributes_deferred", &[]);
    }
}

// =======================================
// Private methods + stubs for Parts 16-17
// =======================================

impl PixyTerrainPlugin {
    /// Build a GizmoState snapshot from current brush state.
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
            is_setting: self.is_setting,
            draw_height_set: self.draw_height_set,
            is_drawing: self.is_drawing,
        }
    }

    fn set_ui_visible(&mut self, visible: bool) {
        if let Some(margin) = self.margin_container.as_mut() {
            margin.set_visible(visible);
        }
        if let Some(scroll) = self.attributes_container.as_mut() {
            scroll.set_visible(visible);
        }
        if let Some(tex) = self.texture_panel.as_mut() {
            tex.set_visible(visible);
        }
        if visible {
            self.base_mut()
                .call_deferred("_rebuild_attributes_deferred", &[]);
            self.base_mut()
                .call_deferred("_rebuild_texture_panel_deferred", &[]);
        }
    }

    fn do_generate(&mut self) {
        if let Some(terrain_node) = self
            .current_terrain
            .as_ref()
            .filter(|t| t.is_instance_valid())
            .cloned()
        {
            self.is_modifying = true;
            let mut terrain: Gd<PixyTerrain> = terrain_node.cast();
            terrain.bind_mut().regenerate();
            self.is_modifying = false;
        }
    }

    fn do_clear(&mut self) {
        if let Some(terrain_node) = self
            .current_terrain
            .as_ref()
            .filter(|t| t.is_instance_valid())
            .cloned()
        {
            self.is_modifying = true;
            let mut terrain: Gd<PixyTerrain> = terrain_node.cast();
            terrain.bind_mut().clear();
            self.is_modifying = false;
        }
    }

    fn update_gizmos(&self) {
        if let Some(terrain_node) = self
            .current_terrain
            .as_ref()
            .filter(|t| t.is_instance_valid())
        {
            let mut terrain_3d: Gd<Node3D> = terrain_node.clone().cast();
            terrain_3d.update_gizmos();
        }
    }

    fn sync_brush_size_slider(&self) {
        // Stub — will sync UI slider in Part 17
    }

    fn apply_collision_visibility_to_all_chunks(&self) {
        // Stub — will iterate chunk StaticBody3D children in Part 16
    }

    fn initialize_draw_state(
        &mut self,
        _terrain: &Gd<PixyTerrain>,
        _dim: Vector3i,
        _cell_size: Vector2,
    ) {
        // Stub — will handle two-click workflow entry in Part 16
    }

    fn build_draw_pattern(
        &mut self,
        _terrain: &Gd<PixyTerrain>,
        _dim: Vector3i,
        _cell_size: Vector2,
    ) {
        // Stub — will calculate brush cells with falloff in Part 16
    }

    fn draw_pattern(&mut self, _terrain: &Gd<PixyTerrain>, _dim: Vector3i, _cell_size: Vector2) {
        // Stub — will apply pattern to terrain in Part 16
    }

    fn rebuild_attributes_impl(&mut self, _plugin_ref: Gd<PixyTerrainPlugin>) {
        // Stub — will build per-mode attribute controls in Part 17
    }

    fn rebuild_texture_panel_impl(&mut self, _plugin_ref: Gd<PixyTerrainPlugin>) {
        // Stub — will build right-side texture panel in Part 17
    }

    fn register_chunk_undo_redo(
        &mut self,
        _terrain_node: &Gd<Node>,
        _chunk_x: i32,
        _chunk_z: i32,
        _action_name: &str,
        _is_remove: bool,
    ) {
        // Stub — will create undo/redo actions in Part 17
    }
}

