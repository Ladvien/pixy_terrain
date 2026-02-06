# Review Checklist — Pixy Terrain

## Code Quality
- [ ] No dead code (unused functions, variables, imports)
- [ ] No WET patterns (duplicated logic that should be extracted)
- [ ] No magic numbers — use named constants or `#[export]` vars
- [ ] Proper error handling (no unwrap() in production paths, only in infallible cases)
- [ ] No TODO/FIXME/HACK without a tracking issue
- [ ] Clippy clean (no warnings)
- [ ] Consistent naming conventions (snake_case for fns/vars, PascalCase for types)

## Safety & Correctness
- [ ] No panic paths reachable during normal editor use
- [ ] Array/Vec bounds checked before indexing
- [ ] No integer overflow in index calculations (especially cell/chunk coordinate math)
- [ ] Borrow checker patterns correct (no unnecessary clones, proper split-borrow)
- [ ] Godot object lifetime handled correctly (Gd<T> not used after free)
- [ ] Thread safety considered for any shared state

## Godot/GDExt Integration
- [ ] `#[func]` methods match expected Godot signatures
- [ ] `#[signal]` definitions correct
- [ ] Export variables have sensible defaults
- [ ] No resource leaks (allocated nodes freed, signals disconnected)
- [ ] Deferred calls used where required (scene tree modifications)
- [ ] Editor vs runtime code properly gated with `Engine::singleton().is_editor_hint()`

## Marching Squares Geometry
- [ ] All 17 cases produce watertight geometry (no holes between cells)
- [ ] Rotation logic matches Yugen's clockwise [A,B,D,C] convention
- [ ] Height interpolation uses correct barycentric weights (a*u + b*v + c*w)
- [ ] Wall UVs include chunk_position offset for seamless tiling
- [ ] Color sampling from correct texture indices
- [ ] Cross-chunk edge heights match (no seams at chunk boundaries)

## Editor Plugin
- [ ] Undo/redo works for all tool modes
- [ ] Brush pattern correctly handles cross-chunk boundaries
- [ ] UI controls properly rebuild when switching tool modes
- [ ] Mouse input consumed correctly (no pass-through to scene)
- [ ] Gizmo updates on brush move/resize

## Performance
- [ ] No unnecessary full-chunk regeneration (prefer incremental updates)
- [ ] No allocations in hot loops (pre-allocate Vecs, reuse buffers)
- [ ] SurfaceTool usage efficient (batch vertex additions)
- [ ] Grass planter doesn't regenerate unnecessarily

## Data Persistence
- [ ] PackedArray ↔ Vec sync correct (sync_to_packed / restore_from_packed)
- [ ] Scene serialization preserves all terrain state
- [ ] Texture presets save/load all fields
- [ ] No data loss on exit_tree / enter_tree cycle
