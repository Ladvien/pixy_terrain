# Pixy Terrain — Part 01: Project Scaffolding & Hello Godot

**Series:** Reconstructing Pixy Terrain
**Part:** 01 of 18
**Previous:** None
**Status:** Complete

## What We're Building

The foundation: a Rust GDExtension project that compiles to a shared library and loads into Godot 4.6. Every module is declared but stubbed — the project compiles with zero functionality, ready to fill in.

## What You'll Have After This

A Godot 4.6 project that opens without errors, with a GDExtension library that loads successfully. Running `cargo build` produces a `.dylib`/`.so`/`.dll` that Godot recognizes. No visible terrain yet — just the skeleton.

## Prerequisites

- Rust toolchain (1.78+)
- Godot 4.6 installed
- Git initialized in your working directory

## Steps

### Step 1: Create the directory structure

**Why:** The project splits cleanly into `rust/` (Rust GDExtension library) and `godot/` (Godot project). This separation keeps build artifacts out of the Godot editor's import scanner and makes the Rust side independently buildable.

```bash
mkdir -p pixy_terrain/rust/src/shaders
mkdir -p pixy_terrain/godot/scenes
mkdir -p pixy_terrain/godot/scripts
mkdir -p pixy_terrain/godot/resources/shaders
mkdir -p pixy_terrain/godot/resources/materials
mkdir -p pixy_terrain/godot/resources/textures
mkdir -p pixy_terrain/godot/textures/ground
mkdir -p pixy_terrain/godot/textures/grass
cd pixy_terrain
git init
```

### Step 2: Create `.gitignore`

**Why:** Keep compiled artifacts, Godot's import cache, and OS metadata out of version control.

**File:** `.gitignore`

```
/rust/target/
/godot/.godot/
*.import
.DS_Store
```

### Step 3: Create `rust/Cargo.toml`

**Why:** This is a `cdylib` crate — it compiles to a C-compatible shared library (`.dylib` on macOS, `.so` on Linux, `.dll` on Windows) that Godot loads at runtime via GDExtension. The `godot` crate comes from the `gdext` repository's master branch, which tracks Godot 4.6 compatibility.

**File:** `rust/Cargo.toml`

```toml
[package]
name = "pixy_terrain"
version = "0.1.0"
edition = "2021"
rust-version = "1.78"

[lib]
crate-type = ["cdylib"]

[dependencies]
godot = { git = "https://github.com/godot-rust/gdext", branch = "master" }

[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3
```

**What's happening:**
- `crate-type = ["cdylib"]` is critical — without it, Rust produces a `.rlib` (Rust library) instead of a shared library Godot can load.
- `opt-level = 1` for dev builds gives a reasonable balance: debug builds aren't painfully slow, but you still get debug symbols. Dependencies get `opt-level = 3` since you never debug into `godot-rust` internals.
- The `godot` crate is pulled from git master because gdext doesn't publish stable crates to crates.io yet. Pin to a specific commit hash in production.

### Step 4: Create `rust/.cargo/config.toml`

**Why:** On macOS, the dynamic library needs an `install_name` with `@rpath` so Godot can find it relative to the project. Without this, Godot silently fails to load the extension on macOS.

**File:** `rust/.cargo/config.toml`

```toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-Wl,-install_name,@rpath/libpixy_terrain.dylib"]

[target.x86_64-apple-darwin]
rustflags = ["-C", "link-arg=-Wl,-install_name,@rpath/libpixy_terrain.dylib"]
```

**What's happening:** This passes a linker flag that embeds `@rpath/libpixy_terrain.dylib` as the library's install name. Godot uses `@rpath` to resolve the library location relative to the `.gdextension` file. Both ARM (Apple Silicon) and Intel targets are covered.

### Step 5: Create `rust/src/lib.rs`

**Why:** This is the GDExtension entry point. The `#[gdextension]` macro generates the C FFI functions that Godot calls to discover and register all your custom classes. Every module is declared here as an empty stub — the project compiles immediately, and we fill in modules one at a time.

**File:** `rust/src/lib.rs`

```rust
use godot::prelude::*;

mod chunk;
mod editor_plugin;
mod gizmo;
mod grass_planter;
mod marching_squares;
mod quick_paint;
mod terrain;
mod texture_preset;

struct PixyTerrainExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PixyTerrainExtension {}
```

**What's happening:**
- `use godot::prelude::*` brings in the core gdext types: `Gd`, `Base`, `GodotClass`, `Vector3`, etc.
- The 8 `mod` declarations tell Rust these modules exist. Each needs a corresponding `.rs` file (even if empty).
- `#[gdextension]` on the `ExtensionLibrary` impl generates the `gdext_rust_init` symbol that Godot looks for (matching the `entry_symbol` in `.gdextension`).
- `unsafe` is required because the FFI boundary is inherently unsafe — gdext handles the actual safety internally.

### Step 6: Create empty module stubs

**Why:** Rust won't compile if `lib.rs` declares modules that don't exist. We create empty files now and fill them in subsequent walkthroughs.

**File:** `rust/src/marching_squares.rs`

```rust
// Marching squares terrain algorithm — implemented in Parts 02-05
```

**File:** `rust/src/chunk.rs`

```rust
// Terrain chunk mesh generation — implemented in Parts 06-07
```

**File:** `rust/src/terrain.rs`

```rust
// Terrain manager node — implemented in Parts 09-10
```

**File:** `rust/src/grass_planter.rs`

```rust
// Grass planting system — implemented in Part 12
```

**File:** `rust/src/editor_plugin.rs`

```rust
// Editor plugin — implemented in Parts 15-17
```

**File:** `rust/src/gizmo.rs`

```rust
// Gizmo visualization — implemented in Part 14
```

**File:** `rust/src/quick_paint.rs`

```rust
// Quick paint resource — implemented in Part 11
```

**File:** `rust/src/texture_preset.rs`

```rust
// Texture preset resource — implemented in Part 11
```

### Step 7: Create `godot/project.godot`

**Why:** This is Godot's project configuration. It declares the project name, target Godot version, and renderer. The `default_texture_filter=0` setting uses nearest-neighbor filtering — essential for the pixel art aesthetic this terrain system targets.

**File:** `godot/project.godot`

```ini
; Engine configuration file.
; It's best edited using the editor UI and not directly,
; since the parameters that go here are not all obvious.
;
; Format:
;   [section] ; section goes between []
;   param=value ; assign values to parameters

config_version=5

[animation]

compatibility/default_parent_skeleton_in_mesh_instance_3d=true

[application]

config/name="Pixy Terrain"
run/main_scene="res://scenes/test_scene.tscn"
config/features=PackedStringArray("4.6", "Forward Plus")
config/icon="res://icon.svg"

[rendering]

textures/canvas_textures/default_texture_filter=0
```

**What's happening:**
- `config_version=5` is Godot 4.x's config format version.
- `Forward Plus` is the rendering backend (the most capable, supports all shader features).
- `default_texture_filter=0` = nearest filtering globally, so terrain textures stay crisp at low resolution.
- `run/main_scene` points to a test scene we'll create in Part 18.

### Step 8: Create `godot/pixy_terrain.gdextension`

**Why:** This file tells Godot where to find the compiled Rust library for each platform. The paths use `res://../rust/target/` because the Godot project is inside `godot/` while the Rust project is in `rust/` — they're siblings.

**File:** `godot/pixy_terrain.gdextension`

```ini
[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = 4.1
reloadable = true

[libraries]
linux.debug.x86_64 = "res://../rust/target/debug/libpixy_terrain.so"
linux.release.x86_64 = "res://../rust/target/release/libpixy_terrain.so"
windows.debug.x86_64 = "res://../rust/target/debug/pixy_terrain.dll"
windows.release.x86_64 = "res://../rust/target/release/pixy_terrain.dll"
macos.debug = "res://../rust/target/debug/libpixy_terrain.dylib"
macos.release = "res://../rust/target/release/libpixy_terrain.dylib"
macos.debug.arm64 = "res://../rust/target/debug/libpixy_terrain.dylib"
macos.release.arm64 = "res://../rust/target/release/libpixy_terrain.dylib"
macos.debug.x86_64 = "res://../rust/target/debug/libpixy_terrain.dylib"
macos.release.x86_64 = "res://../rust/target/release/libpixy_terrain.dylib"
```

**What's happening:**
- `entry_symbol = "gdext_rust_init"` must match exactly what `#[gdextension]` generates. This is the C function Godot calls to initialize the extension.
- `compatibility_minimum = 4.1` means this extension works with Godot 4.1+.
- `reloadable = true` enables hot-reloading: rebuild Rust, and Godot picks up changes without restarting. This is transformative for iteration speed.
- macOS has separate entries for `arm64` (Apple Silicon) and `x86_64` (Intel), plus a bare `macos.debug` fallback. All point to the same file since Rust builds for the host architecture.

## Verify

```bash
cd rust && cargo build
```

**Expected:** Compilation succeeds with output like:

```
   Compiling godot-bindings v0.x.x (...)
   Compiling godot-ffi v0.x.x (...)
   Compiling godot-core v0.x.x (...)
   Compiling godot-macros v0.x.x (...)
   Compiling godot v0.x.x (...)
   Compiling pixy_terrain v0.1.0 (...)
    Finished `dev` profile [optimized + debuginfo] target(s)
```

The first build takes a few minutes (compiling the entire `godot` crate). Subsequent builds are fast (~2-5 seconds).

You can also open the Godot project (`godot/project.godot`) — it should load without errors, though there's nothing to see yet since we haven't created any nodes or scenes.

## What You Learned

- **GDExtension anatomy**: `Cargo.toml` (cdylib) + `lib.rs` (#[gdextension]) + `.gdextension` (library paths) = minimum viable extension
- **macOS rpath trick**: Without the `.cargo/config.toml` linker flag, Godot silently fails to load the library on macOS
- **Module-first architecture**: Declaring all 8 modules upfront (even empty) lets you build incrementally without restructuring `lib.rs` later
- **Pixel art rendering**: `default_texture_filter=0` is the single most important Godot setting for pixel art games

## Stubs Introduced

- [ ] `rust/src/marching_squares.rs` — empty, implemented in Part 02
- [ ] `rust/src/chunk.rs` — empty, implemented in Part 06
- [ ] `rust/src/terrain.rs` — empty, implemented in Part 09
- [ ] `rust/src/grass_planter.rs` — empty, implemented in Part 12
- [ ] `rust/src/editor_plugin.rs` — empty, implemented in Part 15
- [ ] `rust/src/gizmo.rs` — empty, implemented in Part 14
- [ ] `rust/src/quick_paint.rs` — empty, implemented in Part 11
- [ ] `rust/src/texture_preset.rs` — empty, implemented in Part 11

## Stubs Resolved

(None — this is Part 01)
