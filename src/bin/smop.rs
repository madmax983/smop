use smop::prelude::*;
use std::fs;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "smop",
    version,
    about = "Batteries-included scripting utilities for Rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a built-in script template.
    New { template: String },
    /// Validate a declarative script without running it.
    Validate { script: std::path::PathBuf },
    /// Run a declarative script.
    Run { script: std::path::PathBuf },
    /// Generate durable Rust from a declarative script.
    Build {
        script: std::path::PathBuf,
        #[arg(long)]
        out: std::path::PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { template } => dispatch_new(&template),
        Commands::Validate { script } => dispatch_validate(&script),
        Commands::Run { script } => dispatch_run(&script),
        Commands::Build { script, out } => dispatch_build(&script, &out),
    }
}

fn dispatch_new(template_name: &str) -> Result<()> {
    let template = smop::script::templates::Template::parse(template_name)?;
    let script_path = Path::new("script.toml");
    let readme_path = Path::new("README.md");

    refuse_if_exists(script_path)?;
    refuse_if_exists(readme_path)?;

    fs::write(script_path, smop::script::templates::render_script(template))
        .with_context(|| format!("Failed to write {}", script_path.display()))?;
    fs::write(readme_path, smop::script::templates::render_readme(template))
        .with_context(|| format!("Failed to write {}", readme_path.display()))?;

    println!("created template '{template_name}'");
    Ok(())
}

fn refuse_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        bail!("Refusing to scaffold because {} already exists", path.display());
    }

    Ok(())
}

fn dispatch_validate(script_path: &std::path::Path) -> Result<()> {
    let script = smop::script::parse::parse_script(
        &std::fs::read_to_string(script_path)
            .with_context(|| format!("Failed to read script: {}", script_path.display()))?,
    )?;
    let validated = smop::script::validate::validate_script(&script)?;

    println!(
        "validated script '{}' with {} step(s)",
        validated.name,
        validated.steps.len()
    );
    Ok(())
}

fn dispatch_run(_script: &std::path::Path) -> Result<()> {
    bail!("not implemented")
}

fn dispatch_build(_script: &std::path::Path, _out: &std::path::Path) -> Result<()> {
    bail!("not implemented")
}
