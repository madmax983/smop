//! `cmd!` in a crate that forbids lints.
//!
//! A caller cannot lower a forbidden lint. So the expansion must not have
//! an `#[allow]` attribute, and it must not cause the lint.
#![forbid(unused_imports, unused_must_use)]
#![deny(warnings)]

use smop::cmd;

#[test]
fn the_expansion_is_clean_under_forbidden_lints() {
    let (value, list, count) = ("a b", ["c"], 2_u8);
    assert_eq!(
        cmd!("tool {value} {list...} -n{count} --x={value}").to_string(),
        "tool \"a b\" c -n2 \"--x=a b\""
    );
    assert_eq!(cmd!("tool {l...}", l = [value]).to_string(), "tool \"a b\"");
    assert_eq!(cmd!("tool --version").to_string(), "tool --version");
}
