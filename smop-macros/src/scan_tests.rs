//! Tests of the scanner.
//!
//! `lexed` is the Verus spec of `verus/lexer.rs` as executable Rust. The
//! proptests check it on random templates. Thus they also find an error
//! in the spec itself.

use crate::scan::{Atom, Fault, Kind, RawWord, scan};
use proptest::prelude::*;

fn chars(template: &str) -> Vec<char> {
    template.chars().collect()
}

fn ok(template: &str) -> Vec<RawWord> {
    scan(&chars(template)).unwrap_or_else(|e| panic!("{template}: {e:?}"))
}

fn err(template: &str) -> Fault {
    scan(&chars(template)).expect_err(template)
}

const fn atom(kind: Kind, start: usize, end: usize) -> Atom {
    Atom { kind, start, end }
}

const fn ch(c: char, at: usize) -> Atom {
    atom(Kind::Char(c), at, at + 1)
}

#[test]
fn words_have_their_positions() {
    let words = ok(" ab\t{x} ");
    assert_eq!(words.len(), 2);
    assert_eq!((words[0].start, words[0].end), (1, 3));
    assert_eq!(words[0].bare, [ch('a', 1), ch('b', 2)]);
    assert_eq!(words[0].quoted, None);
    assert_eq!((words[1].start, words[1].end), (4, 7));
    assert_eq!(words[1].bare, [atom(Kind::Var, 4, 7)]);
}

#[test]
fn a_blank_template_has_no_words() {
    assert!(ok("").is_empty());
    assert!(ok(" \t\n\r\x0C").is_empty());
}

#[test]
fn a_quoted_word_has_bare_and_quoted_atoms() {
    let words = ok("-m='a b'");
    assert_eq!(words.len(), 1);
    assert_eq!((words[0].start, words[0].end), (0, 8));
    assert_eq!(words[0].bare, [ch('-', 0), ch('m', 1), ch('=', 2)]);
    assert_eq!(
        words[0].quoted,
        Some(vec![ch('a', 4), ch(' ', 5), ch('b', 6)])
    );
    assert_eq!(ok("''")[0].quoted, Some(vec![]));
}

#[test]
fn braces_are_atoms() {
    assert_eq!(
        ok("{{}}")[0].bare,
        [atom(Kind::Char('{'), 0, 2), atom(Kind::Char('}'), 2, 4)]
    );
    assert_eq!(ok("{xs...}")[0].bare, [atom(Kind::Splat, 0, 7)]);
    assert_eq!(ok("'{a}'")[0].quoted, Some(vec![atom(Kind::Var, 1, 4)]));
}

#[test]
fn faults_have_their_positions() {
    assert_eq!(err("ab c'd"), Fault::QuoteInWord { start: 3, at: 4 });
    assert_eq!(
        err("x 'a'b"),
        Fault::QuoteNotLast {
            start: 2,
            open: 2,
            at: 5
        }
    );
    assert_eq!(err("x ='ab"), Fault::Unclosed { start: 2, open: 3 });
    assert_eq!(err("a{b.c}"), Fault::BadName { start: 2, end: 5 });
    assert_eq!(err("a{b"), Fault::LoneOpen);
    assert_eq!(err("a}b"), Fault::LoneClose);
    assert_eq!(err("a\u{a0}"), Fault::FalseSeparator { start: 0, at: 1 });
    assert_eq!(err("'a'\u{a0}"), Fault::FalseSeparator { start: 0, at: 3 });
}

// The spec of `verus/lexer.rs`, as executable Rust.

const fn sep(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

const fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

const fn is_quote(c: char) -> bool {
    matches!(c, '\'' | '"')
}

fn atom_ok(t: &[char], atom: &Atom, quoted: bool) -> bool {
    let (s, e) = (atom.start, atom.end);
    if !(s < e && e <= t.len()) {
        return false;
    }
    match atom.kind {
        Kind::Char(c) if e == s + 1 => t[s] == c && c != '{' && c != '}' && (quoted || !sep(c)),
        Kind::Char(c) => e == s + 2 && (c == '{' || c == '}') && t[s] == c && t[s + 1] == c,
        Kind::Var => {
            s + 3 <= e
                && t[s] == '{'
                && t[e - 1] == '}'
                && t[s + 1..e - 1].iter().all(|&c| name_char(c))
        }
        Kind::Splat => {
            s + 6 <= e
                && t[s] == '{'
                && t[e - 4..e] == ['.', '.', '.', '}']
                && t[s + 1..e - 4].iter().all(|&c| name_char(c))
        }
    }
}

fn chain(t: &[char], atoms: &[Atom], from: usize, to: usize, quoted: bool) -> bool {
    from <= to
        && atoms.iter().all(|a| atom_ok(t, a, quoted))
        && atoms.iter().all(|a| from <= a.start && a.end <= to)
        && atoms.windows(2).all(|w| w[1].start == w[0].end)
        && atoms.first().map_or(from == to, |a| a.start == from)
        && atoms.last().is_none_or(|a| a.end == to)
}

fn chain_end(atoms: &[Atom], from: usize) -> usize {
    atoms.last().map_or(from, |a| a.end)
}

fn word_ok(t: &[char], w: &RawWord) -> bool {
    let open = chain_end(&w.bare, w.start);
    let shape = w.quoted.as_ref().map_or_else(
        || chain(t, &w.bare, w.start, w.end, false),
        |atoms| {
            chain(t, &w.bare, w.start, open, false)
                && open + 2 <= w.end
                && is_quote(t[open])
                && t[w.end - 1] == t[open]
                && chain(t, atoms, open + 1, w.end - 1, true)
        },
    );
    let only_quoted_separators = (w.start..w.end)
        .filter(|&k| sep(t[k]))
        .all(|k| w.quoted.is_some() && open < k && k < w.end - 1);
    w.start < w.end && w.end <= t.len() && shape && only_quoted_separators
}

fn separators(t: &[char], from: usize, to: usize) -> bool {
    t[from..to].iter().all(|&c| sep(c))
}

fn lexed(t: &[char], words: &[RawWord]) -> bool {
    let Some((first, last)) = words.first().zip(words.last()) else {
        return separators(t, 0, t.len());
    };
    words.iter().all(|w| word_ok(t, w))
        && words
            .windows(2)
            .all(|p| p[0].end < p[1].start && separators(t, p[0].end, p[1].start))
        && separators(t, 0, first.start)
        && separators(t, last.end, t.len())
}

#[test]
fn the_spec_holds_for_examples() {
    for template in [
        "",
        "  ",
        "a",
        "cargo test --out={dir} -j{n}",
        "-m 'Release {v}' {files...}",
        "a=\"b c\" '' {{}} }}{{ {x}{y}",
        "'a\u{a0}b' x",
    ] {
        let t = chars(template);
        let words = ok(template);
        assert!(lexed(&t, &words), "{template}: {words:?}");
    }
}

#[test]
fn the_spec_rejects_a_bad_result() {
    let t = chars("a b");
    let mut words = ok("a b");
    assert!(lexed(&t, &words));
    words[1].start = 1;
    assert!(!lexed(&t, &words), "two words with no separator between");
    let mut words = ok("ab");
    words[0].bare.pop();
    assert!(!lexed(&t, &words), "an atom is missing");
}

/// Templates from the characters that the scanner treats in a special way.
fn template() -> impl Strategy<Value = String> {
    let c = prop::sample::select(vec![
        'a', 'Z', '0', '_', '=', '.', '-', ' ', '\t', '\n', '{', '}', '\'', '"', '\u{a0}', '\u{e9}',
    ]);
    prop::collection::vec(c, 0..24).prop_map(|cs| cs.into_iter().collect())
}

/// Templates that the scanner accepts more often.
fn valid_template() -> impl Strategy<Value = String> {
    let word = prop::sample::select(vec![
        "a",
        "--out=",
        "{x}",
        "{xs...}",
        "{{",
        "}}",
        "'a b'",
        "\"{y} z\"",
        "=''",
        "_9",
    ]);
    let sep = prop::sample::select(vec!["", " ", "\t", "  \n"]);
    prop::collection::vec((word, sep), 0..8).prop_map(|parts| {
        parts
            .into_iter()
            .flat_map(<[&str; 2]>::from)
            .collect::<String>()
    })
}

proptest! {
    #[test]
    fn the_spec_holds_for_each_result(template in template()) {
        let t = chars(&template);
        if let Ok(words) = scan(&t) {
            prop_assert!(lexed(&t, &words), "{:?}: {:?}", template, words);
        }
    }

    #[test]
    fn the_spec_holds_for_valid_templates(template in valid_template()) {
        let t = chars(&template);
        if let Ok(words) = scan(&t) {
            prop_assert!(lexed(&t, &words), "{:?}: {:?}", template, words);
        }
    }

    #[test]
    fn no_input_causes_a_panic(template in any::<String>()) {
        let _ = scan(&chars(&template));
    }
}
