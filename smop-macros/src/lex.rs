//! Splits a `cmd!` template into words.
//!
//! `scan` applies the character rules: word separators, quotes and braces.
//! This module makes the strings and the error messages. The validator
//! applies the rules that need a whole word.

use crate::scan::{Atom, Fault, Kind, RawWord, is_false_separator, is_separator, scan};

/// One piece of a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// Literal text.
    Text(String),
    /// `{name}`: one value.
    Var(String),
    /// `{name...}`: a list of values.
    Splat(String),
}

/// One word of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// The pieces before the quoted section.
    pub bare: Vec<Piece>,
    /// The pieces of the quoted section. `Some(vec![])` is `''`.
    pub quoted: Option<Vec<Piece>>,
    /// The template text of the word, for error messages.
    pub source: String,
}

/// The hint for a single quote in the wrong position.
const APOSTROPHE: &str = "; for an apostrophe, quote with \"...\"";

/// Splits a template into words.
///
/// `scan` finds the words and their atoms. This function makes the
/// strings and the error messages.
pub fn lex(template: &str) -> Result<Vec<Word>, String> {
    if template.contains('\0') {
        return Err("an argument cannot hold a NUL character".to_owned());
    }
    let t: Vec<char> = template.chars().collect();
    let words = scan(&t).map_err(|fault| message(&t, &fault))?;
    Ok(words.into_iter().map(|w| word(&t, w)).collect())
}

/// Makes a `Word` from the positions in `w`.
fn word(t: &[char], w: RawWord) -> Word {
    Word {
        bare: pieces(t, w.bare),
        quoted: w.quoted.map(|atoms| pieces(t, atoms)),
        source: t[w.start..w.end].iter().collect(),
    }
}

/// Makes pieces from atoms. Characters in sequence become one piece.
fn pieces(t: &[char], atoms: Vec<Atom>) -> Vec<Piece> {
    let mut out = Vec::new();
    for atom in atoms {
        let piece = match atom.kind {
            Kind::Char(c) => {
                if let Some(Piece::Text(text)) = out.last_mut() {
                    text.push(c);
                    continue;
                }
                Piece::Text(c.to_string())
            }
            Kind::Var => Piece::Var(t[atom.start + 1..atom.end - 1].iter().collect()),
            Kind::Splat => Piece::Splat(t[atom.start + 1..atom.end - 4].iter().collect()),
        };
        out.push(piece);
    }
    out
}

/// Makes the error message for `fault`.
fn message(t: &[char], fault: &Fault) -> String {
    let text = |from: usize, to: usize| t[from..to].iter().collect::<String>();
    // The word from `start` to the first separator at or after `at`.
    let shown = |start: usize, at: usize| {
        let end = t[at..]
            .iter()
            .position(|&c| is_separator(c))
            .map_or(t.len(), |n| at + n);
        text(start, end)
    };
    match *fault {
        Fault::FalseSeparator { start, at } => false_separator(&shown(start, at), t[at]),
        Fault::QuoteInWord { start, at } => format!(
            "`{}`: a quote must start a word or follow `=`{}",
            shown(start, at),
            hint(t[at])
        ),
        Fault::QuoteNotLast { start, open, at } => format!(
            "`{}`: a closing quote must end the word{}",
            shown(start, at),
            hint(t[open])
        ),
        Fault::Unclosed { start, open } => format!(
            "`{}`: unclosed quote {}{}",
            text(start, t.len()),
            t[open],
            hint(t[open])
        ),
        Fault::LoneOpen => "`{` has no `}`: write `{{` for a literal brace".to_owned(),
        Fault::LoneClose => "`}` has no `{`: write `}}` for a literal brace".to_owned(),
        Fault::BadName { start, end } => format!(
            "`{{{}}}` is not a placeholder: write `{{name}}` or `{{name...}}`, \
             or `{{{{` and `}}}}` for literal braces",
            text(start, end)
        ),
    }
}

/// Makes the error for the false separator `c` in the word `shown`.
///
/// The message shows each false separator as an escape, because the
/// character itself is not visible.
fn false_separator(shown: &str, c: char) -> String {
    let visible: String = shown
        .chars()
        .map(|c| {
            if is_false_separator(c) {
                c.escape_unicode().to_string()
            } else {
                c.to_string()
            }
        })
        .collect();
    format!(
        "`{visible}` has U+{:04X}, which is not ASCII whitespace: use a space, or quote it",
        u32::from(c)
    )
}

/// Returns the apostrophe hint for a single quote.
const fn hint(quote: char) -> &'static str {
    if quote == '\'' { APOSTROPHE } else { "" }
}

#[cfg(test)]
mod tests {
    use super::{Piece, Word, lex};
    use proptest::prelude::*;

    fn text(value: &str) -> Piece {
        Piece::Text(value.to_owned())
    }

    fn var(name: &str) -> Piece {
        Piece::Var(name.to_owned())
    }

    fn splat(name: &str) -> Piece {
        Piece::Splat(name.to_owned())
    }

    fn bare(source: &str, pieces: &[Piece]) -> Word {
        Word {
            bare: pieces.to_vec(),
            quoted: None,
            source: source.to_owned(),
        }
    }

    fn quoted(source: &str, bare: &[Piece], quoted: &[Piece]) -> Word {
        Word {
            bare: bare.to_vec(),
            quoted: Some(quoted.to_vec()),
            source: source.to_owned(),
        }
    }

    fn ok(template: &str) -> Vec<Word> {
        lex(template).unwrap_or_else(|e| panic!("{template}: {e}"))
    }

    fn err(template: &str) -> String {
        lex(template).expect_err(template)
    }

    #[test]
    fn ascii_whitespace_separates_words() {
        let words = ok("cargo test\n  --workspace\t-q\r\n\x0Cend ");
        let sources: Vec<&str> = words.iter().map(|w| w.source.as_str()).collect();
        assert_eq!(sources, ["cargo", "test", "--workspace", "-q", "end"]);
        assert_eq!(words[0], bare("cargo", &[text("cargo")]));
    }

    #[test]
    fn a_blank_template_has_no_words() {
        assert_eq!(ok(""), []);
        assert_eq!(ok(" \t\n\r\x0C"), []);
    }

    #[test]
    fn other_whitespace_is_an_error() {
        for c in [
            '\x0B', '\u{85}', '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}', '\u{2028}', '\u{2029}',
            '\u{202f}', '\u{205f}', '\u{3000}', '\u{200b}', '\u{feff}',
        ] {
            let message = err(&format!("ls{c}-la"));
            assert!(
                message.contains(&format!("U+{:04X}", u32::from(c))),
                "{message}"
            );
            assert!(message.contains("ASCII"), "{message}");
        }
    }

    #[test]
    fn other_whitespace_in_quotes_is_text() {
        assert_eq!(
            ok("'a\u{a0}b'"),
            [quoted("'a\u{a0}b'", &[], &[text("a\u{a0}b")])]
        );
    }

    #[test]
    fn a_nul_character_is_an_error() {
        assert!(err("ls a\0b").contains("NUL"));
        assert!(err("ls 'a\0b'").contains("NUL"));
    }

    #[test]
    fn a_placeholder_is_one_piece() {
        assert_eq!(ok("{cargo}"), [bare("{cargo}", &[var("cargo")])]);
        assert_eq!(
            ok("--out={dir}"),
            [bare("--out={dir}", &[text("--out="), var("dir")])]
        );
        assert_eq!(
            ok("{user}@{host}:{path}"),
            [bare(
                "{user}@{host}:{path}",
                &[var("user"), text("@"), var("host"), text(":"), var("path")]
            )]
        );
        assert_eq!(ok("{a}{b}"), [bare("{a}{b}", &[var("a"), var("b")])]);
        assert_eq!(ok("{_x1}"), [bare("{_x1}", &[var("_x1")])]);
    }

    #[test]
    fn a_splat_is_one_piece() {
        assert_eq!(ok("{files...}"), [bare("{files...}", &[splat("files")])]);
        assert_eq!(
            ok("--x={xs...}"),
            [bare("--x={xs...}", &[text("--x="), splat("xs")])]
        );
    }

    #[test]
    fn doubled_braces_are_literal() {
        assert_eq!(ok("{{}}"), [bare("{{}}", &[text("{}")])]);
        assert_eq!(
            ok("{{{x}}}"),
            [bare("{{{x}}}", &[text("{"), var("x"), text("}")])]
        );
        assert_eq!(ok("}}{{"), [bare("}}{{", &[text("}{")])]);
        assert_eq!(ok("a{{b}}c"), [bare("a{{b}}c", &[text("a{b}c")])]);
    }

    // The templates hold format syntax on purpose: `cmd!` reserves it.
    #[test]
    #[allow(clippy::literal_string_with_formatting_args)]
    fn reserved_placeholders_are_errors() {
        for (template, shown) in [
            ("echo {}", "`{}`"),
            ("echo {0}", "`{0}`"),
            ("echo {x:?}", "`{x:?}`"),
            ("echo {a.b}", "`{a.b}`"),
            ("echo { x }", "`{ x }`"),
            ("echo {_}", "`{_}`"),
            ("echo {x..}", "`{x..}`"),
            ("echo {x....}", "`{x....}`"),
            ("echo {...}", "`{...}`"),
            ("echo {r#fn}", "`{r#fn}`"),
            ("echo {caf\u{e9}}", "`{caf\u{e9}}`"),
            ("echo {a{b}", "`{a{b}`"),
        ] {
            let message = err(template);
            assert!(message.contains(shown), "{template}: {message}");
            assert!(message.contains("{name}"), "{template}: {message}");
        }
    }

    #[test]
    fn unmatched_braces_are_errors() {
        assert!(err("echo {x").contains("`{`"));
        assert!(err("echo {").contains("`{`"));
        assert!(err("echo x}").contains("`}`"));
        assert!(err("echo '}'").contains("`}`"));
        assert!(err("echo }}}").contains("`}`"));
    }

    #[test]
    fn quotes_group_words() {
        assert_eq!(
            ok("-m 'Release {v}'"),
            [
                bare("-m", &[text("-m")]),
                quoted("'Release {v}'", &[], &[text("Release "), var("v")]),
            ]
        );
        assert_eq!(ok("\"a  b\""), [quoted("\"a  b\"", &[], &[text("a  b")])]);
    }

    #[test]
    fn an_empty_quote_pair_is_an_empty_word() {
        assert_eq!(ok("''"), [quoted("''", &[], &[])]);
        assert_eq!(ok("\"\""), [quoted("\"\"", &[], &[])]);
        assert_eq!(ok("--x=''"), [quoted("--x=''", &[text("--x=")], &[])]);
    }

    #[test]
    fn a_quote_has_no_backslash_escapes() {
        assert_eq!(ok(r"'a\nb\'"), [quoted(r"'a\nb\'", &[], &[text(r"a\nb\")])]);
        assert_eq!(ok(r#""a\""#), [quoted(r#""a\""#, &[], &[text(r"a\")])]);
    }

    #[test]
    fn the_other_quote_character_is_text_in_a_quote() {
        assert_eq!(
            ok("\"don't\""),
            [quoted("\"don't\"", &[], &[text("don't")])]
        );
        assert_eq!(
            ok("'say \"hi\"'"),
            [quoted("'say \"hi\"'", &[], &[text("say \"hi\"")])]
        );
    }

    #[test]
    fn braces_work_in_quotes() {
        assert_eq!(
            ok("'{{}} {x} }}{{'"),
            [quoted(
                "'{{}} {x} }}{{'",
                &[],
                &[text("{} "), var("x"), text(" }{")]
            )]
        );
        assert_eq!(ok("'{xs...}'"), [quoted("'{xs...}'", &[], &[splat("xs")])]);
    }

    #[test]
    fn shell_characters_in_quotes_are_text() {
        assert_eq!(ok("';'"), [quoted("';'", &[], &[text(";")])]);
        assert_eq!(
            ok("'a && b | c > d $HOME ~ #'"),
            [quoted(
                "'a && b | c > d $HOME ~ #'",
                &[],
                &[text("a && b | c > d $HOME ~ #")]
            )]
        );
    }

    #[test]
    fn a_quote_can_follow_an_equals_sign() {
        assert_eq!(
            ok("--msg='a b'"),
            [quoted("--msg='a b'", &[text("--msg=")], &[text("a b")])]
        );
        assert_eq!(
            ok("a=b=\"c d\""),
            [quoted("a=b=\"c d\"", &[text("a=b=")], &[text("c d")])]
        );
    }

    #[test]
    fn a_quote_in_the_middle_of_a_word_is_an_error() {
        let message = err("echo don't");
        assert!(message.contains("`don't`"), "{message}");
        assert!(message.contains("start a word or follow `=`"), "{message}");
        assert!(
            message.contains("for an apostrophe, quote with \"...\""),
            "{message}"
        );

        let message = err("echo a\"b\"");
        assert!(message.contains("start a word or follow `=`"), "{message}");
        assert!(!message.contains("apostrophe"), "{message}");

        assert!(err("echo {x}'a'").contains("start a word or follow `=`"));
    }

    #[test]
    fn text_after_a_closing_quote_is_an_error() {
        let message = err("echo 'don't'");
        assert!(message.contains("`'don't'`"), "{message}");
        assert!(message.contains("must end the word"), "{message}");
        assert!(
            message.contains("for an apostrophe, quote with \"...\""),
            "{message}"
        );

        assert!(err("echo 'a''b'").contains("must end the word"));
        assert!(err("echo 'a'{x}").contains("must end the word"));
        let message = err("echo \"a\"b");
        assert!(message.contains("must end the word"), "{message}");
        assert!(!message.contains("apostrophe"), "{message}");
    }

    #[test]
    fn a_closing_quote_error_shows_the_word_to_its_end() {
        let message = err("git commit -m 'Release it's done'");
        assert!(message.contains("`'Release it's`"), "{message}");
        let message = err("echo --m='two words'x y");
        assert!(message.contains("`--m='two words'x`"), "{message}");
    }

    #[test]
    fn other_whitespace_after_a_closing_quote_is_a_whitespace_error() {
        for template in ["echo 'a'\u{a0}b", "echo \"a b\"\u{a0}"] {
            let message = err(template);
            assert!(message.contains("U+00A0"), "{message}");
            assert!(message.contains("ASCII"), "{message}");
            assert!(!message.contains("closing quote"), "{message}");
            assert!(!message.contains('\u{a0}'), "{message}");
        }
    }

    #[test]
    fn a_whitespace_error_escapes_only_the_bad_character() {
        let message = err("echo C:\\caf\u{e9}\u{a0}'a'");
        assert!(message.contains("`C:\\caf\u{e9}\\u{a0}'a'`"), "{message}");
    }

    // Each message in full, one for each fault of the scanner.
    #[test]
    #[allow(clippy::literal_string_with_formatting_args)]
    fn each_message_is_exact() {
        const QUOTE: &str = "a quote must start a word or follow `=`";
        const APOSTROPHE: &str = "; for an apostrophe, quote with \"...\"";
        const PLACEHOLDER: &str = "is not a placeholder: write `{name}` or `{name...}`, or `{{` and `}}` for literal braces";
        for (template, message) in [
            (
                "echo a\u{a0}b",
                r"`a\u{a0}b` has U+00A0, which is not ASCII whitespace: use a space, or quote it"
                    .to_owned(),
            ),
            (
                "echo 'a b'\u{a0}",
                r"`'a b'\u{a0}` has U+00A0, which is not ASCII whitespace: use a space, or quote it"
                    .to_owned(),
            ),
            ("echo don't", format!("`don't`: {QUOTE}{APOSTROPHE}")),
            (
                "echo 'a b'c d",
                format!("`'a b'c`: a closing quote must end the word{APOSTROPHE}"),
            ),
            (
                "echo \"a b\"c",
                "`\"a b\"c`: a closing quote must end the word".to_owned(),
            ),
            (
                "echo 'abc def  ",
                format!("`'abc def  `: unclosed quote '{APOSTROPHE}"),
            ),
            (
                "echo {x",
                "`{` has no `}`: write `{{` for a literal brace".to_owned(),
            ),
            (
                "echo x}",
                "`}` has no `{`: write `}}` for a literal brace".to_owned(),
            ),
            ("echo {a b}", format!("`{{a b}}` {PLACEHOLDER}")),
            ("echo {x....}", format!("`{{x....}}` {PLACEHOLDER}")),
        ] {
            assert_eq!(err(template), message, "{template}");
        }
    }

    #[test]
    fn an_unclosed_quote_is_an_error() {
        let message = err("echo 'abc def");
        assert!(message.contains("unclosed"), "{message}");
        assert!(message.contains("`'abc def`"), "{message}");
        assert!(
            message.contains("for an apostrophe, quote with \"...\""),
            "{message}"
        );

        let message = err("echo \"abc");
        assert!(message.contains("unclosed"), "{message}");
        assert!(!message.contains("apostrophe"), "{message}");
    }

    #[test]
    fn each_whitespace_character_is_a_separator_or_an_error() {
        let separators = [' ', '\t', '\n', '\r', '\x0C'];
        let mut refused = 0;
        for c in (0..=u32::from(char::MAX)).filter_map(char::from_u32) {
            if !c.is_whitespace() && !matches!(c, '\u{200b}' | '\u{feff}') {
                continue;
            }
            let bare = lex(&format!("a{c}b"));
            if separators.contains(&c) {
                assert_eq!(
                    bare.map(|words| words.len()),
                    Ok(2),
                    "U+{:04X}",
                    u32::from(c)
                );
            } else {
                refused += 1;
                assert!(bare.is_err(), "U+{:04X}", u32::from(c));
                let in_quotes = format!("'a{c}b'");
                assert_eq!(
                    lex(&in_quotes),
                    Ok(vec![quoted(&in_quotes, &[], &[text(&format!("a{c}b"))])])
                );
            }
        }
        // Unicode has 25 whitespace characters. Five are separators.
        assert_eq!(refused, 25 - separators.len() + 2);
    }

    /// Text that is one bare word with no special character.
    fn plain() -> impl Strategy<Value = String> {
        "[A-Za-z0-9_./:=@%+,*?!^\\[\\]-]{1,12}"
    }

    proptest! {
        #[test]
        fn no_input_causes_a_panic(template in any::<String>()) {
            let _ = lex(&template);
        }

        #[test]
        fn an_error_is_never_empty(template in any::<String>()) {
            if let Err(message) = lex(&template) {
                prop_assert!(!message.is_empty());
            }
        }

        #[test]
        fn plain_words_keep_their_count_and_text(
            words in prop::collection::vec(plain(), 0..8),
            separator in "[ \t\n\r\x0C]{1,3}",
        ) {
            let lexed = lex(&words.join(&separator)).map_err(TestCaseError::fail)?;
            let expected: Vec<Word> = words.iter().map(|w| bare(w, &[text(w)])).collect();
            prop_assert_eq!(lexed, expected);
        }

        #[test]
        fn a_character_with_no_special_meaning_is_text(c in any::<char>()) {
            let special = c.is_whitespace()
                || matches!(c, '\'' | '"' | '{' | '}' | '\0' | '\u{200b}' | '\u{feff}');
            prop_assume!(!special);
            let source = format!("a{c}b");
            let lexed = lex(&source).map_err(TestCaseError::fail)?;
            prop_assert_eq!(lexed, [bare(&source, &[text(&source)])]);
        }

        #[test]
        fn a_quote_keeps_each_character(content in "[^'{}\0]{0,24}") {
            let source = format!("'{content}'");
            let lexed = lex(&source).map_err(TestCaseError::fail)?;
            let pieces = if content.is_empty() { vec![] } else { vec![text(&content)] };
            prop_assert_eq!(lexed, [quoted(&source, &[], &pieces)]);
        }

        #[test]
        fn text_never_splits_a_placeholder_word(before in plain(), after in plain()) {
            let source = format!("{before}{{a}}{after}{{b}}");
            let lexed = lex(&source).map_err(TestCaseError::fail)?;
            let expected = bare(&source, &[text(&before), var("a"), text(&after), var("b")]);
            prop_assert_eq!(lexed, [expected]);
        }
    }
}
