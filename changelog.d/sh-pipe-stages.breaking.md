- **`pipe` takes a `CommandBuilder`** (#43). `CommandBuilder::pipe` and
  `PipeBuilder::pipe` take the next stage as a builder. Each stage keeps its
  `.dir()`, `.env()`, `.env_remove()` and `.echo()`. Arguments go to the
  program byte for byte, so a non-UTF-8 argument is not changed. Before,
  only the first stage kept its settings, and later stages took `&[&str]`.
  When a stage has `.echo()`, the chain prints `$ a | b`. When a stage has
  `.cleanup()`, the chain runs after an interrupt. Migration:
  `.pipe("grep", &["-v", "x"])` becomes `.pipe(sh::cmd("grep").args(["-v", "x"]))`.
