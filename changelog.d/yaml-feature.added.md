- `yaml` feature (in `full`): `fs::read_yaml`, `fs::write_yaml` and
  `smop::yaml::{from_str, to_string, Value}`. smop has its own YAML 1.2
  parser and writer, so the feature adds no crate (ADR 0019). `on`, `yes`
  and `no` are strings. Errors give the line and the column.
  `fs::read_yaml` adds the path. The parser supports anchors, aliases, `<<`
  merge keys and block scalars.
