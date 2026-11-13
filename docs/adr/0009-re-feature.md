# ADR 0009: `re` Feature and the `toml` Re-export

## Status

Accepted (0.4.0).

## Context

Harvest's xtask uses `regex` in about 50 files and `toml::` in 9 files. The
crate has a direct dependency on each, next to smop. 0.3.2 fixed the same problem for
`serde_json` with a re-export.

## Decision

- The `re` feature adds `smop::re`. It is part of `full`.
- `smop::re` re-exports all of `regex` (`pub use regex::*`). `re::Regex`,
  `re::Captures`, `re::escape` and `re::bytes` are the regex items. We do not
  wrap them. A major release of `regex` is a major release of smop.
- `smop::re` adds helpers that take the pattern as `&str`, with short names:
  `compile`, `is_match`, `find`, `find_all`, `captures`, `captures_all`,
  `replace`, `replace_all` and `split`.
- The helpers use the argument order of the `Regex` methods: pattern, then
  text, then replacement. Python's `re.sub` puts the replacement before the
  text. Both are `&str`, so the compiler does not find this mistake. The
  docs tell the user.
- A pattern that is too big to compile is an error:
  `Regex pattern '<pattern>' is too big`.
- An invalid pattern is an error: `Invalid regex pattern '<pattern>'`. The
  `regex::Error` is the cause. All helpers use `compile`, so all give the
  same error.
- The error shows the pattern as it is, in single quotes. `{:?}` would
  double each `\`, and regex patterns have many.
- Each helper compiles its pattern on each call. We do not keep a cache.
  For a loop, the docs tell the user to call `compile` one time.
- `replace` and `replace_all` take `impl Replacer`. A `&str` can use `$1`
  and `${name}`. A closure also works. They return `String`, not `Cow`.
- `smop::toml` re-exports the `toml` crate when the `toml` feature is on.
- smop uses the default features of `regex` and `toml`. They are part of
  smop's API now. To turn one off is a breaking change.
- The API boundary check (`cargo xtask api-boundary`, ADR 0017) allows
  `regex` and `toml`.

## Consequences

- A script needs only smop for regex and TOML. `tests/consumer` proves it.
- The helpers are slow in a hot loop. `compile` is the fix.
- `regex` adds 5 crates (`regex`, `regex-automata`, `regex-syntax`,
  `aho-corasick`, `memchr`). Without `re`, smop adds no regex crates.
