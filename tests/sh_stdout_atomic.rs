//! Tests for `CommandBuilder::stdout_atomic`: stdout goes to a temp file,
//! and only a successful command replaces the target.

#![cfg(feature = "sh")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::time::Duration;

use smop::sh::{self, CommandBuilder};
use tempfile::TempDir;

/// Runs `unix` with `sh -c`, or `windows` with `cmd /C`.
fn script(unix: &str, windows: &str) -> CommandBuilder {
    if cfg!(windows) {
        sh::cmd("cmd").args(["/C", windows])
    } else {
        sh::cmd("sh").args(["-c", unix])
    }
}

/// Writes `partial`, then sleeps 10 s. The sleep is the direct child, so a
/// kill stops it and no grandchild keeps the temp file open (Windows).
fn slow() -> CommandBuilder {
    if cfg!(windows) {
        sh::cmd("powershell").args([
            "-NoProfile",
            "-Command",
            "Write-Output partial; Start-Sleep 10",
        ])
    } else {
        sh::cmd("sh").args(["-c", "echo partial; exec sleep 10"])
    }
}

/// Gets the names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Makes a temp dir with `out.json` that holds `old`.
fn with_old_target() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.json");
    fs::write(&target, "old\n").unwrap();
    (dir, target)
}

#[test]
fn success_replaces_the_target_and_leaves_no_temp_file() {
    let (dir, target) = with_old_target();
    script("echo new", "echo new")
        .stdout_atomic(&target)
        .run()
        .unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap().trim(), "new");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn a_child_that_fails_midway_leaves_the_target_unchanged() {
    let (dir, target) = with_old_target();
    let err = script("echo partial; exit 3", "echo partial& exit 3")
        .stdout_atomic(&target)
        .run()
        .unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(3));
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn a_failed_child_makes_no_new_target() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("new.txt");
    script("echo partial; exit 1", "echo partial& exit 1")
        .stdout_atomic(&target)
        .run()
        .unwrap_err();
    assert_eq!(names(dir.path()), Vec::<String>::new());
}

#[test]
fn a_nonzero_exit_through_capture_or_status_leaves_the_target_unchanged() {
    let (dir, target) = with_old_target();
    let c = script("echo partial; exit 2", "echo partial& exit 2")
        .stdout_atomic(&target)
        .capture()
        .unwrap();
    assert_eq!(c.code(), 2);
    assert_eq!(c.stdout(), "");
    let status = script("echo partial; exit 2", "echo partial& exit 2")
        .stdout_atomic(&target)
        .status()
        .unwrap();
    assert!(!status.success());
    assert!(
        !script("exit 1", "exit 1")
            .stdout_atomic(&target)
            .success()
            .unwrap()
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn a_timeout_leaves_the_target_unchanged() {
    let (dir, target) = with_old_target();
    let err = slow()
        .stdout_atomic(&target)
        .timeout(Duration::from_millis(300))
        .run()
        .unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(124));
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn stderr_to_stdout_sends_both_streams_to_the_target() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("log.txt");
    script("echo out; echo err >&2", "echo out& echo err 1>&2")
        .stderr_to_stdout()
        .stdout_atomic(&target)
        .run()
        .unwrap();
    let text = fs::read_to_string(&target).unwrap();
    assert!(text.contains("out") && text.contains("err"), "{text}");
    assert_eq!(names(dir.path()), ["log.txt"]);
}

#[test]
fn log_stdout_also_gets_a_copy() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.txt");
    let log = dir.path().join("copy.log");
    script("echo one", "echo one")
        .stdout_atomic(&target)
        .log_stdout(&log)
        .run()
        .unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap().trim(), "one");
    assert_eq!(fs::read_to_string(&log).unwrap().trim(), "one");
    assert_eq!(names(dir.path()), ["copy.log", "out.txt"]);
}

#[test]
fn a_missing_directory_is_an_error_before_start() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("ran");
    let marker_text = marker.display().to_string();
    let err = script(
        &format!("touch '{marker_text}'"),
        &format!("type nul > {marker_text}"),
    )
    .stdout_atomic(dir.path().join("missing/out.txt"))
    .run()
    .unwrap_err();
    assert!(format!("{err:#}").contains("out.txt"), "{err:#}");
    assert!(!marker.exists(), "the command must not start");
}

#[test]
fn spawn_then_wait_replaces_the_target() {
    let (dir, target) = with_old_target();
    let child = script("echo spawned", "echo spawned")
        .stdout_atomic(&target)
        .spawn()
        .unwrap();
    child.wait().unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap().trim(), "spawned");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn a_killed_spawn_leaves_the_target_unchanged() {
    let (dir, target) = with_old_target();
    let mut child = slow().stdout_atomic(&target).spawn().unwrap();
    child.kill().unwrap();
    child.wait().unwrap_err();
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn a_kill_after_the_command_ended_leaves_the_target_unchanged() {
    let (dir, target) = with_old_target();
    let mut child = script("echo new", "echo new")
        .stdout_atomic(&target)
        .spawn()
        .unwrap();
    while child.try_wait().unwrap().is_none() {
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    let err = child.wait().unwrap_err();
    assert!(format!("{err:#}").contains("out.json"), "{err:#}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn a_spawn_dropped_without_wait_leaves_no_temp_file() {
    let (dir, target) = with_old_target();
    let mut child = slow().stdout_atomic(&target).spawn().unwrap();
    child.kill().unwrap();
    drop(child);
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn stdout_file_after_stdout_atomic_replaces_it() {
    let (dir, target) = with_old_target();
    let other = dir.path().join("other.txt");
    script("echo x; exit 1", "echo x& exit 1")
        .stdout_atomic(&target)
        .stdout_file(&other)
        .run()
        .unwrap_err();
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    assert_eq!(fs::read_to_string(&other).unwrap().trim(), "x");
    assert_eq!(names(dir.path()), ["other.txt", "out.json"]);
}

#[test]
fn a_rename_that_fails_after_a_success_is_an_error() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("taken");
    fs::create_dir(&target).unwrap();
    let err = script("echo new", "echo new")
        .stdout_atomic(&target)
        .run()
        .unwrap_err();
    assert!(format!("{err:#}").contains("taken"), "{err:#}");
    assert!(target.is_dir());
    assert_eq!(names(dir.path()), ["taken"]);
}

#[test]
fn wait_with_output_replaces_only_after_a_success() {
    let (dir, target) = with_old_target();
    let r = script("echo partial; exit 4", "echo partial& exit 4")
        .stdout_atomic(&target)
        .keep_output()
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert_eq!(r.code(), 4);
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    let r = script("echo new", "echo new")
        .stdout_atomic(&target)
        .keep_output()
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(r.success());
    assert_eq!(r.stdout(), "");
    assert_eq!(fs::read_to_string(&target).unwrap().trim(), "new");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn spawn_guarded_wait_replaces_and_a_dropped_guard_does_not() {
    let (dir, target) = with_old_target();
    let guard = script("echo new", "echo new")
        .stdout_atomic(&target)
        .spawn_guarded()
        .unwrap();
    guard.wait().unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap().trim(), "new");
    drop(slow().stdout_atomic(&target).spawn_guarded().unwrap());
    assert_eq!(fs::read_to_string(&target).unwrap().trim(), "new");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[test]
fn tee_and_quiet_do_not_stop_the_file() {
    let (dir, target) = with_old_target();
    let out = script("echo new", "echo new")
        .stdout_atomic(&target)
        .tee()
        .quiet()
        .output()
        .unwrap();
    assert_eq!(out, "");
    assert_eq!(fs::read_to_string(&target).unwrap().trim(), "new");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[cfg(unix)]
#[test]
fn a_timeout_from_open_output_after_exit_zero_leaves_the_target() {
    let (dir, target) = with_old_target();
    // The command exits 0 at once. Its background child holds stderr.
    let err = sh::cmd("sh")
        .args(["-c", "echo new; sleep 2 &"])
        .stdout_atomic(&target)
        .timeout(Duration::from_millis(300))
        .capture()
        .unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(124));
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[cfg(target_os = "linux")]
#[test]
fn an_output_error_leaves_the_target_unchanged() {
    let (dir, target) = with_old_target();
    let err = sh::cmd("sh")
        .args(["-c", "echo new"])
        .stdout_atomic(&target)
        .log_stdout("/dev/full")
        .run()
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("Command output failed"),
        "{err:#}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
    assert_eq!(names(dir.path()), ["out.json"]);
}

#[cfg(unix)]
#[test]
fn a_symlink_target_stays_a_link() {
    let (dir, target) = with_old_target();
    let link = dir.path().join("link.json");
    std::os::unix::fs::symlink("out.json", &link).unwrap();
    script("echo new", "").stdout_atomic(&link).run().unwrap();
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap().trim(), "new");
    assert_eq!(names(dir.path()), ["link.json", "out.json"]);
}

#[test]
fn pipe_rejects_stdout_atomic() {
    let dir = TempDir::new().unwrap();
    let err = script("echo x", "echo x")
        .pipe(sh::cmd("sort").stdout_atomic(dir.path().join("o.txt")))
        .run()
        .unwrap_err();
    assert!(err.to_string().contains("I/O options"), "{err:#}");
    assert_eq!(names(dir.path()), Vec::<String>::new());
}

#[cfg(unix)]
#[test]
fn success_keeps_the_permissions_of_the_target() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, target) = with_old_target();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    script("echo new", "").stdout_atomic(&target).run().unwrap();
    let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o640);
}
