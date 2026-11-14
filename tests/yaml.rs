//! `yaml` feature: parser, writer, serde and `fs` functions.

// Exact float values are the point of the float tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use smop::yaml::{self, Value};

fn v(text: &str) -> Value {
    yaml::from_str(text).unwrap_or_else(|e| panic!("{text:?}: {e:#}"))
}

fn err(text: &str) -> String {
    format!("{:#}", yaml::from_str::<Value>(text).unwrap_err())
}

fn s(x: &str) -> Value {
    Value::String(x.to_string())
}

fn map(pairs: &[(&str, Value)]) -> Value {
    Value::Mapping(pairs.iter().map(|(k, v)| (s(k), v.clone())).collect())
}

fn seq(items: &[Value]) -> Value {
    Value::Sequence(items.to_vec())
}

// ---------------------------------------------------------------------------
// Scalars and the core schema
// ---------------------------------------------------------------------------

#[test]
fn core_schema_resolution() {
    for null in ["", "~", "null", "Null", "NULL"] {
        assert_eq!(v(null), Value::Null, "{null:?}");
    }
    for (t, b) in [
        ("true", true),
        ("True", true),
        ("TRUE", true),
        ("false", false),
    ] {
        assert_eq!(v(t), Value::Bool(b));
    }
    for (t, i) in [
        ("0", 0),
        ("-12", -12),
        ("+7", 7),
        ("0o17", 15),
        ("0x1F", 31),
        ("012", 12),
    ] {
        assert_eq!(v(t), Value::Int(i), "{t:?}");
    }
    assert_eq!(v("1.5"), Value::Float(1.5));
    assert_eq!(v("-.5e3"), Value::Float(-500.0));
    assert_eq!(v("1."), Value::Float(1.0));
    assert_eq!(v(".inf"), Value::Float(f64::INFINITY));
    assert_eq!(v("-.Inf"), Value::Float(f64::NEG_INFINITY));
    assert!(matches!(v(".NaN"), Value::Float(f) if f.is_nan()));
    // YAML 1.1 words and forms are strings in 1.2.
    for text in [
        "yes", "no", "on", "off", "y", "1_000", "0b101", "1:20", "nULL", "0x", "+",
    ] {
        assert_eq!(v(text), s(text), "{text:?}");
    }
}

#[test]
fn quoted_scalars_are_strings() {
    assert_eq!(v("'true'"), s("true"));
    assert_eq!(v("\"12\""), s("12"));
    assert_eq!(v("''"), s(""));
    assert_eq!(v("'it''s'"), s("it's"));
}

#[test]
fn double_quoted_escapes() {
    assert_eq!(
        v(r#""a\tb\n\\\"\/\0\a\b\v\f\r\e\ \N\_\L\P\x41\u00e9\U0001F600""#),
        s("a\tb\n\\\"/\0\u{7}\u{8}\u{b}\u{c}\r\u{1b} \u{85}\u{a0}\u{2028}\u{2029}Aé😀")
    );
    assert!(err(r#""\q""#).contains("escape"));
    assert!(err(r#""\x4""#).contains("escape"));
    assert!(err(r#""\uD800""#).contains("escape"));
}

#[test]
fn multi_line_scalars_fold() {
    assert_eq!(v("a\n  b\n\n  c"), s("a b\nc"));
    assert_eq!(
        v("k: one\n  two  \n  three"),
        map(&[("k", s("one two three"))])
    );
    assert_eq!(v("'a\n  b\n\n  c'"), s("a b\nc"));
    assert_eq!(v("\"a\\\n  b\""), s("ab"));
    assert_eq!(v("\"a \\\n  b\""), s("a b"));
    assert_eq!(v("\"a\n\n\n b\""), s("a\n\nb"));
}

#[test]
fn plain_scalar_edges() {
    assert_eq!(v("a#b"), s("a#b"));
    assert_eq!(v("a # comment"), s("a"));
    assert_eq!(v("http://x:80/a"), s("http://x:80/a"));
    assert_eq!(v("-a"), s("-a"));
    assert_eq!(v(":a"), s(":a"));
    assert_eq!(v("a, [b]"), s("a, [b]"));
}

// ---------------------------------------------------------------------------
// Block scalars
// ---------------------------------------------------------------------------

#[test]
fn literal_block_chomping() {
    assert_eq!(v("k: |\n  a\n  b\n\n"), map(&[("k", s("a\nb\n"))]));
    assert_eq!(v("k: |-\n  a\n  b\n\n"), map(&[("k", s("a\nb"))]));
    assert_eq!(v("k: |+\n  a\n  b\n\n"), map(&[("k", s("a\nb\n\n"))]));
    assert_eq!(
        v("k: |\n   a\n  # comment\nx: 1"),
        map(&[("k", s("a\n")), ("x", Value::Int(1))])
    );
}

#[test]
fn literal_block_keeps_indent_and_comments() {
    assert_eq!(
        v("k: |\n    a\n      b\n    # c\n# real\nx: 1"),
        map(&[("k", s("a\n  b\n# c\n")), ("x", Value::Int(1))])
    );
}

#[test]
fn block_indentation_indicator() {
    assert_eq!(v("k: |2\n   a\n  b\n"), map(&[("k", s(" a\nb\n"))]));
    assert_eq!(v("- |1-\n  a\n"), seq(&[s(" a")]));
    assert_eq!(v("--- |2+\n  a\n\n"), s("a\n\n"));
    assert_eq!(v("--- |\nfoo\n"), s("foo\n"));
    assert!(err("k: |\n    a\n  b").contains("line 3"));
    assert!(
        err("k: |\n    \n  a").contains("line"),
        "leading empty line indented more"
    );
}

#[test]
fn folded_block() {
    assert_eq!(v(">\n a\n b\n\n c\n"), s("a b\nc\n"));
    assert_eq!(v(">-\n a\n   more\n b\n"), s("a\n  more\nb"));
    assert_eq!(v(">\n\n a\n"), s("\na\n"));
    assert_eq!(v(">+\n a\n\n"), s("a\n\n"));
    assert_eq!(v("k: >\n  a\n\n\n  b\n"), map(&[("k", s("a\n\nb\n"))]));
}

#[test]
fn empty_block_scalars() {
    assert_eq!(v("k: |\nx: 1"), map(&[("k", s("")), ("x", Value::Int(1))]));
    assert_eq!(
        v("k: |+\n\n\nx: 1"),
        map(&[("k", s("\n\n")), ("x", Value::Int(1))])
    );
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

#[test]
fn block_collections() {
    let text = "\
# top comment
name: smop
list:
  - a
  - b # trailing
nested:
  deep:
    x: 1
same_indent:
- p
- q
empty:
items:
  - name: one
    tags: [x, y]
  - - inner
    - 2
";
    assert_eq!(
        v(text),
        map(&[
            ("name", s("smop")),
            ("list", seq(&[s("a"), s("b")])),
            ("nested", map(&[("deep", map(&[("x", Value::Int(1))]))])),
            ("same_indent", seq(&[s("p"), s("q")])),
            ("empty", Value::Null),
            (
                "items",
                seq(&[
                    map(&[("name", s("one")), ("tags", seq(&[s("x"), s("y")]))]),
                    seq(&[s("inner"), Value::Int(2)]),
                ])
            ),
        ])
    );
}

#[test]
fn flow_collections_and_json() {
    assert_eq!(
        v(r#"{"a":1,"b":[true,null,"x"],"c":{}, "d": []}"#),
        map(&[
            ("a", Value::Int(1)),
            ("b", seq(&[Value::Bool(true), Value::Null, s("x")])),
            ("c", Value::Mapping(vec![])),
            ("d", seq(&[])),
        ])
    );
    assert_eq!(
        v("k: [a b, 'c',\n  d, ]\nm: {x: 1, y, z: }"),
        map(&[
            ("k", seq(&[s("a b"), s("c"), s("d")])),
            (
                "m",
                map(&[("x", Value::Int(1)), ("y", Value::Null), ("z", Value::Null)])
            ),
        ])
    );
    assert_eq!(v("[a:b]"), seq(&[s("a:b")]));
    assert_eq!(v("[http://x]"), seq(&[s("http://x")]));
    assert_eq!(
        v("{a: [1, {b: 2}]} # c"),
        map(&[("a", seq(&[Value::Int(1), map(&[("b", Value::Int(2))])]))])
    );
}

#[test]
fn non_string_keys() {
    assert_eq!(
        v("1: a\ntrue: b\n~: c\n'1': d"),
        Value::Mapping(vec![
            (Value::Int(1), s("a")),
            (Value::Bool(true), s("b")),
            (Value::Null, s("c")),
            (s("1"), s("d")),
        ])
    );
}

#[test]
fn github_workflow_on_is_a_string_key() {
    let wf = v(
        "on:\n  push:\n    branches: [main]\n  pull_request:\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v6\n      - run: |\n          cargo test\n          cargo clippy\n",
    );
    assert_eq!(wf["on"]["push"]["branches"][0], s("main"));
    assert_eq!(wf["on"]["pull_request"], Value::Null);
    assert_eq!(
        wf["jobs"]["test"]["steps"][1]["run"].as_str(),
        Some("cargo test\ncargo clippy\n")
    );
    assert_eq!(wf["missing"]["x"][3], Value::Null);
}

#[test]
fn reads_the_ci_workflow_of_this_repository() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/.github/workflows/ci.yml");
    let wf: Value = smop::fs::read_yaml(path).unwrap();
    assert_eq!(wf["on"]["push"]["branches"][0], s("main"));
    let steps = wf["jobs"]["test"]["steps"].as_sequence().unwrap();
    assert!(steps.iter().any(|step| step["run"].as_str().is_some_and(|r| r.contains("ci-features"))));
}

// ---------------------------------------------------------------------------
// Documents, anchors, tags
// ---------------------------------------------------------------------------

#[test]
fn documents() {
    assert_eq!(
        v("%YAML 1.2\n---\na: 1\n...\n# end\n"),
        map(&[("a", Value::Int(1))])
    );
    assert_eq!(v("--- text\n"), s("text"));
    assert_eq!(v("---\n"), Value::Null);
    assert_eq!(v("# only a comment\n"), Value::Null);
    assert_eq!(
        v("\u{feff}a: 1\r\nb: 2\r\n"),
        map(&[("a", Value::Int(1)), ("b", Value::Int(2))])
    );
    assert!(err("a: 1\n---\nb: 2").contains("one document"));
    assert!(err("%TAG ! tag:x,2000:\n---\na").contains("%TAG"));
}

#[test]
fn anchors_aliases_and_merge_keys() {
    let text = "\
base: &base
  image: rust
  env: &env [a, b]
other: *env
job:
  <<: *base
  image: alpine
multi:
  <<: [{a: 1, b: 1}, {b: 2, c: 2}]
  c: 3
";
    let doc = v(text);
    assert_eq!(doc["other"], seq(&[s("a"), s("b")]));
    assert_eq!(
        doc["job"],
        map(&[("env", seq(&[s("a"), s("b")])), ("image", s("alpine"))])
    );
    assert_eq!(
        doc["multi"],
        map(&[
            ("a", Value::Int(1)),
            ("b", Value::Int(1)),
            ("c", Value::Int(3))
        ])
    );
    assert!(err("a: *nope").contains("unknown anchor"));
    assert!(err("a: &x 1\nb:\n  <<: *x").contains("merge"));
}

#[test]
fn alias_bomb_is_an_error() {
    let mut text = String::from("a0: &a0 [x, x, x, x, x, x, x, x, x, x]\n");
    for i in 1..9 {
        let p = i - 1;
        let _ = writeln!(
            text,
            "a{i}: &a{i} [*a{p}, *a{p}, *a{p}, *a{p}, *a{p}, *a{p}, *a{p}, *a{p}, *a{p}, *a{p}]"
        );
    }
    assert!(err(&text).contains("alias"), "{}", err(&text));
}

#[test]
fn deep_nesting_is_an_error() {
    let text = "[".repeat(1000) + &"]".repeat(1000);
    assert!(err(&text).contains("nesting"));
    let block: String = (0..200).map(|i| " ".repeat(i * 2) + "- \n").collect();
    assert!(err(&block).contains("nesting"));
}

#[test]
fn properties_on_the_next_line() {
    assert_eq!(v("---\n  &a x\n"), s("x"));
    assert_eq!(
        v("k:\n  !!str 1\nj: 2"),
        map(&[("k", s("1")), ("j", Value::Int(2))])
    );
    assert_eq!(
        v("k:\n  &a\n  b: 1\nj: *a"),
        map(&[
            ("k", map(&[("b", Value::Int(1))])),
            ("j", map(&[("b", Value::Int(1))]))
        ])
    );
    assert_eq!(
        v("- \n  &a [1]\n- *a"),
        seq(&[seq(&[Value::Int(1)]), seq(&[Value::Int(1)])])
    );
}

#[test]
fn block_scalar_ends_at_a_document_marker_or_the_end() {
    assert_eq!(v("|\n  \n\n..."), s(""));
    assert_eq!(v("k: |\n  a"), map(&[("k", s("a"))]));
    assert_eq!(v("k: |+\n  a\n\n  "), map(&[("k", s("a\n\n"))]));
    assert_eq!(v("k: >\n  a\n    "), map(&[("k", s("a\n  "))]));
}

#[test]
fn tags() {
    assert_eq!(v("!!str 12"), s("12"));
    assert_eq!(v("! 12"), s("12"));
    assert_eq!(v("!!int \"12\""), Value::Int(12));
    assert_eq!(v("!!float 1"), Value::Float(1.0));
    assert_eq!(v("!!bool 'true'"), Value::Bool(true));
    assert_eq!(v("!!null ''"), Value::Null);
    assert_eq!(v("!!map {a: 1}"), map(&[("a", Value::Int(1))]));
    assert_eq!(v("k: !!seq\n- 1"), map(&[("k", seq(&[Value::Int(1)]))]));
    assert!(err("!!int x").contains("!!int"));
    assert!(err("!Ref x").contains("!Ref"));
    assert!(err("!!binary aGk=").contains("!!binary"));
    assert!(err("!!seq {}").contains("!!seq"));
}

// ---------------------------------------------------------------------------
// Errors: each names the line and column
// ---------------------------------------------------------------------------

#[test]
fn syntax_errors_name_the_position() {
    let cases = [
        ("a: 1\nb: [1, 2\n", "line 3"),
        ("a: 1\n\tb: 2", "tab"),
        ("a: b: c", "line 1, column 5"),
        ("a: 1\na: 2", "duplicate key"),
        ("{a: 1, a: 2}", "duplicate key"),
        ("? a\n: b", "?"),
        ("[a]: b", "key"),
        ("'open", "quoted"),
        ("a:\n  b: 1\n c: 2", "indent"),
        ("- a\nb: 1", "line 2"),
        ("a: 1\n- b", "line 2"),
        ("&a b: 1", "anchor"),
        ("a: 'x' y", "line 1"),
        ("[a: b]", "flow"),
        ("@x", "@"),
        ("{a: 1", "line 1"),
        ("a: |x\n b", "block scalar"),
    ];
    for (text, needle) in cases {
        let e = err(text);
        assert!(e.contains("line "), "{text:?} -> {e}");
        assert!(e.contains(needle), "{text:?} -> {e}");
    }
}

// ---------------------------------------------------------------------------
// serde: typed reads
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Shape {
    Dot,
    Circle(f64),
    Pair(i32, i32),
    Rect { w: u32, h: u32 },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Config {
    name: String,
    version: String,
    port: u16,
    ratio: f32,
    debug: bool,
    tags: Vec<String>,
    limits: BTreeMap<String, i64>,
    maybe: Option<u8>,
    none: Option<String>,
    shapes: Vec<Shape>,
    unit: (),
    tuple: (u8, String),
    ch: char,
}

#[test]
fn typed_read() {
    let text = "\
name: smop
version: 3.10
port: 0x1F90
ratio: 0.5
debug: TRUE
tags: [a, 'b']
limits: {cpu: -2, mem: 0o10}
maybe: 7
none: ~
shapes:
  - Dot
  - Circle: 1.5
  - Pair: [1, -2]
  - Rect: {w: 3, h: 4}
unit: null
tuple: [1, two]
ch: x
";
    let c: Config = yaml::from_str(text).unwrap();
    assert_eq!(c.version, "3.10");
    assert_eq!(c.port, 8080);
    assert_eq!(c.limits["mem"], 8);
    assert_eq!(c.maybe, Some(7));
    assert_eq!(c.none, None);
    assert_eq!(
        c.shapes,
        [
            Shape::Dot,
            Shape::Circle(1.5),
            Shape::Pair(1, -2),
            Shape::Rect { w: 3, h: 4 }
        ]
    );
    assert_eq!(c.tuple, (1, "two".into()));
    let again: Config = yaml::from_str(&yaml::to_string(&c).unwrap()).unwrap();
    assert_eq!(again, c);
}

#[test]
fn typed_errors_name_the_position() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct P {
        port: u8,
    }
    let e = format!("{:#}", yaml::from_str::<P>("\nport: 300").unwrap_err());
    assert!(e.contains("line 2, column 7"), "{e}");
    let e = format!("{:#}", yaml::from_str::<P>("port: '1'").unwrap_err());
    assert!(
        e.contains("line 1, column 7") && e.contains("string"),
        "{e}"
    );
    let e = format!("{:#}", yaml::from_str::<P>("other: 1").unwrap_err());
    assert!(e.contains("line 1, column 1") && e.contains("port"), "{e}");
    let e = format!("{:#}", yaml::from_str::<(u8,)>("[1, 2]").unwrap_err());
    assert!(e.contains("line 1"), "{e}");
    let e = format!("{:#}", yaml::from_str::<Shape>("Nope").unwrap_err());
    assert!(e.contains("Nope"), "{e}");
    let e = format!("{:#}", yaml::from_str::<Shape>("Circle").unwrap_err());
    assert!(e.contains("line 1"), "{e}");
    let e = format!(
        "{:#}",
        yaml::from_str::<Shape>("{Dot: 1, Circle: 2}").unwrap_err()
    );
    assert!(e.contains("one key"), "{e}");
    assert!(yaml::from_str::<bool>("'true'").is_err());
    assert!(yaml::from_str::<i8>("1.5").is_err());
    assert!(yaml::from_str::<char>("ab").is_err());
    assert!(yaml::from_str::<()>("1").is_err());
    assert!(yaml::from_str::<Vec<u8>>("{a: 1}").is_err());
    assert!(yaml::from_str::<BTreeMap<String, u8>>("[1]").is_err());
}

#[test]
fn null_reads_as_an_empty_collection() {
    #[derive(Debug, Default, PartialEq, Deserialize)]
    #[serde(default)]
    struct Push {
        branches: Vec<String>,
    }
    #[derive(Debug, PartialEq, Deserialize)]
    struct On {
        push: Push,
        tags: Vec<String>,
        env: BTreeMap<String, String>,
    }
    let on: On = yaml::from_str("push:\ntags: ~\nenv: null").unwrap();
    assert_eq!(on.push, Push::default());
    assert!(on.tags.is_empty() && on.env.is_empty());
}

#[test]
fn numbers_and_strings_into_typed_targets() {
    assert_eq!(yaml::from_str::<String>("true").unwrap(), "true");
    assert_eq!(yaml::from_str::<String>("~").unwrap(), "~");
    assert_eq!(yaml::from_str::<f64>("3").unwrap(), 3.0);
    assert_eq!(yaml::from_str::<f32>("0x10").unwrap(), 16.0);
    assert_eq!(
        yaml::from_str::<u128>("340282366920938463463374607431768211455").unwrap(),
        u128::MAX
    );
    assert_eq!(yaml::from_str::<i128>("0x10").unwrap(), 16);
    assert!(yaml::from_str::<i128>("-0x10").is_err());
    assert_eq!(
        yaml::from_str::<u64>("18446744073709551615").unwrap(),
        u64::MAX
    );
    assert!(yaml::from_str::<u8>("-1").is_err());
    assert_eq!(yaml::from_str::<Vec<u8>>("[1, 2]").unwrap(), [1, 2]);
    assert_eq!(
        yaml::from_str::<(String, i32)>("[a, 1]").unwrap(),
        ("a".into(), 1)
    );
    let m: BTreeMap<u32, bool> = yaml::from_str("1: true\n2: false").unwrap();
    assert!(!m[&2]);
    // Untyped targets see core-schema values.
    let j: Value = yaml::from_str("18446744073709551615").unwrap();
    assert_eq!(j.as_u64(), Some(u64::MAX));
    assert!(yaml::from_str::<Value>("1000000000000000000000000000000000000000000").is_err());
}

#[test]
fn untagged_and_flatten_use_the_core_schema() {
    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(untagged)]
    enum Num {
        I(i64),
        F(f64),
        S(String),
    }
    #[derive(Debug, PartialEq, Deserialize)]
    struct Outer {
        a: u8,
        #[serde(flatten)]
        rest: BTreeMap<String, Value>,
    }
    let n: Vec<Num> = yaml::from_str("[1, 1.5, x]").unwrap();
    assert_eq!(n, [Num::I(1), Num::F(1.5), Num::S("x".into())]);

    let o: Outer = yaml::from_str("a: 1\nb: [2]").unwrap();
    assert_eq!(o.rest["b"], seq(&[Value::Int(2)]));
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

#[test]
fn writer_layout() {
    #[derive(Serialize)]
    struct Job {
        name: &'static str,
        steps: Vec<BTreeMap<&'static str, &'static str>>,
        env: BTreeMap<&'static str, &'static str>,
        empty: Vec<u8>,
        none: Option<u8>,
        nested: Vec<Vec<u8>>,
        script: &'static str,
    }
    let job = Job {
        name: "test",
        steps: vec![BTreeMap::from([("run", "cargo test"), ("uses", "x@v1")])],
        env: BTreeMap::new(),
        empty: vec![],
        none: None,
        nested: vec![vec![1, 2], vec![]],
        script: "a\nb\n",
    };
    assert_eq!(
        yaml::to_string(&job).unwrap(),
        "\
name: test
steps:
  - run: cargo test
    uses: x@v1
env: {}
empty: []
none: null
nested:
  - - 1
    - 2
  - []
script: |
  a
  b
"
    );
}

#[test]
fn writer_quotes_ambiguous_strings() {
    for text in [
        "",
        "true",
        "yes",
        "No",
        "ON",
        "off",
        "y",
        "~",
        "null",
        "12",
        "1.5",
        "0x1",
        "1_000",
        "1:2",
        ".inf",
        "-1",
        " lead",
        "trail ",
        "a: b",
        "a #b",
        "#x",
        "- x",
        "-",
        "? x",
        "[x",
        "{x",
        "*x",
        "&x",
        "!x",
        "|x",
        ">x",
        "'x",
        "\"x",
        "%x",
        "@x",
        "`x",
        "a\tb",
        "\u{7}",
        "a\u{2028}b",
        "---",
        "...",
        ",x",
    ] {
        let out = yaml::to_string(&text).unwrap();
        assert!(out.starts_with('"'), "{text:?} -> {out}");
        assert_eq!(yaml::from_str::<String>(&out).unwrap(), text, "{out}");
        assert_eq!(v(&out), s(text), "{out}");
    }
    for text in [
        "smop",
        "a b",
        "x@v1",
        "a:b",
        "a#b",
        "ünï",
        "ubuntu-latest",
        "http://x",
    ] {
        assert_eq!(yaml::to_string(&text).unwrap(), format!("{text}\n"));
    }
}

#[test]
fn writer_block_scalars() {
    assert_eq!(yaml::to_string(&"a\nb").unwrap(), "|-\n  a\n  b\n");
    assert_eq!(yaml::to_string(&"a\n\n").unwrap(), "|+\n  a\n\n");
    assert_eq!(yaml::to_string(&" a\nb\n").unwrap(), "|2\n   a\n  b\n");
    assert_eq!(yaml::to_string(&vec!["x\ny"]).unwrap(), "- |-\n  x\n  y\n");
    for text in [
        "a\nb", "\na", "a\n\n", "\n", "\n\n", " a\n", "a\n b", "a \n", "a\r\nb", "a\n\u{1}",
    ] {
        let out = yaml::to_string(&text).unwrap();
        assert_eq!(
            yaml::from_str::<String>(&out).unwrap(),
            text,
            "{text:?} -> {out}"
        );
    }
}

#[test]
fn writer_numbers() {
    assert_eq!(yaml::to_string(&1.0_f64).unwrap(), "1.0\n");
    assert_eq!(yaml::to_string(&1.1_f32).unwrap(), "1.1\n");
    assert_eq!(yaml::to_string(&1e300).unwrap(), "1e300\n");
    assert_eq!(yaml::to_string(&f64::NAN).unwrap(), ".nan\n");
    assert_eq!(yaml::to_string(&f64::NEG_INFINITY).unwrap(), "-.inf\n");
    assert_eq!(
        yaml::to_string(&u128::MAX).unwrap(),
        format!("{}\n", u128::MAX)
    );
    assert_eq!(yaml::to_string(&()).unwrap(), "null\n");
    assert_eq!(yaml::to_string(&'c').unwrap(), "c\n");
    assert_eq!(yaml::to_string(&Shape::Dot).unwrap(), "Dot\n");
    assert_eq!(
        yaml::to_string(&Shape::Circle(2.0)).unwrap(),
        "Circle: 2.0\n"
    );
    assert_eq!(
        yaml::to_string(&Shape::Pair(1, 2)).unwrap(),
        "Pair:\n  - 1\n  - 2\n"
    );
    assert_eq!(
        yaml::to_string(&Shape::Rect { w: 1, h: 2 }).unwrap(),
        "Rect:\n  w: 1\n  h: 2\n"
    );
}

#[test]
fn writer_keys() {
    let m = BTreeMap::from([(1, "a"), (2, "b")]);
    assert_eq!(yaml::to_string(&m).unwrap(), "1: a\n2: b\n");
    let m = BTreeMap::from([("on", 1), ("a\nb", 2)]);
    assert_eq!(yaml::to_string(&m).unwrap(), "\"a\\nb\": 2\n\"on\": 1\n");
    let m = BTreeMap::from([(vec![1], 1)]);
    assert!(format!("{:#}", yaml::to_string(&m).unwrap_err()).contains("key"));
}

// ---------------------------------------------------------------------------
// Value
// ---------------------------------------------------------------------------

#[test]
fn value_accessors() {
    let doc = v("a: [1, x, 1.5, true, -1]");
    let a = &doc["a"];
    assert_eq!(a[0].as_i64(), Some(1));
    assert_eq!(a[0].as_u64(), Some(1));
    assert_eq!(a[4].as_u64(), None);
    assert_eq!(a[0].as_f64(), Some(1.0));
    assert_eq!(a[2].as_f64(), Some(1.5));
    assert_eq!(a[1].as_str(), Some("x"));
    assert_eq!(a[3].as_bool(), Some(true));
    assert_eq!(a.as_sequence().map(<[Value]>::len), Some(5));
    assert!(doc.as_mapping().is_some());
    assert!(doc.get("a").is_some() && doc.get("b").is_none());
    assert!(a.get("a").is_none());
    assert!(doc["b"].is_null());
    assert_eq!(a[1].as_i64(), None);
    assert_eq!(Value::default(), Value::Null);
}

// ---------------------------------------------------------------------------
// Properties: the writer output reads back as the same value
// ---------------------------------------------------------------------------

fn any_string() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<String>(),
        "[ a-z:#\\-'\"\n\t]{0,12}",
        prop::sample::select(vec![
            "", " ", "\n", "true", "no", "~", "1", "-1", ".5", "- a", "a: b", "|", ">", "#", "---",
            "...",
        ])
        .prop_map(String::from),
    ]
}

fn any_scalar() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|i| Value::Int(i128::from(i))),
        any::<u64>().prop_map(|i| Value::Int(i128::from(i))),
        any::<f64>()
            .prop_filter("NaN != NaN", |f| !f.is_nan())
            .prop_map(Value::Float),
        any_string().prop_map(Value::String),
    ]
}

fn any_key() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|i| Value::Int(i128::from(i))),
        any_string().prop_map(Value::String),
    ]
}

fn any_value() -> impl Strategy<Value = Value> {
    any_scalar().prop_recursive(4, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Sequence),
            prop::collection::vec((any_key(), inner), 0..6).prop_map(|pairs| {
                let mut seen = std::collections::HashSet::new();
                Value::Mapping(
                    pairs
                        .into_iter()
                        .filter(|(k, _)| seen.insert(format!("{k:?}")))
                        .collect(),
                )
            }),
        ]
    })
}

/// Text made of YAML tokens, so the parser sees many valid and near-valid documents.
fn yaml_like() -> impl Strategy<Value = String> {
    let tokens = vec![
        "a", "b: ", "c:", "- ", "  ", "\n", "\n  ", "[", "]", "{", "}", ", ", "'x y'", "\"z\\n\"",
        "&a ", "*a", "|", "|-", ">", ">+", "# c", "1", "2.5", "~", ": ", "<<: ", "---", "...",
        "\t", "? ", "!!str ", "''", "#",
    ];
    prop::collection::vec(prop::sample::select(tokens), 0..24).prop_map(|t| t.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn value_round_trip(value in any_value()) {
        let text = yaml::to_string(&value).unwrap();
        let back: Value = yaml::from_str(&text).map_err(|e| TestCaseError::fail(format!("{e:#}\n{text}")))?;
        prop_assert_eq!(back, value, "{}", text);
    }

    #[test]
    fn string_round_trip(text in any_string()) {
        for wrapped in [
            yaml::to_string(&text).unwrap(),
            yaml::to_string(&vec![text.clone()]).unwrap(),
            yaml::to_string(&BTreeMap::from([(text.clone(), text.clone())])).unwrap(),
            yaml::to_string(&vec![BTreeMap::from([("k", vec![text.clone()])])]).unwrap(),
        ] {
            let back: Value = yaml::from_str(&wrapped).map_err(|e| TestCaseError::fail(format!("{e:#}\n{wrapped}")))?;
            let found = match &back {
                Value::String(x) => x.clone(),
                Value::Sequence(items) => match &items[0] {
                    Value::String(x) => x.clone(),
                    other => other["k"][0].as_str().unwrap_or_default().to_string(),
                },
                Value::Mapping(pairs) => {
                    prop_assert_eq!(&pairs[0].0, &Value::String(text.clone()), "{}", wrapped);
                    pairs[0].1.as_str().unwrap_or_default().to_string()
                }
                other => format!("{other:?}"),
            };
            prop_assert_eq!(&found, &text, "{}", wrapped);
        }
    }

    #[test]
    fn f32_round_trip(x in any::<f32>().prop_filter("NaN", |f| !f.is_nan())) {
        let back: f32 = yaml::from_str(&yaml::to_string(&x).unwrap()).unwrap();
        prop_assert_eq!(back.to_bits(), x.to_bits());
    }

    #[test]
    fn parser_never_panics(text in "[ -~\n\t]{0,64}") {
        let _ = yaml::from_str::<Value>(&text);
    }

    #[test]
    fn parsed_documents_round_trip(text in yaml_like()) {
        if let Ok(value) = yaml::from_str::<Value>(&text) {
            let out = yaml::to_string(&value).unwrap();
            let back: Value = yaml::from_str(&out).map_err(|e| TestCaseError::fail(format!("{e:#}\n{out}")))?;
            prop_assert_eq!(back, value, "{:?} -> {}", text, out);
        }
    }
}

// ---------------------------------------------------------------------------
// fs
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Record {
    name: String,
    score: u32,
    tags: Vec<String>,
}

#[test]
fn fs_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r.yaml");
    let record = Record {
        name: "a: b".into(),
        score: 7,
        tags: vec!["yes".into(), "multi\nline".into()],
    };
    smop::fs::write_yaml(&path, &record).unwrap();
    let back: Record = smop::fs::read_yaml(&path).unwrap();
    assert_eq!(back, record);
}

#[test]
fn fs_errors_name_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.yaml");
    std::fs::write(&path, "name: a\nscore: x\ntags: []\n").unwrap();
    let e = format!("{:#}", smop::fs::read_yaml::<Record, _>(&path).unwrap_err());
    assert!(e.contains(&path.display().to_string()), "{e}");
    assert!(e.contains("line 2, column 8"), "{e}");

    let missing = dir.path().join("missing.yaml");
    let e = format!(
        "{:#}",
        smop::fs::read_yaml::<Record, _>(&missing).unwrap_err()
    );
    assert!(e.contains(&missing.display().to_string()), "{e}");

    let key = BTreeMap::from([(vec![1], 1)]);
    let e = format!("{:#}", smop::fs::write_yaml(&path, &key).unwrap_err());
    assert!(e.contains(&path.display().to_string()), "{e}");
    let no_dir = dir.path().join("none/x.yaml");
    let e = format!("{:#}", smop::fs::write_yaml(&no_dir, &1).unwrap_err());
    assert!(e.contains(&no_dir.display().to_string()), "{e}");
}
