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
//! fn main() -> Result<()> {
//!     let home = path::home();
//!     let content = fs::read_string(home.join(".bashrc"))?;
//!     println!("Bashrc has {} lines", content.lines().count());
//!     Ok(())
//! }
//! ```

// Note: Using deny instead of forbid to allow unsafe in tests (required for
// std::env::set_var/remove_var in Rust 2024 edition)
#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

pub mod env;
pub mod fs;
pub mod path;
pub mod sh;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "print")]
pub mod print;

#[cfg(feature = "cli")]
pub mod cli;

pub mod prelude;

// Re-export core error handling
pub use anyhow::{self, Context, Result};
