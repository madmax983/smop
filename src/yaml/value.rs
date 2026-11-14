//! `Value`: a YAML document without a Rust type.

use std::fmt;
use std::ops::Index;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};

/// A YAML document without a Rust type.
///
/// A missing key or index gives [`Value::Null`], so a chain of `[]` does
/// not panic:
///
/// ```
/// use smop::yaml::{self, Value};
///
/// let doc: Value = yaml::from_str("jobs:\n  test:\n    runs-on: ubuntu-latest\n")?;
/// assert_eq!(doc["jobs"]["test"]["runs-on"].as_str(), Some("ubuntu-latest"));
/// assert!(doc["jobs"]["lint"]["steps"][0].is_null());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    /// `null`, `~` or no value.
    #[default]
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// An integer. `i128` holds each `i64` and `u64`.
    Int(i128),
    /// A float, also `.inf` and `.nan`.
    Float(f64),
    /// A string.
    String(String),
    /// A sequence.
    Sequence(Vec<Self>),
    /// A mapping, in document order. The parser gives unique keys. The
    /// writer returns an error for a duplicate key.
    Mapping(Vec<(Self, Self)>),
}

static NULL: Value = Value::Null;

impl Value {
    /// The value of the string key `key` in a mapping.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        self.as_mapping()?
            .iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)
    }

    /// The value of a key of any type in a mapping, for example
    /// `Value::Int(200)` for the key `200:`.
    #[must_use]
    pub fn get_key(&self, key: &Self) -> Option<&Self> {
        self.as_mapping()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// True for [`Value::Null`].
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// The boolean, if this is one.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The integer, if this is one in the `i64` range.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(i) => i64::try_from(*i).ok(),
            _ => None,
        }
    }

    /// The integer, if this is one.
    #[must_use]
    pub const fn as_i128(&self) -> Option<i128> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// The integer, if this is one in the `u64` range.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Int(i) => u64::try_from(*i).ok(),
            _ => None,
        }
    }

    /// The float, or the integer as a float.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // As `as f64`.
    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// The string, if this is one.
    #[must_use]
    pub const fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// The items, if this is a sequence.
    #[must_use]
    pub const fn as_sequence(&self) -> Option<&[Self]> {
        match self {
            Self::Sequence(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    /// The pairs, if this is a mapping.
    #[must_use]
    pub const fn as_mapping(&self) -> Option<&[(Self, Self)]> {
        match self {
            Self::Mapping(pairs) => Some(pairs.as_slice()),
            _ => None,
        }
    }
}

impl Index<&str> for Value {
    type Output = Self;

    /// The value of a string key, or [`Value::Null`].
    fn index(&self, key: &str) -> &Self {
        self.get(key).unwrap_or(&NULL)
    }
}

impl Index<usize> for Value {
    type Output = Self;

    /// The item at `index` of a sequence, or [`Value::Null`].
    fn index(&self, index: usize) -> &Self {
        self.as_sequence()
            .and_then(|items| items.get(index))
            .unwrap_or(&NULL)
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => s.serialize_unit(),
            Self::Bool(b) => s.serialize_bool(*b),
            Self::Int(i) => match (i64::try_from(*i), u64::try_from(*i)) {
                (Ok(x), _) => s.serialize_i64(x),
                (_, Ok(x)) => s.serialize_u64(x),
                _ => s.serialize_i128(*i),
            },
            Self::Float(f) => s.serialize_f64(*f),
            Self::String(text) => s.serialize_str(text),
            Self::Sequence(items) => s.collect_seq(items),
            Self::Mapping(pairs) => {
                let mut map = s.serialize_map(Some(pairs.len()))?;
                for (k, v) in pairs {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a YAML value")
    }

    fn visit_bool<E>(self, b: bool) -> Result<Value, E> {
        Ok(Value::Bool(b))
    }

    fn visit_i64<E>(self, i: i64) -> Result<Value, E> {
        Ok(Value::Int(i.into()))
    }

    fn visit_u64<E>(self, u: u64) -> Result<Value, E> {
        Ok(Value::Int(u.into()))
    }

    fn visit_i128<E>(self, i: i128) -> Result<Value, E> {
        Ok(Value::Int(i))
    }

    fn visit_u128<E: de::Error>(self, u: u128) -> Result<Value, E> {
        i128::try_from(u)
            .map(Value::Int)
            .map_err(|_| E::custom(format!("integer `{u}` is out of range for a Value")))
    }

    fn visit_f64<E>(self, f: f64) -> Result<Value, E> {
        Ok(Value::Float(f))
    }

    fn visit_str<E>(self, s: &str) -> Result<Value, E> {
        Ok(Value::String(s.to_owned()))
    }

    fn visit_string<E>(self, s: String) -> Result<Value, E> {
        Ok(Value::String(s))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        Value::deserialize(d)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Sequence(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut pairs = Vec::with_capacity(map.size_hint().unwrap_or(0));
        while let Some(pair) = map.next_entry()? {
            pairs.push(pair);
        }
        Ok(Value::Mapping(pairs))
    }
}
