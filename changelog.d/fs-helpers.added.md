- `fs` helpers (#37), from std only (no `walkdir`):
  - `fs::copy_dir(from, to)`: copies a directory tree, with empty
    directories and directory permissions. `to` must not exist. Symlinks are
    copied as links. A failed copy removes what it made.
  - `fs::move_path(from, to)`: `rename`, then copy and remove when the paths
    are on different file systems, as `shutil.move` does. A failed copy is
    removed and `from` stays.
  - `fs::remove_if_exists(path)`: `Ok` on a missing path. Other errors, such
    as a permission error, still fail. It removes a symlink, not its target.
  - `fs::read_bytes` and `fs::write_bytes`, for data that is not UTF-8.
  - `fs::walk(dir)`: all files below `dir`, sorted by path. It does not
    follow symlinks to directories, so a symlink cycle can't make a loop.
