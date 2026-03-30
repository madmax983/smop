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

#[test]
fn script_parser_reads_minimal_script() {
    let source = r#"
name = "backup-project"

[[step]]
name = "check-env"
type = "env.require"
vars = ["BACKUP_ROOT"]
"#;

    let script = smop::script::parse::parse_script(source).expect("script should parse");

    assert_eq!(script.name, "backup-project");
    assert_eq!(script.steps.len(), 1);

    let step = &script.steps[0];
    assert_eq!(step.name, "check-env");
    assert_eq!(step.step_type, "env.require");
    assert_eq!(
        step.fields
            .get("vars")
            .and_then(toml::Value::as_array)
            .and_then(|vars| vars.first())
            .and_then(toml::Value::as_str),
        Some("BACKUP_ROOT")
    );
}
