//! The Verus proof of the `cmd!` scanner, `smop-macros/src/scan.rs`.
//!
//! The executable code is a copy of that file. `cargo xtask verus-copy`
//! erases the ghost code here and compares the tokens that stay with the
//! shipped file. `cargo xtask verus` runs the check and then Verus.
//!
//! The two properties:
//!
//! - P1, `each_placeholder_is_one_argument`: each `{name}` maps to exactly
//!   one argv element. The placeholder is in one word, and in no other.
//! - P2, `text_never_splits_a_word`: literal text never splits a value.
//!   Word boundaries are only at separators. In a word, a separator is
//!   only between the quotes.
//!
//! `scan` ensures `lexed`. P1 and P2 follow from `lexed`.
#![verifier::loop_isolation(false)]

use vstd::prelude::*;

verus! {

// The spec. `t` is the template.

/// The ASCII whitespace that separates words.
pub open spec fn sep(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\x0C'
}

/// A character of a name: an ASCII letter, an ASCII digit or `_`.
pub open spec fn name_char(c: char) -> bool {
    c == '_' || ('0' <= c && c <= '9') || ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z')
}

pub open spec fn is_quote(c: char) -> bool {
    c == '\'' || c == '"'
}

/// Each character of `t[from..to]` is a separator.
pub open spec fn all_separators(t: Seq<char>, from: int, to: int) -> bool {
    forall|k: int| from <= k < to ==> sep(#[trigger] t[k])
}

/// No character of `t[from..to]` is a separator.
pub open spec fn no_separators(t: Seq<char>, from: int, to: int) -> bool {
    forall|k: int| from <= k < to ==> !sep(#[trigger] t[k])
}

/// Each character of `t[from..to]` is a name character.
pub open spec fn name(t: Seq<char>, from: int, to: int) -> bool {
    forall|k: int| from <= k < to ==> name_char(#[trigger] t[k])
}

/// The text of the atom `a` agrees with its kind.
///
/// A character is one character of `t`, or `{{` or `}}` for one brace.
/// Outside quotes, it is not a separator. A placeholder is `{`, a name and
/// `}`; a splat has `...` before the `}`.
pub open spec fn kind_ok(t: Seq<char>, a: Atom, quoted: bool) -> bool {
    let s = a.start as int;
    let e = a.end as int;
    match a.kind {
        Kind::Char(c) => if e == s + 1 {
            t[s] == c && c != '{' && c != '}' && (quoted || !sep(c))
        } else {
            e == s + 2 && (c == '{' || c == '}') && t[s] == c && t[s + 1] == c
        },
        Kind::Var => s + 3 <= e && t[s] == '{' && t[e - 1] == '}' && name(t, s + 1, e - 1),
        Kind::Splat => s + 6 <= e && t[s] == '{' && t[e - 4] == '.' && t[e - 3] == '.'
            && t[e - 2] == '.' && t[e - 1] == '}' && name(t, s + 1, e - 4),
    }
}

pub open spec fn atom_ok(t: Seq<char>, a: Atom, quoted: bool) -> bool {
    &&& a.start < a.end <= t.len()
    &&& kind_ok(t, a, quoted)
}

/// The end of the last atom, or `from` if there are no atoms.
pub open spec fn chain_end(atoms: Seq<Atom>, from: int) -> int {
    if atoms.len() == 0 {
        from
    } else {
        atoms[atoms.len() - 1].end as int
    }
}

/// The atoms cover `t[from..to]` in sequence: no gap, no overlap.
pub open spec fn chain(t: Seq<char>, atoms: Seq<Atom>, from: int, to: int, quoted: bool) -> bool {
    &&& from <= to
    &&& chain_end(atoms, from) == to
    &&& atoms.len() > 0 ==> atoms[0].start == from
    &&& forall|j: int| 0 <= j < atoms.len() ==> atom_ok(t, #[trigger] atoms[j], quoted)
    &&& forall|j: int|
        0 <= j < atoms.len() ==> from <= (#[trigger] atoms[j]).start && atoms[j].end <= to
    &&& forall|j: int| 0 < j < atoms.len() ==> (#[trigger] atoms[j]).start == atoms[j - 1].end
}

/// The atoms cover the word. A quoted section is a quote, atoms, and the
/// same quote. It ends the word.
pub open spec fn shape_ok(t: Seq<char>, w: RawWord) -> bool {
    let start = w.start as int;
    let end = w.end as int;
    let open = chain_end(w.bare@, start);
    match w.quoted {
        None => chain(t, w.bare@, start, end, false),
        Some(atoms) => {
            &&& chain(t, w.bare@, start, open, false)
            &&& open + 2 <= end
            &&& is_quote(t[open])
            &&& t[end - 1] == t[open]
            &&& chain(t, atoms@, open + 1, end - 1, true)
        },
    }
}

/// A word is `t[start..end]`. In a word, a separator is only between the
/// quotes.
pub open spec fn word_ok(t: Seq<char>, w: RawWord) -> bool {
    let start = w.start as int;
    let end = w.end as int;
    let open = chain_end(w.bare@, start);
    &&& start < end <= t.len()
    &&& shape_ok(t, w)
    &&& forall|k: int|
        start <= k < end && sep(#[trigger] t[k]) ==> (w.quoted is Some && open < k < end - 1)
}

/// The words of `t`. Only separators are before, between and after them.
/// There is at least one separator between two words.
pub open spec fn lexed(t: Seq<char>, words: Seq<RawWord>) -> bool {
    &&& forall|n: int| 0 <= n < words.len() ==> word_ok(t, #[trigger] words[n])
    &&& forall|n: int| 0 < n < words.len() ==> words[n - 1].end < (#[trigger] words[n]).start
    &&& forall|n: int|
        0 < n < words.len() ==> all_separators(
            t,
            words[n - 1].end as int,
            (#[trigger] words[n]).start as int,
        )
    &&& words.len() == 0 ==> all_separators(t, 0, t.len() as int)
    &&& words.len() > 0 ==> all_separators(t, 0, words[0].start as int)
    &&& words.len() > 0 ==> all_separators(
        t,
        words[words.len() - 1].end as int,
        t.len() as int,
    )
}

/// `lexed`, up to index `i` of `t`: the loop invariant of `scan`.
pub open spec fn scanned(t: Seq<char>, words: Seq<RawWord>, i: int) -> bool {
    &&& forall|n: int| 0 <= n < words.len() ==> word_ok(t, #[trigger] words[n])
    &&& forall|n: int| 0 < n < words.len() ==> words[n - 1].end < (#[trigger] words[n]).start
    &&& forall|n: int|
        0 < n < words.len() ==> all_separators(
            t,
            words[n - 1].end as int,
            (#[trigger] words[n]).start as int,
        )
    &&& words.len() == 0 ==> all_separators(t, 0, i)
    &&& words.len() > 0 ==> all_separators(t, 0, words[0].start as int)
    &&& words.len() > 0 ==> words[words.len() - 1].end <= i
    &&& words.len() > 0 ==> all_separators(t, words[words.len() - 1].end as int, i)
    &&& words.len() > 0 && i < t.len() ==> words[words.len() - 1].end < i
}

/// The bare or the quoted atoms of `w`.
pub open spec fn atoms_of(w: RawWord, quoted: bool) -> Seq<Atom> {
    if quoted {
        match w.quoted {
            Some(atoms) => atoms@,
            None => Seq::empty(),
        }
    } else {
        w.bare@
    }
}

// The two properties.

/// P1: each `{name}` maps to exactly one argv element.
///
/// A `Var` atom of word `i` is `{`, a name and `}` in `t`. It is in word
/// `i`, and in no other word.
pub proof fn each_placeholder_is_one_argument(
    t: Seq<char>,
    words: Seq<RawWord>,
    i: int,
    quoted: bool,
    j: int,
)
    requires
        lexed(t, words),
        0 <= i < words.len(),
        0 <= j < atoms_of(words[i], quoted).len(),
        atoms_of(words[i], quoted)[j].kind is Var,
    ensures
        ({
            let a = atoms_of(words[i], quoted)[j];
            &&& t[a.start as int] == '{'
            &&& name(t, a.start + 1, a.end - 1)
            &&& t[a.end - 1] == '}'
            &&& words[i].start <= a.start
            &&& a.end <= words[i].end
            &&& forall|n: int|
                0 <= n < words.len() && n != i ==> (#[trigger] words[n]).end <= a.start
                    || a.end <= words[n].start
        }),
{
    let w = words[i];
    let a = atoms_of(w, quoted)[j];
    assert(word_ok(t, w));
    if quoted {
        let atoms = (w.quoted->0)@;
        assert(atoms[j] == a);
        assert(atom_ok(t, atoms[j], true));
    } else {
        assert(w.bare@[j] == a);
        assert(atom_ok(t, w.bare@[j], false));
    }
    assert forall|n: int| 0 <= n < words.len() && n != i implies (#[trigger] words[n]).end
        <= a.start || a.end <= words[n].start by {
        if n < i {
            lemma_ordered(t, words, n, i);
        } else {
            lemma_ordered(t, words, i, n);
            assert(word_ok(t, words[n]));
        }
    }
}

/// P2: literal text never splits a value.
///
/// A character that is not a separator, next to a character of word `i`,
/// is in word `i`. Thus only separators end a word. (`word_ok` adds: in a
/// word, a separator is only between the quotes.)
pub proof fn text_never_splits_a_word(t: Seq<char>, words: Seq<RawWord>, i: int, k: int)
    requires
        lexed(t, words),
        0 <= i < words.len(),
        words[i].start <= k < words[i].end,
    ensures
        k + 1 < t.len() && !sep(t[k + 1]) ==> k + 1 < words[i].end,
        1 <= k && !sep(t[k - 1]) ==> words[i].start <= k - 1,
{
    assert(word_ok(t, words[i]));
    if i + 1 < words.len() {
        assert(words[i].end < words[i + 1].start);
        assert(all_separators(t, words[i].end as int, words[i + 1].start as int));
    }
    if i > 0 {
        assert(words[i - 1].end < words[i].start);
        assert(all_separators(t, words[i - 1].end as int, words[i].start as int));
    }
}

// Lemmas.

/// The words are in sequence and do not touch.
proof fn lemma_ordered(t: Seq<char>, words: Seq<RawWord>, m: int, n: int)
    requires
        lexed(t, words),
        0 <= m < n < words.len(),
    ensures
        words[m].end < words[n].start,
    decreases n - m,
{
    assert(words[n - 1].end < words[n].start);
    if m < n - 1 {
        lemma_ordered(t, words, m, n - 1);
        assert(word_ok(t, words[n - 1]));
    }
}

/// An atom that starts at the end of a chain makes the chain longer.
proof fn lemma_chain_push(
    t: Seq<char>,
    atoms: Seq<Atom>,
    from: int,
    to: int,
    quoted: bool,
    a: Atom,
)
    requires
        chain(t, atoms, from, to, quoted),
        atom_ok(t, a, quoted),
        a.start == to,
    ensures
        chain(t, atoms.push(a), from, a.end as int, quoted),
{
    let s = atoms.push(a);
    assert(s.len() == atoms.len() + 1);
    assert(s[atoms.len() as int] == a);
    assert forall|j: int| 0 <= j < atoms.len() implies #[trigger] s[j] == atoms[j] by {}
    if atoms.len() > 0 {
        assert(s[0] == atoms[0]);
    }
    assert forall|j: int| 0 <= j < s.len() implies atom_ok(t, #[trigger] s[j], quoted) by {
        if j < atoms.len() {
            assert(s[j] == atoms[j]);
        }
    }
    assert forall|j: int| 0 <= j < s.len() implies from <= (#[trigger] s[j]).start && s[j].end
        <= a.end by {
        if j < atoms.len() {
            assert(s[j] == atoms[j]);
        }
    }
    assert forall|j: int| 0 < j < s.len() implies (#[trigger] s[j]).start == s[j - 1].end by {
        assert(s[j - 1] == atoms[j - 1]);
        if j < atoms.len() {
            assert(s[j] == atoms[j]);
        }
    }
}

/// A word, and the separators after it, make `scanned` go further.
proof fn lemma_scanned_push(t: Seq<char>, words: Seq<RawWord>, i: int, w: RawWord, next: int)
    requires
        scanned(t, words, i),
        i < t.len(),
        word_ok(t, w),
        w.start == i,
        w.end < t.len() ==> sep(t[w.end as int]),
        w.end <= next <= t.len(),
        all_separators(t, w.end as int, next),
        next < t.len() ==> !sep(t[next]),
    ensures
        scanned(t, words.push(w), next),
{
    let s = words.push(w);
    assert(s.len() == words.len() + 1);
    assert(s[words.len() as int] == w);
    assert forall|n: int| 0 <= n < words.len() implies #[trigger] s[n] == words[n] by {}
    if words.len() > 0 {
        assert(s[0] == words[0]);
    } else {
        assert(s[0] == w);
    }
    assert forall|n: int| 0 <= n < s.len() implies word_ok(t, #[trigger] s[n]) by {
        if n < words.len() {
            assert(s[n] == words[n]);
        }
    }
    assert forall|n: int| 0 < n < s.len() implies s[n - 1].end < (#[trigger] s[n]).start
        && all_separators(t, s[n - 1].end as int, s[n].start as int) by {
        assert(s[n - 1] == words[n - 1]);
        if n < words.len() {
            assert(s[n] == words[n]);
        }
    }
    if next < t.len() && w.end == next {
        assert(sep(t[w.end as int]));
    }
}

// The executable code. It is a copy of `smop-macros/src/scan.rs`.

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
    QuoteNotLast { start: usize, open: usize, at: usize },
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
pub fn scan(t: &[char]) -> (r: Result<Vec<RawWord>, Fault>)
    ensures
        r matches Ok(words) ==> lexed(t@, words@),
{
    let mut words = Vec::new();
    let mut i = skip_separators(t, 0);
    while i < t.len()
        invariant
            i <= t@.len(),
            i < t@.len() ==> !sep(t@[i as int]),
            scanned(t@, words@, i as int),
        decreases t@.len() - i,
    {
        let word = scan_word(t, i)?;
        let next = skip_separators(t, word.end);
        proof {
            lemma_scanned_push(t@, words@, i as int, word, next as int);
        }
        words.push(word);
        i = next;
    }
    Ok(words)
}

/// Returns the index of the first character at or after `from` that is
/// not a separator.
#[allow(clippy::missing_const_for_fn)]
fn skip_separators(t: &[char], from: usize) -> (r: usize)
    requires
        from <= t@.len(),
    ensures
        from <= r <= t@.len(),
        all_separators(t@, from as int, r as int),
        r < t@.len() ==> !sep(t@[r as int]),
{
    let mut i = from;
    while i < t.len() && is_separator(t[i])
        invariant
            from <= i <= t@.len(),
            all_separators(t@, from as int, i as int),
        decreases t@.len() - i,
    {
        i += 1;
    }
    i
}

/// Scans the word that starts at `start`.
fn scan_word(t: &[char], start: usize) -> (r: Result<RawWord, Fault>)
    requires
        start < t@.len(),
        !sep(t@[start as int]),
    ensures
        r matches Ok(w) ==> word_ok(t@, w) && w.start == start && (w.end < t@.len() ==> sep(
            t@[w.end as int],
        )),
{
    let mut bare = Vec::new();
    let mut i = start;
    let mut after_equals = false;
    while i < t.len() && !is_separator(t[i])
        invariant
            start <= i <= t@.len(),
            chain(t@, bare@, start as int, i as int, false),
            no_separators(t@, start as int, i as int),
        decreases t@.len() - i,
    {
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
        proof {
            assert(atom.start == i);
            assert(atom_ok(t@, atom, false));
            assert(no_separators(t@, atom.start as int, atom.end as int));
            lemma_chain_push(t@, bare@, start as int, i as int, false, atom);
        }
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
fn scan_quoted(t: &[char], start: usize, bare: Vec<Atom>, open: usize) -> (r: Result<RawWord, Fault>)
    requires
        start <= open < t@.len(),
        chain(t@, bare@, start as int, open as int, false),
        no_separators(t@, start as int, open as int),
        is_quote(t@[open as int]),
    ensures
        r matches Ok(w) ==> word_ok(t@, w) && w.start == start && (w.end < t@.len() ==> sep(
            t@[w.end as int],
        )),
{
    let quote = t[open];
    let mut atoms = Vec::new();
    let mut i = open + 1;
    while i < t.len() && t[i] != quote
        invariant
            open < i <= t@.len(),
            quote == t@[open as int],
            chain(t@, atoms@, open + 1, i as int, true),
        decreases t@.len() - i,
    {
        let atom = if t[i] == '{' || t[i] == '}' {
            scan_brace(t, i)?
        } else {
            Atom {
                kind: Kind::Char(t[i]),
                start: i,
                end: i + 1,
            }
        };
        proof {
            assert(atom.start == i);
            assert(atom_ok(t@, atom, true));
            lemma_chain_push(t@, atoms@, open + 1, i as int, true, atom);
        }
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
    proof {
        assert(t@[i as int] == quote);
        assert(!sep(t@[open as int]));
        assert(!sep(t@[i as int]));
    }
    Ok(RawWord {
        start,
        end,
        bare,
        quoted: Some(atoms),
    })
}

/// Scans `{{`, `}}`, `{name}` or `{name...}` at `at`.
fn scan_brace(t: &[char], at: usize) -> (r: Result<Atom, Fault>)
    requires
        at < t@.len(),
        t@[at as int] == '{' || t@[at as int] == '}',
    ensures
        r matches Ok(a) ==> a.start == at && atom_ok(t@, a, false) && atom_ok(t@, a, true)
            && no_separators(t@, a.start as int, a.end as int),
{
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
    while close < t.len() && t[close] != '}'
        invariant
            at < close <= t@.len(),
        decreases t@.len() - close,
    {
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
    proof {
        assert(t@[at as int] == '{');
        assert(t@[close as int] == '}');
        assert forall|k: int| at <= k < close + 1 implies !sep(#[trigger] t@[k]) by {
            if at < k < name_end {
                assert(name_char(t@[k]));
            }
        }
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
fn is_name(t: &[char], from: usize, to: usize) -> (r: bool)
    requires
        from <= to <= t@.len(),
    ensures
        r ==> from < to && name(t@, from as int, to as int),
{
    if from == to || (to - from == 1 && t[from] == '_') || is_digit(t[from]) {
        return false;
    }
    let mut i = from;
    while i < to
        invariant
            from <= i <= to <= t@.len(),
            name(t@, from as int, i as int),
        decreases to - i,
    {
        if !(is_digit(t[i]) || is_letter(t[i]) || t[i] == '_') {
            return false;
        }
        i += 1;
    }
    true
}

/// Returns `true` for an ASCII digit.
#[allow(clippy::manual_range_contains)]
const fn is_digit(c: char) -> (r: bool)
    ensures
        r == ('0' <= c && c <= '9'),
{
    '0' <= c && c <= '9'
}

/// Returns `true` for an ASCII letter.
#[allow(clippy::manual_range_contains)]
const fn is_letter(c: char) -> (r: bool)
    ensures
        r == (('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z')),
{
    ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z')
}

/// Returns `true` for the ASCII whitespace that separates words.
pub const fn is_separator(c: char) -> (r: bool)
    ensures
        r == sep(c),
{
    c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\x0C'
}

/// Returns `true` for a character that looks like a separator but is not
/// one: other whitespace, and the invisible U+200B and U+FEFF.
pub const fn is_false_separator(c: char) -> bool {
    (c.is_whitespace() && !is_separator(c)) || c == '\u{200b}' || c == '\u{feff}'
}

} // verus!
