use std::collections::BTreeSet;

use anyhow::{Result, bail};
use toml::Value;

use super::model::{Script, Step};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedScript {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<ValidatedStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedStep {
    pub name: String,
    pub kind: StepKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKind {
    EnvRequire { vars: Vec<String> },
    FsMkdirAll { path: String },
    FsWriteString { path: String, content: String },
    FsAppend { path: String, content: String },
    FsCopy { from: String, to: String },
    FsRename { from: String, to: String },
    FsRemove { path: String },
    HttpDownload { url: String, path: String },
    HttpGet { url: String, dest: String },
    ArchiveCreateZip { source: String, dest: String },
    ArchiveCreateTar { source: String, dest: String },
    ArchiveCreateTarGz { source: String, dest: String },
    ArchiveExtractZip { archive: String, dest: String },
    ArchiveExtractTar { archive: String, dest: String },
    ArchiveExtractTarGz { archive: String, dest: String },
    ShRun { command: String },
}

pub fn validate_script(script: &Script) -> Result<ValidatedScript> {
    let mut names = BTreeSet::new();
    let mut steps = Vec::with_capacity(script.steps.len());

    for step in &script.steps {
        if !names.insert(step.name.clone()) {
            bail!("Duplicate step name: {}", step.name);
        }

        steps.push(ValidatedStep {
            name: step.name.clone(),
            kind: validate_step(step)?,
        });
    }

    Ok(ValidatedScript {
        name: script.name.clone(),
        description: script.description.clone(),
        steps,
    })
}

fn validate_step(step: &Step) -> Result<StepKind> {
    match step.step_type.as_str() {
        "env.require" => {
            ensure_fields(step, &["vars"])?;
            Ok(StepKind::EnvRequire {
                vars: string_list(step, "vars")?,
            })
        }
        "fs.mkdir_all" => {
            ensure_fields(step, &["path"])?;
            Ok(StepKind::FsMkdirAll {
                path: required_string(step, "path")?,
            })
        }
        "fs.write_string" => {
            ensure_fields(step, &["path", "content"])?;
            Ok(StepKind::FsWriteString {
                path: required_string(step, "path")?,
                content: required_string(step, "content")?,
            })
        }
        "fs.append" => {
            ensure_fields(step, &["path", "content"])?;
            Ok(StepKind::FsAppend {
                path: required_string(step, "path")?,
                content: required_string(step, "content")?,
            })
        }
        "fs.copy" => {
            ensure_fields(step, &["from", "to"])?;
            Ok(StepKind::FsCopy {
                from: required_string(step, "from")?,
                to: required_string(step, "to")?,
            })
        }
        "fs.rename" => {
            ensure_fields(step, &["from", "to"])?;
            Ok(StepKind::FsRename {
                from: required_string(step, "from")?,
                to: required_string(step, "to")?,
            })
        }
        "fs.remove" => {
            ensure_fields(step, &["path"])?;
            Ok(StepKind::FsRemove {
                path: required_string(step, "path")?,
            })
        }
        "http.download" => {
            ensure_fields(step, &["url", "path"])?;
            Ok(StepKind::HttpDownload {
                url: required_string(step, "url")?,
                path: required_string(step, "path")?,
            })
        }
        "http.get" => {
            ensure_fields(step, &["url", "dest"])?;
            Ok(StepKind::HttpGet {
                url: required_string(step, "url")?,
                dest: required_string(step, "dest")?,
            })
        }
        "archive.create_zip" => {
            ensure_fields(step, &["source", "dest"])?;
            Ok(StepKind::ArchiveCreateZip {
                source: required_string(step, "source")?,
                dest: required_string(step, "dest")?,
            })
        }
        "archive.create_tar" => {
            ensure_fields(step, &["source", "dest"])?;
            Ok(StepKind::ArchiveCreateTar {
                source: required_string(step, "source")?,
                dest: required_string(step, "dest")?,
            })
        }
        "archive.create_tar_gz" => {
            ensure_fields(step, &["source", "dest"])?;
            Ok(StepKind::ArchiveCreateTarGz {
                source: required_string(step, "source")?,
                dest: required_string(step, "dest")?,
            })
        }
        "archive.extract_zip" => {
            ensure_fields(step, &["archive", "dest"])?;
            Ok(StepKind::ArchiveExtractZip {
                archive: required_string(step, "archive")?,
                dest: required_string(step, "dest")?,
            })
        }
        "archive.extract_tar" => {
            ensure_fields(step, &["archive", "dest"])?;
            Ok(StepKind::ArchiveExtractTar {
                archive: required_string(step, "archive")?,
                dest: required_string(step, "dest")?,
            })
        }
        "archive.extract_tar_gz" => {
            ensure_fields(step, &["archive", "dest"])?;
            Ok(StepKind::ArchiveExtractTarGz {
                archive: required_string(step, "archive")?,
                dest: required_string(step, "dest")?,
            })
        }
        "sh.run" => {
            ensure_fields(step, &["command"])?;
            Ok(StepKind::ShRun {
                command: required_string(step, "command")?,
            })
        }
        other => bail!("Unknown step type: {other}"),
    }
}

fn ensure_fields(step: &Step, allowed: &[&str]) -> Result<()> {
    let allowed: BTreeSet<&str> = allowed.iter().copied().collect();

    for field in step.fields.keys() {
        if !allowed.contains(field.as_str()) {
            bail!(
                "Step '{}' ({}) has unknown field '{}'",
                step.name,
                step.step_type,
                field
            );
        }
    }

    Ok(())
}

fn required_string(step: &Step, field: &str) -> Result<String> {
    match step.fields.get(field).and_then(Value::as_str) {
        Some(value) => Ok(value.to_owned()),
        None => bail!(
            "Step '{}' ({}) is missing required field '{}'",
            step.name,
            step.step_type,
            field
        ),
    }
}

fn string_list(step: &Step, field: &str) -> Result<Vec<String>> {
    let values = step
        .fields
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Step '{}' ({}) is missing required field '{}'",
                step.name,
                step.step_type,
                field
            )
        })?;

    values
        .iter()
        .map(|value| {
            value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                anyhow::anyhow!(
                    "Step '{}' ({}) field '{}' must be a list of strings",
                    step.name,
                    step.step_type,
                    field
                )
            })
        })
        .collect()
}
