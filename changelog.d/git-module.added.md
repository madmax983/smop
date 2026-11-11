- `git` feature (in `full`, no new crates, implies `sh`): `git::head`,
  `git::branch` (`None` on a detached HEAD), `git::is_clean`,
  `git::is_clean_paths`, `git::changed_since(rev)`, `git::ls_files`,
  `git::show(rev, path)` and `git::is_ignored(path)`. `git::Repo::new(dir)`
  has the same queries for another directory. Result paths are sorted and
  relative to the repository root. `changed_since` includes uncommitted
  changes, untracked files and changed submodules. A failed git command
  gives a `sh::CommandError` with the stderr of git. Some queries can run
  programs that the repository config names: do not query a repository
  that you do not trust (ADR 0014).
