//! CLI utilities.
//!
//! Re-exports of clap derive macros for building command-line interfaces.
//! This module is only available with the `cli` feature.
//!
//! clap's derive output refers to the `clap` crate by name, so the `clap`
//! re-export must be in scope wherever you derive. `use smop::prelude::*`
//! covers that; with explicit imports, add `clap` to them:
//!
//! ```no_run
//! use smop::cli::{Parser, clap};
//!
//! #[derive(Parser)]
//! struct Args {
//!     #[arg(long)]
//!     count: u32,
//! }
//! ```
//!
//! No direct `clap` dependency is needed.
//!
//! # Examples
//!
//! ```no_run
//! use smop::prelude::*;
//!
//! #[derive(Parser)]
//! #[command(name = "myapp")]
//! #[command(about = "A sample application")]
//! struct Args {
//!     #[arg(short, long)]
//!     verbose: bool,
//!
//!     #[arg(short, long, default_value = "default")]
//!     name: String,
//! }
//!
//! fn main() -> Result<()> {
//!     let args = Args::parse();
//!     if args.verbose {
//!         println!("Verbose mode enabled");
//!     }
//!     println!("Hello, {}!", args.name);
//!     Ok(())
//! }
//! ```

/// The `clap` crate itself. clap's derives expand to `clap::...` paths, so
/// keep this in scope (the prelude does) to derive without depending on clap.
pub use clap;
pub use clap::{Args, Parser, Subcommand, ValueEnum, arg, command};
