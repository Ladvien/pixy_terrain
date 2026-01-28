# GridMap Workflow for Medieval Village Assets

## Key Concepts

### Cell Center Settings Are Per-GridMap Node
- `cell_center_x`, `cell_center_y`, `cell_center_z` are properties of each GridMap node
- Each GridMap has independent settings
- **If changing one affects all**: You're multi-selecting nodes in the editor - click a single node

### Per-Item Offsets via MeshLibrary
For positioning meshes within grid cells (like centering doors), use `MeshLibrary.set_item_mesh_transform()`:

```gdscript
const CATEGORY_OFFSETS := {
    "doors": Vector3(0.0, 0.0, -1.0),  # Z offset to center in doorframe
}

func _add_mesh_to_library(...):
    var final_transform := transform
    if CATEGORY_OFFSETS.has(category):
        final_transform.origin += CATEGORY_OFFSETS[category]
    mesh_library.set_item_mesh_transform(item_id, final_transform)
```

### Pivot Placement Conventions

| Placement | GridMap Settings | Best For |
|-----------|------------------|----------|
| Corner origin | center_x/y/z = OFF | Modular building pieces (walls, corners) |
| Bottom-center | center_x = ON, center_y = OFF, center_z = ON | Floor tiles, props |

### Layered GridMap Pattern
Multiple sibling GridMap nodes, each with its own MeshLibrary:

```
OldHouse (Node3D)
├── GridMap_Floors ─ floors.tres
├── GridMap_Walls ─ walls.tres
├── GridMap_WallsFront ─ walls_front.tres
├── GridMap_WallsBack ─ walls_back.tres
├── GridMap_Corners ─ corners.tres
├── GridMap_Doors ─ doors.tres
├── GridMap_Doorframes ─ doorframes.tres
├── GridMap_Windows ─ windows.tres
├── GridMap_Shutters ─ shutters.tres
├── GridMap_Roofs ─ roofs.tres
├── GridMap_Stairs ─ stairs.tres
├── GridMap_Overhangs ─ overhangs.tres
├── GridMap_Props ─ props.tres
├── GridMap_Balconies ─ balconies.tres
└── GridMap_Holecovers ─ holecovers.tres
```

## Common Issues

| Problem | Cause | Fix |
|---------|-------|-----|
| Tiles don't snap | Mesh origin misaligned | Fix origin in Blender or use `set_item_mesh_transform()` |
| Rotation only 90° | GridMap uses 24 orientations (0-23) | Create pre-rotated variants in Blender |
| Seams between tiles | MSAA or texture filtering | Add 4px texture padding, or make tiles 1.001 units |
| All GridMaps change at once | Multi-selecting nodes | Click single node in scene tree |

## Quaternius Official Setup (Reference Only)

The asset creator does **NOT** use GridMap. Their approach:
- Direct GLTF scene instancing (PackedScenes)
- Manual positioning via transforms
- Import script swaps materials with stored `.tres` files

Location: `/Users/ladvien/Downloads/Medieval Village MegaKit[Source]/Engine Projects/Godot/`

Their import script (`quaternius_import_script.gd`):
```gdscript
# Replaces imported materials with stored .tres materials by name match
const FOLDER = "res://addons/quaternius/materials/"

func _post_import(scene):
    iterate(scene)
    return scene

func iterate(node):
    if node is MeshInstance3D:
        var mesh = node.mesh
        for i in range(mesh.get_surface_count()):
            var material = mesh.surface_get_material(i)
            var mat_name = material.resource_name
            var material_path = FOLDER + mat_name + ".tres"
            if ResourceLoader.exists(material_path):
                mesh.surface_set_material(i, load(material_path))
    for child in node.get_children():
        iterate(child)
```

## Sources
- https://docs.godotengine.org/en/stable/classes/class_gridmap.html
- https://docs.godotengine.org/en/stable/classes/class_meshlibrary.html
- https://forum.godotengine.org/t/gridmap-item-placement-different-from-meshlibrary/120039
- https://godotforums.org/d/26192-offset-to-gridmap-object-in-meshlibrary-through-code
