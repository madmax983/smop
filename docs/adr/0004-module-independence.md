# ADR 0004: Module Independence via Feature Flags

## Status

Accepted. Extended by [ADR 0006](0006-granular-core-features.md), which moves the
core modules (`env`, `fs`, `path`, `sh`) behind features as well. Amended in
0.4: dependency types stay out of the public API (see below).

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

## Amendment (0.4): API boundaries

Types of optional dependencies also stay out of smop's public API.
Otherwise a major release of the dependency is a smop major release, and
the feature flag does not isolate it.

- A crate that smop re-exports is deliberate API: `anyhow`, `clap`, `serde`,
  `serde_json`, `toml` and `regex` (as `smop::re`, ADR 0009). One exception is
  not a whole crate: `time` re-exports chrono's date types on purpose.
- Other dependency types do not go into a public signature, field or trait
  impl. smop wraps them: `print::Progress` (indicatif) and `fs::TempDir`
  (tempfile) replace the types that 0.3 returned.
- `cargo xtask api-boundary` checks this in CI through rustdoc (it was
  `scripts/api_boundary.sh` until ADR 0017).

## References

- [Cargo Features](https://doc.rust-lang.org/cargo/reference/features.html)
