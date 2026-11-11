- **`sh` command errors**:
  - The error of `output()` also holds the captured stdout
    (`CommandError::stdout()`).
  - A quiet `run()` puts the last 4 KiB of stderr in its error, so
    `CommandError::stderr()` is `Some` for it.
  - The error text does not end with an empty line when stderr is empty.
  - `pipe()` gives an error for I/O options such as `stdin`, `timeout` or
    `keep_output()`. `spawn()` accepts each I/O option (#45).
