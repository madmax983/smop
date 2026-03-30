use std::process::Command;

#[test]
fn cli_help_lists_subcommands() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--bin", "smop", "--", "--help"])
        .output()
        .expect("failed to run cargo");

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
