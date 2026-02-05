//! The smop prelude.
//!
//! This module re-exports the most commonly used items for scripting.
//! Import with `use smop::prelude::*` to get everything you need.
//!
//! # What's included
//!
//! - Error handling: `anyhow::{anyhow, bail, ensure, Context, Result}`
//! - Serialization: `serde::{Serialize, Deserialize}`
//! - Core modules: `env`, `fs`, `path`, `sh`
//! - Feature-gated modules: `http`, `print`, `cli`
//! - Macros: `success!`, `warn!`, `error!`
//!
//! # Examples
//!
//! ```rust,no_run
//! use smop::prelude::*;
//!
//! fn main() -> Result<()> {
//!     // Environment variables
//!     let port: u16 = env::var_or("PORT", 8080);
//!
//!     // File operations
//!     let config = fs::read_string("config.txt")?;
//!
//!     // Shell commands
//!     sh::run("echo Hello")?;
//!
//!     Ok(())
//! }
//! ```

// Error handling from anyhow
pub use anyhow::{Context, Result, anyhow, bail, ensure};

// Serialization
pub use serde::{Deserialize, Serialize};

// Core modules
pub use crate::env;
pub use crate::fs;
pub use crate::path;
pub use crate::sh;

// Feature-gated modules
#[cfg(feature = "http")]
pub use crate::http;

#[cfg(feature = "print")]
pub use crate::print;

#[cfg(feature = "cli")]
pub use crate::cli::{Args, Parser, Subcommand, ValueEnum, arg, command};

// Re-export macros at crate root level, included via prelude
#[cfg(feature = "print")]
pub use crate::{error, success, warn};
