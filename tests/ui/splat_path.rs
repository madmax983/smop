use smop::cmd;
use std::path::{Path, PathBuf};

fn main() {
    let p = PathBuf::from("/usr/local/lib");
    let _ = cmd!("ls {p...}");

    let q: &Path = &p;
    let _ = cmd!("ls {q...}");

    let _ = cmd!("ls {r...}", r = p.join("x"));
}
