# smop Roadmap

smop makes it easier to write scripts in Rust: one-off scripts, `cargo
xtask` crates, and your own scripting orchestration. The goal is that a
script you'd write in bash or Python can be written in Rust with about the
same effort, and then can't rot without the compiler telling you.

A useful yardstick is Python's standard library. Most scripts need the
same handful of batteries:

| Python | smop | Status |
|---|---|---|
| `subprocess` | `sh`, `cmd!` | exists; 0.4 adds `cmd!`, real pipes, capture and exit codes |
| `pathlib`, `os.path`, `tempfile` | `path`, `fs` | exists |
| `shutil` | `fs` | 0.4 adds `copy_dir`, `remove_if_exists`, atomic writes |
| `os.walk` | `fs::walk` | 0.4 |
| `os.environ` | `env` | 0.4 makes parsing strict, adds `var_opt` |
| `json`, `tomllib`, `csv` | `fs::read_json` and friends | exists; 0.4 re-exports `smop::toml` |
| `re` | `re` feature | 0.4 |
| `hashlib` | `hash` feature | 0.4 |
| `signal`, `sys.exit` | `script` module | 0.4 |
| `argparse` | `cli` | exists |
| `urllib` / requests | `http` | exists; 0.5 adds query, bearer and retries |
| `zipfile`, `tarfile` | `archive` | exists; 0.5 adds `extract` by extension |
| `datetime`, `time` | `time` | exists |
| PyYAML | `yaml` feature | 0.5 |
| `sqlite3` | `sqlite` feature | later |

Every item below gets a GitHub issue. Each is done when it has tests
(happy path, an edge case, and a regression test for any bug it fixes),
doc comments and examples, a CHANGELOG entry, and a place in CI's
per-feature test and clippy loops if it adds a feature.

## Before 0.4: housekeeping

- Publish 0.3.2. It is merged but still "Unreleased" in the CHANGELOG.
- Close madmax983/smop#24 (zip 8.6). trunk is already on zip 9.
- Decide madmax983/smop#21 (more examples) and madmax983/smop#28 (docs
  drift scan).

## 0.4.0: scripts that don't fail silently

0.4 is a breaking release. Its theme comes from reading the code: the
biggest problems aren't missing features, they're places where smop hides
a failure. That goes against the tagline. The headline feature is `cmd!`,
and the rest of the release brings `sh` up to what real scripts (Autumn's
and Harvest's xtasks, Harvest's Rhai host) had to write by hand.

### Silent failures to fix

| Where | Problem | Fix |
|---|---|---|
| `src/sh.rs:376` | `CommandBuilder::pipe` copies only the program and args, as lossy `String`s. `.dir()`, `.env()`, `.env_remove()` and `.echo()` are dropped, and non-UTF-8 args are mangled. | `pipe(CommandBuilder)` that keeps every setting. Breaking. |
| `src/sh.rs:487-575` | `PipeBuilder::output`/`run` check only the last stage's exit status, and never wait on earlier stages. Middle stages' stderr is piped but never read, so a stage that writes a lot to stderr fills the pipe and hangs. | Pipefail: wait on every stage, report the first failing one, and inherit or drain stderr. |
| `src/sh.rs:409` | `spawn()` pipes stdout and stderr, but `ChildProcess` has no way to read them. A background process that logs a lot blocks once the pipe buffer fills. | Inherit by default. Opt-in capture with accessors. Breaking. |
| `src/fs.rs:280-294` | `fs::copy` is documented as copying "a file or directory", but calls `std::fs::copy`, which fails on directories. | Document it as file-only, give a directory error a pointer to the new `fs::copy_dir`. |
| `src/env.rs:49` | `env::var_or` treats a value it can't parse as unset, so `PORT=808O` quietly becomes the default. | Return `Result<T>`: the default applies only when unset. Breaking. |
| `src/archive.rs:41-44` | `extract_zip` silently skips entries whose path would escape `dest` (zip slip). | Error and name the entry. Breaking. |
| `src/print.rs:152` | `progress()` returns indicatif's `ProgressBar`, so an indicatif major release is a smop breaking change. | Return a smop-owned type, as `Spinner` already is. Breaking. |

### `cmd!` macro

See [ADR 0007](adr/0007-cmd-macro.md). This is a zero-dependency
proc-macro in a new `smop-macros` crate, behind a `macros` feature that is
part of `full`. `cmd!("git commit -m {msg} -- {files...}")` turns each
placeholder into exactly one argument and returns the existing
`CommandBuilder`. The ADR fixes the template grammar and the regression
cases.

### `sh`: processes that behave like a shell

Harvest's sandbox xtask is a port of a bash script. It calls
`std::process::Command` directly about 15 times instead of `sh`, because it
needed these, so it wrote them itself:

- **`pipe` with pipefail semantics** (above).
- **`sh::shell(cmd) -> CommandBuilder`.** Runs `sh -c` / `cmd /C` (ADR
  0005), but returns a builder, so `.dir()` and `.env()` work. Today
  `sh::output(cmd)` can't set a working directory, so Harvest's Rhai host
  writes `sh::cmd("sh").arg("-c").arg(cmd)` itself.
- **Capture with status.** `output_raw()` already returns `std::process::Output`
  without failing on a non-zero exit. Add a smop type with the status and
  stdout/stderr as text, so the common "run, look at the code, use the
  output" case is one call.
- **Tee.** Show output live while also capturing it. Harvest's Rhai example
  lists "a long command prints nothing until it ends" as a known limit.
- **Input and control:** stdin from a string or bytes, `.quiet()`,
  `.timeout(Duration)`, and `.output_lines()`.
- **`spawn_guarded()`:** a child that is killed when the guard drops, for
  the "start a server, run the tests, stop the server" pattern.
- **Shell-style exit codes:** 127 for "not found", 126 for "can't execute",
  128 + the signal number. Command errors carry the code as a typed marker
  in the anyhow chain (consistent with ADR 0001), so a script can exit with
  its child's code.

### `script`: the script's own lifecycle

A new module, `script`, behind its own feature:

- **`script::main(|| { ... })`.** Prints the error chain the way `print`
  does, and exits with the right code: the child's code from an `sh` error,
  130 after an interrupt, otherwise 1.
- **An interrupt guard.** While it lives, Ctrl-C, SIGTERM and SIGHUP set a
  flag instead of killing the process, so cleanup runs. Harvest's
  `interrupt.rs` does this by declaring C's `signal` itself and calling it
  in `unsafe` blocks. smop is
  `#![deny(unsafe_code)]`, so the issue has to choose between a dependency
  (`ctrlc` with its `termination` feature, or `signal-hook`) and one
  reviewed `unsafe` exception.
- **`retry(attempts, backoff, || ...)`** and **`timed("step", || ...)`.**

### `fs` helpers

- `copy_dir(from, to)`.
- `write_atomic(path, content)`: write to a temp file in the same
  directory, then rename.
- A way to write a file and create its missing parent directories.
  Harvest's Rhai host does this by hand. The issue decides whether
  `write_*` does it by default or a separate helper does.
- `remove_if_exists(path)`. Today `remove` fails on a missing path.
- `read_bytes` / `write_bytes`.
- `walk(dir)`: a recursive, sorted file list from std alone. Harvest's xtask
  uses `walkdir` in 9 files.
- `replace_in_file(path, from, to)`, which fails if nothing matched. This is
  for version bumps.
- `lock(path)`: an exclusive lock file with a deadline, as in Harvest's
  `publish_image`. `std::fs::File::lock` is stable from Rust 1.89, and the
  MSRV is 1.88, so this needs an MSRV bump. 0.4 is the place for one.

### `env`

- `var_or` becomes strict (in the table above).
- `var_opt::<T>(name) -> Result<Option<T>>`: `None` when unset, an error when
  set but unparsable. Harvest's Rhai host needed this.
- `is_ci()`.

### New batteries

- **`re` feature:** re-export `regex` with a few helpers. Harvest's xtask
  uses `regex` in about 50 files. This is the biggest missing battery.
- **`smop::toml` re-export,** the same fix 0.3.2 made for `serde_json`.
  Harvest's xtask uses `toml::` directly in 9 files.
- **`hash` feature:** SHA-256 of a file or bytes, as a hex string. Harvest
  shells out to `sha256sum`.
- **`git` module** built on `sh`: `head()`, `branch()`, `is_clean()`,
  `changed_since(rev)`.

### Other breaking changes

- `http::Client::timeout` takes a `Duration` instead of seconds.
- MSRV 1.89, if `fs::lock` lands.

### Tooling and quality

- A coverage gate in CI (`cargo-llvm-cov`) at 85%, the project's standard.
- `cargo-semver-checks` in CI, so 0.4.x releases can't break the API by
  accident.
- Tests for `cli` (none today) and more for `print` (7 today).
- Proofs where they pay: a Verus spec plus proptest for the `cmd!` word
  splitter, and a containment property for archive extraction ("every
  extracted path stays under `dest`").

## 0.5.0: polish

- **`http`:** `.query()` and `.bearer()` on requests. Retries on 429 and
  5xx that honour `Retry-After`. Harvest's xtask passes `curl --retry 3`.
- **`print`:** `info!` and `step!` macros. `select`, `multiselect` and
  `password` prompts. Defined behaviour without a terminal: `confirm_default`
  returns its default, and `confirm` fails with a clear error.
- **`archive::extract(path, dest)`,** which picks the format from the file
  extension.
- **`yaml` feature.** `serde_yaml` is deprecated, so this needs a
  maintained successor, picked when the work starts.
- **Single-file scripts:** docs and a template using cargo-script
  frontmatter, once `cargo script` is stable (rust-lang/cargo#16569). No
  runner binary: madmax983/smop#22 tried one and was closed.

## Later

These are bigger, and both earn their place from real use in Harvest.

- **`rhai` feature.** Embed [Rhai](https://rhai.rs) with smop's `fs`, `sh`,
  `env` and `http` functions registered, so a script runs with no compile
  step at all. Harvest's `examples/rhai-scripts` does this by hand, and runs
  each script as a durable, resumable workflow. The bindings should be
  exposed as a table of name and function pairs rather than registered
  directly, so a host like Harvest can wrap each one as a durable activity
  instead of writing it again. A script that outgrows Rhai ports to Rust
  with the same function names. This is different from madmax983/smop#22's
  TOML runner because Rhai is a real language. Costs: Rhai adds noticeable
  compile time, and smop would have two API surfaces to keep in sync.
- **`sqlite` feature.** `rusqlite` with its `bundled` option, plus
  `query::<T: Deserialize>` and `execute` helpers. Python ships `sqlite3`.
  Harvest's xtask uses `rusqlite` in its audit and perf tasks.
