//! CLI utilities.
//!
//! Re-exports of clap derive macros for building command-line interfaces.
//! This module is only available with the `cli` feature.
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

pub use clap::{Args, Parser, Subcommand, ValueEnum, arg, command};
