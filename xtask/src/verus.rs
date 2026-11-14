//! `verus-copy` and `verus`: the Verus proof of the `cmd!` scanner.
//!
//! `verus/lexer.rs` is `smop-macros/src/scan.rs` plus ghost code. The
//! copy check erases the ghost code and compares the tokens that stay.

use std::path::Path;

use smop::prelude::*;

/// The Verus copy, from the workspace root.
pub const COPY: &str = "verus/lexer.rs";
/// The shipped scanner, from the workspace root.
pub const SHIPPED: &str = "smop-macros/src/scan.rs";

/// The Verus release that `cargo xtask verus` downloads.
pub const RELEASE: &str = "0.2026.11.02.8bf68ee";
/// The SHA-256 of the x86-64 Linux zip of [`RELEASE`].
pub const SHA256: &str = "f3a0119292df8855e4435da8130bee1873b0e6a60c306accf81a08ea687bba3c";
/// The Rust toolchain that [`RELEASE`] needs.
pub const TOOLCHAIN: &str = "1.99.1-x86_64-unknown-linux-gnu";

/// The SHA-256 of the statements in the copy: each `spec fn`, and the
/// signature and clauses of each `pub fn`. A change to it needs a review.
pub const SPEC_SHA256: &str = "895b520859ca9569373a317f46f2f57ed75e2e59a08f0b40c42395ed2a5054dd";

/// Identifiers that let a proof skip a step. The copy must not use them.
/// Verus also runs with `--no-cheating`.
const UNSOUND: &[&str] = &[
    "assume",
    "admit",
    "external",
    "external_body",
    "assume_specification",
    "external_fn_specification",
    "external_type_specification",
    "external_trait_specification",
    "external_trait_extension",
    "exec_allows_no_decreases_clause",
    "assume_termination",
    "assume_unsupported_attributes",
    "assume_new",
];

/// Identifiers that can give the same tokens a different meaning in the
/// two files. Neither file can use them.
const FORBIDDEN: &[&str] = &[
    "cfg",
    "cfg_attr",
    "include",
    "include_str",
    "include_bytes",
    "mod",
    "path",
    "macro_rules",
    "crate",
    "super",
    "env",
    "option_env",
];

/// The `#[verifier::..]` attributes that the copy can use.
const VERIFIER_ATTRIBUTES: &[&str] = &["loop_isolation", "opaque", "rlimit"];

/// The characters of operators. Two of them with no space between are one
/// operator: `&&` is not `& &`.
const OPERATOR: &str = "!#$%&*+-./:<=>?@^|~";

/// Verus clauses: each goes to the next `{` or `;`.
const CLAUSES: &[&str] = &[
    "requires",
    "ensures",
    "recommends",
    "returns",
    "invariant",
    "invariant_except_break",
    "decreases",
];

/// Words that can come before `spec fn` and `proof fn`.
const MODIFIERS: &[&str] = &["pub", "open", "closed", "broadcast", "uninterp"];

/// One token and its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The source text.
    pub text: String,
    /// The line, from 1.
    pub line: usize,
    /// An operator character with an operator character after it.
    pub joint: bool,
}

/// Splits Rust source into tokens. Comments and whitespace are not tokens.
/// A punctuation token is one character.
pub fn tokens(src: &str) -> Result<Vec<Token>> {
    let cs: Vec<char> = src.chars().collect();
    let at = |i: usize| cs.get(i).copied();
    let mut out = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while let Some(c) = at(i) {
        let next = at(i + 1);
        let end = if c.is_whitespace() {
            i + 1
        } else if c == '/' && next == Some('/') {
            cs[i..]
                .iter()
                .position(|&c| c == '\n')
                .map_or(cs.len(), |n| i + n)
        } else if c == '/' && next == Some('*') {
            comment_end(&cs, i).with_context(|| format!("line {line}: unclosed comment"))?
        } else {
            let end =
                token_end(&cs, i).with_context(|| format!("line {line}: unclosed literal"))?;
            let joint = OPERATOR.contains(c) && next.is_some_and(|n| OPERATOR.contains(n));
            out.push(Token {
                text: cs[i..end].iter().collect(),
                line,
                joint,
            });
            end
        };
        line += cs[i..end].iter().filter(|&&c| c == '\n').count();
        i = end;
    }
    Ok(out)
}

/// The end of the `/* */` comment at `i`. Comments nest.
const fn comment_end(cs: &[char], mut i: usize) -> Option<usize> {
    let mut depth = 0;
    while i + 1 < cs.len() {
        match (cs[i], cs[i + 1]) {
            ('/', '*') => depth += 1,
            ('*', '/') => depth -= 1,
            _ => {
                i += 1;
                continue;
            }
        }
        i += 2;
        if depth == 0 {
            return Some(i);
        }
    }
    None
}

/// The end of the token at `i`, which is not whitespace or a comment.
fn token_end(cs: &[char], i: usize) -> Option<usize> {
    let at = |i: usize| cs.get(i).copied();
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let c = cs[i];
    match c {
        '"' => string_end(cs, i),
        'b' | 'c' if at(i + 1) == Some('"') => string_end(cs, i + 1),
        'b' | 'c' if at(i + 1) == Some('r') && matches!(at(i + 2), Some('"' | '#')) => {
            raw_end(cs, i + 1).or_else(|| Some(i + 2))
        }
        'b' if at(i + 1) == Some('\'') => char_end(cs, i + 1),
        'r' if matches!(at(i + 1), Some('"' | '#')) => raw_end(cs, i).or_else(|| Some(i + 1)),
        '\'' => char_end(cs, i),
        c if ident(c) => Some(
            cs[i..]
                .iter()
                .position(|&c| !ident(c))
                .map_or(cs.len(), |n| i + n),
        ),
        _ => Some(i + 1),
    }
}

/// The end of the string literal with its `"` at `i`.
fn string_end(cs: &[char], mut i: usize) -> Option<usize> {
    i += 1;
    loop {
        match cs.get(i)? {
            '\\' => i += 2,
            '"' => return Some(i + 1),
            _ => i += 1,
        }
    }
}

/// The end of the raw string with its `r` at `i`, or `None` if `r#` starts
/// a raw identifier.
fn raw_end(cs: &[char], i: usize) -> Option<usize> {
    let hashes = cs[i + 1..].iter().take_while(|&&c| c == '#').count();
    let open = i + 1 + hashes;
    if cs.get(open) != Some(&'"') {
        return None;
    }
    let mut j = open + 1;
    while j < cs.len() {
        if cs[j] == '"'
            && cs[j + 1..]
                .iter()
                .take(hashes)
                .filter(|&&c| c == '#')
                .count()
                == hashes
        {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    None
}

/// The end of the character literal or lifetime with its `'` at `i`.
fn char_end(cs: &[char], i: usize) -> Option<usize> {
    let at = |i: usize| cs.get(i).copied();
    match at(i + 1)? {
        // The escaped character can be `'`, so the search starts after it.
        '\\' => cs
            .get(i + 3..)?
            .iter()
            .position(|&c| c == '\'')
            .map(|n| i + 4 + n),
        _ if at(i + 2) == Some('\'') => Some(i + 3),
        _ => Some(
            cs[i + 1..]
                .iter()
                .position(|&c| !(c.is_alphanumeric() || c == '_'))
                .map_or(cs.len(), |n| i + 1 + n),
        ),
    }
}

/// The index of the bracket that closes the one at `open`.
fn matching(tokens: &[Token], open: usize) -> Result<usize> {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        match token.text.as_str() {
            "{" | "(" | "[" => depth += 1,
            "}" | ")" | "]" => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
    }
    bail!(
        "line {}: `{}` does not close",
        tokens[open].line,
        tokens[open].text
    )
}

/// The index of the first token at or after `from`, outside brackets,
/// that is one of `stops`.
fn find_stop(tokens: &[Token], from: usize, stops: &[&str]) -> Result<usize> {
    let mut i = from;
    while i < tokens.len() {
        let text = tokens[i].text.as_str();
        if stops.contains(&text) {
            return Ok(i);
        }
        i = if matches!(text, "(" | "[") {
            matching(tokens, i)? + 1
        } else {
            i + 1
        };
    }
    bail!(
        "line {}: no {stops:?} after `{}`",
        tokens[from].line,
        tokens[from].text
    )
}

/// Removes the Verus ghost code from `tokens`: `verus! {}`,
/// `use vstd::prelude::*;`, `spec fn` and `proof fn` items, clauses after a
/// `fn`, `while`, `loop` or `for` header, `proof {}` blocks and `let ghost`
/// at the start of a statement, `#[verifier::..]` and `#[trigger]`, and the
/// name of a named return value. All other tokens stay.
pub fn erase(tokens: &[Token]) -> Result<Vec<Token>> {
    let text = |i: usize| tokens.get(i).map_or("", |t| t.text.as_str());
    let mut out: Vec<Token> = Vec::new();
    let mut dropped = std::collections::BTreeSet::new();
    // In a header: after `fn`, `while`, `loop` or `for`, before its `{`.
    let mut header = false;
    let mut i = 0;
    while i < tokens.len() {
        let t = text(i);
        let statement_start = out
            .last()
            .is_none_or(|l| matches!(l.text.as_str(), ";" | "{" | "}"));
        i = if dropped.contains(&i) {
            i + 1
        } else if t == "verus" && text(i + 1) == "!" && text(i + 2) == "{" {
            dropped.insert(matching(tokens, i + 2)?);
            i + 3
        } else if is_prelude_use(tokens, i) {
            i + PRELUDE_USE.len()
        } else if t == "#" && is_ghost_attribute(text(i + 1), text(i + 2), text(i + 3)) {
            let open = if text(i + 1) == "!" { i + 2 } else { i + 1 };
            matching(tokens, open)? + 1
        } else if let Some(fn_at) = ghost_item(tokens, i) {
            item_end(tokens, fn_at)?
        } else if statement_start && t == "proof" && text(i + 1) == "{" {
            matching(tokens, i + 1)? + 1
        } else if header && CLAUSES.contains(&t) {
            find_stop(tokens, i, &["{", ";"])?
        } else if statement_start && t == "let" && matches!(text(i + 1), "ghost" | "tracked") {
            find_stop(tokens, i, &[";"])? + 1
        } else if t == "-"
            && text(i + 1) == ">"
            && text(i + 2) == "("
            && text(i + 4) == ":"
            && text(i + 5) != ":"
        {
            out.extend_from_slice(&tokens[i..i + 2]);
            dropped.insert(matching(tokens, i + 2)?);
            i + 5
        } else {
            if matches!(t, "fn" | "while" | "loop" | "for") {
                header = true;
            } else if matches!(t, "{" | ";") {
                header = false;
            }
            out.push(tokens[i].clone());
            i + 1
        };
    }
    Ok(out)
}

/// The tokens of `use vstd::prelude::*;`.
const PRELUDE_USE: [&str; 9] = ["use", "vstd", ":", ":", "prelude", ":", ":", "*", ";"];

fn is_prelude_use(tokens: &[Token], i: usize) -> bool {
    tokens
        .get(i..i + PRELUDE_USE.len())
        .is_some_and(|t| t.iter().zip(PRELUDE_USE).all(|(t, p)| t.text == p))
}

/// The index after the item whose `fn` is at `fn_at`.
fn item_end(tokens: &[Token], fn_at: usize) -> Result<usize> {
    let stop = find_stop(tokens, fn_at, &["{", ";"])?;
    Ok(if tokens[stop].text == "{" {
        matching(tokens, stop)? + 1
    } else {
        stop + 1
    })
}

/// Whether `#` and the next three tokens start `#[verifier..]`,
/// `#![verifier..]`, `#[trigger]` or `#![trigger..]`.
fn is_ghost_attribute(a: &str, b: &str, c: &str) -> bool {
    let name = if a == "!" && b == "[" {
        c
    } else if a == "[" {
        b
    } else {
        return false;
    };
    matches!(name, "verifier" | "trigger")
}

/// If a `spec fn` or `proof fn` item starts at `i`, the index of its `fn`.
fn ghost_item(tokens: &[Token], mut i: usize) -> Option<usize> {
    let text = |i: usize| tokens.get(i).map_or("", |t| t.text.as_str());
    while MODIFIERS.contains(&text(i)) {
        i += 1;
        if text(i) == "(" {
            i = matching(tokens, i).ok()? + 1;
        }
    }
    (matches!(text(i), "spec" | "proof") && text(i + 1) == "fn").then_some(i + 1)
}

/// The tokens that let a proof skip a step.
pub fn unsound(tokens: &[Token]) -> Vec<Token> {
    tokens
        .iter()
        .filter(|t| UNSOUND.contains(&t.text.as_str()))
        .cloned()
        .collect()
}

/// Errors for the form of the copy: inner `#![verifier::..]` attributes,
/// then `use vstd::prelude::*;`, then one `verus! {}` block, and nothing
/// after it. Code outside `verus!` is not verified.
fn structure(tokens: &[Token]) -> Result<Vec<String>> {
    let text = |i: usize| tokens.get(i).map_or("", |t| t.text.as_str());
    let mut i = 0;
    while text(i) == "#" && text(i + 1) == "!" && text(i + 2) == "[" {
        i = matching(tokens, i + 2)? + 1;
    }
    if !is_prelude_use(tokens, i) {
        return Ok(vec![format!(
            "{COPY}: after the inner attributes, the copy must have `use vstd::prelude::*;`"
        )]);
    }
    i += PRELUDE_USE.len();
    let one_block = text(i) == "verus"
        && text(i + 1) == "!"
        && text(i + 2) == "{"
        && matching(tokens, i + 2)? + 1 == tokens.len();
    Ok(if one_block {
        Vec::new()
    } else {
        vec![format!(
            "{COPY}: all code must be in one `verus! {{}}` block at the end of the file"
        )]
    })
}

/// Errors for `#[verifier::X]` attributes where `X` is not allowed.
fn verifier_attributes(tokens: &[Token]) -> Vec<String> {
    let text = |i: usize| tokens.get(i).map_or("", |t| t.text.as_str());
    (0..tokens.len())
        .filter(|&i| text(i) == "verifier" && text(i + 1) == ":" && text(i + 2) == ":")
        .filter(|&i| !VERIFIER_ATTRIBUTES.contains(&text(i + 3)))
        .map(|i| {
            format!(
                "{COPY} line {}: `verifier::{}` is not one of {VERIFIER_ATTRIBUTES:?}",
                tokens[i].line,
                text(i + 3)
            )
        })
        .collect()
}

/// Errors for the [`FORBIDDEN`] identifiers in `tokens` of `file`.
fn forbidden(tokens: &[Token], file: &str) -> Vec<String> {
    tokens
        .iter()
        .filter(|t| FORBIDDEN.contains(&t.text.as_str()))
        .map(|t| {
            format!(
                "{file} line {}: `{}` can change what the code means; do not use it",
                t.line, t.text
            )
        })
        .collect()
}

/// The SHA-256 of the statements in the copy: each `spec fn` item, and
/// each `pub fn` from `pub` to its body (signature and clauses). Proof
/// bodies, private lemmas and loop invariants are not in it.
pub fn spec_digest(tokens: &[Token]) -> Result<String> {
    let text = |i: usize| tokens.get(i).map_or("", |t| t.text.as_str());
    let mut parts: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let mut j = i;
        while MODIFIERS.contains(&text(j)) || matches!(text(j), "const" | "spec" | "proof") {
            j += 1;
        }
        let is_fn = text(j) == "fn" && j > i;
        let end = if is_fn && text(j - 1) == "spec" {
            item_end(tokens, j)?
        } else if is_fn && text(i) == "pub" {
            find_stop(tokens, j, &["{", ";"])?
        } else {
            i += 1;
            continue;
        };
        parts.extend(tokens[i..end].iter().map(|t| t.text.as_str()));
        parts.push("\n");
        i = end;
    }
    Ok(hash::sha256(parts.join(" ")))
}

/// Removes each `,` before `}` or `]`, and before a `)` whose group has
/// another comma. rustfmt adds and removes these. A one-element tuple
/// `(x,)` keeps its comma.
pub fn no_trailing_commas(tokens: &[Token]) -> Vec<Token> {
    // The number of commas directly in the group that each `)` closes.
    let mut commas = vec![0usize; tokens.len()];
    let mut count: Vec<usize> = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        match t.text.as_str() {
            "(" | "[" | "{" => count.push(0),
            ")" | "]" | "}" => commas[i] = count.pop().unwrap_or(0),
            "," => {
                if let Some(n) = count.last_mut() {
                    *n += 1;
                }
            }
            _ => {}
        }
    }
    tokens
        .iter()
        .enumerate()
        .filter(|&(i, t)| {
            let next = tokens.get(i + 1).map_or("", |n| n.text.as_str());
            let trailing = matches!(next, "}" | "]") || (next == ")" && commas[i + 1] >= 2);
            t.text != "," || !trailing
        })
        .map(|(_, t)| t.clone())
        .collect()
}

/// Checks that the copy erases to the shipped scanner. Returns the error
/// lines, empty if the two agree.
pub fn compare(copy: &str, shipped: &str) -> Result<Vec<String>> {
    let copy = tokens(copy).with_context(|| format!("{COPY}: cannot read the tokens"))?;
    let shipped = tokens(shipped).with_context(|| format!("{SHIPPED}: cannot read the tokens"))?;
    let mut errors: Vec<String> = unsound(&copy)
        .iter()
        .map(|t| {
            format!(
                "{COPY} line {}: `{}` lets the proof skip a step",
                t.line, t.text
            )
        })
        .collect();
    errors.extend(structure(&copy)?);
    errors.extend(verifier_attributes(&copy));
    errors.extend(forbidden(&copy, COPY));
    errors.extend(forbidden(&shipped, SHIPPED));
    if !errors.is_empty() {
        return Ok(errors);
    }
    let erased = erase(&copy).with_context(|| format!("{COPY}: cannot erase the ghost code"))?;
    let (erased, shipped) = (no_trailing_commas(&erased), no_trailing_commas(&shipped));
    let at = |tokens: &[Token], i: usize, file: &str| {
        tokens.get(i).map_or_else(
            || format!("the end of {file}"),
            |t| format!("{file} line {}: `{}`", t.line, t.text),
        )
    };
    let key = |t: &Token| (t.text.clone(), t.joint);
    let first = (0..erased.len().max(shipped.len()))
        .find(|&i| erased.get(i).map(key) != shipped.get(i).map(key));
    Ok(first
        .map(|i| {
            format!(
                "first difference: {} is not {}",
                at(&erased, i, COPY),
                at(&shipped, i, SHIPPED)
            )
        })
        .into_iter()
        .collect())
}

/// Runs the copy check on the files at `root`.
pub fn check_copy(root: &Path) -> Result<()> {
    let copy = fs::read_string(root.join(COPY))?;
    let shipped = fs::read_string(root.join(SHIPPED))?;
    let errors = compare(&copy, &shipped)?;
    for line in &errors {
        println!("{line}");
    }
    ensure!(
        errors.is_empty(),
        "verus-copy failed: {COPY} must be {SHIPPED} plus ghost code only"
    );
    let digest = spec_digest(&tokens(&copy)?)?;
    ensure!(
        digest == SPEC_SHA256,
        "verus-copy failed: the statements in {COPY} changed. Review them, \
         then set SPEC_SHA256 in xtask/src/verus.rs to {digest}"
    );
    println!("verus-copy: {COPY} erases to {SHIPPED}; its statements are the pinned ones");
    Ok(())
}

/// Runs the copy check, then Verus on the copy. With no `verus`, it
/// downloads [`RELEASE`] to `target/verus/` (x86-64 Linux only).
pub fn run(root: &Path, verus: Option<&Path>) -> Result<()> {
    check_copy(root)?;
    let verus = match verus {
        Some(path) => std::fs::canonicalize(path)
            .with_context(|| format!("no Verus program at {}", path.display()))?,
        None => install(&root.join("target/verus"))?,
    };
    sh::cmd(&verus)
        .args(verus_args())
        .dir(root)
        .echo()
        .run()
        .context("Verus did not verify the scanner")
}

/// The arguments of `verus`. `--no-cheating` rejects `assume`,
/// `external_body` and the other ways to skip a step.
pub const fn verus_args() -> [&'static str; 4] {
    ["--crate-type", "lib", "--no-cheating", COPY]
}

/// The download URL of [`RELEASE`] for x86-64 Linux.
pub fn release_url() -> String {
    format!(
        "https://github.com/verus-lang/verus/releases/download/release/{RELEASE}/{}",
        zip_name()
    )
}

fn zip_name() -> String {
    format!("verus-{RELEASE}-x86-linux.zip")
}

/// Downloads and unpacks [`RELEASE`] below `dir` if it is not there, and
/// installs its Rust toolchain. Returns the path of the `verus` program.
fn install(dir: &Path) -> Result<std::path::PathBuf> {
    ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "the download is for x86-64 Linux only: give --verus <path>"
    );
    let unpacked = dir.join(RELEASE);
    let verus = unpacked.join("verus-x86-linux/verus");
    if !verus.exists() {
        fs::create_dir_all(dir)?;
        let zip = dir.join(zip_name());
        let part = dir.join(format!("{}.part", zip_name()));
        println!("download {}", release_url());
        http::Client::new()
            .timeout(std::time::Duration::from_secs(300))
            .retry(http::Retry::new(3))
            .download(&release_url(), &part)?;
        let checked = hash::verify_sha256_file(&part, SHA256);
        if checked.is_err() {
            fs::remove_if_exists(&part)?;
        }
        checked.context("the release is not the pinned one: check RELEASE and SHA256")?;
        fs::move_path(&part, &zip)?;
        // Unpack to a temporary directory, then rename: a stopped unpack
        // leaves no `verus` program.
        let tmp = dir.join(format!("{RELEASE}.tmp"));
        fs::remove_if_exists(&tmp)?;
        fs::remove_if_exists(&unpacked)?;
        archive::extract_zip(&zip, &tmp)?;
        make_executable(&tmp.join("verus-x86-linux"))?;
        fs::move_path(&tmp, &unpacked)?;
    }
    sh::cmd("rustup")
        .args(["toolchain", "install", TOOLCHAIN, "--profile", "minimal"])
        .echo()
        .run()?;
    Ok(verus)
}

/// Sets the execute bits of the files in `dir` that have no extension, in
/// case the unpack did not keep them.
#[cfg(unix)]
fn make_executable(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for entry in std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_file() && path.extension().is_none() {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .with_context(|| format!("cannot make {} executable", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_dir: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str) -> Vec<String> {
        tokens(src).unwrap().into_iter().map(|t| t.text).collect()
    }

    fn erased(src: &str) -> Vec<String> {
        erase(&tokens(src).unwrap())
            .unwrap()
            .into_iter()
            .map(|t| t.text)
            .collect()
    }

    #[test]
    fn tokens_skip_comments_and_whitespace() {
        assert_eq!(
            texts("fn a() // x\n/* y /* z */ */ { b }"),
            ["fn", "a", "(", ")", "{", "b", "}"]
        );
        assert_eq!(texts("//! doc\n/// doc\nx"), ["x"]);
    }

    #[test]
    fn tokens_keep_literals_whole() {
        assert_eq!(
            texts(r#"'a' '\'' '\u{a0}' "a \" b" 'x: &'static str"#),
            [
                "'a'",
                r"'\''",
                r"'\u{a0}'",
                r#""a \" b""#,
                "'x",
                ":",
                "&",
                "'static",
                "str"
            ]
        );
        assert_eq!(texts("r#\"a\"b\"# x"), ["r#\"a\"b\"#", "x"]);
        assert_eq!(texts("a_1 + 0x1F"), ["a_1", "+", "0x1F"]);
    }

    #[test]
    fn tokens_have_lines() {
        let t = tokens("a\n\n/* \n */ b").unwrap();
        assert_eq!((t[0].line, t[1].line), (1, 4));
    }

    #[test]
    fn an_unclosed_literal_or_comment_is_an_error() {
        assert!(tokens("\"abc").is_err());
        assert!(tokens("/* abc").is_err());
        assert!(tokens("'\\").is_err());
    }

    #[test]
    fn erase_removes_the_verus_wrapper_and_imports() {
        assert_eq!(
            erased("use vstd::prelude::*;\nverus! {\nfn a() {}\n} // verus!"),
            ["fn", "a", "(", ")", "{", "}"]
        );
    }

    #[test]
    fn erase_removes_spec_and_proof_items() {
        let src = "pub open spec fn s(x: int) -> bool { x > 0 }\n\
                   proof fn p() ensures true, { assert(true); }\n\
                   broadcast proof fn q() {}\n\
                   pub closed spec fn r() -> int;\n\
                   fn e() {}";
        assert_eq!(erased(src), ["fn", "e", "(", ")", "{", "}"]);
    }

    #[test]
    fn erase_removes_clauses() {
        let src = "fn f(t: &[char]) -> (r: Result<Vec<u8>, u8>)\n\
                   requires t.len() > 0,\n\
                   ensures r matches Ok(v) ==> v@.len() == 0,\n\
                   { while i < 1 invariant i <= 1, decreases 1 - i, { i += 1; } }";
        assert_eq!(
            erased(src),
            texts(
                "fn f(t: &[char]) -> Result<Vec<u8>, u8> \
                 { while i < 1 { i += 1; } }"
            )
        );
    }

    #[test]
    fn erase_keeps_a_tuple_return_type() {
        assert_eq!(
            erased("fn f() -> (u8, u8) {}"),
            texts("fn f() -> (u8, u8) {}")
        );
    }

    #[test]
    fn erase_removes_proof_blocks_and_verifier_attributes() {
        let src = "#[verifier::loop_isolation(false)]\n\
                   fn f() { let a = 1; proof { assert(a == 1) by { lemma(); } } a; }";
        assert_eq!(erased(src), texts("fn f() { let a = 1; a; }"));
    }

    #[test]
    fn erase_keeps_other_attributes() {
        assert_eq!(
            erased("#[derive(Debug)] #[allow(clippy::x)] struct A;"),
            texts("#[derive(Debug)] #[allow(clippy::x)] struct A;")
        );
    }

    #[test]
    fn erase_removes_ghost_lets() {
        assert_eq!(
            erased("fn f() { let ghost g = t@; let x = 1; }"),
            texts("fn f() { let x = 1; }")
        );
    }

    #[test]
    fn erase_fails_on_an_unclosed_brace() {
        assert!(erase(&tokens("verus! { fn a() {").unwrap()).is_err());
        assert!(erase(&tokens("proof fn a() {").unwrap()).is_err());
    }

    #[test]
    fn unsound_finds_assume_admit_and_external() {
        let src = "proof fn a() { assume(false); admit(); }\n\
                   #[verifier::external_body]\nfn b() {}\n\
                   #[verifier::external] fn c() {}\n\
                   pub assume_specification[ f ](x: u8);";
        let found: Vec<(String, usize)> = unsound(&tokens(src).unwrap())
            .into_iter()
            .map(|t| (t.text, t.line))
            .collect();
        assert_eq!(
            found,
            [
                ("assume".to_owned(), 1),
                ("admit".to_owned(), 1),
                ("external_body".to_owned(), 2),
                ("external".to_owned(), 4),
                ("assume_specification".to_owned(), 5),
            ]
        );
    }

    #[test]
    fn compare_accepts_a_copy() {
        let shipped = "/// Doc.\npub fn f(c: char) -> bool { c == ' ' }\n";
        let copy = "use vstd::prelude::*;\nverus! {\n\
                    pub open spec fn sep(c: char) -> bool { c == ' ' }\n\
                    /// Other doc.\n\
                    pub fn f(c: char) -> (r: bool)\n    ensures r == sep(c),\n\
                    { c == ' ' }\n}\n";
        assert_eq!(compare(copy, shipped).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn compare_finds_a_changed_token() {
        let shipped = "pub fn f(c: char) -> bool {\n    c == ' '\n}\n";
        let copy =
            "use vstd::prelude::*;\nverus! {\npub fn f(c: char) -> bool {\n    c == '\\t'\n}\n}\n";
        let errors = compare(copy, shipped).unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("line 4"), "{errors:?}");
        assert!(errors[0].contains("line 2"), "{errors:?}");
        assert!(
            errors[0].contains("'\\t'") && errors[0].contains("' '"),
            "{errors:?}"
        );
    }

    #[test]
    fn compare_ignores_trailing_commas() {
        let shipped = "enum A { B { c: u8, d: u8, }, }\nfn f(x: u8, y: u8,) {}";
        let copy = &format!("{HEAD}enum A {{ B {{ c: u8, d: u8 }} }} fn f(x: u8, y: u8) {{}} }}");
        assert_eq!(compare(copy, shipped).unwrap(), Vec::<String>::new());
        let errors = compare(
            &format!("{HEAD}fn f(x: u8, y: u8) {{}} }}"),
            "fn f(x: u8 y: u8) {}",
        )
        .unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    #[test]
    fn compare_finds_a_missing_end() {
        let errors = compare(&format!("{HEAD}fn f() {{}} }}"), "fn f() {} fn g() {}").unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("end of"), "{errors:?}");
    }

    #[test]
    fn compare_rejects_unsound_tokens() {
        let errors = compare(
            &format!("{HEAD}fn f() {{ proof {{ assume(false); }} }} }}"),
            "fn f() {}",
        )
        .unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("assume"), "{errors:?}");
    }

    /// A copy that must fail the check, with the reason in the error.
    fn rejected(copy: &str, shipped: &str, reason: &str) {
        let errors = compare(copy, shipped).unwrap();
        assert!(
            errors.iter().any(|e| e.contains(reason)),
            "{reason}: {errors:?}"
        );
    }

    const HEAD: &str = "use vstd::prelude::*;\nverus! {\n";

    #[test]
    fn joint_operators_are_not_split_operators() {
        rejected(
            &format!("{HEAD}fn f(a: bool, b: bool) -> bool {{ a && b }} }}"),
            "fn f(a: bool, b: bool) -> bool { a & & b }",
            "first difference",
        );
    }

    #[test]
    fn raw_byte_and_c_strings_are_one_token() {
        assert_eq!(texts(r#"br"\" x"#), [r#"br"\""#, "x"]);
        assert_eq!(texts(r##"br#"a"b"# x"##), [r##"br#"a"b"#"##, "x"]);
        assert_eq!(texts(r#"cr"\" c"\"" x"#), [r#"cr"\""#, r#"c"\"""#, "x"]);
    }

    #[test]
    fn a_one_element_tuple_keeps_its_comma() {
        rejected(
            &format!("{HEAD}fn f(x: u8) -> (u8,) {{ (x,) }} }}"),
            "fn f(x: u8) -> (u8,) { (x) }",
            "first difference",
        );
        let ok = compare(
            &format!("{HEAD}fn f(x: u8, y: u8) -> (u8, u8) {{ (x, y) }} }}"),
            "fn f(x: u8, y: u8,) -> (u8, u8,) { (x, y,) }",
        )
        .unwrap();
        assert_eq!(ok, Vec::<String>::new());
    }

    #[test]
    fn ghost_words_in_a_body_are_not_erased() {
        rejected(
            &format!("{HEAD}fn f(a: usize) -> usize {{ a + requires(g()) + 1 }} }}"),
            "fn f(a: usize) -> usize { a + 1 }",
            "first difference",
        );
        rejected(
            &format!("{HEAD}fn f() -> u8 {{ 1 + proof {{ a: g() }}.a }} }}"),
            "fn f() -> u8 { 1 + .a }",
            "first difference",
        );
    }

    #[test]
    fn the_copy_must_be_one_verus_block() {
        rejected(
            "use vstd::prelude::*;\nfn f() {}\nverus! {}",
            "fn f() {}",
            "one `verus! {}` block",
        );
        rejected(
            "use vstd::foo::bar as Ok;\nverus! { fn f() {} }",
            "fn f() {}",
            "`use vstd::prelude::*;`",
        );
        rejected(
            "use vstd::prelude::*;\nverus! { fn f() {} }\nfn g() {}",
            "fn f() {}",
            "one `verus! {}` block",
        );
    }

    #[test]
    fn items_that_change_the_meaning_of_the_tokens_are_rejected() {
        for word in [
            "cfg",
            "include",
            "mod",
            "crate",
            "super",
            "macro_rules",
            "env",
        ] {
            rejected(
                &format!("{HEAD}fn f() {{ {word} }} }}"),
                &format!("fn f() {{ {word} }}"),
                &format!("`{word}`"),
            );
        }
    }

    #[test]
    fn only_some_verifier_attributes_are_allowed() {
        rejected(
            &format!("{HEAD}#[verifier::assume_termination] fn f() {{}} }}"),
            "fn f() {}",
            "`verifier::assume_termination`",
        );
        let ok = compare(
            &format!("#![verifier::loop_isolation(false)]\n{HEAD}#[verifier::opaque] spec fn s() -> bool {{ true }} fn f() {{}} }}"),
            "fn f() {}",
        )
        .unwrap();
        assert_eq!(ok, Vec::<String>::new());
    }

    #[test]
    fn more_escape_hatches_are_unsound() {
        for word in [
            "assume_new",
            "external_trait_specification",
            "assume_termination",
        ] {
            rejected(
                &format!("{HEAD}fn f() {{ {word} }} }}"),
                &format!("fn f() {{ {word} }}"),
                "skip a step",
            );
        }
    }

    #[test]
    fn the_spec_digest_sees_a_changed_statement() {
        let base = format!(
            "{HEAD}pub open spec fn p(x: u8) -> bool {{ x > 0 }}\n\
                            pub fn f(x: u8) -> (r: u8) ensures p(r), {{ x }}\n\
                            fn g(x: u8) -> (r: u8) ensures r == x, {{ x }} }}"
        );
        let digest = |src: &str| spec_digest(&tokens(src).unwrap()).unwrap();
        let d = digest(&base);
        assert_ne!(d, digest(&base.replace("ensures p(r),", "")));
        assert_ne!(d, digest(&base.replace("x > 0", "x >= 0")));
        assert_eq!(
            d,
            digest(&base.replace("ensures r == x,", "")),
            "private fn"
        );
        assert_eq!(d, digest(&base.replace("{ x }", "{ x + 0 }")), "body");
    }

    #[test]
    fn verus_runs_with_no_cheating() {
        assert!(verus_args().contains(&"--no-cheating"));
        assert!(verus_args().ends_with(&[COPY]));
    }

    #[test]
    fn the_release_pin_is_well_formed() {
        assert_eq!(SHA256.len(), 64);
        assert!(SHA256.bytes().any(|b| b != b'0'), "SHA256 is a placeholder");
        assert!(
            SHA256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert!(
            release_url().ends_with(&format!("/release/{RELEASE}/verus-{RELEASE}-x86-linux.zip"))
        );
        assert!(TOOLCHAIN.ends_with("-x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn the_copy_in_the_repository_is_identical() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        check_copy(&root).unwrap();
    }
}
