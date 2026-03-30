use super::validate::{StepKind, ValidatedScript, ValidatedStep};

/// Renders a validated script as a standalone Rust binary.
#[must_use]
pub fn render_script(script: &ValidatedScript) -> String {
    let mut output = String::new();

    output.push_str("use smop::prelude::*;\n\n");
    output.push_str("fn main() -> Result<()> {\n");

    for step in &script.steps {
        render_step(&mut output, step);
        output.push('\n');
    }

    output.push_str("    Ok(())\n");
    output.push_str("}\n");
    output
}

fn render_step(output: &mut String, step: &ValidatedStep) {
    push_step_comment(output, &step.name);

    match &step.kind {
        StepKind::EnvRequire { vars } => {
            push_line(output, &format!("    env::require_vars(&{:?})?;", vars));
        }
        StepKind::FsMkdirAll { path } => {
            push_line(
                output,
                &format!("    std::fs::create_dir_all({})?;", quote(path)),
            );
        }
        StepKind::FsWriteString { path, content } => {
            push_line(
                output,
                &format!(
                    "    fs::write_string({}, {})?;",
                    quote(path),
                    quote(content)
                ),
            );
        }
        StepKind::FsAppend { path, content } => {
            push_line(
                output,
                &format!("    fs::append({}, {})?;", quote(path), quote(content)),
            );
        }
        StepKind::FsCopy { from, to } => {
            push_line(
                output,
                &format!("    fs::copy({}, {})?;", quote(from), quote(to)),
            );
        }
        StepKind::FsRename { from, to } => {
            push_line(
                output,
                &format!("    fs::rename({}, {})?;", quote(from), quote(to)),
            );
        }
        StepKind::FsRemove { path } => {
            push_line(output, &format!("    fs::remove({})?;", quote(path)));
        }
        StepKind::HttpDownload { url, path } => {
            push_line(
                output,
                &format!("    http::download({}, {})?;", quote(url), quote(path)),
            );
        }
        StepKind::HttpGet { url, dest } => {
            push_line(
                output,
                &format!("    let body = http::get({})?;", quote(url)),
            );
            push_line(
                output,
                &format!("    fs::write_string({}, body)?;", quote(dest)),
            );
        }
        StepKind::ArchiveCreateZip { source, dest } => {
            push_line(
                output,
                &format!(
                    "    archive::create_zip({}, {})?;",
                    quote(source),
                    quote(dest)
                ),
            );
        }
        StepKind::ArchiveCreateTar { source, dest } => {
            push_line(
                output,
                &format!(
                    "    archive::create_tar({}, {})?;",
                    quote(source),
                    quote(dest)
                ),
            );
        }
        StepKind::ArchiveCreateTarGz { source, dest } => {
            push_line(
                output,
                &format!(
                    "    archive::create_tar_gz({}, {})?;",
                    quote(source),
                    quote(dest)
                ),
            );
        }
        StepKind::ArchiveExtractZip { archive, dest } => {
            push_line(
                output,
                &format!(
                    "    archive::extract_zip({}, {})?;",
                    quote(archive),
                    quote(dest)
                ),
            );
        }
        StepKind::ArchiveExtractTar { archive, dest } => {
            push_line(
                output,
                &format!(
                    "    archive::extract_tar({}, {})?;",
                    quote(archive),
                    quote(dest)
                ),
            );
        }
        StepKind::ArchiveExtractTarGz { archive, dest } => {
            push_line(
                output,
                &format!(
                    "    archive::extract_tar_gz({}, {})?;",
                    quote(archive),
                    quote(dest)
                ),
            );
        }
        StepKind::ShRun { command } => {
            push_line(output, &format!("    sh::run({})?;", quote(command)));
        }
    }
}

fn push_step_comment(output: &mut String, step_name: &str) {
    let mut lines = step_name.lines();

    if let Some(first_line) = lines.next() {
        push_line(output, &format!("    // Step: {first_line}"));
        for line in lines {
            push_line(output, &format!("    // {line}"));
        }
    } else {
        push_line(output, "    // Step:");
    }
}

fn quote(value: &str) -> String {
    format!("{value:?}")
}

fn push_line(output: &mut String, line: &str) {
    output.push_str(line);
    output.push('\n');
}
