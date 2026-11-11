# smop

[![CI](https://github.com/madmax983/smop/actions/workflows/ci.yml/badge.svg)](https://github.com/madmax983/smop/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/smop.svg)](https://crates.io/crates/smop)
[![Documentation](https://docs.rs/smop/badge.svg)](https://docs.rs/smop)
[![License](https://img.shields.io/crates/l/smop.svg)](LICENSE)

**Batteries-included scripting utilities for Rust.** Write Rust scripts like Python, but with a compiler that won't let them rot.

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    fs::write_string("hello.txt", "Hello, smop!")?;
    let text = fs::read_string("hello.txt")?;
    success!("Read back: {text}");
    Ok(())
}
```

Paste it into `src/main.rs` of a fresh `cargo new` project (see [Installation](#installation)) and `cargo run`.
It touches nothing but `hello.txt`; the larger script is under [A bigger script](#a-bigger-script).

## Features

- **Zero ceremony** - `use smop::prelude::*` and go
- **Error handling** - anyhow's `Result<T>` everywhere, with context
- **File I/O** - Read/write strings, JSON, TOML, CSV, lines, glob patterns, temp files
- **HTTP** - Full REST API support (GET/POST/PUT/PATCH/DELETE) with configurable client
- **Shell** - Cross-platform execution, pipe chaining, background processes
- **Environment** - Typed env vars, dotenv loading
- **Terminal UI** - Spinners, progress bars, colored output, prompts, tables
- **Time** - DateTime parsing/formatting, sleep utilities
- **Archives** - ZIP/TAR/TAR.GZ creation and extraction
- **Path** - Home/cwd, expansion, executable detection, workspace root
- **Regex** - `re::find_all`, `re::replace_all` and more on `&str` patterns, plus all of `regex`
- **Hashing** - SHA-256 of bytes or a file as lowercase hex, and pinned-digest checks
- **xtask-friendly** - Every module is feature-gated; `sh` + `fs` + `path` costs four small crates

## Installation

```toml
[dependencies]
smop = "0.3"
```

Or with specific features:

```toml
[dependencies]
smop = { version = "0.3", default-features = false, features = ["sh", "fs", "http", "csv", "time"] }
```

## A bigger script

CSV in, HTTP out. Derive macros work with no direct `serde` dependency because
smop re-exports it; the `#[serde(crate = ...)]` line is what makes that work.

```rust
use smop::prelude::*;

#[derive(Deserialize)]
#[serde(crate = "smop::serde")]
struct User {
    id: u32,
    name: String,
}

#[derive(Deserialize)]
#[serde(crate = "smop::serde")]
struct ApiResponse {
    status: String,
}

fn main() -> Result<()> {
    let users: Vec<User> = fs::read_csv("users.csv")?;

    for user in &users {
        let data: ApiResponse = http::get_json(&format!("https://api.example.com/{}", user.id))?;
        println!("{}: {}", user.name, data.status);
    }

    success!("Processed {} users", users.len());
    Ok(())
}
```

## Quick Examples

### Environment & Config

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    env::dotenv()?;  // Load .env file

    let port: u16 = env::var("PORT")?;
    let timeout: u32 = env::var_or("TIMEOUT", 30)?;  // Default if unset or empty
    let level: Option<u8> = env::var_opt("LEVEL")?;  // None if unset or empty

    env::require_vars(&["API_KEY", "DATABASE_URL"])?;

    Ok(())
}
```

### File Operations

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // Strings
    let content = fs::read_string("config.txt")?;
    fs::write_string("output.txt", "Hello, world!")?;

    // JSON & TOML
    let config: Config = fs::read_json("config.json")?;
    fs::write_json("output.json", &data)?;
    let settings: Settings = fs::read_toml("settings.toml")?;
    fs::write_toml("output.toml", &settings)?;

    // CSV (requires `csv` feature)
    let records: Vec<Record> = fs::read_csv("data.csv")?;
    fs::write_csv("output.csv", &records)?;

    // Lines & patterns
    let lines = fs::read_lines("data.txt")?;
    fs::append("log.txt", "New entry\n")?;
    let rs_files = fs::glob("src/**/*.rs")?;

    // Bytes
    let blob = fs::read_bytes("logo.png")?;
    fs::write_bytes("copy.png", &blob)?;

    // File operations
    fs::copy("src.txt", "dst.txt")?;           // files only
    fs::copy_dir("assets", "dist")?;           // `to` must not exist
    fs::rename("old.txt", "new.txt")?;
    fs::move_path("/tmp/out", "target/out")?;  // also across file systems
    fs::remove("file_or_dir")?;
    fs::remove_if_exists("target/stale")?;     // Ok if missing
    let files = fs::walk("src")?;              // all files below, sorted

    // Temporary files. Drop removes a TempDir; keep() leaves it on disk
    let (file, path) = fs::temp_file()?;
    let temp_dir = fs::temp_dir()?;
    let work = fs::TempDir::with_prefix_in("build-", "target")?;
    let kept: std::path::PathBuf = work.keep();

    // A lock that works with flock(1); released when the guard drops
    let _guard = fs::lock("build.lock", std::time::Duration::from_secs(60))?;

    Ok(())
}
```

### HTTP Requests

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // Simple requests
    let html = http::get("https://example.com")?;
    let body = http::post("https://api.example.com", "data")?;

    // Full REST API support
    let user: User = http::get_json("https://api.example.com/user/1")?;
    let created: User = http::post_json("https://api.example.com/users", &new_user)?;
    let updated: User = http::put_json("https://api.example.com/user/1", &user)?;
    let patched: User = http::patch_json("https://api.example.com/user/1", &patch)?;
    http::delete("https://api.example.com/user/1")?;

    // Download files
    http::download("https://example.com/file.zip", "local.zip")?;

    // Configurable client
    let client = http::Client::new()
        .timeout(std::time::Duration::from_secs(30))
        .header("X-Api-Key", "secret");
    let response = client.get("https://api.example.com/data")?;

    Ok(())
}
```

Non-2xx responses are errors that quote the status and the response body. When
you need to branch on the status (a 404, a 409, a 422 with a validation
payload), use the raw layer, which returns every response:

```rust
let gh = http::Client::new().header("Authorization", "Bearer TOKEN");
let resp = gh
    .request("POST", "https://api.github.com/repos/o/r/labels")
    .json(&smop::serde_json::json!({ "name": "bug" }))
    .send()?;                         // Err only for transport failures

match resp.status() {
    201 => println!("created"),
    422 => println!("already exists: {}", resp.text()?),
    _ => { resp.error_for_status()?; } // "failed with status 500: <body>"
}
```

`Response` also has `is_success()`, `header(name)`, `headers()`, `bytes()`,
`into_text()` and `json::<T>()`.

### Shell Commands

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // Simple commands (cross-platform: uses cmd.exe on Windows, sh on Unix)
    sh::run("git status")?;
    let branch = sh::output("git rev-parse --abbrev-ref HEAD")?;

    // Builder for complex commands
    sh::cmd("cargo")
        .args(["build", "--release"])
        .dir("./my-project")
        .env("RUSTFLAGS", "-C target-cpu=native")
        .run()?;

    // Pipe chaining (Unix)
    let result = sh::cmd("ls")
        .pipe("grep", &["txt"])
        .pipe("wc", &["-l"])
        .output()?;

    // Background processes
    let mut child = sh::cmd("long-running-server").spawn()?;
    // ... do work ...
    child.kill()?;

    // Shell-style exit codes: 127 not found, 126 not executable, 128 + N for signal N
    if let Err(err) = sh::run("exit 3").context("step failed") {
        assert_eq!(sh::exit_code(&err), Some(3)); // found below .context()
    }

    Ok(())
}
```

Command errors name the command line and the code
(`Command failed with exit code 3: exit 3`, or
`Command killed by signal 9 (exit code 137): ...`). They hold a
`sh::CommandError` with the code, the signal, the command line and the
captured stderr.

### Script Lifecycle

```rust
use std::time::Duration;
use smop::prelude::*;

fn main() {
    // Prints "error: ..." and "caused by: ..." lines, then exits with the
    // code of a failed command, or 1.
    script::main(|| {
        let _guard = script::guard()?; // Ctrl-C: cleanup runs, then the process stops
        sh::cmd("docker").args(["run", "-d", "--name", "db", "postgres"]).run()?;
        let _cleanup = script::defer(sh::cmd("docker").args(["rm", "-f", "db"]));
        // Waits 1 s, 2 s, 4 s and 8 s between attempts.
        script::retry(5, Duration::from_secs(1), || sh::run("pg_isready -h localhost"))?;
        script::timed("tests", || sh::cargo().arg("test").run()) // "tests: 12.34s"
    })
}
```

### Terminal UI

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // Colored output
    success!("Build complete");
    warn!("Deprecated API");
    error!("Connection failed");

    // Spinner for long operations
    let spinner = print::spinner("Downloading...");
    // ... do work ...
    spinner.finish();

    // Progress bar
    let bar = print::progress(100);
    for i in 0..100 {
        bar.set_message(format!("item {i}"));
        bar.inc(1);
    }
    bar.finish_with_message("done");

    // Tables
    let headers = &["Name", "Score"];
    let rows = vec![
        vec!["Alice".to_string(), "95".to_string()],
        vec!["Bob".to_string(), "87".to_string()],
    ];
    println!("{}", print::table(headers, &rows));

    // JSON output
    print::print_json(&data)?;

    // Interactive prompts
    let name = print::prompt("What's your name?")?;
    let port = print::prompt_default("Port", "8080")?;

    if print::confirm("Continue?")? {
        // ...
    }

    Ok(())
}
```

### Path Utilities

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    let home = path::home();
    let cwd = path::cwd()?;
    let expanded = path::expand("~/Documents/$PROJECT");

    // Find executables
    let git_path = path::which("git")?;

    // Check if executable
    if path::is_executable("/usr/bin/python3")? {
        println!("Python is executable");
    }

    Ok(())
}
```

### Time & Dates

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // Current time
    let now = time::now();
    let local = time::now_local();

    // Parsing & formatting
    let dt = time::parse("2024-01-15 14:30:00", "%Y-%m-%d %H:%M:%S")?;
    let formatted = time::format(&now, "%Y-%m-%d");

    // Sleep
    time::sleep_secs(2);
    time::sleep_millis(500);

    Ok(())
}
```

### Archives

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // ZIP
    archive::create_zip("source_dir", "archive.zip")?;
    archive::extract_zip("archive.zip", "output_dir")?;

    // TAR
    archive::create_tar("source_dir", "archive.tar")?;
    archive::extract_tar("archive.tar", "output_dir")?;

    // TAR.GZ
    archive::create_tar_gz("source_dir", "archive.tar.gz")?;
    archive::extract_tar_gz("archive.tar.gz", "output_dir")?;

    Ok(())
}
```

Extraction stays in the output directory. These entries cause an error that
names the entry:

- an absolute path (`/x`)
- a path that goes above the output directory (`../x`)
- a link that points outside the output directory, or to an absolute path

See [ADR 0008](docs/adr/0008-safe-archive-extraction.md).

### Regex

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // Helpers take the pattern as `&str`. A bad pattern gives
    // "Invalid regex pattern '<pattern>'".
    if re::is_match(r"^v\d+", "v1.2")? {
        println!("version tag");
    }
    let nums = re::find_all(r"\d+", "a1 b22")?; // ["1", "22"]
    let swapped = re::replace_all(r"(\w+)=(\w+)", "a=1", "$2=$1")?; // "1=a"
    if let Some(caps) = re::captures(r"(?<user>\w+)@(?<host>\S+)", "me@example.com")? {
        println!("{} at {}", &caps["user"], &caps["host"]);
    }

    // In a loop, compile one time. `re::Regex` is regex's type.
    let comment = re::compile(r"^\s*#")?;
    for line in fs::read_lines("config.ini")? {
        if !comment.is_match(&line) {
            println!("{line}");
        }
    }
    println!("{nums:?} {swapped}");
    Ok(())
}
```

### Hashing

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    let digest = hash::sha256(b"abc"); // "ba7816bf...", lowercase hex
    println!("{digest}");

    // `sha256_file` reads through one 64 KiB buffer. Errors name the path.
    println!("{}  Cargo.lock", hash::sha256_file("Cargo.lock")?);

    // A mismatch error shows the path and the two digests.
    let pinned = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    hash::verify_sha256_file("downloads/tool.tar.gz", pinned)?;
    Ok(())
}
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `full` | Yes | Everything below |
| `sh` | Yes | Shell: `run`, `output`, `cmd()` / `cargo()` builders, pipes, background processes (no deps) |
| `fs` | Yes | File I/O: strings, lines, append, copy/rename/remove, `create_dir_all`, temp files, file locks (`lock`/`lock_shared`) (tempfile) |
| `path` | Yes | `home`, `cwd`, `expand`, `which`, `is_executable`, `workspace_root` (which, shellexpand) |
| `env` | Yes | Typed env vars, dotenv loading (dotenvy) |
| `json` | Yes | `fs::read_json` / `fs::write_json` (serde_json; implies `fs`) |
| `toml` | Yes | `fs::read_toml` / `fs::write_toml` (toml; implies `fs`) |
| `glob` | Yes | `fs::glob` (glob; implies `fs`) |
| `serde` | Yes | Re-exports `Serialize` / `Deserialize` in the prelude and `smop::serde` (implied by json/toml/csv/http/print) |
| `http` | Yes | HTTP client (ureq) - typed GET/POST/PUT/PATCH/DELETE, raw `Request`/`Response`, file downloads |
| `cli` | Yes | CLI parsing (clap derives, plus `clap` itself for the derive output) |
| `print` | Yes | Terminal UI (spinners, progress, prompts, tables) |
| `csv` | Yes | CSV read/write (implies `fs`) |
| `time` | Yes | DateTime parsing/formatting, sleep utilities (chrono) |
| `archive` | Yes | ZIP/TAR/TAR.GZ creation and extraction |
| `re` | Yes | `smop::re`: all of `regex`, plus `&str`-pattern helpers (regex) |
| `hash` | Yes | `hash::sha256`, `hash::sha256_file`, `hash::verify_sha256_file` (sha2) |
| `script` | Yes | `script::main` (exit codes), `defer`, `retry`, `timed` (no deps; implies `sh`) |
| `script-signals` | Yes | `script::guard`: cleanup after Ctrl-C, SIGTERM, SIGHUP (signal-hook) |

`default-features = false` leaves only `anyhow` (`smop::Result`, `smop::Context`);
add back exactly the modules you use.

### No extra dependencies for derives

smop re-exports everything its derives need, so `smop` can be your only
dependency:

```rust
use smop::prelude::*; // brings `clap` into scope for #[derive(Parser)]

#[derive(Parser)]
struct Cli { #[arg(long)] dry_run: bool }

#[derive(Serialize, Deserialize)]
#[serde(crate = "smop::serde")] // only needed without a direct `serde` dependency
struct Config { name: String }

let v = smop::serde_json::json!({ "name": "x" }); // `smop::serde_json` re-export
let t: smop::toml::Value = smop::toml::from_str("a = 1")?; // `smop::toml` re-export
let r = smop::re::Regex::new(r"\d+")?; // `regex` re-export as `smop::re`
```

With explicit imports instead of the prelude, import `clap` alongside the
derives: `use smop::cli::{Parser, clap};`.

### Using smop in a `cargo xtask`

An xtask crate is a workspace member, so everything it depends on is compiled by
`cargo build --workspace`. The core modules are cheap on their own:

```toml
# xtask/Cargo.toml
[dependencies]
smop = { version = "0.3", default-features = false, features = ["sh", "fs", "path"] }
```

That adds `anyhow`, `tempfile`, `which` and `shellexpand` (12 crates on Linux including transitive deps, versus ~140 for `full`).

```rust
use smop::{Context, Result, fs, path, sh};

fn main() -> Result<()> {
    let root = path::workspace_root()?;          // `cargo locate-project --workspace`
    path::which("bash").context("needs bash")?;  // clear error on Windows

    sh::cargo()                                  // honours $CARGO from `cargo xtask`
        .args(["clippy", "--workspace", "--", "-D", "warnings"])
        .dir(&root)
        .echo()                                  // prints `$ cargo clippy --workspace -- -D warnings`
        .run()?;                                 // error names the command and exit code

    let notes = fs::read_string(root.join("CHANGELOG.md"))?; // error names the path
    println!("{} lines of changelog", notes.lines().count());
    Ok(())
}
```

See [`examples/xtask.rs`](examples/xtask.rs) for a runnable version.

## Cargo Script (Future)

With [RFC 3424](https://rust-lang.github.io/rfcs/3424-cargo-script.html), you'll be able to write single-file scripts:

```rust
#!/usr/bin/env cargo
---
[dependencies]
smop = "0.3"
---

use smop::prelude::*;

fn main() -> Result<()> {
    let data: Vec<Record> = fs::read_csv("input.csv")?;
    success!("Loaded {} records", data.len());
    Ok(())
}
```

## Why smop?

| | Python | Bash | Rust + smop |
|---|--------|------|---------------|
| Type safety | Runtime errors | What errors? | Compile-time |
| Dependencies | pip chaos | Pray it's installed | Cargo.lock |
| IDE support | Variable | None | rust-analyzer |
| Performance | Slow | Fast-ish | Fast |
| Refactoring | Scary | Terrifying | Confident |

Scripts rot. Python scripts fail silently when APIs change. Bash scripts break on edge cases. Rust scripts fail to compile when something's wrong - and that's a feature.

## License

MIT OR Apache-2.0
