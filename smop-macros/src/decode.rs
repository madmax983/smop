//! Decodes the source text of a string literal.
//!
//! Stable `proc_macro` gives a literal only as source text. This module
//! turns that text into the string value.

/// Decodes the source text of a string literal into its value.
///
/// It accepts a string literal and a raw string literal. It refuses a byte
/// string, a C string, a literal with a suffix, and each other literal.
pub fn decode(source: &str) -> Result<String, String> {
    if source.starts_with('b') {
        return Err("the template is a byte string; remove the `b`".to_owned());
    }
    if source.starts_with('c') {
        return Err("the template is a C string; remove the `c`".to_owned());
    }
    let decoded = match source.chars().next() {
        Some('r') => raw(&source[1..]),
        Some('"') => cooked(&source[1..]),
        _ => None,
    };
    let (value, suffix) = decoded.ok_or("the template must be a string literal")?;
    if suffix.is_empty() {
        Ok(value)
    } else {
        Err(format!("the template has the literal suffix `{suffix}`"))
    }
}

/// Decodes a raw string after its `r`. Returns the value and the suffix.
fn raw(rest: &str) -> Option<(String, &str)> {
    let body = rest.trim_start_matches('#');
    let fence = &rest[..rest.len() - body.len()];
    let body = body.strip_prefix('"')?;
    let end = body.find(&format!("\"{fence}"))?;
    Some((body[..end].to_owned(), &body[end + 1 + fence.len()..]))
}

/// Decodes a string after its opening quote. Returns the value and the
/// suffix.
fn cooked(rest: &str) -> Option<(String, &str)> {
    let mut value = String::with_capacity(rest.len());
    let mut chars = rest.chars();
    loop {
        match chars.next()? {
            '"' => return Some((value, chars.as_str())),
            '\\' => match chars.next()? {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'r' => value.push('\r'),
                '0' => value.push('\0'),
                c @ ('\\' | '"' | '\'') => value.push(c),
                'x' => value.push(hex_escape(&mut chars)?),
                'u' => value.push(unicode_escape(&mut chars)?),
                '\n' => skip_continuation(&mut chars),
                _ => return None,
            },
            c => value.push(c),
        }
    }
}

/// Decodes the two digits of a `\x` escape. The largest value is `7F`.
fn hex_escape(chars: &mut std::str::Chars<'_>) -> Option<char> {
    let high = chars.next()?.to_digit(16)?;
    let low = chars.next()?.to_digit(16)?;
    char::from_u32(high * 16 + low).filter(char::is_ascii)
}

/// Decodes the `{...}` of a `\u` escape: one to six digits, `_` allowed.
fn unicode_escape(chars: &mut std::str::Chars<'_>) -> Option<char> {
    if chars.next()? != '{' {
        return None;
    }
    let mut code = 0_u32;
    let mut digits = 0;
    loop {
        match chars.next()? {
            '}' if digits > 0 => return char::from_u32(code),
            '_' if digits > 0 => {}
            c if digits < 6 => {
                code = code * 16 + c.to_digit(16)?;
                digits += 1;
            }
            _ => return None,
        }
    }
}

/// Skips the whitespace after a backslash at the end of a line.
fn skip_continuation(chars: &mut std::str::Chars<'_>) {
    let rest = chars.as_str();
    *chars = rest.trim_start_matches([' ', '\t', '\n', '\r']).chars();
}

#[cfg(test)]
mod tests {
    use super::decode;
    use proptest::prelude::*;

    fn ok(source: &str) -> String {
        decode(source).unwrap_or_else(|e| panic!("{source}: {e}"))
    }

    fn err(source: &str) -> String {
        decode(source).expect_err(source)
    }

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(ok(r#""git status""#), "git status");
        assert_eq!(ok(r#""""#), "");
        assert_eq!(ok("\"caf\u{e9} \u{1f600}\""), "caf\u{e9} \u{1f600}");
    }

    #[test]
    fn simple_escapes_decode() {
        assert_eq!(ok(r#""a\nb\tc\rd\\e\"f\'g\0h""#), "a\nb\tc\rd\\e\"f'g\0h");
    }

    #[test]
    fn hex_escapes_decode() {
        assert_eq!(ok(r#""\x41\x7f\x00""#), "A\x7f\0");
        assert_eq!(ok(r#""\x0C""#), "\x0C");
    }

    #[test]
    fn unicode_escapes_decode() {
        assert_eq!(ok(r#""\u{a0}""#), "\u{a0}");
        assert_eq!(ok(r#""\u{1F600}""#), "\u{1f600}");
        assert_eq!(ok(r#""\u{00_41}""#), "A");
        assert_eq!(ok(r#""\u{10FFFF}""#), "\u{10ffff}");
    }

    #[test]
    fn a_line_continuation_removes_the_newline_and_the_indent() {
        assert_eq!(ok("\"cargo \\\n    test\""), "cargo test");
        assert_eq!(ok("\"a\\\n\n \t\r\nb\""), "ab");
    }

    #[test]
    fn a_newline_in_the_literal_stays() {
        assert_eq!(
            ok("\"cargo test\n  --workspace\""),
            "cargo test\n  --workspace"
        );
    }

    #[test]
    fn raw_strings_keep_each_character() {
        assert_eq!(ok(r#"r"a\nb""#), "a\\nb");
        assert_eq!(ok("r#\"say \"hi\"\"#"), "say \"hi\"");
        assert_eq!(ok(r###"r##"a"#b"##"###), "a\"#b");
        assert_eq!(ok(r#"r"""#), "");
    }

    #[test]
    fn byte_strings_are_errors() {
        for source in [r#"b"ls""#, r#"br"ls""#, r##"br#"ls"#"##, "b'l'"] {
            assert!(err(source).contains("byte"), "{source}");
        }
    }

    #[test]
    fn c_strings_are_errors() {
        for source in [r#"c"ls""#, r#"cr"ls""#, r##"cr#"ls"#"##] {
            assert!(err(source).contains("C string"), "{source}");
        }
    }

    #[test]
    fn suffixed_literals_are_errors() {
        assert!(err(r#""ls"suffix"#).contains("suffix"));
        assert!(err(r##"r#"ls"#suffix"##).contains("suffix"));
    }

    #[test]
    fn other_literals_are_errors() {
        for source in ["42", "1.5", "'c'", "true", "", "-1", "1u8"] {
            assert!(err(source).contains("string literal"), "{source}");
        }
    }

    #[test]
    fn malformed_text_is_an_error_and_not_a_panic() {
        for source in [
            "\"abc",
            "\"abc\\",
            r#""\q""#,
            r#""\x4""#,
            r#""\xZZ""#,
            r#""\x80""#,
            r#""\u{110000}""#,
            r#""\u{D800}""#,
            r#""\u{}""#,
            r#""\u{1234567}""#,
            r#""\u{12""#,
            r#""\u41""#,
            "r",
            "r#",
            "r#\"abc\"",
            "r##\"abc\"#",
        ] {
            assert!(decode(source).is_err(), "{source}");
        }
    }

    proptest! {
        #[test]
        fn debug_text_round_trips(value in any::<String>()) {
            prop_assert_eq!(decode(&format!("{value:?}")), Ok(value));
        }

        #[test]
        fn escape_default_text_round_trips(value in any::<String>()) {
            let source = format!("\"{}\"", value.escape_default());
            prop_assert_eq!(decode(&source), Ok(value));
        }

        #[test]
        fn raw_text_round_trips(value in any::<String>(), hashes in 0_usize..4) {
            let fence = "#".repeat(hashes);
            prop_assume!(!value.contains(&format!("\"{fence}")));
            let source = format!("r{fence}\"{value}\"{fence}");
            prop_assert_eq!(decode(&source), Ok(value));
        }

        #[test]
        fn no_input_causes_a_panic(source in any::<String>()) {
            let _ = decode(&source);
            let _ = decode(&format!("\"{source}"));
            let _ = decode(&format!("r#\"{source}"));
        }
    }
}
