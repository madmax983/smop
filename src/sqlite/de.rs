//! One SQLite row -> serde.
//!
//! - Struct or map: one entry for each column, by column name.
//! - Tuple: one item for each column, by position. The counts must agree.
//! - Scalar, `Option`, enum: the row must have one column.
//! - `Vec<T>`: one item for each column. A one-column BLOB row gives its bytes.

use std::fmt;

use rusqlite::types::Value;
use serde::de::value::{SeqDeserializer, StrDeserializer};
use serde::de::{
    self, DeserializeSeed, Deserializer, IntoDeserializer, MapAccess, SeqAccess, Unexpected,
    Visitor,
};

/// A row mapping error. It names the column when it applies to one.
#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

impl Error {
    fn in_column(self, name: &str) -> Self {
        Self(format!("column {name:?}: {}", self.0))
    }
}

fn columns(n: usize) -> String {
    if n == 1 {
        "1 column".to_string()
    } else {
        format!("{n} columns")
    }
}

/// One row: the column names of the statement and the values of the row.
pub struct Row<'a> {
    pub names: &'a [String],
    pub values: Vec<Value>,
}

impl<'a> Row<'a> {
    /// The one column of the row.
    fn single(self) -> Result<Cell<'a>, Error> {
        let n = self.values.len();
        match (self.names.first(), self.values.into_iter().next()) {
            (Some(name), Some(value)) if n == 1 => Ok(Cell { name, value }),
            _ => Err(Error(format!("expected 1 column, got {}", columns(n)))),
        }
    }

    /// Runs `f` on the one column. An error names the column.
    fn on_single<T>(self, f: impl FnOnce(Cell<'a>) -> Result<T, Error>) -> Result<T, Error> {
        let cell = self.single()?;
        let name = cell.name;
        f(cell).map_err(|e| e.in_column(name))
    }

    fn cells(self) -> Cells<'a> {
        Cells {
            names: self.names.iter(),
            values: self.values.into_iter(),
            value: None,
        }
    }
}

macro_rules! single {
    ($($method:ident)*) => {
        $(
            fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
                self.on_single(|cell| cell.$method(visitor))
            }
        )*
    };
}

impl<'de> Deserializer<'de> for Row<'_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    single! {
        deserialize_bool deserialize_i8 deserialize_i16 deserialize_i32 deserialize_i64
        deserialize_i128 deserialize_u8 deserialize_u16 deserialize_u32 deserialize_u64
        deserialize_u128 deserialize_f32 deserialize_f64 deserialize_char deserialize_str
        deserialize_string deserialize_bytes deserialize_byte_buf deserialize_option
        deserialize_unit deserialize_identifier
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if matches!(self.values.as_slice(), [Value::Blob(_)]) {
            return self.on_single(|cell| cell.deserialize_seq(visitor));
        }
        visitor.visit_seq(self.cells())
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        let n = self.values.len();
        if n != len {
            return Err(Error(format!(
                "expected {}, got {}",
                columns(len),
                columns(n)
            )));
        }
        visitor.visit_seq(self.cells())
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_map(self.cells())
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.on_single(|cell| cell.deserialize_enum(name, variants, visitor))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }
}

/// The columns of a row, for a map or a sequence.
struct Cells<'a> {
    names: std::slice::Iter<'a, String>,
    values: std::vec::IntoIter<Value>,
    /// The value after `next_key_seed`, for `next_value_seed`.
    value: Option<Cell<'a>>,
}

impl<'de> SeqAccess<'de> for Cells<'_> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        match (self.names.next(), self.values.next()) {
            (Some(name), Some(value)) => seed
                .deserialize(Cell { name, value })
                .map(Some)
                .map_err(|e| e.in_column(name)),
            _ => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.values.len())
    }
}

impl<'de> MapAccess<'de> for Cells<'_> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        match (self.names.next(), self.values.next()) {
            (Some(name), Some(value)) => {
                self.value = Some(Cell { name, value });
                let key: StrDeserializer<'_, Error> = name.as_str().into_deserializer();
                seed.deserialize(key).map(Some)
            }
            _ => Ok(None),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let cell = self
            .value
            .take()
            .ok_or_else(|| Error("a value was read before its column name".to_string()))?;
        let name = cell.name;
        seed.deserialize(cell).map_err(|e| e.in_column(name))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.values.len())
    }
}

/// One column of a row.
struct Cell<'a> {
    name: &'a str,
    value: Value,
}

impl Cell<'_> {
    /// `NULL` into a type that is not `Option` or `()`.
    fn null<'de, V: Visitor<'de>>(visitor: &V) -> Error {
        de::Error::invalid_type(Unexpected::Other("NULL"), visitor)
    }
}

macro_rules! not_null {
    ($($method:ident)*) => {
        $(
            fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
                if matches!(self.value, Value::Null) {
                    return Err(Self::null(&visitor));
                }
                self.deserialize_any(visitor)
            }
        )*
    };
}

impl<'de> Deserializer<'de> for Cell<'_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Null => visitor.visit_unit(),
            Value::Integer(i) => visitor.visit_i64(i),
            Value::Real(f) => visitor.visit_f64(f),
            Value::Text(s) => visitor.visit_string(s),
            Value::Blob(b) => visitor.visit_byte_buf(b),
        }
    }

    not_null! {
        deserialize_i8 deserialize_i16 deserialize_i32 deserialize_i64 deserialize_i128
        deserialize_u8 deserialize_u16 deserialize_u32 deserialize_u64 deserialize_u128
        deserialize_f32 deserialize_f64
        deserialize_bytes deserialize_byte_buf deserialize_map deserialize_identifier
    }

    /// Only TEXT gives a string. serde would take UTF-8 bytes from a BLOB.
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Null => Err(Self::null(&visitor)),
            Value::Blob(b) => Err(de::Error::invalid_type(Unexpected::Bytes(&b), &visitor)),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_string(visitor)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_string(visitor)
    }

    /// SQLite has no boolean type: 0 is `false`, other integers are `true`.
    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Null => Err(Self::null(&visitor)),
            Value::Integer(i) => visitor.visit_bool(i != 0),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_any(visitor)
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_any(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    /// A BLOB is a sequence of bytes, so `Vec<u8>` works.
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Null => Err(Self::null(&visitor)),
            Value::Blob(b) => SeqDeserializer::new(b.into_iter()).deserialize_any(visitor),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    /// TEXT gives a unit variant by its name.
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.value {
            Value::Null => Err(Self::null(&visitor)),
            Value::Text(s) => visitor.visit_enum(s.into_deserializer()),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }
}
