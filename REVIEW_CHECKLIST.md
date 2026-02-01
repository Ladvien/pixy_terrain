# Review Checklist

## Code Quality
- [ ] No magic numbers (use `#[export]` variables or constants)
- [ ] Functions are focused and single-purpose
- [ ] Clear naming conventions followed
- [ ] No dead code or unused imports
- [ ] Proper error handling (no unwrap in production paths)

## Rust/GDExtension
- [ ] `#[derive(GodotClass)]` used correctly
- [ ] `#[func]` exposed methods are documented
- [ ] `#[export]` variables have sensible defaults
- [ ] No blocking operations in main thread
- [ ] Resources properly managed (no leaks)

## Architecture (per ARCHITECTURE.md)
- [ ] Chunk system respects boundaries
- [ ] Noise field uses 2D sampling (not 3D)
- [ ] Wall normals point outward
- [ ] Watertight seams maintained
- [ ] Transvoxel boundaries respected

## Security
- [ ] No command injection vulnerabilities
- [ ] No unsafe blocks without justification
- [ ] No hardcoded paths or credentials
- [ ] Input validation on user data

## Performance
- [ ] No unnecessary allocations in hot paths
- [ ] Chunk updates are incremental where possible
- [ ] No blocking I/O in render path
- [ ] Mesh generation uses worker threads

## Testing
- [ ] New features have tests
- [ ] Edge cases covered
- [ ] Tests pass (`cargo test`)

## Documentation
- [ ] Public API documented
- [ ] Complex algorithms explained
- [ ] CLAUDE.md updated if needed
