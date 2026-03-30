use smop::prelude::*;
use std::io::Write;
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

    write_new_text_file(
        script_path,
        &smop::script::templates::render_script(template),
    )?;
    if let Err(error) = write_new_text_file(
        readme_path,
        &smop::script::templates::render_readme(template),
    ) {
        let _ = std::fs::remove_file(script_path);
        return Err(error);
    }

    println!("created template '{template_name}'");
    Ok(())
}

fn write_new_text_file(path: &Path, contents: &str) -> Result<()> {
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            bail!(
                "Refusing to scaffold because {} already exists",
                path.display()
            );
        }
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to create {}", path.display()));
        }
    };

    if let Err(error) = file.write_all(contents.as_bytes()) {
        let _ = std::fs::remove_file(path);
        return Err(error).with_context(|| format!("Failed to write {}", path.display()));
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

fn dispatch_run(script_path: &std::path::Path) -> Result<()> {
    let script = smop::script::parse::parse_script(
        &std::fs::read_to_string(script_path)
            .with_context(|| format!("Failed to read script: {}", script_path.display()))?,
    )?;
    let validated = smop::script::validate::validate_script(&script)?;
    smop::script::execute::execute_script(&validated)
}

fn dispatch_build(script_path: &std::path::Path, out_path: &std::path::Path) -> Result<()> {
    let script = smop::script::parse::parse_script(
        &std::fs::read_to_string(script_path)
            .with_context(|| format!("Failed to read script: {}", script_path.display()))?,
    )?;
    let validated = smop::script::validate::validate_script(&script)?;
    let generated = smop::script::codegen::render_script(&validated);

    if let Some(parent) = out_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    std::fs::write(out_path, generated)
        .with_context(|| format!("Failed to write {}", out_path.display()))?;
    Ok(())
}
