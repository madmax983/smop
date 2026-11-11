- `macros` feature (in `full`, adds `smop-macros`, a proc-macro with no
  dependencies): `cmd!("git commit -m {msg} -- {files...}")` makes a
  `sh::CommandBuilder` from a command line template. No shell runs. The
  macro splits the template into words at compile time, so a value is never
  split and never read as shell syntax. `{name}` is exactly one argument:
  an `AsRef<OsStr>` value, an integer or a `char`. `{name...}` is one
  argument for each item of a list. `'...'` and `"..."` hold text with
  spaces, and `{{` and `}}` are braces. `name = expr` after the template
  gives a name to an expression. Shell syntax outside quotes (`&&`, `|`,
  `>`, `$HOME`, `~`), a splat of a path and whitespace that is not ASCII
  are compile errors. `cmd!` is in the prelude. `sh` alone keeps no
  dependencies.
