use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Script {
    pub name: String,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(rename = "step", default)]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Step {
    pub name: String,

    #[serde(rename = "type")]
    pub step_type: String,

    #[serde(flatten)]
    pub fields: BTreeMap<String, toml::Value>,
}
