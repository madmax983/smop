use std::process::Command;

#[test]
fn cli_help_lists_subcommands() {
    let output = Command::new(env!("CARGO_BIN_EXE_smop"))
        .arg("--help")
        .output()
        .expect("failed to run smop binary");

    assert!(output.status.success(), "smop --help failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("new")
            && stdout.contains("validate")
            && stdout.contains("run")
            && stdout.contains("build"),
        "help output missing subcommands: {stdout}"
    );
}
