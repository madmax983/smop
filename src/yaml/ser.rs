//! serde to YAML text: first a tree, then block-style text.

use serde::ser::{self, Serialize};

use super::Error;
use super::scalar;

type Result<T> = std::result::Result<T, Error>;

/// A serialized value before layout.
pub(super) enum Out {
    /// Text that is already a valid plain scalar: a number, a bool or null.
    Plain(String),
    /// A string. The writer chooses plain, quoted or block style.
    Str(String),
    Seq(Vec<Self>),
    Map(Vec<(Self, Self)>),
}

fn null() -> Out {
    Out::Plain("null".into())
}

fn f64_text(v: f64) -> String {
    if v.is_nan() {
        ".nan".into()
    } else if v.is_infinite() {
        if v > 0.0 { ".inf" } else { "-.inf" }.into()
    } else {
        float_1_1(&format!("{v:?}"))
    }
}

fn f32_text(v: f32) -> String {
    if v.is_nan() || v.is_infinite() {
        f64_text(f64::from(v))
    } else {
        float_1_1(&format!("{v:?}"))
    }
}

/// Rust's shortest float text in a form that YAML 1.1 also reads as a
/// float: a `.` in the mantissa and a sign on the exponent (`1.0e-5`,
/// `1.5e+300`). Debug text always has a `.` or an exponent.
fn float_1_1(debug: &str) -> String {
    let Some((mantissa, exponent)) = debug.split_once('e') else {
        return debug.to_string();
    };
    let dot = if mantissa.contains('.') { "" } else { ".0" };
    let sign = if exponent.starts_with('-') { "" } else { "+" };
    format!("{mantissa}{dot}e{sign}{exponent}")
}

/// serde to [`Out`].
pub(super) struct OutSerializer;

/// Defines `serialize_*` methods for numbers that are plain text.
macro_rules! plain {
    ($($method:ident: $ty:ty),* $(,)?) => {$(
        fn $method(self, v: $ty) -> Result<Out> {
            Ok(Out::Plain(v.to_string()))
        }
    )*};
}

impl ser::Serializer for OutSerializer {
    type Ok = Out;
    type Error = Error;
    type SerializeSeq = SeqOut;
    type SerializeTuple = SeqOut;
    type SerializeTupleStruct = SeqOut;
    type SerializeTupleVariant = VariantOut<SeqOut>;
    type SerializeMap = MapOut;
    type SerializeStruct = MapOut;
    type SerializeStructVariant = VariantOut<MapOut>;

    plain! {
        serialize_bool: bool,
        serialize_i8: i8, serialize_i16: i16, serialize_i32: i32, serialize_i64: i64, serialize_i128: i128,
        serialize_u8: u8, serialize_u16: u16, serialize_u32: u32, serialize_u64: u64, serialize_u128: u128,
    }

    fn serialize_f32(self, v: f32) -> Result<Out> {
        Ok(Out::Plain(f32_text(v)))
    }

    fn serialize_f64(self, v: f64) -> Result<Out> {
        Ok(Out::Plain(f64_text(v)))
    }

    fn serialize_char(self, v: char) -> Result<Out> {
        Ok(Out::Str(v.to_string()))
    }

    fn serialize_str(self, v: &str) -> Result<Out> {
        Ok(Out::Str(v.to_string()))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Out> {
        Ok(Out::Seq(
            v.iter().map(|b| Out::Plain(b.to_string())).collect(),
        ))
    }

    fn serialize_none(self) -> Result<Out> {
        Ok(null())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Out> {
        v.serialize(self)
    }

    fn serialize_unit(self) -> Result<Out> {
        Ok(null())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Out> {
        Ok(null())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<Out> {
        Ok(Out::Str(variant.into()))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        v: &T,
    ) -> Result<Out> {
        v.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Out> {
        Ok(Out::Map(vec![(
            Out::Str(variant.into()),
            v.serialize(self)?,
        )]))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SeqOut> {
        Ok(SeqOut(Vec::with_capacity(len.unwrap_or(0))))
    }

    fn serialize_tuple(self, len: usize) -> Result<SeqOut> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<SeqOut> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<VariantOut<SeqOut>> {
        Ok(VariantOut(variant, self.serialize_seq(Some(len))?))
    }

    fn serialize_map(self, len: Option<usize>) -> Result<MapOut> {
        Ok(MapOut {
            pairs: Vec::with_capacity(len.unwrap_or(0)),
            key: None,
        })
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<MapOut> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<VariantOut<MapOut>> {
        Ok(VariantOut(variant, self.serialize_map(Some(len))?))
    }
}

pub(super) struct SeqOut(Vec<Out>);

impl SeqOut {
    fn push<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<()> {
        self.0.push(v.serialize(OutSerializer)?);
        Ok(())
    }
}

impl ser::SerializeSeq for SeqOut {
    type Ok = Out;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<()> {
        self.push(v)
    }

    fn end(self) -> Result<Out> {
        Ok(Out::Seq(self.0))
    }
}

impl ser::SerializeTuple for SeqOut {
    type Ok = Out;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<()> {
        self.push(v)
    }

    fn end(self) -> Result<Out> {
        Ok(Out::Seq(self.0))
    }
}

impl ser::SerializeTupleStruct for SeqOut {
    type Ok = Out;
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<()> {
        self.push(v)
    }

    fn end(self) -> Result<Out> {
        Ok(Out::Seq(self.0))
    }
}

pub(super) struct MapOut {
    pairs: Vec<(Out, Out)>,
    key: Option<Out>,
}

impl ser::SerializeMap for MapOut {
    type Ok = Out;
    type Error = Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        self.key = Some(key.serialize(OutSerializer)?);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<()> {
        let key = self.key.take().ok_or_else(|| {
            <Error as ser::Error>::custom("a map value was written before its key")
        })?;
        self.pairs.push((key, v.serialize(OutSerializer)?));
        Ok(())
    }

    fn end(self) -> Result<Out> {
        Ok(Out::Map(self.pairs))
    }
}

impl ser::SerializeStruct for MapOut {
    type Ok = Out;
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, v: &T) -> Result<()> {
        self.pairs
            .push((Out::Str(key.into()), v.serialize(OutSerializer)?));
        Ok(())
    }

    fn end(self) -> Result<Out> {
        Ok(Out::Map(self.pairs))
    }
}

/// An enum variant with data: a mapping with the variant name as its key.
pub(super) struct VariantOut<T>(&'static str, T);

impl ser::SerializeTupleVariant for VariantOut<SeqOut> {
    type Ok = Out;
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<()> {
        self.1.push(v)
    }

    fn end(self) -> Result<Out> {
        Ok(Out::Map(vec![(
            Out::Str(self.0.into()),
            Out::Seq(self.1.0),
        )]))
    }
}

impl ser::SerializeStructVariant for VariantOut<MapOut> {
    type Ok = Out;
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, v: &T) -> Result<()> {
        ser::SerializeStruct::serialize_field(&mut self.1, key, v)
    }

    fn end(self) -> Result<Out> {
        Ok(Out::Map(vec![(
            Out::Str(self.0.into()),
            Out::Map(self.1.pairs),
        )]))
    }
}

// -- Layout ----------------------------------------------------------------

/// Where a node goes: the document start, after `key:`, or after `-`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Top,
    Value,
    Item,
}

/// Block-style text for `out`, with a line break at the end.
pub(super) fn emit(out: &Out) -> Result<String> {
    let mut text = String::new();
    node(&mut text, out, 0, Place::Top)?;
    Ok(text)
}

fn pad(text: &mut String, n: usize) {
    text.extend(std::iter::repeat_n(' ', n));
}

/// Writes `out` after its prefix (`key:` or `-` at column `indent`), then
/// a line break.
fn node(text: &mut String, out: &Out, indent: usize, place: Place) -> Result<()> {
    let sep = if place == Place::Top { "" } else { " " };
    let child = if place == Place::Top { 0 } else { indent + 2 };
    match out {
        Out::Seq(items) if !items.is_empty() => {
            match place {
                Place::Value => text.push('\n'),
                Place::Item => text.push(' '),
                Place::Top => {}
            }
            seq(text, items, child, place == Place::Item)
        }
        Out::Map(pairs) if !pairs.is_empty() => {
            match place {
                Place::Value => text.push('\n'),
                Place::Item => text.push(' '),
                Place::Top => {}
            }
            map(text, pairs, child, place == Place::Item)
        }
        Out::Seq(_) => {
            text.push_str(sep);
            text.push_str("[]\n");
            Ok(())
        }
        Out::Map(_) => {
            text.push_str(sep);
            text.push_str("{}\n");
            Ok(())
        }
        Out::Plain(plain) => {
            text.push_str(sep);
            text.push_str(plain);
            text.push('\n');
            Ok(())
        }
        Out::Str(s) if scalar::literal_ok(s) => {
            text.push_str(sep);
            literal(text, s, if place == Place::Top { 2 } else { indent + 2 });
            Ok(())
        }
        Out::Str(s) => {
            text.push_str(sep);
            text.push_str(&scalar::inline(s));
            text.push('\n');
            Ok(())
        }
    }
}

fn seq(text: &mut String, items: &[Out], indent: usize, first_inline: bool) -> Result<()> {
    for (i, item) in items.iter().enumerate() {
        if i > 0 || !first_inline {
            pad(text, indent);
        }
        text.push('-');
        node(text, item, indent, Place::Item)?;
    }
    Ok(())
}

fn map(text: &mut String, pairs: &[(Out, Out)], indent: usize, first_inline: bool) -> Result<()> {
    // The writer gives one text for one key value, so equal text is a
    // duplicate key. The parser rejects a duplicate key.
    let mut keys = std::collections::HashSet::new();
    for (i, (key, value)) in pairs.iter().enumerate() {
        let key = match key {
            Out::Plain(plain) => plain.clone(),
            Out::Str(s) => scalar::inline(s),
            Out::Seq(_) | Out::Map(_) => {
                return Err(<Error as ser::Error>::custom(
                    "a mapping key must be a scalar, not a sequence or a mapping",
                ));
            }
        };
        if !keys.insert(key.clone()) {
            return Err(<Error as ser::Error>::custom(format!(
                "duplicate key `{key}` in a mapping"
            )));
        }
        if i > 0 || !first_inline {
            pad(text, indent);
        }
        text.push_str(&key);
        text.push(':');
        node(text, value, indent, Place::Value)?;
    }
    Ok(())
}

/// A literal block scalar with its lines at column `indent`. The header
/// has an indentation indicator if the first line starts with a space, and
/// a chomping indicator from the number of line breaks at the end.
fn literal(text: &mut String, s: &str, indent: usize) {
    let body = s.trim_end_matches('\n');
    let breaks = s.len() - body.len();
    text.push('|');
    if body
        .split('\n')
        .find(|l| !l.is_empty())
        .is_some_and(|l| l.starts_with(' '))
    {
        text.push('2');
    }
    text.push_str(match breaks {
        0 => "-",
        1 => "",
        _ => "+",
    });
    text.push('\n');
    for line in body.split('\n') {
        if !line.is_empty() {
            pad(text, indent);
            text.push_str(line);
        }
        text.push('\n');
    }
    text.extend(std::iter::repeat_n('\n', breaks.saturating_sub(1)));
}
