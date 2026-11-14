- `print` prompts do not wait if stdin or stderr is not a terminal.
  `confirm_default` and `prompt_default` return their default. Before,
  they returned an error if stderr was not a terminal. `confirm`, `prompt`
  and the new prompts return an error that names the prompt and the
  stream. Before, if stdin was not a terminal, they read keys from
  `/dev/tty`. To get an answer while stdin is a pipe, use a flag or an
  environment variable.
