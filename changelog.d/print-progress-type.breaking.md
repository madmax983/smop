- **`print::progress()` returns a `print::Progress`**, not indicatif's
  `ProgressBar`. It has `inc`, `set_position`, `position`, `set_message`,
  `finish`, `finish_with_message`, `Clone` and `Debug`. Migration: code that
  uses only these methods needs no change. For other `ProgressBar` methods,
  add indicatif and make your own `ProgressBar`.
