//! Tests for background processes: `spawn`, `spawn_guarded` and
//! `wait_until` (issue #45).
//!
//! A case that checks the terminal of the test process runs in a new test
//! process (see `unix::child_entry`). The parent reads its stdout.

#![cfg(feature = "sh")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::thread;
use std::time::{Duration, Instant};

use smop::sh::{self, CommandBuilder};

/// Runs a command that sleeps for 60 s: `sleep` on Unix, `ping` on
/// Windows.
fn sleeper() -> CommandBuilder {
    if cfg!(windows) {
        sh::cmd("cmd").args(["/C", "ping -n 60 127.0.0.1 > NUL"])
    } else {
        sh::cmd("sh").args(["-c", "sleep 60"])
    }
}

// spawn_guarded on each platform. The `unix` module checks grandchildren.

#[test]
fn dropped_guard_stops_its_command_at_once() {
    let guard = sleeper().spawn_guarded().unwrap();
    assert!(guard.id() > 0);
    let start = Instant::now();
    drop(guard);
    // Drop reaps the command. A failed kill would wait 60 s.
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn guard_kill_is_ok_for_a_running_command() {
    let start = Instant::now();
    sleeper().spawn_guarded().unwrap().kill().unwrap();
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn guard_kill_is_ok_after_the_command_ended() {
    let guard = if cfg!(windows) {
        sh::cmd("cmd").args(["/C", "exit 0"])
    } else {
        sh::cmd("true")
    }
    .spawn_guarded()
    .unwrap();
    thread::sleep(Duration::from_millis(300));
    guard.kill().unwrap();
}

#[test]
fn guard_debug_names_the_command() {
    let guard = sleeper().spawn_guarded().unwrap();
    let text = format!("{guard:?}");
    assert!(text.contains("ChildGuard") && text.contains("60"), "{text}");
}

// wait_until.

#[test]
fn wait_until_is_ok_when_the_probe_succeeds() {
    let mut calls = 0;
    sh::wait_until(Duration::from_secs(10), || {
        calls += 1;
        calls == 3
    })
    .unwrap();
    assert_eq!(calls, 3);
}

#[test]
fn wait_until_error_names_the_timeout() {
    let start = Instant::now();
    let err = sh::wait_until(Duration::from_millis(150), || false).unwrap_err();
    let elapsed = start.elapsed();
    assert!(err.to_string().contains("150ms"), "{err:#}");
    assert!(elapsed >= Duration::from_millis(150), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
    assert_eq!(sh::exit_code(&err), None, "{err:#}");
}

#[test]
fn wait_until_with_zero_timeout_probes_one_time() {
    let mut calls = 0;
    let err = sh::wait_until(Duration::ZERO, || {
        calls += 1;
        false
    })
    .unwrap_err();
    assert_eq!(calls, 1, "{err:#}");
    sh::wait_until(Duration::ZERO, || true).unwrap();
}

#[test]
fn wait_until_does_not_sleep_past_the_deadline() {
    // The pause between probes grows. It must stop at the deadline.
    let start = Instant::now();
    let _ = sh::wait_until(Duration::from_millis(30), || false);
    assert!(start.elapsed() < Duration::from_millis(500));
}

#[cfg(unix)]
mod unix {
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use smop::sh::{self, CommandBuilder};
    use tempfile::TempDir;

    /// Gets the `CommandError` from an error chain.
    fn command_error(err: &anyhow::Error) -> &sh::CommandError {
        err.chain()
            .find_map(|e| e.downcast_ref::<sh::CommandError>())
            .unwrap_or_else(|| panic!("no CommandError in: {err:#}"))
    }

    /// The environment variable that selects a case of `child_entry`.
    const CHILD_ENV: &str = "SMOP_SH_SPAWN_CHILD_CASE";

    /// The line that a case prints when it ends without a panic.
    const CASE_DONE: &str = "smop-case-done";

    /// The number of bytes that fill a pipe buffer three times on Linux.
    const BIG: usize = 200_000;

    /// The longest time that a guard may take to stop its group.
    const KILL_LIMIT: Duration = Duration::from_secs(1);

    /// Runs `script` with `sh -c`. `$1` is `file`.
    fn sh_with_file(script: &str, file: &Path) -> CommandBuilder {
        sh::cmd("sh").args(["-c", script, "sh"]).arg(file)
    }

    /// Writes [`BIG`] bytes of `o` to stdout, then makes `$1`.
    fn big_writer(done: &Path) -> CommandBuilder {
        sh_with_file(
            &format!("head -c {BIG} /dev/zero | tr '\\0' o; touch \"$1\""),
            done,
        )
    }

    /// A shell that starts `sleep 60`, writes `$$ $!` to `$1`, then waits.
    fn shell_with_grandchild(pids: &Path) -> CommandBuilder {
        sh_with_file(
            r#"sleep 60 & echo "$$ $!" > "$1.tmp"; mv "$1.tmp" "$1"; wait"#,
            pids,
        )
    }

    /// Returns `true` when `path` exists before `limit` ends.
    fn appears(path: &Path, limit: Duration) -> bool {
        let end = Instant::now() + limit;
        while Instant::now() < end {
            if path.exists() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        path.exists()
    }

    /// Waits for the file of [`shell_with_grandchild`]. Gets the shell and
    /// the grandchild process ids.
    fn read_pids(pids: &Path) -> (u32, u32) {
        assert!(appears(pids, Duration::from_secs(10)), "no pid file");
        let text = std::fs::read_to_string(pids).unwrap();
        let mut ids = text.split_whitespace().map(|s| s.parse().unwrap());
        (ids.next().unwrap(), ids.next().unwrap())
    }

    /// Returns `true` when process `pid` does not run. A zombie does not
    /// run.
    fn stopped(pid: u32) -> bool {
        let out = Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let stat = String::from_utf8_lossy(&out.stdout);
        let stat = stat.trim();
        stat.is_empty() || stat.starts_with('Z')
    }

    /// Returns `true` when process `pid` stops before `limit` ends.
    fn stops_within(pid: u32, limit: Duration) -> bool {
        let end = Instant::now() + limit;
        while Instant::now() < end {
            if stopped(pid) {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        stopped(pid)
    }

    /// Kills a process that a test left. Ignores errors.
    fn kill_now(pid: u32) {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }

    /// Runs `case` of `child_entry` in a new test process. Gets its stdout.
    fn run_case(case: &str) -> String {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "unix::child_entry",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_ENV, case)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "case {case} failed:\n{stderr}");
        assert!(stdout.contains(CASE_DONE), "case {case} did not run");
        stdout
    }

    /// The entry of a case. It does nothing in a normal test run.
    #[test]
    fn child_entry() {
        let Ok(case) = std::env::var(CHILD_ENV) else {
            return;
        };
        let dir = TempDir::new().unwrap();
        let done = dir.path().join("done");
        match case.as_str() {
            "big-default" => {
                let child = big_writer(&done).spawn().unwrap();
                assert!(
                    appears(&done, Duration::from_secs(10)),
                    "the child blocked on a full pipe"
                );
                child.wait().unwrap();
            }
            "big-guarded" => {
                let guard = big_writer(&done).spawn_guarded().unwrap();
                assert!(
                    appears(&done, Duration::from_secs(10)),
                    "the child blocked on a full pipe"
                );
                guard.wait().unwrap();
            }
            "default-output-not-kept" => {
                let out = sh::cmd("echo")
                    .arg("smop-inherited")
                    .spawn()
                    .unwrap()
                    .wait_with_output()
                    .unwrap();
                assert!(out.success());
                assert_eq!(out.stdout(), "");
            }
            other => panic!("unknown case {other}"),
        }
        println!("{CASE_DONE}");
    }

    // Regression: a child that writes more than a pipe buffer must not block.

    #[test]
    fn default_spawn_of_a_big_writer_ends_and_inherits_stdout() {
        let stdout = run_case("big-default");
        let os = stdout.bytes().filter(|&b| b == b'o').count();
        assert!(os >= BIG, "the child stdout did not reach the terminal");
    }

    #[test]
    fn guarded_spawn_of_a_big_writer_ends_and_inherits_stdout() {
        let stdout = run_case("big-guarded");
        let os = stdout.bytes().filter(|&b| b == b'o').count();
        assert!(os >= BIG, "the child stdout did not reach the terminal");
    }

    #[test]
    fn wait_with_output_of_a_default_spawn_has_no_text() {
        let stdout = run_case("default-output-not-kept");
        assert!(stdout.contains("smop-inherited"), "{stdout}");
    }

    #[test]
    fn quiet_spawn_of_a_big_writer_ends() {
        let dir = TempDir::new().unwrap();
        let done = dir.path().join("done");
        let child = big_writer(&done).quiet().spawn().unwrap();
        assert!(appears(&done, Duration::from_secs(10)));
        child.wait().unwrap();
    }

    // Opt-in capture.

    #[test]
    fn keep_output_drains_while_nobody_waits() {
        let dir = TempDir::new().unwrap();
        let done = dir.path().join("done");
        let child = big_writer(&done).keep_output().spawn().unwrap();
        // Threads read the pipes, so the child ends before `wait`.
        assert!(appears(&done, Duration::from_secs(10)));
        let out = child.wait_with_output().unwrap();
        assert!(out.success());
        assert_eq!(out.code(), 0);
        assert_eq!(out.stdout().len(), BIG);
        assert!(out.stdout().bytes().all(|b| b == b'o'));
    }

    #[test]
    fn wait_with_output_keeps_a_failure_as_a_value() {
        let out = sh::cmd("sh")
            .args(["-c", "echo out; echo err >&2; exit 3"])
            .keep_output()
            .spawn()
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert!(!out.success());
        assert_eq!(out.code(), 3);
        assert_eq!(out.stdout(), "out\n");
        assert_eq!(out.stderr(), "err\n");
    }

    #[test]
    fn wait_of_a_kept_failure_has_the_output() {
        let err = sh::cmd("sh")
            .args(["-c", "echo out; echo err >&2; exit 3"])
            .keep_output()
            .spawn()
            .unwrap()
            .wait()
            .unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(3), "{err:#}");
        assert_eq!(command_error(&err).stdout(), Some("out\n"));
        assert_eq!(command_error(&err).stderr(), Some("err\n"));
    }

    #[test]
    fn spawn_honours_redirection_and_stdin() {
        let dir = TempDir::new().unwrap();
        let log = dir.path().join("serve.log");
        sh::cmd("sh")
            .args(["-c", "echo out; echo err >&2"])
            .stdout_file(&log)
            .stderr_to_stdout()
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "out\nerr\n");

        let out = sh::cmd("cat")
            .stdin("in")
            .keep_output()
            .spawn()
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert_eq!(out.stdout(), "in");
    }

    #[test]
    fn quiet_spawn_wait_error_has_the_stderr_tail() {
        let err = sh::cmd("sh")
            .args(["-c", "echo smop-tail >&2; exit 4"])
            .quiet()
            .spawn()
            .unwrap()
            .wait()
            .unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(4));
        assert_eq!(command_error(&err).stderr(), Some("smop-tail\n"));
    }

    // Errors of `ChildProcess::wait` hold a `CommandError` (#34).

    #[test]
    fn spawn_timeout_gives_124() {
        let start = Instant::now();
        let err = sh::cmd("sleep")
            .arg("30")
            .timeout(Duration::from_millis(100))
            .spawn()
            .unwrap()
            .wait()
            .unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(sh::exit_code(&err), Some(124), "{err:#}");
        assert!(command_error(&err).timed_out());
    }

    #[test]
    fn child_id_is_the_process_id() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        let mut child = shell_with_grandchild(&pids).spawn().unwrap();
        let (shell, grandchild) = read_pids(&pids);
        assert_eq!(child.id(), shell);
        child.kill().unwrap();
        let err = child.wait().unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(137), "{err:#}");
        kill_now(grandchild);
    }

    // spawn_guarded.

    #[test]
    fn dropped_guard_kills_the_shell_and_its_grandchild() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        let guard = shell_with_grandchild(&pids).spawn_guarded().unwrap();
        let (shell, grandchild) = read_pids(&pids);
        assert_eq!(guard.id(), shell);
        let start = Instant::now();
        drop(guard);
        assert!(stops_within(shell, KILL_LIMIT), "the shell runs");
        assert!(stops_within(grandchild, KILL_LIMIT), "the grandchild runs");
        assert!(start.elapsed() < KILL_LIMIT * 2);
    }

    #[test]
    fn guard_dropped_in_a_panic_kills_its_group() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        let result = std::panic::catch_unwind(|| {
            let _guard = shell_with_grandchild(&pids).spawn_guarded().unwrap();
            let _ = read_pids(&pids);
            panic!("smop test panic");
        });
        assert!(result.is_err());
        let (shell, grandchild) = read_pids(&pids);
        assert!(stops_within(shell, KILL_LIMIT), "the shell runs");
        assert!(stops_within(grandchild, KILL_LIMIT), "the grandchild runs");
    }

    #[test]
    fn guard_wait_does_not_kill() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        // The shell exits at once. Its grandchild runs on.
        let guard = sh_with_file(
            r#"sleep 60 & echo "$$ $!" > "$1.tmp"; mv "$1.tmp" "$1""#,
            &pids,
        )
        .spawn_guarded()
        .unwrap();
        guard.wait().unwrap();
        let (_, grandchild) = read_pids(&pids);
        thread::sleep(Duration::from_millis(100));
        let alive = !stopped(grandchild);
        kill_now(grandchild);
        assert!(alive, "wait killed the grandchild");
    }

    #[test]
    fn guard_kill_stops_the_group_and_reports_ok() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        let guard = shell_with_grandchild(&pids).spawn_guarded().unwrap();
        let (shell, grandchild) = read_pids(&pids);
        guard.kill().unwrap();
        assert!(stops_within(shell, KILL_LIMIT));
        assert!(stops_within(grandchild, KILL_LIMIT));
    }

    #[test]
    fn guard_wait_with_output_gives_the_kept_output() {
        let out = sh::cmd("sh")
            .args(["-c", "echo guarded"])
            .keep_output()
            .spawn_guarded()
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert_eq!(out.stdout(), "guarded\n");
    }

    #[test]
    fn guard_timeout_kills_the_group() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        let guard = shell_with_grandchild(&pids)
            .keep_output()
            .timeout(Duration::from_millis(300))
            .spawn_guarded()
            .unwrap();
        let (_, grandchild) = read_pids(&pids);
        let err = guard.wait().unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(124), "{err:#}");
        assert!(stops_within(grandchild, KILL_LIMIT), "the grandchild runs");
    }

    #[test]
    fn spawn_guarded_of_a_missing_program_gives_127() {
        let err = sh::cmd("smop_missing_program").spawn_guarded().unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(127), "{err:#}");
    }

    #[test]
    fn wait_until_waits_for_a_guarded_server() {
        let dir = TempDir::new().unwrap();
        let ready = dir.path().join("ready");
        let _server = sh_with_file(r#"sleep 0.2; touch "$1"; sleep 60"#, &ready)
            .spawn_guarded()
            .unwrap();
        sh::wait_until(Duration::from_secs(10), || ready.exists()).unwrap();
    }
}
