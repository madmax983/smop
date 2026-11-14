# ADR 0019: `yaml` Feature with a smop Parser

## Status

Accepted (0.5 cycle). Issue [madmax983/smop#53](https://github.com/madmax983/smop/issues/53).

## Context

YAML is in most CI and config files. `serde_yaml` is deprecated. The issue
sets three criteria for a crate:

1. Maintained: a release in the last 12 months, and its author does not
   mark it unmaintained.
2. serde support.
3. Builds on smop's MSRV (1.89).

The candidates on 2026-11-14:

| Crate | Last release | MSRV | Result |
|-------|--------------|------|--------|
| `serde_yaml` 0.9.34 | 2024-03 | 1.64 | Deprecated |
| `serde_yml` 0.0.13 | 2026-05 | 1.85 | Fails 1. Its author marks it unmaintained |
| `serde_norway` 0.9.42 | 2024-12 | 1.71 | Fails 1. Uses a C-to-Rust `unsafe` port of libyaml |
| `serde_yaml_ng` 0.10.0 | 2024-05 | 1.64 | Fails 1 |
| `serde-saphyr` 2.1.1 | 2026-11 | 1.89 | Passes 1 to 3. Adds 19 crates |
| `yaml-rust2`, `saphyr` | 2026 | 1.85 | Fail 2 |

Only `serde-saphyr` passes. It adds 19 crates. The `sh`, `git` and
`script` features add no crate. `macros` adds only `smop-macros`.

## Decision

smop has its own YAML parser and writer in `src/yaml/`. The feature adds no
crate: `yaml = ["fs", "serde"]`. smop itself then meets the three
criteria.

- API: `fs::read_yaml`, `fs::write_yaml`, and `smop::yaml::{from_str,
  to_string, Value}`. `Value` is for documents without a type.
- Schema: YAML 1.2 core. `on`, `yes` and `no` are strings. A plain scalar
  into `String` gives its text, so `python: 3.10` gives `"3.10"`.
- Supported: block and flow collections; plain, quoted and block scalars
  (`|`, `>`, chomping and indentation indicators); comments; `---` and
  `...`; `%YAML`; anchors, aliases and `<<` merge keys; tags `!`, `!!str`,
  `!!int`, `!!float`, `!!bool`, `!!null`, `!!seq` and `!!map`.
- Errors, each with line and column: a second document, `?` keys, a
  collection or an alias as a key, an anchor or tag before an implicit key,
  a key-value pair in a flow sequence, directives other than `%YAML`, tags
  other than the core tags, duplicate keys, a tab in indentation, nesting
  deeper than 128, and aliases that copy more than 1,000,000 nodes.
- A null value into a sequence, map or struct gives an empty one. So
  `push:` with no value reads as an empty struct.
- Enums: a scalar is a unit variant. A mapping with one key is any variant.
- The writer uses block style with 2 spaces. It quotes a string that a
  YAML 1.1 reader reads as another type (`yes`, `off`, `012`). A string with
  `\n` line breaks and no control characters is a literal block scalar.
  Other strings that need quotes are double-quoted. A duplicate key in a
  mapping is an error.

## Consequences

### Positive

- No new crate and no `unsafe`. smop sets the MSRV.
- Errors give the line and column. `fs::read_yaml` adds the path.
- A writer round trip is a proptest: `from_str(to_string(v)) == v`.

### Negative

- smop maintains a parser. The supported subset is smaller than the spec.
  Other input gives an error, not a wrong value.
- No multi-document streams, no custom tags, no complex keys.
- A top-level block scalar can have text at column 0, as YAML 1.2 permits.
  libyaml does not permit it.
- serde buffers `flatten`, untagged and internally tagged types. In these
  types a plain `3.10` is a float, not a string. Their errors can give the
  position of the enclosing node.
- The writer does not limit a key to 1024 characters. A YAML 1.1 reader
  can reject a longer key.
