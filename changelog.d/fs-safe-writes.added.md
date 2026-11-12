- Safe writes for release scripts:
  - `fs::write_atomic(path, content)`: writes a temp file in the same
    directory, then renames it. A failed write leaves the target and no
    temp file. An existing file keeps its permissions. A symlink stays.
  - `sh::CommandBuilder::stdout_atomic(path)`: `cmd > tmp && mv tmp path`.
    Only a successful command replaces the file. An interrupt or a kill
    stops the rename.
  - `fs::write_with_parents(path, content)`: creates the missing parent
    directories, then writes. The other `write_*` functions still fail on
    a missing directory.
  - `fs::replace_in_file(path, from, to)`: replaces each match, writes
    atomically and returns the count. No match is an error that names the
    path and the pattern, and the file does not change.
