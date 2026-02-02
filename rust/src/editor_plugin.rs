use godot::classes::editor_plugin::AfterGuiInput;
use godot::classes::editor_plugin::CustomControlContainer;
use godot::classes::{
    Button, Camera3D, EditorPlugin, IEditorPlugin, InputEvent, InputEventKey, MarginContainer,
    VBoxContainer,
};
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
    #[init(val = None)]
    test_floor_button: Option<Gd<Button>>,
    #[init(val = None)]
    tower_button: Option<Gd<Button>>,
    #[init(val = None)]
    clear_button: Option<Gd<Button>>,
    #[init(val = false)]
    is_modifying: bool,
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
        toolbar.add_theme_constant_override("separation", 8);

        // Create Test Floor button
        let mut test_floor_button = Button::new_alloc();
        test_floor_button.set_text("Test Floor (T)");
        test_floor_button.set_custom_minimum_size(Vector2::new(120.0, 30.0));

        // Create Tower button
        let mut tower_button = Button::new_alloc();
        tower_button.set_text("Test Tower (W)");
        tower_button.set_custom_minimum_size(Vector2::new(120.0, 30.0));

        // Create Clear button
        let mut clear_button = Button::new_alloc();
        clear_button.set_text("Clear All (C)");
        clear_button.set_custom_minimum_size(Vector2::new(120.0, 30.0));

        // Add buttons to VBoxContainer
        toolbar.add_child(&test_floor_button);
        toolbar.add_child(&tower_button);
        toolbar.add_child(&clear_button);

        // Add VBoxContainer to MarginContainer
        margin_container.add_child(&toolbar);

        // Connect button signals
        let plugin_ref = self.to_gd();
        test_floor_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_test_floor_pressed"),
        );
        tower_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_tower_pressed"),
        );
        clear_button.connect(
            "pressed",
            &Callable::from_object_method(&plugin_ref, "on_clear_pressed"),
        );

        // Add MarginContainer to the spatial editor side left
        self.base_mut().add_control_to_container(
            CustomControlContainer::SPATIAL_EDITOR_SIDE_LEFT,
            &margin_container,
        );

        self.margin_container = Some(margin_container);
        self.toolbar = Some(toolbar);
        self.test_floor_button = Some(test_floor_button);
        self.tower_button = Some(tower_button);
        self.clear_button = Some(clear_button);
        godot_print!("PixyTerrainPlugin: toolbar added to SPATIAL_EDITOR_SIDE_LEFT");
    }

    fn exit_tree(&mut self) {
        // Clean up child refs
        self.test_floor_button = None;
        self.tower_button = None;
        self.clear_button = None;
        self.toolbar = None;

        // Remove and free the margin container (and all children)
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
        godot_print!("PixyTerrainPlugin: handles called for class: {}", class_name);
        class_name == "PixyTerrain"
    }

    fn edit(&mut self, object: Option<Gd<Object>>) {
        godot_print!(
            "PixyTerrainPlugin: edit called, object is_some: {}",
            object.is_some()
        );
        if let Some(obj) = object {
            if let Ok(node) = obj.try_cast::<Node>() {
                self.current_terrain = Some(node);
                self.set_ui_visible(true);
                return;
            }
        }
        self.set_ui_visible(false)
    }

    fn make_visible(&mut self, visible: bool) {
        // Guard against false-positive hides during child modifications (bug #40166)
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
        _camera: Option<Gd<Camera3D>>,
        event: Option<Gd<InputEvent>>,
    ) -> i32 {
        let Some(event) = event else {
            return AfterGuiInput::PASS.ord();
        };

        if let Ok(key_event) = event.try_cast::<InputEventKey>() {
            if key_event.is_pressed() && !key_event.is_echo() {
                match key_event.get_keycode() {
                    godot::global::Key::T => {
                        self.do_create_test_floor();
                        return AfterGuiInput::STOP.ord();
                    }
                    godot::global::Key::W => {
                        self.do_create_test_tower();
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

        AfterGuiInput::PASS.ord()
    }
}

#[godot_api]
impl PixyTerrainPlugin {
    #[func]
    fn on_test_floor_pressed(&mut self) {
        godot_print!("PixyTerrainPlugin: Test Floor button pressed");
        self.do_create_test_floor();
    }

    #[func]
    fn on_tower_pressed(&mut self) {
        godot_print!("PixyTerrainPlugin: Tower button pressed");
        self.do_create_test_tower();
    }

    #[func]
    fn on_clear_pressed(&mut self) {
        godot_print!("PixyTerrainPlugin: Clear button pressed");
        self.do_clear();
    }
}

impl PixyTerrainPlugin {
    fn set_ui_visible(&mut self, visible: bool) {
        godot_print!("PixyTerrainPlugin: set_ui_visible({})", visible);
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

    fn do_create_test_floor(&mut self) {
        self.call_terrain_method("create_test_floor");
    }

    fn do_create_test_tower(&mut self) {
        self.call_terrain_method("create_test_tower");
    }

    fn do_clear(&mut self) {
        self.call_terrain_method("clear_all");
    }
}
