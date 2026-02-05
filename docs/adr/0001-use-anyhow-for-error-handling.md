# ADR 0001: Use anyhow for Error Handling

## Status

Accepted

## Context

Scripting libraries need simple, ergonomic error handling. Options include:

1. **Custom error types** with thiserror - Maximum type safety, high boilerplate
2. **anyhow** - Dynamic errors with context, minimal boilerplate
3. **eyre** - Similar to anyhow with customizable reports
4. **Box<dyn Error>** - Standard library, no dependencies

For a scripting-focused crate, the priority is developer ergonomics over compile-time error type checking.

## Decision

Use `anyhow` for all error handling:

- Re-export `anyhow::Result<T>` as `Result<T>` in the prelude
- Expose `anyhow!`, `bail!`, `ensure!`, and `Context` trait
- All public functions return `anyhow::Result<T>`

## Consequences

### Positive

- Zero friction for scripts: `fn main() -> Result<()>`
- Context chaining: `.context("what I was doing")?`
- Compatible with any error type via `Into<anyhow::Error>`
- Backtraces in debug mode for debugging

### Negative

- Cannot match on specific error types without downcasting
- Slight runtime overhead vs static dispatch

### Mitigation

For users needing typed errors, they can use `anyhow::Error::downcast_ref()` or create their own error types that implement `std::error::Error`.

## References

- [anyhow crate](https://docs.rs/anyhow)
- [Error Handling in Rust](https://blog.burntsushi.net/rust-error-handling/)
