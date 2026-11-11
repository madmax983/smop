- **`CommandBuilder::pipe` keeps the first stage**: its `.dir()`, `.env()`,
  `.env_remove()` and `.echo()` now apply. With `.echo()`, the chain prints
  `$ a | b`. Before, `pipe` kept only the program and the arguments.
