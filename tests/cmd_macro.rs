//! `cmd!` argument tests.
//!
//! Each test runs a real process and reads back the arguments that the
//! process received. The process is a shell script that prints them with
//! `printf`, so the tests are for Unix.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use proptest::prelude::*;
use smop::prelude::*;

/// Writes a script at `path` that prints each of its arguments, then a NUL.
///
/// The script uses the `printf` of the shell. A `printf` program can read
/// an argument as an option: one implementation removes a first `--`.
///
/// A child process writes the file. If this process has the file open for
/// writing, a `fork` in a different test thread copies the descriptor. Then
/// the start of the script fails with "Text file busy". The child renames
/// the complete file into position, so no process starts a partial script.
fn write_script(path: &Path) {
    let script = r#"for arg in "$@"; do printf "%s\0" "$arg"; done"#;
    sh::shell(r#"printf '%s\n' '#!/bin/sh' "$2" > "$1.$$" && chmod +x "$1.$$" && mv "$1.$$" "$1""#)
        .args([OsStr::new("sh"), path.as_os_str(), OsStr::new(script)])
        .run()
        .unwrap();
}

/// Gets the path of the script that prints its arguments.
fn dump() -> &'static Path {
    static DUMP: OnceLock<PathBuf> = OnceLock::new();
    DUMP.get_or_init(|| {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cmd-macro-argv.sh");
        write_script(&path);
        path
    })
}

/// Runs a command whose program is the [`dump`] script. Returns the
/// arguments that the script received.
///
/// The last argument is a marker. It tells no arguments from one empty
/// argument.
fn argv(command: sh::CommandBuilder) -> Vec<OsString> {
    let output = command.arg("<end>").output_raw().unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut args: Vec<OsString> = output
        .stdout
        .split(|byte| *byte == 0)
        .map(|bytes| OsString::from_vec(bytes.to_vec()))
        .collect();
    assert_eq!(args.pop().as_deref(), Some(OsStr::new("")));
    assert_eq!(args.pop().as_deref(), Some(OsStr::new("<end>")));
    args
}

/// Values that a shell splits or expands.
fn hostile() -> Vec<OsString> {
    let mut values: Vec<OsString> = [
        "; rm -rf / #",
        "$(id)",
        "a\nb",
        "",
        "{}",
        "'\"",
        "a\u{a0}b",
        "a b  c",
        "`id`",
        "$HOME",
        "~",
        "*",
        "-rf",
        "--",
        "--help",
        "-n",
        "%s\\n",
        "{x}",
        "{x...}",
        "&& false",
    ]
    .map(OsString::from)
    .to_vec();
    values.push(OsString::from_vec(b"\xff\xfe a\x80".to_vec()));
    values
}

// The grammar table of ADR 0007: the valid rows.

#[test]
fn ascii_whitespace_separates_words() {
    let dump = dump();
    assert_eq!(argv(cmd!("{dump} a b")), ["a", "b"]);
    assert_eq!(
        argv(cmd!(
            "{dump} cargo test
                --workspace\t-q\r\n\x0Cend "
        )),
        ["cargo", "test", "--workspace", "-q", "end"]
    );
}

#[test]
fn a_raw_string_is_a_template() {
    let dump = dump();
    assert_eq!(argv(cmd!(r"{dump} a\b")), [r"a\b"]);
    assert_eq!(argv(cmd!(r#"{dump} "a b" c"#)), ["a b", "c"]);
}

#[test]
fn a_line_continuation_joins_the_lines() {
    let dump = dump();
    assert_eq!(
        argv(cmd!(
            "{dump} --long-\
             option"
        )),
        ["--long-option"]
    );
}

#[test]
fn the_program_can_be_a_local() {
    let program = dump();
    assert_eq!(argv(cmd!("{program} fmt")), ["fmt"]);

    let (dir, name) = (program.parent().unwrap(), program.file_name().unwrap());
    assert_eq!(argv(cmd!("{dir}/{name} fmt")), ["fmt"]);
    assert_eq!(argv(cmd!("{dir}/cmd-macro-argv.sh fmt")), ["fmt"]);
}

#[test]
fn a_placeholder_is_one_argument() {
    let dump = dump();
    let msg = "two words";
    assert_eq!(argv(cmd!("{dump} -m {msg} --")), ["-m", "two words", "--"]);
}

#[test]
fn a_placeholder_can_be_glued_to_text() {
    let dump = dump();
    let (dir, user, host, path) = ("my dir", "me", "example.org", "/srv/a b");
    assert_eq!(
        argv(cmd!("{dump} --out={dir} {user}@{host}:{path}")),
        ["--out=my dir", "me@example.org:/srv/a b"]
    );
}

#[test]
fn two_placeholders_glue_into_one_argument() {
    let dump = dump();
    let (a, b) = ("x y", "z");
    assert_eq!(argv(cmd!("{dump} {a}{b}")), ["x yz"]);
}

#[test]
fn a_placeholder_accepts_os_string_types() {
    let dump = dump();
    let string = String::from("string");
    let str_ref = "str";
    let path_buf = PathBuf::from("path/buf");
    let path = Path::new("a path");
    let os_string = OsString::from("os string");
    let os_str = OsStr::new("os str");
    let cow: Cow<'_, OsStr> = Cow::Borrowed(OsStr::new("cow"));
    let string_ref = &string;
    assert_eq!(
        argv(cmd!(
            "{dump} {string} {str_ref} {path_buf} {path} {os_string} {os_str} {cow} \
             {string_ref}"
        )),
        [
            "string",
            "str",
            "path/buf",
            "a path",
            "os string",
            "os str",
            "cow",
            "string"
        ]
    );
}

#[test]
fn a_placeholder_does_not_change_non_utf8_bytes() {
    let dump = dump();
    let raw = OsString::from_vec(b"\xff\xfe\x80".to_vec());
    let got = argv(cmd!("{dump} {raw} --x={raw} '{raw} {raw}'"));
    assert_eq!(got[0].as_bytes(), b"\xff\xfe\x80");
    assert_eq!(got[1].as_bytes(), b"--x=\xff\xfe\x80");
    assert_eq!(got[2].as_bytes(), b"\xff\xfe\x80 \xff\xfe\x80");
}

#[test]
fn a_placeholder_accepts_integers_and_char() {
    let dump = dump();
    let jobs = 8_usize;
    assert_eq!(argv(cmd!("{dump} -j{jobs}")), ["-j8"]);

    let (small, short, long, huge, letter) = (-1_i8, 2_u16, -3_i64, u128::MAX, 'x');
    let untyped = 7;
    assert_eq!(
        argv(cmd!(
            "{dump} {small} {short} {long} {huge} {letter} {untyped} -n{untyped}{letter}"
        )),
        [
            "-1",
            "2",
            "-3",
            "340282366920938463463374607431768211455",
            "x",
            "7",
            "-n7x"
        ]
    );
}

#[test]
fn a_splat_gives_one_argument_for_each_item() {
    let dump = dump();
    let files = vec![String::from("a b"), String::from("c")];
    let array = ["x", "y z"];
    assert_eq!(
        argv(cmd!("{dump} -- {files...} {array...}")),
        ["--", "a b", "c", "x", "y z"]
    );
}

#[test]
fn a_splat_of_an_option_gives_zero_or_one_argument() {
    let dump = dump();
    let verbose: Option<&str> = Some("--verbose");
    let quiet: Option<&str> = None;
    assert_eq!(
        argv(cmd!("{dump} build {verbose...} {quiet...}")),
        ["build", "--verbose"]
    );
}

#[test]
fn a_splat_of_an_empty_list_gives_no_argument() {
    let dump = dump();
    let none: Vec<String> = Vec::new();
    assert_eq!(argv(cmd!("{dump} {none...}")), [] as [&str; 0]);
}

#[test]
fn a_splat_accepts_other_collections() {
    let dump = dump();
    let set: BTreeSet<&str> = ["b", "a"].into();
    let hash: HashSet<String> = [String::from("only")].into();
    let deque: VecDeque<PathBuf> = [PathBuf::from("p q")].into();
    let boxed: Box<[OsString]> = Box::new([OsString::from("os")]);
    let paths = vec![PathBuf::from("/usr/local/lib")];
    assert_eq!(
        argv(cmd!(
            "{dump} {set...} {hash...} {deque...} {boxed...} {paths...}"
        )),
        ["a", "b", "only", "p q", "os", "/usr/local/lib"]
    );
}

fn with_slice_parameters(slice: &[&str], vec: &Vec<String>) -> Vec<OsString> {
    let dump = dump();
    argv(cmd!("{dump} {slice...} {vec...}"))
}

#[test]
fn a_splat_looks_through_references() {
    let dump = dump();
    let vec = vec![String::from("c d")];
    assert_eq!(with_slice_parameters(&["a", "b"], &vec), ["a", "b", "c d"]);

    let slice: &[&str] = &["e"];
    let twice = &slice;
    let thrice = &twice;
    assert_eq!(
        argv(cmd!("{dump} {slice...} {twice...} {thrice...}")),
        ["e", "e", "e"]
    );
}

#[test]
fn a_splat_accepts_a_listed_reference() {
    let dump = dump();
    let files = vec!["a", "b"];
    let slice: &[&str] = &files;
    assert_eq!(
        argv(cmd!(
            "{dump} {x...} {y...} {z...}",
            x = &files,
            y = slice,
            z = &slice,
        )),
        ["a", "b", "a", "b", "a", "b"]
    );
    assert_eq!(
        argv(cmd!("{dump} {x...}", x = ["new", "list"])),
        ["new", "list"]
    );
}

#[test]
fn quotes_group_words() {
    let dump = dump();
    let v = "1.2.0";
    assert_eq!(
        argv(cmd!(
            "{dump} -m 'Release {v}' \"a  b\" --msg='c d' --x=\"{v} {v}\""
        )),
        [
            "-m",
            "Release 1.2.0",
            "a  b",
            "--msg=c d",
            "--x=1.2.0 1.2.0"
        ]
    );
}

#[test]
fn an_empty_quote_pair_is_an_empty_argument() {
    let dump = dump();
    assert_eq!(argv(cmd!("{dump} '' \"\" --x=''")), ["", "", "--x="]);
}

#[test]
fn a_quote_has_no_backslash_escapes() {
    let dump = dump();
    assert_eq!(argv(cmd!(r#"{dump} 'a\nb' "c\""#)), [r"a\nb", r"c\"]);
}

#[test]
fn a_quote_can_hold_the_other_quote_character() {
    let dump = dump();
    assert_eq!(
        argv(cmd!("{dump} \"don't\" 'say \"hi\"'")),
        ["don't", "say \"hi\""]
    );
}

#[test]
fn doubled_braces_are_literal_braces() {
    let dump = dump();
    assert_eq!(
        argv(cmd!("{dump} . -exec rm {{}} ';'")),
        [".", "-exec", "rm", "{}", ";"]
    );
    let x = "v";
    assert_eq!(
        argv(cmd!("{dump} {{}} {{{x}}} }}{{ '{{{x}}} }}{{'")),
        ["{}", "{v}", "}{", "{v} }{"]
    );
}

#[test]
fn quoted_shell_syntax_is_text() {
    let dump = dump();
    assert_eq!(
        argv(cmd!(
            "{dump} '&&' \"$HOME\" '~' '#' --a='b;c' '`id`' '2>/dev/null' '|'"
        )),
        [
            "&&",
            "$HOME",
            "~",
            "#",
            "--a=b;c",
            "`id`",
            "2>/dev/null",
            "|"
        ]
    );
}

#[test]
fn a_glob_is_literal_text() {
    let dump = dump();
    assert_eq!(
        argv(cmd!("{dump} *.rs src/**/? [a-z]")),
        ["*.rs", "src/**/?", "[a-z]"]
    );
}

#[test]
fn the_result_is_the_command_builder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let pwd: String = cmd!("pwd").dir(&root).output().unwrap();
    assert_eq!(Path::new(&pwd), root);

    let var = "SMOP_CMD_MACRO_TEST";
    let value: String = cmd!("printenv {var}").env(var, "a b").output().unwrap();
    assert_eq!(value, "a b");

    let builder: sh::CommandBuilder = cmd!("git commit -m {m}", m = "a b");
    assert_eq!(builder.to_string(), "git commit -m \"a b\"");

    let err = cmd!("false").run().unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(1));
}

// The regression cases of ADR 0007.

#[test]
fn a_hostile_value_is_one_argument() {
    let dump = dump();
    for value in hostile() {
        let expected = std::slice::from_ref(&value);
        assert_eq!(argv(cmd!("{dump} {value}")), expected, "{value:?}");
        assert_eq!(argv(cmd!("{dump} {v}", v = &value)), expected, "{value:?}");
    }
}

#[test]
fn a_hostile_value_glued_to_text_is_one_argument() {
    let dump = dump();
    for value in hostile() {
        let mut expected = OsString::from("--x=");
        expected.push(&value);
        expected.push("/");
        expected.push(&value);
        assert_eq!(
            argv(cmd!("{dump} --x={value}/{value}")),
            [expected],
            "{value:?}"
        );
    }
}

#[test]
fn a_hostile_value_in_quotes_is_one_argument() {
    let dump = dump();
    for value in hostile() {
        let mut expected = OsString::from("a ");
        expected.push(&value);
        expected.push(" b");
        assert_eq!(
            argv(cmd!("{dump} 'a {value} b' \"a {value} b\"")),
            [expected.clone(), expected],
            "{value:?}"
        );
    }
}

#[test]
fn hostile_values_in_a_splat_are_one_argument_each() {
    let dump = dump();
    let values = hostile();
    assert_eq!(argv(cmd!("{dump} {values...}")), values);
}

/// Makes a script that prints its arguments, at a path that ends in `name`.
fn script_named(root: &Path, name: &OsStr) -> PathBuf {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_script(&path);
    path
}

#[test]
fn a_hostile_value_is_one_program() {
    let dir = tempfile::tempdir().unwrap();
    for (index, value) in hostile().into_iter().enumerate() {
        // A file name cannot be empty. Some file systems refuse bad UTF-8.
        if value.is_empty() || (cfg!(not(target_os = "linux")) && value.to_str().is_none()) {
            continue;
        }
        let root = dir.path().join(index.to_string());
        let program = script_named(&root, &value);
        assert_eq!(argv(cmd!("{program} a")), ["a"], "{value:?}");
        assert_eq!(argv(cmd!("{root}/{value} b")), ["b"], "{value:?}");
    }
}

#[test]
fn a_hostile_program_does_not_go_through_a_shell() {
    for value in ["; touch smop-cmd-pwned #", "$(touch smop-cmd-pwned)", ""] {
        let dir = tempfile::tempdir().unwrap();
        let err = cmd!("{value} a").dir(dir.path()).run().unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(127), "{value:?}: {err:#}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

#[test]
fn each_listed_value_is_evaluated_one_time_in_the_listed_order() {
    let dump = dump();
    let log = RefCell::new(Vec::new());
    let note = |name: &'static str| {
        log.borrow_mut().push(name);
        name
    };
    let got = argv(cmd!(
        "{dump} {c} {a} {a}{b} '{c}'",
        a = note("a"),
        b = note("b"),
        c = note("c"),
    ));
    assert_eq!(got, ["c", "a", "ab", "c"]);
    assert_eq!(*log.borrow(), ["a", "b", "c"]);
}

#[test]
fn a_listed_value_is_borrowed() {
    let dump = dump();
    let owned = String::from("kept");
    let list = vec![String::from("also kept")];
    assert_eq!(
        argv(cmd!("{dump} {a} {b...}", a = owned, b = list)),
        ["kept", "also kept"]
    );
    assert_eq!(owned, "kept");
    assert_eq!(list.len(), 1);
}

#[test]
fn a_listed_value_can_be_a_temporary() {
    let dump = dump();
    let dir = Path::new("/tmp");
    assert_eq!(
        argv(cmd!(
            "{dump} {file} {n} {items...}",
            file = dir.join("a b"),
            n = 2 + 2,
            items = "x,y".split(',').collect::<Vec<_>>(),
        )),
        ["/tmp/a b", "4", "x", "y"]
    );
}

#[test]
fn locals_named_as_the_internal_items_do_not_collide() {
    let dump = dump();
    let cmd = "cmd";
    let arg = "arg";
    let args = ["args"];
    let word = "word";
    let sh = "sh";
    let smop = "smop";
    let std = "std";
    let core = "core";
    let __cmd = "__cmd";
    let __private = "__private";
    let __smop_cmd_arg = "__smop_cmd_arg";
    let __smop_cmd_splat = ["__smop_cmd_splat"];
    assert_eq!(
        argv(cmd!(
            "{dump} {cmd} {arg} {args...} {word} {sh} {smop} {std} {core} {__cmd} \
             {__private} --x={__smop_cmd_arg} {__smop_cmd_splat...}"
        )),
        [
            "cmd",
            "arg",
            "args",
            "word",
            "sh",
            "smop",
            "std",
            "core",
            "__cmd",
            "__private",
            "--x=__smop_cmd_arg",
            "__smop_cmd_splat",
        ]
    );
}

#[test]
fn listed_names_as_the_internal_items_do_not_collide() {
    let dump = dump();
    assert_eq!(
        argv(cmd!(
            "{dump} {cmd} {arg} {sh} {args...}",
            cmd = "a",
            arg = "b",
            sh = "c",
            args = ["d"],
        )),
        ["a", "b", "c", "d"]
    );
}

macro_rules! forward {
    ($template:literal) => {
        cmd!($template)
    };
    ($template:literal, $($name:ident = $value:expr),+) => {
        cmd!($template, $($name = $value),+)
    };
}

macro_rules! forward_twice {
    ($($tokens:tt)*) => {
        forward!($($tokens)*)
    };
}

macro_rules! with_inner_local {
    ($value:expr) => {{
        let inner = $value;
        let dump = dump();
        cmd!("{dump} {inner}")
    }};
}

macro_rules! with_inner_listed_name {
    ($template:literal, $value:expr) => {
        cmd!($template, it = $value)
    };
}

#[test]
fn a_forwarding_macro_captures_the_locals_of_the_caller() {
    let dump = dump();
    let msg = "a b";
    let files = ["c d", "e"];
    assert_eq!(
        argv(forward!("{dump} -m {msg} {files...}")),
        ["-m", "a b", "c d", "e"]
    );
    assert_eq!(argv(forward_twice!("{dump} {msg}")), ["a b"]);
    assert_eq!(argv(forward!("{dump} {msg} {n}", n = 1)), ["a b", "1"]);
    assert_eq!(argv(forward_twice!("{dump} {n}", n = msg)), ["a b"]);
}

#[test]
fn a_macro_can_use_its_own_local() {
    assert_eq!(argv(with_inner_local!("x y")), ["x y"]);
}

#[test]
fn a_macro_can_list_a_name_for_the_template_of_the_caller() {
    let dump = dump();
    assert_eq!(argv(with_inner_listed_name!("{dump} {it}", "x y")), ["x y"]);
}

fn generic<T: AsRef<OsStr>>(value: T, list: &[T]) -> Vec<OsString> {
    let dump = dump();
    argv(cmd!("{dump} {value} --x={value} {list...}"))
}

#[test]
fn generic_code_can_use_os_string_types() {
    assert_eq!(generic("a b", &["c"]), ["a b", "--x=a b", "c"]);
    assert_eq!(generic(PathBuf::from("p"), &[]), ["p", "--x=p"]);
}

struct Tool {
    name: String,
}

impl Tool {
    fn run(&self) -> Vec<OsString> {
        let dump = dump();
        let name = &self.name;
        argv(cmd!("{dump} {name} {field}", field = self.name))
    }
}

#[test]
fn a_field_is_listed_by_name() {
    let tool = Tool { name: "t".into() };
    assert_eq!(tool.run(), ["t", "t"]);
}

#[test]
fn the_macro_is_usable_by_its_full_path() {
    let dump = dump();
    let v = "x";
    assert_eq!(argv(smop::cmd!("{dump} {v}")), ["x"]);
    assert_eq!(argv(::smop::cmd!("{dump} {v}",)), ["x"]);
    assert_eq!(argv(smop::cmd!["{dump} {v}", v = 1,]), ["1"]);
}

#[test]
fn the_template_text_has_the_same_argv_on_each_use() {
    let dump = dump();
    for round in 0..3_u8 {
        assert_eq!(argv(cmd!("{dump} {round}")), [round.to_string().as_str()]);
    }
}

/// Bytes of an argument: no NUL, the only byte that an argument cannot hold.
fn any_arg() -> impl Strategy<Value = OsString> {
    prop::collection::vec(1_u8..=255, 0..24).prop_map(OsString::from_vec)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn any_value_is_exactly_one_argument(value in any_arg(), other in any_arg()) {
        let dump = dump();
        let mut glued = OsString::from("--x=");
        glued.push(&value);
        let mut quoted = value.clone();
        quoted.push(" and ");
        quoted.push(&other);
        prop_assert_eq!(
            argv(cmd!("{dump} {value} --x={value} '{value} and {other}' {other}")),
            [value.clone(), glued, quoted, other.clone()]
        );
    }

    #[test]
    fn any_list_is_one_argument_for_each_item(
        list in prop::collection::vec(any_arg(), 0..6),
        last in any_arg(),
    ) {
        let dump = dump();
        let mut expected = list.clone();
        expected.push(last.clone());
        prop_assert_eq!(argv(cmd!("{dump} {list...} {last}")), expected);
    }
}
