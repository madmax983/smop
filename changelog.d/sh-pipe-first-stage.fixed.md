- **`CommandBuilder::pipe` keeps the first stage**: its `.dir()`, `.env()`
  and `.env_remove()` now apply. Before, `pipe` kept only the program and
  the arguments.
