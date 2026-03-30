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

#[test]
fn script_validation_rejects_duplicate_step_names() {
    let source = r#"
name = "dup-steps"

[[step]]
name = "shared"
type = "env.require"
vars = ["BACKUP_ROOT"]

[[step]]
name = "shared"
type = "fs.write_string"
path = "build/output.txt"
content = "hello"
"#;

    let script = smop::script::parse::parse_script(source).expect("script should parse");
    let result = smop::script::validate::validate_script(&script);

    assert!(
        result.is_err(),
        "duplicate step names should fail validation"
    );
}

#[test]
fn script_validation_rejects_unknown_step_types() {
    let source = r#"
name = "unknown-step-type"

[[step]]
name = "mystery"
type = "not.a.real.step"
path = "build/output.txt"
"#;

    let script = smop::script::parse::parse_script(source).expect("script should parse");
    let result = smop::script::validate::validate_script(&script);

    assert!(result.is_err(), "unknown step types should fail validation");
}

#[test]
fn script_validation_rejects_missing_required_fields() {
    let source = r#"
name = "missing-field"

[[step]]
name = "write-manifest"
type = "fs.write_string"
path = "build/manifest.txt"
"#;

    let script = smop::script::parse::parse_script(source).expect("script should parse");
    let result = smop::script::validate::validate_script(&script);

    assert!(
        result.is_err(),
        "missing required fields should fail validation"
    );
}

#[test]
fn script_validation_rejects_unknown_fields() {
    let source = r#"
name = "unknown-field"

[[step]]
name = "write-manifest"
type = "fs.write_string"
path = "build/manifest.txt"
content = "backup starting\n"
bogus = "nope"
"#;

    let script = smop::script::parse::parse_script(source).expect("script should parse");
    let result = smop::script::validate::validate_script(&script);

    assert!(result.is_err(), "unknown fields should fail validation");
}

#[test]
fn script_validation_lowers_valid_script_to_expected_step_kind() {
    let source = r#"
name = "valid-script"

[[step]]
name = "check-env"
type = "env.require"
vars = ["BACKUP_ROOT", "ARCHIVE_ROOT"]
"#;

    let script = smop::script::parse::parse_script(source).expect("script should parse");
    let validated =
        smop::script::validate::validate_script(&script).expect("script should validate");

    assert_eq!(validated.name, "valid-script");
    assert_eq!(validated.steps.len(), 1);

    match &validated.steps[0].kind {
        smop::script::validate::StepKind::EnvRequire { vars } => {
            assert_eq!(
                vars,
                &["BACKUP_ROOT".to_string(), "ARCHIVE_ROOT".to_string()]
            );
        }
        other => panic!("unexpected step kind: {other:?}"),
    }
}

#[test]
fn script_validation_reports_wrong_type_for_string_field() {
    let source = r#"
name = "wrong-string-type"

[[step]]
name = "write-manifest"
type = "fs.write_string"
path = 42
content = "backup starting\n"
"#;

    let script = smop::script::parse::parse_script(source).expect("script should parse");
    let result = smop::script::validate::validate_script(&script);

    let message = result.expect_err("wrong type should fail").to_string();
    assert!(
        message.contains("expected string") || message.contains("must be a string"),
        "wrong-type string field should be reported distinctly: {message}"
    );
    assert!(
        !message.contains("missing required field 'path'"),
        "wrong-type string field should not be reported as missing: {message}"
    );
}

#[test]
fn script_validation_reports_wrong_type_for_string_list_field() {
    let source = r#"
name = "wrong-list-type"

[[step]]
name = "check-env"
type = "env.require"
vars = "BACKUP_ROOT"
"#;

    let script = smop::script::parse::parse_script(source).expect("script should parse");
    let result = smop::script::validate::validate_script(&script);

    let message = result.expect_err("wrong type should fail").to_string();
    assert!(
        message.contains("expected list of strings")
            || message.contains("must be a list of strings"),
        "wrong-type list field should be reported distinctly: {message}"
    );
    assert!(
        !message.contains("missing required field 'vars'"),
        "wrong-type list field should not be reported as missing: {message}"
    );
}

#[test]
fn validate_command_accepts_valid_script() {
    let output = Command::new(env!("CARGO_BIN_EXE_smop"))
        .args(["validate", "tests/fixtures/scripts/valid-backup.toml"])
        .output()
        .expect("failed to run smop validate");

    assert!(output.status.success(), "valid script should validate");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("validated script"),
        "validate success output should mention success: {stdout}"
    );
}

#[test]
fn validate_command_reports_errors() {
    let output = Command::new(env!("CARGO_BIN_EXE_smop"))
        .args([
            "validate",
            "tests/fixtures/scripts/invalid-missing-field.toml",
        ])
        .output()
        .expect("failed to run smop validate");

    assert!(
        !output.status.success(),
        "invalid script should fail validation"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("write-manifest"),
        "validation error should mention the failing step name: {stderr}"
    );
}
