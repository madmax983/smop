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
├── path.rs       # Path utilities: home(), cwd(), expand()
├── env.rs        # Environment: var(), var_or(), dotenv(), require_vars()
├── fs.rs         # File ops: read_string, write_string, read_json, write_json, read_lines, append + CSV (feature-gated)
├── sh.rs         # Shell execution: run(), output(), cmd() builder - cross-platform
├── http.rs       # [feature: http] HTTP client: get, post, get_json, post_json
├── print.rs      # [feature: print] Terminal: success!/warn!/error!, spinner, progress, prompts
└── cli.rs        # [feature: cli] Re-exports clap derive macros
```

## Features

| Feature | Dependencies | What it enables |
|---------|--------------|-----------------|
| (core)  | anyhow, serde, dirs, shellexpand, dotenvy | env, fs, path, sh modules |
| `http`  | ureq | http module for sync HTTP requests |
| `cli`   | clap | Parser, Args, Subcommand derives |
| `print` | console, indicatif, dialoguer | Terminal UI, macros, prompts |
| `csv`   | csv | read_csv, write_csv, read_csv_rows in fs module |
| `full`  | All above | Everything (default) |

## Design Decisions

See `docs/adr/` for Architecture Decision Records:

1. **anyhow for errors** - `Result<T>` everywhere, no custom error types
2. **ureq for HTTP** - Sync-only, no tokio dependency (~20 deps vs ~300)
3. **Prelude philosophy** - `use smop::prelude::*` gets everything
4. **Feature flags** - Optional deps isolated, minimal core
5. **Cross-platform shell** - `cmd /C` on Windows, `sh -c` on Unix

## Testing

```bash
cargo test              # All tests with all features
cargo test --no-default-features  # Core only
cargo clippy            # Lint check
cargo fmt --check       # Format check
```

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
let timeout: u32 = env::var_or("TIMEOUT", 30); // Optional with default
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

let bar = print::progress(100);
for i in 0..100 {
    bar.inc(1);
}
bar.finish();

let name = print::prompt("What's your name?")?;
let confirm = print::confirm("Continue?")?;
```

### CSV Files (requires `csv` feature)
```rust
#[derive(Serialize, Deserialize)]
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
