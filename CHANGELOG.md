# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.2.0]: https://github.com/madmax983/smop/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/madmax983/smop/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/madmax983/smop/releases/tag/v0.1.0
