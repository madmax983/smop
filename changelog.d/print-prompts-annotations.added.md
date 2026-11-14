- `print`: `info!` and `step!` macros. They write a styled line to stderr.
- `print::select`, `print::multiselect` and `print::password` prompts.
  `select` returns the index of the item, `multiselect` the indexes.
- `print::ci_error`, `print::ci_warning` and `print::group`. When
  `GITHUB_ACTIONS` is `true`, they write the workflow commands `::error::`,
  `::warning::` and `::group::`/`::endgroup::` to stderr. Elsewhere they
  write the `error!`, `warn!` or `step!` line. `group` runs a closure and
  ends the group also after a panic (ADR 0019).
