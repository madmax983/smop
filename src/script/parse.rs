use anyhow::Result;

use super::model::Script;

pub fn parse_script(source: &str) -> Result<Script> {
    Ok(toml::from_str(source)?)
}
