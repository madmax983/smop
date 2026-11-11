//! Process I/O tests for `sh::CommandBuilder`: capture, tee, redirection,
//! stdin, quiet, timeout and `output_lines`.
//!
//! Tests that look at the terminal of this process run a case in a new test
//! process (see `child_entry`) and read its stdout and stderr.

#![cfg(feature = "sh")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use smop::sh::{self, CommandBuilder, CommandError};
use tempfile::TempDir;

/// Runs `unix` with `sh -c`, or `windows` with `cmd /C`.
fn script(unix: &str, windows: &str) -> CommandBuilder {
    if cfg!(windows) {
        sh::cmd("cmd").args(["/C", windows])
    } else {
        sh::cmd("sh").args(["-c", unix])
    }
}

/// Gets the `CommandError` from an error chain.
fn command_error(err: &anyhow::Error) -> &CommandError {
    err.chain()
        .find_map(|e| e.downcast_ref::<CommandError>())
        .unwrap_or_else(|| panic!("no CommandError in: {err:#}"))
}

// Capture.

#[test]
fn capture_exit_zero_gives_code_zero_and_text() {
    let c = script("echo out; echo err >&2", "echo out& echo err 1>&2")
        .capture()
        .unwrap();
    assert!(c.success());
    assert_eq!(c.code(), 0);
    assert_eq!(c.status().code(), Some(0));
    assert_eq!(c.stdout().trim(), "out");
    assert_eq!(c.stderr().trim(), "err");
}

#[test]
fn capture_exit_three_is_not_an_error() {
    let c = script(
        "echo out; echo err >&2; exit 3",
        "echo out& echo err 1>&2& exit 3",
    )
    .capture()
    .unwrap();
    assert!(!c.success());
    assert_eq!(c.code(), 3);
    assert_eq!(c.status().code(), Some(3));
    assert_eq!(c.stdout().trim(), "out");
    assert_eq!(c.stderr().trim(), "err");
}

#[test]
fn capture_missing_program_gives_127() {
    let err = sh::cmd("smop_definitely_missing_program")
        .capture()
        .unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(127), "{err:#}");
}

// output_lines.

#[test]
fn output_lines_splits_crlf_and_lf() {
    let lines = script(r"printf 'a\r\nb\n'", "echo a& echo b")
        .output_lines()
        .unwrap();
    assert_eq!(lines, ["a", "b"]);
}

#[test]
fn output_lines_of_empty_output_is_empty() {
    let lines = script("true", "exit 0").output_lines().unwrap();
    assert!(lines.is_empty(), "{lines:?}");
}

#[test]
fn output_lines_nonzero_exit_is_an_error_with_the_code() {
    let err = script("echo a; exit 3", "echo a& exit 3")
        .output_lines()
        .unwrap_err();
    assert_eq!(sh::exit_code(&err), Some(3), "{err:#}");
}

// Redirection.

#[test]
fn stdout_file_truncates_and_stdout_append_appends() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("out.txt");
    fs::write(&file, "old content that is long\n").unwrap();

    script("echo one", "echo one")
        .stdout_file(&file)
        .run()
        .unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert_eq!(text.lines().collect::<Vec<_>>(), ["one"]);

    script("echo two", "echo two")
        .stdout_append(&file)
        .run()
        .unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert_eq!(text.lines().collect::<Vec<_>>(), ["one", "two"]);
}

#[test]
fn stdout_file_is_not_captured() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("out.txt");
    let c = script("echo one", "echo one")
        .stdout_file(&file)
        .capture()
        .unwrap();
    assert_eq!(c.stdout(), "");
    assert_eq!(fs::read_to_string(&file).unwrap().trim(), "one");
}

#[test]
fn stdout_file_that_can_not_open_is_an_error_before_start() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("ran");
    let marker_text = marker.display().to_string();
    let err = script(
        &format!("touch '{marker_text}'"),
        &format!("type nul > \"{marker_text}\""),
    )
    .stdout_file(dir.path().join("missing/out.txt"))
    .run()
    .unwrap_err();
    assert!(format!("{err:#}").contains("out.txt"), "{err:#}");
    assert!(!marker.exists(), "the command must not start");
}

// Timeout (cross-platform part).

#[test]
fn timeout_that_is_not_hit_gives_normal_result() {
    let out = script("echo fast", "echo fast")
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .unwrap();
    assert_eq!(out, "fast");
}

#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;

    const KIB_100: usize = 100 * 1024;

    /// A shell command that prints `n` copies of `c` to stdout.
    fn bytes_of(c: char, n: usize) -> String {
        format!("head -c {n} /dev/zero | tr '\\0' '{c}'")
    }

    // Capture.

    #[test]
    fn capture_100_kib_on_each_stream_with_exit_3() {
        let c = sh::cmd("sh")
            .args([
                "-c",
                &format!(
                    "{}; {} >&2; exit 3",
                    bytes_of('o', KIB_100),
                    bytes_of('e', KIB_100)
                ),
            ])
            .capture()
            .unwrap();
        assert_eq!(c.code(), 3);
        assert_eq!(c.stdout().len(), KIB_100);
        assert_eq!(c.stderr().len(), KIB_100);
        assert!(c.stdout().bytes().all(|b| b == b'o'));
        assert!(c.stderr().bytes().all(|b| b == b'e'));
    }

    #[test]
    fn capture_non_executable_file_gives_126() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("not_executable.sh");
        fs::write(&file, "#!/bin/sh\nexit 0\n").unwrap();
        let err = sh::cmd(&file).capture().unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(126), "{err:#}");
    }

    #[test]
    fn capture_decodes_invalid_utf8_lossily() {
        let c = sh::cmd("sh")
            .args(["-c", r"printf '\377ok'; printf '\376err' >&2"])
            .capture()
            .unwrap();
        assert_eq!(c.stdout(), "\u{FFFD}ok");
        assert_eq!(c.stderr(), "\u{FFFD}err");
    }

    #[test]
    fn capture_death_by_signal_gives_128_plus_signal() {
        let c = sh::cmd("sh").args(["-c", "kill -9 $$"]).capture().unwrap();
        assert!(!c.success());
        assert_eq!(c.code(), 137);
    }

    // Tee.

    /// Runs `case` of `child_entry` in a new test process.
    fn spawn_case(case: &str) -> std::process::Child {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "unix::child_entry",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_ENV, case)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    /// Runs `case` of `child_entry` and gets its stdout and stderr.
    fn run_case(case: &str) -> (String, String) {
        let out = spawn_case(case).wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            out.status.success(),
            "case {case} failed:\n{stdout}\n{stderr}"
        );
        (stdout, stderr)
    }

    const CHILD_ENV: &str = "SMOP_SH_IO_CHILD_CASE";

    /// The cases that other tests run in a new process. Without the
    /// environment variable, this test does nothing.
    #[test]
    fn child_entry() {
        let Ok(case) = std::env::var(CHILD_ENV) else {
            return;
        };
        let noisy = || sh::cmd("sh").args(["-c", "echo smop-quiet-out; echo smop-quiet-err >&2"]);
        match case.as_str() {
            "tee" => {
                let out = sh::cmd("sh")
                    .args([
                        "-c",
                        "echo smop-tee-a; echo smop-tee-e >&2; sleep 1; echo smop-tee-b",
                    ])
                    .tee()
                    .output()
                    .unwrap();
                assert_eq!(out, "smop-tee-a\nsmop-tee-b");
            }
            "quiet" => {
                let dir = TempDir::new().unwrap();
                let log = dir.path().join("log.txt");
                noisy().quiet().run().unwrap();
                assert!(noisy().quiet().status().unwrap().success());
                noisy().quiet().tee().capture().unwrap();
                noisy().quiet().tee().output().unwrap();
                noisy().quiet().stderr_to_stdout().run().unwrap();
                noisy().quiet().stdout_to_stderr().run().unwrap();
                noisy()
                    .quiet()
                    .stderr_to_stdout()
                    .log_stdout(&log)
                    .run()
                    .unwrap();
                let text = fs::read_to_string(&log).unwrap();
                assert_eq!(text, "smop-quiet-out\nsmop-quiet-err\n");
            }
            "echo-quiet" => noisy().echo().quiet().run().unwrap(),
            other => panic!("unknown case {other}"),
        }
    }

    #[test]
    fn tee_shows_output_before_the_command_ends() {
        let mut child = spawn_case("tee");
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                tx.send((line.unwrap(), Instant::now())).unwrap();
            }
        });
        let mut seen_a = None;
        let mut seen_b = None;
        for (line, at) in rx {
            if line.contains("smop-tee-a") {
                seen_a = Some(at);
            }
            if line.contains("smop-tee-b") {
                seen_b = Some(at);
            }
        }
        reader.join().unwrap();
        let out = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert!(stderr.contains("smop-tee-e"), "{stderr}");
        let a = seen_a.expect("a is shown");
        let b = seen_b.expect("b is shown");
        // The child sleeps 1 s between a and b, so a arrived before it ended.
        assert!(b.duration_since(a) >= Duration::from_millis(700));
    }

    #[test]
    fn log_stdout_with_stderr_merged_keeps_order() {
        let dir = TempDir::new().unwrap();
        let log = dir.path().join("log.txt");
        let c = sh::cmd("sh")
            .args(["-c", "echo 1; echo 2 >&2; echo 3; echo 4 >&2"])
            .stderr_to_stdout()
            .log_stdout(&log)
            .capture()
            .unwrap();
        assert_eq!(c.stdout(), "1\n2\n3\n4\n");
        assert_eq!(c.stderr(), "");
        assert_eq!(fs::read_to_string(&log).unwrap(), "1\n2\n3\n4\n");
    }

    #[test]
    fn log_per_stream_with_run() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("out.log");
        let err = dir.path().join("err.log");
        fs::write(&out, "old old old\n").unwrap();
        sh::cmd("sh")
            .args(["-c", "echo o1; echo e1 >&2; echo o2"])
            .log_stdout(&out)
            .log_stderr(&err)
            .quiet()
            .run()
            .unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "o1\no2\n");
        assert_eq!(fs::read_to_string(&err).unwrap(), "e1\n");
    }

    #[test]
    fn log_stdout_with_stdout_file_writes_both_files() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("out.txt");
        let log = dir.path().join("log.txt");
        sh::cmd("echo")
            .arg("both")
            .stdout_file(&file)
            .log_stdout(&log)
            .run()
            .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "both\n");
        assert_eq!(fs::read_to_string(&log).unwrap(), "both\n");
    }

    #[test]
    fn log_of_a_merged_away_stream_is_an_error() {
        let dir = TempDir::new().unwrap();
        let log = dir.path().join("log.txt");
        let err = sh::cmd("true")
            .stderr_to_stdout()
            .log_stderr(&log)
            .run()
            .unwrap_err();
        assert!(err.to_string().contains("log_stderr"), "{err:#}");
        let err = sh::cmd("true")
            .stdout_to_stderr()
            .log_stdout(&log)
            .run()
            .unwrap_err();
        assert!(err.to_string().contains("log_stdout"), "{err:#}");
    }

    // Redirection.

    #[test]
    fn stderr_to_stdout_merges_in_order() {
        let c = sh::cmd("sh")
            .args(["-c", "echo out; echo err >&2; echo out2"])
            .stderr_to_stdout()
            .capture()
            .unwrap();
        assert_eq!(c.stdout(), "out\nerr\nout2\n");
        assert_eq!(c.stderr(), "");
    }

    #[test]
    fn stdout_to_stderr_merges_in_order() {
        let c = sh::cmd("sh")
            .args(["-c", "echo out; echo err >&2; echo out2"])
            .stdout_to_stderr()
            .capture()
            .unwrap();
        assert_eq!(c.stdout(), "");
        assert_eq!(c.stderr(), "out\nerr\nout2\n");
    }

    #[test]
    fn stdout_file_with_stderr_to_stdout_writes_both_streams() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("out.txt");
        sh::cmd("sh")
            .args(["-c", "echo out; echo err >&2"])
            .stdout_append(&file)
            .stderr_to_stdout()
            .run()
            .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "out\nerr\n");
    }

    #[test]
    fn last_merge_wins() {
        let c = sh::cmd("sh")
            .args(["-c", "echo out; echo err >&2"])
            .stderr_to_stdout()
            .stdout_to_stderr()
            .capture()
            .unwrap();
        assert_eq!(c.stdout(), "");
        assert_eq!(c.stderr(), "out\nerr\n");
        let c = sh::cmd("sh")
            .args(["-c", "echo out; echo err >&2"])
            .stdout_to_stderr()
            .stderr_to_stdout()
            .capture()
            .unwrap();
        assert_eq!(c.stdout(), "out\nerr\n");
        assert_eq!(c.stderr(), "");
    }

    // stdin.

    #[test]
    fn stdin_1_mib_round_trips_through_cat() {
        let input: Vec<u8> = (0..1 << 20).map(|i: u32| (i % 251) as u8).collect();
        let start = Instant::now();
        let out = sh::cmd("cat").stdin(&input).output_raw().unwrap();
        assert!(start.elapsed() < Duration::from_secs(10));
        assert!(out.status.success());
        assert_eq!(out.stdout.len(), input.len());
        assert!(out.stdout == input, "bytes differ");
    }

    #[test]
    fn stdin_from_str_and_string() {
        let lines = sh::cmd("cat").stdin("a\nb\n").output_lines().unwrap();
        assert_eq!(lines, ["a", "b"]);
        let text = String::from("x");
        assert_eq!(sh::cmd("cat").stdin(&text).output().unwrap(), "x");
        assert_eq!(sh::cmd("cat").stdin(text).output().unwrap(), "x");
    }

    #[test]
    fn stdin_with_run_replaces_the_terminal() {
        sh::cmd("sh")
            .args(["-c", r#"read x; test "$x" = yes"#])
            .stdin("yes\n")
            .run()
            .unwrap();
    }

    #[test]
    fn stdin_that_the_command_does_not_read_is_not_an_error() {
        let input = vec![b'x'; 1 << 20];
        sh::cmd("true").stdin(input).run().unwrap();
    }

    #[test]
    fn default_stdin_of_capture_is_empty() {
        let c = sh::cmd("cat").capture().unwrap();
        assert_eq!(c.stdout(), "");
    }

    // quiet.

    /// Returns `true` when a line of `text` is a line that `noisy` prints.
    fn has_noisy_line(text: &str) -> bool {
        text.lines()
            .any(|l| l == "smop-quiet-out" || l == "smop-quiet-err")
    }

    #[test]
    fn quiet_success_prints_nothing() {
        let (stdout, stderr) = run_case("quiet");
        assert!(!has_noisy_line(&stdout), "{stdout}");
        assert!(!has_noisy_line(&stderr), "{stderr}");
    }

    #[test]
    fn echo_quiet_prints_only_the_command_line() {
        let (stdout, stderr) = run_case("echo-quiet");
        assert!(stderr.contains("$ sh -c"), "{stderr}");
        assert!(!has_noisy_line(&stdout), "{stdout}");
        assert!(!has_noisy_line(&stderr), "{stderr}");
    }

    #[test]
    fn quiet_failure_error_has_the_tail_of_stderr() {
        let err = sh::cmd("sh")
            .args([
                "-c",
                &format!(
                    "{} >&2; echo >&2; echo smop-last-line >&2; exit 2",
                    bytes_of('x', 1 << 20)
                ),
            ])
            .quiet()
            .run()
            .unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(2), "{err:#}");
        let msg = err.to_string();
        assert!(msg.ends_with("smop-last-line\n"), "{msg}");
        assert!(msg.len() < 8 * 1024, "the tail is short: {}", msg.len());
        let e = command_error(&err);
        assert!(e.stderr().unwrap().contains("smop-last-line"));
    }

    #[test]
    fn quiet_failure_with_merged_stderr_has_the_tail() {
        let err = sh::cmd("sh")
            .args(["-c", "echo smop-merged-err >&2; exit 5"])
            .quiet()
            .stderr_to_stdout()
            .run()
            .unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(5));
        assert!(err.to_string().contains("smop-merged-err"), "{err:#}");
    }

    // timeout.

    #[test]
    fn timeout_kills_reaps_and_gives_124() {
        let start = Instant::now();
        let err = sh::cmd("sh")
            .args(["-c", "echo $$; exec sleep 10"])
            .timeout(Duration::from_millis(200))
            .capture()
            .unwrap_err();
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(sh::exit_code(&err), Some(124), "{err:#}");
        let e = command_error(&err);
        assert!(e.timed_out());
        assert_eq!(e.code(), 124);
        assert!(err.to_string().contains("timed out"), "{err}");
        let pid = e.stdout().unwrap().trim().to_string();
        assert!(!pid.is_empty());
        // A reaped process is gone. A zombie still answers `kill -0`.
        let alive = Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!alive.success(), "process {pid} is not reaped");
    }

    #[test]
    fn timeout_returns_what_was_captured() {
        let err = sh::cmd("sh")
            .args(["-c", "echo early; echo warn >&2; exec sleep 10"])
            .timeout(Duration::from_millis(300))
            .output()
            .unwrap_err();
        let e = command_error(&err);
        assert_eq!(e.stdout(), Some("early\n"));
        assert_eq!(e.stderr(), Some("warn\n"));
    }

    #[test]
    fn timeout_drains_both_streams_while_waiting() {
        let n = 256 * 1024;
        let err = sh::cmd("sh")
            .args([
                "-c",
                &format!(
                    "{}; {} >&2; exec sleep 10",
                    bytes_of('o', n),
                    bytes_of('e', n)
                ),
            ])
            .timeout(Duration::from_secs(2))
            .capture()
            .unwrap_err();
        let e = command_error(&err);
        assert!(e.timed_out());
        assert_eq!(e.stdout().unwrap().len(), n);
        assert_eq!(e.stderr().unwrap().len(), n);
    }

    #[test]
    fn timeout_with_run_and_status_gives_124() {
        let short = Duration::from_millis(100);
        let err = sh::cmd("sleep").arg("10").timeout(short).run().unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(124));
        assert_eq!(command_error(&err).stdout(), None);
        let err = sh::cmd("sleep")
            .arg("10")
            .timeout(short)
            .status()
            .unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(124));
        let err = sh::cmd("sleep")
            .arg("10")
            .timeout(short)
            .success()
            .unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(124));
    }

    #[test]
    fn timeout_does_not_wait_for_a_grandchild_that_keeps_the_pipe() {
        let start = Instant::now();
        let out = sh::cmd("sh")
            .args(["-c", "sleep 5 & echo hi"])
            .timeout(Duration::from_millis(300))
            .output()
            .unwrap();
        assert_eq!(out, "hi");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
    }

    // No deadlock.

    #[test]
    fn no_deadlock_when_both_streams_exceed_64_kib() {
        let n = 128 * 1024;
        let both = format!("{}; {} >&2", bytes_of('o', n), bytes_of('e', n));
        let cmd = || sh::cmd("sh").args(["-c", &both]);
        assert_eq!(cmd().output().unwrap().len(), n);
        assert_eq!(cmd().output_lines().unwrap().concat().len(), n);
        cmd().quiet().run().unwrap();
        let c = cmd().tee().quiet().capture().unwrap();
        assert_eq!((c.stdout().len(), c.stderr().len()), (n, n));
        let raw = cmd().output_raw().unwrap();
        assert_eq!((raw.stdout.len(), raw.stderr.len()), (n, n));
        let c = cmd()
            .stdin(vec![b'i'; n])
            .timeout(Duration::from_secs(30))
            .capture()
            .unwrap();
        assert_eq!((c.stdout().len(), c.stderr().len()), (n, n));
    }

    // spawn.

    #[test]
    fn spawn_rejects_options_it_can_not_honour() {
        let err = sh::cmd("true").stdin("x").spawn().unwrap_err();
        assert!(err.to_string().contains("spawn"), "{err:#}");
        assert!(
            sh::cmd("true")
                .timeout(Duration::from_secs(1))
                .spawn()
                .is_err()
        );
        assert!(sh::cmd("true").tee().spawn().is_err());
        assert!(sh::cmd("true").stderr_to_stdout().spawn().is_err());
        sh::cmd("true").quiet().spawn().unwrap().wait().unwrap();
    }
}
