//! The smop prelude.
//!
//! This module re-exports the most commonly used items for scripting.
//! Import with `use smop::prelude::*` to get everything you need.
//!
//! # What's included
//!
//! - Error handling: `anyhow::{anyhow, bail, ensure, Context, Result}`
//! - Serialization: `serde::{Serialize, Deserialize}` (`serde` feature,
//!   implied by `json`, `toml`, `csv`, `http` and `print`). Without a direct
//!   `serde` dependency, add `#[serde(crate = "smop::serde")]` to derived types.
//! - CLI: clap's derives plus `clap` itself, which the derive output needs.
//! - Core modules: `env`, `fs`, `path`, `sh` (each behind its own feature)
//! - Optional modules: `http`, `print`, `cli`, `time`, `archive`
//! - Macros: `success!`, `warn!`, `error!` (`print` feature)
//!
//! Only modules whose feature is enabled are re-exported.
//!
//! # Examples
//!
//! ```rust,no_run
//! use smop::prelude::*;
//!
//! # #[cfg(all(feature = "env", feature = "fs", feature = "sh"))]
//! fn main() -> Result<()> {
//!     // Environment variables
//!     let port: u16 = env::var_or("PORT", 8080)?;
//!
//!     // File operations
//!     let config = fs::read_string("config.txt")?;
//!
//!     // Shell commands
//!     sh::run("echo Hello")?;
//!
//!     Ok(())
//! }
//! # #[cfg(not(all(feature = "env", feature = "fs", feature = "sh")))]
//! # fn main() {}
//! ```

// Error handling from anyhow
pub use anyhow::{Context, Result, anyhow, bail, ensure};

// Serialization
#[cfg(feature = "serde")]
pub use serde::{Deserialize, Serialize};

// Core modules
#[cfg(feature = "env")]
pub use crate::env;

#[cfg(feature = "fs")]
pub use crate::fs;

#[cfg(feature = "path")]
pub use crate::path;

#[cfg(feature = "sh")]
pub use crate::sh;

// Feature-gated modules
#[cfg(feature = "http")]
pub use crate::http;

#[cfg(feature = "print")]
pub use crate::print;

#[cfg(feature = "cli")]
pub use crate::cli::{Args, Parser, Subcommand, ValueEnum, arg, clap, command};

#[cfg(feature = "time")]
pub use crate::time;

#[cfg(feature = "archive")]
pub use crate::archive;

// Re-export macros at crate root level, included via prelude
#[cfg(feature = "print")]
pub use crate::{error, success, warn};
