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

    // smop's TempDir needs no tempfile dependency.
    let work = smop::fs::TempDir::with_prefix_in("work-", dir.path())?;
    ensure!(work.path().starts_with(dir.path()));
    let kept: std::path::PathBuf = work.keep();
    ensure!(kept.is_dir());

    let _client = smop::http::Client::new().timeout(std::time::Duration::from_secs(30));

    // http raw layer type-checks with only smop in scope.
    let _unsent: smop::http::Request =
        smop::http::request("GET", "http://localhost/").json(&record);

    // `smop::toml` needs no toml dependency.
    let table: smop::toml::Value = smop::toml::from_str("name = \"a\"\nscore = 1")?;
    ensure!(table["score"].as_integer() == Some(1));
    let from_toml: Record = smop::toml::from_str("name = \"a\"\nscore = 1")?;
    ensure!(from_toml == record);
    ensure!(smop::toml::to_string(&record)?.contains("score = 1"));

    // YAML needs no YAML crate: smop has its own parser.
    let yaml_path = dir.path().join("r.yaml");
    smop::fs::write_yaml(&yaml_path, &record)?;
    let from_yaml: Record = smop::fs::read_yaml(&yaml_path)?;
    ensure!(from_yaml == record);
    let doc: smop::yaml::Value = smop::yaml::from_str("on: [push]\nscore: 1")?;
    ensure!(doc["on"][0].as_str() == Some("push"));
    ensure!(doc["score"].as_i64() == Some(1));
    ensure!(smop::yaml::to_string(&record)? == "name: a\nscore: 1\n");
    let typed: Record = yaml::from_str("{name: a, score: 1}")?; // through the prelude
    ensure!(typed == record);
    let bad = smop::fs::write_string(&yaml_path, "name: a\nscore: x\n")
        .and_then(|()| smop::fs::read_yaml::<Record, _>(&yaml_path))
        .expect_err("score: x is not a u32");
    let msg = format!("{bad:#}");
    ensure!(msg.contains("r.yaml") && msg.contains("line 2, column 8"), "{msg}");

    // `smop::re` needs no regex dependency.
    let kv = smop::re::Regex::new(r"(?<k>\w+)=(?<v>\d+)")?;
    ensure!(
        &kv.captures("port=80")
            .map(|c| c["v"].to_string())
            .unwrap_or_default()
            == "80"
    );
    ensure!(smop::re::find_all(r"\d+", "a1 b22")? == ["1", "22"]);
    ensure!(re::is_match(r"^\d+$", "42")?); // through the prelude

    // `cmd!` needs no smop-macros dependency: its expansion names only smop.
    let (word, list) = ("a b", ["c", "d e"]);
    let command = cmd!("consumer-tool --flag={word} {list...} -j{jobs}", jobs = 3);
    ensure!(command.to_string() == r#"consumer-tool "--flag=a b" c "d e" -j3"#);
    ensure!(smop::cmd!("true").to_string() == "true"); // without the prelude

    // `smop::rusqlite` and the row deserializer need no rusqlite dependency.
    let db = sqlite::open(":memory:")?;
    sqlite::execute(&db, "CREATE TABLE r (name TEXT, score INTEGER)", ())?;
    sqlite::execute(&db, "INSERT INTO r VALUES (?1, ?2)", sqlite::params!["a", 1])?;
    let rows: Vec<Record> = sqlite::query(&db, "SELECT * FROM r", ())?;
    ensure!(rows == [record]);
    let raw = smop::rusqlite::Connection::open_in_memory()?;
    ensure!(sqlite::query_one::<i64, _>(&raw, "SELECT 1", ())? == 1);

    println!("smop-consumer: ok");
    Ok(())
}
