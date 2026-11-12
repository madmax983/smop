# ADR 0015: Atomic Writes

## Status

Accepted (0.4.0).

## Context

Release scripts write generated files and bump versions. A crash or a
failed generator must not leave a half-written file. Harvest's
`regenerate_openapi` writes `openapi.json.tmp` from the stdout of a child
and renames it. Its Rhai host makes the parent directory by hand before
each write. A version bump must fail when the pattern is stale.

## Decision

- `fs::write_atomic(path, content)`, `fs::write_with_parents(path,
  content)` and `fs::replace_in_file(path, from, to) -> Result<usize>`.
  `sh::CommandBuilder::stdout_atomic(path)` is the stdout variant.
- One private module, `src/atomic.rs`, does the replace for `fs` and `sh`.
  It uses std only. Thus `sh` still adds no crate, and `fs` does not need
  `tempfile` for this.
- The replace:
  1. Make a new file (`create_new`) in the directory of the target. The
     name is `.<name>.<pid>.<counter>.<nanos>.tmp`. A target name longer
     than 64 bytes is not in it, so the name is not too long.
     On Unix, it gets the mode bits of the existing target less the
     umask. Thus other users can not read the new content of a private
     file. For a new target, it gets the mode of `std::fs::write` (0o666
     less the umask). `tempfile` gives 0o600, which is wrong for a
     generated file.
  2. Write it.
  3. Give it the permissions of the existing target.
  4. `sync_all`, close, then `rename`. On Unix, `sync_all` the directory
     and ignore its error: the rename is done.
  5. On an error, a drop removes the temp file. The target does not
     change.
- A symlink target is resolved. smop replaces the file that the link
  points to, and the link stays. A rename on the link would change a
  dotfile link into a plain file. A dangling link is an error.
- The owner of the file is not kept. Only root can keep it, and std has
  `chown` only on Unix.
- `write_string`, `write_bytes` and `write_atomic` do not create parent
  directories. A wrong path must give an error. `write_with_parents` is a
  separate name, so the call shows the intent. It is a plain write, not an
  atomic one: one function does one job.
- `replace_in_file` reads UTF-8 text and replaces plain text, as
  `str::replace` does. Matches do not overlap. Zero matches and an empty
  pattern are errors that name the path and the pattern. Then smop does not
  write, so the file stays byte-identical. A regex variant can use
  `smop::re` and `write_atomic`.
- `stdout_atomic` renames only after a full success: exit code 0, no
  timeout and no output error. `capture`, `status` and `success` give `Ok`
  for a non-zero exit, but they do not rename. Every run method and
  `ChildProcess::wait` go through `Running::finish`, so the rule is in one
  place. A pipe rejects the option, as it rejects all I/O options.
- An interrupt that a guard caught, or `ChildProcess::kill`, stops the
  rename and gives an error, also when the command exits 0. A generator
  that traps Ctrl-C and exits 0 must not replace the file. A dropped child
  does not rename.
- The command writes the temp file directly, as with `stdout_file`. A
  background process that it starts can write after the rename, as with
  `cmd > tmp && mv tmp path`. A pipe and a reader thread would find this,
  but they cost a copy of all output.

## Options considered

- `tempfile::NamedTempFile::persist`: mode 0o600 for new files, and `sh`
  would need a crate.
- Create parent directories in `write_*` by default: hides a wrong path.
- A function in `fs` that takes a command: puts `sh` into `fs`. The
  redirection already has all I/O options.
- A Verus spec: this module is OS I/O. Tests and a proptest cover it (see
  ADR 0012).

## Consequences

- A reader sees the old or the new content, never a part. A test reads the
  file in a loop while another thread replaces it.
- An existing read-only file can be replaced on Unix, because a rename
  needs only a writable directory. On Windows, a rename onto a read-only
  file fails.
- A write needs a writable directory, not only a writable file.
- A crash between steps 1 and 5 can leave a `.*.tmp` file.
