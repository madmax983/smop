//! Tests for the safe-write helpers of `smop::fs`: `write_atomic`,
//! `write_with_parents` and `replace_in_file`.

#![cfg(feature = "fs")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs as stdfs;
use std::path::Path;

use smop::fs;
use tempfile::TempDir;

/// Gets the names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = stdfs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Gets the permission bits of `path`.
#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    stdfs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// Sets the permission bits of `path`.
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    stdfs::set_permissions(path, stdfs::Permissions::from_mode(mode)).unwrap();
}

// write_atomic.

#[test]
fn write_atomic_makes_a_new_file_and_leaves_no_temp_file() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.json");
    fs::write_atomic(&target, "{}\n").unwrap();
    assert_eq!(stdfs::read_to_string(&target).unwrap(), "{}\n");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn write_atomic_replaces_the_content_and_takes_bytes() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("blob.bin");
    stdfs::write(&target, "old content that is longer").unwrap();
    fs::write_atomic(&target, [0_u8, 0xff, 7]).unwrap();
    assert_eq!(stdfs::read(&target).unwrap(), [0, 0xff, 7]);
    assert_eq!(names(dir.path()), ["blob.bin"]);
}

#[test]
fn write_atomic_failure_leaves_the_target_unchanged_and_no_temp_file() {
    let dir = TempDir::new().unwrap();
    // A directory at the target: the rename fails after the write.
    let target = dir.path().join("taken");
    stdfs::create_dir(&target).unwrap();
    stdfs::write(target.join("inside"), "keep").unwrap();
    let err = fs::write_atomic(&target, "new").unwrap_err();
    assert!(format!("{err:#}").contains("taken"), "{err:#}");
    assert!(target.is_dir());
    assert_eq!(
        stdfs::read_to_string(target.join("inside")).unwrap(),
        "keep"
    );
    assert_eq!(names(dir.path()), ["taken"]);
}

#[test]
fn write_atomic_into_a_missing_directory_is_an_error_naming_the_path() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("missing/out.txt");
    let err = fs::write_atomic(&target, "x").unwrap_err();
    assert!(format!("{err:#}").contains("out.txt"), "{err:#}");
    assert!(names(dir.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn write_atomic_keeps_the_permissions_of_an_existing_target() {
    let dir = TempDir::new().unwrap();
    for bits in [0o640, 0o755, 0o444] {
        let target = dir.path().join(format!("f{bits:o}"));
        stdfs::write(&target, "old").unwrap();
        set_mode(&target, bits);
        fs::write_atomic(&target, "new").unwrap();
        assert_eq!(stdfs::read_to_string(&target).unwrap(), "new");
        assert_eq!(mode(&target), bits, "mode of {}", target.display());
    }
}

#[cfg(unix)]
#[test]
fn write_atomic_gives_a_new_file_the_same_mode_as_write_string() {
    let dir = TempDir::new().unwrap();
    let plain = dir.path().join("plain");
    let atomic = dir.path().join("atomic");
    fs::write_string(&plain, "x").unwrap();
    fs::write_atomic(&atomic, "x").unwrap();
    assert_eq!(mode(&atomic), mode(&plain));
}

#[cfg(unix)]
#[test]
fn write_atomic_writes_through_a_symlink_and_keeps_the_link() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real.txt");
    let link = dir.path().join("link.txt");
    stdfs::write(&real, "old").unwrap();
    std::os::unix::fs::symlink("real.txt", &link).unwrap();
    fs::write_atomic(&link, "new").unwrap();
    assert!(stdfs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(stdfs::read_to_string(&real).unwrap(), "new");
    assert_eq!(names(dir.path()), ["link.txt", "real.txt"]);
}

#[cfg(unix)]
#[test]
fn write_atomic_in_a_read_only_directory_leaves_the_target_unchanged() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("ro");
    stdfs::create_dir(&sub).unwrap();
    let target = sub.join("t.txt");
    stdfs::write(&target, "old").unwrap();
    set_mode(&sub, 0o555);
    // root can write in a read-only directory. Then this test proves nothing.
    let probe = stdfs::write(sub.join("probe"), "");
    if probe.is_ok() {
        set_mode(&sub, 0o755);
        return;
    }
    let err = fs::write_atomic(&target, "new").unwrap_err();
    set_mode(&sub, 0o755);
    assert!(format!("{err:#}").contains("t.txt"), "{err:#}");
    assert_eq!(stdfs::read_to_string(&target).unwrap(), "old");
    assert_eq!(names(&sub), ["t.txt"]);
}

// write_with_parents.

#[test]
fn write_with_parents_makes_the_missing_directories_and_the_file() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a/b/c/out.txt");
    fs::write_with_parents(&target, "deep").unwrap();
    assert!(dir.path().join("a/b/c").is_dir());
    assert_eq!(stdfs::read_to_string(&target).unwrap(), "deep");
}

#[test]
fn write_with_parents_replaces_a_file_in_an_existing_directory() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.bin");
    stdfs::write(&target, "old").unwrap();
    fs::write_with_parents(&target, [1_u8, 2]).unwrap();
    assert_eq!(stdfs::read(&target).unwrap(), [1, 2]);
}

#[test]
fn write_with_parents_fails_naming_the_path_when_a_parent_is_a_file() {
    let dir = TempDir::new().unwrap();
    stdfs::write(dir.path().join("a"), "file").unwrap();
    let err = fs::write_with_parents(dir.path().join("a/b/out.txt"), "x").unwrap_err();
    assert!(format!("{err:#}").contains("out.txt"), "{err:#}");
    assert_eq!(stdfs::read_to_string(dir.path().join("a")).unwrap(), "file");
}

#[test]
fn plain_writes_into_a_missing_directory_still_fail_naming_the_path() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a/b/c/out.txt");
    let err = fs::write_string(&target, "x").unwrap_err();
    assert!(format!("{err:#}").contains("out.txt"), "{err:#}");
    let err = fs::write_bytes(&target, "x").unwrap_err();
    assert!(format!("{err:#}").contains("out.txt"), "{err:#}");
    assert!(!dir.path().join("a").exists());
}

// replace_in_file.

#[test]
fn replace_in_file_replaces_every_match_and_gives_the_count() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("Cargo.toml");
    stdfs::write(&target, "v = \"0.3.0\"\ndep = \"0.3.0\"\n# 0.3.0\n").unwrap();
    let n = fs::replace_in_file(&target, "0.3.0", "0.4.0").unwrap();
    assert_eq!(n, 3);
    assert_eq!(
        stdfs::read_to_string(&target).unwrap(),
        "v = \"0.4.0\"\ndep = \"0.4.0\"\n# 0.4.0\n"
    );
    assert_eq!(names(dir.path()), ["Cargo.toml"]);
}

#[test]
fn replace_in_file_counts_matches_that_do_not_overlap() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("t.txt");
    stdfs::write(&target, "aaaaa").unwrap();
    assert_eq!(fs::replace_in_file(&target, "aa", "b").unwrap(), 2);
    assert_eq!(stdfs::read_to_string(&target).unwrap(), "bba");
}

#[test]
fn replace_in_file_with_no_match_fails_and_keeps_the_file_byte_identical() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("version.txt");
    let before = b"version = 1.0\r\nno newline at end";
    stdfs::write(&target, before).unwrap();
    let err = fs::replace_in_file(&target, "2.0", "3.0").unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("version.txt"), "{text}");
    assert!(text.contains("2.0"), "{text}");
    assert_eq!(stdfs::read(&target).unwrap(), before);
    assert_eq!(names(dir.path()), ["version.txt"]);
}

#[test]
fn replace_in_file_with_an_empty_pattern_is_an_error() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("t.txt");
    stdfs::write(&target, "abc").unwrap();
    let err = fs::replace_in_file(&target, "", "x").unwrap_err();
    assert!(format!("{err:#}").contains("t.txt"), "{err:#}");
    assert_eq!(stdfs::read_to_string(&target).unwrap(), "abc");
}

#[test]
fn replace_in_file_of_a_missing_file_is_an_error_naming_the_path() {
    let dir = TempDir::new().unwrap();
    let err = fs::replace_in_file(dir.path().join("none.txt"), "a", "b").unwrap_err();
    assert!(format!("{err:#}").contains("none.txt"), "{err:#}");
    assert!(names(dir.path()).is_empty());
}

#[test]
fn replace_in_file_of_text_that_is_not_utf8_is_an_error_and_keeps_the_file() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("bin.dat");
    stdfs::write(&target, [b'a', 0xff, b'a']).unwrap();
    let err = fs::replace_in_file(&target, "a", "b").unwrap_err();
    assert!(format!("{err:#}").contains("bin.dat"), "{err:#}");
    assert_eq!(stdfs::read(&target).unwrap(), [b'a', 0xff, b'a']);
}

#[cfg(unix)]
#[test]
fn replace_in_file_keeps_the_permissions() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("bump.sh");
    stdfs::write(&target, "echo 1\n").unwrap();
    set_mode(&target, 0o750);
    assert_eq!(fs::replace_in_file(&target, "1", "2").unwrap(), 1);
    assert_eq!(mode(&target), 0o750);
    assert_eq!(stdfs::read_to_string(&target).unwrap(), "echo 2\n");
}

// Properties.

#[cfg(unix)]
#[test]
fn a_reader_never_sees_a_partial_write() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let dir = TempDir::new().unwrap();
    let target = dir.path().join("swap.txt");
    let a = "a".repeat(64 * 1024);
    let b = "b".repeat(64 * 1024);
    fs::write_atomic(&target, &a).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let reader = {
        let (target, stop, a, b) = (target.clone(), Arc::clone(&stop), a.clone(), b.clone());
        std::thread::spawn(move || {
            let mut reads = 0_u32;
            while !stop.load(Ordering::Relaxed) {
                let text = stdfs::read_to_string(&target).unwrap();
                assert!(
                    text == a || text == b,
                    "partial read of {} bytes",
                    text.len()
                );
                reads += 1;
            }
            reads
        })
    };
    for i in 0..200 {
        fs::write_atomic(&target, if i % 2 == 0 { &b } else { &a }).unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    assert!(reader.join().unwrap() > 0);
    assert_eq!(names(dir.path()), ["swap.txt"]);
}

mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn write_atomic_round_trips_any_bytes(content in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let dir = TempDir::new().unwrap();
            let target = dir.path().join("t.bin");
            stdfs::write(&target, "old").unwrap();
            fs::write_atomic(&target, &content).unwrap();
            prop_assert_eq!(stdfs::read(&target).unwrap(), content);
            prop_assert_eq!(names(dir.path()), ["t.bin"]);
        }

        #[test]
        fn replace_in_file_matches_str_replace(
            text in "[ab\n]{0,40}",
            from in "[ab]{1,3}",
            to in "[abc]{0,3}",
        ) {
            let dir = TempDir::new().unwrap();
            let target = dir.path().join("t.txt");
            stdfs::write(&target, &text).unwrap();
            let expected = text.matches(from.as_str()).count();
            match fs::replace_in_file(&target, &from, &to) {
                Ok(n) => {
                    prop_assert_eq!(n, expected);
                    prop_assert!(n > 0);
                    prop_assert_eq!(stdfs::read_to_string(&target).unwrap(), text.replace(&from, &to));
                }
                Err(err) => {
                    prop_assert_eq!(expected, 0);
                    let text_of_err = format!("{err:#}");
                    let quoted = format!("{from:?}");
                    prop_assert!(text_of_err.contains(&quoted));
                    prop_assert_eq!(stdfs::read_to_string(&target).unwrap(), text);
                }
            }
            prop_assert_eq!(names(dir.path()), ["t.txt"]);
        }
    }
}
