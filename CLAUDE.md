# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Critical Info
- Use Godot 4.6
- Always consult documentation, download it if needed
- Prefer Godot `export` variables over magic numbers or constants

## Docs
- Godot Rust Book - https://godot-rust.github.io/book/
- Godot API Docs - https://godot-rust.github.io/docs/ and https://godot-rust.github.io/docs/gdext/master/godot/
- Godot Github - https://github.com/godot-rust/gdext
- Godot clip-plane / cross-sectional shaders
  - Walkthrough - https://www.ronja-tutorials.com/post/021-plane-clipping/
  - Godot Code - https://github.com/19PHOBOSS98/Godot-Planar-and-Box-Material-Cutoff-Shader
- Godot Pixel Art Shaders - https://github.com/DylearnDev/Dylearn-3D-Pixel-Art-Grass-Demo/tree/main/Shaders
- Other Shader Info - https://minionsart.github.io/tutorials/

## Project Overview

Pixy Terrain is a Godot 4 terrain editor tool written in Rust using GDExtension (godot-rust/gdext). It creates terrain for 3D pixel art games using solid geometry and transvoxel algorithms.

## Known Working States

**IMPORTANT: Restore working stencil cap shader:**
```bash
git show working-stencil-cap-v1:rust/src/shaders/stencil_cap.gdshader > rust/src/shaders/stencil_cap.gdshader && cd rust && cargo build
```

## Build Commands

```bash
# Build the Rust GDExtension library
cd rust && cargo build

# Build release version
cd rust && cargo build --release

# Run tests
cd rust && cargo test

# Check code without building
cd rust && cargo check

# Format code
cd rust && cargo fmt

# Lint code
cd rust && cargo clippy
```

## Project Structure

```
pixy_terrain/                         # Lives at ~/pixy/pixy_terrain/
├── godot/                            # Godot test project for standalone addon development
│   ├── project.godot                 # Godot project configuration
│   ├── addons/pixy_terrain/          # Addon directory (symlinked into game projects)
│   │   ├── pixy_terrain.gdextension  # GDExtension configuration
│   │   ├── bin/                      # Symlinks to compiled libraries
│   │   └── resources/                # Shaders, textures, materials
│   │       ├── shaders/
│   │       └── textures/
│   ├── scenes/
│   │   └── test_scene.tscn           # Test scene with terrain node
│   └── scripts/
│       └── player.gd                 # Isometric camera + WASD player
└── rust/                             # Rust GDExtension library
    ├── Cargo.toml
    └── src/
        ├── lib.rs                    # Extension entry point
        ├── terrain.rs                # PixyTerrain node (Node3D)
        ├── chunk.rs                  # PixyTerrainChunk (MeshInstance3D)
        ├── marching_squares/         # Geometry generation (17 cases)
        ├── editor_plugin.rs          # Editor tool modes
        ├── gizmo.rs                  # Brush visualization
        ├── grass_planter.rs          # MultiMesh grass
        ├── texture_preset.rs         # Texture presets
        └── quick_paint.rs            # Quick paint presets
```

## Addon Architecture

This repo is structured as a **GDExtension addon**. The addon lives at `godot/addons/pixy_terrain/`.

- **Symlink into game projects**: `ln -s ~/pixy/pixy_terrain/godot/addons/pixy_terrain ~/pixy/pixy_game/godot/addons/pixy_terrain`
- **Resource paths**: All Rust code uses `res://addons/pixy_terrain/resources/` prefix
- **Library paths**: .gdextension references `res://addons/pixy_terrain/bin/` with platform-specific symlinks to `rust/target/`

### Related Projects

- `~/pixy/pixy_tree/` — Procedural tree addon (same addon pattern)
- `~/pixy/pixy_game/` — Game project that symlinks both addons + owns player/camera

## Development Workflow

1. Make changes to Rust code in `rust/src/`
2. Run `cargo build` from `rust/` directory
3. Open Godot project from `godot/` directory - it auto-loads the extension
4. Godot 4.2+ supports hot reloading - changes rebuild without restarting Godot

## Key Dependencies

- `godot` crate from godot-rust/gdext (master branch) - Rust bindings for Godot 4
- Transvoxel algorithm will be used for mesh generation from voxel data

## GDExtension Notes

- Custom Godot classes are created using `#[derive(GodotClass)]`
- Methods exposed to GDScript use `#[func]`
- Signals use `#[signal]`
- See https://godot-rust.github.io/book/ for gdext documentation
