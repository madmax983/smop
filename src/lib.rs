//! # smop
//!
//! Batteries-included scripting utilities for Rust.
//!
//! `use smop::prelude::*` and write Rust scripts like Python,
//! but with a compiler that won't let them rot.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use smop::prelude::*;
//!
//! # #[cfg(all(feature = "fs", feature = "path"))]
//! fn main() -> Result<()> {
//!     let home = path::home();
//!     let content = fs::read_string(home.join(".bashrc"))?;
//!     println!("Bashrc has {} lines", content.lines().count());
//!     Ok(())
//! }
//! # #[cfg(not(all(feature = "fs", feature = "path")))]
//! # fn main() {}
//! ```
//!
//! ## Features
//!
//! Everything is on by default (`full`). Every module, including the core
//! ones, sits behind its own feature so you can opt out of what you don't use:
//!
//! | Feature   | Module / items                         | Dependencies added          |
//! |-----------|----------------------------------------|-----------------------------|
//! | `sh`      | `sh`                                   | none                        |
//! | `fs`      | `fs` (read/write, temp files, …)       | `tempfile`                  |
//! | `path`    | `path` (`home`, `expand`, `which`, …)  | `which`, `shellexpand`      |
//! | `env`     | `env` (`var`, `var_or`, `is_ci`, …)    | `dotenvy`                   |
//! | `json`    | `fs::read_json` / `fs::write_json`     | `serde`, `serde_json`       |
//! | `toml`    | `fs::read_toml` / `fs::write_toml`     | `serde`, `toml`             |
//! | `glob`    | `fs::glob`                             | `glob`                      |
//! | `csv`     | `fs::read_csv` and friends             | `serde`, `csv`              |
//! | `http`    | `http`                                 | `ureq`, `serde_json`        |
//! | `cli`     | clap derive re-exports                 | `clap`                      |
//! | `print`   | `print`, `success!`/`warn!`/`error!`   | `console`, `indicatif`, …   |
//! | `time`    | `time`                                 | `chrono`                    |
//! | `archive` | `archive`                              | `zip`, `tar`, `flate2`      |
//! | `re`      | `re` (`regex` re-export and helpers)   | `regex`                     |
//!
//! `anyhow` is the only unconditional dependency.
//!
//! ## Using smop in a `cargo xtask`
//!
//! An xtask crate lives in your workspace, so every dependency it pulls in is
//! compiled by `cargo build --workspace`. Turn off the defaults and take only
//! the core modules:
//!
//! ```toml
//! [dependencies]
//! smop = { version = "0.3", default-features = false, features = ["sh", "fs", "path"] }
//! ```
//!
//! ```rust,no_run
//! # #[cfg(all(feature = "sh", feature = "fs", feature = "path"))]
//! fn main() -> smop::Result<()> {
//!     use smop::{fs, path, sh};
//!
//!     let root = path::workspace_root()?;
//!     path::which("bash")?; // clear error on Windows if bash is missing
//!     sh::cargo()
//!         .args(["test", "--workspace"])
//!         .dir(&root)
//!         .echo() // prints "$ cargo test --workspace"
//!         .run()?; // on failure: "Command failed with exit code 101: cargo test --workspace"
//!
//!     let version = fs::read_string(root.join("VERSION"))?;
//!     println!("released {version}");
//!     Ok(())
//! }
//! # #[cfg(not(all(feature = "sh", feature = "fs", feature = "path")))]
//! # fn main() {}
//! ```

// Note: Using deny instead of forbid to allow unsafe in tests (required for
// std::env::set_var/remove_var in Rust 2024 edition)
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

#[cfg(feature = "env")]
pub mod env;

#[cfg(feature = "fs")]
pub mod fs;

#[cfg(feature = "path")]
pub mod path;

#[cfg(feature = "sh")]
pub mod sh;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "print")]
pub mod print;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "time")]
pub mod time;

#[cfg(feature = "archive")]
pub mod archive;

#[cfg(feature = "re")]
pub mod re;

pub mod prelude;

// Re-export core error handling
pub use anyhow::{self, Context, Result};

/// The `serde` crate, so derives work without a direct dependency:
/// `#[derive(Serialize)] #[serde(crate = "smop::serde")] struct S { .. }`.
#[cfg(feature = "serde")]
pub use serde;

/// The `serde_json` crate (`json!`, `Value`, `from_str`, …).
#[cfg(any(feature = "json", feature = "http", feature = "print"))]
pub use serde_json;

/// The `toml` crate (`Value`, `Table`, `from_str`, `to_string`, …).
#[cfg(feature = "toml")]
pub use toml;
