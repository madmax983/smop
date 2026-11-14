# smop

Batteries-included scripting utilities for Rust. Write Rust scripts like Python, but with a compiler that won't let them rot.

## Quick Start

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    let home = path::home();
    let config = fs::read_string(home.join(".config/app.json"))?;
    success!("Loaded config: {} bytes", config.len());
    Ok(())
}
```

## Architecture

```
src/
├── lib.rs        # Crate root, module declarations, feature gates
├── prelude.rs    # User-facing API surface - import everything with `use smop::prelude::*`
├── path.rs       # [feature: path] home(), cwd(), expand(), which(), workspace_root()
├── env.rs        # [feature: env] var(), var_or(), var_opt(), is_ci(), dotenv(), require_vars()
├── fs.rs         # [feature: fs] read_string, write_string, read/write_bytes, read_lines, append, create_dir_all, temp files (smop TempDir), lock/lock_shared
│                 #   + copy (files only), copy_dir, move_path (cross-device), remove_if_exists, walk (sorted files)
│                 #   + write_atomic, write_with_parents, replace_in_file (ADR 0015)
│                 #   + read/write_json [json], read/write_toml [toml], read/write_yaml [yaml], glob [glob], CSV [csv]
├── sh.rs         # [feature: sh] shell() builder (/bin/sh -c / cmd /C + raw_arg); run(), output() wrap it; cmd()/cargo() builders (echo, success, status, named errors)
│                 #   + pipe(CommandBuilder) -> PipeBuilder: pipefail, waits for each stage
│                 #   + CommandError (exit code, signal, timeout, command line, output) and exit_code()
│                 #   + capture() -> Captured, output_lines, tee, log_*, stdout_file/append, 2>&1, >&2, stdin, quiet, timeout
│                 #   + cmd! [macros]: the macro_rules! wrapper of the proc-macro (ADR 0007)
│                 #   + spawn() -> ChildProcess, keep_output, spawn_guarded() -> ChildGuard, wait_until (#45)
│                 #   + stdout_atomic: temp file, renamed only on success (ADR 0015)
│   ├── sh/stdio.rs  # One I/O engine for all run methods and spawn: reader/writer threads + deadline wait (ADR 0012)
│   └── sh/group.rs  # Process group kill for spawn_guarded: /bin/sh `kill -- -PGID`, Windows taskkill /T (ADR 0013)
├── atomic.rs     # [fs or sh] private: temp file in the same dir + rename, for fs and sh (ADR 0015)
├── __private.rs  # [feature: macros] doc-hidden support for the cmd! expansion: type dispatch
├── http.rs       # [feature: http] typed get/post/*_json (errors quote status+body) + raw request()/Client::request() -> Request -> Response
│                 #   + Client::timeout(Duration); query (percent-encoded), bearer; retry(Retry) on Client and Request (ADR 0018)
│   └── http/retry.rs # Retry policy (curl --retry set), Retry-After parser (seconds, three HTTP-date forms)
├── print.rs      # [feature: print] Terminal: success!/warn!/error!/info!/step!, Spinner, Progress, prompts
│                 #   + select/multiselect/password; no TTY on stdin or stderr: *_default = default, others Err
│                 #   + ci_error/ci_warning/group: GitHub Actions workflow commands, plain lines elsewhere (ADR 0021)
├── cli.rs        # [feature: cli] Re-exports clap derive macros AND `clap` itself (derive output uses `clap::` paths)
├── time.rs       # [feature: time] Date/time parsing and formatting (chrono)
├── archive.rs    # [feature: archive] create/extract ZIP, TAR, TAR.GZ; extract() selects by extension (ADR 0020)
│   └── archive/contain.rs  # One containment policy for all extract_* (ADR 0008)
├── re.rs         # [feature: re] `pub use regex::*` + compile, is_match, find(_all), captures(_all), replace(_all), split
├── hash.rs       # [feature: hash] sha256(bytes), sha256_file (64 KiB buffer), verify_sha256_file -> lowercase hex
├── yaml.rs       # [feature: yaml] from_str, to_string, Value; private Error with line/column (ADR 0019)
│   ├── yaml/parse.rs   # Text -> Node tree: block/flow, scalars, anchors, merge keys, limits
│   ├── yaml/scalar.rs  # YAML 1.2 core schema; writer quoting (YAML 1.1 words quoted)
│   ├── yaml/de.rs      # Node -> serde; the innermost node gives the error position
│   ├── yaml/ser.rs     # serde -> Out tree -> block-style text
│   └── yaml/value.rs   # Value: Null, Bool, Int(i128), Float, String, Sequence, Mapping
├── sqlite.rs     # [feature: sqlite] open, open_read_only, execute, query::<T>, query_one::<T>, query_opt::<T>; re-exports rusqlite (ADR 0022)
│   └── sqlite/de.rs  # One row -> serde: struct/map by column name, tuple by position, scalar = one column
├── git.rs        # [feature: git] head, branch, is_clean(_paths), changed_since, ls_files, show, is_ignored; Repo::new(dir) (ADR 0014)
└── script.rs     # [feature: script] main/Main (exit codes), Interrupted, interrupted, defer, retry, timed
                  #   + guard [script-signals]: Ctrl-C/SIGTERM/SIGHUP set a flag; sh starts no new command (ADR 0011)
smop-macros/      # Workspace member: the cmd! proc-macro. No dependencies (ADR 0007)
└── src/
    ├── lib.rs       # __cmd: gets $crate, the template literal and the listed names
    ├── decode.rs    # Source text of a string literal -> its value
    ├── lex.rs       # Template -> words: separators, quotes, braces
    ├── validate.rs  # Words -> template: shell syntax, splats, names
    └── expand.rs    # Template -> builder calls, or compile_error!
xtask/            # Workspace member, publish = false: smop's checks and release steps, written with smop (ADR 0017)
└── src/          # `cargo xtask <task>`; `cargo xtask --help` lists them. CI runs the same commands
    ├── main.rs          # clap CLI under script::main
    ├── meta.rs          # cargo metadata -> typed structs (serde derive through smop::serde)
    ├── changelog.rs     # changelog check | preview | release <version> [date]
    ├── features.rs      # ci-features (CI's per-feature loops), docs-features (CLAUDE.md table)
    ├── drift.rs         # docs-drift: module::symbol in README/CLAUDE/ADRs exists; ADR numbers
    ├── api_boundary.rs  # api-boundary: rustdoc links to non-re-exported crates
    ├── examples.rs      # examples: each builds with only its required-features; README runs it
    ├── snippets.rs      # snippets: compile each ```rust block of README.md and CLAUDE.md
    ├── clean_room.rs    # clean-room: README install + first block -> cargo run (clean-room.Dockerfile)
    └── tag_release.rs   # tag-release <version>...: tag the commit crates.io records
```

## Features

Only `anyhow` is unconditional. Every module is behind a feature (see ADR 0006).

| Feature | Dependencies | What it enables |
|---------|--------------|-----------------|
| `sh`    | (none) | sh module |
| `fs`    | tempfile | fs module (core file ops, temp files) |
| `path`  | which, shellexpand (no default features) | path module; home dir via `std::env::home_dir` |
| `env`   | dotenvy | env module |
| `serde` | serde | `Serialize`/`Deserialize` in prelude |
| `json`  | serde, serde_json (+fs) | fs::read_json / write_json |
| `toml`  | serde, toml (+fs) | fs::read_toml / write_toml |
| `yaml`  | (none) (+fs, serde) | fs::read_yaml / write_yaml, yaml module (own parser) |
| `glob`  | glob (+fs) | fs::glob |
| `http`  | ureq, serde_json | http module for sync HTTP requests |
| `cli`   | clap | Parser, Args, Subcommand derives |
| `print` | console, indicatif, dialoguer, comfy-table, serde_json | Terminal UI, macros, prompts |
| `csv`   | csv, serde (+fs) | read_csv, write_csv, read_csv_rows in fs module |
| `time`  | chrono | time module |
| `archive` | zip, tar, flate2 | archive module |
| `re`    | regex | re module: regex re-export and `&str`-pattern helpers |
| `hash`  | sha2 (no default features; 8 crates, 9 with libc on aarch64 Linux and Apple) | hash module: SHA-256 as lowercase hex |
| `sqlite` | rusqlite (bundled SQLite: 9 crates, 14 with build deps, a C compile) (+serde) | sqlite module, `smop::rusqlite` |
| `git`   | (none) (+sh) | git module: queries through the `git` program |
| `script` | (none) (+sh) | script module: main, defer, retry, timed |
| `script-signals` | signal-hook (+script) | script::guard, the interrupt guard |
| `macros` | smop-macros (+sh) | `cmd!`, the command template macro |
| `full`  | All above | Everything (default) |

## Design Decisions

See `docs/adr/` for Architecture Decision Records:

1. **anyhow for errors** - `Result<T>` everywhere, no custom error types in signatures. `sh::CommandError` is inside the `anyhow::Error` (ADR amendment, 0.4)
2. **ureq for HTTP** - Sync-only, no tokio dependency (~40 crates for the `http` feature vs ~300)
3. **Prelude philosophy** - `use smop::prelude::*` gets everything
4. **Feature flags** - Optional deps isolated, minimal core. Types of optional deps stay out of the public API, except re-exported crates (see API boundaries below)
6. **Granular core features** - sh/fs/path/env/json/toml/glob are separate features so xtask crates stay light
5. **Cross-platform shell** - `cmd /C` on Windows, `sh -c` on Unix
7. **`cmd!` macro** - zero-dependency proc-macro in a `smop-macros` crate behind a `macros` feature; one implementation, strict template grammar. `{name}` is one argument. `{name...}` is one argument for each item. Shell syntax outside quotes is a compile error. Type dispatch is autoref specialization in `src/__private.rs`. Stable rustc never caches macro expansion, `macro_rules!` included (nightly `-Zcache-proc-macros` covers derives only), so caching was not a reason to avoid a proc-macro
8. **Strict `env` parsing** - a bad value is an error, never the default. `var_opt`/`var_or` read an empty value as unset; `var` gives it to the parser (ADR 0016)
9. **Archive containment** - `extract_*` return an error for absolute paths, `..` above `dest`, and links out of `dest`. No write follows a link out of `dest`. One module for all formats (ADR 0008). `extract` selects the function from the end of the file name: `.zip`, `.tar`, `.tar.gz`, `.tgz`, any letter case; a different end is an error before any I/O (ADR 0020)
10. **`re` = regex + helpers** - `smop::re` re-exports all of `regex`. Each helper compiles its `&str` pattern on each call. An invalid pattern is an error that quotes it (ADR 0009)
11. **`hash` = SHA-256 as hex** - No `sha2` type is in the public API. smop encodes hex itself (`sha2` 0.11 output has no `LowerHex`). `sha256_file` reads through one buffer (ADR 0010)
12. **Interrupt guard = `signal-hook`, no `unsafe`** - `script-signals` only; `script` has no deps. Handlers stay installed; a flag gives the default action back when the last guard drops. After an interrupt (`Ok` or `Err`), `script::main` stops by the same signal on Unix (a bash loop stops), 130 on Windows. A signal ignored at start (`nohup`) is caught, then gets its default action (ADR 0011)
13. **`sh` process I/O** - one std-only engine runs every run method. Threads drain stdout and stderr, so no deadlock. `timeout` kills and reaps the direct child and also bounds open output: code 124, captured output kept. `spawn` uses the engine and honours each I/O option; only `wait`/`wait_with_output` apply its timeout. It inherits stdout/stderr; `keep_output` keeps them for `wait_with_output`. `pipe` rejects I/O options (ADR 0012)
14. **`sh` pipes = pipefail** - each stage is a `CommandBuilder`. smop waits for each stage. The rightmost failed stage gives the code. A stage before the last that `SIGPIPE` stops is a success, so `yes | head -n1` is `Ok` (#43)
15. **`spawn_guarded` = group kill, no crate, no `unsafe`** - the child gets its own process group. `ChildGuard` drop runs `/bin/sh -c 'command kill -s KILL -- -PGID'` with no environment (Windows: `taskkill /T /F`), kills before the reap, ignores errors. SIGKILL only. `wait`/`wait_with_output` kill nothing. `sh::wait_until(timeout, || bool)` polls a probe and stops on an interrupt (ADR 0013)
16. **`git` = the `git` program through `sh`** - no crate. `Repo::new(dir)` runs `git -C dir` and removes `GIT_DIR` and the other repo-local variables (a hook sets them); free functions keep them. Each call sets `core.fsmonitor=` and removes `GIT_*_PATHSPECS`. Errors are `sh::CommandError` with git's stderr. Result paths are repo-relative, sorted, deduped; your paths are literal (`icase` with `core.ignorecase`). `changed_since(rev)` = index + work tree + untracked + submodules, so `changed_since("HEAD")` is empty only when `is_clean()` is true. A `rev` that is empty or starts with `-` or `:` is an error. `show` = `cat-file blob`, untrimmed (ADR 0014)
17. **Atomic writes** - one std-only module (`src/atomic.rs`) for `fs::write_atomic`, `fs::replace_in_file` and `sh` `stdout_atomic`: `create_new` temp file in the same directory (target's mode, else as `std::fs::write`; never more open than the target), target's permissions, `sync_all`, rename. A drop removes the temp file. A symlink target is resolved; the link stays. `stdout_atomic` renames only on exit 0, no timeout, no output error; an interrupt or `ChildProcess::kill` is an `Err` and no rename. `write_*` never create parents; `write_with_parents` does. `replace_in_file`: plain text, no match = `Err` and no write (ADR 0015)
18. **Tooling = `cargo xtask` written with smop** - `xtask/` (`publish = false`) holds every check and release step; no bash, Python or jq. It uses smop from the checkout with only the features it needs. Logic in plain functions with unit tests; thin wrappers run cargo, git and HTTP (ADR 0017)
19. **`http` retries = `curl --retry`** - opt-in `Retry::new(n)`: n retries after the first try. Only a timeout or status 408, 429, 500, 502, 503, 504. Not a refused connection. Wait: 1 s, doubles; `Retry-After` (seconds or HTTP-date) replaces it; `max_delay` (60 s) caps both. Only idempotent methods unless `any_method()`. Client timeout is per attempt. One `warning:` line on stderr per retry, no query values or user info. A `script::guard` interrupt stops the wait. `Retry` is `Clone`, not `Copy`. `bearer`, `auth` and `header("Authorization", ..)` replace an earlier `Authorization` (ADR 0018)
20. **`yaml` = smop's own parser** - no crate: only `serde-saphyr` met the issue's criteria, with 19 crates. YAML 1.2 core schema: `on`/`yes`/`no` are strings; a plain scalar into `String` is its text (`3.10` -> `"3.10"`), except in flatten/untagged/internally tagged types (serde buffers them). Anchors, aliases (shared, `Rc`), `<<` merges, block scalars, flow, `!!` core tags. Errors (line, column): second document, `?` keys, empty key, collection or alias keys, anchor/tag before an implicit key or on an alias, `[a: b]`, `%TAG`, non-core tags, control characters, tab in block indentation, depth > 128, aliases copying > 1,000,000 nodes or 64 MiB. Other directives are ignored. Duplicate key = same value (`1`, `01`) or same text (`1`, `"1"`). Null into a seq/map/struct = empty. Writer: block style, 2 spaces, YAML 1.1 words and `=` quoted, floats as `1.0e-5`, `\n`-only multi-line strings as `|`, else double quotes; duplicate keys = Err (ADR 0019)
21. **`print` without a terminal, CI annotations** - a prompt asks only when stdin and stderr are terminals (console reads `/dev/tty` when stdin is not one, and waits). If not, `confirm_default`/`prompt_default` return the default and the others return an error that names the prompt and the stream. No wait. `select`/`multiselect` return indexes; no items = `Err`. `info!`/`step!` go to stderr. `ci_error`, `ci_warning`, `group` write `::error::`, `::warning::`, `::group::`/`::endgroup::` to stderr only when `GITHUB_ACTIONS` is `true`, else the `error!`/`warn!`/`step!` line. The `::` text escapes `%`, CR and LF. `group` ends on a panic. Status lines hide live bars while they print (ADR 0021)
22. **`sqlite` = rusqlite, bundled, re-exported** - `smop::rusqlite` is 0.42; `links = "sqlite3"` allows one 0.x line in a build, so a new line is a smop breaking change. `open` makes the file, not parents; `open_read_only` makes no database file. A path is a file path: a relative `file:` name gets `./`, so it is not a URI. An empty path is an error. Both read the schema without a lock wait, so a non-database file is an error at open. `query`/`query_one`/`query_opt` take `impl Params`, so `query::<T>` needs no `_`. Own serde row deserializer: struct/map/`Value`/untagged by name (extra columns ignored, two columns with one name = Err), tuple by position (exact count), scalar/`Option`/enum = one column, a one-BLOB row -> `Vec`/array; INTEGER -> ints, floats, bool (0 = false); only TEXT -> String; BLOB = byte sequence; NULL -> `None`, else an error that says NULL. SQL errors: `SQL error in "<statement>"`, cause = `rusqlite::Error`; smop adds no parameter values (SQLite's message or a row error can show one). Row errors give the row and the column. Clean build: debug +6 s, release +~50 s (one C file) (ADR 0022)

Planned work is in `docs/roadmap.md`. Each deliverable there has one GitHub issue, under a tracking issue per release.

### API boundaries

A crate that smop re-exports is part of smop's API: `anyhow`, `clap`,
`serde`, `serde_json`, `toml`, `regex` (as `smop::re`) and `rusqlite` (as
`smop::rusqlite`). A major release of that crate, or a new 0.x line, is a
smop major release. One exception is not a whole
crate: `time` re-exports chrono's date types on purpose.

Do not put a type of any other dependency in a public signature, field or
trait impl. Wrap it in a smop type, as `print::Progress`, `print::Spinner`
and `fs::TempDir` do. `cargo xtask api-boundary` (CI) finds such types in
rustdoc. To re-export a crate on purpose, add it to `ALLOWED` in
`xtask/src/api_boundary.rs`.

## Testing

```bash
cargo test --workspace --all-features  # All tests with all features
cargo test -p smop --no-default-features  # anyhow only
cargo test -p smop --no-default-features --features sh,fs,path  # the xtask configuration
# CI also runs test + clippy for every feature on its own (the list comes from
# `cargo xtask ci-features`, i.e. cargo metadata); new code must compile under each.
# Tests needing serde/serde_json must be #[cfg(feature = "...")]-gated
# (tempfile is a dev-dependency, so tests may always use it).
# tests/archive_safety.rs builds archives with raw entry names (`..`, `/`) and
# has the extraction containment proptest. Use it for new extract_* tests.
# Its link tests and the proptest are #[cfg(unix)].
cargo clippy --workspace --all-targets --all-features -- -D warnings  # Lint check, tests included
cargo fmt --all --check  # Format check
cargo xtask api-boundary  # No dependency types in the public API (rustdoc)
cargo xtask examples      # Each example builds with only its required-features; README lists it
cargo xtask docs-drift    # Each module::symbol in README/CLAUDE/ADRs exists; no duplicate ADR numbers
cargo xtask docs-features # CLAUDE.md's feature table matches Cargo.toml
cargo xtask snippets      # Each ```rust block of README.md and CLAUDE.md compiles
cargo xtask clean-room --smop-path "$PWD"  # README install + first block, then cargo run
cargo llvm-cov --workspace --all-features --fail-under-lines 85  # CI's coverage gate
# The repository's tooling is the xtask crate, written with smop (ADR 0017).
# No bash, Python or jq: a new check is a task in xtask/src with unit tests.
# cmd!: tests/cmd_macro.rs runs a script and reads back the argv (Unix).
# tests/ui/ has the compile-fail cases. Their snapshots are for one pinned
# toolchain and one feature set, so that test is #[ignore].
# tests/ui/README.md has the command.
# smop-macros: the decoder, lexer and validator are plain functions over &str.
# Test them there, with no proc_macro types.
cargo test -p smop-macros
```

CI also runs `cargo-semver-checks` against the latest smop on crates.io. The
release type comes from `version` in Cargo.toml. Trunk is at 0.4.0, the version
being released, so after the publish the check rejects a breaking change. The
first breaking change of the 0.5 cycle moves both crates to 0.5.0 (and the
`=` pin of `smop-macros`). Behaviour-only changes are invisible to it: list
those in changelog.d/ by hand.

### Changelog

Do not edit `CHANGELOG.md` for a change. Add a fragment
`changelog.d/<name>.<section>.md` (see `changelog.d/README.md`), with a
migration note for a breaking change. CI runs `cargo xtask changelog check`.

A release gets a tag `v<version>` on the commit that crates.io records:
`cargo xtask tag-release <version>` (steps in `changelog.d/README.md`).

### Examples

Each file in `examples/` has an `[[example]]` entry with `required-features`
and a `cargo run --example <name>` line in README.md. Put testable logic in
functions with a `#[cfg(test)]` module, and set `test = true`. Write output
below `target/smop-example-output/`.

Code that needs a terminal (prompts) takes a `console::Term` internally, so
tests can pass a non-TTY one (`Term::read_write_pair` on a temp file) instead
of hanging when run from a shell.

## Key Patterns

### Error Handling
All functions return `anyhow::Result<T>`. Use `.context()` for error messages:
```rust
let config = fs::read_string("config.json")
    .context("Failed to load configuration")?;
```

### Environment Variables with Types
```rust
let port: u16 = env::var("PORT")?;           // Required, parsed
let timeout: u32 = env::var_or("TIMEOUT", 30)?; // Default if unset or empty; bad value = Err
let level: Option<u8> = env::var_opt("LEVEL")?;  // None if unset or empty
if env::is_ci() { /* no prompts */ }            // CI, then TF_BUILD, JENKINS_URL+BUILD_ID, …
env::require_vars(&["API_KEY", "API_SECRET"])?; // Ensure multiple are set
```

### Shell Commands (Cross-Platform)
```rust
sh::run("git status")?;                    // Inherits stdout/stderr
let output = sh::output("git rev-parse HEAD")?; // Captures stdout
sh::shell("make").dir("./project").env("X", "1").run()?; // run/output = shell(line).run()/.output()
sh::shell(r#"mkdir -p "$1""#).args(["mkdir", "a b"]).run()?; // Unix: extra args are $0, $1, …; Windows: appended to the text

// Builder pattern for complex commands
sh::cmd("cargo")
    .args(["build", "--release"])
    .dir("./project")
    .env("RUSTFLAGS", "-C target-cpu=native")
    .run()?;

// cmd! (requires `macros`): a template, split at compile time. No shell runs.
let (msg, files, jobs) = ("two words", ["a b.rs", "c.rs"], 8);
cmd!("git commit -m {msg} -- {files...}").run()?; // {name} = one argument, {name...} = one for each item
cmd!("make -j{jobs} 'CFLAGS=-O2 -g' --out={dir}", dir = std::env::temp_dir().join("out")).dir("./project").run()?; // name = expr
// Compile errors: `&&`, `|`, `>`, `$HOME`, `~` outside quotes; `{name...}` with a path; `{}`; a no-break space
```

Prefer `cmd!` to `format!` + `sh::run` when a value goes into a command line.

### Process I/O (`sh`)
```rust
let r = sh::cmd("git").args(["diff", "--quiet"]).capture()?; // non-zero exit is Ok; r.code(), r.stdout()
let lines = sh::cmd("cat").stdin("a\r\nb\n").output_lines()?;   // ["a", "b"]
sh::cmd("cargo").arg("test").stderr_to_stdout().log_stdout("test.log").run()?; // 2>&1 | tee test.log
let out = sh::cmd("make").tee().output()?;                     // live on the terminal and captured
sh::cmd("date").stdout_append("dates.txt").run()?;             // >>; also stdout_file (>), stdout_to_stderr (>&2)
sh::cmd("npm").arg("ci").quiet().run()?;                        // silent; error has the stderr tail
let n = sh::cmd("git").args(["ls-files"]).pipe(sh::cmd("wc").arg("-l")).output()?; // pipefail; each stage is a builder
let err = sh::cmd("sleep").arg("10").timeout(std::time::Duration::from_millis(200)).run().unwrap_err();
assert_eq!(sh::exit_code(&err), Some(124));                    // CommandError::timed_out(), stdout(), stderr()
let job = sh::cmd("make").keep_output().spawn()?;              // background; threads drain the pipes
let r = job.wait_with_output()?;                               // sh::Captured; without keep_output, output went to the terminal
let _srv = sh::shell("npm run serve").stdout_file("serve.log").spawn_guarded()?; // drop kills the process group
sh::wait_until(std::time::Duration::from_secs(30), || std::path::Path::new("ready").exists())?; // Err names the timeout
```
Tests that check the terminal of the test process run a case in a new test
process (`child_entry` in `tests/sh_io.rs` and `tests/sh_spawn.rs`).

### Terminal UI (requires `print` feature)
```rust
success!("Build complete");
warn!("Deprecated API used");
error!("Connection failed");

let spinner = print::spinner("Compiling...");
// work...
spinner.finish();

let bar = print::progress(100); // print::Progress
for i in 0..100 {
    bar.set_message(format!("item {i}"));
    bar.inc(1);
}
bar.finish_with_message("done");

let name = print::prompt("What's your name?")?;
let confirm = print::confirm("Continue?")?;
let i = print::select("Profile", &["debug", "release"])?;  // index; also multiselect -> Vec<usize>
let token = print::password("API token")?;                // not shown; empty = ask again
let go = print::confirm_default("Deploy?", false)?;       // no TTY on stdin or stderr: the default, no wait

info!("Using cached index");                              // ℹ, stderr
step!("Test");                                            // →, stderr
print::ci_error("Cargo.lock is out of date");             // GITHUB_ACTIONS=true: ::error::…, else ✗ line
print::group("cargo test", || sh::cargo().arg("test").run())?; // ::group:: … ::endgroup::, also on a panic
```
Tests: annotations run only in a child process (`tests/print.rs`). In CI, an
in-process `::error::` line makes a real annotation. `script` (util-linux,
BSD on macOS) gives a child a pty: stdin null, or keys that the test types.

### Derives without direct deps
clap and serde derives expand to code naming their crate. smop re-exports
`clap` (in `smop::cli` and the prelude), `smop::serde`, `smop::serde_json`,
`smop::toml`, `regex` as `smop::re`, and `smop::rusqlite`.
`tests/consumer/` is a crate depending only on smop; CI runs it. Never rely on
smop's own tests/doctests for this: cargo gives them every smop dependency as
`--extern`, which hides missing re-exports.
```rust
#[derive(Serialize, Deserialize)]
#[serde(crate = "smop::serde")] // only without a direct serde dependency
struct Config { name: String }
let v: smop::toml::Value = smop::toml::from_str("a = 1")?; // no direct toml dependency
```

### HTTP: status-aware calls (requires `http` feature)
```rust
let resp = http::Client::new().request("POST", url).json(&body).send()?; // Err = transport only
match resp.status() { 404 => {}, 422 => { let e: ApiError = resp.json()?; } _ => { resp.error_for_status()?; } }
let gh = http::Client::new().bearer("TOKEN").retry(http::Retry::new(3)); // Authorization: Bearer; curl --retry 3
let r = gh.request("GET", "https://api.github.com/search/code").query("q", "a b").send()?; // ?q=a%20b
http::Client::new().retry(http::Retry::new(3).any_method()).post(url, "x")?; // POST/PATCH retry only when asked
```
Tests use `mockito`; for "connection fails" use a closed localhost port, not a
bogus hostname (an HTTP proxy may answer for unknown hosts). mockito answers
one request at a time: for a stalled body next to other replies, use
`scripted` in the `http` tests; for a timeout, `silent_port`.

### Regex (requires `re` feature)
```rust
let ok = re::is_match(r"^v\d+\.\d+", "v1.2")?;          // Err quotes a bad pattern
let nums: Vec<&str> = re::find_all(r"\d+", "a1 b22")?;   // ["1", "22"]
if let Some(c) = re::captures(r"(?<k>\w+)=(?<v>\w+)", "port=80")? { let _v = &c["v"]; }
let out = re::replace_all(r"(\w+)@", "me@x", "$1 at ")?; // "me at x"
let fields = re::split(r"\s*,\s*", "a , b")?;           // ["a", "b"]
let line_re = re::compile(r"^\s*#")?;                     // compile one time for a loop
```

### Hashing (requires `hash` feature)
```rust
let d = hash::sha256(b"abc");                   // "ba7816bf...", lowercase hex
let f = hash::sha256_file("dist/tool.tar.gz")?; // constant memory; Err names the path
let pinned = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
hash::verify_sha256_file("dist/tool.tar.gz", pinned)?; // Err shows path + both digests
```

### Git (requires `git`)
```rust
let sha = git::head()?;                                // 40 hex (64 for SHA-256); Err before the first commit
let on: Option<String> = git::branch()?;               // None on a detached HEAD
ensure!(git::is_clean()?, "uncommitted changes");      // staged, modified, untracked (not ignored)
let lock_ok = git::is_clean_paths(["Cargo.lock"])?;    // literal paths; none = whole repo
let changed = git::changed_since("origin/main")?;      // Vec<PathBuf>, sorted, repo-relative, with uncommitted
let old = git::show("HEAD~1", "Cargo.toml")?;          // committed text, not trimmed; root-relative path
let files = git::ls_files()?;                          // tracked, sorted, repo-relative
let skip = git::is_ignored("target")?;                 // check-ignore; a tracked file is not ignored
let lib_clean = git::Repo::new("vendor/lib").is_clean()?; // the same queries for another directory
```
Tests: `tests/git.rs` makes a repo in a temp dir. Free functions and the
environment are tested in a child process (`child`).

### Script lifecycle (requires `script`; `guard` requires `script-signals`)
```rust
use smop::prelude::*;

fn main() {
    script::main(|| {                      // error: / caused by: lines; exit code of the failed command, else 1
        let _guard = script::guard()?;     // Ctrl-C: sh starts no new command, cleanups run, then the process stops by SIGINT
        let _rm = script::defer(sh::cmd("docker").args(["rm", "-f", "db"])); // runs on drop, failure = warning
        script::retry(5, std::time::Duration::from_secs(1), || sh::run("pg_isready"))?; // waits 1, 2, 4, 8 s
        script::timed("tests", || sh::cargo().arg("test").run()) // prints "tests: 1.23s"
    })
}
// script::Main::new().error_code(2).run(|| ..)  -- other errors exit 2
// sh::cmd("psql").arg("-c").arg("DROP DATABASE t").cleanup().run()?;  -- also runs after an interrupt
```

### YAML (requires `yaml` feature)
```rust
#[derive(Serialize, Deserialize)]
#[serde(crate = "smop::serde")] // only without a direct serde dependency
struct Service { image: String, ports: Vec<String> }
let svc: Service = fs::read_yaml("service.yaml")?;   // Err: path, then "line 3, column 9: ..."
fs::write_yaml("out.yaml", &svc)?;                    // block style; "yes" and "3.10" quoted
let wf: yaml::Value = yaml::from_str("on: [push]\njobs: {}")?;
let first = wf["on"][0].as_str();                     // Some("push"); a missing key is Value::Null
let text = yaml::to_string(&wf)?;
```
Tests: `tests/yaml.rs` (syntax, serde, writer, fs, proptests: writer round
trip, parser never panics, parsed documents round trip).

### SQLite (requires `sqlite` feature)
```rust
#[derive(Deserialize)]
#[serde(crate = "smop::serde")] // only without a direct serde dependency
struct Task { id: i64, title: String, done: bool }
let db = sqlite::open("tasks.db")?;                  // makes the file; ":memory:" works
sqlite::execute(&db, "CREATE TABLE IF NOT EXISTS task (id INTEGER PRIMARY KEY, title TEXT, done INTEGER)", ())?;
let n = sqlite::execute(&db, "INSERT INTO task (title, done) VALUES (?1, ?2)", ("docs", false))?; // changed rows
let open: Vec<Task> = sqlite::query(&db, "SELECT * FROM task WHERE NOT done", ())?; // by column name
let total: i64 = sqlite::query_one(&db, "SELECT count(*) FROM task", ())?;        // one column = scalar
let pairs: Vec<(String, Option<i64>)> = sqlite::query(&db, "SELECT title, NULL FROM task", ())?; // by position
let found: Option<Task> = sqlite::query_opt(&db, "SELECT * FROM task WHERE id = ?1", [7])?; // None: no row
let reader = sqlite::open_read_only("tasks.db")?;   // next to a live writer; no new file
// Err: SQL error in "SELECT * FROM nope" / caused by: no such table: nope
db.execute_batch("PRAGMA journal_mode = WAL; CREATE INDEX IF NOT EXISTS t_done ON task (done);")?; // rusqlite API
```
Tests: `tests/sqlite.rs` (temp files, errors, row mapping, round-trip proptest).

### CSV Files (requires `csv` feature)
```rust
#[derive(Serialize, Deserialize)]
#[serde(crate = "smop::serde")] // only without a direct serde dependency
struct Record {
    name: String,
    score: u32,
}

// Read typed records
let records: Vec<Record> = fs::read_csv("scores.csv")?;

// Write typed records (with headers)
fs::write_csv("output.csv", &records)?;

// Raw access without types
let rows: Vec<Vec<String>> = fs::read_csv_rows("data.csv")?;
```

### xtask Pattern (features: sh, fs, path)
```rust
let root = path::workspace_root()?;                  // cargo locate-project --workspace
sh::cargo().args(["test", "--workspace"]).dir(&root).echo().run()?;
if !sh::cmd("cargo-nextest").arg("--version").success()? { /* not installed */ }
let status = sh::cmd("bash").arg("scripts/ci.sh").status()?;  // no error on non-zero
std::process::exit(status.code().unwrap_or(1));
if let Err(err) = sh::cmd("bash").arg("scripts/ci.sh").run().context("ci failed") {
    std::process::exit(sh::exit_code(&err).unwrap_or(1).into()); // 127 not found, 126 not executable, 128+N signal
}
fs::create_dir_all(root.join("target"))?;
let _guard = fs::lock(root.join("target/dist.lock"), std::time::Duration::from_secs(60))?; // flock -w 60 -x; released on drop
fs::remove_if_exists(root.join("target/dist"))?;              // Ok if missing
fs::copy_dir(root.join("assets"), root.join("target/dist"))?; // `to` must not exist
fs::write_atomic(root.join("openapi.json"), "{}\n")?;          // temp + rename; failure leaves the old file
fs::write_with_parents(root.join("target/a/b/c.txt"), "x")?;   // mkdir -p first; write_string does not
let n = fs::replace_in_file(root.join("Cargo.toml"), "0.3.0", "0.4.0")?; // count; 0 matches = Err, file unchanged
sh::cargo().args(["run", "--bin", "gen"]).stdout_atomic("gen.json").run()?; // renamed only on success
```

## MSRV

Rust 1.89 (`rust-version` in Cargo.toml and in smop-macros/Cargo.toml). `fs::lock` uses `std::fs::File::lock`, stable since 1.89; zip 9 and comfy-table 8 need 1.88; rusqlite 0.42 needs 1.89. CI checks it. Raising it is a breaking change: add a `changelog.d/` fragment and move both `rust-version` values, CI's MSRV job and this section together.

## Rust 2024 Notes

- `std::env::set_var` and `remove_var` are now unsafe
- Tests use `unsafe {}` blocks with `#[allow(unsafe_code)]` on test modules
- Crate uses `#![deny(unsafe_code)]` (not forbid) to allow test overrides

## Development Commands

```bash
cargo build                     # Build library
cargo build --examples          # Build all examples
cargo run --example fetch_api -- --url https://httpbin.org/json
cargo doc --no-deps --open      # Generate and view docs
```
