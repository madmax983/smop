# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Changes that are not released yet are in [changelog.d/](changelog.d/), one
file for each change. `python3 scripts/changelog.py preview` shows them.

## [0.4.0] - 2026-11-13

Scripts that don't fail silently. 0.4 removes the places where smop hid a
failure: lenient `env::var_or`, pipes that checked only the last stage,
archive entries skipped without an error, and `spawn()` pipes that no one
read. It adds the `cmd!` macro (`macros` feature, ADR 0007), shell-style
exit codes, process I/O for `sh`, the `script`, `git`, `re` and `hash`
modules, and more `fs` helpers. The tracking issue is
[madmax983/smop#31](https://github.com/madmax983/smop/issues/31).

`smop-macros` 0.4.0 is new. Do not depend on it: enable the `macros`
feature of `smop`, which pins it with `=0.4.0`.

#### Upgrading from 0.3

The compiler shows these breaks. Each entry under Breaking has a migration
snippet:

- `env::var_or` returns `Result<T>`: add `?`.
- `pipe` takes a `CommandBuilder`: `.pipe(sh::cmd("grep").arg("x"))`.
- `http::Client::timeout` takes a `Duration`.
- `print::progress()` returns `print::Progress`, and `fs::TempDir` is a
  smop type.
- `sh` spawn failures are a `sh::CommandError`: `downcast_ref::<io::Error>()`
  gives `None`; use `sh::exit_code(&err) == Some(127)`.
- MSRV 1.89.

The compiler does not show these. The code builds, and the behaviour
changes:

- Pipes fail when any stage fails (pipefail).
- `spawn()` gives the child the terminal's stdout and stderr.
- `archive::extract_*` return an error for an unsafe entry.
- `env::var_or` gives the default for an empty value.
- Windows: `sh::run` and `sh::output` give the line to `cmd /C` as is.
  Unix: they use `/bin/sh`.

### Breaking
- **`archive::extract_zip`, `extract_tar` and `extract_tar_gz` return an
  error for an unsafe entry** (#35), and the error names it. Before, they
  skipped it and returned `Ok`. This is a change in behaviour only, so the
  compiler does not show it. One policy for all formats (ADR 0008):
  - An absolute path (`/a.txt`) is an error. Before, it extracted to
    `dest/a.txt`.
  - `a/../b.txt` extracts to `dest/b.txt`. Before, tar skipped it.
  - `../b.txt` is an error.
  - A link to a path outside `dest`, or to an absolute path, is an error,
    and the link does not stay on disk. The target must stay in `dest` also
    by name: `l/../../x` is an error even if `l` is a link to `a/b`.
  - No write follows a link out of `dest`. This includes links that were in
    `dest` before the extraction. A file entry replaces a link at its path.
  - On Windows, a name with `:` is an error. Before, `extract_zip` skipped
    it. A device name such as `CON` now extracts as a normal file.
  - ZIP link entries become links. Before, they became files that held the
    target path. On Windows, to make a link can need Developer Mode.
  - A tar hard link to a link becomes a copy of the link. A tar hard link to
    itself does nothing. Before, it gave an error.

  Migration: for an archive from an unknown source, handle the error. If
  `dest` has links with absolute targets, make them relative or extract to
  a new directory. To extract absolute entries, use the `zip` or `tar`
  crate.

  ```rust
  // 0.3: an unsafe entry was skipped, and this gave Ok.
  // 0.4: the error names the entry.
  if let Err(err) = archive::extract_tar_gz("upload.tar.gz", "out") {
      warn!("rejected upload.tar.gz: {err:#}");
  }
  ```
- **`env::var_or` gives the default for an empty value** (#36). This is a
  change in behaviour only. It applies to a type that parses `""`, such as
  `String` or `PathBuf`: before, `var_or("NAME", "x".to_string())` with
  `NAME=` gave `""`. Migration: to keep the old behaviour, use
  `std::env::var("NAME").unwrap_or_else(|_| "x".to_string())`.

  ```rust
  // NAME= (set, empty)
  let name: String = env::var_or("NAME", "x".to_string())?; // 0.4: "x" (0.3: "")
  let name = std::env::var("NAME").unwrap_or_else(|_| "x".to_string()); // "", as 0.3
  ```
- **`env::var_or` returns `Result<T>`** (#36). The default applies only when
  the variable is unset or empty. A value that does not parse is an error
  (before, `PORT=808O` gave the default). A value that is not valid UTF-8 is
  an error (before, it gave the default). Migration: add `?` to each call.

  ```rust
  let timeout: u32 = env::var_or("TIMEOUT", 30);  // 0.3
  let timeout: u32 = env::var_or("TIMEOUT", 30)?; // 0.4
  ```
- **`fs::TempDir` is a smop type**, not a re-export of `tempfile::TempDir`.
  `fs::temp_dir()` returns it. It has `new()`, `with_prefix_in(prefix, dir)`,
  `path()`, `keep()`, `close()`, `AsRef<Path>` and `Debug`. Drop removes the
  directory. The constructors and `close()` return `anyhow::Result`, not
  `io::Result`. Migration: `?` in a function that returns `anyhow::Result`
  needs no change. For other tempfile items (for example
  `tempfile::Builder`), add tempfile and use its `TempDir`.

  ```rust
  let dir: smop::fs::TempDir = fs::temp_dir()?; // 0.3: tempfile::TempDir
  let kept: std::path::PathBuf = dir.keep();     // as tempfile's keep()
  ```
- **`http::Client::timeout` takes a `Duration`**, not seconds as a `u64`.
  `Duration::ZERO` makes every request fail, as `.timeout(0)` did. A limit
  above 100 years, such as `Duration::MAX`, sets no limit. Before,
  `.timeout(u64::MAX)` panicked. Migration:

  ```rust
  let client = http::Client::new().timeout(30);                      // 0.3
  let client = http::Client::new().timeout(Duration::from_secs(30)); // 0.4
  ```
- **MSRV is now Rust 1.89** (#33), for `std::fs::File::lock`. Migration:
  build with Rust 1.89 or later.

  ```bash
  rustup update stable
  ```
- **`print::progress()` returns a `print::Progress`**, not indicatif's
  `ProgressBar`. It has `inc`, `set_position`, `position`, `set_message`,
  `finish`, `finish_with_message`, `Clone` and `Debug`. Migration: code that
  uses only these methods needs no change. For other `ProgressBar` methods,
  add indicatif and make your own `ProgressBar`.

  ```rust
  let bar: print::Progress = print::progress(100); // 0.3: indicatif::ProgressBar
  bar.inc(1);
  bar.finish_with_message("done");
  ```
- **Pipes fail when any stage fails (pipefail)** (#43). This is a change
  in behaviour only, so the compiler does not show it. Before, only the
  last stage counted: `false` piped into `cat` gave `Ok`. Now the error
  holds the `CommandError` of the rightmost failed stage, so
  `sh::exit_code` gives its code, as bash `pipefail` does. When two or more
  stages fail, the error text names each one. One difference from bash: on
  Unix, a stage before the last that `SIGPIPE` stops is a success, so `yes`
  piped into `head -n1` is `Ok`. A stage that exits with an error code on
  a closed pipe is still a failure. Migration: a pipe that must succeed
  when a stage fails needs `|| true` in that stage, for example
  `sh::shell("grep x file || true")`. `grep` with no match exits with 1.

  ```rust
  // 0.3: Ok, because only `wc` counted. 0.4: Err, exit code 1 from grep.
  let n = sh::cmd("grep").args(["x", "file"]).pipe(sh::cmd("wc").arg("-l")).output();
  // 0.4: keep going when grep finds nothing.
  let n = sh::shell("grep x file || true").pipe(sh::cmd("wc").arg("-l")).output()?;
  ```
- **`pipe` waits for each stage** (#43). Before, it waited for the last
  stage only, and left the others as zombies or still running. When a
  stage does not start, smop stops the stages that started.
- **Earlier pipe stages send stderr to the terminal** (#43). Before,
  `output()` gave each earlier stage a stderr pipe that no one read. The
  first line to stderr stopped that stage with `SIGPIPE`. The pipe then
  gave `Ok` with output cut short. The first stage still gets the stdin of
  this process.
- **`pipe` takes a `CommandBuilder`** (#43). `CommandBuilder::pipe` and
  `PipeBuilder::pipe` take the next stage as a builder. Each stage keeps its
  `.dir()`, `.env()`, `.env_remove()` and `.echo()`. Arguments go to the
  program byte for byte, so a non-UTF-8 argument is not changed. Before,
  each stage kept only its program and arguments, and later stages took
  `&[&str]`.
  When a stage has `.echo()`, the chain prints `$ a | b`. When a stage has
  `.cleanup()`, the chain runs after an interrupt. Migration:

  ```rust
  sh::cmd("ls").pipe("grep", &["-v", "x"]).run()?;                 // 0.3
  sh::cmd("ls").pipe(sh::cmd("grep").args(["-v", "x"])).run()?;   // 0.4
  ```
- **Windows: `sh::run` and `sh::output` give the line to `cmd /C` with no
  C runtime quoting** (`raw_arg`). This is a change in behaviour only, so
  the compiler does not show it. Before, the quoting changed lines that
  have quotes, so `run(r#"echo "a  b""#)` printed `\"a  b\"`. Migration: if
  a line has `\"` to get past the old quoting, remove the backslashes.

  ```rust
  sh::run(r#"findstr \"a b\" log.txt"#)?; // 0.3, Windows
  sh::run(r#"findstr "a b" log.txt"#)?;   // 0.4
  ```
- **Unix: `sh::run` and `sh::output` use `/bin/sh`**, not `sh` on `PATH`.
  Migration: to run another shell, name it,
  `sh::cmd("bash").args(["-c", line]).run()?`.
- **`sh` spawn failures are a `sh::CommandError`** (#34), with the
  `io::Error` as its source. `err.downcast_ref::<io::Error>()` now gives
  `None`. The error text is the same. Migration: use
  `sh::exit_code`, or search `err.chain()` for the `io::Error`.

  ```rust
  let err = sh::cmd("no-such-tool").run().unwrap_err();
  let missing = err.downcast_ref::<std::io::Error>().is_some();   // 0.3
  let missing = sh::exit_code(&err) == Some(127);                  // 0.4
  ```
- **`sh` `spawn()` gives the child the stdout and stderr of this process**
  (#45). This is a change in behaviour only, so the compiler does not show
  it. Before, it piped both streams and no one read them, so a child
  that wrote more than 64 KiB blocked. Stdin is still empty, unless
  `stdin()` sets it. Migration: add `.keep_output()` and call
  `ChildProcess::wait_with_output()` to get the output as `sh::Captured`,
  or add `.quiet()` to discard it.

  ```rust
  let job = sh::cmd("make").keep_output().spawn()?; // 0.4: capture it
  let out: sh::Captured = job.wait_with_output()?;
  let job = sh::cmd("make").quiet().spawn()?;       // 0.4: discard it
  job.wait()?;
  ```
- **`sh` `spawn()` honours each I/O option** (#45): `stdin`, `tee`,
  `timeout`, the log files and the redirections. Before, these gave an
  error. The timeout starts at the spawn. Only `wait()` and
  `wait_with_output()` apply it. A pipe stage with `keep_output` gives an
  error.
- **A quiet `spawn()` keeps the last 4 KiB of stderr** (#45), as a quiet
  `run()` does. A thread reads stderr, and the error of `wait()` holds the
  tail. Before, stderr went to the null device.
- **`ChildProcess` is `#[must_use]`** (#45), because drop does not stop the
  child. Migration: call `.wait()` or `.kill()`, or bind it with
  `let _child = ...`.

### Added
- A proptest for archive extraction (#35, Unix only): for generated
  entries, every path in `dest` resolves in `dest`, and nothing outside it
  changes.
- CI quality gates (#32):
  - a line-coverage gate: `cargo llvm-cov --workspace --all-features
    --fail-under-lines 85`, the low end of the project's 85–90% standard
    (91.5% today)
  - `cargo-semver-checks` against the latest smop on crates.io
- Tests for `cli` (`Parser`, `Args`, `Subcommand` and `ValueEnum` derives
  through the prelude, including the errors for bad input) and for `print`
  (status lines, JSON serialization errors, and prompts without a terminal,
  which fail instead of waiting). `print.rs` is covered at 98%, up from 68%.
- `macros` feature (in `full`, adds `smop-macros`, a proc-macro with no
  dependencies): `cmd!("git commit -m {msg} -- {files...}")` makes a
  `sh::CommandBuilder` from a command template. No shell runs. The macro
  splits the template into words at compile time. It never splits a value
  and never reads a value as shell syntax. `{name}` is exactly one
  argument: an `AsRef<OsStr>` value, an integer or a `char`. `{name...}` is
  one argument for each item of a list. `'...'` and `"..."` hold text with
  spaces, and `{{` and `}}` are braces. `name = expr` after the template
  gives a name to an expression. These are compile errors: shell syntax
  outside quotes (`&&`, `|`, `>`, `$HOME`, `~`), `{name...}` with a path,
  with text or with a `Result`, and a no-break space or other unusual
  whitespace outside quotes. The type of each value must be known at the
  call. `cmd!` is in the prelude. `sh` alone still adds no crate.
- `env::is_ci()` (#36): `true` when `CI` is set and is not `false` or `0`.
  When `CI` is unset or empty, it reads `TF_BUILD`, `JENKINS_URL` with
  `BUILD_ID`, `TEAMCITY_VERSION` and `CODEBUILD_BUILD_ID`.
- `env::var_opt::<T>(name) -> Result<Option<T>>` (#36): `None` when the
  variable is unset or empty, an error when it does not parse or is not
  valid UTF-8.
- Three examples: `env_report` (strict `env` parsing, JSON and TOML output),
  `shell_automation` (`cmd!`, `script::main`, `cargo metadata`) and
  `archive_backup` (`.tar.gz` backup, restore and check). README lists each
  example. CI builds each example with only its `required-features`
  (`scripts/examples.sh`).
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
- `fs::lock(path, timeout)` and `fs::lock_shared(path, timeout)` (#33):
  file locks that work together with `flock(1)`, held by a `fs::FileLock`
  guard until it drops. While another holder has the lock they retry every
  50 ms until `timeout` (`Duration::ZERO` tries once, `Duration::MAX` waits
  indefinitely), then fail naming the path and the timeout. Like `flock(1)`,
  a missing path is created as an empty file, and an exclusive lock opens
  the file for writing when it can (NFS needs that). On Unix the lock is advisory
  and a directory can be locked too. On Windows the lock is mandatory (other
  handles can't write the file, or read it under an exclusive lock) and
  locking a directory is an error, so cross-platform code locks a dedicated
  lock file.
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
- `hash` feature (in `full`): `hash::sha256(bytes)`, `hash::sha256_file(path)`
  and `hash::verify_sha256_file(path, expected)` give SHA-256 as lowercase
  hex. `sha256_file` reads through one 64 KiB buffer, so memory use does not
  change with the file size. Errors name the path. A mismatch error shows
  both digests. Adds `sha2` (8 crates; 9 on aarch64 Linux and Apple).
- `re` feature (in `full`): `smop::re` re-exports all of `regex` and adds
  helpers that take the pattern as `&str`: `compile`, `is_match`, `find`,
  `find_all`, `captures`, `captures_all`, `replace`, `replace_all` and
  `split`. An invalid pattern gives `Invalid regex pattern '<pattern>'`;
  a pattern that is too big gives `Regex pattern '<pattern>' is too big`.
  Each helper compiles its pattern on each call. In a loop, use `compile`.
- Git tags `v0.1.0` to `v0.3.2`, each on the commit that the crate on
  crates.io was published from. `scripts/tag_release.sh` makes the tag for a
  new release. `CHANGELOG.md` has a compare link for each release, and
  `scripts/changelog.py` writes it and checks it.
- `script` feature (in `full`, no dependencies): `script::main(|| ..)` prints
  the error chain (`error:`, then `caused by:` lines) and exits with the
  code of a failed `sh` command, or with 1. `script::Main::error_code`
  sets the code for an error that has no code. `script::defer(cmd)` runs a
  cleanup command on drop.
  `script::retry(attempts, backoff, f)` waits `backoff`, `2 × backoff`, …
  between attempts. `script::timed(label, f)` prints `<label>: <secs>s`.
- `script-signals` feature (in `full`, adds `signal-hook`): `script::guard()`
  catches Ctrl-C, SIGTERM and SIGHUP while it lives. After an interrupt,
  `sh` starts no new command and gives `script::Interrupted`. When the
  script function returns, its `defer` commands run, then `script::main`
  stops by the same signal (Unix) or exits with 130 (Windows). A second
  signal gets its default action. The handlers stay installed, so a signal
  ignored at start (`nohup`) is caught, and later gets its default action.
- `sh::CommandBuilder::cleanup()`: the command also runs after an
  interrupt. `sh::exit_code` gives 128 + N (130 on Windows) for
  `script::Interrupted`.
- `ChildProcess` implements `Debug`.
- **Shell-style exit codes in command errors** (#34): `sh::CommandError` and
  `sh::exit_code(&err) -> Option<u8>`. Every `sh` error for a command that
  does not start or that fails holds a `CommandError`, with the command line,
  the code and the signal. The code follows bash:
  - 127: the program is not found (on Unix, a missing `.dir()` directory
    also gives 127)
  - 126: the program does not start (for example, it is not executable)
  - 128 + N: signal N stops the process (Unix)
  - Other failures: the exit code of the process

  `exit_code` searches the error chain, so it works below `.context()`
  layers. It changes a code outside 1..=255 to 1, so a failure never exits
  with 0. This is smop's first public error type. Signatures stay
  `anyhow::Result` (ADR 0001 has an amendment). Accessors: `command()`,
  `code()`, `signal()` and `stderr()` (captured by `output()` calls only).
- **`sh` process I/O**. New `CommandBuilder` methods:
  - `capture() -> Captured`: the status, the shell-style `code() -> u8`, and
    stdout and stderr as text. smop changes bytes that are not UTF-8 to
    U+FFFD. A non-zero exit is not an error.
  - `output_lines()`: the lines of stdout (`\n` or `\r\n`).
  - `tee()`: shows output live while smop captures it.
  - `log_stdout(path)` and `log_stderr(path)`: copy a stream to a file.
  - `stdout_file(path)` (`>`), `stdout_append(path)` (`>>`),
    `stderr_to_stdout()` (`2>&1`) and `stdout_to_stderr()` (`>&2`).
  - `stdin(text or bytes)`, `quiet()` and `timeout(Duration)`.

  Threads read stdout and stderr while smop waits, so a child that writes
  much to both streams does not stop. A timeout kills the child, reaps it,
  and gives an error with code 124 that holds the output captured before.
  `sh::CommandError` adds `stdout()` and `timed_out()`. See ADR 0012.
- **`sh::shell(line) -> CommandBuilder`**: runs `line` through `/bin/sh -c`
  (Unix) or `cmd /C` (Windows), so `.dir()`, `.env()` and `.echo()` work on
  a shell line. `sh::run` and `sh::output` keep their signatures and wrap it.
  On Unix, extra `.arg()` values are `$0`, `$1`, and so on. The shell does
  not parse them. On Windows, they go after the line, and cmd.exe parses
  them.
- `sh::CommandBuilder::spawn_guarded()` returns a `sh::ChildGuard` (#45).
  The child gets its own process group. Drop of the guard kills the group
  and reaps the child, also while a panic unwinds. `wait()` and
  `wait_with_output()` kill nothing. No new crate and no `unsafe` code
  (ADR 0013).
- `sh::CommandBuilder::keep_output()` and
  `sh::ChildProcess::wait_with_output()`, which returns `sh::Captured`
  (#45).
- `sh::ChildProcess::id()` and `sh::ChildGuard::id()` (#45).
- `sh::wait_until(timeout, || probe)` calls a probe until it returns
  `true`. The error names the timeout. It stops on an interrupt (#45).
- `smop::toml` re-exports the `toml` crate (`toml` feature), so a script
  needs no direct `toml` dependency.

### Changed
- CI checks the public API for types of dependencies that smop does not
  re-export (`scripts/api_boundary.sh`). The policy is in CLAUDE.md and in
  an amendment to ADR 0004.
- `Cargo.toml` declares a workspace, and the lint table moved to
  `[workspace.lints]`, so `smop-macros` (ADR 0007) can join and share it.
  CI passes `--workspace`, and its per-feature loops read the feature list
  from `cargo metadata` (`scripts/ci-features.sh`) instead of a hand-written
  copy.
- `env` parse errors name the value (#36):
  `Failed to parse environment variable PORT="808O"`. Before:
  `Failed to parse environment variable PORT`.
- `fs::copy` is documented as file-only. Before, the docs said "a file or
  directory", but a directory always failed. A directory now gives an error
  that names `fs::copy_dir` (#37).
- `fs::copy(a, a)`, or a copy onto a hard link of `a`, is an error. Before,
  it returned `Ok` and emptied the file (#37).
- `hash::sha256_file` sizes its read buffer to the file (at most 64 KiB) instead of zeroing 64 KiB for each file. Digests are the same.
- A timeout while smop reads the response body gives the same error as a
  timeout before the headers: `HTTP request to <url> timed out: Global`.
  This includes `download`.
- A failed `download` removes the partial file.
- `sh::CommandBuilder` is `#[must_use]`. A builder that no method starts,
  such as `cmd!("make");` or a builder that a `match` or `if` gives, is an
  `unused_must_use` warning. Before, only a direct call such as
  `sh::cmd("make");` gave the warning.
- **`sh` command errors**:
  - The error of `output()` also holds the captured stdout
    (`CommandError::stdout()`).
  - A quiet `run()` puts the last 4 KiB of stderr in its error, so
    `CommandError::stderr()` is `Some` for it.
  - The error text does not end with an empty line when stderr is empty.
  - `pipe()` gives an error for I/O options such as `stdin`, `timeout` or
    `keep_output()`. `spawn()` accepts each I/O option (#45).
- Error text for failed commands. Errors that said "exit code N" still do.
  - Death by signal: `Command killed by signal 9 (exit code 137): <command>`.
    Before: `exit code -1`.
  - `PipeBuilder` errors name the full command line and code of the
    rightmost stage that failed:
    `Command failed with exit code 1: grep -q "no match"`. When two or more
    stages fail, the text starts with `Pipe failed in N stages:` and names
    each stage and its code (#43). Before: `output` gave
    `Pipe command failed: grep`, and `run` gave
    `Pipe command failed with exit code 1: grep`.
  - A pipe stage that does not start gives
    `Failed to execute command: <stage>`. Before: `Failed to spawn: <program>`
    or `Failed to execute: <program>`.
  - `ChildProcess::wait` errors name the command:
    `Command failed with exit code 5: <command>`. Before:
    `Child process exited with code 5`. If the wait itself fails:
    `Failed to wait for command: <command>`. Before:
    `Failed to wait for child process`.
  - `spawn()` failures say `Failed to execute command: <command>`, as the
    other paths do. Before: `Failed to spawn command: <command>`.
  - `ChildProcess::kill` and `try_wait` errors name the command:
    `Failed to kill command: <command>` and
    `Failed to check command: <command>`. Before:
    `Failed to kill child process` and
    `Failed to check child process status` (#45).

### Fixed
- `env::require_vars` reported a value that is not valid UTF-8 as missing
  (#36). Now it accepts the value.
- `env::var` reported a value that is not valid UTF-8 as "not set" (#36).
  Now the error is `Environment variable X is not valid UTF-8: "80\xFF80"`.

## [0.3.2] - 2026-11-03

### Added
- **Raw HTTP layer for status-aware API work.** `http::request(method, url)`
  and `Client::request(method, url)` build a `Request` (`header`, `body`,
  `json`) whose `send()` returns a `Response` for *every* status code. It
  errors only on transport failures. `Response` has `status()`,
  `is_success()`, `header(name)`, `headers()`, `bytes()`, `text()`,
  `into_text()`, `json::<T>()` (works on error bodies, e.g. a 422 payload),
  `url()` and `error_for_status()`.
- `smop::cli::clap` (also in the prelude), `smop::serde` and
  `smop::serde_json` re-exports. clap's derives now work with smop as the
  only dependency; serde derives work with `#[serde(crate = "smop::serde")]`.
- `tests/consumer/`: a crate depending only on smop, run in CI.

### Changed
- Typed HTTP helpers (`get`, `get_json`, `post_json`, …, and `Client`
  methods, plus `download`) now include the response body (up to 2000 bytes)
  in non-2xx errors: `HTTP request to <url> failed with status 422: {...}`.

### Fixed
- `cli` feature wasn't self-contained: clap's derive output names the `clap`
  crate, so consumers needed a direct `clap` dependency or nothing compiled.
- `Client::auth` sent `Basic user:pass` without base64-encoding it, so
  servers rejected the credentials. It now sends `Basic base64(user:pass)`.

## [0.3.1] - 2026-11-03

### Added
- `CommandBuilder::status()` - run with inherited stdio and return the
  `ExitStatus` instead of erroring on a non-zero exit, so a caller (e.g. an
  xtask wrapping a script) can pass the exact exit code through. Errors only
  if the program can't start, naming the command line.

## [0.3.0] - 2026-11-03

Every module is now behind its own feature, so smop can be used from a
`cargo xtask` (or any dependency-sensitive crate) without pulling in the
whole batteries-included set. See ADR 0006.

### Breaking
- **Core modules are feature-gated.** `sh`, `fs`, `path` and `env` are now
  features, as are the `fs` format helpers: `json` (`read_json`/`write_json`),
  `toml` (`read_toml`/`write_toml`) and `glob` (`fs::glob`). The prelude's
  `Serialize`/`Deserialize` re-export needs the `serde` feature (implied by
  `json`, `toml`, `csv`, `http`, `print`). `anyhow` is the only unconditional
  dependency. Default (`full`) users are unaffected; `default-features = false`
  users must add the modules they use, e.g.
  `features = ["sh", "fs", "path"]`.
- **MSRV is now Rust 1.88** (declared via `rust-version`), required by zip 9
  and comfy-table 8.
- `CommandBuilder` error messages changed: they now include the full command
  line (e.g. `Command failed with exit code 101: cargo test --workspace`).

### Added
- `sh::cargo()` - a `CommandBuilder` for `$CARGO` (falls back to `cargo`), so
  nested cargo calls from `cargo xtask` use the same toolchain.
- `CommandBuilder::echo()` - print `$ program args...` to stderr before running.
- `CommandBuilder::success()` - run quietly and return `Ok(bool)` for the exit
  status; errors only if the program can't start.
- `CommandBuilder::env_remove()`.
- `impl Display` / `impl Debug` for `CommandBuilder` (quoted command line).
- `path::workspace_root()` - root of the enclosing Cargo workspace via
  `cargo locate-project --workspace`.
- `fs::create_dir_all()` - `mkdir -p` with the path in the error.
- `examples/xtask.rs` - a minimal xtask using only `sh`, `fs` and `path`.
- docs.rs builds with all features and shows which feature each item needs.

### Changed
- Dependencies updated to their latest releases: zip 2 → 9, comfy-table 6 → 8,
  plus newer minimum versions for every other dependency (ureq 3.5, clap 4.6,
  indicatif 0.18.6, csv 1.4, toml 1.1, tempfile 3.27, which 8.0.6, ...).
- `dirs` removed: `path::home()` uses `std::env::home_dir` (fixed on Windows
  in Rust 1.85). `shellexpand` is used without default features, which also
  drops its own `dirs` dependency.
- `archive::extract_zip` detects directory entries with `ZipFile::is_dir()`.

## [0.2.1] - 2026-02-05

### Fixed
- Fixed unused variable warning in test that caused CI failures
- Downgraded `comfy-table` from v7.2 to v6 for Rust 2024 edition compatibility
  - v7.2.2 uses unstable let-chain features not available in stable Rust

### Added
- Dependabot configuration for automated dependency updates
  - Weekly checks for Cargo dependencies (grouped by patch/minor)
  - Weekly checks for GitHub Actions updates
  - Automatic PR creation with appropriate labels

## [0.2.0] - 2026-02-05

### Added

#### New Modules
- **time** module (feature: `time`) - DateTime utilities powered by chrono
  - `now()` - Get current UTC time
  - `now_local()` - Get current local time
  - `parse()` - Parse datetime strings with format specifiers
  - `format()` - Format datetime to strings
  - `sleep_secs()` / `sleep_millis()` - Sleep utilities
  - Re-exports: `DateTime`, `Utc`, `Local`, `NaiveDate`, `NaiveTime`, `NaiveDateTime`

- **archive** module (feature: `archive`) - Archive creation and extraction
  - `create_zip()` / `extract_zip()` - ZIP archive operations
  - `create_tar()` / `extract_tar()` - TAR archive operations
  - `create_tar_gz()` / `extract_tar_gz()` - Compressed TAR.GZ operations
  - Path traversal protection for safe extraction

#### fs Module Extensions
- `glob()` - Match files using glob patterns (e.g., `src/**/*.rs`)
- `read_toml()` / `write_toml()` - TOML serialization support
- `copy()` - Copy files or directories
- `rename()` - Rename/move files or directories
- `remove()` - Remove files or directories (recursive for dirs)
- `temp_file()` - Create temporary files with auto-cleanup
- `temp_dir()` - Create temporary directories with auto-cleanup
- Re-exported `TempDir` type for convenience

#### http Module Extensions
- `download()` - Stream downloads directly to file
- `put()` / `delete()` / `patch()` - Additional HTTP methods
- `put_json()` / `delete_json()` / `patch_json()` - JSON variants
- `Client` struct - Configurable HTTP client with builder pattern
  - `.timeout()` - Set request timeout
  - `.auth()` - Basic authentication
  - `.header()` - Custom headers
  - All HTTP methods (GET/POST/PUT/DELETE/PATCH) with text and JSON variants

#### path Module Extensions
- `which()` - Find executables in system PATH
- `is_executable()` - Check if a path is executable (cross-platform)

#### sh Module Extensions
- `PipeBuilder` - Chain piped commands
  - `.pipe()` - Add commands to pipe chain
  - `.output()` / `.run()` - Execute pipe chain
- `ChildProcess` - Background process management
  - `.spawn()` - Start command in background
  - `.wait()` - Wait for process completion
  - `.kill()` - Terminate process
  - `.try_wait()` - Non-blocking status check

#### print Module Extensions
- `table()` - Format data as tables with headers and rows
- `print_json()` - Pretty-print JSON to stdout

### Changed
- Moved `tempfile` from dev-dependencies to main dependencies
- Updated `full` feature to include new `time` and `archive` features

### Dependencies
- Added `glob` 0.3 - File pattern matching
- Added `toml` 0.8 - TOML serialization
- Added `which` 7.0 - Executable path resolution
- Added `comfy-table` 7.2 (optional, `print` feature) - Table formatting
- Added `chrono` 0.4 (optional, `time` feature) - DateTime handling
- Added `zip` 2.2 (optional, `archive` feature) - ZIP archives
- Added `tar` 0.4 (optional, `archive` feature) - TAR archives
- Added `flate2` 1.0 (optional, `archive` feature) - Gzip compression

### Quality
- 81 unit tests covering all new functionality
- 70 documentation tests
- All code passes `cargo clippy` with pedantic/nursery lints
- Cross-platform testing (Windows/Unix)
- Comprehensive examples in documentation

## [0.1.1] - 2026-02-05

### Changed
- Added repository, homepage, and documentation metadata
- Fixed GitHub username in README badges
- Added required-features to examples for individual feature builds

## [0.1.0] - 2026-02-05

### Added
- Initial release with core functionality
- `env` module - Environment variable utilities
- `fs` module - File system operations (strings, JSON, CSV, lines)
- `path` module - Path utilities (home, cwd, expansion)
- `sh` module - Cross-platform shell command execution
- `http` module (feature: `http`) - HTTP client with JSON support
- `print` module (feature: `print`) - Terminal UI (spinners, progress, prompts)
- `cli` module (feature: `cli`) - CLI parsing with clap
- Feature flags for optional dependencies
- Comprehensive error handling with anyhow

[0.4.0]: https://github.com/madmax983/smop/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/madmax983/smop/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/madmax983/smop/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/madmax983/smop/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/madmax983/smop/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/madmax983/smop/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/madmax983/smop/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/madmax983/smop/releases/tag/v0.1.0
