//! Scalars: the YAML 1.2 core schema, and the quoting rules of the writer.

use std::fmt::Write as _;

/// The type of a scalar after resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Kind {
    Null,
    Bool(bool),
    /// The text is an integer. Parse it with [`parse_int`] or [`parse_uint`].
    Int,
    /// The text is a float. Parse it with [`parse_float`] or [`parse_f32`].
    Float,
    Str,
}

/// Resolves a plain (unquoted, untagged) scalar with the core schema.
pub(super) fn resolve_plain(text: &str) -> Kind {
    if is_null(text) {
        Kind::Null
    } else if let Some(b) = parse_bool(text) {
        Kind::Bool(b)
    } else if is_int(text) {
        Kind::Int
    } else if is_float(text) {
        Kind::Float
    } else {
        Kind::Str
    }
}

pub(super) fn is_null(text: &str) -> bool {
    matches!(text, "" | "~" | "null" | "Null" | "NULL")
}

pub(super) fn parse_bool(text: &str) -> Option<bool> {
    match text {
        "true" | "True" | "TRUE" => Some(true),
        "false" | "False" | "FALSE" => Some(false),
        _ => None,
    }
}

fn digits(text: &str, radix: u32) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_digit(radix))
}

/// The digits and radix of an integer, or `None`. Hex and octal have no
/// sign in the core schema.
fn int_parts(text: &str) -> Option<(&str, u32)> {
    if let Some(hex) = text.strip_prefix("0x") {
        return digits(hex, 16).then_some((hex, 16));
    }
    if let Some(oct) = text.strip_prefix("0o") {
        return digits(oct, 8).then_some((oct, 8));
    }
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
    digits(unsigned, 10).then_some((text, 10))
}

pub(super) fn is_int(text: &str) -> bool {
    int_parts(text).is_some()
}

/// The value of an integer, or `None` if it is not one or not in range.
pub(super) fn parse_int(text: &str) -> Option<i128> {
    let (body, radix) = int_parts(text)?;
    i128::from_str_radix(body, radix).ok()
}

/// The value of a non-negative integer above the `i128` range.
pub(super) fn parse_uint(text: &str) -> Option<u128> {
    let (body, radix) = int_parts(text)?;
    u128::from_str_radix(body.strip_prefix('+').unwrap_or(body), radix).ok()
}

/// `[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?`
fn float_grammar(text: &str) -> bool {
    let text = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (mantissa, exponent) = text
        .find(['e', 'E'])
        .map_or((text, None), |i| (&text[..i], Some(&text[i + 1..])));
    if let Some(exp) = exponent
        && !digits(exp.strip_prefix(['-', '+']).unwrap_or(exp), 10)
    {
        return false;
    }
    match mantissa.split_once('.') {
        Some(("", frac)) => digits(frac, 10),
        Some((whole, frac)) => digits(whole, 10) && (frac.is_empty() || digits(frac, 10)),
        None => digits(mantissa, 10),
    }
}

fn special_float(text: &str) -> Option<f64> {
    match text {
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => Some(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => Some(f64::NAN),
        _ => None,
    }
}

/// True for a float that is not an integer.
pub(super) fn is_float(text: &str) -> bool {
    special_float(text).is_some() || float_grammar(text)
}

/// An integer or a float as `f64`.
#[allow(clippy::cast_precision_loss)] // A YAML integer read as a float.
pub(super) fn parse_float(text: &str) -> Option<f64> {
    if let Some(f) = special_float(text) {
        return Some(f);
    }
    if let Some(i) = parse_int(text) {
        return Some(i as f64);
    }
    if let Some(u) = parse_uint(text) {
        return Some(u as f64);
    }
    if float_grammar(text) {
        return text.parse().ok();
    }
    None
}

/// An integer or a float as `f32`. The text is parsed as `f32`, so the
/// shortest `f32` text gives back the same bits.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // As `as f32`.
pub(super) fn parse_f32(text: &str) -> Option<f32> {
    if let Some(f) = special_float(text) {
        return Some(f as f32);
    }
    if let Some(i) = parse_int(text) {
        return Some(i as f32);
    }
    if let Some(u) = parse_uint(text) {
        return Some(u as f32);
    }
    if float_grammar(text) {
        return text.parse().ok();
    }
    None
}

/// YAML 1.1 booleans and nulls that 1.2 reads as strings. The writer quotes
/// them, so a 1.1 reader gets a string too.
fn yaml11_word(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "y" | "n" | "yes" | "no" | "on" | "off" | "true" | "false" | "null" | "~"
    )
}

/// The YAML printable set, without the byte order mark.
pub(super) const fn printable(c: char) -> bool {
    matches!(c,
        '\t' | '\n' | '\r' | ' '..='~' | '\u{85}' | '\u{a0}'..='\u{d7ff}' | '\u{e000}'..='\u{fefe}'
        | '\u{ff00}'..='\u{fffd}' | '\u{10000}'..)
}

/// Characters that a reader can take as a line break.
const fn line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// True if the writer can put `text` as a plain scalar in block context.
pub(super) fn plain_ok(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let second = chars.next();
    let numeric_start = first.is_ascii_digit()
        || (matches!(first, '+' | '-' | '.')
            && second.is_some_and(|c| c.is_ascii_digit() || c == '.'));
    if resolve_plain(text) != Kind::Str
        || yaml11_word(text)
        || numeric_start
        // `<<` is a merge key. YAML 1.1 gives `=` a special type.
        || matches!(text, "<<" | "=")
        || "-?:,[]{}#&*!|>'\"%@`".contains(first)
        || text.starts_with(' ')
        || text.ends_with([' ', '\t'])
    {
        return false;
    }
    let mut prev = ' ';
    for (i, c) in text.char_indices() {
        if !printable(c) || line_break(c) || c == '\t' {
            return false;
        }
        if c == ':' && matches!(text[i + 1..].chars().next(), None | Some(' ')) {
            return false;
        }
        if c == '#' && prev == ' ' {
            return false;
        }
        prev = c;
    }
    true
}

/// True if the writer can put `text` as a literal block scalar: it has a
/// line break, it has text that is not a line break, and it needs no escape.
pub(super) fn literal_ok(text: &str) -> bool {
    text.contains('\n')
        && text.chars().any(|c| c != '\n')
        && text
            .chars()
            .all(|c| printable(c) && (c == '\n' || !line_break(c)))
}

/// `text` as a double-quoted scalar, with escapes for the characters that
/// are not printable or that a reader can take as a line break.
pub(super) fn double_quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{b}' => out.push_str("\\v"),
            '\u{c}' => out.push_str("\\f"),
            '\u{1b}' => out.push_str("\\e"),
            '\u{85}' => out.push_str("\\N"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            c if printable(c) => out.push(c),
            c if u32::from(c) <= 0xff => {
                let _ = write!(out, "\\x{:02X}", u32::from(c));
            }
            c if u32::from(c) <= 0xffff => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => {
                let _ = write!(out, "\\U{:08X}", u32::from(c));
            }
        }
    }
    out.push('"');
    out
}

/// `text` as a plain scalar if that is safe, else double-quoted.
pub(super) fn inline(text: &str) -> String {
    if plain_ok(text) {
        text.to_string()
    } else {
        double_quoted(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::cast_precision_loss)] // The expected values are casts.
    fn grammar() {
        for t in ["0", "-1", "+1", "0x1f", "0o7", "007"] {
            assert!(is_int(t), "{t}");
        }
        for t in ["", "-", "0x", "-0x1", "0o8", "1_0", "0b1"] {
            assert!(!is_int(t), "{t}");
        }
        for t in [
            "1.", ".5", "-1.5e3", "1e5", "1E+5", "+.5", ".inf", "-.INF", ".NaN",
        ] {
            assert!(is_float(t), "{t}");
        }
        for t in [".", "e5", "1e", "1.5.5", "inf", "nan", ".nAn", "1e+", "+"] {
            assert!(!is_float(t), "{t}");
        }
        assert_eq!(parse_int("0x10"), Some(16));
        assert_eq!(parse_uint("+0o10"), None);
        assert_eq!(parse_uint("+10"), Some(10));
        assert_eq!(parse_float("x"), None);
        assert_eq!(parse_f32("x"), None);
        assert_eq!(parse_float("0x10"), Some(16.0));
        assert_eq!(parse_f32("-.inf"), Some(f32::NEG_INFINITY));
        assert_eq!(parse_f32(&u128::MAX.to_string()), Some(u128::MAX as f32));
        assert_eq!(parse_float(&u128::MAX.to_string()), Some(u128::MAX as f64));
    }

    #[test]
    fn escapes() {
        assert_eq!(
            double_quoted("\u{1}\u{feff}\u{10ffff}\u{fffe}"),
            "\"\\x01\\uFEFF\u{10ffff}\\uFFFE\""
        );
        assert_eq!(inline("a b"), "a b");
        assert_eq!(inline("a: b"), "\"a: b\"");
    }
}
