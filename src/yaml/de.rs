//! A node tree to serde. Each error gets the position of the innermost node.

use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, Expected, MapAccess, SeqAccess, Unexpected,
    VariantAccess, Visitor,
};

use super::Error;
use super::parse::{Node, NodeKind, Scalar};
use super::scalar::{self, Kind};

type Result<T> = std::result::Result<T, Error>;

/// Deserializes one node.
pub(super) struct NodeDe<'de>(pub(super) &'de Node);

impl<'de> NodeDe<'de> {
    const fn scalar(&self) -> Option<&'de Scalar> {
        match &self.0.kind {
            NodeKind::Scalar(s) => Some(s),
            _ => None,
        }
    }

    fn kind(&self) -> Option<Kind> {
        self.scalar().map(|s| s.kind)
    }

    fn is_null(&self) -> bool {
        self.kind() == Some(Kind::Null)
    }

    fn unexpected(&self) -> Unexpected<'de> {
        match &self.0.kind {
            NodeKind::Seq(_) => Unexpected::Seq,
            NodeKind::Map(_) => Unexpected::Map,
            NodeKind::Scalar(s) => match s.kind {
                Kind::Null => Unexpected::Unit,
                Kind::Bool(b) => Unexpected::Bool(b),
                Kind::Int => scalar::parse_int(&s.text)
                    .and_then(|i| i64::try_from(i).ok())
                    .map_or(Unexpected::Other("integer"), Unexpected::Signed),
                Kind::Float => scalar::parse_float(&s.text)
                    .map_or(Unexpected::Other("float"), Unexpected::Float),
                Kind::Str => Unexpected::Str(&s.text),
            },
        }
    }

    fn invalid<T>(&self, exp: &dyn Expected) -> Result<T> {
        Err(de::Error::invalid_type(self.unexpected(), exp))
    }

    /// An integer node as `T`, in range.
    fn int<T: TryFrom<i128> + TryFrom<u128>>(&self, exp: &dyn Expected) -> Result<T> {
        let Some(s) = self.scalar().filter(|s| s.kind == Kind::Int) else {
            return self.invalid(exp);
        };
        scalar::parse_int(&s.text)
            .and_then(|i| T::try_from(i).ok())
            .or_else(|| scalar::parse_uint(&s.text).and_then(|u| T::try_from(u).ok()))
            .ok_or_else(|| {
                de::Error::custom(format!(
                    "integer `{}` is out of range for {}",
                    s.text,
                    std::any::type_name::<T>()
                ))
            })
    }

    fn any<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        match &self.0.kind {
            NodeKind::Seq(items) => visit_seq(items, v),
            NodeKind::Map(pairs) => visit_map(pairs, v),
            NodeKind::Scalar(s) => match s.kind {
                Kind::Null => v.visit_unit(),
                Kind::Bool(b) => v.visit_bool(b),
                Kind::Str => v.visit_borrowed_str(&s.text),
                Kind::Float => v.visit_f64(scalar::parse_float(&s.text).unwrap_or(f64::NAN)),
                Kind::Int => match scalar::parse_int(&s.text) {
                    Some(i) => match (i64::try_from(i), u64::try_from(i)) {
                        (Ok(x), _) => v.visit_i64(x),
                        (_, Ok(x)) => v.visit_u64(x),
                        _ => v.visit_i128(i),
                    },
                    None => scalar::parse_uint(&s.text).map_or_else(
                        || {
                            Err(de::Error::custom(format!(
                                "integer `{}` is out of range",
                                s.text
                            )))
                        },
                        |u| v.visit_u128(u),
                    ),
                },
            },
        }
    }

    #[allow(clippy::cast_precision_loss)] // A YAML integer read as a float.
    fn f64<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        match self.scalar() {
            Some(s) if matches!(s.kind, Kind::Int | Kind::Float) => {
                v.visit_f64(scalar::parse_float(&s.text).unwrap_or(f64::NAN))
            }
            _ => self.invalid(&v),
        }
    }

    fn f32<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        match self.scalar() {
            Some(s) if matches!(s.kind, Kind::Int | Kind::Float) => {
                v.visit_f32(scalar::parse_f32(&s.text).unwrap_or(f32::NAN))
            }
            _ => self.invalid(&v),
        }
    }

    fn bool<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        match self.kind() {
            Some(Kind::Bool(b)) => v.visit_bool(b),
            _ => self.invalid(&v),
        }
    }

    fn char<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        let Some(s) = self.scalar() else {
            return self.invalid(&v);
        };
        let mut chars = s.text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => v.visit_char(c),
            _ => Err(de::Error::invalid_value(Unexpected::Str(&s.text), &v)),
        }
    }

    /// Any scalar gives its text, so `version: 3.10` reads as `"3.10"`.
    fn str<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        match self.scalar() {
            Some(s) => v.visit_borrowed_str(&s.text),
            None => self.invalid(&v),
        }
    }

    fn bytes<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        match &self.0.kind {
            NodeKind::Scalar(s) => v.visit_borrowed_bytes(s.text.as_bytes()),
            NodeKind::Seq(items) => visit_seq(items, v),
            NodeKind::Map(_) => self.invalid(&v),
        }
    }

    fn option<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        if self.is_null() {
            v.visit_none()
        } else {
            v.visit_some(self)
        }
    }

    fn unit<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        if self.is_null() {
            v.visit_unit()
        } else {
            self.invalid(&v)
        }
    }

    /// A null node is an empty sequence.
    fn seq<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        match &self.0.kind {
            NodeKind::Seq(items) => visit_seq(items, v),
            _ if self.is_null() => visit_seq(&[], v),
            _ => self.invalid(&v),
        }
    }

    /// A null node is an empty mapping.
    fn map<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        match &self.0.kind {
            NodeKind::Map(pairs) => visit_map(pairs, v),
            _ if self.is_null() => visit_map(&[], v),
            _ => self.invalid(&v),
        }
    }

    /// A scalar is a unit variant. A mapping with one key is any variant.
    fn enumeration<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        match &self.0.kind {
            NodeKind::Scalar(_) => v.visit_enum(EnumDe {
                variant: self.0,
                value: None,
            }),
            NodeKind::Map(pairs) => match pairs.as_slice() {
                [(k, value)] => v.visit_enum(EnumDe {
                    variant: k,
                    value: Some(value),
                }),
                _ => Err(de::Error::custom(
                    "an enum needs a scalar or a mapping with one key",
                )),
            },
            NodeKind::Seq(_) => self.invalid(&v),
        }
    }
}

fn visit_seq<'de, V: Visitor<'de>>(items: &'de [Node], v: V) -> Result<V::Value> {
    let mut access = SeqDe(items.iter());
    let value = v.visit_seq(&mut access)?;
    if access.0.len() == 0 {
        Ok(value)
    } else {
        Err(de::Error::invalid_length(
            items.len(),
            &"fewer elements in the sequence",
        ))
    }
}

fn visit_map<'de, V: Visitor<'de>>(pairs: &'de [(Node, Node)], v: V) -> Result<V::Value> {
    let mut access = MapDe {
        iter: pairs.iter(),
        value: None,
    };
    let value = v.visit_map(&mut access)?;
    if access.iter.len() == 0 {
        Ok(value)
    } else {
        Err(de::Error::invalid_length(
            pairs.len(),
            &"fewer entries in the mapping",
        ))
    }
}

/// Defines `deserialize_*` methods that call an inner method and give an
/// error the position of this node.
macro_rules! forward {
    ($($method:ident => $inner:ident),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
            let mark = self.0.mark;
            self.$inner(v).map_err(|e| e.at(mark))
        }
    )*};
}

/// Defines integer `deserialize_*` methods.
macro_rules! ints {
    ($($method:ident => $visit:ident),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
            let mark = self.0.mark;
            let n = self.int(&v).map_err(|e| e.at(mark))?;
            v.$visit::<Error>(n).map_err(|e| e.at(mark))
        }
    )*};
}

impl<'de> Deserializer<'de> for NodeDe<'de> {
    type Error = Error;

    forward! {
        deserialize_any => any,
        deserialize_bool => bool,
        deserialize_f32 => f32,
        deserialize_f64 => f64,
        deserialize_char => char,
        deserialize_str => str,
        deserialize_string => str,
        deserialize_identifier => str,
        deserialize_bytes => bytes,
        deserialize_byte_buf => bytes,
        deserialize_option => option,
        deserialize_unit => unit,
        deserialize_seq => seq,
        deserialize_map => map,
    }

    ints! {
        deserialize_i8 => visit_i8,
        deserialize_i16 => visit_i16,
        deserialize_i32 => visit_i32,
        deserialize_i64 => visit_i64,
        deserialize_i128 => visit_i128,
        deserialize_u8 => visit_u8,
        deserialize_u16 => visit_u16,
        deserialize_u32 => visit_u32,
        deserialize_u64 => visit_u64,
        deserialize_u128 => visit_u128,
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        v: V,
    ) -> Result<V::Value> {
        self.deserialize_unit(v)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        v: V,
    ) -> Result<V::Value> {
        let mark = self.0.mark;
        v.visit_newtype_struct(self).map_err(|e| e.at(mark))
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, v: V) -> Result<V::Value> {
        self.deserialize_seq(v)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        v: V,
    ) -> Result<V::Value> {
        self.deserialize_seq(v)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        v: V,
    ) -> Result<V::Value> {
        self.deserialize_map(v)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        v: V,
    ) -> Result<V::Value> {
        let mark = self.0.mark;
        self.enumeration(v).map_err(|e| e.at(mark))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        v.visit_unit()
    }
}

struct SeqDe<'de>(std::slice::Iter<'de, Node>);

impl<'de> SeqAccess<'de> for SeqDe<'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        self.0
            .next()
            .map(|n| seed.deserialize(NodeDe(n)))
            .transpose()
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

struct MapDe<'de> {
    iter: std::slice::Iter<'de, (Node, Node)>,
    value: Option<&'de Node>,
}

impl<'de> MapAccess<'de> for MapDe<'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        let Some((key, value)) = self.iter.next() else {
            return Ok(None);
        };
        self.value = Some(value);
        seed.deserialize(NodeDe(key)).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        let value = self
            .value
            .take()
            .ok_or_else(|| <Error as de::Error>::custom("a map value was read before its key"))?;
        seed.deserialize(NodeDe(value))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct EnumDe<'de> {
    variant: &'de Node,
    value: Option<&'de Node>,
}

impl<'de> EnumAccess<'de> for EnumDe<'de> {
    type Error = Error;
    type Variant = VariantDe<'de>;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, VariantDe<'de>)> {
        let name = seed.deserialize(NodeDe(self.variant))?;
        Ok((name, VariantDe(self.value)))
    }
}

struct VariantDe<'de>(Option<&'de Node>);

impl<'de> VariantDe<'de> {
    fn value(self, exp: &dyn Expected) -> Result<NodeDe<'de>> {
        self.0
            .map(NodeDe)
            .ok_or_else(|| de::Error::invalid_type(Unexpected::UnitVariant, exp))
    }
}

impl<'de> VariantAccess<'de> for VariantDe<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        match self.0.map(NodeDe) {
            Some(n) if !n.is_null() => n.invalid(&"a unit variant"),
            _ => Ok(()),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(self.value(&"a newtype variant")?)
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, v: V) -> Result<V::Value> {
        self.value(&"a tuple variant")?.deserialize_seq(v)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        v: V,
    ) -> Result<V::Value> {
        self.value(&"a struct variant")?.deserialize_map(v)
    }
}
