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
- **Shell** - Cross-platform execution, `cmd!` command templates, pipe chaining, background processes
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
smop = "0.4"
```

Or with specific features:

```toml
[dependencies]
smop = { version = "0.4", default-features = false, features = ["sh", "fs", "http", "csv", "time"] }
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

    // Safe writes
    fs::write_atomic("openapi.json", "{}\n")?; // temp file + rename; keeps permissions
    fs::write_with_parents("out/a/b/c.txt", "x")?; // mkdir -p, then write
    let n = fs::replace_in_file("Cargo.toml", "0.3.0", "0.4.0")?; // no match = Err

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
let gh = http::Client::new().bearer("TOKEN");
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

Query strings, bearer tokens and retries:

```rust
// GET https://api.example.com/search?q=a%20b&page=2
let client = http::Client::new()
    .bearer("TOKEN")                  // Authorization: Bearer TOKEN
    .retry(http::Retry::new(3));      // like `curl --retry 3`
let resp = client
    .request("GET", "https://api.example.com/search")
    .query("q", "a b")                // percent-encoded
    .query("page", "2")
    .send()?;

// Download with retries, then check the SHA-256.
http::Client::new()
    .retry(http::Retry::new(3).max_delay(std::time::Duration::from_secs(30)))
    .download("https://example.com/tool.tar.gz", "tool.tar.gz")?;
let pinned = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
hash::verify_sha256_file("tool.tar.gz", pinned)?;
```

A retry occurs after a timeout, or after status 408, 429, 500, 502, 503 or
504. The wait starts at 1 s and doubles. A `Retry-After` header (seconds or
HTTP-date) replaces it. `max_delay` (60 s) is the longest wait. Only GET,
HEAD, PUT, DELETE, OPTIONS and TRACE retry; `.any_method()` adds POST, PATCH
and all other methods. Retries are off unless you set them. Each retry
writes one `warning:` line to stderr, without the query values. Under
`script::guard`, Ctrl-C stops the wait.

### Shell Commands

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // Simple commands (cross-platform: uses cmd.exe on Windows, sh on Unix)
    sh::run("git status")?;
    let branch = sh::output("git rev-parse --abbrev-ref HEAD")?;

    // Shell line with a working directory and environment
    sh::shell("make && make install")
        .dir("./my-project")
        .env("PREFIX", "/usr/local")
        .run()?;

    // Builder for complex commands
    sh::cmd("cargo")
        .args(["build", "--release"])
        .dir("./my-project")
        .env("RUSTFLAGS", "-C target-cpu=native")
        .run()?;

    // Command template: reads as a shell line, but no shell runs
    let msg = "Fix the build";
    let files = ["src/a b.rs", "src/c.rs"];
    cmd!("git commit -m {msg} -- {files...}").run()?;
    // argv: git, commit, -m, "Fix the build", --, "src/a b.rs", "src/c.rs"

    // Pipe chaining (Unix), with pipefail: a failed stage is an error.
    // Each stage is a builder, with its own dir, env and arguments.
    let count = sh::cmd("git")
        .arg("ls-files")
        .dir("./my-project")
        .pipe(sh::cmd("wc").arg("-l"))
        .output()?;
    // grep exits with 1 for no match. `|| true` makes no match not an error.
    let todos = sh::cmd("git")
        .args(["grep", "-n", "TODO"])
        .pipe(sh::shell("grep -v vendor/ || true"))
        .output()?;

    // Background processes: stdout and stderr go to the terminal (0.4)
    let mut child = sh::cmd("long-running-server").spawn()?;
    // ... do work ...
    child.kill()?;
    // Opt-in capture: threads read the output, so the child never blocks
    let build = sh::cargo().arg("build").keep_output().spawn()?;
    println!("build: {}", build.wait_with_output()?.code()); // sh::Captured

    // Atomic stdout: a failed generator leaves the old file
    sh::cargo().args(["run", "--bin", "openapi"]).stdout_atomic("openapi.json").run()?;

    // A guarded server: the guard kills its whole process group on drop
    let _server = sh::shell("npm run serve").stdout_file("serve.log").stderr_to_stdout().spawn_guarded()?;
    sh::wait_until(std::time::Duration::from_secs(30), || {
        sh::cmd("curl").args(["-fsS", "localhost:8080/health"]).success().unwrap_or(false)
    })?;

    // Shell-style exit codes: 127 not found, 126 not executable, 128 + N for signal N
    if let Err(err) = sh::run("exit 3").context("step failed") {
        assert_eq!(sh::exit_code(&err), Some(3)); // found below .context()
    }

    // Capture with status: a non-zero exit is not an error
    let diff = sh::cmd("git").args(["diff", "--quiet"]).capture()?;
    println!("code {}: {}", diff.code(), diff.stderr());

    // cargo test 2>&1 | tee test.log, shown live, killed after 10 min (code 124)
    sh::cmd("cargo")
        .arg("test")
        .stderr_to_stdout()
        .log_stdout("test.log")
        .timeout(std::time::Duration::from_secs(600))
        .run()?;

    // stdin, lines, redirection and quiet runs
    let sorted = sh::cmd("sort").stdin("b\na\n").output_lines()?; // ["a", "b"]
    sh::cmd("date").stdout_append("dates.txt").run()?; // date >> dates.txt
    sh::cmd("npm").arg("ci").quiet().run()?; // on failure, the error has the stderr tail

    Ok(())
}
```

Command errors name the command line and the code
(`Command failed with exit code 3: exit 3`, or
`Command killed by signal 9 (exit code 137): ...`). They hold a
`sh::CommandError` with the code, the signal, the command line and the
captured output. Threads read stdout and stderr while smop waits, so a
command that writes much to both streams does not block.

`cmd!` (the `macros` feature) splits its template into words at compile
time and gives the same builder as `sh::cmd`:

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    let (dir, jobs) = (std::path::Path::new("native libs"), 8);
    let verbose: Option<&str> = None;

    // {name}: exactly one argument. {name...}: one argument for each item.
    cmd!("make -C {dir} -j{jobs} {verbose...} 'CFLAGS=-O2 -g'").run()?;

    // An expression gets a name after the template.
    let tag = cmd!("git describe --tags {rev}", rev = "HEAD~1").output()?;
    println!("{tag}");
    Ok(())
}
```

- The macro never splits a value and never reads it as shell syntax.
- `{name}` takes an `AsRef<OsStr>` value, an integer or a `char`.
- `{name...}` takes a list: a `Vec`, a slice, an array or an `Option`.
  A path is one argument, so `{name...}` with a path is a compile error.
- `'...'` and `"..."` hold text with spaces. `{{` and `}}` are braces.
- Shell syntax outside quotes (`&&`, `|`, `>`, `$HOME`, `~`) is a compile
  error. For a real shell line, use `sh::shell`.

`sh::run(line)` and `sh::output(line)` are `sh::shell(line).run()` and
`sh::shell(line).output()`. The meaning of extra `.arg()` values to
`sh::shell` is different on each platform. On Unix, they are `$0`, `$1`, and
so on. The shell does not parse them. On Windows, they go after the command
text, and cmd.exe parses them.

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
    info!("Using cached index"); // stderr
    step!("Test");               // stderr

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
    let profile = print::select("Profile", &["debug", "release"])?; // index
    let targets = print::multiselect("Targets", &["linux", "macos"])?; // indexes
    let token = print::password("API token")?; // not shown

    // No terminal on stdin or stderr: a prompt does not wait.
    // `*_default` returns the default; the others return an error.
    let go = print::confirm_default("Deploy?", false)?; // false in CI

    // GitHub Actions annotations (`GITHUB_ACTIONS=true`), else plain lines
    print::ci_error("Cargo.lock is out of date"); // ::error::…
    print::ci_warning("Slow tests skipped");      // ::warning::…
    print::group("cargo test", || sh::cargo().arg("test").run())?; // ::group::…

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

### Git

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    ensure!(git::is_clean()?, "commit your changes first"); // untracked files count
    let head = git::head()?; // 40 hex characters (64 in a SHA-256 repository)
    let branch = git::branch()?.unwrap_or_else(|| "(detached)".into());
    println!("{branch} at {head}");

    // Sorted, relative to the repository root. Uncommitted changes are in it.
    for path in git::changed_since("origin/main")? {
        println!("changed: {}", path.display());
    }
    let old = git::show("HEAD~1", "Cargo.toml")?; // the file as it was committed
    let tracked = git::ls_files()?.len();
    let ignored = git::is_ignored("target")?;
    println!("{} bytes, {tracked} files, target ignored: {ignored}", old.len());

    // Another repository: the same functions on `git::Repo`.
    let vendored = git::Repo::new("vendor/lib");
    ensure!(vendored.is_clean_paths(["src"])?, "vendor/lib/src has changes");
    Ok(())
}
```

Outside a repository, the error has the stderr of git, and `sh::exit_code`
gives 128. Some queries can run programs that the repository config names
(for example a clean filter). Do not query a repository that you do not
trust.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `full` | Yes | Everything below |
| `sh` | Yes | Shell: `run`, `output`, `cmd()` / `cargo()` builders, capture, tee, redirection, stdin, timeout, pipes, background processes (no deps) |
| `fs` | Yes | File I/O: strings, lines, append, copy/rename/remove, `create_dir_all`, temp files, file locks (`lock`/`lock_shared`) (tempfile) |
| `path` | Yes | `home`, `cwd`, `expand`, `which`, `is_executable`, `workspace_root` (which, shellexpand) |
| `env` | Yes | Typed env vars, dotenv loading (dotenvy) |
| `json` | Yes | `fs::read_json` / `fs::write_json` (serde_json; implies `fs`) |
| `toml` | Yes | `fs::read_toml` / `fs::write_toml` (toml; implies `fs`) |
| `glob` | Yes | `fs::glob` (glob; implies `fs`) |
| `serde` | Yes | Re-exports `Serialize` / `Deserialize` in the prelude and `smop::serde` (implied by json/toml/csv/http/print) |
| `http` | Yes | HTTP client (ureq) - typed GET/POST/PUT/PATCH/DELETE, raw `Request`/`Response`, file downloads, query, bearer, retries |
| `cli` | Yes | CLI parsing (clap derives, plus `clap` itself for the derive output) |
| `print` | Yes | Terminal UI (status lines, spinners, progress, prompts, tables, GitHub Actions annotations) |
| `csv` | Yes | CSV read/write (implies `fs`) |
| `time` | Yes | DateTime parsing/formatting, sleep utilities (chrono) |
| `archive` | Yes | ZIP/TAR/TAR.GZ creation and extraction |
| `re` | Yes | `smop::re`: all of `regex`, plus `&str`-pattern helpers (regex) |
| `hash` | Yes | `hash::sha256`, `hash::sha256_file`, `hash::verify_sha256_file` (sha2) |
| `git` | Yes | `git::head`, `branch`, `is_clean`, `changed_since`, `ls_files`, `show`, `is_ignored`, and `git::Repo` for another directory (no deps; implies `sh`) |
| `script` | Yes | `script::main` (exit codes), `defer`, `retry`, `timed` (no deps; implies `sh`) |
| `script-signals` | Yes | `script::guard`: cleanup after Ctrl-C, SIGTERM, SIGHUP (signal-hook) |
| `macros` | Yes | `cmd!`: command templates with no shell and no word splitting (smop-macros, a proc-macro with no dependencies; implies `sh`) |

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
smop = { version = "0.4", default-features = false, features = ["sh", "fs", "path"] }
```

That adds `anyhow`, `tempfile`, `which` and `shellexpand` (12 crates on Linux including transitive deps, versus ~140 for `full`).
Add `macros` for `cmd!`. It adds one crate, `smop-macros`, which has no dependencies.

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

## Runnable Examples

Each file in [`examples/`](examples) is a full script. Run one from this
repository:

| Command | What it shows |
|---------|---------------|
| `cargo run --example env_report` | strict `env` parsing, `.env`, JSON and TOML output |
| `cargo run --example shell_automation` | `cmd!`, `script::main`, `script::timed`, `cargo metadata` as JSON |
| `cargo run --example archive_backup` | a `.tar.gz` backup, restore and check, `fs::walk` |
| `cargo run --example xtask -- root` | a `cargo xtask` with `sh`, `fs` and `path` only |
| `cargo run --example file_processor` | a progress bar that threads share |
| `cargo run --example interactive_cli` | prompts, then a JSON config file |
| `cargo run --example fetch_api -- --url https://httpbin.org/json` | `clap` arguments, a spinner, HTTP JSON (needs the network) |
| `cargo run --example bolt_workload` | profiling harness (use `--release`): run it under `valgrind --tool=callgrind` or `--tool=dhat` |

By default, the first three examples write their output below
`target/smop-example-output/`. CI builds each example with only the
features in its `required-features`.

## Cargo Script (Future)

With [RFC 3424](https://rust-lang.github.io/rfcs/3424-cargo-script.html), you'll be able to write single-file scripts:

```rust
#!/usr/bin/env cargo
---
[dependencies]
smop = "0.4"
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
