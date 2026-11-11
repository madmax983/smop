//! Support for the expansion of [`cmd!`](crate::cmd!). Not a public API.
//!
//! # How a value selects its conversion
//!
//! A `{name}` accepts `AsRef<OsStr>` values, integers and `char`. A plain
//! trait cannot express that: an impl for each `T: AsRef<OsStr>` and an
//! impl for `u32` are a coherence error. So the expansion uses autoref
//! specialization at the call site.
//!
//! The expansion wraps a reference to the value, puts some `&` before the
//! wrapper, and calls a method. Several traits have that method. Each impl
//! is for a different number of `&`. Method lookup tries the largest number
//! first, and it skips an impl when the bounds of the impl do not hold.
//!
//! The last impl of each method has no bounds on its impl, so lookup always
//! finds it. The bound is on its method. No type satisfies that bound, and
//! the bound has the error message.

use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

pub use smop_macros::__cmd;

/// Joins the parts of one word into one argument.
#[must_use]
pub fn word(parts: &[&dyn AsRef<OsStr>]) -> OsString {
    let mut joined = OsString::new();
    for part in parts {
        joined.push(part);
    }
    joined
}

/// A reference to the value of a `{name}`.
pub struct Arg<'a, T: ?Sized>(pub &'a T);

/// A value that becomes an argument by its text: an integer or a `char`.
pub trait Scalar {
    /// Gets the text of the value.
    fn text(&self) -> String;
}

macro_rules! scalar {
    ($($type:ty)*) => {$(
        impl Scalar for $type {
            fn text(&self) -> String {
                self.to_string()
            }
        }
    )*};
}

scalar!(u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize char);

impl<T: Scalar + ?Sized> Scalar for &T {
    fn text(&self) -> String {
        (**self).text()
    }
}

impl<T: Scalar + ?Sized> Scalar for &mut T {
    fn text(&self) -> String {
        (**self).text()
    }
}

/// The conversions of a `{name}` value. The `OsStr` impl is first.
pub trait ArgValue {
    /// The argument.
    type Out: AsRef<OsStr>;

    /// Converts the value into one argument.
    fn __smop_cmd_arg(&self) -> Self::Out;
}

impl<'a, T: AsRef<OsStr> + ?Sized> ArgValue for &&Arg<'a, T> {
    type Out = &'a OsStr;

    fn __smop_cmd_arg(&self) -> &'a OsStr {
        self.0.as_ref()
    }
}

impl<T: Scalar + ?Sized> ArgValue for &Arg<'_, T> {
    type Out = String;

    fn __smop_cmd_arg(&self) -> String {
        self.0.text()
    }
}

/// A type that `{name}` accepts. No type has this trait: it holds the
/// error message for all other types.
#[diagnostic::on_unimplemented(
    message = "cmd!: `{Self}` is not an argument; use `.to_string()`",
    label = "`{{name}}` has a value of type `{Self}`",
    note = "`{{name}}` accepts `AsRef<OsStr>` values, integers and `char`",
    note = "for a list of arguments, write `{{name...}}`"
)]
pub trait Argument {}

/// The last conversion of a `{name}` value: a compile error.
pub trait ArgOther {
    /// The type of the value.
    type Value: ?Sized;

    /// Never callable. See [`Argument`].
    fn __smop_cmd_arg(&self) -> &'static OsStr
    where
        Self::Value: Argument;
}

impl<T: ?Sized> ArgOther for Arg<'_, T> {
    type Value = T;

    fn __smop_cmd_arg(&self) -> &'static OsStr
    where
        T: Argument,
    {
        OsStr::new("")
    }
}

/// A reference to the value of a `{name...}`.
pub struct Splat<'a, T: ?Sized>(pub &'a T);

/// A path, or a reference to one. A path iterates over its components, so
/// a splat of a path is never correct.
pub trait PathLike {}

impl PathLike for Path {}
impl PathLike for PathBuf {}
impl PathLike for Box<Path> {}
impl PathLike for Rc<Path> {}
impl PathLike for Arc<Path> {}
impl PathLike for Cow<'_, Path> {}
impl<T: PathLike + ?Sized> PathLike for &T {}
impl<T: PathLike + ?Sized> PathLike for &mut T {}

/// A type that is not a path. No type has this trait: it holds the error
/// message for a splat of a path.
#[diagnostic::on_unimplemented(
    message = "cmd!: a path is one argument: write `{{name}}`, not `{{name...}}`",
    label = "`{{name...}}` has a value of type `{Self}`",
    note = "a splat of a path gives one argument for each component"
)]
pub trait NotAPath {}

/// The first conversion of a `{name...}` value: a compile error for a path.
pub trait SplatPath {
    /// The type of the value.
    type Value: ?Sized;

    /// Never callable. See [`NotAPath`].
    fn __smop_cmd_splat(&self) -> [&'static OsStr; 0]
    where
        Self::Value: NotAPath;
}

impl<T: PathLike + ?Sized> SplatPath for &&&&&Splat<'_, T> {
    type Value = T;

    fn __smop_cmd_splat(&self) -> [&'static OsStr; 0]
    where
        T: NotAPath,
    {
        []
    }
}

/// The conversions of a `{name...}` value: iteration by reference.
///
/// There is one impl for each number of references around the list, from
/// zero to three. So `Vec<T>`, `&[T]`, `&&[T]` and `&&&[T]` all work.
pub trait SplatValue {
    /// The arguments.
    type Out: IntoIterator<Item: AsRef<OsStr>>;

    /// Converts the value into zero or more arguments.
    fn __smop_cmd_splat(&self) -> Self::Out;
}

impl<'a, T: ?Sized> SplatValue for &&&&Splat<'a, T>
where
    &'a T: IntoIterator<Item: AsRef<OsStr>>,
{
    type Out = &'a T;

    fn __smop_cmd_splat(&self) -> &'a T {
        self.0
    }
}

impl<'a, T: ?Sized> SplatValue for &&&Splat<'_, &'a T>
where
    &'a T: IntoIterator<Item: AsRef<OsStr>>,
{
    type Out = &'a T;

    fn __smop_cmd_splat(&self) -> &'a T {
        self.0
    }
}

impl<'a, T: ?Sized> SplatValue for &&Splat<'_, &&'a T>
where
    &'a T: IntoIterator<Item: AsRef<OsStr>>,
{
    type Out = &'a T;

    fn __smop_cmd_splat(&self) -> &'a T {
        self.0
    }
}

impl<'a, T: ?Sized> SplatValue for &Splat<'_, &&&'a T>
where
    &'a T: IntoIterator<Item: AsRef<OsStr>>,
{
    type Out = &'a T;

    fn __smop_cmd_splat(&self) -> &'a T {
        self.0
    }
}

/// A type that `{name...}` accepts. No type has this trait: it holds the
/// error message for all other types.
#[diagnostic::on_unimplemented(
    message = "cmd!: `{Self}` is not a list of arguments",
    label = "`{{name...}}` has a value of type `{Self}`",
    note = "`{{name...}}` accepts a value that iterates by reference over `AsRef<OsStr>` items",
    note = "collect an iterator into a `Vec` first; for one argument, write `{{name}}`"
)]
pub trait ArgumentList {}

/// The last conversion of a `{name...}` value: a compile error.
pub trait SplatOther {
    /// The type of the value.
    type Value: ?Sized;

    /// Never callable. See [`ArgumentList`].
    fn __smop_cmd_splat(&self) -> [&'static OsStr; 0]
    where
        Self::Value: ArgumentList;
}

impl<T: ?Sized> SplatOther for Splat<'_, T> {
    type Value = T;

    fn __smop_cmd_splat(&self) -> [&'static OsStr; 0]
    where
        T: ArgumentList,
    {
        []
    }
}

// The `&` before each wrapper select the impl, as in the expansion.
#[cfg(test)]
#[allow(clippy::needless_borrow)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn list<I: IntoIterator<Item: AsRef<OsStr>>>(items: I) -> Vec<OsString> {
        items
            .into_iter()
            .map(|item| item.as_ref().to_owned())
            .collect()
    }

    #[test]
    fn word_joins_its_parts() {
        let none: [&dyn AsRef<OsStr>; 0] = [];
        assert_eq!(word(&none), "");
        assert_eq!(
            word(&[&"--out=", &PathBuf::from("a b"), &String::from("/c")]),
            "--out=a b/c"
        );
    }

    #[test]
    fn an_os_string_type_is_borrowed() {
        let string = String::from("a b");
        let got: &OsStr = (&&&Arg(&string)).__smop_cmd_arg();
        assert_eq!(got, "a b");
        assert_eq!((&&&Arg(&"str")).__smop_cmd_arg(), "str");
        assert_eq!((&&&Arg(&&&"refs")).__smop_cmd_arg(), "refs");
        assert_eq!((&&&Arg(Path::new("p"))).__smop_cmd_arg(), "p");
        assert_eq!((&&&Arg(&PathBuf::from("pb"))).__smop_cmd_arg(), "pb");
        assert_eq!((&&&Arg(OsStr::new("os"))).__smop_cmd_arg(), "os");
        assert_eq!((&&&Arg(&OsString::from("oss"))).__smop_cmd_arg(), "oss");
    }

    #[test]
    fn a_scalar_becomes_its_text() {
        let got: String = (&&&Arg(&8_usize)).__smop_cmd_arg();
        assert_eq!(got, "8");
        assert_eq!((&&&Arg(&-3_i64)).__smop_cmd_arg(), "-3");
        assert_eq!((&&&Arg(&u128::MAX)).__smop_cmd_arg(), u128::MAX.to_string());
        assert_eq!((&&&Arg(&i128::MIN)).__smop_cmd_arg(), i128::MIN.to_string());
        assert_eq!((&&&Arg(&'x')).__smop_cmd_arg(), "x");
        assert_eq!((&&&Arg(&&7_u8)).__smop_cmd_arg(), "7");
        let mut n = 5_i32;
        assert_eq!((&&&Arg(&&mut n)).__smop_cmd_arg(), "5");
    }

    #[test]
    fn a_list_iterates_by_reference() {
        let vec = vec![String::from("a b"), String::from("c")];
        assert_eq!(list((&&&&&&Splat(&vec)).__smop_cmd_splat()), ["a b", "c"]);
        assert_eq!(vec.len(), 2);

        assert_eq!(
            list((&&&&&&Splat(&["x", "y"])).__smop_cmd_splat()),
            ["x", "y"]
        );
        assert_eq!(
            list((&&&&&&Splat(&Some("one"))).__smop_cmd_splat()),
            ["one"]
        );
        assert_eq!(
            list((&&&&&&Splat(&None::<&str>)).__smop_cmd_splat()),
            [] as [OsString; 0]
        );

        let set: BTreeSet<&str> = ["b", "a"].into();
        assert_eq!(list((&&&&&&Splat(&set)).__smop_cmd_splat()), ["a", "b"]);

        let paths = [PathBuf::from("/usr/lib")];
        assert_eq!(list((&&&&&&Splat(&paths)).__smop_cmd_splat()), ["/usr/lib"]);
    }

    #[test]
    fn a_list_is_found_through_three_references() {
        let slice: &[&str] = &["a", "b"];
        assert_eq!(list((&&&&&&Splat(slice)).__smop_cmd_splat()), ["a", "b"]);
        assert_eq!(list((&&&&&&Splat(&slice)).__smop_cmd_splat()), ["a", "b"]);
        assert_eq!(list((&&&&&&Splat(&&slice)).__smop_cmd_splat()), ["a", "b"]);
        assert_eq!(list((&&&&&&Splat(&&&slice)).__smop_cmd_splat()), ["a", "b"]);
    }
}
