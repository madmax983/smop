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

#[cfg(test)]
mod tests {
    // Everything comes through the prelude, as a script would use it.
    use crate::prelude::*;

    use crate::cli::clap::error::ErrorKind;

    #[derive(Parser, Debug)]
    #[command(name = "tool", version = "1.2.3")]
    struct Cli {
        #[arg(short, long)]
        verbose: bool,
        #[arg(long, value_enum, default_value_t = Mode::Fast)]
        mode: Mode,
        #[command(flatten)]
        output: Output,
        #[command(subcommand)]
        task: Task,
    }

    #[derive(Args, Debug)]
    struct Output {
        #[arg(long, default_value = "target")]
        out: String,
    }

    #[derive(Subcommand, Debug, PartialEq, Eq)]
    enum Task {
        Build {
            #[arg(long)]
            release: bool,
            packages: Vec<String>,
        },
        Clean,
    }

    #[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
    enum Mode {
        Fast,
        Slow,
    }

    fn kind_of(argv: &[&str]) -> Option<ErrorKind> {
        Cli::try_parse_from(argv).err().map(|e| e.kind())
    }

    #[test]
    fn parses_flags_value_enum_flatten_and_subcommand() -> Result<()> {
        let cli = Cli::try_parse_from([
            "tool",
            "-v",
            "--mode",
            "slow",
            "--out",
            "dist",
            "build",
            "--release",
            "core",
            "cli",
        ])?;
        assert!(cli.verbose);
        assert_eq!(cli.mode, Mode::Slow);
        assert_eq!(cli.output.out, "dist");
        assert_eq!(
            cli.task,
            Task::Build {
                release: true,
                packages: vec!["core".into(), "cli".into()],
            }
        );
        Ok(())
    }

    #[test]
    fn defaults_apply_when_flags_are_omitted() -> Result<()> {
        let cli = Cli::try_parse_from(["tool", "clean"])?;
        assert!(!cli.verbose);
        assert_eq!(cli.mode, Mode::Fast);
        assert_eq!(cli.output.out, "target");
        assert_eq!(cli.task, Task::Clean);
        Ok(())
    }

    #[test]
    fn bad_input_is_an_error_not_a_default() {
        assert_eq!(
            kind_of(&["tool", "--nope", "clean"]),
            Some(ErrorKind::UnknownArgument)
        );
        assert_eq!(
            kind_of(&["tool", "--mode", "medium", "clean"]),
            Some(ErrorKind::InvalidValue)
        );
        assert!(
            kind_of(&["tool"]).is_some(),
            "a missing subcommand must be rejected"
        );
        assert_eq!(
            kind_of(&["tool", "--version"]),
            Some(ErrorKind::DisplayVersion)
        );
    }

    #[test]
    fn clap_reexport_resolves() {
        // `smop::cli::clap` is the path clap's derive output relies on.
        let command = crate::cli::clap::Command::new("probe");
        assert_eq!(command.get_name(), "probe");
        assert_eq!(
            <Cli as crate::cli::clap::CommandFactory>::command().get_name(),
            "tool"
        );
    }
}
