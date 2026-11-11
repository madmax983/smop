//! Applies the `cmd!` grammar rules that need a whole word.
//!
//! The input is the output of the lexer. The output is the template that
//! the macro expands.

use crate::lex::{Piece, Word};

/// One part of an argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Literal text.
    Text(String),
    /// The value of a name.
    Var(String),
}

/// One word after the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    /// Exactly one argument. No parts is an empty argument.
    Word(Vec<Part>),
    /// Zero or more arguments from the list of a name.
    Splat(String),
}

impl Arg {
    /// Gets the parts of a word. A splat has none.
    pub const fn word(&self) -> Option<&Vec<Part>> {
        match self {
            Self::Word(parts) => Some(parts),
            Self::Splat(_) => None,
        }
    }
}

/// A valid template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// The parts of the program word.
    pub program: Vec<Part>,
    /// The words after the program.
    pub args: Vec<Arg>,
}

/// The message for shell syntax outside quotes.
pub const NOT_A_SHELL: &str =
    "cmd! runs one program, not a shell: quote it, or use `smop::sh::shell`";

/// Checks the words of a template against the grammar.
///
/// `listed` holds the names of the `name = value` arguments. Each one must
/// be in the template.
pub fn validate(words: Vec<Word>, listed: &[&str]) -> Result<Template, String> {
    let mut used: Vec<String> = Vec::new();
    let mut args = Vec::with_capacity(words.len());
    for word in words {
        check_shell_syntax(&word)?;
        args.push(validate_word(word, &mut used)?);
    }

    let mut args = args.into_iter();
    let program = match args.next() {
        None => return Err("the template is empty: write a program name".to_owned()),
        Some(Arg::Splat(name)) => {
            return Err(format!(
                "`{{{name}...}}` cannot be the program: the program is one word"
            ));
        }
        Some(Arg::Word(parts)) if parts.is_empty() => {
            return Err("the program is an empty word: write a program name".to_owned());
        }
        Some(Arg::Word(parts)) => parts,
    };

    if let Some(name) = listed.iter().find(|name| !used.iter().any(|u| u == *name)) {
        return Err(format!(
            "`{name}` is listed, but the template has no `{{{name}}}`"
        ));
    }
    Ok(Template {
        program,
        args: args.collect(),
    })
}

/// Changes one lexed word into an argument. Adds its names to `used`.
fn validate_word(word: Word, used: &mut Vec<String>) -> Result<Arg, String> {
    let is_quoted = word.quoted.is_some();
    let pieces: Vec<Piece> = word
        .bare
        .into_iter()
        .chain(word.quoted.into_iter().flatten())
        .collect();
    let is_glued = pieces.len() > 1 || is_quoted;

    let mut parts: Vec<Part> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match piece {
            Piece::Text(text) => {
                if let Some(Part::Text(last)) = parts.last_mut() {
                    last.push_str(&text);
                } else {
                    parts.push(Part::Text(text));
                }
            }
            Piece::Var(name) => {
                check_name(&name, "")?;
                used.push(name.clone());
                parts.push(Part::Var(name));
            }
            Piece::Splat(name) if is_glued => {
                return Err(format!(
                    "`{{{name}...}}` in `{}`: a splat must be a whole word, \
                     not glued to text and not in quotes",
                    word.source
                ));
            }
            Piece::Splat(name) => {
                check_name(&name, "...")?;
                used.push(name.clone());
                return Ok(Arg::Splat(name));
            }
        }
    }
    Ok(Arg::Word(parts))
}

/// Keywords and reserved words of each edition. None can name a local.
/// `self` can be a value, so it is not in the list.
const KEYWORDS: &[&str] = &[
    "abstract", "as", "become", "box", "break", "const", "continue", "crate", "do", "else", "enum",
    "extern", "false", "final", "fn", "for", "if", "impl", "in", "let", "loop", "macro", "match",
    "mod", "move", "mut", "override", "priv", "pub", "ref", "return", "Self", "static", "struct",
    "super", "trait", "true", "type", "typeof", "unsafe", "unsized", "use", "virtual", "where",
    "while", "yield",
];

/// Refuses a keyword as a placeholder name. `dots` is `...` for a splat.
fn check_name(name: &str, dots: &str) -> Result<(), String> {
    if KEYWORDS.contains(&name) {
        return Err(format!(
            "`{{{name}{dots}}}`: `{name}` is a keyword, not the name of a value"
        ));
    }
    Ok(())
}

/// Refuses shell syntax in the unquoted part of a word.
fn check_shell_syntax(word: &Word) -> Result<(), String> {
    let mut at_start = true;
    let mut pieces = word.bare.iter().peekable();
    while let Some(piece) = pieces.next() {
        if let Piece::Text(text) = piece {
            let next_is_placeholder = pieces.peek().is_some();
            if is_shell_syntax(text, at_start, next_is_placeholder) {
                return Err(format!("`{}` has shell syntax. {NOT_A_SHELL}", word.source));
            }
        }
        at_start = false;
    }
    Ok(())
}

/// Returns `true` if unquoted `text` has shell syntax.
///
/// `at_start` says that the text starts its word. `before_placeholder`
/// says that a placeholder follows the text.
fn is_shell_syntax(text: &str, at_start: bool, before_placeholder: bool) -> bool {
    if at_start && text.starts_with(['~', '#']) {
        return true;
    }
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let found = match c {
            '|' | '&' | ';' | '<' | '>' | '`' => true,
            '$' => chars.peek().map_or(before_placeholder, |next| {
                next.is_ascii_alphanumeric() || matches!(next, '_' | '{' | '(')
            }),
            _ => false,
        };
        if found {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{Arg, NOT_A_SHELL, Part, Template, validate};
    use crate::lex::lex;
    use proptest::prelude::*;

    fn text(value: &str) -> Part {
        Part::Text(value.to_owned())
    }

    fn var(name: &str) -> Part {
        Part::Var(name.to_owned())
    }

    fn word(parts: &[Part]) -> Arg {
        Arg::Word(parts.to_vec())
    }

    fn splat(name: &str) -> Arg {
        Arg::Splat(name.to_owned())
    }

    fn parse(template: &str, listed: &[&str]) -> Result<Template, String> {
        validate(lex(template)?, listed)
    }

    fn ok(template: &str) -> Template {
        parse(template, &[]).unwrap_or_else(|e| panic!("{template}: {e}"))
    }

    fn err(template: &str) -> String {
        parse(template, &[]).expect_err(template)
    }

    #[test]
    fn words_become_a_program_and_arguments() {
        assert_eq!(
            ok("cargo test\n  --workspace"),
            Template {
                program: vec![text("cargo")],
                args: vec![word(&[text("test")]), word(&[text("--workspace")])],
            }
        );
    }

    #[test]
    fn a_blank_template_is_an_error() {
        assert!(err("").contains("empty"));
        assert!(err(" \n\t ").contains("empty"));
    }

    #[test]
    fn the_program_can_be_a_placeholder() {
        assert_eq!(
            ok("{cargo} fmt"),
            Template {
                program: vec![var("cargo")],
                args: vec![word(&[text("fmt")])],
            }
        );
        assert_eq!(ok("{dir}/tool").program, [var("dir"), text("/tool")]);
        assert_eq!(ok("'my tool' x").program, [text("my tool")]);
    }

    #[test]
    fn the_program_cannot_be_a_splat() {
        let message = err("{argv...}");
        assert!(message.contains("`{argv...}`"), "{message}");
        assert!(message.contains("program"), "{message}");
    }

    #[test]
    fn the_program_cannot_be_an_empty_word() {
        assert!(err("'' x").contains("program is an empty word"));
        assert!(err("\"\"").contains("program is an empty word"));
    }

    #[test]
    fn a_placeholder_can_be_glued_to_text() {
        assert_eq!(
            ok("x --out={dir} {user}@{host}:{path} -j{jobs} {a}{b}").args,
            [
                word(&[text("--out="), var("dir")]),
                word(&[var("user"), text("@"), var("host"), text(":"), var("path")]),
                word(&[text("-j"), var("jobs")]),
                word(&[var("a"), var("b")]),
            ]
        );
    }

    #[test]
    fn a_whole_word_splat_is_an_argument_list() {
        assert_eq!(
            ok("git add -- {files...} {more...}").args,
            [
                word(&[text("add")]),
                word(&[text("--")]),
                splat("files"),
                splat("more"),
            ]
        );
    }

    #[test]
    fn a_splat_must_be_a_whole_word() {
        for template in [
            "x --x={xs...}",
            "x {xs...}y",
            "x {a}{xs...}",
            "x {xs...}{ys...}",
            "x '{xs...}'",
            "x \"{xs...}\"",
            "x 'a {xs...}'",
            "x --x='{xs...}'",
        ] {
            let message = err(template);
            assert!(message.contains("`{xs...}`"), "{template}: {message}");
            assert!(message.contains("whole word"), "{template}: {message}");
        }
    }

    #[test]
    fn quoted_text_joins_the_text_before_it() {
        assert_eq!(
            ok("git commit -m 'Release {v}' --msg='a b' '' --x=''").args,
            [
                word(&[text("commit")]),
                word(&[text("-m")]),
                word(&[text("Release "), var("v")]),
                word(&[text("--msg=a b")]),
                word(&[]),
                word(&[text("--x=")]),
            ]
        );
    }

    #[test]
    fn literal_braces_are_text() {
        assert_eq!(
            ok("find . -exec rm {{}} ';'").args,
            [
                word(&[text(".")]),
                word(&[text("-exec")]),
                word(&[text("rm")]),
                word(&[text("{}")]),
                word(&[text(";")]),
            ]
        );
        assert_eq!(
            ok("x {{{v}}} }}{{").args,
            [word(&[text("{"), var("v"), text("}")]), word(&[text("}{")])]
        );
    }

    #[test]
    fn shell_operators_outside_quotes_are_errors() {
        for (template, shown) in [
            ("rm -f {f} && echo done", "`&&`"),
            ("make 2>/dev/null", "`2>/dev/null`"),
            ("ls | wc", "`|`"),
            ("a|b", "`a|b`"),
            ("sleep 1 &", "`&`"),
            ("a; b", "`a;`"),
            ("find . -exec rm {{}} ;", "`;`"),
            ("sort <in", "`<in`"),
            ("echo `id`", "`id`"),
            ("curl http://x/?a=1&b=2", "`http://x/?a=1&b=2`"),
            ("a>{out}", "`a>{out}`"),
            ("{a}|{b}", "`{a}|{b}`"),
            ("&& x", "`&&`"),
        ] {
            let message = err(template);
            assert!(message.contains(shown), "{template}: {message}");
            assert!(message.contains(NOT_A_SHELL), "{template}: {message}");
        }
    }

    #[test]
    fn shell_expansions_outside_quotes_are_errors() {
        for template in [
            "echo $HOME",
            "echo $1",
            "echo $_x",
            "echo ${{HOME}}",
            "echo ${home}",
            "echo $(id)",
            "echo a$b",
            "echo ~",
            "echo ~/x",
            "echo ~{user}",
            "echo #x",
            "echo #",
            "$tool x",
            "~/bin/tool x",
        ] {
            let message = err(template);
            assert!(message.contains(NOT_A_SHELL), "{template}: {message}");
        }
    }

    #[test]
    fn harmless_uses_of_shell_characters_are_text() {
        assert_eq!(
            ok("x a$ $ $$ $. a~b a#b --x=~/y *.rs ? [a] ! % ^ + , : @ = -").args,
            [
                "a$", "$", "$$", "$.", "a~b", "a#b", "--x=~/y", "*.rs", "?", "[a]", "!", "%", "^",
                "+", ",", ":", "@", "=", "-",
            ]
            .map(|t| word(&[text(t)]))
        );
    }

    #[test]
    fn quoted_shell_syntax_is_text() {
        assert_eq!(
            ok("x '&&' \"$HOME\" '~' '#' --a='b;c' '`id`' '2>/dev/null'").args,
            ["&&", "$HOME", "~", "#", "--a=b;c", "`id`", "2>/dev/null"].map(|t| word(&[text(t)]))
        );
    }

    #[test]
    fn a_keyword_is_not_a_name() {
        for name in ["fn", "let", "true", "crate", "Self", "match"] {
            let message = err(&format!("echo {{{name}}}"));
            assert!(message.contains("keyword"), "{name}: {message}");
            let message = err(&format!("echo {{{name}...}}"));
            assert!(message.contains("keyword"), "{name}: {message}");
        }
        assert_eq!(ok("echo {self}").args, [word(&[var("self")])]);
        assert_eq!(ok("echo {gen} {dyn}").args.len(), 2);
    }

    #[test]
    fn a_reserved_word_of_each_edition_is_not_a_name() {
        for name in [
            "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof",
            "unsized", "virtual", "yield",
        ] {
            let message = err(&format!("echo {{{name}}}"));
            assert!(message.contains("keyword"), "{name}: {message}");
        }
    }

    #[test]
    fn a_keyword_error_shows_the_placeholder_as_written() {
        assert!(err("echo {fn}").contains("`{fn}`"));
        assert!(err("echo {fn...}").contains("`{fn...}`"));
    }

    #[test]
    fn a_listed_name_must_be_in_the_template() {
        assert!(parse("ls {dir}", &["dir"]).is_ok());
        assert!(parse("ls {xs...}", &["xs"]).is_ok());
        assert!(parse("{tool} '{a}' --b={b}", &["tool", "a", "b"]).is_ok());

        let message = parse("ls {dir}", &["dir", "extra"]).expect_err("unused name");
        assert!(message.contains("`extra`"), "{message}");
        assert!(message.contains("{extra}"), "{message}");
        assert!(parse("ls {{dir}}", &["dir"]).is_err());
    }

    /// A model word: parts, or a splat.
    fn model_arg() -> impl Strategy<Value = Arg> {
        prop_oneof![
            4 => model_parts().prop_map(Arg::Word),
            1 => "[a-z][a-z0-9_]{0,5}".prop_map(|n| Arg::Splat(format!("s_{n}"))),
        ]
    }

    /// Model parts in normal form: no empty text and no two texts in sequence.
    fn model_parts() -> impl Strategy<Value = Vec<Part>> {
        let part = prop_oneof![
            "[^\0]{1,8}".prop_map(Part::Text),
            "[a-z][a-z0-9_]{0,5}".prop_map(|n| Part::Var(format!("v_{n}"))),
        ];
        prop::collection::vec(part, 0..5).prop_map(|parts| {
            let mut merged: Vec<Part> = Vec::new();
            for part in parts {
                match (merged.last_mut(), part) {
                    (Some(Part::Text(last)), Part::Text(next)) => last.push_str(&next),
                    (_, part) => merged.push(part),
                }
            }
            merged
        })
    }

    fn all_text(parts: &[Part]) -> String {
        parts
            .iter()
            .filter_map(|p| match p {
                Part::Text(t) => Some(t.as_str()),
                Part::Var(_) => None,
            })
            .collect()
    }

    /// Writes parts as one quoted word. Returns `None` if no quote can hold
    /// the text, because a quote has no escapes.
    fn render(parts: &[Part]) -> Option<String> {
        let literal = all_text(parts);
        let quote = if literal.contains('\'') { '"' } else { '\'' };
        if literal.contains(quote) {
            return None;
        }
        let inner: String = parts
            .iter()
            .map(|part| match part {
                Part::Text(t) => t.replace('{', "{{").replace('}', "}}"),
                Part::Var(name) => format!("{{{name}}}"),
            })
            .collect();
        Some(format!("{quote}{inner}{quote}"))
    }

    proptest! {
        #[test]
        fn a_rendered_model_parses_back_to_the_model(
            program in model_parts(),
            args in prop::collection::vec(model_arg(), 0..6),
        ) {
            prop_assume!(!program.is_empty());
            let mut words = vec![render(&program)];
            words.extend(args.iter().map(|arg| match arg {
                Arg::Word(parts) => render(parts),
                Arg::Splat(name) => Some(format!("{{{name}...}}")),
            }));
            let words: Option<Vec<String>> = words.into_iter().collect();
            prop_assume!(words.is_some());
            let template = words.unwrap_or_default().join(" ");

            let parsed = parse(&template, &[]).map_err(TestCaseError::fail)?;
            prop_assert_eq!(parsed, Template { program, args });
        }

        #[test]
        fn each_placeholder_is_in_exactly_one_argument(
            before in "[A-Za-z0-9_./:=@%+,-]{0,8}",
            after in "[A-Za-z0-9_./:=@%+,-]{0,8}",
            count in 1_usize..6,
        ) {
            let one = format!("{before}{{v}}{after}");
            let template = format!("prog {}", vec![one; count].join(" "));
            let parsed = parse(&template, &["v"]).map_err(TestCaseError::fail)?;

            prop_assert_eq!(parsed.args.len(), count);
            for arg in parsed.args {
                let Arg::Word(parts) = arg else {
                    return Err(TestCaseError::fail("a splat"));
                };
                let vars = parts.iter().filter(|p| matches!(p, Part::Var(_))).count();
                prop_assert_eq!(vars, 1);
            }
        }

        #[test]
        fn no_input_causes_a_panic(template in any::<String>()) {
            let _ = parse(&template, &["a"]);
        }
    }
}
