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
fn handles_are_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<sh::ChildProcess>();
    check::<sh::ChildGuard>();
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

// The unit test `next_pause_doubles_to_the_cap_and_stops_at_the_deadline`
// in `src/sh.rs` checks that no sleep passes the deadline.

#[cfg(windows)]
mod windows {
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant};

    use smop::sh;
    use tempfile::TempDir;

    /// Returns `true` when process `pid` runs.
    fn runs(pid: &str) -> bool {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .any(|word| word == pid)
    }

    #[test]
    fn dropped_guard_kills_the_process_tree() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids.txt");
        // PowerShell starts `ping` as its own child, then waits for it.
        let script = format!(
            "$p = Start-Process ping -ArgumentList '-n','60','127.0.0.1' \
             -PassThru -WindowStyle Hidden; \
             Set-Content -Path '{}' -Value $p.Id; Wait-Process -Id $p.Id",
            pids.display()
        );
        let guard = sh::cmd("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .quiet()
            .spawn_guarded()
            .unwrap();
        sh::wait_until(Duration::from_secs(60), || {
            std::fs::read_to_string(&pids).is_ok_and(|t| !t.trim().is_empty())
        })
        .unwrap();
        let pid = std::fs::read_to_string(&pids).unwrap().trim().to_owned();
        assert!(runs(&pid), "ping {pid} does not run");
        drop(guard);
        let end = Instant::now() + Duration::from_secs(10);
        while runs(&pid) && Instant::now() < end {
            thread::sleep(Duration::from_millis(100));
        }
        let left = runs(&pid);
        if left {
            let _ = Command::new("taskkill").args(["/F", "/PID", &pid]).status();
        }
        assert!(!left, "the grandchild {pid} runs after the drop");
    }
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
    fn read_pids(pids: &Path) -> [u32; 2] {
        assert!(appears(pids, Duration::from_secs(10)), "no pid file");
        let text = std::fs::read_to_string(pids).unwrap();
        let mut ids = text.split_whitespace().map(|s| s.parse().unwrap());
        [ids.next().unwrap(), ids.next().unwrap()]
    }

    /// A shell that starts `sleep 60`, writes `$$ $!` to `$1`, then exits.
    fn shell_that_leaves_a_grandchild(pids: &Path) -> CommandBuilder {
        sh_with_file(
            r#"sleep 60 & echo "$$ $!" > "$1.tmp"; mv "$1.tmp" "$1""#,
            pids,
        )
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

    /// Asserts that each process in `pids` stops before `end`. Kills the
    /// processes that still run before the assert fails, so no test leaks
    /// them.
    fn assert_stop_by(pids: &[u32], end: Instant) {
        while Instant::now() < end && !pids.iter().all(|&pid| stopped(pid)) {
            thread::sleep(Duration::from_millis(10));
        }
        let running: Vec<u32> = pids.iter().copied().filter(|&p| !stopped(p)).collect();
        for &pid in &running {
            kill_now(pid);
        }
        assert!(running.is_empty(), "processes still run: {running:?}");
    }

    /// Kills a process that a test left. Ignores errors.
    fn kill_now(pid: u32) {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }

    /// Kills its processes on drop, so a failed test leaks none.
    struct Reap([u32; 2]);

    impl Drop for Reap {
        fn drop(&mut self) {
            for pid in self.0 {
                if !stopped(pid) {
                    kill_now(pid);
                }
            }
        }
    }

    /// A limit for a kill that is not part of the 1 s acceptance criterion.
    const SLOW_LIMIT: Duration = Duration::from_secs(5);

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
            "keep-tee" => {
                let out = sh::cmd("echo")
                    .arg("smop-tee")
                    .keep_output()
                    .tee()
                    .spawn()
                    .unwrap()
                    .wait_with_output()
                    .unwrap();
                assert_eq!(out.stdout(), "smop-tee\n");
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
    fn keep_output_with_tee_shows_and_keeps() {
        let stdout = run_case("keep-tee");
        assert!(stdout.contains("smop-tee"), "{stdout}");
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
        let [shell, grandchild] = read_pids(&pids);
        let _reap = Reap([shell, grandchild]);
        assert_eq!(child.id(), shell);
        // Kill the shell first. A dead grandchild would let the shell exit 0.
        child.kill().unwrap();
        let err = child.wait().unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(137), "{err:#}");
    }

    // spawn_guarded.

    #[test]
    fn dropped_guard_kills_the_shell_and_its_grandchild() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        let guard = shell_with_grandchild(&pids).spawn_guarded().unwrap();
        let [shell, grandchild] = read_pids(&pids);
        let _reap = Reap([shell, grandchild]);
        assert_eq!(guard.id(), shell);
        let start = Instant::now();
        drop(guard);
        assert_stop_by(&[shell, grandchild], start + KILL_LIMIT);
    }

    #[test]
    fn guard_dropped_in_a_panic_kills_its_group() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        let mut start = Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = shell_with_grandchild(&pids).spawn_guarded().unwrap();
            let _ = read_pids(&pids);
            start = Instant::now();
            panic!("smop test panic");
        }));
        assert!(result.is_err());
        let [shell, grandchild] = read_pids(&pids);
        assert_stop_by(&[shell, grandchild], start + KILL_LIMIT);
    }

    #[test]
    fn guard_wait_does_not_kill() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        // The shell exits at once. Its grandchild runs on.
        let guard = shell_that_leaves_a_grandchild(&pids)
            .spawn_guarded()
            .unwrap();
        let [shell, grandchild] = read_pids(&pids);
        let _reap = Reap([shell, grandchild]);
        guard.wait().unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(!stopped(grandchild), "wait killed the grandchild");
    }

    #[test]
    fn guard_kill_stops_the_group_and_reports_ok() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        let guard = shell_with_grandchild(&pids).spawn_guarded().unwrap();
        let [shell, grandchild] = read_pids(&pids);
        let start = Instant::now();
        guard.kill().unwrap();
        assert_stop_by(&[shell, grandchild], start + SLOW_LIMIT);
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
        // The timeout is long enough for the shell to write its pid file.
        let guard = shell_with_grandchild(&pids)
            .keep_output()
            .timeout(Duration::from_secs(2))
            .spawn_guarded()
            .unwrap();
        let [shell, grandchild] = read_pids(&pids);
        let err = guard.wait().unwrap_err();
        let end = Instant::now() + SLOW_LIMIT;
        assert_eq!(sh::exit_code(&err), Some(124), "{err:#}");
        assert_stop_by(&[shell, grandchild], end);
    }

    #[test]
    fn guard_timeout_kills_a_group_that_holds_the_output() {
        let dir = TempDir::new().unwrap();
        let pids = dir.path().join("pids");
        // The shell ends at once. Its grandchild holds the stdout pipe.
        let guard = shell_that_leaves_a_grandchild(&pids)
            .keep_output()
            .timeout(Duration::from_secs(2))
            .spawn_guarded()
            .unwrap();
        let err = guard.wait().unwrap_err();
        let end = Instant::now() + SLOW_LIMIT;
        let [_, grandchild] = read_pids(&pids);
        assert_eq!(sh::exit_code(&err), Some(124), "{err:#}");
        assert_stop_by(&[grandchild], end);
    }

    #[test]
    fn wait_with_output_timeout_gives_124_and_the_output() {
        let err = sh::cmd("sh")
            .args(["-c", "echo early; sleep 30"])
            .keep_output()
            .timeout(Duration::from_millis(200))
            .spawn()
            .unwrap()
            .wait_with_output()
            .unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(124), "{err:#}");
        assert_eq!(command_error(&err).stdout(), Some("early\n"));
    }

    #[test]
    fn quiet_spawn_wait_with_output_keeps_no_text() {
        let out = sh::cmd("sh")
            .args(["-c", "echo out; echo err >&2; exit 2"])
            .quiet()
            .spawn()
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert_eq!(out.code(), 2);
        assert_eq!((out.stdout(), out.stderr()), ("", ""));
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
