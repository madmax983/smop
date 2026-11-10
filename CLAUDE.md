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
│                 #   + read/write_json [json], read/write_toml [toml], glob [glob], CSV [csv]
├── sh.rs         # [feature: sh] run(), output(), cmd()/cargo() builders (echo, success, status, named errors)
│                 #   + CommandError (exit code, signal, command line) and exit_code()
├── http.rs       # [feature: http] typed get/post/*_json (errors quote status+body) + raw request()/Client::request() -> Request -> Response
│                 #   + Client::timeout(Duration)
├── print.rs      # [feature: print] Terminal: success!/warn!/error!, Spinner, Progress, prompts
└── cli.rs        # [feature: cli] Re-exports clap derive macros AND `clap` itself (derive output uses `clap::` paths)
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
| `print` | console, indicatif, dialoguer | Terminal UI, macros, prompts |
| `csv`   | csv, serde (+fs) | read_csv, write_csv, read_csv_rows in fs module |
| `time`  | chrono | time module |
| `archive` | zip, tar, flate2 | archive module |
| `full`  | All above | Everything (default) |

## Design Decisions

See `docs/adr/` for Architecture Decision Records:

1. **anyhow for errors** - `Result<T>` everywhere, no custom error types in signatures. `sh::CommandError` is inside the `anyhow::Error` (ADR amendment, 0.4)
2. **ureq for HTTP** - Sync-only, no tokio dependency (~40 crates for the `http` feature vs ~300)
3. **Prelude philosophy** - `use smop::prelude::*` gets everything
4. **Feature flags** - Optional deps isolated, minimal core. Types of optional deps stay out of the public API, except re-exported crates (see API boundaries below)
6. **Granular core features** - sh/fs/path/env/json/toml/glob are separate features so xtask crates stay light
5. **Cross-platform shell** - `cmd /C` on Windows, `sh -c` on Unix
7. **`cmd!` macro** (planned for 0.4) - zero-dependency proc-macro in a `smop-macros` crate behind a `macros` feature; one implementation, strict template grammar. Stable rustc never caches macro expansion, `macro_rules!` included (nightly `-Zcache-proc-macros` covers derives only), so caching was not a reason to avoid a proc-macro
8. **Strict `env` parsing** - a bad value is an error, never the default. `var_opt`/`var_or` read an empty value as unset; `var` gives it to the parser

Planned work is in `docs/roadmap.md`. Each deliverable there has one GitHub issue, under a tracking issue per release.

### API boundaries

A crate that smop re-exports is part of smop's API: `anyhow`, `clap`,
`serde` and `serde_json` (`toml` and `regex` are planned for 0.4). A major
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
cargo clippy --workspace --all-features --lib --examples -- -D warnings  # Lint check
cargo fmt --all --check  # Format check
bash scripts/api_boundary.sh  # No dependency types in the public API (needs jq, python3)
cargo llvm-cov --workspace --all-features --fail-under-lines 85  # CI's coverage gate
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

// Builder pattern for complex commands
sh::cmd("cargo")
    .args(["build", "--release"])
    .dir("./project")
    .env("RUSTFLAGS", "-C target-cpu=native")
    .run()?;
```

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
`clap` (in `smop::cli` and the prelude), `smop::serde` and `smop::serde_json`.
`tests/consumer/` is a crate depending only on smop; CI runs it. Never rely on
smop's own tests/doctests for this: cargo gives them every smop dependency as
`--extern`, which hides missing re-exports.
```rust
#[derive(Serialize, Deserialize)]
#[serde(crate = "smop::serde")] // only without a direct serde dependency
struct Config { name: String }
```

### HTTP: status-aware calls (requires `http` feature)
```rust
let resp = http::Client::new().request("POST", url).json(&body).send()?; // Err = transport only
match resp.status() { 404 => {}, 422 => { let e: ApiError = resp.json()?; } _ => { resp.error_for_status()?; } }
```
Tests use `mockito`; for "connection fails" use a closed localhost port, not a
bogus hostname (an HTTP proxy may answer for unknown hosts).

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
```

## MSRV

Rust 1.89 (`rust-version` in Cargo.toml). `fs::lock` uses `std::fs::File::lock`, stable since 1.89; zip 9 and comfy-table 8 need 1.88. CI checks it. Raising it is a breaking change: add a `changelog.d/` fragment and move `rust-version`, CI's MSRV job and this section together.

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
