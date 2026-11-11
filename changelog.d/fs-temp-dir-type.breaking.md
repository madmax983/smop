- **`fs::TempDir` is a smop type**, not a re-export of `tempfile::TempDir`.
  `fs::temp_dir()` returns it. It has `new()`, `with_prefix_in(prefix, dir)`,
  `path()`, `keep()`, `close()`, `AsRef<Path>` and `Debug`. Drop removes the
  directory. The constructors and `close()` return `anyhow::Result`, not
  `io::Result`. Migration: `?` in a function that returns `anyhow::Result`
  needs no change. For other tempfile items (for example
  `tempfile::Builder`), add tempfile and use its `TempDir`.
