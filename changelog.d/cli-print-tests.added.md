- Tests for `cli` (`Parser`, `Args`, `Subcommand` and `ValueEnum` derives
  through the prelude, including the errors for bad input) and for `print`
  (status lines, JSON serialization errors, and prompts without a terminal,
  which fail instead of waiting). `print.rs` is covered at 98%, up from 68%.
