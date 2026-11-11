//! Regular expressions. Needs the `re` feature.
//!
//! This module re-exports the `regex` crate (`re::Regex`, `re::Captures`,
//! `re::escape`, …) and adds helpers that take the pattern as `&str`, as
//! Python's `re` module does.
//!
//! Each helper compiles its pattern again. In a loop, compile the pattern one
//! time with [`compile`] and use the `Regex` methods.
//!
//! An invalid pattern gives an error that quotes the pattern:
//! `Invalid regex pattern 'a(b'`. The regex error is its cause.
//!
//! # Examples
//!
//! ```
//! use smop::re;
//!
//! assert!(re::is_match(r"^\d+$", "42")?);
//! assert_eq!(re::find_all(r"\d+", "a1 b22")?, ["1", "22"]);
//! assert_eq!(re::replace_all(r"(\w+)@", "me@x", "$1 at ")?, "me at x");
//!
//! let caps = re::captures(r"(?<k>\w+)=(?<v>\d+)", "port=80")?;
//! assert_eq!(caps.map(|c| c["v"].to_string()), Some("80".into()));
//!
//! let re = re::compile(r"\s+")?; // compile one time for a loop
//! for line in ["a  b", "c d"] {
//!     assert_eq!(re.split(line).count(), 2);
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::{Context, Result};

pub use regex::*;

/// Compiles `pattern`. Use the result for many matches.
///
/// # Errors
///
/// Returns an error that quotes `pattern` if it is not valid.
pub fn compile(pattern: &str) -> Result<Regex> {
    Regex::new(pattern).with_context(|| format!("Invalid regex pattern '{pattern}'"))
}

/// Returns `true` if `pattern` matches any part of `text`.
///
/// # Errors
///
/// Returns an error that quotes `pattern` if it is not valid.
pub fn is_match(pattern: &str, text: &str) -> Result<bool> {
    Ok(compile(pattern)?.is_match(text))
}

/// Returns the first match of `pattern` in `text`, or `None`.
///
/// # Errors
///
/// Returns an error that quotes `pattern` if it is not valid.
pub fn find<'t>(pattern: &str, text: &'t str) -> Result<Option<&'t str>> {
    Ok(compile(pattern)?.find(text).map(|m| m.as_str()))
}

/// Returns all matches of `pattern` in `text` that do not overlap, in order.
///
/// # Errors
///
/// Returns an error that quotes `pattern` if it is not valid.
pub fn find_all<'t>(pattern: &str, text: &'t str) -> Result<Vec<&'t str>> {
    Ok(compile(pattern)?
        .find_iter(text)
        .map(|m| m.as_str())
        .collect())
}

/// Returns the groups of the first match, or `None`.
///
/// Index the result by number (`caps[1]`) or by name (`caps["key"]`).
///
/// # Errors
///
/// Returns an error that quotes `pattern` if it is not valid.
pub fn captures<'t>(pattern: &str, text: &'t str) -> Result<Option<Captures<'t>>> {
    Ok(compile(pattern)?.captures(text))
}

/// Returns the groups of each match that does not overlap, in order.
///
/// # Errors
///
/// Returns an error that quotes `pattern` if it is not valid.
pub fn captures_all<'t>(pattern: &str, text: &'t str) -> Result<Vec<Captures<'t>>> {
    Ok(compile(pattern)?.captures_iter(text).collect())
}

/// Replaces the first match of `pattern` in `text`.
///
/// `rep` is a string with `$1` or `${name}` group references, or a closure
/// `|caps: &Captures| -> String`. Use [`NoExpand`] for a literal string.
///
/// # Errors
///
/// Returns an error that quotes `pattern` if it is not valid.
pub fn replace(pattern: &str, text: &str, rep: impl Replacer) -> Result<String> {
    Ok(compile(pattern)?.replace(text, rep).into_owned())
}

/// Replaces all matches of `pattern` in `text`. `rep` is as in [`replace`].
///
/// # Errors
///
/// Returns an error that quotes `pattern` if it is not valid.
pub fn replace_all(pattern: &str, text: &str, rep: impl Replacer) -> Result<String> {
    Ok(compile(pattern)?.replace_all(text, rep).into_owned())
}

/// Splits `text` at each match of `pattern`. Empty fields stay.
///
/// # Errors
///
/// Returns an error that quotes `pattern` if it is not valid.
pub fn split<'t>(pattern: &str, text: &'t str) -> Result<Vec<&'t str>> {
    Ok(compile(pattern)?.split(text).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn regex_type_is_reexported() {
        let re = Regex::new(r"\d+").unwrap();
        assert!(re.is_match("a1"));
        assert_eq!(escape("a.b"), r"a\.b");
    }

    #[test]
    fn compile_returns_regex() {
        let re = compile(r"^\w+$").unwrap();
        assert!(re.is_match("abc"));
        assert!(!re.is_match("a b"));
    }

    #[test]
    fn is_match_finds_pattern() {
        assert!(is_match(r"\d", "a1b").unwrap());
        assert!(!is_match(r"\d", "abc").unwrap());
    }

    #[test]
    fn is_match_empty_text() {
        assert!(is_match("^$", "").unwrap());
        assert!(!is_match("a", "").unwrap());
    }

    #[test]
    fn find_returns_first_match() {
        assert_eq!(find(r"\d+", "a12b345").unwrap(), Some("12"));
        assert_eq!(find(r"\d+", "abc").unwrap(), None);
    }

    #[test]
    fn find_all_returns_matches_in_order() {
        assert_eq!(
            find_all(r"\d+", "a1 b22 c333").unwrap(),
            vec!["1", "22", "333"]
        );
    }

    #[test]
    fn find_all_no_match_is_empty() {
        assert!(find_all(r"\d+", "abc").unwrap().is_empty());
    }

    #[test]
    fn captures_gives_numbered_and_named_groups() {
        let caps = captures(r"(?<key>\w+)=(\d+)", "x port=8080 y").unwrap();
        let caps = caps.unwrap();
        assert_eq!(&caps[0], "port=8080");
        assert_eq!(&caps["key"], "port");
        assert_eq!(&caps[2], "8080");
    }

    #[test]
    fn captures_no_match_is_none() {
        assert!(captures(r"(\d+)", "abc").unwrap().is_none());
    }

    #[test]
    fn captures_optional_group_can_be_missing() {
        let caps = captures(r"a(b)?", "a").unwrap().unwrap();
        assert!(caps.get(1).is_none());
    }

    #[test]
    fn captures_all_returns_each_match() {
        let all = captures_all(r"(\w+)=(\d+)", "a=1, b=22").unwrap();
        let pairs: Vec<(&str, &str)> = all
            .iter()
            .map(|c| (c.get(1).unwrap().as_str(), c.get(2).unwrap().as_str()))
            .collect();
        assert_eq!(pairs, vec![("a", "1"), ("b", "22")]);
    }

    #[test]
    fn captures_all_no_match_is_empty() {
        assert!(captures_all(r"(\d)", "abc").unwrap().is_empty());
    }

    #[test]
    fn replace_changes_first_match_only() {
        assert_eq!(replace(r"\d", "a1b2", "#").unwrap(), "a#b2");
    }

    #[test]
    fn replace_all_changes_every_match() {
        assert_eq!(replace_all(r"\d", "a1b2", "#").unwrap(), "a#b#");
    }

    #[test]
    fn replace_all_expands_groups() {
        let out = replace_all(r"(?<k>\w+)=(?<v>\w+)", "a=1 b=2", "$v=$k").unwrap();
        assert_eq!(out, "1=a 2=b");
    }

    #[test]
    fn replace_all_accepts_closure() {
        let out = replace_all(r"\d+", "a1 b22", |c: &Captures<'_>| c[0].len().to_string()).unwrap();
        assert_eq!(out, "a1 b2");
    }

    #[test]
    fn replace_all_no_match_returns_text() {
        assert_eq!(replace_all(r"\d", "abc", "#").unwrap(), "abc");
    }

    #[test]
    fn split_on_pattern() {
        assert_eq!(split(r"[,;]\s*", "a, b;c").unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn split_keeps_empty_fields() {
        assert_eq!(split(",", "a,,b,").unwrap(), vec!["a", "", "b", ""]);
    }

    /// Every helper must reject a bad pattern and quote it.
    #[test]
    fn invalid_pattern_error_quotes_pattern() {
        let bad = "a(b";
        let errors = [
            compile(bad).err(),
            is_match(bad, "x").err(),
            find(bad, "x").err(),
            find_all(bad, "x").err(),
            captures(bad, "x").err(),
            captures_all(bad, "x").err(),
            replace(bad, "x", "y").err(),
            replace_all(bad, "x", "y").err(),
            split(bad, "x").err(),
        ];
        for err in errors {
            let err = err.expect("bad pattern must give an error");
            assert_eq!(err.to_string(), "Invalid regex pattern 'a(b'");
            // The regex detail stays in the chain.
            assert!(format!("{err:#}").contains("unclosed group"), "{err:#}");
            assert!(err.downcast_ref::<Error>().is_some());
        }
    }

    proptest! {
        #[test]
        fn escaped_text_matches_itself(s in "\\PC{1,20}") {
            let pat = escape(&s);
            prop_assert!(is_match(&pat, &s).unwrap());
            prop_assert_eq!(find(&pat, &s).unwrap(), Some(s.as_str()));
            prop_assert_eq!(replace_all(&pat, &s, NoExpand("")).unwrap(), "");
        }

        #[test]
        fn split_then_join_restores_text(s in "[a-c,]{0,20}") {
            prop_assert_eq!(split(",", &s).unwrap().join(","), s);
        }

        #[test]
        fn find_all_agrees_with_regex(s in "[a-c0-9 ]{0,30}") {
            let re = Regex::new(r"\d+").unwrap();
            let want: Vec<&str> = re.find_iter(&s).map(|m| m.as_str()).collect();
            prop_assert_eq!(find_all(r"\d+", &s).unwrap(), want);
        }
    }
}
