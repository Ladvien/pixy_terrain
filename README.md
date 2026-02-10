# Pixy Terrain

A Godot 4.6 terrain editor tool written in Rust using GDExtension ([godot-rust/gdext](https://github.com/godot-rust/gdext)). Creates terrain for 3D pixel art games using solid geometry and a marching squares algorithm.

**This project is a port/remix** — the core terrain system, marching squares algorithm, editor plugin, and shaders are ported from existing open-source work by other authors. See the credits below.

## Credits & Attribution

### Yugen — Original Terrain Authoring Toolkit

The marching squares terrain algorithm, chunk mesh generation, terrain manager architecture, editor plugin design, terrain shader, and grass shader pipeline are ported from Yugen's GDScript implementation.

- **Repository:** [Yugens-Terrain-Authoring-Toolkit](https://github.com/Yukitty/Yugens-Terrain-Authoring-Toolkit)
- **What was ported:** `marching_squares_terrain_chunk.gd`, `marching_squares_terrain.gd`, `marching_squares_terrain_plugin.gd`, `mst_terrain.gdshader`, `mst_grass.gdshader`

### Dylearn (DylearnDev) — 3D Pixel Art Grass Demo

The grass shader's dual-noise wind system, character displacement, cloud shadow system (`clouds.gdshaderinc`), hybrid toon lighting, and fake perspective effect are adapted from Dylearn's demo project.

- **Repository:** [Dylearn-3D-Pixel-Art-Grass-Demo](https://github.com/DylearnDev/Dylearn-3D-Pixel-Art-Grass-Demo)
- **License:** Code under MIT License, Art under CC BY 4.0 — see `godot/resources/shaders/DYLEARN_LICENSE` for details
- **What was adapted:** `clouds.gdshaderinc`, wind/displacement/lighting portions of the grass shader

### godot-rust/gdext — Rust GDExtension Bindings

The Rust bindings that make this project possible.

- **Repository:** [godot-rust/gdext](https://github.com/godot-rust/gdext)

### Ronja Tutorials — Shader Techniques

Plane clipping and cross-sectional shader concepts referenced during development.

- **Website:** [ronja-tutorials.com](https://www.ronja-tutorials.com/post/021-plane-clipping/)

### Minions Art — Shader Techniques

General shader techniques and references used during development.

- **Website:** [minionsart.github.io/tutorials](https://minionsart.github.io/tutorials/)

## Building

```bash
# Build the Rust GDExtension library
cd rust && cargo build

# Build release version
cd rust && cargo build --release

# Run tests
cd rust && cargo test
```

See [CLAUDE.md](CLAUDE.md) for full project structure and development workflow details.

## License

This project contains code adapted from multiple sources with different licenses. See individual file headers and `godot/resources/shaders/DYLEARN_LICENSE` for specifics.
