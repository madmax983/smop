//! Compiles and runs only if smop's re-exports are self-contained.

use smop::prelude::*;

/// clap derives through the prelude: generated code refers to `clap::...`,
/// which resolves through `smop::prelude::*`.
#[derive(Parser, Debug)]
#[command(name = "consumer")]
struct Cli {
    #[arg(short, long)]
    verbose: bool,

    #[arg(long, value_enum, default_value_t = Mode::Fast)]
    mode: Mode,

    #[command(subcommand)]
    command: Command,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Fast,
    Slow,
}

#[derive(Subcommand, Debug)]
enum Command {
    Greet(GreetArgs),
}

#[derive(Args, Debug)]
struct GreetArgs {
    name: String,
}

/// Explicit imports instead of the prelude: `clap` must be imported too.
mod explicit {
    use smop::cli::{Parser, clap};

    #[derive(Parser)]
    pub struct Mini {
        #[arg(long)]
        pub count: u32,
    }
}

/// serde derives need `#[serde(crate = "smop::serde")]` without a direct
/// serde dependency.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(crate = "smop::serde")]
struct Record {
    name: String,
    score: u32,
}

fn main() -> Result<()> {
    let cli = Cli::parse_from(["consumer", "-v", "--mode", "slow", "greet", "world"]);
    ensure!(cli.verbose);
    ensure!(cli.mode == Mode::Slow);
    let Command::Greet(args) = cli.command;
    ensure!(args.name == "world");

    let mini = explicit::Mini::parse_from(["mini", "--count", "3"]);
    ensure!(mini.count == 3);

    let value = smop::serde_json::json!({ "name": "a", "score": 1 });
    let record: Record = smop::serde_json::from_value(value)?;
    ensure!(
        record
            == Record {
                name: "a".into(),
                score: 1
            }
    );

    let dir = smop::fs::temp_dir()?;
    let path = dir.path().join("r.json");
    smop::fs::write_json(&path, &record)?;
    let back: Record = smop::fs::read_json(&path)?;
    ensure!(back == record);

    // http raw layer type-checks with only smop in scope.
    let _unsent: smop::http::Request =
        smop::http::request("GET", "http://localhost/").json(&record);

    println!("smop-consumer: ok");
    Ok(())
}
