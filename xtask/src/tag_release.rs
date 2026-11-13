//! `tag-release`: tags published versions. `v<version>` goes on the commit
//! in the `.cargo_vcs_info.json` of the crate on crates.io. A tag that
//! exists must point at that commit.

use std::path::Path;

use smop::prelude::*;

/// `.cargo_vcs_info.json` in a published crate.
#[derive(Debug, Default, Deserialize)]
#[serde(crate = "smop::serde")]
pub struct VcsInfo {
    #[serde(default)]
    pub git: Option<VcsGit>,
}

/// The `git` object of [`VcsInfo`].
#[derive(Debug, Default, Deserialize)]
#[serde(crate = "smop::serde")]
pub struct VcsGit {
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub dirty: bool,
}

/// The commit that `version` was published from, checked: a full SHA-1,
/// from a clean tree.
pub fn published_commit(info: &VcsInfo, version: &str) -> Result<String> {
    let git = info.git.as_ref();
    let sha = git.map_or("", |g| g.sha1.as_str());
    ensure!(
        sha.len() == 40
            && sha
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{version}: no git commit in the crate"
    );
    ensure!(
        !git.is_some_and(|g| g.dirty),
        "{version}: published from a dirty tree"
    );
    Ok(sha.to_string())
}

/// Whether a Cargo.toml has the line `version = "<version>"`.
pub fn has_version(manifest: &str, version: &str) -> bool {
    let line = format!("version = \"{version}\"");
    manifest.lines().any(|l| l == line)
}

/// The commit in the `.cargo_vcs_info.json` of smop `version` on crates.io.
fn crates_io_commit(version: &str) -> Result<String> {
    let tmp = fs::temp_dir()?;
    let krate = tmp.path().join(format!("smop-{version}.crate"));
    http::download(
        &format!("https://static.crates.io/crates/smop/smop-{version}.crate"),
        &krate,
    )
    .with_context(|| format!("{version}: no crate on crates.io (is it published?)"))?;
    commit_in_crate(&krate, version)
}

/// The checked commit in the `.cargo_vcs_info.json` of a `.crate` file.
pub fn commit_in_crate(krate: &Path, version: &str) -> Result<String> {
    let unpacked = fs::temp_dir()?;
    archive::extract_tar_gz(krate, unpacked.path())?;
    let info: VcsInfo = fs::read_json(
        unpacked
            .path()
            .join(format!("smop-{version}/.cargo_vcs_info.json")),
    )?;
    published_commit(&info, version)
}

/// Tags `sha` as `v<version>` in the repository at `root`, after checking
/// that the commit is there and that its Cargo.toml has `version`. A tag
/// that exists must point at `sha`. Returns the line to print.
pub fn tag(root: &Path, version: &str, sha: &str) -> Result<String> {
    let commit = format!("{sha}^{{commit}}");
    ensure!(
        cmd!("git cat-file -e {commit}")
            .dir(root)
            .quiet()
            .success()?,
        "{version}: commit {sha} is not in this clone"
    );
    ensure!(
        has_version(&git::Repo::new(root).show(sha, "Cargo.toml")?, version),
        "{version}: Cargo.toml at {sha} has another version"
    );

    let tag = format!("v{version}");
    let tag_commit = format!("refs/tags/{tag}^{{commit}}");
    let existing = cmd!("git rev-parse -q --verify {tag_commit}")
        .dir(root)
        .quiet()
        .capture()?;
    if existing.success() {
        let old = existing.stdout().trim();
        ensure!(old == sha, "{tag}: points at {old}, not {sha}");
        return Ok(format!("{tag}: ok ({sha})"));
    }
    let message = format!("smop {version}");
    cmd!("git tag -a {tag} {sha} -m {message}")
        .dir(root)
        .run()?;
    Ok(format!("{tag}: made ({sha})"))
}

/// Tags each of `versions` in the repository at `root`.
pub fn run(root: &Path, versions: &[String]) -> Result<()> {
    for version in versions {
        let sha = crates_io_commit(version)?;
        println!("{}", tag(root, version, &sha)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "944ff47d0c1e6b5b7e0b0c6b9c7a0d1e2f3a4b5c";

    fn info(json: &str) -> VcsInfo {
        smop::serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_clean_commit_is_taken() {
        let vcs = info(&format!(
            r#"{{"git": {{"sha1": "{SHA}"}}, "path_in_vcs": ""}}"#
        ));
        assert_eq!(published_commit(&vcs, "0.4.0").unwrap(), SHA);
    }

    #[test]
    fn no_commit_a_short_one_or_a_dirty_tree_is_an_error() {
        for (json, message) in [
            ("{}".to_string(), "0.4.0: no git commit in the crate"),
            (
                r#"{"git": {"sha1": "abc"}}"#.to_string(),
                "0.4.0: no git commit in the crate",
            ),
            (
                format!(r#"{{"git": {{"sha1": "{}"}}}}"#, SHA.to_uppercase()),
                "0.4.0: no git commit in the crate",
            ),
            (
                format!(r#"{{"git": {{"sha1": "{SHA}", "dirty": true}}}}"#),
                "0.4.0: published from a dirty tree",
            ),
        ] {
            let err = published_commit(&info(&json), "0.4.0").unwrap_err();
            assert_eq!(err.to_string(), message, "{json}");
        }
    }

    #[test]
    fn has_version_needs_the_whole_line() {
        let manifest =
            "[package]\nversion = \"0.4.0\"\n[dependencies]\nx = { version = \"0.4.0\" }\n";
        assert!(has_version(manifest, "0.4.0"));
        assert!(!has_version(manifest, "0.4"));
        assert!(!has_version("version = \"0.4.0-rc\"\n", "0.4.0"));
    }

    /// A repository with one commit whose Cargo.toml has `version`.
    fn repo(version: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            sh::cmd("git")
                .args(args)
                .dir(dir.path())
                .quiet()
                .run()
                .unwrap();
        };
        git(&["init", "-q"]);
        for (key, value) in [
            ("user.name", "t"),
            ("user.email", "t@example.com"),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
        ] {
            git(&["config", key, value]);
        }
        fs::write_string(
            dir.path().join("Cargo.toml"),
            format!("[package]\nversion = \"{version}\"\n"),
        )
        .unwrap();
        git(&["add", "Cargo.toml"]);
        git(&["commit", "-q", "-m", "release"]);
        let sha = git::Repo::new(dir.path()).head().unwrap();
        (dir, sha)
    }

    #[test]
    fn tag_makes_the_tag_then_finds_it() {
        let (dir, sha) = repo("0.1.0");
        assert_eq!(
            tag(dir.path(), "0.1.0", &sha).unwrap(),
            format!("v0.1.0: made ({sha})")
        );
        assert_eq!(
            tag(dir.path(), "0.1.0", &sha).unwrap(),
            format!("v0.1.0: ok ({sha})")
        );
        let kind = sh::cmd("git")
            .args(["cat-file", "-t", "v0.1.0"])
            .dir(dir.path())
            .output()
            .unwrap();
        assert_eq!(kind, "tag", "an annotated tag");
    }

    #[test]
    fn tag_checks_the_commit_the_version_and_an_existing_tag() {
        let (dir, sha) = repo("0.1.0");
        let missing = "0123456789abcdef0123456789abcdef01234567";
        let err = tag(dir.path(), "0.1.0", missing).unwrap_err().to_string();
        assert_eq!(err, format!("0.1.0: commit {missing} is not in this clone"));

        let err = tag(dir.path(), "0.2.0", &sha).unwrap_err().to_string();
        assert_eq!(
            err,
            format!("0.2.0: Cargo.toml at {sha} has another version")
        );

        sh::cmd("git")
            .args(["tag", "v0.1.0", &sha])
            .dir(dir.path())
            .run()
            .unwrap();
        fs::write_string(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = \"0.1.0\"\n#\n",
        )
        .unwrap();
        sh::cmd("git")
            .args(["commit", "-q", "-am", "again"])
            .dir(dir.path())
            .run()
            .unwrap();
        let newer = git::Repo::new(dir.path()).head().unwrap();
        let err = tag(dir.path(), "0.1.0", &newer).unwrap_err().to_string();
        assert_eq!(err, format!("v0.1.0: points at {sha}, not {newer}"));
    }

    #[test]
    fn commit_in_crate_reads_the_vcs_info() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("pkg/smop-0.1.0");
        fs::create_dir_all(&pkg).unwrap();
        fs::write_string(
            pkg.join(".cargo_vcs_info.json"),
            format!(r#"{{"git": {{"sha1": "{SHA}"}}, "path_in_vcs": ""}}"#),
        )
        .unwrap();
        let krate = dir.path().join("smop-0.1.0.crate");
        archive::create_tar_gz(dir.path().join("pkg"), &krate).unwrap();
        assert_eq!(commit_in_crate(&krate, "0.1.0").unwrap(), SHA);
        assert!(commit_in_crate(&krate, "0.2.0").is_err());
    }
}
