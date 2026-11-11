- `macros` feature (in `full`, adds `smop-macros`, a proc-macro with no
  dependencies): `cmd!("git commit -m {msg} -- {files...}")` makes a
  `sh::CommandBuilder` from a command template. No shell runs. The macro
  splits the template into words at compile time. It never splits a value
  and never reads a value as shell syntax. `{name}` is exactly one
  argument: an `AsRef<OsStr>` value, an integer or a `char`. `{name...}` is
  one argument for each item of a list. `'...'` and `"..."` hold text with
  spaces, and `{{` and `}}` are braces. `name = expr` after the template
  gives a name to an expression. These are compile errors: shell syntax
  outside quotes (`&&`, `|`, `>`, `$HOME`, `~`), `{name...}` with a path,
  with text or with a `Result`, and a no-break space or other unusual
  whitespace outside quotes. The type of each value must be known at the
  call. `cmd!` is in the prelude. `sh` alone still adds no crate.
