//! `cmd!` compile-fail tests: the error rows of the ADR 0007 grammar table.
//!
//! Each case is a file in `tests/ui/` with a `.stderr` snapshot. Read
//! `tests/ui/README.md` before you change a case.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

/// Each case, and text that its compiler output must contain.
const CASES: &[(&str, &[&str])] = &[
    ("template_not_a_literal", &["string literal"]),
    (
        "template_other_literal",
        &["byte string", "C string", "must be a string literal"],
    ),
    (
        "template_empty",
        &["the template is empty", "the program is an empty word"],
    ),
    (
        "whitespace_not_ascii",
        &[
            "U+00A0",
            "U+200B",
            "U+000B",
            "U+2003",
            "U+2028",
            "U+3000",
            "U+FEFF",
            "not ASCII whitespace",
        ],
    ),
    ("nul_character", &["NUL"]),
    ("program_splat", &["cannot be the program"]),
    ("splat_not_whole_word", &["a splat must be a whole word"]),
    (
        "splat_path",
        &["a path is one argument: write `{name}`, not `{name...}`"],
    ),
    ("splat_not_a_list", &["is not a list of arguments"]),
    (
        "quote_apostrophe",
        &["must end the word", "for an apostrophe, quote with \"...\""],
    ),
    (
        "quote_position",
        &["a quote must start a word or follow `=`", "unclosed quote"],
    ),
    (
        "shell_syntax",
        &["cmd! runs one program, not a shell: quote it, or use `smop::sh::shell`"],
    ),
    (
        "placeholder_reserved",
        &[
            "`{}`",
            "`{0}`",
            "`{x:?}`",
            "`{a.b}`",
            "is not a placeholder",
        ],
    ),
    ("placeholder_brace", &["has no `}`", "has no `{`"]),
    ("placeholder_keyword", &["is a keyword"]),
    (
        "arg_not_an_argument",
        &["is not an argument; use `.to_string()`"],
    ),
    (
        "name_not_in_template",
        &["is listed, but the template has no `{extra}`"],
    ),
    (
        "name_shadows_constant",
        &["E0308", "interpreted as a constant"],
    ),
    ("type_not_known", &["E0282", "type annotations needed"]),
    ("name_typo", &["E0425", "verison"]),
    ("result_not_used", &["unused"]),
];

fn ui_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ui")
}

/// Compiles each case and compares the compiler output with its snapshot.
///
/// The output of rustc changes between releases, so this test runs only on
/// the pinned toolchain.
#[test]
#[ignore = "run on the pinned toolchain only: see tests/ui/README.md"]
fn each_case_fails_with_its_snapshot() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}

fn read(name: &str) -> String {
    let path = ui_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Makes sure that an updated snapshot still has the message of its rule.
#[test]
fn each_snapshot_has_the_message_of_its_rule() {
    for (case, expected) in CASES {
        let snapshot = read(&format!("{case}.stderr"));
        for text in *expected {
            assert!(
                snapshot.contains(text),
                "{case}.stderr does not have {text:?}"
            );
        }
    }
}

/// Makes sure that an updated snapshot did not lose the error of a call.
/// Each `cmd!` call of a case gives one error or more.
#[test]
fn each_call_of_a_case_has_an_error() {
    for (case, _) in CASES {
        let source = read(&format!("{case}.rs"));
        let snapshot = read(&format!("{case}.stderr"));
        for call in source.lines().filter(|line| line.contains("cmd!(")) {
            // An error shows the source line that it is for.
            assert!(
                snapshot.contains(call.trim()),
                "{case}.stderr has no error for: {call}"
            );
        }
    }
}

#[test]
fn each_case_file_is_in_the_table() {
    let mut on_disk: Vec<String> = std::fs::read_dir(ui_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut in_table: Vec<&str> = CASES.iter().map(|(case, _)| *case).collect();
    in_table.sort_unstable();
    assert_eq!(on_disk, in_table);
}
