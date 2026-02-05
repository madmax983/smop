# ADR 0002: Sync-Only HTTP with ureq

## Status

Accepted

## Context

HTTP client options for Rust:

1. **reqwest** - Full-featured, async-first, optional blocking mode
2. **ureq** - Pure sync, minimal dependencies, simple API
3. **hyper** - Low-level, maximum control, async-only
4. **attohttpc** - Sync, minimal, similar to ureq
5. **isahc** - curl-based, sync and async

Considerations:
- Scripts typically don't need async complexity
- Tokio adds ~300+ transitive dependencies
- Simple GET/POST covers 90% of scripting needs

## Decision

Use `ureq` as the HTTP client:

- Synchronous-only API
- Feature-gated under `http` feature
- Simple functions: `get()`, `post()`, `get_json()`, `post_json()`

## Consequences

### Positive

- No async runtime required
- Minimal dependency footprint (~20 deps vs ~300 for reqwest)
- Simple mental model for scripts
- Fast compile times

### Negative

- No async support (intentional)
- Less feature-rich than reqwest (no cookies, redirects need manual handling)
- Users needing advanced HTTP features should use reqwest directly

### Trade-offs

We sacrifice advanced HTTP features for simplicity. This aligns with the crate's philosophy of "good enough" for scripting. Users with complex HTTP needs can add reqwest alongside scriptkit.

## References

- [ureq crate](https://docs.rs/ureq)
- [reqwest crate](https://docs.rs/reqwest)
