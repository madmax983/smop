//! `clean-room`: the documented first run, followed to the letter.
//!
//! 1. `cargo new` (the only step the README assumes you know)
//! 2. the README's "Installation" block, pasted into Cargo.toml
//! 3. the README's first ```` ```rust ```` block, pasted into `src/main.rs`
//! 4. `cargo run`
//!
//! Prints each step with its exit code and seconds, then a summary. smop
//! comes from crates.io, as a newcomer gets it. `--smop-path` uses a
//! checkout instead, for a run before a release.
//!
//! In a pristine container (only a Rust toolchain):
//!
//! ```text
//! docker build -f xtask/clean-room.Dockerfile -t smop-cleanroom .
//! docker run --rm smop-cleanroom
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use smop::prelude::*;

/// The first ```` ```lang ```` block of `text`, with `\n` line ends. A
/// Windows checkout has `\r\n`.
fn first_block(text: &str, lang: &str) -> Result<String> {
    let text = text.replace("\r\n", "\n");
    let open = format!("```{lang}\n");
    let start = text
        .find(&open)
        .map(|i| i + open.len())
        .with_context(|| format!("README has no ```{lang} block"))?;
    let len = text[start..]
        .find("```")
        .with_context(|| format!("README's first ```{lang} block is not closed"))?;
    Ok(text[start..start + len].to_string())
}

/// The lines of the README's first TOML block after `[dependencies]`.
pub fn install_lines(readme: &str) -> Result<String> {
    let block = first_block(readme, "toml")?;
    let (_, deps) = block
        .split_once("[dependencies]\n")
        .context("README's first ```toml block has no [dependencies]")?;
    Ok(deps.trim_end().to_string())
}

/// The README's first Rust block, as is.
pub fn first_rust_block(readme: &str) -> Result<String> {
    first_block(readme, "rust")
}

/// `manifest` with its `smop = "<version>"` line made a path dependency.
pub fn use_checkout(manifest: &str, checkout: &Path) -> Result<String> {
    let line = format!("smop = {{ path = {:?} }}", checkout.display().to_string());
    Ok(re::compile(r#"(?m)^smop = "[^"]*""#)?
        .replace(manifest, re::NoExpand(&line))
        .into_owned())
}

/// One step of the run.
struct Step {
    label: &'static str,
    code: u8,
    seconds: u64,
    first_error: Option<String>,
}

fn timed_step(label: &'static str, f: impl FnOnce() -> Result<sh::Captured>) -> Result<Step> {
    let start = Instant::now();
    let result = f()?;
    let first_error = (!result.success()).then(|| {
        result
            .stdout()
            .lines()
            .find(|l| l.starts_with("error"))
            .unwrap_or("")
            .to_string()
    });
    Ok(Step {
        label,
        code: result.code(),
        seconds: start.elapsed().as_secs(),
        first_error,
    })
}

/// Runs the clean-room journey in a new temporary directory.
pub fn run(readme: &Path, checkout: Option<&Path>) -> Result<()> {
    let readme = fs::read_string(readme)?;
    let start = Instant::now();
    let work = fs::temp_dir()?;
    let hello: PathBuf = work.path().join("hello");
    let mut steps = Vec::new();

    steps.push(timed_step("cargo new hello", || {
        sh::cargo()
            .args(["new", "--quiet", "hello"])
            .dir(work.path())
            .stderr_to_stdout()
            .capture()
    })?);
    if steps[0].code == 0 {
        let manifest_path = hello.join("Cargo.toml");
        let mut manifest = fs::read_string(&manifest_path)?;
        manifest.push_str(&install_lines(&readme)?);
        manifest.push('\n');
        if let Some(checkout) = checkout {
            manifest = use_checkout(&manifest, checkout)?;
        }
        fs::write_string(&manifest_path, &manifest)?;
        fs::write_string(hello.join("src/main.rs"), first_rust_block(&readme)?)?;
        steps.push(timed_step("cargo run", || {
            sh::cargo()
                .args(["run", "--quiet"])
                .dir(&hello)
                .stderr_to_stdout()
                .tee()
                .capture()
        })?);
    }

    println!("== clean-room log ==");
    for (n, step) in steps.iter().enumerate() {
        println!(
            "{:2}  rc={:<3} {:4}s  {}",
            n + 1,
            step.code,
            step.seconds,
            step.label
        );
        if let Some(error) = &step.first_error {
            println!("      first error: {error}");
        }
    }
    let failed = steps.iter().filter(|s| s.code != 0).count();
    let success = failed == 0 && steps.len() == 2;
    println!(
        "== summary: steps={} failed_steps={failed} wall_seconds={} first_success={} ==",
        steps.len(),
        start.elapsed().as_secs(),
        if success { "yes" } else { "NO" }
    );
    ensure!(success, "clean room: the first run failed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const README: &str = "# smop\n\n```rust\nfn main() {}\n```\n\n## Installation\n\n```toml\n[dependencies]\nsmop = \"0.4\"\n```\n\n```toml\n[dependencies]\nother = 1\n```\n";

    #[test]
    fn install_lines_come_from_the_first_toml_block() {
        assert_eq!(install_lines(README).unwrap(), "smop = \"0.4\"");
    }

    #[test]
    fn the_first_rust_block_is_taken_as_is() {
        assert_eq!(first_rust_block(README).unwrap(), "fn main() {}\n");
    }

    #[test]
    fn a_readme_without_blocks_is_an_error() {
        assert!(install_lines("# x\n").is_err());
        assert!(first_rust_block("# x\n").is_err());
        assert!(install_lines("```toml\na = 1\n```\n").is_err());
        assert!(first_rust_block("```rust\nfn main() {}\n").is_err());
    }

    #[test]
    fn use_checkout_rewrites_only_the_smop_line() {
        let manifest = "[dependencies]\nsmop = \"0.4\"\nsmop-macros = \"0.4\"\n";
        let out = use_checkout(manifest, Path::new("/src/$smop")).unwrap();
        assert_eq!(
            out,
            "[dependencies]\nsmop = { path = \"/src/$smop\" }\nsmop-macros = \"0.4\"\n"
        );
    }

    #[test]
    fn crlf_line_ends_are_read_as_lf() {
        let crlf = README.replace('\n', "\r\n");
        assert_eq!(install_lines(&crlf).unwrap(), "smop = \"0.4\"");
        assert_eq!(first_rust_block(&crlf).unwrap(), "fn main() {}\n");
    }

    #[test]
    fn the_repository_readme_has_both_blocks() {
        let readme =
            fs::read_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../README.md")).unwrap();
        assert!(install_lines(&readme).unwrap().starts_with("smop = "));
        assert!(first_rust_block(&readme).unwrap().contains("fn main"));
    }

    fn readme_with(body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let readme = dir.path().join("README.md");
        let text = format!("# t\n\n```toml\n[dependencies]\n```\n\n```rust\n{body}\n```\n");
        fs::write_string(&readme, text).unwrap();
        (dir, readme)
    }

    #[test]
    fn a_readme_that_runs_is_a_first_success() {
        let (_dir, readme) = readme_with("fn main() { println!(\"hi\"); }");
        run(&readme, None).unwrap();
    }

    #[test]
    fn a_readme_that_does_not_compile_fails() {
        let (_dir, readme) = readme_with("fn main() { let x: u8 = \"no\"; }");
        let err = run(&readme, Some(Path::new("/unused"))).unwrap_err();
        assert_eq!(err.to_string(), "clean room: the first run failed");
    }
}
