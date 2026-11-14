//! YAML 1.2 for config and CI files, with no extra crate.
//!
//! smop has its own parser and writer (ADR 0019). [`crate::fs::read_yaml`]
//! and [`crate::fs::write_yaml`] use them. [`from_str`] and [`to_string`]
//! work on text, and [`Value`] holds a document that has no Rust type.
//!
//! # Schema
//!
//! Plain scalars use the YAML 1.2 core schema. `on`, `yes` and `no` are
//! strings, not booleans. A plain scalar read into `String` gives its text:
//! `python: 3.10` gives `"3.10"`. Read into [`Value`] or `f64`, it gives
//! `3.1`. A null value read into a sequence, map or struct gives an empty one.
//!
//! serde reads `#[serde(flatten)]`, untagged and internally tagged types
//! through a buffer. In these types a `String` field gets the core-schema
//! value, so quote `"3.10"` there. An error in these types can give the
//! position of the enclosing node.
//!
//! # Supported syntax
//!
//! - Block and flow collections. JSON is valid input.
//! - Plain, single-quoted and double-quoted scalars, and block scalars
//!   (`|`, `>`, chomping and indentation indicators).
//! - Comments, `---`, `...` and `%YAML`.
//! - Anchors, aliases and `<<` merge keys.
//! - The tags `!`, `!!str`, `!!int`, `!!float`, `!!bool`, `!!null`,
//!   `!!seq` and `!!map`.
//!
//! These are errors: a second document, `?` keys, a collection or an alias
//! as a key, an anchor or tag before an implicit key, a key-value pair in a
//! flow sequence, directives other than `%YAML`, other tags, a duplicate
//! key, a tab in indentation, nesting deeper than 128, and aliases that copy
//! more than 1,000,000 nodes. Each error gives the line and the column.
//!
//! # Examples
//!
//! ```
//! use smop::yaml::{self, Value};
//!
//! let doc: Value = yaml::from_str("on:\n  push:\n    branches: [main]\n")?;
//! assert_eq!(doc["on"]["push"]["branches"][0].as_str(), Some("main"));
//!
//! let text = yaml::to_string(&vec!["a", "yes"])?;
//! assert_eq!(text, "- a\n- \"yes\"\n");
//! # Ok::<(), anyhow::Error>(())
//! ```

mod de;
mod parse;
mod scalar;
mod ser;
mod value;

use std::fmt;

use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};

pub use value::Value;

/// A position in the text. Line and column start at 1. The column counts
/// characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mark {
    line: usize,
    column: usize,
}

/// A parse, read or write error, with the position when one is known.
#[derive(Debug)]
struct Error {
    msg: String,
    mark: Option<Mark>,
}

impl Error {
    fn new(msg: impl Into<String>, mark: Mark) -> Self {
        Self {
            msg: msg.into(),
            mark: Some(mark),
        }
    }

    fn message(msg: impl fmt::Display) -> Self {
        Self {
            msg: msg.to_string(),
            mark: None,
        }
    }

    /// Sets the position if the error has none. So the innermost node wins.
    const fn at(mut self, mark: Mark) -> Self {
        if self.mark.is_none() {
            self.mark = Some(mark);
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.mark {
            Some(m) => write!(f, "line {}, column {}: {}", m.line, m.column, self.msg),
            None => f.write_str(&self.msg),
        }
    }
}

impl std::error::Error for Error {}

impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::message(msg)
    }
}

impl serde::ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::message(msg)
    }
}

/// Parses one YAML document into `T`.
///
/// # Errors
///
/// Returns an error if the text is not valid YAML in the supported syntax,
/// or if the document does not fit `T`. The error gives the line and the
/// column.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use smop::yaml;
///
/// let ports: BTreeMap<String, u16> = yaml::from_str("web: 80\nssl: 443\n")?;
/// assert_eq!(ports["ssl"], 443);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn from_str<T: DeserializeOwned>(text: &str) -> Result<T> {
    let node = parse::parse(text)?;
    Ok(T::deserialize(de::NodeDe(&node)).map_err(|e| e.at(node.mark))?)
}

/// Writes `value` as one YAML document in block style, with 2 spaces of
/// indentation and a line break at the end.
///
/// A string that a YAML 1.1 reader reads as another type (`yes`, `off`,
/// `012`) is quoted. A string with `\n` line breaks and no control
/// characters is a literal block scalar. Other strings that need quotes are
/// double-quoted.
///
/// # Errors
///
/// Returns an error if `value` has no YAML form here: a sequence or a
/// mapping as a mapping key, a duplicate key in a mapping, or a custom
/// `Serialize` error.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use smop::yaml;
///
/// let env = BTreeMap::from([("CI", "true"), ("NAME", "smop")]);
/// assert_eq!(yaml::to_string(&env)?, "CI: \"true\"\nNAME: smop\n");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let out = value.serialize(ser::OutSerializer)?;
    Ok(ser::emit(&out)?)
}
