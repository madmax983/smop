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
fn new_command_scaffolds_backup_template() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");

    let output = Command::new(env!("CARGO_BIN_EXE_smop"))
        .args(["new", "backup"])
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to run smop new");

    assert!(output.status.success(), "smop new backup failed");
    assert!(
        temp_dir.path().join("script.toml").exists(),
        "script.toml should be created"
    );
    assert!(
        temp_dir.path().join("README.md").exists(),
        "README.md should be created"
    );

    let validate_output = Command::new(env!("CARGO_BIN_EXE_smop"))
        .args(["validate", "script.toml"])
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to validate scaffolded script");

    assert!(
        validate_output.status.success(),
        "scaffolded script should validate"
    );
}

#[test]
fn new_command_refuses_to_overwrite_existing_script() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    std::fs::write(temp_dir.path().join("script.toml"), "keep me").expect("failed to seed script");

    let output = Command::new(env!("CARGO_BIN_EXE_smop"))
        .args(["new", "backup"])
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to run smop new");

    assert!(
        !output.status.success(),
        "smop new should refuse to overwrite script.toml"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("script.toml") && stderr.contains("already exists"),
        "refusal should mention script.toml: {stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(temp_dir.path().join("script.toml"))
            .expect("failed to read original script"),
        "keep me"
    );
    assert!(
        !temp_dir.path().join("README.md").exists(),
        "README.md should not be created after refusal"
    );
}

#[test]
fn new_command_refuses_to_overwrite_existing_readme() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    std::fs::write(temp_dir.path().join("README.md"), "keep me").expect("failed to seed readme");

    let output = Command::new(env!("CARGO_BIN_EXE_smop"))
        .args(["new", "backup"])
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to run smop new");

    assert!(
        !output.status.success(),
        "smop new should refuse to overwrite README.md"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("README.md") && stderr.contains("already exists"),
        "refusal should mention README.md: {stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(temp_dir.path().join("README.md"))
            .expect("failed to read original readme"),
        "keep me"
    );
    assert!(
        !temp_dir.path().join("script.toml").exists(),
        "script.toml should not be created after refusal"
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

#[test]
fn run_command_executes_steps() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/scripts/fs-only.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_smop"))
        .args([
            "run",
            script.to_str().expect("script path should be valid UTF-8"),
        ])
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to run smop run");

    assert!(output.status.success(), "filesystem-only script should run");

    let build_dir = temp_dir.path().join("build");
    let manifest = build_dir.join("manifest.txt");
    let notes = build_dir.join("notes.txt");

    assert!(build_dir.exists(), "run should create the build directory");
    assert!(manifest.exists(), "run should create the manifest file");
    assert!(notes.exists(), "run should create the notes file");

    let manifest_content = std::fs::read_to_string(&manifest).expect("failed to read manifest");
    let notes_content = std::fs::read_to_string(&notes).expect("failed to read notes");
    assert!(
        manifest_content.contains("backup starting"),
        "manifest should contain the expected contents"
    );
    assert!(
        notes_content.contains("extra notes"),
        "notes should contain the appended contents"
    );
}

#[test]
fn run_command_fails_when_required_env_is_missing() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/scripts/valid-backup.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_smop"))
        .args([
            "run",
            script.to_str().expect("script path should be valid UTF-8"),
        ])
        .current_dir(temp_dir.path())
        .env_remove("BACKUP_ROOT")
        .output()
        .expect("failed to run smop run");

    assert!(
        !output.status.success(),
        "run should fail when required env vars are missing"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("check-env") && stderr.contains("Missing required environment variables"),
        "env failure should mention the missing variables: {stderr}"
    );
    assert!(
        !temp_dir.path().join("build/manifest.txt").exists(),
        "run should stop before creating later files"
    );
}

#[cfg(all(feature = "cli", not(feature = "http"), not(feature = "archive")))]
#[test]
fn cli_only_build_rejects_unsupported_step_kinds() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");

    let http_script = temp_dir.path().join("http.toml");
    std::fs::write(
        &http_script,
        r#"
name = "http-script"

[[step]]
name = "fetch"
type = "http.get"
url = "https://example.invalid"
dest = "build/body.txt"
"#,
    )
    .expect("failed to write http script");

    let archive_script = temp_dir.path().join("archive.toml");
    std::fs::write(
        &archive_script,
        r#"
name = "archive-script"

[[step]]
name = "pack"
type = "archive.create_tar_gz"
source = "src"
dest = "build/source.tar.gz"
"#,
    )
    .expect("failed to write archive script");

    for script in [&http_script, &archive_script] {
        let validate_output = Command::new(env!("CARGO_BIN_EXE_smop"))
            .args([
                "validate",
                script.to_str().expect("script path should be valid UTF-8"),
            ])
            .current_dir(temp_dir.path())
            .output()
            .expect("failed to run smop validate");

        assert!(
            !validate_output.status.success(),
            "validate should reject unsupported step kinds"
        );
        let stderr = String::from_utf8_lossy(&validate_output.stderr);
        assert!(
            stderr.contains("not supported by this build"),
            "validate should fail before blessing unsupported steps: {stderr}"
        );

        let run_output = Command::new(env!("CARGO_BIN_EXE_smop"))
            .args([
                "run",
                script.to_str().expect("script path should be valid UTF-8"),
            ])
            .current_dir(temp_dir.path())
            .output()
            .expect("failed to run smop run");

        assert!(
            !run_output.status.success(),
            "run should reject unsupported step kinds"
        );
        let stderr = String::from_utf8_lossy(&run_output.stderr);
        assert!(
            stderr.contains("not supported by this build"),
            "run should fail before any step executes: {stderr}"
        );
    }
}
