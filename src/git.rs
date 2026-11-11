//! Git queries for scripts. Needs the `git` feature.
//!
//! The functions run the `git` program through [`crate::sh`]. They
//! query the repository of the current directory. [`Repo`] has the same
//! queries for a directory that you give.
//!
//! - [`head`]: the commit ID of `HEAD`.
//! - [`branch`]: the current branch. `None` on a detached `HEAD`.
//! - [`is_clean`], [`is_clean_paths`]: no changes and no untracked files.
//! - [`changed_since`]: the paths that changed after a revision.
//! - [`ls_files`]: the tracked files.
//! - [`show`]: the text of a file at a revision.
//! - [`is_ignored`]: `true` when ignore rules match a path.
//!
//! # Paths
//!
//! - Paths in a result are relative to the repository root. They are
//!   sorted, with no duplicates. The sort compares path components, so
//!   `a/b` comes before `a.txt`.
//! - A path that you give is relative to the directory of the query, as on
//!   the git command line. [`show`] is different: its path is relative to
//!   the repository root.
//! - smop reads your paths literally. `*` and `:(glob)` are not patterns.
//! - A revision that is empty or starts with `-` or `:` is an error.
//!
//! # Errors
//!
//! When git fails, the error holds a [`sh::CommandError`]. Its text has
//! the git command line and the stderr of git. [`sh::exit_code`] gives the
//! code of git: 128 outside a repository, 127 when git is not installed.
//!
//! # Trust
//!
//! Some queries can run programs that the repository config names, for
//! example a clean filter. Do not query a repository that you do not trust.
//! smop turns off `core.fsmonitor`.
//!
//! # Examples
//!
//! ```no_run
//! use smop::git;
//!
//! if !git::is_clean()? {
//!     anyhow::bail!("commit your changes before the release");
//! }
//! let version = git::show("HEAD", "VERSION")?;
//! let changed = git::changed_since("origin/main")?;
//! println!("{} at {}: {} changed paths", version.trim(), git::head()?, changed.len());
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::borrow::Cow;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::sh::{self, CommandBuilder};

/// Repository-local variables, as `git rev-parse --local-env-vars` lists
/// them. A [`Repo`] for a directory removes them.
const LOCAL_ENV: [&str; 15] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// Variables that change how git reads pathspecs. Some of them stop
/// `--literal-pathspecs`, so smop removes them.
const PATHSPEC_ENV: [&str; 4] = [
    "GIT_LITERAL_PATHSPECS",
    "GIT_GLOB_PATHSPECS",
    "GIT_NOGLOB_PATHSPECS",
    "GIT_ICASE_PATHSPECS",
];

/// The repository in a directory.
///
/// [`Repo::new`] runs `git -C <dir>`. It also removes the repository-local
/// variables (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` and the others
/// that `git rev-parse --local-env-vars` lists). Thus a script that a git
/// hook starts still queries `dir`.
///
/// [`Repo::default`] is the current directory. It keeps these variables, as
/// git does. The free functions of this module use it.
///
/// A subdirectory of a work tree is the same repository. See the
/// [module docs](self) for the paths.
///
/// # Examples
///
/// ```no_run
/// use smop::git::Repo;
///
/// let repo = Repo::new("vendor/lib");
/// if !repo.is_clean()? {
///     println!("changed: {:?}", repo.changed_since("HEAD")?);
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Repo {
    dir: Option<PathBuf>,
}

impl Repo {
    /// Makes a handle for the repository in `dir`. This does not run git.
    #[must_use]
    pub fn new<P: AsRef<Path>>(dir: P) -> Self {
        Self {
            dir: Some(dir.as_ref().to_path_buf()),
        }
    }

    /// Gets the full commit ID of `HEAD`: 40 lowercase hex characters (64
    /// in a SHA-256 repository).
    ///
    /// # Errors
    ///
    /// Returns an error outside a repository, or before the first commit.
    pub fn head(&self) -> Result<String> {
        let out = stdout(self.git().args(["rev-parse", "--verify", "HEAD"]))?;
        line(out, "commit ID")
    }

    /// Gets the name of the current branch, for example `main`.
    ///
    /// Returns `Ok(None)` on a detached `HEAD`. A branch with no commit yet
    /// has a name. A tag with the same name does not change the name.
    ///
    /// # Errors
    ///
    /// Returns an error outside a repository.
    pub fn branch(&self) -> Result<Option<String>> {
        let (code, out) = self
            .git()
            .args(["symbolic-ref", "--quiet", "HEAD"])
            .output_bytes(&[1])?;
        if code == 1 {
            return Ok(None);
        }
        // `--short` gives `heads/main` when a tag `main` exists too.
        let name = line(out, "branch name")?;
        Ok(Some(
            name.strip_prefix("refs/heads/")
                .map_or_else(|| name.clone(), str::to_owned),
        ))
    }

    /// Returns `true` when the repository has no changes.
    ///
    /// These make it not clean: a changed or deleted file, a staged change,
    /// and an untracked file that ignore rules do not match. A submodule
    /// with changes makes it not clean too. An ignored file does not. The
    /// configs `status.showUntrackedFiles` and `diff.ignoreSubmodules` have
    /// no effect.
    ///
    /// # Errors
    ///
    /// Returns an error outside a repository.
    pub fn is_clean(&self) -> Result<bool> {
        self.is_clean_paths::<_, &Path>([])
    }

    /// Returns `true` when `paths` have no changes, as [`is_clean`](Self::is_clean)
    /// finds them.
    ///
    /// A directory includes the files in it. A path that git does not know
    /// is clean. The case of a path counts only when `core.ignorecase` is
    /// off. With no paths, this checks the whole repository, as
    /// `git status` does.
    ///
    /// # Errors
    ///
    /// Returns an error outside a repository, or for a path outside it.
    pub fn is_clean_paths<I, P>(&self, paths: I) -> Result<bool>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let paths: Vec<P> = paths.into_iter().collect();
        let mut cmd = self
            .git()
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args(["status", "--porcelain", "-z", "--untracked-files=normal"])
            .args(["--ignore-submodules=none", "--"]);
        if !paths.is_empty() {
            // Without `icase`, `A.TXT` does not match `a.txt` on macOS.
            let magic = if self.ignore_case()? {
                ":(literal,icase)"
            } else {
                ":(literal)"
            };
            for path in &paths {
                let mut spec = OsString::from(magic);
                spec.push(path.as_ref());
                cmd = cmd.arg(spec);
            }
        }
        Ok(stdout(cmd)?.is_empty())
    }

    /// Gets the paths that changed after `rev`, sorted and relative to the
    /// repository root.
    ///
    /// The list includes the uncommitted changes: changes in the index and
    /// in the work tree, and untracked files that ignore rules do not
    /// match. It also includes deleted paths, the two paths of a rename, and
    /// a submodule with changes. Thus `changed_since("HEAD")` is empty if, and only if,
    /// [`is_clean`](Self::is_clean) is `true`.
    ///
    /// `rev` is one commit. smop compares it with the work tree, not with
    /// the merge base. For the changes of a branch only, give the merge
    /// base (`git merge-base origin/main HEAD`).
    ///
    /// # Errors
    ///
    /// Returns an error outside a repository, for an unknown `rev`, or for
    /// a `rev` that is empty or starts with `-` or `:`.
    pub fn changed_since(&self, rev: &str) -> Result<Vec<PathBuf>> {
        check_rev(rev)?;
        // Outside a repository, `git diff` is `git diff --no-index`. This
        // command gives the correct error first.
        let commit = line(
            stdout(
                self.git()
                    .args(["rev-parse", "--verify"])
                    .arg(format!("{rev}^{{commit}}")),
            )?,
            "commit ID",
        )?;
        let diff = |cached: &[&str]| {
            self.git()
                .args(["-c", "diff.relative=false", "diff", "--name-only", "-z"])
                .args(["--no-renames", "--no-ext-diff", "--no-color"])
                .arg("--ignore-submodules=none")
                .args(cached)
                .args([commit.as_str(), "--"])
        };
        Ok(paths_of(&[
            stdout(diff(&[]))?,
            stdout(diff(&["--cached"]))?,
            stdout(self.ls_files_cmd(&["--others", "--exclude-standard"]))?,
        ]))
    }

    /// Gets the files in the index, sorted and relative to the repository
    /// root.
    ///
    /// A staged new file is in the list. A file with a merge conflict is in
    /// it one time.
    ///
    /// # Errors
    ///
    /// Returns an error outside a repository.
    pub fn ls_files(&self) -> Result<Vec<PathBuf>> {
        Ok(paths_of(&[stdout(self.ls_files_cmd(&[]))?]))
    }

    /// Gets the text of the file at `path` in `rev`, as `git show
    /// rev:path` shows it.
    ///
    /// `path` is relative to the repository root. A path that starts with
    /// `./` or `../` is relative to the directory of the query. The text is
    /// the committed content: smop does not trim it and does not change the
    /// line ends.
    ///
    /// # Errors
    ///
    /// Returns an error when `path` is not a file in `rev`, when the content
    /// is not UTF-8, or when `rev` is empty or starts with `-` or `:`.
    pub fn show<P: AsRef<Path>>(&self, rev: &str, path: P) -> Result<String> {
        check_rev(rev)?;
        let object = object(rev, path.as_ref());
        let out = stdout(self.git().args(["cat-file", "blob"]).arg(&object))?;
        String::from_utf8(out)
            .with_context(|| format!("Content of {} is not UTF-8", object.to_string_lossy()))
    }

    /// Returns `true` when ignore rules (`.gitignore`, `.git/info/exclude`,
    /// `core.excludesFile`) match `path`.
    ///
    /// The path does not have to exist. But a pattern that ends with `/`
    /// matches only a directory: give `build/` for a directory `build`
    /// that does not exist. `git check-ignore` does not report a
    /// tracked file. Thus a tracked file is not ignored.
    ///
    /// # Errors
    ///
    /// Returns an error outside a repository, or for a path outside it.
    pub fn is_ignored<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        let (code, _) = self
            .git()
            .args(["check-ignore", "--quiet", "--"])
            .arg(no_magic(path.as_ref()).as_ref())
            .output_bytes(&[1])?;
        Ok(code == 0)
    }

    /// Makes a `git` command for this repository, with `core.fsmonitor`
    /// off. An empty value is off in all git versions.
    fn git(&self) -> CommandBuilder {
        let mut cmd = sh::cmd("git");
        for var in PATHSPEC_ENV {
            cmd = cmd.env_remove(var);
        }
        if let Some(dir) = &self.dir {
            cmd = cmd.arg("-C").arg(dir);
            for var in LOCAL_ENV {
                cmd = cmd.env_remove(var);
            }
        }
        cmd.args(["-c", "core.fsmonitor="])
    }

    /// Returns `true` when `core.ignorecase` is on.
    fn ignore_case(&self) -> Result<bool> {
        let (_, out) = self
            .git()
            .args(["config", "--bool", "--get", "core.ignorecase"])
            .output_bytes(&[1])?;
        Ok(out.trim_ascii() == b"true")
    }

    /// Makes `git ls-files` for the whole repository, with paths relative
    /// to the root.
    fn ls_files_cmd(&self, options: &[&str]) -> CommandBuilder {
        // `:/` is the root. It is pathspec magic, so it is not literal.
        self.git()
            .args(["ls-files", "-z", "--full-name"])
            .args(options)
            .args(["--", ":/"])
    }
}

/// Gets the full commit ID of `HEAD`. See [`Repo::head`].
///
/// # Errors
///
/// Returns an error outside a repository, or before the first commit.
pub fn head() -> Result<String> {
    Repo::default().head()
}

/// Gets the name of the current branch. `None` on a detached `HEAD`. See
/// [`Repo::branch`].
///
/// # Errors
///
/// Returns an error outside a repository.
pub fn branch() -> Result<Option<String>> {
    Repo::default().branch()
}

/// Returns `true` when the repository has no changes. See
/// [`Repo::is_clean`].
///
/// # Errors
///
/// Returns an error outside a repository.
pub fn is_clean() -> Result<bool> {
    Repo::default().is_clean()
}

/// Returns `true` when `paths` have no changes. See
/// [`Repo::is_clean_paths`].
///
/// # Errors
///
/// Returns an error outside a repository, or for a path outside it.
///
/// # Examples
///
/// ```no_run
/// if !smop::git::is_clean_paths(["Cargo.lock"])? {
///     anyhow::bail!("Cargo.lock has changes");
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn is_clean_paths<I, P>(paths: I) -> Result<bool>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    Repo::default().is_clean_paths(paths)
}

/// Gets the paths that changed after `rev`, with the uncommitted changes.
/// See [`Repo::changed_since`].
///
/// # Errors
///
/// Returns an error outside a repository, for an unknown `rev`, or for a
/// `rev` that is empty or starts with `-` or `:`.
pub fn changed_since(rev: &str) -> Result<Vec<PathBuf>> {
    Repo::default().changed_since(rev)
}

/// Gets the tracked files. See [`Repo::ls_files`].
///
/// # Errors
///
/// Returns an error outside a repository.
pub fn ls_files() -> Result<Vec<PathBuf>> {
    Repo::default().ls_files()
}

/// Gets the text of a file at a revision. See [`Repo::show`].
///
/// # Errors
///
/// Returns an error when `path` is not a file in `rev`, when the content is
/// not UTF-8, or when `rev` is empty or starts with `-` or `:`.
pub fn show<P: AsRef<Path>>(rev: &str, path: P) -> Result<String> {
    Repo::default().show(rev, path)
}

/// Returns `true` when ignore rules match `path`. See
/// [`Repo::is_ignored`].
///
/// # Errors
///
/// Returns an error outside a repository, or for a path outside it.
pub fn is_ignored<P: AsRef<Path>>(path: P) -> Result<bool> {
    Repo::default().is_ignored(path)
}

/// Runs `cmd` and gets stdout. A non-zero exit is an error.
fn stdout(cmd: CommandBuilder) -> Result<Vec<u8>> {
    cmd.output_bytes(&[]).map(|(_, out)| out)
}

/// Gets the first line of `out` as text. `what` names it in an error.
fn line(out: Vec<u8>, what: &str) -> Result<String> {
    let mut text = String::from_utf8(out).with_context(|| format!("Git {what} is not UTF-8"))?;
    text.truncate(text.trim_end().len());
    Ok(text)
}

/// Stops a revision that git would read as an option, such as
/// `--output=FILE`. An empty revision and a leading `:` are errors too:
/// `:path` is a file in the index, and `:/text` reads to the end.
fn check_rev(rev: &str) -> Result<()> {
    if rev.is_empty() {
        bail!("Invalid revision: it is empty");
    }
    if let Some(first @ ('-' | ':')) = rev.chars().next() {
        bail!("Invalid revision {rev:?}: it starts with '{first}'");
    }
    Ok(())
}

/// `check-ignore` has no literal mode. A leading `:` starts pathspec magic,
/// so smop puts `./` before it.
fn no_magic(path: &Path) -> Cow<'_, Path> {
    if path.as_os_str().as_encoded_bytes().first() == Some(&b':') {
        Cow::Owned(Path::new(".").join(path))
    } else {
        Cow::Borrowed(path)
    }
}

/// Makes the object name `rev:path`. Git needs `/` between the parts of
/// the path.
fn object(rev: &str, path: &Path) -> OsString {
    let mut object = OsString::from(rev);
    object.push(":");
    #[cfg(windows)]
    object.push(path.to_string_lossy().replace('\\', "/"));
    #[cfg(not(windows))]
    object.push(path);
    object
}

/// Gets the paths in NUL-separated git outputs, sorted, with no duplicates.
fn paths_of(outputs: &[Vec<u8>]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = outputs
        .iter()
        .flat_map(|out| out.split(|&b| b == 0))
        .filter(|name| !name.is_empty())
        .map(path_of)
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Gets a path from the bytes that git gives. On Unix, smop keeps bytes
/// that are not UTF-8.
#[cfg(unix)]
fn path_of(name: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(name))
}

/// Gets a path from the bytes that git gives. Git for Windows gives UTF-8.
#[cfg(not(unix))]
fn path_of(name: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(name).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_of_splits_sorts_and_removes_duplicates() {
        let outputs = [b"b\0a/c\0".to_vec(), b"a/c\0a-b\0\0".to_vec(), Vec::new()];
        assert_eq!(
            paths_of(&outputs),
            [Path::new("a/c"), Path::new("a-b"), Path::new("b")]
        );
        assert_eq!(paths_of(&[]), Vec::<PathBuf>::new());
    }

    #[test]
    fn line_trims_end_only() {
        assert_eq!(line(b" main\n".to_vec(), "x").unwrap(), " main");
        let err = line(vec![0xff], "branch name").unwrap_err();
        assert_eq!(err.to_string(), "Git branch name is not UTF-8");
    }

    #[test]
    fn check_rev_rejects_options() {
        assert!(check_rev("HEAD~1").is_ok());
        assert!(check_rev("a-b").is_ok());
        let err = check_rev("").unwrap_err();
        assert_eq!(err.to_string(), "Invalid revision: it is empty");
        let err = check_rev(":/fix").unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"Invalid revision ":/fix": it starts with ':'"#
        );
        let err = check_rev("--output=x").unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"Invalid revision "--output=x": it starts with '-'"#
        );
    }

    #[test]
    fn no_magic_protects_leading_colon_only() {
        assert_eq!(no_magic(Path::new(":x")), Path::new("./:x"));
        assert_eq!(no_magic(Path::new("a:x")), Path::new("a:x"));
        assert!(matches!(no_magic(Path::new("a")), Cow::Borrowed(_)));
    }

    #[test]
    fn object_joins_rev_and_path_with_slashes() {
        let path = Path::new("dir").join("c.txt");
        assert_eq!(object("HEAD", &path), OsString::from("HEAD:dir/c.txt"));
    }

    #[test]
    fn new_repo_runs_git_in_dir() {
        let shown = format!("{:?}", Repo::new("x").git());
        assert!(shown.contains("git -C x -c core.fsmonitor="), "{shown}");
        assert_eq!(Repo::default(), Repo { dir: None });
        assert_ne!(Repo::new("x"), Repo::default());
    }
}
