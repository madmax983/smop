use smop::cmd;
use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// A path type of a different crate: it iterates over its components.
struct OtherPath(PathBuf);

impl AsRef<Path> for OtherPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl<'a> IntoIterator for &'a OtherPath {
    type Item = &'a OsStr;
    type IntoIter = std::path::Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

fn main() {
    let p = PathBuf::from("/usr/local/lib");
    let _ = cmd!("ls {p...}");

    let q: &Path = &p;
    let _ = cmd!("ls {q...}");

    let _ = cmd!("ls {r...}", r = p.join("x"));

    let other = OtherPath(p.clone());
    let _ = cmd!("ls {other...}");

    let twice = &&other;
    let _ = cmd!("ls {twice...}");

    let boxed: Box<Path> = p.clone().into_boxed_path();
    let _ = cmd!("ls {boxed...}");

    let cow: Cow<'_, Path> = Cow::Borrowed(q);
    let _ = cmd!("ls {cow...}");
}
