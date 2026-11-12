//! Tests for `smop::git`. Each test makes a repository in a temporary
//! directory.
//!
//! Tests of the free functions start this test binary again in a
//! repository, with `CHILD` set. The `child` test then runs that case.
#![cfg(feature = "git")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use proptest::prelude::*;
use smop::git::{self, Repo};
use smop::sh;
use tempfile::TempDir;

/// The case for the `child` test.
const CHILD: &str = "SMOP_GIT_CHILD";
/// The expected HEAD of the `child` test.
const EXPECT: &str = "SMOP_GIT_EXPECT";
/// The repository of the `repo_env` case.
const OTHER: &str = "SMOP_GIT_OTHER";

/// Repository-local and pathspec variables. The setup commands and the
/// child processes remove them, so a test that runs in a git hook still
/// works.
const CLEAR_ENV: [&str; 19] = [
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
    "GIT_LITERAL_PATHSPECS",
    "GIT_GLOB_PATHSPECS",
    "GIT_NOGLOB_PATHSPECS",
    "GIT_ICASE_PATHSPECS",
];

/// Makes a git command in `dir` for the setup of a test, with an identity
/// and no signing.
fn git_cmd(dir: &Path, args: &[&str]) -> sh::CommandBuilder {
    let mut cmd = sh::cmd("git").arg("-C").arg(dir).args([
        "-c",
        "user.name=smop",
        "-c",
        "user.email=smop@example.com",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "protocol.file.allow=always",
    ]);
    for var in CLEAR_ENV {
        cmd = cmd.env_remove(var);
    }
    cmd.args(args)
}

/// Runs git in `dir` for the setup of a test, and gets stdout.
fn git_in(dir: &Path, args: &[&str]) -> String {
    git_cmd(dir, args).output().unwrap()
}

/// A repository in a temporary directory, on branch `main`, with no commit.
struct Fixture {
    tmp: TempDir,
    repo: Repo,
}

impl Fixture {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        git_in(tmp.path(), &["init", "-q"]);
        git_in(tmp.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git_in(tmp.path(), &["config", "core.autocrlf", "false"]);
        // No hooks of the user config run.
        git_in(tmp.path(), &["config", "core.hooksPath", ".git/no-hooks"]);
        let repo = Repo::new(tmp.path());
        Self { tmp, repo }
    }

    /// A repository with one commit: `a.txt`, `b.txt`, `dir/c.txt` and a
    /// `.gitignore` for `*.log`.
    fn committed() -> Self {
        let fx = Self::new();
        fx.write("a.txt", "a\n");
        fx.write("b.txt", "b\n");
        fx.write("dir/c.txt", "c\n");
        fx.write(".gitignore", "*.log\n");
        fx.commit("first");
        fx
    }

    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn write(&self, rel: &str, text: &str) {
        self.write_bytes(rel, text.as_bytes());
    }

    fn write_bytes(&self, rel: &str, bytes: &[u8]) {
        let path = self.root().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn git(&self, args: &[&str]) -> String {
        git_in(self.root(), args)
    }

    /// Commits all changes and gets the new HEAD.
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--no-verify", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

fn is_hex40(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// head

#[test]
fn head_is_40_hex_sha_of_last_commit() {
    let fx = Fixture::committed();
    let head = fx.repo.head().unwrap();
    assert!(is_hex40(&head), "{head:?}");
    assert_eq!(head, fx.git(&["rev-parse", "HEAD"]));

    fx.write("a.txt", "changed\n");
    let second = fx.commit("second");
    assert_eq!(fx.repo.head().unwrap(), second);
    assert_ne!(head, second);
}

#[test]
fn head_is_64_hex_in_sha256_repository() {
    let tmp = TempDir::new().unwrap();
    git_in(tmp.path(), &["init", "-q", "--object-format=sha256"]);
    std::fs::write(tmp.path().join("a.txt"), "a\n").unwrap();
    git_in(tmp.path(), &["add", "-A"]);
    git_in(tmp.path(), &["commit", "-q", "--no-verify", "-m", "first"]);
    let head = Repo::new(tmp.path()).head().unwrap();
    assert_eq!(head.len(), 64, "{head:?}");
    assert!(head.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
}

#[test]
fn head_errors_before_first_commit() {
    let fx = Fixture::new();
    let err = fx.repo.head().unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(128), "{err:#}");
}

// branch

#[test]
fn branch_names_current_branch() {
    let fx = Fixture::new();
    // An unborn branch has a name too.
    assert_eq!(fx.repo.branch().unwrap().as_deref(), Some("main"));
    fx.write("a.txt", "a\n");
    fx.commit("first");
    fx.git(&["checkout", "-q", "-b", "feature/x"]);
    assert_eq!(fx.repo.branch().unwrap().as_deref(), Some("feature/x"));
}

#[test]
fn branch_name_does_not_change_for_tag_with_same_name() {
    let fx = Fixture::committed();
    fx.git(&["tag", "main"]);
    assert_eq!(fx.repo.branch().unwrap().as_deref(), Some("main"));
}

#[test]
fn branch_is_none_on_detached_head() {
    let fx = Fixture::committed();
    fx.git(&["checkout", "-q", "--detach"]);
    assert_eq!(fx.repo.branch().unwrap(), None);
}

// is_clean

#[test]
fn is_clean_after_commit_and_with_ignored_files() {
    let fx = Fixture::committed();
    assert!(fx.repo.is_clean().unwrap());
    fx.write("build.log", "ignored\n");
    fx.write("dir/deep.log", "ignored\n");
    assert!(fx.repo.is_clean().unwrap());
}

#[test]
fn is_clean_false_for_modified_file() {
    let fx = Fixture::committed();
    fx.write("a.txt", "changed\n");
    assert!(!fx.repo.is_clean().unwrap());
}

#[test]
fn is_clean_false_for_staged_file() {
    let fx = Fixture::committed();
    fx.write("new.txt", "new\n");
    fx.git(&["add", "new.txt"]);
    assert!(!fx.repo.is_clean().unwrap());

    // Staged and then changed back in the working tree: the index differs.
    let fx = Fixture::committed();
    fx.write("a.txt", "changed\n");
    fx.git(&["add", "a.txt"]);
    fx.write("a.txt", "a\n");
    assert!(!fx.repo.is_clean().unwrap());
}

#[test]
fn is_clean_false_for_untracked_file() {
    let fx = Fixture::committed();
    fx.write("dir/new/untracked.txt", "new\n");
    assert!(!fx.repo.is_clean().unwrap());
}

#[test]
fn is_clean_sees_untracked_file_when_config_hides_them() {
    let fx = Fixture::committed();
    fx.git(&["config", "status.showUntrackedFiles", "no"]);
    fx.write("untracked.txt", "new\n");
    assert!(!fx.repo.is_clean().unwrap());
}

#[test]
fn is_clean_false_for_deleted_file() {
    let fx = Fixture::committed();
    std::fs::remove_file(fx.root().join("b.txt")).unwrap();
    assert!(!fx.repo.is_clean().unwrap());
}

#[test]
fn is_clean_paths_checks_only_given_paths() {
    let fx = Fixture::committed();
    fx.write("a.txt", "changed\n");
    fx.write("dir/new.txt", "new\n");
    assert!(fx.repo.is_clean_paths(["b.txt"]).unwrap());
    assert!(!fx.repo.is_clean_paths(["a.txt"]).unwrap());
    assert!(!fx.repo.is_clean_paths(["b.txt", "a.txt"]).unwrap());
    // A directory covers the files in it, also untracked ones.
    assert!(!fx.repo.is_clean_paths(["dir"]).unwrap());
    assert!(
        fx.repo
            .is_clean_paths([Path::new("dir").join("c.txt")])
            .unwrap()
    );
    // A path that is not in the repository is clean.
    assert!(fx.repo.is_clean_paths(["missing.txt"]).unwrap());
    // A nested path with the separator of the platform.
    fx.write("dir/c.txt", "changed\n");
    assert!(
        !fx.repo
            .is_clean_paths([Path::new("dir").join("c.txt")])
            .unwrap()
    );
    // An absolute path in the repository.
    assert!(!fx.repo.is_clean_paths([fx.root().join("a.txt")]).unwrap());
}

#[test]
fn is_clean_paths_follows_core_ignorecase() {
    let fx = Fixture::committed();
    fx.write("a.txt", "changed\n");
    fx.git(&["config", "core.ignorecase", "false"]);
    assert!(fx.repo.is_clean_paths(["A.TXT"]).unwrap());
    fx.git(&["config", "core.ignorecase", "true"]);
    assert!(!fx.repo.is_clean_paths(["A.TXT"]).unwrap());
}

#[test]
fn is_clean_paths_errors_for_path_outside_repository() {
    let fx = Fixture::committed();
    let other = TempDir::new().unwrap();
    let err = fx.repo.is_clean_paths([other.path()]).unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(128), "{err:#}");
    assert!(err.to_string().contains("outside"), "{err:#}");
}

#[test]
fn submodule_changes_count_in_is_clean_and_changed_since() {
    let sub = Fixture::new();
    sub.write("s.txt", "s\n");
    sub.commit("sub");
    let fx = Fixture::committed();
    let url = sub.root().to_str().unwrap().replace('\\', "/");
    fx.git(&["submodule", "add", "-q", &url, "sm"]);
    fx.commit("add submodule");
    assert!(fx.repo.is_clean().unwrap());
    assert_eq!(fx.repo.changed_since("HEAD").unwrap(), paths(&[]));

    // An untracked file in the submodule. Config that hides it has no effect.
    fx.write("sm/untracked.txt", "u\n");
    fx.git(&["config", "submodule.sm.ignore", "all"]);
    fx.git(&["config", "diff.ignoreSubmodules", "all"]);
    assert!(!fx.repo.is_clean().unwrap());
    assert_eq!(fx.repo.changed_since("HEAD").unwrap(), paths(&["sm"]));
}

#[cfg(unix)]
#[test]
fn queries_do_not_run_core_fsmonitor() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::committed();
    let marker = fx.root().join(".git").join("fsmonitor-ran");
    let hook = fx.root().join(".git").join("fsmonitor.sh");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    fx.git(&["config", "core.fsmonitor", hook.to_str().unwrap()]);
    fx.repo.is_clean().unwrap();
    fx.repo.is_clean_paths(["a.txt"]).unwrap();
    fx.repo.changed_since("HEAD").unwrap();
    fx.repo.ls_files().unwrap();
    fx.repo.is_ignored("a.txt").unwrap();
    assert!(!marker.exists());
}

#[test]
fn is_clean_paths_reads_paths_literally() {
    let fx = Fixture::committed();
    fx.write("ab.txt", "untracked\n");
    // As a glob, `a*` would match `ab.txt`.
    assert!(fx.repo.is_clean_paths(["a*"]).unwrap());
    assert!(fx.repo.is_clean_paths([":(glob)a*"]).unwrap());
}

#[test]
fn is_clean_paths_without_paths_checks_whole_repository() {
    let fx = Fixture::committed();
    assert!(fx.repo.is_clean_paths::<_, &str>([]).unwrap());
    fx.write("a.txt", "changed\n");
    assert!(!fx.repo.is_clean_paths::<_, &str>([]).unwrap());
}

#[test]
fn is_clean_paths_are_relative_to_repo_directory() {
    let fx = Fixture::committed();
    fx.write("dir/c.txt", "changed\n");
    let sub = Repo::new(fx.root().join("dir"));
    assert!(!sub.is_clean_paths(["c.txt"]).unwrap());
    assert!(sub.is_clean_paths(["a.txt"]).unwrap());
    // The whole repository, also from a subdirectory.
    assert!(!sub.is_clean().unwrap());
}

// changed_since

#[test]
fn changed_since_lists_sorted_repo_relative_paths_with_uncommitted_changes() {
    let fx = Fixture::committed();
    let first = fx.repo.head().unwrap();
    fx.write("dir/c.txt", "committed change\n");
    fx.write("b.txt", "committed change\n");
    fx.commit("second");
    fx.write("a.txt", "uncommitted\n");
    fx.write("z/new.txt", "untracked\n");
    fx.write("staged.txt", "staged\n");
    fx.git(&["add", "staged.txt"]);
    fx.write("ignored.log", "ignored\n");

    let changed = fx.repo.changed_since(&first).unwrap();
    assert_eq!(
        changed,
        paths(&["a.txt", "b.txt", "dir/c.txt", "staged.txt", "z/new.txt"])
    );
    // Since HEAD: only the uncommitted changes.
    assert_eq!(
        fx.repo.changed_since("HEAD").unwrap(),
        paths(&["a.txt", "staged.txt", "z/new.txt"])
    );
}

#[test]
fn changed_since_lists_both_paths_of_rename_and_deleted_paths() {
    let fx = Fixture::committed();
    fx.git(&["mv", "a.txt", "renamed.txt"]);
    std::fs::remove_file(fx.root().join("b.txt")).unwrap();
    assert_eq!(
        fx.repo.changed_since("HEAD").unwrap(),
        paths(&["a.txt", "b.txt", "renamed.txt"])
    );
}

#[test]
fn changed_since_lists_change_that_is_only_in_the_index() {
    let fx = Fixture::committed();
    fx.write("a.txt", "staged\n");
    fx.git(&["add", "a.txt"]);
    fx.write("a.txt", "a\n");
    assert_eq!(fx.repo.changed_since("HEAD").unwrap(), paths(&["a.txt"]));
}

#[test]
fn changed_since_is_empty_when_clean() {
    let fx = Fixture::committed();
    assert_eq!(fx.repo.changed_since("HEAD").unwrap(), paths(&[]));
}

#[test]
fn changed_since_from_subdirectory_is_repo_relative() {
    let fx = Fixture::committed();
    fx.write("a.txt", "changed\n");
    fx.write("new.txt", "untracked\n");
    fx.write("dir/c.txt", "changed\n");
    fx.git(&["config", "diff.relative", "true"]);
    let sub = Repo::new(fx.root().join("dir"));
    assert_eq!(
        sub.changed_since("HEAD").unwrap(),
        paths(&["a.txt", "dir/c.txt", "new.txt"])
    );
}

#[test]
fn changed_since_rejects_rev_that_starts_with_dash() {
    let fx = Fixture::committed();
    let out = fx.root().join("written.txt");
    let rev = format!("--output={}", out.display());
    let err = fx.repo.changed_since(&rev).unwrap_err();
    assert!(err.to_string().contains("starts with '-'"), "{err:#}");
    assert!(!out.exists());
}

#[test]
fn changed_since_compares_with_rev_not_with_merge_base() {
    let fx = Fixture::committed();
    fx.git(&["checkout", "-q", "-b", "other"]);
    fx.write("only-on-other.txt", "x\n");
    fx.commit("other");
    fx.git(&["checkout", "-q", "main"]);
    fx.write("b.txt", "main\n");
    fx.commit("main");
    assert_eq!(
        fx.repo.changed_since("other").unwrap(),
        paths(&["b.txt", "only-on-other.txt"])
    );
}

#[test]
fn changed_since_rejects_empty_rev_and_leading_colon() {
    let fx = Fixture::committed();
    let err = fx.repo.changed_since("").unwrap_err();
    assert!(err.to_string().contains("empty"), "{err:#}");
    let err = fx.repo.changed_since(":/first").unwrap_err();
    assert!(err.to_string().contains("starts with ':'"), "{err:#}");
}

#[test]
fn changed_since_errors_for_unknown_rev() {
    let fx = Fixture::committed();
    let err = fx.repo.changed_since("no-such-rev").unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(128), "{err:#}");
    assert!(err.to_string().contains("no-such-rev"), "{err:#}");
}

// ls_files

#[test]
fn ls_files_lists_tracked_files_sorted_and_repo_relative() {
    let fx = Fixture::committed();
    fx.write("staged.txt", "staged\n");
    fx.git(&["add", "staged.txt"]);
    fx.write("untracked.txt", "untracked\n");
    fx.write("ignored.log", "ignored\n");
    let expected = paths(&[".gitignore", "a.txt", "b.txt", "dir/c.txt", "staged.txt"]);
    assert_eq!(fx.repo.ls_files().unwrap(), expected);
    // The whole repository, also from a subdirectory.
    assert_eq!(
        Repo::new(fx.root().join("dir")).ls_files().unwrap(),
        expected
    );
}

#[test]
fn ls_files_of_new_repository_is_empty() {
    let fx = Fixture::new();
    assert_eq!(fx.repo.ls_files().unwrap(), paths(&[]));
}

#[test]
fn ls_files_lists_conflicted_file_one_time() {
    let fx = Fixture::committed();
    fx.git(&["checkout", "-q", "-b", "other"]);
    fx.write("a.txt", "other\n");
    fx.commit("other");
    fx.git(&["checkout", "-q", "main"]);
    fx.write("a.txt", "main\n");
    fx.commit("main");
    let merge = git_cmd(fx.root(), &["merge", "-q", "other"])
        .capture()
        .unwrap();
    assert_eq!(merge.code(), 1, "{}", merge.stderr());
    // Git has three entries for the file.
    assert_eq!(
        fx.git(&["ls-files", "-u", "--", "a.txt"]).lines().count(),
        3
    );
    let files = fx.repo.ls_files().unwrap();
    assert_eq!(files.iter().filter(|p| *p == Path::new("a.txt")).count(), 1);
}

// show

#[test]
fn show_gets_file_at_rev_unchanged() {
    let fx = Fixture::new();
    fx.write("a.txt", "one\n  \n\n");
    let first = fx.commit("first");
    fx.write("a.txt", "two\n");
    fx.commit("second");
    fx.write("a.txt", "working tree\n");

    assert_eq!(fx.repo.show(&first, "a.txt").unwrap(), "one\n  \n\n");
    assert_eq!(fx.repo.show("HEAD", "a.txt").unwrap(), "two\n");
}

#[test]
fn show_reads_nested_path_relative_to_repository_root() {
    let fx = Fixture::committed();
    assert_eq!(
        fx.repo
            .show("HEAD", Path::new("dir").join("c.txt"))
            .unwrap(),
        "c\n"
    );
    // From a subdirectory, the path is still relative to the root.
    let sub = Repo::new(fx.root().join("dir"));
    assert_eq!(sub.show("HEAD", "a.txt").unwrap(), "a\n");
    // `./` and `../` are relative to the directory of the query.
    assert_eq!(sub.show("HEAD", "./c.txt").unwrap(), "c\n");
    assert_eq!(sub.show("HEAD", "../b.txt").unwrap(), "b\n");
}

#[test]
fn show_errors_for_missing_path_and_directory() {
    let fx = Fixture::committed();
    let err = fx.repo.show("HEAD", "missing.txt").unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(128), "{err:#}");
    assert!(err.to_string().contains("missing.txt"), "{err:#}");
    let err = fx.repo.show("HEAD", "dir").unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(128), "{err:#}");
}

#[test]
fn show_errors_for_content_that_is_not_utf8() {
    let fx = Fixture::new();
    fx.write_bytes("bin.dat", &[0xff, 0xfe, 0x00]);
    fx.commit("binary");
    let err = fx.repo.show("HEAD", "bin.dat").unwrap_err();
    assert!(err.to_string().contains("UTF-8"), "{err:#}");
    assert!(err.to_string().contains("bin.dat"), "{err:#}");
}

#[test]
fn show_rejects_rev_that_starts_with_dash() {
    let fx = Fixture::committed();
    let err = fx.repo.show("-p", "a.txt").unwrap_err();
    assert!(err.to_string().contains("starts with '-'"), "{err:#}");
    // `:a.txt` would be the index, not a commit.
    let err = fx.repo.show("", "a.txt").unwrap_err();
    assert!(err.to_string().contains("empty"), "{err:#}");
}

// is_ignored

#[test]
fn is_ignored_follows_gitignore() {
    let fx = Fixture::committed();
    fx.write("build.log", "x\n");
    assert!(fx.repo.is_ignored("build.log").unwrap());
    assert!(fx.repo.is_ignored("dir/deep.log").unwrap());
    // The path does not have to exist.
    assert!(fx.repo.is_ignored("missing.log").unwrap());
    assert!(!fx.repo.is_ignored("a.txt").unwrap());
    assert!(!fx.repo.is_ignored("missing.txt").unwrap());
    // A nested path with the separator of the platform.
    assert!(
        fx.repo
            .is_ignored(Path::new("dir").join("deep.log"))
            .unwrap()
    );
}

#[test]
fn is_ignored_directory_pattern_needs_slash_for_missing_directory() {
    let fx = Fixture::committed();
    fx.write(".gitignore", "build/\n");
    assert!(fx.repo.is_ignored("build/").unwrap());
    assert!(!fx.repo.is_ignored("build").unwrap());
    fx.write("build/out.txt", "x\n");
    assert!(fx.repo.is_ignored("build").unwrap());
}

#[test]
fn is_ignored_errors_for_path_outside_repository() {
    let fx = Fixture::committed();
    let other = TempDir::new().unwrap();
    let err = fx.repo.is_ignored(other.path().join("x.log")).unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(128), "{err:#}");
    assert!(err.to_string().contains("outside"), "{err:#}");
}

#[test]
fn is_ignored_false_for_tracked_file_that_matches_a_pattern() {
    let fx = Fixture::committed();
    fx.write("kept.log", "x\n");
    fx.git(&["add", "-f", "kept.log"]);
    assert!(!fx.repo.is_ignored("kept.log").unwrap());
}

#[test]
fn is_ignored_reads_leading_colon_literally() {
    let fx = Fixture::committed();
    fx.write(".gitignore", ":top.txt\n");
    // As pathspec magic, `:top.txt` would be `top.txt`.
    assert!(fx.repo.is_ignored(":top.txt").unwrap());
    assert!(!fx.repo.is_ignored("top.txt").unwrap());
}

#[test]
fn is_ignored_path_is_relative_to_repo_directory() {
    let fx = Fixture::committed();
    fx.write("dir/.gitignore", "local.txt\n");
    let sub = Repo::new(fx.root().join("dir"));
    assert!(sub.is_ignored("local.txt").unwrap());
    assert!(!fx.repo.is_ignored("local.txt").unwrap());
}

// Errors

#[test]
fn outside_repository_errors_include_git_stderr() {
    // Git stops its search for a repository at the parent of `tmp`. Thus a
    // repository above the temporary directory has no effect.
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().parent().unwrap();
    run_child(
        tmp.path(),
        "outside",
        &[("GIT_CEILING_DIRECTORIES", parent)],
        "",
    );
}

/// Checks that each query fails outside a repository with the stderr of
/// git.
fn check_outside(repo: &Repo) {
    let results: Vec<(&str, anyhow::Error)> = vec![
        ("head", repo.head().unwrap_err()),
        ("branch", repo.branch().unwrap_err()),
        ("is_clean", repo.is_clean().unwrap_err()),
        ("is_clean_paths", repo.is_clean_paths(["a"]).unwrap_err()),
        ("changed_since", repo.changed_since("HEAD").unwrap_err()),
        ("ls_files", repo.ls_files().unwrap_err()),
        ("show", repo.show("HEAD", "a").unwrap_err()),
        ("is_ignored", repo.is_ignored("a").unwrap_err()),
    ];
    for (name, err) in results {
        let text = err.to_string();
        assert!(text.contains("not a git repository"), "{name}: {text}");
        assert!(text.starts_with("Command failed"), "{name}: {text}");
        assert_eq!(sh::exit_code(&err), Some(128), "{name}: {err:#}");
    }
}

/// Checks the free functions outside a repository.
fn check_outside_free_functions() {
    let results: Vec<(&str, anyhow::Error)> = vec![
        ("head", git::head().unwrap_err()),
        ("branch", git::branch().unwrap_err()),
        ("is_clean", git::is_clean().unwrap_err()),
        ("is_clean_paths", git::is_clean_paths(["a"]).unwrap_err()),
        ("changed_since", git::changed_since("HEAD").unwrap_err()),
        ("ls_files", git::ls_files().unwrap_err()),
        ("show", git::show("HEAD", "a").unwrap_err()),
        ("is_ignored", git::is_ignored("a").unwrap_err()),
    ];
    for (name, err) in results {
        assert!(
            err.to_string().contains("not a git repository"),
            "{name}: {err:#}"
        );
        assert_eq!(sh::exit_code(&err), Some(128), "{name}: {err:#}");
    }
}

#[test]
fn missing_directory_error_names_it() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("missing");
    let err = Repo::new(&missing).head().unwrap_err();
    let text = err.to_string();
    assert!(text.contains("missing"), "{text}");
    assert_eq!(sh::exit_code(&err), Some(128), "{err:#}");
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_file_names_are_kept() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let fx = Fixture::committed();
    let name = OsStr::from_bytes(b"f\xff.txt");
    std::fs::write(fx.root().join(name), "x\n").unwrap();
    assert_eq!(
        fx.repo.changed_since("HEAD").unwrap(),
        [PathBuf::from(name)]
    );
    fx.commit("non-UTF-8 name");
    assert!(fx.repo.ls_files().unwrap().contains(&PathBuf::from(name)));
}

// The free functions and the environment, in a child process.

/// Runs the `child` test of this binary in `dir` with `case`.
fn run_child(dir: &Path, case: &str, env: &[(&str, &Path)], expect: &str) {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["child", "--exact", "--nocapture", "--test-threads=1"])
        .current_dir(dir)
        .env(CHILD, case)
        .env(EXPECT, expect);
    for var in CLEAR_ENV {
        cmd.env_remove(var);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
    assert!(
        text.contains("1 passed"),
        "the child test did not run: {text}"
    );
}

/// Runs the case that `CHILD` names. Without it, does nothing.
#[test]
fn child() {
    let Ok(case) = std::env::var(CHILD) else {
        return;
    };
    let expect = std::env::var(EXPECT).unwrap();
    match case.as_str() {
        "cwd" => {
            assert_eq!(git::head().unwrap(), expect);
            assert_eq!(git::branch().unwrap().as_deref(), Some("main"));
            assert!(!git::is_clean().unwrap());
            assert!(git::is_clean_paths(["a.txt"]).unwrap());
            assert!(!git::is_clean_paths(["u.txt"]).unwrap());
            assert_eq!(git::changed_since(&expect).unwrap(), paths(&["u.txt"]));
            assert_eq!(git::ls_files().unwrap(), paths(&[".gitignore", "a.txt"]));
            assert_eq!(git::show("HEAD", "a.txt").unwrap(), "a\n");
            assert!(git::is_ignored("x.log").unwrap());
            assert!(!git::is_ignored("a.txt").unwrap());
        }
        "repo_env" => {
            // The free functions use the environment, as git does.
            let other = std::env::var(OTHER).unwrap();
            assert_eq!(git::head().unwrap(), other);
            // A `Repo` uses its own directory.
            let dir = std::env::current_dir().unwrap();
            assert_eq!(Repo::new(&dir).head().unwrap(), expect);
            assert!(Repo::new(&dir).is_clean().unwrap());
            assert_eq!(Repo::default().head().unwrap(), other);
        }
        "outside" => {
            check_outside(&Repo::new(std::env::current_dir().unwrap()));
            check_outside(&Repo::default());
            check_outside_free_functions();
        }
        _ => panic!("unknown case {case}"),
    }
}

#[test]
fn free_functions_use_current_directory() {
    let fx = Fixture::new();
    fx.write("a.txt", "a\n");
    fx.write(".gitignore", "*.log\n");
    let head = fx.commit("first");
    fx.write("u.txt", "untracked\n");
    // Pathspec variables of the user have no effect.
    let on = Path::new("1");
    run_child(
        fx.root(),
        "cwd",
        &[
            ("GIT_LITERAL_PATHSPECS", on),
            ("GIT_GLOB_PATHSPECS", on),
            ("GIT_ICASE_PATHSPECS", on),
        ],
        &head,
    );
}

#[test]
fn repo_ignores_variables_that_select_another_repository() {
    let fx = Fixture::committed();
    let head = fx.repo.head().unwrap();
    let other = Fixture::new();
    other.write("o.txt", "other\n");
    let other_head = other.commit("other");
    let git_dir = other.root().join(".git");
    run_child(
        fx.root(),
        "repo_env",
        &[
            ("GIT_DIR", &git_dir),
            ("GIT_WORK_TREE", other.root()),
            ("GIT_INDEX_FILE", &git_dir.join("index")),
            (OTHER, Path::new(&other_head)),
        ],
        &head,
    );
}

// Invariants

/// One change to the working tree or the index.
#[derive(Debug, Clone, Copy)]
enum Op {
    Modify(usize),
    Stage(usize),
    Delete(usize),
    Ignored(usize),
    Untracked(usize),
    /// Writes the committed text back. The index can still differ.
    Revert(usize),
    /// Stages all, then deletes the untracked file of `Untracked`.
    StageAndDeleteNew(usize),
}

fn op() -> impl Strategy<Value = Op> {
    (0..3usize, 0..7u8).prop_map(|(i, kind)| match kind {
        0 => Op::Modify(i),
        1 => Op::Stage(i),
        2 => Op::Delete(i),
        3 => Op::Ignored(i),
        4 => Op::Untracked(i),
        5 => Op::Revert(i),
        _ => Op::StageAndDeleteNew(i),
    })
}

/// The text of each file, by name. `None`: no file.
type Files = BTreeMap<String, Option<&'static str>>;

/// The expected `changed_since("HEAD")`: each path whose text in the work
/// tree or in the index is not the text in `HEAD`.
fn model(head: &Files, work: &Files, index: &Files) -> Vec<PathBuf> {
    let names: BTreeSet<&String> = head.keys().chain(work.keys()).chain(index.keys()).collect();
    let text = |files: &Files, name: &String| files.get(name).copied().flatten();
    names
        .into_iter()
        .filter(|name| {
            text(work, name) != text(head, name) || text(index, name) != text(head, name)
        })
        .map(PathBuf::from)
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// `changed_since("HEAD")` is the model. It is empty only when
    /// `is_clean()`. A path is in it only when `is_clean_paths([path])` is
    /// false.
    #[test]
    fn changed_since_head_agrees_with_is_clean(ops in prop::collection::vec(op(), 0..6)) {
        let fx = Fixture::new();
        fx.write(".gitignore", "*.log\n");
        for i in 0..3 {
            fx.write(&format!("f{i}.txt"), "start\n");
        }
        fx.commit("start");
        let head: Files = (0..3).map(|i| (format!("f{i}.txt"), Some("start\n"))).collect();
        let mut work = head.clone();
        let mut index = head.clone();
        let put = |work: &mut Files, name: String, text: Option<&'static str>| {
            match text {
                Some(text) => fx.write(&name, text),
                None => {
                    let _ = std::fs::remove_file(fx.root().join(&name));
                }
            }
            work.insert(name, text);
        };
        for op in &ops {
            match *op {
                Op::Modify(i) => put(&mut work, format!("f{i}.txt"), Some("changed\n")),
                Op::Stage(i) => {
                    put(&mut work, format!("f{i}.txt"), Some("staged\n"));
                    fx.git(&["add", "-A"]);
                    index.clone_from(&work);
                }
                Op::Delete(i) => put(&mut work, format!("f{i}.txt"), None),
                // Not in the model: ignored files never change.
                Op::Ignored(i) => fx.write(&format!("i{i}.log"), "ignored\n"),
                Op::Untracked(i) => put(&mut work, format!("n{i}.txt"), Some("new\n")),
                Op::Revert(i) => put(&mut work, format!("f{i}.txt"), Some("start\n")),
                Op::StageAndDeleteNew(i) => {
                    fx.git(&["add", "-A"]);
                    index.clone_from(&work);
                    put(&mut work, format!("n{i}.txt"), None);
                }
            }
        }
        let changed = fx.repo.changed_since("HEAD").unwrap();
        prop_assert_eq!(&changed, &model(&head, &work, &index));
        prop_assert_eq!(changed.is_empty(), fx.repo.is_clean().unwrap());
        let mut sorted = changed.clone();
        sorted.sort();
        sorted.dedup();
        prop_assert_eq!(&sorted, &changed);
        for name in ["f0.txt", "f1.txt", "f2.txt", "n0.txt", "n1.txt", "n2.txt", "i0.log", "i1.log", "i2.log"] {
            let path = PathBuf::from(name);
            prop_assert_eq!(
                changed.contains(&path),
                !fx.repo.is_clean_paths([&path]).unwrap(),
                "{}", name
            );
        }
    }
}
