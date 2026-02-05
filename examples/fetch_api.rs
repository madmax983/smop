//! Example: Fetch JSON from an API with a spinner.
//!
//! Run with: `cargo run --example fetch_api -- --url https://httpbin.org/json`

use smop::prelude::*;

#[derive(Parser)]
#[command(name = "fetch_api")]
#[command(about = "Fetch JSON from a URL and display key count")]
struct Args {
    /// URL to fetch JSON from
    #[arg(short, long)]
    url: String,

    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.verbose {
        println!("Fetching from: {}", args.url);
    }

    let spinner = print::spinner("Fetching data...");
    let data: serde_json::Value = http::get_json(&args.url)?;
    spinner.finish();

    let key_count = data.as_object().map_or(0, |o| o.len());
    success!("Got response with {} top-level keys", key_count);

    if args.verbose {
        println!("{}", serde_json::to_string_pretty(&data)?);
    }

    Ok(())
}
