//! Splits a `cmd!` template into words.
//!
//! The lexer applies the character rules: word separators, quotes and
//! braces. The validator applies the rules that need a whole word.

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
pub fn lex(template: &str) -> Result<Vec<Word>, String> {
    if template.contains('\0') {
        return Err("an argument cannot contain a NUL character".to_owned());
    }
    let mut words = Vec::new();
    let mut rest = template.trim_start_matches(is_separator);
    while !rest.is_empty() {
        let (word, after) = lex_word(rest)?;
        words.push(word);
        rest = after.trim_start_matches(is_separator);
    }
    Ok(words)
}

/// Returns `true` for the ASCII whitespace that separates words.
const fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

/// Returns `true` for a character that looks like a separator but is not
/// one: other whitespace, and the invisible U+200B and U+FEFF.
const fn is_false_separator(c: char) -> bool {
    (c.is_whitespace() && !is_separator(c)) || matches!(c, '\u{200b}' | '\u{feff}')
}

/// Lexes the word at the start of `input`. Returns it and the text after it.
fn lex_word(input: &str) -> Result<(Word, &str), String> {
    // The word as far as the next separator. A quoted section can go past it.
    let shown = input.split(is_separator).next().unwrap_or(input);
    let mut bare = Pieces::default();
    let mut rest = input;
    let mut previous = None;

    while let Some(c) = rest.chars().next().filter(|c| !is_separator(*c)) {
        match c {
            '\'' | '"' if bare.is_empty() || previous == Some('=') => {
                let (quoted, after) = lex_quoted(input, rest, c)?;
                let source = &input[..input.len() - after.len()];
                if after.chars().next().is_some_and(|next| !is_separator(next)) {
                    return Err(format!(
                        "`{shown}`: a closing quote must end the word{}",
                        hint(c)
                    ));
                }
                let word = Word {
                    bare: bare.finish(),
                    quoted: Some(quoted),
                    source: source.to_owned(),
                };
                return Ok((word, after));
            }
            '\'' | '"' => {
                return Err(format!(
                    "`{shown}`: a quote must start a word or follow `=`{}",
                    hint(c)
                ));
            }
            '{' | '}' => rest = lex_brace(rest, &mut bare)?,
            c if is_false_separator(c) => {
                return Err(format!(
                    "`{}` has U+{:04X}, which is not ASCII whitespace: use a space, or quote it",
                    shown.escape_default(),
                    u32::from(c)
                ));
            }
            c => {
                bare.text(c);
                rest = &rest[c.len_utf8()..];
            }
        }
        previous = Some(c);
    }

    let source = &input[..input.len() - rest.len()];
    let word = Word {
        bare: bare.finish(),
        quoted: None,
        source: source.to_owned(),
    };
    Ok((word, rest))
}

/// Lexes a quoted section. `rest` starts at the opening quote. Returns the
/// pieces and the text after the closing quote.
fn lex_quoted<'a>(
    word: &'a str,
    rest: &'a str,
    quote: char,
) -> Result<(Vec<Piece>, &'a str), String> {
    let mut pieces = Pieces::default();
    let mut rest = &rest[quote.len_utf8()..];
    loop {
        match rest.chars().next() {
            None => {
                return Err(format!("`{word}`: unclosed quote {quote}{}", hint(quote)));
            }
            Some(c) if c == quote => return Ok((pieces.finish(), &rest[c.len_utf8()..])),
            Some('{' | '}') => rest = lex_brace(rest, &mut pieces)?,
            Some(c) => {
                pieces.text(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
}

/// Lexes a brace: `{{`, `}}`, `{name}` or `{name...}`. `rest` starts at
/// the brace. Returns the text after it.
fn lex_brace<'a>(rest: &'a str, pieces: &mut Pieces) -> Result<&'a str, String> {
    if let Some(after) = rest.strip_prefix("{{") {
        pieces.text('{');
        return Ok(after);
    }
    if let Some(after) = rest.strip_prefix("}}") {
        pieces.text('}');
        return Ok(after);
    }
    if rest.starts_with('}') {
        return Err("`}` has no `{`: write `}}` for a literal brace".to_owned());
    }
    let Some(end) = rest.find('}') else {
        return Err("`{` has no `}`: write `{{` for a literal brace".to_owned());
    };
    let inner = &rest[1..end];
    let (name, splat) = inner
        .strip_suffix("...")
        .map_or((inner, false), |name| (name, true));
    if !is_name(name) {
        return Err(format!(
            "`{{{inner}}}` is not a placeholder: write `{{name}}` or `{{name...}}`, \
             or `{{{{` and `}}}}` for literal braces"
        ));
    }
    pieces.push(if splat {
        Piece::Splat(name.to_owned())
    } else {
        Piece::Var(name.to_owned())
    });
    Ok(&rest[end + 1..])
}

/// Returns `true` for an ASCII identifier that is not `_`.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name != "_"
}

/// Returns the apostrophe hint for a single quote.
const fn hint(quote: char) -> &'static str {
    if quote == '\'' { APOSTROPHE } else { "" }
}

/// Collects pieces. Text characters in sequence become one piece.
#[derive(Default)]
struct Pieces {
    done: Vec<Piece>,
    text: String,
}

impl Pieces {
    const fn is_empty(&self) -> bool {
        self.done.is_empty() && self.text.is_empty()
    }

    fn text(&mut self, c: char) {
        self.text.push(c);
    }

    fn push(&mut self, piece: Piece) {
        self.flush();
        self.done.push(piece);
    }

    fn flush(&mut self) {
        if !self.text.is_empty() {
            self.done.push(Piece::Text(std::mem::take(&mut self.text)));
        }
    }

    fn finish(mut self) -> Vec<Piece> {
        self.flush();
        self.done
    }
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
