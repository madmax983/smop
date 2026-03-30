use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    Backup,
    HttpFetch,
    FilePipeline,
}

impl Template {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "backup" => Ok(Self::Backup),
            "http-fetch" => Ok(Self::HttpFetch),
            "file-pipeline" => Ok(Self::FilePipeline),
            other => bail!("Unknown template: {other}"),
        }
    }
}

pub fn render_script(template: Template) -> &'static str {
    match template {
        Template::Backup => BACKUP_SCRIPT,
        Template::HttpFetch => HTTP_FETCH_SCRIPT,
        Template::FilePipeline => FILE_PIPELINE_SCRIPT,
    }
}

pub fn render_readme(template: Template) -> &'static str {
    match template {
        Template::Backup => BACKUP_README,
        Template::HttpFetch => HTTP_FETCH_README,
        Template::FilePipeline => FILE_PIPELINE_README,
    }
}

const BACKUP_SCRIPT: &str = r#"name = "backup-project"
description = "Backup the current project into build artifacts"

[[step]]
name = "check-env"
type = "env.require"
vars = ["BACKUP_ROOT"]

[[step]]
name = "prepare-output"
type = "fs.mkdir_all"
path = "build"

[[step]]
name = "write-manifest"
type = "fs.write_string"
path = "build/manifest.txt"
content = "backup starting\n"
"#;

const BACKUP_README: &str = r#"# backup

This template creates a simple backup manifest and archive pipeline.

Run:

```bash
smop validate script.toml
```
"#;

const HTTP_FETCH_SCRIPT: &str = r#"name = "http-fetch"
description = "Fetch JSON from an API and store it locally"

[[step]]
name = "prepare-output"
type = "fs.mkdir_all"
path = "build"

[[step]]
name = "fetch-data"
type = "http.get"
url = "https://httpbin.org/json"
dest = "build/response.json"
"#;

const HTTP_FETCH_README: &str = r#"# http-fetch

This template fetches a JSON document and stores it on disk.
"#;

const FILE_PIPELINE_SCRIPT: &str = r#"name = "file-pipeline"
description = "Read, copy, and transform files locally"

[[step]]
name = "prepare-output"
type = "fs.mkdir_all"
path = "build"

[[step]]
name = "write-message"
type = "fs.write_string"
path = "build/message.txt"
content = "hello from smop\n"
"#;

const FILE_PIPELINE_README: &str = r#"# file-pipeline

This template demonstrates local filesystem work without network access.
"#;
