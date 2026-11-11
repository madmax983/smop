- **`env::var_or` returns `Result<T>`** (#36). The default applies only when
  the variable is unset or empty. A value that does not parse is an error
  (before, `PORT=808O` gave the default). A value that is not valid UTF-8 is
  an error (before, it gave the default). Migration: add `?` to each call,
  `env::var_or("TIMEOUT", 30)?`.
