# smop

[![CI](https://github.com/madmax983/smop/actions/workflows/ci.yml/badge.svg)](https://github.com/madmax983/smop/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/smop.svg)](https://crates.io/crates/smop)
[![Documentation](https://docs.rs/smop/badge.svg)](https://docs.rs/smop)
[![License](https://img.shields.io/crates/l/smop.svg)](LICENSE)

**Batteries-included scripting utilities for Rust.** Write Rust scripts like Python, but with a compiler that won't let them rot.

```rust
use smop::prelude::*;

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

## Features

- **Zero ceremony** - `use smop::prelude::*` and go
- **Error handling** - anyhow's `Result<T>` everywhere, with context
- **File I/O** - Read/write strings, JSON, CSV, lines
- **HTTP** - Sync GET/POST with JSON support (no async runtime needed)
- **Shell** - Cross-platform command execution (Windows + Unix)
- **Environment** - Typed env vars, dotenv loading
- **Terminal UI** - Spinners, progress bars, colored output, prompts

## Installation

```toml
[dependencies]
smop ="0.1"
```

Or with specific features:

```toml
[dependencies]
smop ={ version = "0.1", default-features = false, features = ["http", "csv"] }
```

## Quick Examples

### Environment & Config

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    env::dotenv()?;  // Load .env file

    let port: u16 = env::var("PORT")?;
    let timeout: u32 = env::var_or("TIMEOUT", 30);

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

    // JSON
    let config: Config = fs::read_json("config.json")?;
    fs::write_json("output.json", &data)?;

    // CSV (requires `csv` feature)
    let records: Vec<Record> = fs::read_csv("data.csv")?;
    fs::write_csv("output.csv", &records)?;

    // Lines
    let lines = fs::read_lines("data.txt")?;
    fs::append("log.txt", "New entry\n")?;

    Ok(())
}
```

### HTTP Requests

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // Simple GET
    let html = http::get("https://example.com")?;

    // JSON API
    let user: User = http::get_json("https://api.example.com/user/1")?;
    let created: User = http::post_json("https://api.example.com/users", &new_user)?;

    Ok(())
}
```

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

    Ok(())
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
    for _ in 0..100 {
        bar.inc(1);
    }
    bar.finish();

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

    Ok(())
}
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `full` | Yes | Everything below |
| `http` | Yes | HTTP client (ureq) |
| `cli` | Yes | CLI parsing (clap derives) |
| `print` | Yes | Terminal UI (spinners, progress, prompts) |
| `csv` | Yes | CSV read/write |

Minimal build (just core utilities):

```toml
smop ={ version = "0.1", default-features = false }
```

## Cargo Script (Future)

With [RFC 3424](https://rust-lang.github.io/rfcs/3424-cargo-script.html), you'll be able to write single-file scripts:

```rust
#!/usr/bin/env cargo
---
[dependencies]
smop ="0.1"
---

use smop::prelude::*;

fn main() -> Result<()> {
    let data: Vec<Record> = fs::read_csv("input.csv")?;
    success!("Loaded {} records", data.len());
    Ok(())
}
```

## Why scriptkit?

| | Python | Bash | Rust + scriptkit |
|---|--------|------|------------------|
| Type safety | Runtime errors | What errors? | Compile-time |
| Dependencies | pip chaos | Pray it's installed | Cargo.lock |
| IDE support | Variable | None | rust-analyzer |
| Performance | Slow | Fast-ish | Fast |
| Refactoring | Scary | Terrifying | Confident |

Scripts rot. Python scripts fail silently when APIs change. Bash scripts break on edge cases. Rust scripts fail to compile when something's wrong - and that's a feature.

## License

MIT OR Apache-2.0
