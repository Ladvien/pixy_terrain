# Transvoxel Noise Terrain - Walkthrough Series

A step-by-step implementation guide for building 3D noise-based terrain generation using the transvoxel algorithm in Godot 4 GDExtension.

## Series Overview

| # | Walkthrough | Focus |
|---|-------------|-------|
| 01 | [Dependencies](01-transvoxel-dependencies.md) | Cargo.toml setup |
| 02 | [Core Data Structures](02-core-data-structures.md) | ChunkCoord, MeshResult, ChunkState |
| 03 | [LOD Configuration](03-lod-configuration.md) | Distance-based LOD selection |
| 04 | [Noise Field SDF](04-noise-field-sdf.md) | 3D noise terrain generation |
| 05 | [Transvoxel Extraction](05-transvoxel-extraction.md) | Mesh generation from SDF |
| 06 | [Worker Pool](06-bevy-tasks-worker-pool.md) | Parallel mesh generation |
| 07 | [Chunk Manager](07-chunk-manager.md) | LOD selection, chunk lifecycle |
| 08 | [Godot Integration](08-godot-integration.md) | PixyTerrain node, ArrayMesh upload |

## Architecture

```
┌─────────────────────┐
│   PixyTerrain       │  Godot Node (main thread)
│   (walkthrough 08)  │
├─────────────────────┤
│                     │
│  ChunkManager  ─────┼──► MeshWorkerPool
│  (walkthrough 07)   │    (walkthrough 06)
│                     │         │
│  LODConfig          │         ▼
│  (walkthrough 03)   │    extract_chunk_mesh
│                     │    (walkthrough 05)
└─────────────────────┘         │
                                ▼
                           NoiseField
                           (walkthrough 04)
```

## Build Order Rationale

The walkthroughs are ordered to minimize rework:

1. **Dependencies first** - Everything else needs the crates
2. **Data structures** - Foundation types used everywhere
3. **LOD config** - Needed by chunk manager
4. **Noise field** - Needed by mesh extraction
5. **Transvoxel extraction** - Needed by worker pool
6. **Worker pool** - Needed by chunk manager
7. **Chunk manager** - Needed by Godot integration
8. **Godot integration** - Ties everything together

Each walkthrough produces working, testable code before moving on.

## Final File Structure

```
rust/
├── Cargo.toml           # 01: Dependencies
└── src/
    ├── lib.rs           # Module registration
    ├── chunk.rs         # 02: ChunkCoord, MeshResult, ChunkState
    ├── lod.rs           # 03: LODConfig
    ├── noise_field.rs   # 04: NoiseField, NoiseDataField
    ├── mesh_extraction.rs # 05: extract_chunk_mesh
    ├── mesh_worker.rs   # 06: MeshWorkerPool
    ├── chunk_manager.rs # 07: ChunkManager
    └── terrain.rs       # 08: PixyTerrain
```

## Key Technical Decisions

- **bevy_tasks over rayon** - 8-10x lower idle CPU
- **32³ chunks** - Optimal L2 cache fit
- **Crossbeam channels** - Thread-safe main/worker communication
- **World-space vertices** - Simplified chunk positioning
- **No Godot types in workers** - Required for thread safety

## Running Tests

After each walkthrough:
```bash
cd rust && cargo test
```

Final build:
```bash
cd rust && cargo build
```
