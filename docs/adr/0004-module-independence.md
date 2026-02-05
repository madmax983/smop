# ADR 0004: Module Independence via Feature Flags

## Status

Accepted

## Context

Scriptkit bundles multiple capabilities (HTTP, CLI, terminal UI). Not all scripts need all features. Options:

1. **Monolithic** - Everything always compiled
2. **Separate crates** - scriptkit-http, scriptkit-cli, etc.
3. **Feature flags** - Single crate with optional features

## Decision

Use feature flags with a `full` default:

```toml
[features]
default = ["full"]
full = ["http", "cli", "print"]
http = ["dep:ureq"]
cli = ["dep:clap"]
print = ["dep:console", "dep:indicatif", "dep:dialoguer"]
```

## Consequences

### Positive

- Minimal builds: `default-features = false` for core only
- Single crate to depend on
- Selective feature inclusion
- Compile time reduction for unused features

### Negative

- Conditional compilation adds complexity
- Documentation must clarify feature requirements
- CI needs to test feature combinations

### Feature Matrix

| Feature | Dependencies Added | Modules Enabled |
|---------|-------------------|-----------------|
| (none)  | anyhow, serde, dirs, shellexpand, dotenvy | env, fs, path, sh |
| http    | ureq | http |
| cli     | clap | cli |
| print   | console, indicatif, dialoguer | print, macros |

### Testing Strategy

CI tests:
- `cargo test` (all features)
- `cargo test --no-default-features` (core only)
- `cargo test --features http` (individual features)

## References

- [Cargo Features](https://doc.rust-lang.org/cargo/reference/features.html)
