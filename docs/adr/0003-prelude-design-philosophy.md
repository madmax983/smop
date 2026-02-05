# ADR 0003: Prelude Design Philosophy

## Status

Accepted

## Context

A prelude module provides a curated set of re-exports for ergonomic imports. Design questions:

1. What should be included vs excluded?
2. How to handle feature-gated items?
3. Should we flatten all modules or preserve namespacing?

## Decision

The prelude includes:

### Always Available
- `anyhow::{anyhow, bail, ensure, Context, Result}` - Error handling essentials
- `serde::{Serialize, Deserialize}` - Serialization traits
- Modules: `env`, `fs`, `path`, `sh` - Core scripting utilities

### Feature-Gated
- `http` module (requires `http` feature)
- `print` module + macros (requires `print` feature)
- `cli` types from clap (requires `cli` feature)

### Design Principles

1. **Preserve namespacing** - Use `fs::read_string()` not `read_string()`
2. **Import once** - `use scriptkit::prelude::*` gets everything
3. **No conflicts** - Avoid items that shadow std prelude
4. **Feature-aware** - Gated items only appear with features enabled

## Consequences

### Positive

- Single import for all common operations
- Clear module boundaries reduce naming conflicts
- Feature flags keep unused dependencies out

### Negative

- Glob imports are controversial (but appropriate for scripts)
- Users must know which features enable which modules

### Rationale

For application code, explicit imports are preferred. For scripts, convenience wins. The prelude is designed for `main.rs` scripting use cases where you want everything at your fingertips.

## References

- [Rust API Guidelines on Preludes](https://rust-lang.github.io/api-guidelines/organization.html)
