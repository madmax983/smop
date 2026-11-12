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
│                 #   + read/write_json [json], read/write_toml [toml], glob [glob], CSV [csv]
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
│                 #   + Client::timeout(Duration)
├── print.rs      # [feature: print] Terminal: success!/warn!/error!, Spinner, Progress, prompts
├── cli.rs        # [feature: cli] Re-exports clap derive macros AND `clap` itself (derive output uses `clap::` paths)
├── time.rs       # [feature: time] Date/time parsing and formatting (chrono)
├── archive.rs    # [feature: archive] create/extract ZIP, TAR, TAR.GZ
│   └── archive/contain.rs  # One containment policy for all extract_* (ADR 0008)
├── re.rs         # [feature: re] `pub use regex::*` + compile, is_match, find(_all), captures(_all), replace(_all), split
├── hash.rs       # [feature: hash] sha256(bytes), sha256_file (64 KiB buffer), verify_sha256_file -> lowercase hex
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
| `glob`  | glob (+fs) | fs::glob |
| `http`  | ureq, serde_json | http module for sync HTTP requests |
| `cli`   | clap | Parser, Args, Subcommand derives |
| `print` | console, indicatif, dialoguer, comfy-table, serde_json | Terminal UI, macros, prompts |
| `csv`   | csv, serde (+fs) | read_csv, write_csv, read_csv_rows in fs module |
| `time`  | chrono | time module |
| `archive` | zip, tar, flate2 | archive module |
| `re`    | regex | re module: regex re-export and `&str`-pattern helpers |
| `hash`  | sha2 (no default features; 8 crates, 9 with libc on aarch64 Linux and Apple) | hash module: SHA-256 as lowercase hex |
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
8. **Strict `env` parsing** - a bad value is an error, never the default. `var_opt`/`var_or` read an empty value as unset; `var` gives it to the parser
9. **Archive containment** - `extract_*` return an error for absolute paths, `..` above `dest`, and links out of `dest`. No write follows a link out of `dest`. One module for all formats (ADR 0008)
10. **`re` = regex + helpers** - `smop::re` re-exports all of `regex`. Each helper compiles its `&str` pattern on each call. An invalid pattern is an error that quotes it (ADR 0009)
11. **`hash` = SHA-256 as hex** - No `sha2` type is in the public API. smop encodes hex itself (`sha2` 0.11 output has no `LowerHex`). `sha256_file` reads through one buffer (ADR 0010)
12. **Interrupt guard = `signal-hook`, no `unsafe`** - `script-signals` only; `script` has no deps. Handlers stay installed; a flag gives the default action back when the last guard drops. After an interrupt (`Ok` or `Err`), `script::main` stops by the same signal on Unix (a bash loop stops), 130 on Windows. A signal ignored at start (`nohup`) is caught, then gets its default action (ADR 0011)
13. **`sh` process I/O** - one std-only engine runs every run method. Threads drain stdout and stderr, so no deadlock. `timeout` kills and reaps the direct child and also bounds open output: code 124, captured output kept. `spawn` uses the engine and honours each I/O option; only `wait`/`wait_with_output` apply its timeout. It inherits stdout/stderr; `keep_output` keeps them for `wait_with_output`. `pipe` rejects I/O options (ADR 0012)
14. **`sh` pipes = pipefail** - each stage is a `CommandBuilder`. smop waits for each stage. The rightmost failed stage gives the code. A stage before the last that `SIGPIPE` stops is a success, so `yes | head -n1` is `Ok` (#43)
15. **`spawn_guarded` = group kill, no crate, no `unsafe`** - the child gets its own process group. `ChildGuard` drop runs `/bin/sh -c 'command kill -s KILL -- -PGID'` with no environment (Windows: `taskkill /T /F`), kills before the reap, ignores errors. SIGKILL only. `wait`/`wait_with_output` kill nothing. `sh::wait_until(timeout, || bool)` polls a probe and stops on an interrupt (ADR 0013)
16. **`git` = the `git` program through `sh`** - no crate. `Repo::new(dir)` runs `git -C dir` and removes `GIT_DIR` and the other repo-local variables (a hook sets them); free functions keep them. Each call sets `core.fsmonitor=` and removes `GIT_*_PATHSPECS`. Errors are `sh::CommandError` with git's stderr. Result paths are repo-relative, sorted, deduped; your paths are literal (`icase` with `core.ignorecase`). `changed_since(rev)` = index + work tree + untracked + submodules, so `changed_since("HEAD")` is empty only when `is_clean()` is true. A `rev` that is empty or starts with `-` or `:` is an error. `show` = `cat-file blob`, untrimmed (ADR 0014)
17. **Atomic writes** - one std-only module (`src/atomic.rs`) for `fs::write_atomic`, `fs::replace_in_file` and `sh` `stdout_atomic`: `create_new` temp file in the same directory (target's mode, else as `std::fs::write`; never more open than the target), target's permissions, `sync_all`, rename. A drop removes the temp file. A symlink target is resolved; the link stays. `stdout_atomic` renames only on exit 0, no timeout, no output error; an interrupt or `ChildProcess::kill` is an `Err` and no rename. `write_*` never create parents; `write_with_parents` does. `replace_in_file`: plain text, no match = `Err` and no write (ADR 0015)

Planned work is in `docs/roadmap.md`. Each deliverable there has one GitHub issue, under a tracking issue per release.

### API boundaries

A crate that smop re-exports is part of smop's API: `anyhow`, `clap`,
`serde`, `serde_json`, `toml` and `regex` (as `smop::re`). A major
release of that crate is a smop major release. One exception is not a whole
crate: `time` re-exports chrono's date types on purpose.

Do not put a type of any other dependency in a public signature, field or
trait impl. Wrap it in a smop type, as `print::Progress`, `print::Spinner`
and `fs::TempDir` do. `scripts/api_boundary.sh` (CI) finds such types in
rustdoc. To re-export a crate on purpose, add it to the allow list in that
script.

## Testing

```bash
cargo test --workspace --all-features  # All tests with all features
cargo test -p smop --no-default-features  # anyhow only
cargo test -p smop --no-default-features --features sh,fs,path  # the xtask configuration
# CI also runs test + clippy for every feature on its own (the list comes from
# scripts/ci-features.sh, i.e. cargo metadata); new code must compile under each.
# Tests needing serde/serde_json must be #[cfg(feature = "...")]-gated
# (tempfile is a dev-dependency, so tests may always use it).
# tests/archive_safety.rs builds archives with raw entry names (`..`, `/`) and
# has the extraction containment proptest. Use it for new extract_* tests.
# Its link tests and the proptest are #[cfg(unix)].
cargo clippy --workspace --all-targets --all-features -- -D warnings  # Lint check, tests included
cargo fmt --all --check  # Format check
bash scripts/api_boundary.sh  # No dependency types in the public API (needs jq, python3)
cargo llvm-cov --workspace --all-features --fail-under-lines 85  # CI's coverage gate
# cmd!: tests/cmd_macro.rs runs a script and reads back the argv (Unix).
# tests/ui/ has the compile-fail cases. Their snapshots are for one pinned
# toolchain and one feature set, so that test is #[ignore].
# tests/ui/README.md has the command.
# smop-macros: the decoder, lexer and validator are plain functions over &str.
# Test them there, with no proc_macro types.
cargo test -p smop-macros
```

CI also runs `cargo-semver-checks` against the latest smop on crates.io. The
release type comes from `version` in Cargo.toml, so during a breaking cycle
trunk carries the next breaking version (0.4.0 now). Behaviour-only changes are
invisible to it: list those in changelog.d/ by hand.

### Changelog

Do not edit `CHANGELOG.md` for a change. Add a fragment
`changelog.d/<name>.<section>.md` (see `changelog.d/README.md`), with a
migration note for a breaking change. CI runs `python3 scripts/changelog.py
check`.

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
```

### Derives without direct deps
clap and serde derives expand to code naming their crate. smop re-exports
`clap` (in `smop::cli` and the prelude), `smop::serde`, `smop::serde_json`,
`smop::toml` and `regex` as `smop::re`.
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
```
Tests use `mockito`; for "connection fails" use a closed localhost port, not a
bogus hostname (an HTTP proxy may answer for unknown hosts).

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

Rust 1.89 (`rust-version` in Cargo.toml and in smop-macros/Cargo.toml). `fs::lock` uses `std::fs::File::lock`, stable since 1.89; zip 9 and comfy-table 8 need 1.88. CI checks it. Raising it is a breaking change: add a `changelog.d/` fragment and move both `rust-version` values, CI's MSRV job and this section together.

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
