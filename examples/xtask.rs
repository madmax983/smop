//! A minimal `cargo xtask` built on smop's core modules.
//!
//! In a real workspace this would be `xtask/src/main.rs`, with
//!
//! ```toml
//! # xtask/Cargo.toml
//! [dependencies]
//! smop = { version = "0.3", default-features = false, features = ["sh", "fs", "path"] }
//! ```
//!
//! and a `.cargo/config.toml` alias: `xtask = "run --package xtask --"`.
//!
//! Try it here with:
//!
//! ```sh
//! cargo run --example xtask --no-default-features --features sh,fs,path -- root
//! cargo run --example xtask --no-default-features --features sh,fs,path -- dist
//! ```

use smop::anyhow::bail;
use smop::{Context, Result, fs, path, sh};

fn main() -> Result<()> {
    let task = std::env::args().nth(1).unwrap_or_default();
    let root = path::workspace_root()?;

    match task.as_str() {
        "root" => println!("{}", root.display()),
        "lint" => {
            sh::cargo()
                .args(["fmt", "--check"])
                .dir(&root)
                .echo()
                .run()?;
            sh::cargo()
                .args(["clippy", "--all-targets", "--", "-D", "warnings"])
                .dir(&root)
                .echo()
                .run()?;
        }
        "version" => {
            let manifest = fs::read_string(root.join("Cargo.toml"))
                .context("xtask needs to run inside a Cargo project")?;
            let version = manifest
                .lines()
                .find_map(|l| l.strip_prefix("version = "))
                .map(|v| v.trim_matches('"'))
                .context("no version in Cargo.toml")?;
            println!("{version}");
        }
        "dist" => {
            // A clear, early error on machines (e.g. Windows) without bash.
            let bash = path::which("bash").context("`dist` needs bash on PATH")?;
            let out = root.join("target/dist");
            fs::create_dir_all(&out)?;
            sh::cmd(bash)
                .args(["-c", "echo built > dist.txt"])
                .dir(&out)
                .echo()
                .run()?;
            println!("{}", fs::read_string(out.join("dist.txt"))?.trim());
        }
        other => bail!("unknown task {other:?}; expected one of: root, lint, version, dist"),
    }
    Ok(())
}
