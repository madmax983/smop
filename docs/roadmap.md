# smop Roadmap

smop makes it easier to write scripts in Rust: one-off scripts, `cargo
xtask` crates, and your own scripting orchestration. The goal is that a
script you'd write in bash or Python can be written in Rust with about the
same effort, and then can't rot without the compiler telling you.

A useful yardstick is Python's standard library. Most scripts need the
same handful of batteries:

| Python | smop | Status |
|---|---|---|
| `subprocess` | `sh`, `cmd!` | exists; 0.4 adds `cmd!` (done), real pipes (done), capture and exit codes |
| `pathlib`, `os.path`, `tempfile` | `path`, `fs` | exists |
| `shutil` | `fs` | 0.4 adds `copy_dir`, `move_path`, `remove_if_exists`, atomic writes |
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
| PyYAML | `yaml` feature | 0.5 (done) |
| `sqlite3` | `sqlite` feature | later |

Each deliverable below has one GitHub issue. Related items, such as a bug
fix and the API that replaces it, share one. An issue is done when it has:

- tests: the happy path, an edge case, and a regression test for any bug
  it fixes
- doc comments and examples
- a `changelog.d/` fragment, with a migration note if it breaks something
- README and CLAUDE.md updated wherever they show the changed API
- if it adds a feature: a place in `full`, in CLAUDE.md's feature table,
  and in CI's per-feature test and clippy loops

## Before 0.4: housekeeping

[madmax983/smop#30](https://github.com/madmax983/smop/issues/30)

Done:

- The 0.3.2 CHANGELOG heading has its date, 2026-11-03.
- `scripts/tag_release.sh` tags each release (`v0.1.0` to `v0.3.2`, then
  each new one) on the commit that crates.io records.
- [madmax983/smop#24](https://github.com/madmax983/smop/pull/24) (zip 8.6) and [madmax983/smop#13](https://github.com/madmax983/smop/pull/13) (comfy-table 7) are closed. trunk
  uses zip 9 and comfy-table 8.
- [madmax983/smop#21](https://github.com/madmax983/smop/pull/21) is closed. Its three examples are ported to the 0.4
  API. CI builds each example with only its `required-features`
  (`scripts/examples.sh`).
- CI runs `scripts/docs_drift.sh` (the `Docs drift` job). It finds 0
  drifted symbols on trunk.

## 0.4.0: scripts that don't fail silently

Tracking issue: [madmax983/smop#31](https://github.com/madmax983/smop/issues/31)

0.4 is a breaking release. Its theme comes from reading the code: the
biggest problems aren't missing features, they're places where smop hides
a failure. That goes against the tagline. The headline feature is `cmd!`.
The rest of the release brings `sh` up to what Harvest's xtasks and its
Rhai host had to write by hand.

0.4 is not xtask-only. `sh`, `script`, `fs` and `env` are what every kind of
script calls first. Harvest's Rhai host, a scripting host rather than an
xtask, hit the same gaps. Prompts and single-file scripts help one-off
scripts most, and follow in 0.5.

### Silent failures to fix

| Where | Problem | Fix |
|---|---|---|
| `src/sh.rs:376` | `CommandBuilder::pipe` copies only the program and args, as lossy `String`s. `.dir()`, `.env()`, `.env_remove()` and `.echo()` are dropped, and non-UTF-8 args are mangled. | `pipe(CommandBuilder)` that keeps every setting. Breaking. [madmax983/smop#43](https://github.com/madmax983/smop/issues/43) |
| `src/sh.rs:487-582` | `PipeBuilder::output`/`run` check only the last stage's exit status (`false` piped into `cat` returns `Ok`), and never wait on earlier stages, which are left as zombies or still running. In `output`, every stage but the last gets a piped stderr whose read end is closed right after spawn, so the first line such a stage writes to stderr kills it with SIGPIPE, and the pipe still returns `Ok` with truncated output. | Pipefail (see the `sh` section). Earlier stages inherit stderr. Breaking: pipes whose earlier stages fail now return `Err`. [madmax983/smop#43](https://github.com/madmax983/smop/issues/43) |
| `src/sh.rs:409` | `spawn()` pipes stdout and stderr, but `ChildProcess` has no way to read them. A background process that logs a lot blocks once the pipe buffer fills. | Inherit by default. Opt-in capture with accessors. Breaking. [madmax983/smop#45](https://github.com/madmax983/smop/issues/45) |
| `src/fs.rs:280-294` | `fs::copy` is documented as copying "a file or directory", but calls `std::fs::copy`, which fails on directories. | Document it as file-only, as Python has `shutil.copy` next to `shutil.copytree`, and give a directory error a pointer to the new `fs::copy_dir`. [madmax983/smop#37](https://github.com/madmax983/smop/issues/37) |
| `src/env.rs:49` | `env::var_or` treats a value it can't parse as unset, so `PORT=808O` quietly becomes the default. | Return `Result<T>`: the default applies only when unset. Breaking. [madmax983/smop#36](https://github.com/madmax983/smop/issues/36) |
| `src/archive.rs:41-47`, `:190`, `:263` | `extract_zip` silently skips entries whose path would escape `dest` (zip slip). `extract_tar` and `extract_tar_gz` do the same through `tar::Archive::unpack`, which drops every entry with a `..` component, even one that stays inside `dest`, and still returns `Ok`. | Error and name the entry, in all three, with one containment policy (ADR 0008): absolute paths, paths above `dest` and links out of `dest` are errors; `a/../b` extracts. Breaking. [madmax983/smop#35](https://github.com/madmax983/smop/issues/35) |
| `src/print.rs:152`, `src/fs.rs:19` | `progress()` returns indicatif's `ProgressBar`, and `fs::TempDir` is tempfile's type, so a major release of either crate is a smop breaking change. | Return smop-owned types, as `Spinner` already is. Breaking. [madmax983/smop#38](https://github.com/madmax983/smop/issues/38) |

The errors these paths produce need work too: `PipeBuilder::output` reports
only `Pipe command failed: <program>`, with no exit code or arguments, and
every error path reports death by a signal as "exit code -1"
(`status.code().unwrap_or(-1)`). Both are fixed with the exit codes in [madmax983/smop#34](https://github.com/madmax983/smop/issues/34).

### `cmd!` macro

[madmax983/smop#48](https://github.com/madmax983/smop/issues/48)

Done. See [ADR 0007](adr/0007-cmd-macro.md). This is a zero-dependency
proc-macro in a new `smop-macros` crate, behind a `macros` feature that is
part of `full`. `cmd!("git commit -m {msg} -- {files...}")` turns each
`{name}` into exactly one argument and each `{name...}` into one argument
per item, and returns the existing `CommandBuilder`. The ADR fixes the
template grammar and the regression cases.

### `sh`: processes that behave like a shell

Harvest's sandbox xtask (`xtask/src/sandbox/build_guest_image.rs`) is a
port of a bash script. It calls `std::process::Command::new` 12 times
instead of `sh`. Across the whole xtask, 23 files call `Command::new` 60
times, against 46 `sh::` calls. Harvest's xtasks and Rhai host wrote these
by hand:

- **Shell-style exit codes** ([madmax983/smop#34](https://github.com/madmax983/smop/issues/34)). Map 127 for "not found"
  and 126 for "can't execute" from the spawn error, since no process ran.
  On Unix, a death by signal is 128 + the signal number. On Windows the
  code is the child's own. Command errors stay `anyhow::Error` but carry a
  public type with the exit code, signal and command line, which
  `downcast_ref` and `err.chain()` can find. This is the mitigation ADR
  0001 names, and smop's first public error type, so ADR 0001 and
  CLAUDE.md get an amendment ("no custom error types in signatures"). A
  public `sh::exit_code(&err) -> Option<u8>` gives the bash mapping, which Harvest's
  xtask writes in 12 files. The Display text keeps the "exit code N"
  wording: Harvest's Rhai tests match on it, and durable hosts store it in
  history.
- **`pipe` with pipefail** ([madmax983/smop#43](https://github.com/madmax983/smop/issues/43)). Done. Wait on every stage, after
  closing the parent's copy of each pipe's read end, or `yes` piped into
  `head -n1` hangs. When stages fail, report the rightmost failing stage's
  code, as bash's `pipefail` and Harvest's `pipe_into` do, and name every
  failing stage. One deliberate difference from bash: an upstream stage
  killed by SIGPIPE counts as success, because it only means a later stage
  stopped reading. So `yes` piped into `head -n1` stays `Ok`, where bash
  pipefail gives 141.
- **`sh::shell(cmd) -> CommandBuilder`** ([madmax983/smop#41](https://github.com/madmax983/smop/issues/41)). Runs `sh -c` /
  `cmd /C` (ADR 0005), but returns a builder, so `.dir()`, `.env()` and
  `.echo()` work. Today `sh::output(cmd)` can't set a working directory, so
  Harvest's Rhai host writes `sh::cmd("sh").arg("-c").arg(cmd)` itself.
  `sh::run` and `sh::output` become wrappers over it. On Windows the
  command goes through `raw_arg`, because cmd.exe doesn't follow the C
  runtime quoting that `.arg` applies. On Unix, `.arg()`s after the script
  become `$0`, `$1`, …, so values reach it without quoting. Harvest's
  sandbox runs `sh -euc SCRIPT build-rootfs ROOTFS AGENT …` this way.
  `cmd /C` instead appends them to the command text, so the docs say so.
- **Process I/O** ([madmax983/smop#42](https://github.com/madmax983/smop/issues/42)):
  - **Capture with status.** `output_raw()` already returns
    `std::process::Output` without failing on a non-zero exit. Add a smop
    type with the status, the shell-style code, and stdout/stderr as text,
    so "run, look at the code, use the output" is one call.
  - **Tee and redirection.** Show output live while also capturing it or
    copying it to a log (`cmd 2>&1 | tee log`). Redirect stdout to a file
    (`>`, `>>`), stderr into stdout (`2>&1`) and stdout to stderr (`>&2`).
    Harvest's Rhai example lists "a long command prints nothing until it
    ends" as a known limit, and its xtask writes these redirections by hand
    in about 15 files (`perf/support.rs`, `check/formal_models.rs`).
  - **Input and control:** stdin from a string or bytes, `.quiet()`,
    `.output_lines()`, and `.timeout(Duration)`. A timeout kills the child
    at the deadline and still returns what it captured. Both streams are
    drained while waiting, or a chatty child deadlocks before the deadline.
    Harvest's `formal_traces` does this with reader threads.
- **`spawn()` and `spawn_guarded()`** ([madmax983/smop#45](https://github.com/madmax983/smop/issues/45)). Done (ADR 0013). A guarded child
  runs in its own process group on Unix and is killed with its group when
  the guard drops. Killing only the direct child leaves a server started
  through `sh -c`, `npm` or `cargo run` running, with captured pipes still
  open. Add `wait_until(timeout, || probe)` for the "until the server
  answers" step. Harvest's `run_standalone_chapter` uses `setsid`, `kill --
  -PID` and a curl health loop. std has no stable group kill
  (`Child::kill_process_group` is unstable), so this shares the
  dependency-or-`unsafe` decision with the interrupt guard. ADR 0013 uses
  the kill tool of the platform: `kill -- -PGID` and `taskkill /T`. A
  Windows Job Object can follow.

### `script`: the script's own lifecycle

[madmax983/smop#44](https://github.com/madmax983/smop/issues/44)

A new module, `script`, behind its own `script` feature. It implies
neither `print` nor a signal crate.

- **`script::main(|| { ... })`.** Prints the error chain as plain text
  (coloured only when `print` is on), and ends the process the way a shell
  expects:
  - After an `sh` error it exits with the child's code, through
    `std::process::exit`, because `ExitCode` holds only a `u8`.
  - After an interrupt it runs cleanup and then, on Unix, re-raises the
    signal with its default action, so a calling shell loop stops and sees
    130 or 143. Exiting with 130 instead makes a calling `for` loop carry
    on. On Windows it exits with 130.
  - Any other error exits with 1, and that code is configurable. Harvest's
    xtask exits 2 on an internal error, so it stays distinct from a failed
    gate's 1.
- **An interrupt guard.** While it lives, Ctrl-C, SIGTERM and SIGHUP set a
  flag instead of killing the process, so cleanup runs. `sh` honours it:
  while a guard lives, it starts no new child after an interrupt and fails
  with the interrupt marker, as Harvest's `spawn` and `check` helpers do. A
  cleanup command can opt out. Harvest's `interrupt.rs` does this by
  declaring C's `signal` itself and calling it in `unsafe` blocks. smop is
  `#![deny(unsafe_code)]`, so the choice is between `signal-hook` and one
  reviewed `unsafe` exception, recorded in an ADR:
  - `signal-hook`'s `flag::register` and `flag::register_conditional_default`
    give exactly "while it lives" and add 3 crates on Linux. Its handlers
    can't be unregistered, so the guard drops by flipping a flag.
  - `ctrlc` doesn't fit. Its handler can be installed once per process, is
    never removed, and fails with `MultipleHandlers` if the script installs
    its own.
  - The guard sits behind its own sub-feature (for example `script-signals`),
    so the rest of `script` stays dependency-free for xtasks.
  - SIGTERM and SIGHUP are Unix-only. On Windows the guard covers the
    Ctrl-C and Ctrl-Break console events.
- **`script::defer(...)`:** a guard that runs a cleanup command when it
  drops, also after an interrupt, and ignores its failure, like bash's
  `trap … EXIT`. Harvest's xtask writes four of these for `docker` and
  `psql` teardown.
- **`retry(attempts, backoff, || ...)`** and **`timed("step", || ...)`**,
  with no dependencies. Harvest's xtask writes three retry and poll loops
  (`build_typescript_client`, `pull_test_images`, `notify_commit`).

### `fs` helpers

- `copy_dir(from, to)` ([madmax983/smop#37](https://github.com/madmax983/smop/issues/37)).
- `move_path(from, to)`: rename, falling back to copy and remove across
  file systems, as `shutil.move` does. `fs::rename` fails across devices,
  so Harvest shells out to `mv` ([madmax983/smop#37](https://github.com/madmax983/smop/issues/37)).
- `remove_if_exists(path)`. Today `remove` fails on a missing path
  ([madmax983/smop#37](https://github.com/madmax983/smop/issues/37)).
- `read_bytes` / `write_bytes` ([madmax983/smop#37](https://github.com/madmax983/smop/issues/37)).
- `walk(dir)`: a recursive, sorted file list from std alone. Harvest's xtask
  uses `walkdir` in 9 files ([madmax983/smop#37](https://github.com/madmax983/smop/issues/37)).
- `write_atomic(path, content)`. Done (ADR 0015), with `sh`
  `stdout_atomic`. Write to a temp file in the same
  directory, then rename. The same should work for a command's stdout:
  Harvest's `regenerate_openapi` writes `openapi.json.tmp` from a child
  and renames it ([madmax983/smop#47](https://github.com/madmax983/smop/issues/47)).
- A separate helper that creates missing parent directories, then writes.
  Done: `write_with_parents` (ADR 0015).
  Harvest's Rhai host does this by hand. `write_*` keep failing on a
  missing parent, because creating it by default would hide a mistyped
  path, which is the kind of failure 0.4 removes ([madmax983/smop#47](https://github.com/madmax983/smop/issues/47)).
- `replace_in_file(path, from, to)`, which fails if nothing matched. Done
  (ADR 0015). This is
  for version bumps ([madmax983/smop#47](https://github.com/madmax983/smop/issues/47)).
- `lock(path, timeout)`: an exclusive advisory lock (`flock`) on a file or
  directory (a missing path is created as an empty file, as `flock(1)` does),
  waiting up to a deadline, plus a shared variant, so it works with
  `flock(1)`. Harvest's `publish_image` locks its output
  directory this way (`flock -w 60 -x`). It uses `std::fs::File::lock`,
  which is stable from Rust 1.89, so 0.4 raises the MSRV from 1.88 to 1.89
  (decided 2026-11-09). A deadline means polling `try_lock`
  ([madmax983/smop#33](https://github.com/madmax983/smop/issues/33)).

### `env`

[madmax983/smop#36](https://github.com/madmax983/smop/issues/36)

- `var_or` becomes strict (see the table above). It breaks in place rather
  than gaining a strict twin. A lenient `var_or` is exactly the silent
  failure 0.4 is about, and the change shows up as a compile error at every
  call site (`T` becomes `Result<T>`), not as a silent change in behaviour.
- `var_opt::<T>(name) -> Result<Option<T>>`: `None` when unset, an error
  when set but unparsable. Harvest's Rhai host returns `()` for an unset
  variable.
- An empty value counts as unset in `var_opt` and `var_or`, as in
  `${NAME:-default}`. Harvest's xtask defines that helper five times.
  `var` gives it to the parser ([ADR 0016](adr/0016-env-empty-values.md)).
- `is_ci()`: `CI`, then `TF_BUILD`, `JENKINS_URL` with `BUILD_ID`,
  `TEAMCITY_VERSION` and `CODEBUILD_BUILD_ID`. `CI=false` or `CI=0` gives
  `false`.

### New batteries

- **`re` feature** ([madmax983/smop#39](https://github.com/madmax983/smop/issues/39)): re-export `regex` with a few
  helpers. Harvest's xtask uses `regex` in about 50 files. This is the
  biggest missing battery.
- **`smop::toml` re-export** ([madmax983/smop#39](https://github.com/madmax983/smop/issues/39)), the same fix 0.3.2 made
  for `serde_json`. Harvest's xtask uses `toml::` directly in 9 files.
- **`hash` feature** ([madmax983/smop#40](https://github.com/madmax983/smop/issues/40)): SHA-256 of a file or bytes as a
  lowercase hex string, through `sha2` 0.11 (MSRV 1.88, 8 crates; 9 with `libc` on aarch64 Linux and Apple), with
  hand-written hex encoding because 0.11's output type has no `LowerHex`.
  Harvest shells out to `sha256sum`.
- **`git` module** ([madmax983/smop#46](https://github.com/madmax983/smop/issues/46)). Done (ADR 0014). Behind a `git` feature (`git =
  ["sh"]`, no dependencies, part of `full`). It provides `head()`,
  `branch()`, `is_clean()` (optionally for given paths), `changed_since(rev)`,
  `ls_files()`, `show(rev, path)` and `is_ignored(path)`. The last three are
  calls Harvest's xtask makes by hand.

### API boundaries

[madmax983/smop#38](https://github.com/madmax983/smop/issues/38)

Re-exporting a whole crate (`clap`, `serde`, `serde_json`, and in 0.4
`toml` and `regex`) is deliberate API, and that crate's major releases are
smop major releases. A dependency's type leaking through one smop function
is not deliberate: `progress()` and `fs::TempDir` get smop-owned types (see
the table above). Harvest uses `TempDir::with_prefix_in` and `.keep()`, so
the smop type keeps those. Also in this issue:
`http::Client::timeout` takes a `Duration` instead of seconds, matching every
other duration 0.4 adds (`.timeout()`, `retry`, `fs::lock`).

### Breaking changes, all together

- `CommandBuilder::pipe` and `PipeBuilder::pipe` take a `CommandBuilder`.
- Pipes fail when any stage fails (pipefail), and earlier stages' stderr is
  inherited instead of captured.
- `spawn()` inherits stdout and stderr by default.
- `env::var_or` returns `Result<T>`.
- `archive::extract_zip`, `extract_tar` and `extract_tar_gz` return an
  error for an absolute entry, an entry above `dest`, or a link out of
  `dest`.
- `print::progress()` and `fs::temp_dir()` return smop-owned types.
- `http::Client::timeout` takes a `Duration`.
- MSRV 1.89, for `std::fs::File::lock`. `rust-version` in both crates,
  CI's MSRV job and CLAUDE.md's MSRV section move together. rusqlite 0.41,
  which a later `sqlite` feature may want, needs 1.89 too.

Behaviour-only changes (pipefail, `spawn` stdio, archive errors) are
invisible to `cargo-semver-checks`, so the CHANGELOG lists them by hand.

### Platforms

Signals are Unix-only: SIGTERM, SIGHUP, 128 + N exit codes, and
re-raising after an interrupt. On Windows the interrupt guard handles
console Ctrl-C and Ctrl-Break, exit codes pass through unchanged, `sh::shell`
uses `cmd /C` with `raw_arg`, and a tree kill uses a Job Object. Tests for
Unix-only behaviour are `#[cfg(unix)]`, because CI runs on all three OSes.

### Tooling and quality

[madmax983/smop#32](https://github.com/madmax983/smop/issues/32)

- **Coverage gate.** One Linux CI job using
  `taiki-e/install-action@cargo-llvm-cov` and the `llvm-tools-preview`
  component runs `cargo llvm-cov --workspace --all-features
  --fail-under-lines 85`. That is the low end of the project's 85–90%
  standard. Today smop measures about 89% of lines (87% without inline test
  modules, and `print` alone is at 54–68%). New `sh` and `script` code
  needs its own tests to stay above the gate. Doctests aren't counted on
  stable.
- **`cargo-semver-checks`** in CI (`obi1kenobi/cargo-semver-checks-action@v2`
  with `package: smop`), turned on once `Cargo.toml` says 0.4.0, so 0.4.x
  releases can't break the API by accident. It sees API changes only.
- **A workspace-aware CI.** `smop-macros` is not a default workspace member,
  so CI runs `cargo test --workspace` and `cargo clippy --workspace`. The
  lint table moves to `[workspace.lints]`. The per-feature list is written
  twice in `ci.yml` today. It should be generated from `cargo metadata`.
  `tests/consumer` enables `macros`, `toml` and `re`, and uses `cmd!`,
  `smop::toml` and the regex re-export.
- **Tests for `cli` and `print`.** `cli` has no unit tests today, only two
  `no_run` doctests and the `tests/consumer` derive check. `print` has 7
  unit tests and none for the prompts.
- **Proofs where they pay.** proptest round trips for the `cmd!` lexer
  (part of the `cmd!` work), and a containment property for archive
  extraction: every extracted path stays under `dest`, and no write follows
  a symlink out of it. A Verus spec for the lexer moves to 0.5.

### Release

The tracking issue owns the release steps. The steps are in
`changelog.d/README.md`.
- Done: both crates are at 0.4.0, and `smop-macros` is pinned with
  `=0.4.0`.
- Done: the CHANGELOG 0.4.0 entry, from `python3 scripts/changelog.py
  release 0.4.0`. Each breaking change has a migration snippet. The intro
  lists the breaks that the compiler does not show.
- Done: the README installs `smop = "0.4"`, and its examples and CLAUDE.md's
  compile against 0.4 (`scripts/dx/snippets.sh`, the `DX` CI job).
- Done: `cargo publish --workspace --dry-run` packages and builds
  `smop-macros`, then `smop`.
- To do: `cargo publish --workspace`, then `cargo xtask tag-release
  0.4.0`.
- `cargo-semver-checks` compares with the latest smop on crates.io, so
  after the publish its baseline is 0.4.0 with no change to CI.

## 0.5.0: polish

Tracking issue: [madmax983/smop#49](https://github.com/madmax983/smop/issues/49)

- **`http`** ([madmax983/smop#50](https://github.com/madmax983/smop/issues/50)): `.query()` and `.bearer()` on requests.
  Retries on timeouts and on 408, 429, 500, 502, 503 and 504 (the set
  `curl --retry` uses), honouring `Retry-After`, also for `http::download`.
  Harvest's xtask downloads three pinned artifacts with
  `curl -fsSL --retry 3 -o` and checks each SHA-256 afterwards.
- **`print`** ([madmax983/smop#51](https://github.com/madmax983/smop/issues/51)): `info!` and `step!` macros. `select`,
  `multiselect` and `password` prompts. Defined behaviour without a
  terminal: `confirm_default` returns its default, and `confirm` fails with
  a clear error. GitHub Actions annotations (`::error::`, `::group::`)
  that print plain text outside CI. Harvest's xtask prints `::error::` by
  hand in 12 files.
- **`archive::extract(path, dest)`** ([madmax983/smop#52](https://github.com/madmax983/smop/issues/52)), which picks the
  format from the file extension.
- **`yaml` feature** ([madmax983/smop#53](https://github.com/madmax983/smop/issues/53)). Done (ADR 0019). `serde_yaml` is
  deprecated. Of the candidates, only `serde-saphyr` met the criteria
  (maintenance, serde support, the MSRV), and it adds 19 crates. smop has
  its own YAML 1.2 parser and writer instead, so the feature adds no crate.
- **Single-file scripts** ([madmax983/smop#54](https://github.com/madmax983/smop/issues/54)): docs and a template using
  cargo-script frontmatter, once `cargo script` is stable
  ([rust-lang/cargo#16569](https://github.com/rust-lang/cargo/issues/16569)). The template uses `default-features = false` with
  only the features the script needs. cargo builds each script in its own
  directory, so every new script compiles smop from scratch (about 23 s for
  `full`, about 2 s for `sh,fs,path`; see ADR 0007). There is no runner
  binary: [madmax983/smop#22](https://github.com/madmax983/smop/issues/22) tried one and was closed.
- **A Verus spec for the `cmd!` lexer** ([madmax983/smop#55](https://github.com/madmax983/smop/issues/55)). It proves that
  each `{name}` maps to exactly one argv element and that literal text never
  splits a value. It lives outside the published crates and runs in its own
  CI job with a pinned Verus release.
- **The repository's tooling is a `cargo xtask` written with smop**
  ([madmax983/smop#84](https://github.com/madmax983/smop/issues/84), [ADR 0017](adr/0017-xtask-tooling.md)). The bash and Python
  in `scripts/` (changelog, drift scans, API boundary, examples, snippets,
  clean room, release tags) become `cargo xtask` tasks, so smop runs its own
  checks the way its README tells users to, and CI needs no `jq` or Python.

## Later

These are bigger, and both earn their place from real use in Harvest.

- **`rhai` feature** ([madmax983/smop#56](https://github.com/madmax983/smop/issues/56)). Embed [Rhai](https://rhai.rs) with
  smop's `fs`, `sh`, `env` and `http` functions registered, so a script runs
  with no compile step at all.
  - **What Harvest does today.** `examples/rhai-scripts` registers `sh`,
    `read_file`, `write_file` and `env` by hand (no `http`), and runs each
    script as a durable, resumable workflow.
  - **Binding shape.** The bindings are a table of name and function pairs.
    Each function takes and returns JSON (`serde_json::Value`) and resolves
    paths and commands against an explicit base directory, not the process
    cwd. Then a host like Harvest can register each one as a durable
    activity instead of writing it again. Harvest's activities are
    `fn(Value) -> Result<Value, String>` with a `cwd` input field, because a
    resume may run from another directory.
  - **Porting out.** A script that outgrows Rhai ports to Rust with the same
    function names.
  - **How scripts run.** Scripts run from a host binary you write, as
    Harvest's does. Whether smop also ships a `smop run` binary is decided
    when this work starts. [madmax983/smop#22](https://github.com/madmax983/smop/issues/22)'s TOML runner was closed, but
    Rhai is a real language.
  - **Costs.** Rhai adds noticeable compile time, and smop would have two API
    surfaces to keep in sync.
- **`sqlite` feature** ([madmax983/smop#57](https://github.com/madmax983/smop/issues/57)). `rusqlite` with its `bundled`
  option, plus `query::<T: Deserialize>` and `execute` helpers. Python
  ships `sqlite3`. Harvest's xtask uses `rusqlite` in its audit tasks
  (`ledger_census`, `ledger_profiles`), and its Rhai example opens a
  read-only connection with it.
  - **Version coupling.** `libsqlite3-sys` declares `links = "sqlite3"`, so
    cargo allows one rusqlite 0.x per dependency graph. A smop on a
    different rusqlite minor than its consumer (Harvest pins 0.40) fails to
    resolve. So smop re-exports `smop::rusqlite`, and a rusqlite bump is a
    smop breaking change, like the indicatif row above.
