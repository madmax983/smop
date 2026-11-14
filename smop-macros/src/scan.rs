//! The scanner of the `cmd!` lexer: template characters to atoms.
//!
//! `verus/lexer.rs` has a copy of this file with a Verus proof.
//! `cargo xtask verus-copy` makes sure that the two have the same code.
//! Thus, write only Rust that Verus accepts: indexes, `while` loops, no
//! closures and no `String`. `lex` makes the strings and error messages.

/// What an atom is.
#[derive(Debug, PartialEq, Eq)]
pub enum Kind {
    /// One literal character. `{{` and `}}` give one brace.
    Char(char),
    /// `{name}`.
    Var,
    /// `{name...}`.
    Splat,
}

/// A piece of a word: `t[start..end]` in the template `t`.
#[derive(Debug, PartialEq, Eq)]
pub struct Atom {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

/// One word: `t[start..end]` in the template `t`.
#[derive(Debug, PartialEq, Eq)]
pub struct RawWord {
    pub start: usize,
    pub end: usize,
    /// The atoms before the quoted section.
    pub bare: Vec<Atom>,
    /// The atoms between the quotes. `Some(vec![])` is `''`.
    pub quoted: Option<Vec<Atom>>,
}

/// Why the scanner stopped. Each position is an index into the template.
#[derive(Debug, PartialEq, Eq)]
pub enum Fault {
    /// The character at `at` looks like a separator, but is not one.
    FalseSeparator { start: usize, at: usize },
    /// The quote at `at` does not start the word or follow `=`.
    QuoteInWord { start: usize, at: usize },
    /// The quote that opens at `open` closes before `at`, which is not a
    /// separator.
    QuoteNotLast {
        start: usize,
        open: usize,
        at: usize,
    },
    /// The quote at `open` does not close.
    Unclosed { start: usize, open: usize },
    /// A `{` has no `}`.
    LoneOpen,
    /// A `}` has no `{`.
    LoneClose,
    /// `t[start..end]`, between braces, is not a name.
    BadName { start: usize, end: usize },
}

/// Splits the template `t` into words.
pub fn scan(t: &[char]) -> Result<Vec<RawWord>, Fault> {
    let mut words = Vec::new();
    let mut i = skip_separators(t, 0);
    while i < t.len() {
        let word = scan_word(t, i)?;
        let next = skip_separators(t, word.end);
        words.push(word);
        i = next;
    }
    Ok(words)
}

/// Returns the index of the first character at or after `from` that is
/// not a separator.
#[allow(clippy::missing_const_for_fn)]
fn skip_separators(t: &[char], from: usize) -> usize {
    let mut i = from;
    while i < t.len() && is_separator(t[i]) {
        i += 1;
    }
    i
}

/// Scans the word that starts at `start`.
fn scan_word(t: &[char], start: usize) -> Result<RawWord, Fault> {
    let mut bare = Vec::new();
    let mut i = start;
    let mut after_equals = false;
    while i < t.len() && !is_separator(t[i]) {
        let c = t[i];
        if c == '\'' || c == '"' {
            if i == start || after_equals {
                return scan_quoted(t, start, bare, i);
            }
            return Err(Fault::QuoteInWord { start, at: i });
        }
        let atom = if c == '{' || c == '}' {
            scan_brace(t, i)?
        } else if is_false_separator(c) {
            return Err(Fault::FalseSeparator { start, at: i });
        } else {
            Atom {
                kind: Kind::Char(c),
                start: i,
                end: i + 1,
            }
        };
        i = atom.end;
        bare.push(atom);
        after_equals = c == '=';
    }
    Ok(RawWord {
        start,
        end: i,
        bare,
        quoted: None,
    })
}

/// Scans the rest of a word from its opening quote at `open`. The quoted
/// section must end the word.
fn scan_quoted(t: &[char], start: usize, bare: Vec<Atom>, open: usize) -> Result<RawWord, Fault> {
    let quote = t[open];
    let mut atoms = Vec::new();
    let mut i = open + 1;
    while i < t.len() && t[i] != quote {
        let atom = if t[i] == '{' || t[i] == '}' {
            scan_brace(t, i)?
        } else {
            Atom {
                kind: Kind::Char(t[i]),
                start: i,
                end: i + 1,
            }
        };
        i = atom.end;
        atoms.push(atom);
    }
    if i == t.len() {
        return Err(Fault::Unclosed { start, open });
    }
    let end = i + 1;
    if end < t.len() && !is_separator(t[end]) {
        if is_false_separator(t[end]) {
            return Err(Fault::FalseSeparator { start, at: end });
        }
        return Err(Fault::QuoteNotLast {
            start,
            open,
            at: end,
        });
    }
    Ok(RawWord {
        start,
        end,
        bare,
        quoted: Some(atoms),
    })
}

/// Scans `{{`, `}}`, `{name}` or `{name...}` at `at`.
fn scan_brace(t: &[char], at: usize) -> Result<Atom, Fault> {
    let c = t[at];
    if at + 1 < t.len() && t[at + 1] == c {
        return Ok(Atom {
            kind: Kind::Char(c),
            start: at,
            end: at + 2,
        });
    }
    if c == '}' {
        return Err(Fault::LoneClose);
    }
    let mut close = at + 1;
    while close < t.len() && t[close] != '}' {
        close += 1;
    }
    if close == t.len() {
        return Err(Fault::LoneOpen);
    }
    let splat =
        close - at >= 4 && t[close - 3] == '.' && t[close - 2] == '.' && t[close - 1] == '.';
    let name_end = if splat { close - 3 } else { close };
    if !is_name(t, at + 1, name_end) {
        return Err(Fault::BadName {
            start: at + 1,
            end: close,
        });
    }
    let kind = if splat { Kind::Splat } else { Kind::Var };
    Ok(Atom {
        kind,
        start: at,
        end: close + 1,
    })
}

/// Returns `true` if `t[from..to]` is an ASCII identifier that is not `_`.
#[allow(clippy::missing_const_for_fn)]
fn is_name(t: &[char], from: usize, to: usize) -> bool {
    if from == to || (to - from == 1 && t[from] == '_') || is_digit(t[from]) {
        return false;
    }
    let mut i = from;
    while i < to {
        if !(is_digit(t[i]) || is_letter(t[i]) || t[i] == '_') {
            return false;
        }
        i += 1;
    }
    true
}

/// Returns `true` for an ASCII digit.
#[allow(clippy::manual_range_contains)]
const fn is_digit(c: char) -> bool {
    '0' <= c && c <= '9'
}

/// Returns `true` for an ASCII letter.
#[allow(clippy::manual_range_contains)]
const fn is_letter(c: char) -> bool {
    ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z')
}

/// Returns `true` for the ASCII whitespace that separates words.
pub const fn is_separator(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\x0C'
}

/// Returns `true` for a character that looks like a separator but is not
/// one: other whitespace, and the invisible U+200B and U+FEFF.
pub const fn is_false_separator(c: char) -> bool {
    (c.is_whitespace() && !is_separator(c)) || c == '\u{200b}' || c == '\u{feff}'
}
