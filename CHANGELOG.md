# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - Unreleased

Planned in [docs/roadmap.md](docs/roadmap.md) (tracking issue #31). Breaking
changes will each get a migration note here.

### Breaking
- **MSRV is now Rust 1.89** (#33), for `std::fs::File::lock`. Migration:
  build with Rust 1.89 or later (`rustup update`).
- **`sh` spawn failures are a `sh::CommandError`** (#34), with the
  `io::Error` as its source. `err.downcast_ref::<io::Error>()` now gives
  `None`. The error text is the same. Migration: use
  `sh::exit_code(&err) == Some(127)`, or search `err.chain()` for the
  `io::Error`.

### Added
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
- `ChildProcess` implements `Debug`.
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
- CI quality gates (#32):
  - a line-coverage gate: `cargo llvm-cov --workspace --all-features
    --fail-under-lines 85`, the low end of the project's 85–90% standard
    (91.5% today)
  - `cargo-semver-checks` against the latest smop on crates.io
- Tests for `cli` (`Parser`, `Args`, `Subcommand` and `ValueEnum` derives
  through the prelude, including the errors for bad input) and for `print`
  (status lines, JSON serialization errors, and prompts without a terminal,
  which fail instead of waiting). `print.rs` is covered at 98%, up from 68%.

### Changed
- `fs::copy` is documented as file-only. Before, the docs said "a file or
  directory", but a directory always failed. A directory now gives an error
  that names `fs::copy_dir` (#37).
- `fs::copy(a, a)`, or a copy onto a hard link of `a`, is an error. Before,
  it returned `Ok` and emptied the file (#37).
- Error text for failed commands. Errors that said "exit code N" still do.
  - Death by signal: `Command killed by signal 9 (exit code 137): <command>`.
    Before: `exit code -1`.
  - `PipeBuilder` errors name the full command line and code of the stage
    that failed: `Command failed with exit code 1: grep -q "no match"`. Only
    the last stage can fail with an exit code. Earlier stages fail only when
    they do not start. Before: `output` gave `Pipe command failed: grep`, and
    `run` gave `Pipe command failed with exit code 1: grep`.
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
- `Cargo.toml` declares a workspace, and the lint table moved to
  `[workspace.lints]`, so `smop-macros` (ADR 0007) can join and share it.
  CI passes `--workspace`, and its per-feature loops read the feature list
  from `cargo metadata` (`scripts/ci-features.sh`) instead of a hand-written
  copy.
- The version on trunk is 0.4.0 for the 0.4 cycle, so `cargo-semver-checks`
  treats changes against 0.3.2 as a breaking release. Once 0.4.0 is
  published, the same check rejects breaking changes in 0.4.x.

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

## [0.1.1] - 2024-XX-XX

### Changed
- Added repository, homepage, and documentation metadata
- Fixed GitHub username in README badges
- Added required-features to examples for individual feature builds

## [0.1.0] - 2024-XX-XX

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

[0.2.1]: https://github.com/madmax983/smop/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/madmax983/smop/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/madmax983/smop/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/madmax983/smop/releases/tag/v0.1.0
