# Building Godot 4.6 tool UI with Rust and GDExt

Creating editor plugins and tool UI in Godot 4.6 using Rust requires mastering both Godot's EditorPlugin APIs and gdext's Rust-specific binding patterns. The gdext library (godot-rust) provides **full access to Godot's Control nodes, theme system, and editor extension APIs**, enabling Rust developers to build everything from simple inspector customizations to complete main screen editor plugins. While the Rust bindings add some complexity around borrow checking and signal connections, the type-safe signal system and `OnReady<Gd<T>>` pattern significantly improve developer experience compared to earlier godot-rust iterations.

This guide covers the complete toolchain: EditorPlugin lifecycle and dock management, gdext-specific UI patterns with working code, advanced components like Tree and GraphEdit, and UX best practices that align with Godot's native editor styling.

## EditorPlugin architecture and dock registration

The **EditorPlugin class** serves as the foundation for all Godot editor extensions. In Godot 4.x, plugins must use the `@tool` annotation (or `#[class(tool)]` in gdext) and extend EditorPlugin. The plugin lifecycle centers on `_enter_tree()` for initialization and `_exit_tree()` for cleanup—every control added must be explicitly removed and freed.

Adding custom docks uses `add_control_to_dock()` with a **DockSlot enum** specifying position. The eight dock slots map to the editor's panel layout: `DOCK_SLOT_LEFT_UL` through `DOCK_SLOT_RIGHT_BR`, with slots 2-3 (left side) containing Scene/FileSystem and slots 4-5 (right side) containing Inspector/Node. Position persists across editor sessions.

```rust
use godot::prelude::*;
use godot::classes::{EditorPlugin, IEditorPlugin, Control, Button};

#[derive(GodotClass)]
#[class(tool, editor_plugin, init, base=EditorPlugin)]
pub struct MyToolPlugin {
    base: Base<EditorPlugin>,
    dock: Option<Gd<Control>>,
}

#[godot_api]
impl IEditorPlugin for MyToolPlugin {
    fn enter_tree(&mut self) {
        let dock = Control::new_alloc();
        // Configure dock contents here
        self.base_mut().add_control_to_dock(
            EditorPlugin::DOCK_SLOT_LEFT_UL, 
            dock.clone().upcast()
        );
        self.dock = Some(dock);
    }
    
    fn exit_tree(&mut self) {
        if let Some(dock) = self.dock.take() {
            self.base_mut().remove_control_from_docks(dock.clone().upcast());
            dock.free();
        }
    }
}
```

For **bottom panels** (alongside Output, Debugger, Animation), use `add_control_to_bottom_panel()` which returns a Button reference for programmatic show/hide control. The `add_control_to_container()` method provides finer placement via CustomControlContainer enum—options include `CONTAINER_TOOLBAR` for main toolbar, `CONTAINER_CANVAS_EDITOR_MENU` for 2D editor, and `CONTAINER_INSPECTOR_BOTTOM` for inspector footer.

**Main screen plugins** create new workspace tabs beside 2D, 3D, and Script. Implementation requires four virtual methods: `_has_main_screen()` returning true, `_get_plugin_name()` for the tab label, `_get_plugin_icon()` for the toolbar icon (16x16 white with transparency works best), and `_make_visible()` to toggle your panel's visibility when switching workspaces. The main panel itself attaches to `EditorInterface.get_editor_main_screen()`.

## Custom inspector plugins transform property editing

**EditorInspectorPlugin** enables custom property editors that replace or augment the inspector's default controls. The workflow involves overriding `_can_handle()` to specify which object types the plugin handles, then using `_parse_property()` to intercept individual properties.

The `_parse_property()` method receives comprehensive metadata: the edited object, property type (from Variant.Type), property name, hint type, hint string, and usage flags. Returning `true` removes the default editor for that property. Custom editors must extend **EditorProperty** and implement value synchronization via `get_edited_object()`, `get_edited_property()`, and `emit_changed()`.

```rust
#[derive(GodotClass)]
#[class(tool, init, base=EditorInspectorPlugin)]
struct CustomInspector {
    base: Base<EditorInspectorPlugin>,
}

#[godot_api]
impl IEditorInspectorPlugin for CustomInspector {
    fn can_handle(&self, object: Gd<Object>) -> bool {
        object.is_class("MyCustomResource".into())
    }
    
    fn parse_property(
        &mut self, 
        _object: Gd<Object>, 
        type_: VariantType,
        name: GString,
        _hint_type: PropertyHint,
        _hint_string: GString,
        _usage_flags: PropertyUsageFlags,
        _wide: bool
    ) -> bool {
        if type_ == VariantType::INT && name == "special_value".into() {
            let editor = CustomPropertyEditor::new_alloc();
            self.base_mut().add_property_editor(name, editor.upcast());
            return true; // Hide default editor
        }
        false
    }
}
```

Register inspector plugins in `_enter_tree()` via `add_inspector_plugin()` and remove them in `_exit_tree()`. The `_parse_begin()` and `_parse_end()` callbacks allow adding controls at the top or bottom of the inspector independent of specific properties—useful for action buttons or status displays.

## gdext signal connections use a typed builder pattern

The gdext library provides a **type-safe signal system** that prevents common runtime errors. Signals declared with `#[signal]` in a `#[godot_api]` impl block generate compile-time checked connection methods accessible via `.signals()` on the node.

```rust
#[derive(GodotClass)]
#[class(init, base=Control)]
struct LoginForm {
    base: Base<Control>,
    #[init(node = "UsernameInput")]
    username: OnReady<Gd<LineEdit>>,
    #[init(node = "SubmitButton")]
    submit_btn: OnReady<Gd<Button>>,
}

#[godot_api]
impl LoginForm {
    #[signal]
    fn login_submitted(username: GString);
}

#[godot_api]
impl IControl for LoginForm {
    fn ready(&mut self) {
        // Type-safe connection to built-in Button signal
        self.submit_btn.signals().pressed().connect_self(|this| {
            let username = this.username.get_text();
            this.signals().login_submitted().emit(username);
        });
        
        // Connection with flags (deferred, one-shot)
        self.username.signals().text_changed()
            .builder()
            .flags(ConnectFlags::DEFERRED)
            .connect(|new_text| {
                godot_print!("Username changed: {}", new_text);
            });
    }
}
```

The `connect_self()` variant provides access to `self` within the closure, while plain `connect()` takes standalone closures. For connecting to signals on other objects, use `.connect_self_mut()` when mutation is needed. **Critical limitation**: never access `to_gd()` or `signals()` in the `init()` constructor—this will panic. All signal connections belong in `ready()`.

For dynamic or cross-GDExtension scenarios requiring runtime flexibility, the untyped `Callable::from_object_method()` pattern remains available:

```rust
button.connect(
    "pressed".into(),
    Callable::from_object_method(&self.base(), "on_button_click"),
);
```

## OnReady eliminates boilerplate for child node references

The `OnReady<Gd<T>>` wrapper combined with `#[init(node = "path")]` attribute provides **automatic node resolution** before `ready()` executes—equivalent to GDScript's `@onready` but with compile-time type checking.

```rust
#[derive(GodotClass)]
#[class(init, base=Node)]
struct GameHUD {
    base: Base<Node>,
    
    #[init(node = "HealthBar")]
    health_bar: OnReady<Gd<ProgressBar>>,
    
    #[init(node = "ScoreLabel")]
    score_label: OnReady<Gd<Label>>,
    
    #[init(load = "res://assets/ui_font.tres")]
    font: OnReady<Gd<Font>>,
    
    #[init(val = OnReady::manual())]
    computed_data: OnReady<i32>,
}

#[godot_api]
impl INode for GameHUD {
    fn ready(&mut self) {
        // All OnReady fields except manual() are initialized here
        self.health_bar.set_value(100.0);
        self.computed_data.init(calculate_something());
    }
}
```

OnReady requires the class to have a `Base<T>` field where T inherits Node. The `#[init(load = "path")]` variant preloads resources. For values computed at runtime, `OnReady::manual()` defers initialization until you call `.init()` explicitly. **Gotcha**: OnReady cannot be combined with `#[export]`—use `#[var]` instead if needed.

## Custom Control classes implement IControl for full UI behavior

Creating custom UI widgets requires implementing the **IControl trait**, which exposes all Control virtual methods including input handling, drawing, and layout.

```rust
#[derive(GodotClass)]
#[class(base=Control)]
struct ColorPicker {
    base: Base<Control>,
    selected_color: Color,
    is_pressed: bool,
}

#[godot_api]
impl IControl for ColorPicker {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            selected_color: Color::WHITE,
            is_pressed: false,
        }
    }
    
    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(mouse) = event.try_cast::<InputEventMouseButton>() {
            if mouse.get_button_index() == MouseButton::LEFT {
                self.is_pressed = mouse.is_pressed();
                self.base_mut().queue_redraw();
            }
        }
    }
    
    fn draw(&mut self) {
        let size = self.base().get_size();
        let color = if self.is_pressed {
            self.selected_color.darkened(0.2)
        } else {
            self.selected_color
        };
        self.base_mut().draw_rect(Rect2::new(Vector2::ZERO, size), color);
    }
    
    fn get_minimum_size(&self) -> Vector2 {
        Vector2::new(64.0, 64.0)
    }
}
```

Key IControl methods include `gui_input()` for mouse/keyboard events (call `accept_event()` to consume), `draw()` for custom 2D rendering (triggered by `queue_redraw()`), `get_minimum_size()` for layout constraints, and `has_point()` for hit testing. Drag-and-drop uses `get_drag_data()`, `can_drop_data()`, and `drop_data()`.

## Tree and ItemList display hierarchical and list data

The **Tree control** builds hierarchical displays programmatically via TreeItem objects. Unlike scene-based UI, Tree contents are constructed entirely in code. The first `create_item()` call with null parent becomes the root; subsequent calls create children.

```gdscript
# GDScript example (pattern applies to Rust via method calls)
func populate_tree():
    tree.clear()
    var root = tree.create_item()
    root.set_text(0, "Root")
    
    for category in data.categories:
        var cat_item = tree.create_item(root)
        cat_item.set_text(0, category.name)
        cat_item.set_metadata(0, category)  # Store arbitrary data
        cat_item.set_icon(0, category_icon)
        
        for entry in category.entries:
            var entry_item = tree.create_item(cat_item)
            entry_item.set_text(0, entry.name)
            entry_item.set_cell_mode(1, TreeItem.CELL_MODE_CHECK)
            entry_item.set_checked(1, entry.enabled)
```

TreeItem supports multiple columns with different **cell modes**: `CELL_MODE_STRING` for text, `CELL_MODE_CHECK` for checkboxes, `CELL_MODE_RANGE` for sliders, and `CELL_MODE_CUSTOM` for custom drawing. Key signals include `item_selected`, `item_activated` (double-click), `button_clicked`, and `item_edited`. Access edited items via `get_edited()` and `get_edited_column()` in the signal handler.

**ItemList** provides simpler list displays with icon/text items. Icon modes (`ICON_MODE_TOP` or `ICON_MODE_LEFT`) control layout style. Use `set_item_metadata()` to associate arbitrary data with items and retrieve it in selection handlers.

## GraphEdit enables visual node-based editors

For visual scripting or shader graph interfaces, **GraphEdit** manages a canvas of **GraphNode** elements with connection lines between ports. GraphEdit handles zooming, panning, and connection routing; your code responds to connection requests.

```gdscript
class_name NodeGraph extends GraphEdit

func _ready():
    connection_request.connect(_on_connection_request)
    disconnection_request.connect(_on_disconnection_request)
    delete_nodes_request.connect(_on_delete_request)

func _on_connection_request(from_node: StringName, from_port: int, 
                            to_node: StringName, to_port: int):
    # Prevent duplicate connections to same input
    for conn in get_connection_list():
        if conn.to_node == to_node and conn.to_port == to_port:
            return
    connect_node(from_node, from_port, to_node, to_port)

func add_graph_node(title: String) -> GraphNode:
    var node = GraphNode.new()
    node.title = title
    node.set_slot(0, true, 0, Color.RED, true, 0, Color.BLUE)
    add_child(node)
    return node
```

In Godot 4.x, GraphNode inherits from **GraphElement** (new base class). Slots are created automatically for each Control child—`set_slot()` enables/disables input (left) and output (right) ports per slot index. Port types enable type-checking connections via color and integer type identifiers. The `connection_to_empty` signal fires when users drag to empty space, useful for showing "add node" menus.

**Note**: GraphEdit and GraphNode are marked experimental in Godot 4.x—APIs may change in future versions.

## Theme inheritance keeps tools visually consistent

Editor plugins should **inherit the editor theme** rather than defining custom styles. Access the theme via `EditorInterface.get_base_control().get_theme()` and use theme query methods:

```rust
fn setup_styled_button(&mut self) {
    let base = EditorInterface::singleton().get_base_control();
    let theme = base.get_theme().unwrap();
    
    let mut button = Button::new_alloc();
    let add_icon = base.get_theme_icon("Add".into(), "EditorIcons".into());
    button.set_icon(add_icon);
    
    // Buttons added as children automatically inherit editor theme
    self.base_mut().add_child(button.clone().upcast());
}
```

Built-in Controls added to the editor automatically inherit styling. For custom drawing, query **StyleBox**, **Color**, and **Font** values using `get_theme_stylebox()`, `get_theme_color()`, and `get_theme_font()` with the appropriate theme type (e.g., "Button", "Panel", "EditorIcons").

StyleBoxFlat provides the most flexibility for custom panels: `bg_color`, `border_width_*` (per-side), `corner_radius_*`, `shadow_color`, and `shadow_size` cover most styling needs without textures.

## EditorUndoRedoManager enables reversible tool actions

Every tool operation modifying scene data should integrate with **EditorUndoRedoManager** for proper undo/redo support. Access it via `get_undo_redo()` from EditorPlugin or `EditorInterface.get_editor_undo_redo()`.

```rust
fn rename_node(&mut self, node: Gd<Node>, new_name: GString) {
    let undo_redo = self.base().get_undo_redo().unwrap();
    let old_name = node.get_name();
    
    undo_redo.create_action("Rename Node".into());
    undo_redo.add_do_property(node.clone().upcast(), "name".into(), new_name.to_variant());
    undo_redo.add_undo_property(node.upcast(), "name".into(), old_name.to_variant());
    undo_redo.commit_action();
}
```

Use **property-based** operations (`add_do_property`/`add_undo_property`) when possible—they're simpler than method-based. For newly created nodes, call `add_do_reference()` to prevent garbage collection before commit. Action names should use Title Case. Merge modes (`MERGE_ENDS`, `MERGE_ALL`) combine consecutive same-named actions for continuous operations like dragging.

## Debug overlays strip cleanly from release builds

In-game debug UI should use conditional instantiation based on `OS.is_debug_build()` or custom feature flags:

```gdscript
# AutoLoad debug overlay (add only in debug)
extends CanvasLayer

var _properties: Array[Dictionary] = []
@onready var container: VBoxContainer = $VBox

func _ready():
    visible = false

func _process(_delta):
    for prop in _properties:
        var value = prop.object.get_indexed(prop.property)
        prop.label.text = "%s: %s" % [prop.property, str(value)]

func track(object: Object, property: String):
    if not OS.is_debug_build():
        return
    var label = Label.new()
    container.add_child(label)
    _properties.append({"object": object, "property": property, "label": label})

func _input(event):
    if event.is_action_pressed("toggle_debug"):
        visible = !visible
```

For Rust, use `#[cfg(debug_assertions)]` or check `OS::is_debug_build()` at runtime. The Engine singleton's `is_editor_hint()` distinguishes editor preview from gameplay.

## Performance considerations for Rust-Godot interop

Every call between Rust and Godot incurs **FFI overhead**. The `bind()` and `bind_mut()` methods on `Gd<T>` perform runtime borrow checking, adding slight cost. For performance-critical UI code:

- Batch property updates rather than setting individually
- Cache frequently-accessed node references in struct fields
- Use `balanced` safeguard level (Cargo feature) for release builds—it removes extra debug checks
- Prefer Godot's built-in Controls over custom `_draw()` implementations when possible

The typed signal system eliminates the string-based lookup overhead of manual signal connections, making it both safer and marginally faster than untyped approaches.

## Essential documentation and community resources

Official Godot documentation provides comprehensive API references for all editor and UI classes. The gdext book and API docs cover Rust-specific patterns including the signal system, OnReady, and class registration.

- **EditorPlugin**: https://docs.godotengine.org/en/stable/classes/class_editorplugin.html
- **Making Plugins Tutorial**: https://docs.godotengine.org/en/stable/tutorials/plugins/editor/making_plugins.html
- **EditorInspectorPlugin**: https://docs.godotengine.org/en/stable/classes/class_editorinspectorplugin.html
- **EditorUndoRedoManager**: https://docs.godotengine.org/en/stable/classes/class_editorundoredomanager.html
- **Control**: https://docs.godotengine.org/en/stable/classes/class_control.html
- **Theme**: https://docs.godotengine.org/en/stable/classes/class_theme.html
- **Tree**: https://docs.godotengine.org/en/stable/classes/class_tree.html
- **GraphEdit**: https://docs.godotengine.org/en/stable/classes/class_graphedit.html
- **gdext GitHub**: https://github.com/godot-rust/gdext
- **gdext Book**: https://godot-rust.github.io/book/
- **gdext API Docs**: https://godot-rust.github.io/docs/gdext/master/godot/
- **Demo Projects**: https://github.com/godot-rust/demo-projects

The godot-rust Discord (~2,400 members) provides community support. The **Editor Theme Explorer** plugin (Asset Library) helps discover available theme items for consistent styling.

## Conclusion

Building tool UI in Godot 4.6 with Rust and gdext is now production-viable, with the **typed signal system** and **OnReady pattern** eliminating much of the boilerplate that plagued earlier godot-rust versions. The key architectural insight is treating editor plugins as first-class Rust code: define your UI hierarchy in `_enter_tree()`, wire up typed signals in `ready()`, and clean up everything in `_exit_tree()`.

The primary friction points remain borrow checker interactions with Godot's signal-driven architecture—solved consistently by using `ConnectFlags::DEFERRED` and keeping signal handlers simple—and the lack of hot reload for Rust code during plugin development (though GDExtension hot reload works for game code in Godot 4.2+).

For complex editor tools, combine Rust's strengths (data processing, type safety, performance-critical algorithms) with GDScript or scene-based UI for rapid iteration on visual layout. The `EditorInterface.get_base_control()` pattern ensures your Rust-built Controls seamlessly inherit Godot's native editor appearance without manual theme work.
