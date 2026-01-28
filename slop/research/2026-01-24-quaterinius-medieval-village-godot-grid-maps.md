# Configuring Quaternius Medieval Village assets for Godot 4 GridMap

Quaternius Medieval Village MegaKit assets use a **1-meter modular grid with corner-origin pivots**, requiring GridMap cell_size of `Vector3(1, 1, 1)` with all cell_center properties set to `false`. The Source version ($14.99) provides a pre-configured Godot 4.3+ project with optimized collisions, while the free Standard version requires manual setup using these parameters. This comprehensive guide covers the complete workflow from Blender preprocessing through MeshLibrary generation to common alignment troubleshooting.

## Quaternius assets use 1-meter corner-origin grid design

The Medieval Village MegaKit contains **304 modular models** specifically designed to "fit perfectly in a grid." While official documentation doesn't explicitly state dimensions, community implementations—particularly Malcolm Nixon's Godot conversions of other Quaternius packs—confirm the **1-meter unit standard** across Quaternius modular kits.

For proper GridMap integration, configure these essential settings:

```gdscript
# GridMap Configuration
gridmap.cell_size = Vector3(1, 1, 1)  # 1-meter cells
gridmap.cell_center_x = false         # Corner origin on X
gridmap.cell_center_y = false         # Corner origin on Y  
gridmap.cell_center_z = false         # Corner origin on Z
```

The corner-origin design means mesh pivots sit at one corner rather than the geometric center. This allows modular pieces to align edge-to-edge when placed on adjacent grid cells—crucial for walls, floors, and building segments to connect seamlessly.

## Source version versus Standard: what matters for GridMap

Quaternius offers three tiers with significant differences for GridMap workflows:

| Version | Price | Models | GridMap Features |
|---------|-------|--------|------------------|
| **Standard** | Free | ~176 | FBX, OBJ, glTF only; requires manual collision setup |
| **Pro** | $9.99 | 300+ | All formats, additional models |
| **Source** | $14.99 | 300+ | Pre-configured Godot 4.3+ project, custom optimized collisions, .blend files, wear color shaders |

The Source version provides the smoothest path to GridMap implementation. It includes **pre-configured collision shapes** for each model—a substantial time-saver since manually creating collision shapes for 300+ pieces is tedious. The included Godot project also features custom shaders enabling wear color customization without texture modifications.

For budget-conscious developers, the free Standard version works perfectly with manual setup. The workflow simply requires additional steps: importing glTF files, generating collisions, and configuring the MeshLibrary yourself using the specifications outlined in this guide.

## MeshLibrary generation workflow from glTF files

Converting Quaternius .glb/.gltf files into a usable MeshLibrary follows a structured process in Godot 4.x:

**Step 1: Scene organization.** Create a new scene with `Node3D` as root. Import each glTF model and add them as **direct first-level children**—only first-level children become MeshLibrary items. Name each child descriptively as these names appear in your tile palette.

**Step 2: Collision setup.** For each `MeshInstance3D`, add collision using one of these approaches:
- Select the mesh, then use **Mesh menu → Create Trimesh Static Body** for accurate complex collisions
- Use **Mesh menu → Create Single Convex Collision Sibling** for simpler shapes with better physics performance
- For floor and wall tiles, manually add `StaticBody3D` with `BoxShape3D`—dramatically faster physics than trimesh

**Step 3: Export MeshLibrary.** Navigate to **Scene → Convert To → MeshLibrary**. Enable "Apply MeshInstance Transforms" to bake any positioning into the library. Save as a `.meshlib` or `.tres` resource.

An alternative workflow uses the **godot_gltf2meshlib** plugin, which automates import directly from glTF. Add `-col` or `-colonly` suffixes to mesh names in Blender to automatically designate collision shapes during import.

## Cell_center settings control alignment behavior

The `cell_center_x/y/z` properties determine whether meshes center within their cell or align to the cell's corner/edge:

| Setting | TRUE behavior | FALSE behavior |
|---------|--------------|----------------|
| `cell_center_x` | Mesh centered horizontally on X | Origin aligns to cell corner |
| `cell_center_y` | Mesh centered vertically | Origin aligns to cell bottom |
| `cell_center_z` | Mesh centered horizontally on Z | Origin aligns to cell corner |

For Quaternius corner-origin assets, **all three should be false**. Using center settings with corner-origin meshes causes half-cell offset misalignment—floors hover, walls gap, and modular connections break.

**Important caveat**: Godot 4.x has a known bug (GitHub #77196) where mouse cursor and selection alignment behave incorrectly in the editor when cell_center settings differ from default. This affects only the visual editor feedback—runtime placement remains accurate. Work around this by placing tiles with knowledge that visual grid preview may be offset.

## Blender preprocessing: origins and coordinate conversion

Godot's glTF importer handles Y-up to Z-up coordinate conversion automatically, but **forward direction requires attention**. glTF defines +Z as forward while Godot uses -Z forward, causing models to appear rotated 180 degrees. For character-facing assets, rotate the model to face +Y in Blender before export. For architectural GridMap tiles, this rarely matters since buildings are symmetric or orientation-agnostic.

**Origin placement for modular tiles:**
1. Select object and enter Edit Mode (Tab)
2. Select the bottom corner vertex you want as origin
3. Press Shift+S → "Cursor to Selected"
4. Exit Edit Mode, then Object → Set Origin → Origin to 3D Cursor

**Critical pre-export checklist:**
- Set Blender scene units to **Meters** (Scene Properties → Units)
- **Apply All Transforms** (Ctrl+A → All Transforms)—scale must be (1, 1, 1)
- Verify mesh dimensions match intended cell_size
- Name collision meshes with `-colonly` suffix for automatic recognition

Export using **File → Export → glTF 2.0 (.glb)** with "Apply Modifiers" and "Selected Objects" enabled. The binary .glb format keeps textures embedded in a single portable file.

## Common alignment issues and proven solutions

**Meshes floating or sinking**: This classic GridMap problem stems from mismatch between mesh origin and cell_center_y setting. With Quaternius corner-origin assets and `cell_center_y = false`, mesh origins should sit at the bottom corner. If assets were modeled with center origins, they'll hover at half-height.

**Multi-cell meshes offset by half-cell**: A 2×1×1 staircase in a 1×1×1 grid offsets by 0.5m because GridMap centers based on cell_size regardless of mesh size. Solutions include designing all meshes as single-cell units, adjusting cell_size to match your largest common module, or accepting the offset and compensating with placement coordinates.

**"Bumpy" character movement over floors**: Adjacent trimesh collision shapes create micro-gaps causing physics jitter. Use convex or box collision shapes instead of trimesh for floor tiles, or slightly expand collision boundaries to overlap neighboring tiles.

**Rotated meshes misalign**: When mesh origin isn't at the rotation pivot point, 90-degree rotations shift the tile position. Ensure origins sit at the corner you want to rotate around, and disable centering on axes where corner-snapping matters.

## Community resources remain sparse but useful

Direct Quaternius + GridMap tutorials are limited, but several resources provide applicable guidance:

**Malcolm Nixon's GitHub repos** (github.com/Malcolmnixon/Quaternius-Modular-Scifi-Pack) offer MeshLibrary-ready conversions of Quaternius Sci-Fi assets for Godot 3.5. While not Godot 4 compatible directly, they demonstrate the correct GridMap configuration approach and confirm the 1m/corner-origin conventions.

**HauntedWindow's blog post** "Choosing A 3D Level Design Workflow For Godot" compares GridMap against CSG, Blender, and Trenchbroom/Qodot approaches, noting GridMap's efficient single-object rendering but constraint to tileset-based design.

**Godot Forum discussions** (forum.godotengine.org) include threads on rendering modular Quaternius Sci-Fi assets, comparing GridMap versus snap-to-grid versus MultiMesh approaches for interior environments.

**Key limitation to note**: GridMaps don't support occlusion culling. Malcolm Nixon's repos include occlusion polygons on individual node versions, but the MeshLibrary/GridMap approach renders everything visible. For large interior environments, this may impact performance compared to individually-placed scene instances.

## Collision generation strategy depends on use case

For GridMap tiles, collision approach significantly impacts both workflow time and runtime performance:

| Method | Performance | Accuracy | Best for |
|--------|-------------|----------|----------|
| BoxShape3D (manual) | Fastest | Approximate | Floors, simple walls |
| Single Convex | Fast | Hull approximation | Solid objects |
| Multiple Convex | Medium | Good concave handling | Hollow structures |
| Trimesh | Slowest | Exact mesh | Complex props, final resort |

The Source version's pre-optimized collisions likely use simplified shapes rather than direct trimesh generation—a significant workflow advantage. For manual setup, invest time creating simple box collisions for repetitive floor and wall tiles while reserving trimesh for complex decorative pieces that see minimal physics interaction.

## Conclusion

Successful Quaternius + GridMap integration requires understanding three core principles: **1-meter corner-origin grid design** drives all configuration choices, **cell_center settings must all be false** for proper alignment, and **Source version collisions save substantial setup time**. The workflow from Blender through MeshLibrary generation follows standard Godot practices with attention to origin placement and coordinate conventions. While community resources specifically covering this combination remain limited, the modular design philosophy of Quaternius assets maps naturally to GridMap's tile-based approach—the main challenges involve initial configuration rather than fundamental incompatibility.
