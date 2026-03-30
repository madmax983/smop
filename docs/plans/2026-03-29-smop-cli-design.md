# Smop CLI Design Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a first-party `smop` binary that can scaffold declarative scripts, validate them, run them, and generate durable Rust source from them.

**Architecture:** Keep `smop` library-first and add a thin CLI/runtime layer on top. Parse `script.toml` into a validated internal IR once, then lower that IR either to direct execution (`smop run`) or readable Rust source (`smop build --out script.rs`) so runtime and codegen share one semantic model.

**Tech Stack:** Rust 2024, `clap` derive macros behind the existing `cli` feature, `serde` + `toml` for script parsing, `anyhow` for errors, existing `smop` modules (`env`, `fs`, `http`, `archive`, `sh`, `time`) for step execution, `assert_cmd`/`predicates`/`tempfile` for CLI integration tests.

---

## Product Scope

### V1 commands

- `smop new <template>`
- `smop validate <script.toml>`
- `smop run <script.toml>`
- `smop build <script.toml> --out <generated.rs>`

### V1 execution model

- Declarative `script.toml`
- Sequential steps only
- Filesystem-first data flow
- Explicit `sh.run` escape hatch
- Stop on first error
- Exit non-zero on any failure

### V1 non-goals

- Variables or interpolation
- Conditionals or loops
- Parallel step execution
- Remote/community template registries
- Bidirectional sync between generated Rust and `script.toml`
- Hidden runtime behavior that cannot be lowered into generated Rust

## CLI Contract

### Command behavior

- `smop new <template>`
  - Creates a runnable directory in the current working directory.
  - Writes `script.toml`, a short `README.md`, and any companion files the template needs.
- `smop validate <script.toml>`
  - Parses and validates without side effects.
  - Prints success to stdout and actionable errors to stderr.
- `smop run <script.toml>`
  - Parses, validates, then executes each step in order.
  - Prints step progress to stderr and user-relevant data to stdout.
- `smop build <script.toml> --out <generated.rs>`
  - Emits durable, editable Rust using `smop::prelude::*`.
  - Fails if any step cannot be lowered cleanly.

### UX rules

- Errors go to stderr, never stdout.
- Validation errors include the step name and step type when possible.
- Generated Rust is readable handoff code, not machine-owned sludge.
- `smop run` and `smop build` must reject scripts with unsupported step types rather than diverging semantically.

## Script Format

### Top-level structure

```toml
name = "backup-project"
description = "Create a timestamped archive and copy it elsewhere"

[[step]]
name = "check-env"
type = "env.require"
vars = ["BACKUP_ROOT"]

[[step]]
name = "prepare-output-dir"
type = "fs.mkdir_all"
path = "build"

[[step]]
name = "write-manifest"
type = "fs.write_string"
path = "build/manifest.txt"
content = "backup starting\n"

[[step]]
name = "archive-source"
type = "archive.create_tar_gz"
source = "src"
dest = "build/source.tar.gz"

[[step]]
name = "upload-copy"
type = "sh.run"
command = "aws s3 cp build/source.tar.gz s3://my-bucket/source.tar.gz"
```

### V1 supported step types

- `env.require`
  - `vars = ["NAME", "OTHER"]`
- `fs.mkdir_all`
  - `path = "build/output"`
- `fs.write_string`
  - `path`, `content`
- `fs.append`
  - `path`, `content`
- `fs.copy`
  - `from`, `to`
- `fs.rename`
  - `from`, `to`
- `fs.remove`
  - `path`
- `http.download`
  - `url`, `path`
- `http.get`
  - `url`, `dest`
  - Writes response body to `dest`
- `archive.create_zip`
  - `source`, `dest`
- `archive.create_tar`
  - `source`, `dest`
- `archive.create_tar_gz`
  - `source`, `dest`
- `archive.extract_zip`
  - `archive`, `dest`
- `archive.extract_tar`
  - `archive`, `dest`
- `archive.extract_tar_gz`
  - `archive`, `dest`
- `sh.run`
  - `command`
  - Optional v1.1 follow-up: structured `program` + `args`

### Validation rules

- `name` is required at the script level.
- Each step must have a unique `name`.
- Each step must declare a known `type`.
- Required fields are enforced per step type.
- Unknown fields should fail validation so typos do not silently noop.
- `build` and `run` share the same validator.

## Internal Architecture

### File layout

- Create: `src/bin/smop.rs`
- Create: `src/script/mod.rs`
- Create: `src/script/model.rs`
- Create: `src/script/parse.rs`
- Create: `src/script/validate.rs`
- Create: `src/script/execute.rs`
- Create: `src/script/codegen.rs`
- Create: `src/script/templates.rs`
- Create: `tests/smop_cli.rs`
- Create: `tests/fixtures/scripts/`
- Modify: `src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `README.md`
- Optional but recommended: `docs/adr/0006-first-party-cli-and-script-ir.md`

### Module responsibilities

- `src/bin/smop.rs`
  - Clap entrypoint
  - Command dispatch
  - Exit status handling
- `src/script/model.rs`
  - `Script`, `Step`, `StepKind`
  - Strongly typed shape of declarative scripts
- `src/script/parse.rs`
  - TOML parsing into raw model structs
- `src/script/validate.rs`
  - Uniqueness checks, required field checks, supported-step checks
- `src/script/execute.rs`
  - Step executor that maps validated `StepKind` to `smop` calls
- `src/script/codegen.rs`
  - Lowers validated script IR into readable Rust source
- `src/script/templates.rs`
  - Built-in templates: `backup`, `http-fetch`, `file-pipeline`

### Cargo wiring

- Add a `[[bin]]` entry for `smop`.
- Set `required-features = ["cli"]` on the binary to preserve `--no-default-features` library builds.
- Add dev dependencies for CLI integration testing.

## Codegen Contract

`smop build` is not a debug dump. It must emit code a human can keep.

### Generated Rust requirements

- `use smop::prelude::*;`
- `fn main() -> Result<()>`
- One commented block per declarative step
- Straightforward calls into the library or `std::fs::create_dir_all`
- No generated macros beyond what the library already exposes
- No hidden runtime dependency on the original TOML file

### Example generated shape

```rust
use smop::prelude::*;

fn main() -> Result<()> {
    // Step: check-env
    env::require_vars(&["BACKUP_ROOT"])?;

    // Step: prepare-output-dir
    std::fs::create_dir_all("build")
        .context("Failed to create build directory")?;

    // Step: write-manifest
    fs::write_string("build/manifest.txt", "backup starting\n")?;

    // Step: archive-source
    archive::create_tar_gz("src", "build/source.tar.gz")?;

    // Step: upload-copy
    sh::run("aws s3 cp build/source.tar.gz s3://my-bucket/source.tar.gz")?;

    Ok(())
}
```

### Codegen invariants

- Every runnable step must be buildable.
- Field normalization belongs in the validator, not ad hoc in the renderer.
- The codegen output should be stable enough for snapshot tests.

## Template Strategy

### `backup`

- Demonstrates `env.require`, `fs.mkdir_all`, `fs.write_string`, `archive.create_tar_gz`, and optional `sh.run`.

### `http-fetch`

- Demonstrates `fs.mkdir_all`, `http.get` or `http.download`, and writing output into a local file.

### `file-pipeline`

- Demonstrates local filesystem operations without network dependencies.

### Template output

Each template should include:

- `script.toml`
- `README.md`
- Companion directories/files if needed for immediate execution

## Testing Strategy

### Unit tests

- Parse valid and invalid scripts
- Reject duplicate step names
- Reject unknown step types
- Reject missing required fields
- Verify codegen output for representative scripts
- Verify template rendering chooses the correct file set

### Integration tests

- `smop validate tests/fixtures/scripts/valid-backup.toml` succeeds
- `smop validate tests/fixtures/scripts/invalid-missing-field.toml` fails
- `smop run tests/fixtures/scripts/fs-only.toml` creates expected files
- `smop build tests/fixtures/scripts/fs-only.toml --out tmp/generated.rs` succeeds
- Generated Rust compiles with `cargo check` in a temp directory

### Verification commands

- `cargo fmt --all`
- `cargo test`
- `cargo test --test smop_cli`
- `cargo run --bin smop -- validate tests/fixtures/scripts/valid-backup.toml`
- `cargo run --bin smop -- build tests/fixtures/scripts/fs-only.toml --out target/generated/fs_only.rs`

## Implementation Tasks

### Task 1: Add binary entrypoint and command grammar

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Create: `src/bin/smop.rs`
- Test: `tests/smop_cli.rs`

**Step 1: Write the failing CLI smoke test**

- Add an integration test that runs `smop --help`.
- Assert that the command list includes `new`, `validate`, `run`, and `build`.

**Step 2: Run test to verify it fails**

Run: `cargo test --test smop_cli cli_help_lists_subcommands -- --exact`
Expected: FAIL because the binary does not exist yet.

**Step 3: Write minimal implementation**

- Add `[[bin]]` with `required-features = ["cli"]`.
- Create `src/bin/smop.rs` with clap structs and placeholder dispatch functions returning `bail!("not implemented")`.
- Export `pub mod script;` from `src/lib.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test --test smop_cli cli_help_lists_subcommands -- --exact`
Expected: PASS

### Task 2: Parse raw TOML into a typed script model

**Files:**
- Create: `src/script/mod.rs`
- Create: `src/script/model.rs`
- Create: `src/script/parse.rs`
- Test: `tests/smop_cli.rs`

**Step 1: Write the failing parser test**

- Add a test that feeds a minimal valid script to a parser function.
- Assert that one `env.require` step is parsed with the expected fields.

**Step 2: Run test to verify it fails**

Run: `cargo test script_parser_reads_minimal_script`
Expected: FAIL because the parser module is absent.

**Step 3: Write minimal implementation**

- Define raw `Script` and `Step` structs with serde derives.
- Parse TOML using `toml::from_str`.
- Preserve step `name` and raw `type`.

**Step 4: Run test to verify it passes**

Run: `cargo test script_parser_reads_minimal_script`
Expected: PASS

### Task 3: Validate script semantics before any execution

**Files:**
- Create: `src/script/validate.rs`
- Modify: `src/script/model.rs`
- Test: `tests/smop_cli.rs`

**Step 1: Write the failing validation tests**

- Duplicate step names should fail.
- Unknown step types should fail.
- Missing required fields should fail.

**Step 2: Run tests to verify they fail**

Run: `cargo test script_validation`
Expected: FAIL

**Step 3: Write minimal implementation**

- Introduce a validated `StepKind` enum.
- Add per-step field checks.
- Reject unknown fields if feasible via serde `deny_unknown_fields`; otherwise perform explicit checks for v1.

**Step 4: Run tests to verify they pass**

Run: `cargo test script_validation`
Expected: PASS

### Task 4: Implement `smop validate`

**Files:**
- Modify: `src/bin/smop.rs`
- Modify: `src/script/mod.rs`
- Create: `tests/fixtures/scripts/valid-backup.toml`
- Create: `tests/fixtures/scripts/invalid-missing-field.toml`
- Test: `tests/smop_cli.rs`

**Step 1: Write the failing CLI validation tests**

- Valid script returns exit code `0`.
- Invalid script returns non-zero and prints the failing step name to stderr.

**Step 2: Run tests to verify they fail**

Run: `cargo test --test smop_cli validate_command_reports_errors`
Expected: FAIL

**Step 3: Write minimal implementation**

- Hook the CLI `validate` subcommand into parse + validate.
- Print success to stdout and errors to stderr.

**Step 4: Run tests to verify they pass**

Run: `cargo test --test smop_cli validate_command_reports_errors`
Expected: PASS

### Task 5: Implement `smop new <template>`

**Files:**
- Create: `src/script/templates.rs`
- Modify: `src/bin/smop.rs`
- Test: `tests/smop_cli.rs`

**Step 1: Write the failing template scaffold tests**

- `smop new backup` creates `script.toml` and `README.md`.
- The generated `script.toml` validates cleanly.

**Step 2: Run tests to verify they fail**

Run: `cargo test --test smop_cli new_command_scaffolds_backup_template`
Expected: FAIL

**Step 3: Write minimal implementation**

- Add a small enum for built-in templates.
- Render fixed embedded files for `backup`, `http-fetch`, and `file-pipeline`.

**Step 4: Run tests to verify they pass**

Run: `cargo test --test smop_cli new_command_scaffolds_backup_template`
Expected: PASS

### Task 6: Implement the v1 executor for `smop run`

**Files:**
- Create: `src/script/execute.rs`
- Modify: `src/bin/smop.rs`
- Test: `tests/smop_cli.rs`
- Create: `tests/fixtures/scripts/fs-only.toml`

**Step 1: Write the failing executor tests**

- A filesystem-only fixture creates expected files.
- An `env.require` fixture fails when vars are missing.
- Execution stops on the first error.

**Step 2: Run tests to verify they fail**

Run: `cargo test --test smop_cli run_command_executes_steps`
Expected: FAIL

**Step 3: Write minimal implementation**

- Map each validated `StepKind` to existing `smop` or `std` operations.
- Keep stderr progress lines simple: `Running step 2/4: archive-source`.

**Step 4: Run tests to verify they pass**

Run: `cargo test --test smop_cli run_command_executes_steps`
Expected: PASS

### Task 7: Implement Rust code generation for `smop build`

**Files:**
- Create: `src/script/codegen.rs`
- Modify: `src/bin/smop.rs`
- Test: `tests/smop_cli.rs`

**Step 1: Write the failing codegen tests**

- `smop build` writes a `.rs` file.
- Output contains the expected calls for a representative fixture.
- Generated Rust compiles in a temp directory.

**Step 2: Run tests to verify they fail**

Run: `cargo test --test smop_cli build_command_emits_compilable_rust`
Expected: FAIL

**Step 3: Write minimal implementation**

- Render stable Rust source from validated steps.
- Use direct `smop` library calls and `std::fs::create_dir_all` where needed.
- Keep formatting simple enough for `cargo fmt`.

**Step 4: Run tests to verify they pass**

Run: `cargo test --test smop_cli build_command_emits_compilable_rust`
Expected: PASS

### Task 8: Document and harden the public surface

**Files:**
- Modify: `README.md`
- Optional: Create `docs/adr/0006-first-party-cli-and-script-ir.md`
- Test: `tests/smop_cli.rs`

**Step 1: Write the failing documentation check**

- Add a test or review checklist ensuring `README.md` includes the new CLI commands and one script example.

**Step 2: Run verification to expose gaps**

Run: `cargo test --test smop_cli`
Expected: FAIL if docs/examples are still stale.

**Step 3: Write minimal implementation**

- Document `new`, `validate`, `run`, and `build`.
- Include one complete `script.toml` example and one generated Rust excerpt.
- Add ADR if you want the library-first plus IR design recorded formally.

**Step 4: Run full verification**

Run:

```bash
cargo fmt --all
cargo test
cargo run --bin smop -- validate tests/fixtures/scripts/valid-backup.toml
cargo run --bin smop -- run tests/fixtures/scripts/fs-only.toml
cargo run --bin smop -- build tests/fixtures/scripts/fs-only.toml --out target/generated/fs_only.rs
```

Expected: all commands pass

## Open Questions Resolved for V1

- Use one shared IR for `validate`, `run`, and `build`: yes.
- Keep generated Rust editable by humans: yes.
- Support declarative steps plus explicit `sh.run`: yes.
- Support only sequential execution and fail-fast semantics: yes.

## Risks

- Step catalog bloat will tempt DSL creep. Resist it.
- If `run` gains behaviors that `build` cannot lower, the product contract breaks.
- `http.get` needs a clear contract for text-only output vs raw bytes. If binary payload handling becomes important, prefer `http.download`.
- Template quality matters more than template count. Three solid templates beats a sprawling pile of junk.

## Suggested First Commit Sequence

1. `feat: add smop CLI skeleton`
2. `feat: add script parsing and validation`
3. `feat: scaffold built-in smop templates`
4. `feat: execute declarative smop scripts`
5. `feat: generate durable Rust from smop scripts`
6. `docs: document smop CLI workflow`
