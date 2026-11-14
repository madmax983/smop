- `print` prompts do not wait when stdin or stderr is not a terminal.
  Before, with stdin redirected and a terminal on stderr, a prompt read
  from `/dev/tty` and waited. Now `confirm_default` and `prompt_default`
  return their default, and `confirm`, `prompt` and the other prompts
  return an error that names the prompt and the stream. Before,
  `confirm_default` and `prompt_default` returned an error when stderr was
  not a terminal.
