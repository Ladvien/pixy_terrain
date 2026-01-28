# Godot 4 addons for 3D level design and modular asset workflows

The Godot 4 ecosystem offers **over 20 specialized addons** for 3D level design, with the strongest options being **Terrain3D** for landscapes, **ProtonScatter** for procedural placement, **AssetSnap** for modular kit workflows, and several GridMap enhancements. Most are MIT-licensed, free, and compatible with Godot **4.2-4.6**, with full support for imported glTF/GLB assets. For modular asset packs like Quaternius kits, the recommended workflow combines the **GLTF2MeshLib** plugin for automatic MeshLibrary conversion with GridMap for structured placement and ProtonScatter for environmental decoration.

---

## GridMap extensions bring modern editing to tile-based 3D

Godot's built-in GridMap remains the foundation for grid-based 3D level design, but several addons significantly enhance its capabilities.

**GridMap Plus** (github.com/Portponky/gridmap-plus) adds a Minecraft-style first-person editor mode with WASD movement, fly controls, and intuitive placement rules. It supports upward, outward, and random placement modes with mesh orientation controls via Shift+WASD for pitch/yaw. Licensed under Unlicense (public domain) and compatible with **Godot 4.2**, it works with any MeshLibrary including those created from glTF imports.

**EnhancedGridMap** (github.com/DanchieGO/EnhancedGridMap) extends GridMap with **A* pathfinding**, custom cell states, and multi-floor support. Targeted at tactical/strategy games, it provides grid operations like auto-generate, randomize, and fill, plus click-to-move player movement examples. Licensed MIT, compatible with **Godot 4.3-4.4**.

**Tile to Gridmap** (github.com/MatthewKonobri/Godot_Tile_to_Gridmap, Asset Library ID: **3672**) takes a unique approach—build 3D GridMaps using the 2D TileMap editor with autotiling support. Features include procedural terrain generation with chunk-based splitting, a dual-grid placement system, and a T2GProcGenManager for noise-based world generation. Requires **Godot 4.4+**, MIT license. Meshes use bitmask naming conventions (e.g., dirt0-dirt15 for 16 terrain variants).

| Addon | Godot Version | License | Key Feature |
|-------|---------------|---------|-------------|
| GridMap Plus | 4.2 | Unlicense | Minecraft-style editor |
| EnhancedGridMap | 4.3-4.4 | MIT | A* pathfinding, multi-floor |
| Tile to Gridmap | 4.4+ | MIT | 2D TileMap → 3D GridMap |
| AutoGrid | 3.x (needs port) | MIT | Autotiling for GridMap |

---

## Modular building tools streamline kit-based workflows

For placing modular kit pieces like Quaternius medieval and nature megakits, several tools provide library management, snapping, and rapid placement capabilities.

**AssetSnap** (github.com/misoe92/AssetSnap-Godot) is the **best free option** for modular kit workflows. Features include library management with 3D previews, object and plane snapping, a **groups system** for creating reusable combinations of modular pieces (like assembling houses from blocks), drag-path placement, and continuous placement with Alt+Shift. Requires **Godot 4.2+ Mono** (.NET version), MIT license. Supports .FBX, .GLTF, .GLB, and .OBJ formats directly.

**AssetPlacer** (cookiebadger.itch.io/assetplacer) is the **best commercial option** at $17.99. Offers asset libraries with visual previews, custom grid snapping with offset, surface and Terrain3D placement, rotation/flip shortcuts, and detachable UI. Compatible with **Godot 4.0-4.5**, requires Mono. Explicitly designed for modular asset pack workflows.

**ExtraSnaps** (github.com/mharitsnf/ExtraSnaps, MIT license) provides lightweight surface snapping—hold CTRL/CMD+W to snap objects to PhysicsBody3D or CSGShape3D surfaces with optional normal alignment. Compatible with **Godot 4.0-4.2+**, pure GDScript with no dependencies.

**Cyclops Level Builder** (github.com/blackears/cyclopsLevelBuilder, Asset Library available) enables click-drag block creation for rapid prototyping with automatic collision. Best for level blockouts before replacing with final modular pieces. **Godot 4.2+**, MIT license, **1,400+ GitHub stars**.

**FuncGodot** (github.com/func-godot/func_godot_plugin, Asset Library ID: **1631**) imports Quake .map and Valve .vmf files, integrating TrenchBroom's grid-snapping editor into Godot workflows. Point entities can spawn GLB models as display meshes, making it suitable for professional-grade level design pipelines. **Godot 4.2+**, MIT license, pure GDScript.

---

## TileMapLayer3D offers Crocotile-style 2D-to-3D level editing

**TileMapLayer3D** (github.com/DanTrz/TileMapLayer3D) provides a Crocotile3D-inspired workflow for painting 3D levels from 2D tilesheets. It supports **4 mesh modes** (flat square, flat triangle, box mesh, prism mesh), **18 orientations** including walls, ceiling, and 45° tilted variants, and multi-tile selection up to 48 tiles.

Controls include WASD for cursor movement, Q/E for 90° rotation, R for cycling tilt angles, and Shift+Drag for area painting. Features autotiling using Godot's native TileSet system, collision generation, and mesh baking. Requires **Godot 4.5+**, MIT license, **56 GitHub stars**. This is a completely independent system from GridMap—ideal for pixel-art or retro-style 3D games rather than modular asset placement.

**GodotNode2Tile** (github.com/QJPG/GodotNode2Tile, Asset Library ID: **2873**) offers similar functionality with vertex editing in-editor, UV property controls, TileMap layers, and auto-generated collisions. Compatible with **Godot 4.2-4.3**, MIT license.

---

## Terrain and environment systems support stylized aesthetics

Four major terrain addons provide excellent support for stylized and low-poly workflows.

**Terrain3D** (github.com/TokisanGames/Terrain3D, Asset Library ID: **3134**) is the most feature-complete option with **3,500+ GitHub stars**. Written in C++ as GDExtension for high performance, it supports terrains from 64×64m up to **65.5×65.5km**, 32 textures, 10 LOD levels, built-in foliage instancing with shadow impostors, sculpting, holes, and texture painting. Imports heightmaps from HTerrain, Gaea, Unity, and Unreal. The instancer uses MultiMesh and works with any imported mesh including glTF/GLB. Features a **LOW_POLY shader mode** specifically for stylized workflows. **Godot 4.3+**, MIT license.

**HTerrain** (github.com/Zylann/godot_heightmap_plugin, godot4 branch) is Zylann's pure GDScript terrain with texture painting, built-in grass/detail layers, and multiple shader types including LOW_POLY mode. Detail layers support custom meshes via the `instance_mesh` property. **Godot 4.1+**, MIT license, ~2k GitHub stars.

**TerraBrush** (github.com/spimort/TerraBrush, Asset Library ID: **2700**) provides user-friendly terrain editing with integrated **packed scene scattering**—scatter any scene with random rotation following terrain sculpting. Rewritten in C++ GDExtension since v0.14.0, no .NET required. Features water with flow painting, snow system, and experimental in-game editor. **Godot 4.4+**, MIT license.

**ProtonScatter** (github.com/HungryProton/scatter, Asset Library ID: **1866**) is the **premier procedural placement system** with **2,700+ GitHub stars**. Uses a Blender-like modifier stack for non-destructive asset scattering. Supports grid, random, and edge-based point creation, box/sphere/path shapes (combinable with negative exclusion shapes), surface projection onto colliders, and array modifiers for stacking. Works with any scene including glTF/GLB imports. **Godot 4.x**, MIT license.

**Spatial Gardener** (github.com/dreadpon/godot_spatial_gardener) specializes in painting props on **any 3D surface**, not just terrain. Uses octree-based spatial organization and frustum culling for efficient handling of thousands of instances. Documentation explicitly mentions compatibility with Quaternius asset packs. **Godot 4.2+**, MIT license, **1,200+ GitHub stars**.

---

## Converting asset packs to MeshLibraries follows established workflows

The most efficient method for converting Quaternius and similar asset packs uses the **GLTF2MeshLib plugin** (github.com/zincles/godot_gltf2meshlib, Asset Library ID: **2845**). Drag GLTF/GLB files into the editor, select "GLTF To MeshLibrary" in import settings, and the plugin automatically generates MeshLibraries. Use `--collision` or `-col` flags in model names for collision shapes, `--noimp` to exclude objects.

The manual workflow requires creating a scene with MeshInstance3D nodes as direct children of the root, adding StaticBody3D with CollisionShape3D for each mesh, then exporting via Scene → Convert To → MeshLibrary. Only the first two levels of the node tree are included—use "Make Local" on imported assets to modify hierarchy.

**Pre-converted Quaternius packs** are available on the Asset Library:
- Quaternius Modular Sci-Fi Pack (ID: **1671**)
- Quaternius Simple Nature (ID: **1819**)  
- Quaternius Ultimate Spaceships (ID: **1674**)

These include both MeshLibrary resources and scene nodes with occlusion culling, maintained by Malcolmnixon on GitHub.

For Blender-originated assets, use import hints: `-col` or `-colonly` suffix adds collision shapes, `-convcolonly` generates convex collision. Godot auto-generates physics bodies on import when these suffixes are present.

---

## Practical recommendations for modular kit workflows

**Recommended tool stack for Quaternius-style packs:**

1. **Primary placement:** AssetSnap (free, MIT) or AssetPlacer ($17.99) for library management and grid snapping
2. **GridMap workflow:** GLTF2MeshLib for automatic MeshLibrary conversion, then GridMap for structured placement
3. **Surface snapping:** ExtraSnaps for precision alignment between pieces
4. **Environmental decoration:** ProtonScatter for procedural scattering of grass, rocks, debris
5. **Terrain:** Terrain3D for large landscapes, TerraBrush for smaller hand-crafted areas

**Grid configuration for modular assets:** Set GridMap cell size to match asset grid (typically 1m or 2m for Quaternius kits). Set Center X/Y/Z to Off for corner-aligned assets. Consider 0.5m Y grid size for flexible vertical placement.

**Performance considerations:** GridMaps efficiently batch render but don't support occlusion culling—for large levels, use scene instances with occlusion culling polygons for distant areas. Use GridMap for static geometry only; place interactable objects as separate scene instances.

**Common issues and solutions:**
- Meshes not appearing in MeshLibrary: ensure MeshInstance3D nodes are direct children of scene root
- Character bumping on tile edges: ensure collision shapes perfectly match mesh boundaries
- GridMap collision not working on first frame: documented bug, add single-frame delay before physics interactions

---

## Conclusion

The Godot 4 ecosystem provides mature tooling for 3D level design across different workflows. **GridMap Plus** and **EnhancedGridMap** extend the built-in system with modern editing features, while **Tile to Gridmap** and **TileMapLayer3D** offer alternative 2D-driven approaches. For modular kit placement, **AssetSnap** delivers the most comprehensive free solution with its groups system enabling prefab-like workflows. **ProtonScatter** and **Spatial Gardener** handle procedural environmental decoration excellently, both explicitly tested with stylized asset packs. The **GLTF2MeshLib** plugin eliminates conversion friction when working with downloaded asset packs. Most tools target **Godot 4.2-4.4** with MIT licensing, and all support imported glTF/GLB assets either directly or through MeshLibrary conversion.