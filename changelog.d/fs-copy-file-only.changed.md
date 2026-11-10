- `fs::copy` is documented as file-only. Before, the docs said "a file or
  directory", but a directory always failed. A directory now gives an error
  that names `fs::copy_dir` (#37).
