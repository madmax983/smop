//! Example: Interactive configuration wizard.
//!
//! Run with: `cargo run --example interactive_cli`

use smop::prelude::*;

#[derive(Serialize, Debug)]
struct Config {
    project_name: String,
    author: String,
    port: u16,
    enable_logging: bool,
}

fn main() -> Result<()> {
    println!("=== Project Configuration Wizard ===\n");

    // Gather configuration interactively
    let project_name = print::prompt("Project name")?;
    let author = print::prompt_default("Author", "Anonymous")?;

    let port_str = print::prompt_default("Port", "8080")?;
    let port: u16 = port_str
        .parse()
        .context("Port must be a valid number between 0-65535")?;

    let enable_logging = print::confirm_default("Enable logging?", true)?;

    let config = Config {
        project_name,
        author,
        port,
        enable_logging,
    };

    println!("\n=== Configuration Summary ===\n");
    println!("{:#?}", config);

    if print::confirm("\nSave configuration?")? {
        let config_path = path::expand("./config.json");
        fs::write_json(&config_path, &config)?;
        success!("Configuration saved to {}", config_path.display());
    } else {
        warn!("Configuration not saved");
    }

    Ok(())
}
