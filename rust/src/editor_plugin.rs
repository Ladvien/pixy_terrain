use godot::classes::editor_plugin::AfterGuiInput;
use godot::classes::editor_plugin::CustomControlContainer;
use godot::classes::{
    Button, Camera3D, EditorPlugin, HBoxContainer, HSlider, IEditorPlugin, InputEvent,
    InputEventKey, InputEventMouseButton, InputEventMouseMotion, Label, MarginContainer,
    PhysicsRayQueryParameters3D, VBoxContainer, VSeparator,
};
use godot::global::MouseButton;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(tool, init, base=EditorPlugin)]
pub struct PixyTerrainPlugin {
    base: Base<EditorPlugin>,
    #[init(val = None)]
    current_terrain: Option<Gd<Node>>,
    #[init(val = None)]
    margin_container: Option<Gd<MarginContainer>>,
    #[init(val = None)]
    toolbar: Option<Gd<VBoxContainer>>,

    // Generation buttons
    #[init(val = None)]
    generate_button: Option<Gd<Button>>,
    #[init(val = None)]
    clear_button: Option<Gd<Button>>,

    // Post-processing buttons
    #[init(val = None)]
    merge_button: Option<Gd<Button>>,
    #[init(val = None)]
    weld_button: Option<Gd<Button>>,
    #[init(val = None)]
    decimate_button: Option<Gd<Button>>,
    #[init(val = None)]
    normals_button: Option<Gd<Button>>,

    // Brush buttons
    #[init(val = None)]
    brush_toggle_button: Option<Gd<Button>>,
    #[init(val = None)]
    elevation_mode_button: Option<Gd<Button>>,
    #[init(val = None)]
    texture_mode_button: Option<Gd<Button>>,
    #[init(val = None)]
    flatten_mode_button: Option<Gd<Button>>,
    #[init(val = None)]
    plateau_mode_button: Option<Gd<Button>>,
    #[init(val = None)]
    smooth_mode_button: Option<Gd<Button>>,
    #[init(val = None)]
    strength_slider: Option<Gd<HSlider>>,
    #[init(val = None)]
    brush_size_slider: Option<Gd<HSlider>>,
    #[init(val = None)]
    step_size_slider: Option<Gd<HSlider>>,
    #[init(val = None)]
    feather_slider: Option<Gd<HSlider>>,
    #[init(val = [None, None, None, None])]
    texture_buttons: [Option<Gd<Button>>; 4],

    #[init(val = false)]
    is_modifying: bool,
    /// Last screen Y position for height adjustment
    #[init(val = 0.0)]
    last_screen_y: f32,
    /// Track if we're currently dragging for brush
    #[init(val = false)]
    brush_dragging: bool,
}

#[godot_api]
impl IEditorPlugin for PixyTerrainPlugin {
    fn enter_tree(&mut self) {
        godot_print!("PixyTerrainPlugin: enter_tree called");

        // Create MarginContainer for outer padding
        let mut margin_container = MarginContainer::new_alloc();
        margin_container.set_name("PixyTerrainMargin");
        margin_container.set_visible(false);
        margin_container.set_custom_minimum_size(Vector2::new(140.0, 0.0));
        margin_container.add_theme_constant_override("margin_top", 8);
        margin_container.add_theme_constant_override("margin_left", 8);
        margin_container.add_theme_constant_override("margin_right", 8);
        margin_container.add_theme_constant_override("margin_bottom", 8);

        // Create VBoxContainer for vertical button layout
        let mut toolbar = VBoxContainer::new_alloc();
        toolbar.set_name("PixyTerrainToolbar");
        toolbar.add_theme_constant_override("separation", 4);

        // ═══════════════════════════════════════════════════════════════════
        // Generation Section
        // ═══════════════════════════════════════════════════════════════════
        let mut gen_label = Label::new_alloc();
        gen_label.set_text("Generation");
        toolbar.add_child(&gen_label);

        let mut generate_button = Button::new_alloc();
        generate_button.set_text("Generate (G)");
        generate_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));

        let mut clear_button = Button::new_alloc();
        clear_button.set_text("Clear (C)");
        clear_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));

        toolbar.add_child(&generate_button);
        toolbar.add_child(&clear_button);

        // ═══════════════════════════════════════════════════════════════════
        // Brush Section
        // ═══════════════════════════════════════════════════════════════════
        let mut sep1 = VSeparator::new_alloc();
        sep1.set_custom_minimum_size(Vector2::new(0.0, 8.0));
        toolbar.add_child(&sep1);

        let mut brush_label = Label::new_alloc();
        brush_label.set_text("Brush Painting");
        toolbar.add_child(&brush_label);

        // Brush toggle
        let mut brush_toggle_button = Button::new_alloc();
        brush_toggle_button.set_text("Enable Brush (B)");
        brush_toggle_button.set_toggle_mode(true);
        brush_toggle_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));
        toolbar.add_child(&brush_toggle_button);

        // Mode buttons — stacked vertically
        let mut elevation_mode_button = Button::new_alloc();
        elevation_mode_button.set_text("Elevation (E)");
        elevation_mode_button.set_toggle_mode(true);
        elevation_mode_button.set_pressed(true);
        elevation_mode_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));
        elevation_mode_button.set_tooltip_text("Elevation Mode - Sculpt terrain height");

        let mut texture_mode_button = Button::new_alloc();
        texture_mode_button.set_text("Texture");
        texture_mode_button.set_toggle_mode(true);
        texture_mode_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));
        texture_mode_button.set_tooltip_text("Texture Mode - Paint terrain textures");

        let mut flatten_mode_button = Button::new_alloc();
        flatten_mode_button.set_text("Flatten (F)");
        flatten_mode_button.set_toggle_mode(true);
        flatten_mode_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));
        flatten_mode_button.set_tooltip_text("Flatten Mode - Level terrain to click height");

        let mut plateau_mode_button = Button::new_alloc();
        plateau_mode_button.set_text("Plateau (P)");
        plateau_mode_button.set_toggle_mode(true);
        plateau_mode_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));
        plateau_mode_button.set_tooltip_text("Plateau Mode - Snap terrain to step heights");

        let mut smooth_mode_button = Button::new_alloc();
        smooth_mode_button.set_text("Smooth (S)");
        smooth_mode_button.set_toggle_mode(true);
        smooth_mode_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));
        smooth_mode_button.set_tooltip_text("Smooth Mode - Laplacian smoothing on terrain");

        toolbar.add_child(&elevation_mode_button);
        toolbar.add_child(&texture_mode_button);
        toolbar.add_child(&flatten_mode_button);
        toolbar.add_child(&plateau_mode_button);
        toolbar.add_child(&smooth_mode_button);

        // Strength slider
        let mut strength_label = Label::new_alloc();
        strength_label.set_text("Strength:");
        toolbar.add_child(&strength_label);

        let mut strength_slider = HSlider::new_alloc();
        strength_slider.set_min(0.0);
        strength_slider.set_max(1.0);
        strength_slider.set_step(0.05);
        strength_slider.set_value(1.0);
        strength_slider.set_custom_minimum_size(Vector2::new(100.0, 0.0));
        strength_slider.set_tooltip_text("Brush strength (0.0 - 1.0)");
        toolbar.add_child(&strength_slider);

        // Brush size slider
        let mut size_label = Label::new_alloc();
        size_label.set_text("Size:");
        toolbar.add_child(&size_label);

        let mut brush_size_slider = HSlider::new_alloc();
        brush_size_slider.set_min(1.0);
        brush_size_slider.set_max(50.0);
        brush_size_slider.set_step(1.0);
        brush_size_slider.set_value(5.0);
        brush_size_slider.set_custom_minimum_size(Vector2::new(100.0, 0.0));
        brush_size_slider.set_tooltip_text("Brush size ([ / ] keys)");
        toolbar.add_child(&brush_size_slider);

        // Step size slider (for Plateau mode)
        let mut step_size_label = Label::new_alloc();
        step_size_label.set_text("Step Size:");
        toolbar.add_child(&step_size_label);

        let mut step_size_slider = HSlider::new_alloc();
        step_size_slider.set_min(0.5);
        step_size_slider.set_max(32.0);
        step_size_slider.set_step(0.5);
        step_size_slider.set_value(4.0);
        step_size_slider.set_custom_minimum_size(Vector2::new(100.0, 0.0));
        step_size_slider.set_tooltip_text("Step size for plateau mode (world units per level)");
        toolbar.add_child(&step_size_slider);

        // Feather slider
        let mut feather_label = Label::new_alloc();
        feather_label.set_text("Feather:");
        toolbar.add_child(&feather_label);

        let mut feather_slider = HSlider::new_alloc();
        feather_slider.set_min(0.0);
        feather_slider.set_max(1.0);
        feather_slider.set_step(0.05);
        feather_slider.set_value(0.0);
        feather_slider.set_custom_minimum_size(Vector2::new(100.0, 0.0));
        feather_slider.set_tooltip_text("Brush feather (0.0 = hard edge, 1.0 = full falloff)");
        toolbar.add_child(&feather_slider);

        // Texture selection buttons
        let mut tex_label = Label::new_alloc();
        tex_label.set_text("Texture:");
        toolbar.add_child(&tex_label);

        let mut tex_container = HBoxContainer::new_alloc();
        tex_container.add_theme_constant_override("separation", 2);

        let mut texture_buttons: [Option<Gd<Button>>; 4] = [None, None, None, None];
        for i in 0..4 {
            let mut tex_button = Button::new_alloc();
            tex_button.set_text(&format!("{}", i + 1));
            tex_button.set_toggle_mode(true);
            if i == 0 {
                tex_button.set_pressed(true);
            }
            tex_button.set_custom_minimum_size(Vector2::new(28.0, 28.0));
            tex_button.set_tooltip_text(&format!("Select texture {} ({})", i + 1, i + 1));
            tex_container.add_child(&tex_button);
            texture_buttons[i] = Some(tex_button);
        }
        toolbar.add_child(&tex_container);

        // ═══════════════════════════════════════════════════════════════════
        // Post-Processing Section
        // ═══════════════════════════════════════════════════════════════════
        let mut sep2 = VSeparator::new_alloc();
        sep2.set_custom_minimum_size(Vector2::new(0.0, 8.0));
        toolbar.add_child(&sep2);

        let mut post_label = Label::new_alloc();
        post_label.set_text("Post-Processing");
        toolbar.add_child(&post_label);

        let mut merge_button = Button::new_alloc();
        merge_button.set_text("Merge & Export");
        merge_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));

        let mut weld_button = Button::new_alloc();
        weld_button.set_text("Weld Seams");
        weld_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));

        let mut decimate_button = Button::new_alloc();
        decimate_button.set_text("Decimate");
        decimate_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));

        let mut normals_button = Button::new_alloc();
        normals_button.set_text("Recompute Normals");
        normals_button.set_custom_minimum_size(Vector2::new(100.0, 28.0));

        toolbar.add_child(&merge_button);
        toolbar.add_child(&weld_button);
        toolbar.add_child(&decimate_button);
        toolbar.add_child(&normals_button);

        // Add VBoxContainer to MarginContainer
        margin_container.add_child(&toolbar);

        // ═══════════════════════════════════════════════════════════════════
        // Connect Signals
        // ═══════════════════════════════════════════════════════════════════
        let plugin_ref = self.to_gd();

        // Generation buttons
        generate_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_generate_pressed"),
        );
        clear_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_clear_pressed"),
        );

        // Brush buttons
        brush_toggle_button.connect(
            "toggled",
            &Callable::from_object_method(&plugin_ref, "on_brush_toggled"),
        );
        elevation_mode_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_elevation_mode_pressed"),
        );
        texture_mode_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_texture_mode_pressed"),
        );
        brush_size_slider.connect(
            "value_changed",
            &Callable::from_object_method(&plugin_ref, "on_brush_size_changed"),
        );
        flatten_mode_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_flatten_mode_pressed"),
        );
        plateau_mode_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_plateau_mode_pressed"),
        );
        smooth_mode_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_smooth_mode_pressed"),
        );
        strength_slider.connect(
            "value_changed",
            &Callable::from_object_method(&plugin_ref, "on_strength_changed"),
        );
        step_size_slider.connect(
            "value_changed",
            &Callable::from_object_method(&plugin_ref, "on_step_size_changed"),
        );
        feather_slider.connect(
            "value_changed",
            &Callable::from_object_method(&plugin_ref, "on_feather_changed"),
        );

        // Texture buttons
        for (i, tex_btn) in texture_buttons.iter().enumerate() {
            if let Some(ref btn) = tex_btn {
                let method_name = format!("on_texture_{}_pressed", i);
                btn.clone().connect(
                    "pressed",
                    &Callable::from_object_method(&plugin_ref, &method_name),
                );
            }
        }

        // Post-processing buttons
        merge_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_merge_pressed"),
        );
        weld_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_weld_pressed"),
        );
        decimate_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_decimate_pressed"),
        );
        normals_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_normals_pressed"),
        );

        // Add MarginContainer to the spatial editor side left
        self.base_mut().add_control_to_container(
            CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT,
            &margin_container,
        );

        // Store references
        self.margin_container = Some(margin_container);
        self.toolbar = Some(toolbar);
        self.generate_button = Some(generate_button);
        self.clear_button = Some(clear_button);
        self.merge_button = Some(merge_button);
        self.weld_button = Some(weld_button);
        self.decimate_button = Some(decimate_button);
        self.normals_button = Some(normals_button);
        self.brush_toggle_button = Some(brush_toggle_button);
        self.elevation_mode_button = Some(elevation_mode_button);
        self.texture_mode_button = Some(texture_mode_button);
        self.flatten_mode_button = Some(flatten_mode_button);
        self.plateau_mode_button = Some(plateau_mode_button);
        self.smooth_mode_button = Some(smooth_mode_button);
        self.strength_slider = Some(strength_slider);
        self.brush_size_slider = Some(brush_size_slider);
        self.step_size_slider = Some(step_size_slider);
        self.feather_slider = Some(feather_slider);
        self.texture_buttons = texture_buttons;

        godot_print!("PixyTerrainPlugin: toolbar added with brush controls");
    }

    fn exit_tree(&mut self) {
        // Clean up child refs
        self.generate_button = None;
        self.clear_button = None;
        self.merge_button = None;
        self.weld_button = None;
        self.decimate_button = None;
        self.normals_button = None;
        self.brush_toggle_button = None;
        self.elevation_mode_button = None;
        self.texture_mode_button = None;
        self.flatten_mode_button = None;
        self.plateau_mode_button = None;
        self.smooth_mode_button = None;
        self.strength_slider = None;
        self.brush_size_slider = None;
        self.step_size_slider = None;
        self.feather_slider = None;
        self.texture_buttons = [None, None, None, None];
        self.toolbar = None;

        // Remove and free the margin container
        if let Some(mut margin) = self.margin_container.take() {
            self.base_mut().remove_control_from_container(
                CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT,
                &margin,
            );
            margin.queue_free();
        }
    }

    fn handles(&self, object: Gd<Object>) -> bool {
        let class_name = object.get_class();
        class_name == "PixyTerrain"
    }

    fn edit(&mut self, object: Option<Gd<Object>>) {
        if let Some(obj) = object {
            if let Ok(node) = obj.try_cast::<Node>() {
                self.current_terrain = Some(node);
                self.set_ui_visible(true);
                self.sync_ui_from_terrain();
                return;
            }
        }
        self.set_ui_visible(false)
    }

    fn make_visible(&mut self, visible: bool) {
        // Guard against false-positive hides during child modifications
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

        // Handle keyboard shortcuts
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
                    godot::global::Key::B => {
                        self.toggle_brush();
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::E => {
                        self.ensure_brush_enabled();
                        self.set_brush_mode(0);
                        self.update_mode_buttons(0);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::F => {
                        self.ensure_brush_enabled();
                        let current_mode = self.get_terrain_property("brush_mode").to::<i32>();
                        if current_mode == 2 {
                            self.cycle_flatten_direction();
                        } else {
                            self.set_terrain_property("brush_flatten_direction", 0.to_variant());
                            self.set_brush_mode(2);
                        }
                        self.update_mode_buttons(2);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::P => {
                        self.ensure_brush_enabled();
                        self.set_brush_mode(3);
                        self.update_mode_buttons(3);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::S => {
                        self.ensure_brush_enabled();
                        self.set_brush_mode(4);
                        self.update_mode_buttons(4);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::BRACKETLEFT => {
                        self.adjust_brush_size(-1.0);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::BRACKETRIGHT => {
                        self.adjust_brush_size(1.0);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::KEY_1 => {
                        self.select_texture(0);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::KEY_2 => {
                        self.select_texture(1);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::KEY_3 => {
                        self.select_texture(2);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::KEY_4 => {
                        self.select_texture(3);
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::Z => {
                        let ctrl_or_cmd = key_event.is_ctrl_pressed()
                            || key_event.is_meta_pressed();
                        if ctrl_or_cmd && key_event.is_shift_pressed() {
                            // Ctrl+Shift+Z / Cmd+Shift+Z = Redo
                            self.call_terrain_method("redo");
                            return AfterGuiInput::STOP.ord();
                        } else if ctrl_or_cmd {
                            // Ctrl+Z / Cmd+Z = Undo
                            self.call_terrain_method("undo");
                            return AfterGuiInput::STOP.ord();
                        }
                    }
                    godot::global::Key::ESCAPE => {
                        if self.is_brush_active() {
                            self.brush_cancel();
                            self.brush_dragging = false;
                            return AfterGuiInput::STOP.ord();
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check if brush is enabled
        if !self.is_brush_enabled() {
            return AfterGuiInput::PASS.ord();
        }

        let Some(camera) = camera else {
            return AfterGuiInput::PASS.ord();
        };

        // Handle mouse button events for brush
        if let Ok(mouse_button) = event.clone().try_cast::<InputEventMouseButton>() {
            if mouse_button.get_button_index() == MouseButton::LEFT {
                let screen_pos = mouse_button.get_position();
                self.last_screen_y = screen_pos.y;

                if mouse_button.is_pressed() {
                    // During height/curvature adjustment phases, consume the press
                    // so the release triggers the next transition
                    let phase = self.get_brush_phase();
                    if phase == 2 || phase == 4 {
                        return AfterGuiInput::STOP.ord();
                    }
                    if let Some(hit_pos) = self.raycast_terrain(&camera, screen_pos) {
                        self.brush_begin(hit_pos);
                        self.brush_dragging = true;
                        return AfterGuiInput::STOP.ord();
                    }
                } else {
                    if self.brush_dragging || self.is_brush_active() {
                        let action = self.brush_end(screen_pos.y);
                        // Keep dragging for height adjust (1) and curvature adjust (7)
                        if action != 1 && action != 7 {
                            self.brush_dragging = false;
                        }
                        return AfterGuiInput::STOP.ord();
                    }
                }
            } else if mouse_button.get_button_index() == MouseButton::RIGHT {
                if self.is_brush_active() {
                    self.brush_cancel();
                    self.brush_dragging = false;
                    return AfterGuiInput::STOP.ord();
                }
            }
        }

        // Handle mouse motion for brush dragging
        if let Ok(mouse_motion) = event.try_cast::<InputEventMouseMotion>() {
            let screen_pos = mouse_motion.get_position();
            let brush_phase = self.get_brush_phase();

            if brush_phase == 2 {
                self.brush_adjust_height(screen_pos.y);
                return AfterGuiInput::STOP.ord();
            } else if brush_phase == 4 {
                self.brush_adjust_curvature(screen_pos.y);
                return AfterGuiInput::STOP.ord();
            } else if self.brush_dragging && (brush_phase == 1 || brush_phase == 3) {
                if let Some(hit_pos) = self.raycast_terrain(&camera, screen_pos) {
                    self.brush_continue(hit_pos);
                    return AfterGuiInput::STOP.ord();
                }
            }
        }

        AfterGuiInput::PASS.ord()
    }
}

#[godot_api]
impl PixyTerrainPlugin {
    // ═══════════════════════════════════════════════════════════════════════
    // Generation Callbacks
    // ═══════════════════════════════════════════════════════════════════════

    #[func]
    fn on_generate_pressed(&mut self) {
        self.do_generate();
    }

    #[func]
    fn on_clear_pressed(&mut self) {
        self.do_clear();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Brush Callbacks
    // ═══════════════════════════════════════════════════════════════════════

    #[func]
    fn on_brush_toggled(&mut self, enabled: bool) {
        self.set_terrain_property("brush_enabled", enabled.to_variant());
        godot_print!(
            "PixyTerrainPlugin: Brush {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    #[func]
    fn on_elevation_mode_pressed(&mut self) {
        self.set_brush_mode(0);
        self.update_mode_buttons(0);
    }

    #[func]
    fn on_texture_mode_pressed(&mut self) {
        self.set_brush_mode(1);
        self.update_mode_buttons(1);
    }

    #[func]
    fn on_flatten_mode_pressed(&mut self) {
        let current_mode = self.get_terrain_property("brush_mode").to::<i32>();
        if current_mode == 2 {
            // Already in flatten mode — cycle direction
            self.cycle_flatten_direction();
        } else {
            // Switch to flatten mode, reset direction to Both
            self.set_terrain_property("brush_flatten_direction", 0.to_variant());
            self.set_brush_mode(2);
        }
        self.update_mode_buttons(2);
    }

    #[func]
    fn on_plateau_mode_pressed(&mut self) {
        self.set_brush_mode(3);
        self.update_mode_buttons(3);
    }

    #[func]
    fn on_smooth_mode_pressed(&mut self) {
        self.set_brush_mode(4);
        self.update_mode_buttons(4);
    }

    #[func]
    fn on_brush_size_changed(&mut self, value: f64) {
        self.set_terrain_property("brush_size", (value as f32).to_variant());
    }

    #[func]
    fn on_strength_changed(&mut self, value: f64) {
        self.set_terrain_property("brush_strength", (value as f32).to_variant());
    }

    #[func]
    fn on_step_size_changed(&mut self, value: f64) {
        self.set_terrain_property("brush_step_size", (value as f32).to_variant());
    }

    #[func]
    fn on_feather_changed(&mut self, value: f64) {
        self.set_terrain_property("brush_feather", (value as f32).to_variant());
    }

    #[func]
    fn on_texture_0_pressed(&mut self) {
        self.select_texture(0);
    }

    #[func]
    fn on_texture_1_pressed(&mut self) {
        self.select_texture(1);
    }

    #[func]
    fn on_texture_2_pressed(&mut self) {
        self.select_texture(2);
    }

    #[func]
    fn on_texture_3_pressed(&mut self) {
        self.select_texture(3);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Post-Processing Callbacks
    // ═══════════════════════════════════════════════════════════════════════

    #[func]
    fn on_merge_pressed(&mut self) {
        self.call_terrain_method("merge_and_export");
    }

    #[func]
    fn on_weld_pressed(&mut self) {
        self.call_terrain_method("weld_seams");
    }

    #[func]
    fn on_decimate_pressed(&mut self) {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                let mut terrain_clone = terrain.clone();
                if terrain_clone.has_method("decimate_mesh") {
                    self.is_modifying = true;
                    let target = terrain_clone.get("decimation_target_triangles");
                    terrain_clone.call("decimate_mesh", &[target]);
                    self.is_modifying = false;
                }
            }
        }
    }

    #[func]
    fn on_normals_pressed(&mut self) {
        self.call_terrain_method("recompute_normals");
    }
}

impl PixyTerrainPlugin {
    fn set_ui_visible(&mut self, visible: bool) {
        if let Some(ref mut margin) = self.margin_container {
            margin.set_visible(visible);
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

    fn call_terrain_method_with_args(&mut self, method_name: &str, args: &[Variant]) -> Variant {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                let mut terrain_clone = terrain.clone();
                if terrain_clone.has_method(method_name) {
                    self.is_modifying = true;
                    let result = terrain_clone.call(method_name, args);
                    self.is_modifying = false;
                    return result;
                }
            }
        }
        Variant::nil()
    }

    fn set_terrain_property(&mut self, property: &str, value: Variant) {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                let mut terrain_clone = terrain.clone();
                terrain_clone.set(property, &value);
            }
        }
    }

    fn get_terrain_property(&self, property: &str) -> Variant {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                return terrain.clone().get(property);
            }
        }
        Variant::nil()
    }

    fn do_generate(&mut self) {
        self.call_terrain_method("regenerate");
    }

    fn do_clear(&mut self) {
        self.call_terrain_method("clear");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Brush UI Helpers
    // ═══════════════════════════════════════════════════════════════════════

    fn toggle_brush(&mut self) {
        let current = self.get_terrain_property("brush_enabled").to::<bool>();
        let new_value = !current;
        self.set_terrain_property("brush_enabled", new_value.to_variant());

        // Update toggle button state
        if let Some(ref mut btn) = self.brush_toggle_button {
            btn.set_pressed(new_value);
        }

        godot_print!(
            "PixyTerrainPlugin: Brush {}",
            if new_value { "enabled" } else { "disabled" }
        );
    }

    /// Ensure brush is enabled (auto-enable if disabled)
    fn ensure_brush_enabled(&mut self) {
        let current = self.get_terrain_property("brush_enabled").to::<bool>();
        if !current {
            self.set_terrain_property("brush_enabled", true.to_variant());
            if let Some(ref mut btn) = self.brush_toggle_button {
                btn.set_pressed(true);
            }
        }
    }

    fn set_brush_mode(&mut self, mode: i32) {
        self.set_terrain_property("brush_mode", mode.to_variant());
    }

    fn update_mode_buttons(&mut self, active_mode: i32) {
        if let Some(ref mut btn) = self.elevation_mode_button {
            btn.set_pressed(active_mode == 0);
        }
        if let Some(ref mut btn) = self.texture_mode_button {
            btn.set_pressed(active_mode == 1);
        }
        if let Some(ref mut btn) = self.flatten_mode_button {
            btn.set_pressed(active_mode == 2);
        }
        if let Some(ref mut btn) = self.plateau_mode_button {
            btn.set_pressed(active_mode == 3);
        }
        if let Some(ref mut btn) = self.smooth_mode_button {
            btn.set_pressed(active_mode == 4);
        }

        // Reset flatten direction when switching away from flatten
        if active_mode != 2 {
            self.set_terrain_property("brush_flatten_direction", 0.to_variant());
        }

        // Update flatten button text to show direction
        let flatten_dir = self
            .get_terrain_property("brush_flatten_direction")
            .to::<i32>();
        self.update_flatten_button_text(flatten_dir);

        // Update slider editability per mode
        self.update_slider_states(active_mode);
    }

    fn adjust_brush_size(&mut self, delta: f32) {
        let current = self.get_terrain_property("brush_size").to::<f32>();
        let new_size = (current + delta).max(1.0).min(50.0);
        self.set_terrain_property("brush_size", new_size.to_variant());
        self.update_size_slider(new_size);
    }

    fn update_size_slider(&mut self, size: f32) {
        if let Some(ref mut slider) = self.brush_size_slider {
            slider.set_value_no_signal(size as f64);
        }
    }

    fn select_texture(&mut self, index: i32) {
        self.set_terrain_property("selected_texture_index", index.to_variant());
        self.update_texture_buttons(index as usize);
    }

    fn update_texture_buttons(&mut self, selected: usize) {
        for (i, btn_opt) in self.texture_buttons.iter_mut().enumerate() {
            if let Some(ref mut btn) = btn_opt {
                btn.set_pressed(i == selected);
            }
        }
    }

    /// Cycle flatten direction: Both → Up → Down → Both
    fn cycle_flatten_direction(&mut self) {
        let current = self
            .get_terrain_property("brush_flatten_direction")
            .to::<i32>();
        let next = (current + 1) % 3;
        self.set_terrain_property("brush_flatten_direction", next.to_variant());
    }

    /// Update flatten mode button text to reflect current direction
    fn update_flatten_button_text(&mut self, direction: i32) {
        let text = match direction {
            1 => "Flatten: Up (F)",
            2 => "Flatten: Down (F)",
            _ => "Flatten (F)",
        };
        if let Some(ref mut btn) = self.flatten_mode_button {
            btn.set_text(text);
        }
    }

    /// Enable/disable sliders based on which are applicable for the current mode
    fn update_slider_states(&mut self, mode: i32) {
        // Strength: all except Texture (mode 1)
        let has_strength = mode != 1;
        // Step size: Plateau only (mode 3)
        let has_step_size = mode == 3;
        // Feather: all except Texture (mode 1)
        let has_feather = mode != 1;

        if let Some(ref mut s) = self.strength_slider {
            s.set_editable(has_strength);
        }
        // size_slider is always editable
        if let Some(ref mut s) = self.step_size_slider {
            s.set_editable(has_step_size);
        }
        if let Some(ref mut s) = self.feather_slider {
            s.set_editable(has_feather);
        }
    }

    /// Sync UI state from terrain node (called when selecting terrain)
    fn sync_ui_from_terrain(&mut self) {
        // Sync brush enabled
        let brush_enabled = self.get_terrain_property("brush_enabled").to::<bool>();
        if let Some(ref mut btn) = self.brush_toggle_button {
            btn.set_pressed(brush_enabled);
        }

        // Sync brush mode (0=Elevation, 1=Texture, 2=Flatten, 3=Plateau, 4=Smooth)
        let brush_mode = self.get_terrain_property("brush_mode").to::<i32>();
        self.update_mode_buttons(brush_mode);

        // Sync strength
        let strength = self.get_terrain_property("brush_strength").to::<f32>();
        if let Some(ref mut slider) = self.strength_slider {
            slider.set_value_no_signal(strength as f64);
        }

        // Sync brush size
        let brush_size = self.get_terrain_property("brush_size").to::<f32>();
        self.update_size_slider(brush_size);

        // Sync step size
        let step_size = self.get_terrain_property("brush_step_size").to::<f32>();
        if let Some(ref mut slider) = self.step_size_slider {
            slider.set_value_no_signal(step_size as f64);
        }

        // Sync feather
        let feather = self.get_terrain_property("brush_feather").to::<f32>();
        if let Some(ref mut slider) = self.feather_slider {
            slider.set_value_no_signal(feather as f64);
        }

        // Sync selected texture
        let selected_tex = self
            .get_terrain_property("selected_texture_index")
            .to::<i32>();
        self.update_texture_buttons(selected_tex.max(0) as usize);

        // Sync flatten direction into flatten button text
        let flatten_dir = self
            .get_terrain_property("brush_flatten_direction")
            .to::<i32>();
        self.update_flatten_button_text(flatten_dir);

        // Update slider editability per current mode
        self.update_slider_states(brush_mode);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Brush Input Helpers
    // ═══════════════════════════════════════════════════════════════════════

    fn is_brush_enabled(&self) -> bool {
        self.get_terrain_property("brush_enabled").to::<bool>()
    }

    fn is_brush_active(&self) -> bool {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                let mut terrain_clone = terrain.clone();
                if terrain_clone.has_method("is_brush_active") {
                    return terrain_clone.call("is_brush_active", &[]).to::<bool>();
                }
            }
        }
        false
    }

    fn get_brush_phase(&self) -> i32 {
        if let Some(ref terrain) = self.current_terrain {
            if terrain.is_instance_valid() {
                let mut terrain_clone = terrain.clone();
                if terrain_clone.has_method("get_brush_phase") {
                    return terrain_clone.call("get_brush_phase", &[]).to::<i32>();
                }
            }
        }
        0
    }

    fn brush_begin(&mut self, world_pos: Vector3) {
        self.call_terrain_method_with_args("brush_begin", &[world_pos.to_variant()]);
    }

    fn brush_continue(&mut self, world_pos: Vector3) {
        self.call_terrain_method_with_args("brush_continue", &[world_pos.to_variant()]);
    }

    fn brush_end(&mut self, screen_y: f32) -> i32 {
        let result = self.call_terrain_method_with_args("brush_end", &[screen_y.to_variant()]);
        result.to::<i32>()
    }

    fn brush_adjust_height(&mut self, screen_y: f32) {
        self.call_terrain_method_with_args("brush_adjust_height", &[screen_y.to_variant()]);
    }

    fn brush_adjust_curvature(&mut self, screen_y: f32) {
        self.call_terrain_method_with_args("brush_adjust_curvature", &[screen_y.to_variant()]);
    }

    fn brush_cancel(&mut self) {
        self.call_terrain_method("brush_cancel");
    }

    /// Raycast from camera through screen position to find terrain hit point
    fn raycast_terrain(&self, camera: &Gd<Camera3D>, screen_pos: Vector2) -> Option<Vector3> {
        let Some(ref terrain) = self.current_terrain else {
            return None;
        };

        if !terrain.is_instance_valid() {
            return None;
        }

        let ray_origin = camera.project_ray_origin(screen_pos);
        let ray_direction = camera.project_ray_normal(screen_pos);
        let ray_end = ray_origin + ray_direction * 10000.0;

        let Some(mut world_3d) = camera.get_world_3d() else {
            return None;
        };

        let Some(mut space_state) = world_3d.get_direct_space_state() else {
            return None;
        };

        let mut query = PhysicsRayQueryParameters3D::create(ray_origin, ray_end).unwrap();
        query.set_collide_with_areas(false);
        query.set_collide_with_bodies(true);

        let result = space_state.intersect_ray(&query);

        if result.is_empty() {
            return self.raycast_horizontal_plane(camera, screen_pos);
        }

        if let Some(position) = result.get("position") {
            return Some(position.to::<Vector3>());
        }

        None
    }

    /// Fallback raycast against a horizontal plane at terrain floor level
    fn raycast_horizontal_plane(
        &self,
        camera: &Gd<Camera3D>,
        screen_pos: Vector2,
    ) -> Option<Vector3> {
        let floor_y = self.get_terrain_property("terrain_floor_y").to::<f32>();

        let ray_origin = camera.project_ray_origin(screen_pos);
        let ray_direction = camera.project_ray_normal(screen_pos);

        if ray_direction.y.abs() < 0.0001 {
            return None;
        }

        let t = (floor_y - ray_origin.y) / ray_direction.y;

        if t < 0.0 {
            return None;
        }

        let hit_point = ray_origin + ray_direction * t;
        Some(hit_point)
    }
}
