//! Subprocess tests for `smop::script`.
//!
//! A test starts this test binary again as a child, with `SCENARIO` set. The
//! `child` test then runs that scenario. Interrupt state is global to a
//! process, so it is tested only in children.
#![cfg(feature = "script")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
#[cfg(feature = "script-signals")]
use std::time::{Duration, Instant};

use smop::prelude::*;
use smop::script;

const SCENARIO: &str = "SMOP_SCRIPT_SCENARIO";
const DIR: &str = "SMOP_SCRIPT_DIR";

/// Runs the scenario that `SCENARIO` names. Without it, does nothing.
#[test]
fn child() {
    let Ok(name) = std::env::var(SCENARIO) else {
        return;
    };
    let dir = PathBuf::from(std::env::var(DIR).unwrap_or_default());
    match name.as_str() {
        "ok" => script::main(|| Ok(())),
        "chain" => script::main(|| Err(anyhow!("root cause")).context("middle").context("top")),
        "error_code" => script::Main::new().error_code(2).run(|| bail!("internal")),
        "error_code_zero" => script::Main::new().error_code(0).run(|| bail!("internal")),
        "sh_code" => script::main(|| {
            if cfg!(windows) {
                sh::run("exit 7").context("step")
            } else {
                sh::cmd("sh").args(["-c", "exit 7"]).run().context("step")
            }
        }),
        "defer_fails" => script::main(|| {
            let failing = if cfg!(windows) {
                sh::cmd("cmd").args(["/C", "exit 3"])
            } else {
                sh::cmd("false")
            };
            let _cleanup = script::defer(failing);
            Ok(())
        }),
        "timed" => script::main(|| {
            let v = script::timed("step", || 5);
            ensure!(v == 5, "timed changed the value");
            Ok(())
        }),
        #[cfg(feature = "script-signals")]
        other => signals::scenario(other, &dir),
        #[cfg(not(feature = "script-signals"))]
        other => panic!("unknown scenario {other} in {}", dir.display()),
    }
}

/// Runs `scenario` in a child and waits for it.
fn run(scenario: &str, dir: &Path) -> Output {
    child_command(scenario, dir).output().unwrap()
}

/// Makes the command that runs `scenario` in a child.
fn child_command(scenario: &str, dir: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["child", "--exact", "--nocapture", "--test-threads=1"])
        .env(SCENARIO, scenario)
        .env(DIR, dir)
        // A forced colour would put escape codes in piped stderr.
        .env_remove("CLICOLOR_FORCE")
        // libtest prints to stdout. Keep the parent's output clean.
        .stdout(std::process::Stdio::null());
    command
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Waits until `path` exists. Panics after 20 s.
#[cfg(all(unix, feature = "script-signals"))]
fn wait_for(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "timeout: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn ok_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let out = run("ok", dir.path());
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(!stderr(&out).contains("error:"));
}

#[test]
fn error_prints_chain_and_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let out = run("chain", dir.path());
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(
        err.contains("error: top\ncaused by: middle\ncaused by: root cause\n"),
        "{err}"
    );
    // Piped stderr has no colour, also with `print`.
    assert!(!err.contains('\u{1b}'), "{err}");
}

#[test]
fn error_exits_with_configured_code() {
    let dir = tempfile::tempdir().unwrap();
    let out = run("error_code", dir.path());
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("error: internal"));
}

#[test]
fn error_code_zero_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let out = run("error_code_zero", dir.path());
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
}

#[test]
fn sh_error_exits_with_child_code() {
    let dir = tempfile::tempdir().unwrap();
    let out = run("sh_code", dir.path());
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(7), "{err}");
    assert!(err.contains("error: step\ncaused by: Command failed with exit code 7"));
}

#[test]
fn defer_failure_is_a_warning_only() {
    let dir = tempfile::tempdir().unwrap();
    let out = run("defer_fails", dir.path());
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert!(
        err.contains("warning: cleanup failed: Command failed"),
        "{err}"
    );
    assert!(!err.contains("error:"), "{err}");
}

#[test]
fn timed_prints_label_and_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let out = run("timed", dir.path());
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(0), "{err}");
    let line = err.lines().find(|l| l.starts_with("step: ")).unwrap();
    let secs = line
        .strip_prefix("step: ")
        .and_then(|s| s.strip_suffix('s'))
        .unwrap();
    assert!(secs.parse::<f64>().is_ok(), "{line}");
}

#[cfg(feature = "script-signals")]
mod signals {
    use super::*;

    /// Writes an empty file.
    fn touch(path: &Path) {
        std::fs::write(path, "").unwrap();
    }

    /// Makes a command that creates `path`.
    fn touch_cmd(path: &Path) -> sh::CommandBuilder {
        if cfg!(windows) {
            sh::cmd("cmd")
                .arg("/C")
                .arg(format!("echo x> {}", path.display()))
        } else {
            sh::cmd("touch").arg(path)
        }
    }

    /// Sends a signal to this process and waits until a guard sees it.
    fn interrupt_self(signal: &str) -> Result<()> {
        #[cfg(unix)]
        {
            let pid = std::process::id().to_string();
            std::process::Command::new("kill")
                .args([&format!("-{signal}"), &pid])
                .status()?;
        }
        #[cfg(windows)]
        {
            let _ = signal;
            signal_hook::low_level::raise(signal_hook::consts::SIGINT)?;
        }
        let start = Instant::now();
        while !script::interrupted() {
            ensure!(start.elapsed() < Duration::from_secs(20), "no interrupt");
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    /// Waits up to 20 s for a signal from the parent test.
    fn wait_interrupt() -> Result<()> {
        let start = Instant::now();
        while !script::interrupted() {
            ensure!(start.elapsed() < Duration::from_secs(20), "no interrupt");
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    // One arm for each scenario: the length is the number of scenarios.
    #[allow(clippy::too_many_lines)]
    pub fn scenario(name: &str, dir: &Path) -> ! {
        let dir = dir.to_path_buf();
        match name {
            // Cleanup runs, then the process dies of the signal.
            "wait_interrupt" => script::main(|| {
                let _guard = script::guard()?;
                let _cleanup = script::defer(touch_cmd(&dir.join("cleaned")));
                touch(&dir.join("ready"));
                wait_interrupt()?;
                touch_cmd(&dir.join("spawned")).run()
            }),
            "self_interrupt" => script::main(|| {
                let _guard = script::guard()?;
                let _cleanup = script::defer(touch_cmd(&dir.join("cleaned")));
                interrupt_self("INT")?;
                touch_cmd(&dir.join("spawned")).run()
            }),
            // No new child after an interrupt; cleanup commands still run.
            "blocked_spawn" => script::main(|| {
                let _guard = script::guard()?;
                interrupt_self("INT")?;
                let spawned = dir.join("spawned");
                let mut errors = vec![
                    touch_cmd(&spawned).run().unwrap_err(),
                    touch_cmd(&spawned).output().unwrap_err(),
                    touch_cmd(&spawned).output_raw().unwrap_err(),
                    touch_cmd(&spawned).status().unwrap_err(),
                    touch_cmd(&spawned).success().unwrap_err(),
                    touch_cmd(&spawned).spawn().unwrap_err(),
                    touch_cmd(&spawned).spawn_guarded().unwrap_err(),
                    sh::wait_until(Duration::from_secs(30), || false).unwrap_err(),
                    sh::run("echo x").unwrap_err(),
                    sh::output("echo x").unwrap_err(),
                    sh::shell("echo x").run().unwrap_err(),
                    touch_cmd(&spawned).capture().unwrap_err(),
                    touch_cmd(&spawned).output_lines().unwrap_err(),
                    touch_cmd(&spawned)
                        .stdin("x")
                        .quiet()
                        .timeout(Duration::from_secs(30))
                        .run()
                        .unwrap_err(),
                ];
                if cfg!(unix) {
                    errors.push(
                        sh::cmd("echo")
                            .arg("x")
                            .pipe(sh::cmd("cat"))
                            .run()
                            .unwrap_err(),
                    );
                    errors.push(
                        sh::cmd("echo")
                            .arg("x")
                            .pipe(sh::cmd("cat"))
                            .output()
                            .unwrap_err(),
                    );
                }
                for err in &errors {
                    let int = err
                        .chain()
                        .find_map(|e| e.downcast_ref::<script::Interrupted>());
                    ensure!(int.is_some(), "no interrupt marker: {err:#}");
                    ensure!(sh::exit_code(err) == Some(130), "code: {err:#}");
                }
                ensure!(!spawned.exists(), "a child started");
                touch_cmd(&dir.join("cleaned")).cleanup().run()?;
                // A cleanup command with I/O options also runs.
                let cleaned = touch_cmd(&dir.join("cleaned2"))
                    .cleanup()
                    .quiet()
                    .timeout(Duration::from_secs(30))
                    .capture()?;
                ensure!(cleaned.success(), "cleanup capture failed");
                ensure!(dir.join("cleaned2").exists(), "cleanup capture did not run");
                if cfg!(unix) {
                    let piped = sh::cmd("echo").arg("x").cleanup().pipe(sh::cmd("cat"));
                    ensure!(piped.output()? == "x", "cleanup pipe did not run");
                }
                touch(&dir.join("passed"));
                // `Ok` after an interrupt also stops by the signal.
                Ok(())
            }),
            // `retry` stops after the attempt that saw the interrupt.
            "retry_interrupt" => script::main(|| {
                let _guard = script::guard()?;
                let mut calls = 0;
                let err = script::retry(5, Duration::from_secs(30), || -> Result<()> {
                    calls += 1;
                    interrupt_self("INT")?;
                    bail!("attempt {calls}")
                })
                .unwrap_err();
                ensure!(calls == 1, "calls: {calls}");
                let text = format!("{err:#}");
                ensure!(text.contains("last error: attempt 1"), "{text}");
                let marker = err
                    .chain()
                    .find_map(|e| e.downcast_ref::<script::Interrupted>());
                ensure!(marker.is_some(), "no interrupt marker: {err:#}");
                ensure!(sh::exit_code(&err) == Some(130), "code: {err:#}");
                // After the interrupt, `retry` does not call `f` at all.
                let mut late = 0;
                let err = script::retry(3, Duration::ZERO, || -> Result<()> {
                    late += 1;
                    Ok(())
                })
                .unwrap_err();
                ensure!(late == 0, "calls after the interrupt: {late}");
                ensure!(
                    err.downcast_ref::<script::Interrupted>().is_some(),
                    "{err:#}"
                );
                touch(&dir.join("passed"));
                Ok(())
            }),
            // An interrupt in the last attempt also gives `Interrupted`.
            "retry_last_attempt" => script::main(|| {
                let _guard = script::guard()?;
                let err = script::retry(1, Duration::ZERO, || -> Result<()> {
                    interrupt_self("INT")?;
                    bail!("last")
                })
                .unwrap_err();
                let text = format!("{err:#}");
                ensure!(sh::exit_code(&err) == Some(130), "code: {text}");
                ensure!(text.contains("last error: last"), "{text}");
                touch(&dir.join("passed"));
                Ok(())
            }),
            // `f`'s error holds the interrupt already: name it only once.
            "retry_sh_blocked" => script::main(|| {
                let _guard = script::guard()?;
                let err = script::retry(3, Duration::ZERO, || {
                    interrupt_self("INT")?;
                    touch_cmd(&dir.join("spawned")).run()
                })
                .unwrap_err();
                let text = format!("{err:#}");
                ensure!(sh::exit_code(&err) == Some(130), "code: {text}");
                ensure!(text.contains("Command not started"), "{text}");
                ensure!(text.matches("interrupted by").count() == 1, "{text}");
                touch(&dir.join("passed"));
                Ok(())
            }),
            // A guard that caught no signal: the default action is back
            // after it drops.
            "drop_quiet" => script::main(|| {
                drop(script::guard()?);
                touch(&dir.join("ready"));
                std::thread::sleep(Duration::from_secs(20));
                touch(&dir.join("survived"));
                Ok(())
            }),
            // An interrupt stays: a new guard after it lets the next signal
            // take the default action.
            "guard_after_interrupt" => script::main(|| {
                {
                    let _guard = script::guard()?;
                    touch(&dir.join("ready"));
                    wait_interrupt()?;
                }
                let _guard = script::guard()?;
                touch(&dir.join("second_guard"));
                std::thread::sleep(Duration::from_secs(20));
                touch(&dir.join("survived"));
                Ok(())
            }),
            // The default action comes back when the guard drops.
            "drop_guard" => script::main(|| {
                {
                    let _guard = script::guard()?;
                    touch(&dir.join("ready"));
                    wait_interrupt()?;
                }
                touch(&dir.join("dropped"));
                std::thread::sleep(Duration::from_secs(20));
                touch(&dir.join("survived"));
                Ok(())
            }),
            // A second signal while a guard lives kills the process.
            "second_signal" => script::main(|| {
                let _guard = script::guard()?;
                touch(&dir.join("ready"));
                wait_interrupt()?;
                touch(&dir.join("first"));
                std::thread::sleep(Duration::from_secs(20));
                touch(&dir.join("survived"));
                Ok(())
            }),
            // An interrupt during a command that exits 0: the target stays.
            #[cfg(unix)]
            "atomic_interrupt" => script::main(|| {
                let _guard = script::guard()?;
                let target = dir.join("out.txt");
                std::fs::write(&target, "old")?;
                let me = std::process::id().to_string();
                let err = sh::cmd("sh")
                    .args(["-c", r#"kill -INT "$1"; sleep 0.3; echo partial"#])
                    .arg("sh")
                    .arg(&me)
                    .stdout_atomic(&target)
                    .run()
                    .unwrap_err();
                ensure!(script::interrupted(), "no interrupt: {err:#}");
                ensure!(std::fs::read_to_string(&target)? == "old", "target changed");
                let names = std::fs::read_dir(&dir)?.count();
                ensure!(names == 1, "a temp file stays");
                touch(&dir.join("passed"));
                Ok(())
            }),
            // After an interrupt, a dropped `ChildGuard` still kills its group.
            #[cfg(unix)]
            "guarded_interrupt" => script::main(|| {
                let _guard = script::guard()?;
                let pids = dir.join("pids");
                let _server = sh::cmd("sh")
                    .args([
                        "-c",
                        r#"sleep 60 & echo "$$ $!" > "$1.tmp"; mv "$1.tmp" "$1"; wait"#,
                    ])
                    .arg("sh")
                    .arg(&pids)
                    .quiet()
                    .spawn_guarded()?;
                sh::wait_until(Duration::from_secs(20), || pids.exists())?;
                interrupt_self("INT")?;
                touch(&dir.join("passed"));
                Ok(())
            }),
            // An interrupt in `ChildGuard::wait` kills the group. The group
            // does not get the signal itself.
            #[cfg(unix)]
            "guarded_wait_interrupt" => script::main(|| {
                let _guard = script::guard()?;
                let pids = dir.join("pids");
                let server = sh::cmd("sh")
                    .args([
                        "-c",
                        r#"sleep 60 & echo "$$ $!" > "$1.tmp"; mv "$1.tmp" "$1"; wait"#,
                    ])
                    .arg("sh")
                    .arg(&pids)
                    .quiet()
                    .spawn_guarded()?;
                sh::wait_until(Duration::from_secs(20), || pids.exists())?;
                let me = std::process::id().to_string();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(200));
                    let _ = std::process::Command::new("kill")
                        .args(["-INT", &me])
                        .status();
                });
                let start = Instant::now();
                let err = server.wait().unwrap_err();
                ensure!(
                    start.elapsed() < Duration::from_secs(10),
                    "wait did not stop"
                );
                ensure!(sh::exit_code(&err) == Some(130), "code: {err:#}");
                touch(&dir.join("passed"));
                Ok(())
            }),
            // An interrupt stops an `http` retry wait. No new attempt starts.
            #[cfg(feature = "http")]
            "http_retry_interrupt" => script::main(|| {
                let _guard = script::guard()?;
                let mut server = mockito::Server::new();
                let busy = server
                    .mock("GET", "/")
                    .with_status(503)
                    .with_header("retry-after", "30")
                    .expect(1)
                    .create();
                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_millis(300));
                    let _ = interrupt_self("INT");
                });
                let start = Instant::now();
                let err = http::Client::new()
                    .retry(http::Retry::new(3))
                    .get(&server.url())
                    .unwrap_err();
                let one_hit = busy.matched();
                drop(server);
                ensure!(
                    start.elapsed() < Duration::from_secs(10),
                    "wait did not stop"
                );
                let marker = err
                    .chain()
                    .find_map(|e| e.downcast_ref::<script::Interrupted>());
                ensure!(marker.is_some(), "no interrupt marker: {err:#}");
                ensure!(format!("{err:#}").contains("status 503"), "{err:#}");
                ensure!(sh::exit_code(&err) == Some(130), "code: {err:#}");
                ensure!(one_hit, "{err:#}");
                touch(&dir.join("passed"));
                Ok(())
            }),
            other => panic!("unknown scenario {other}"),
        }
    }

    /// Runs `scenario`, which writes `passed`, then stops by SIGINT.
    fn assert_passed_then_interrupted(scenario: &str) {
        let dir = tempfile::tempdir().unwrap();
        let out = run(scenario, dir.path());
        assert!(dir.path().join("passed").exists(), "{}", stderr(&out));
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(out.status.signal(), Some(2), "{:?}", out.status);
        }
        #[cfg(windows)]
        assert_eq!(out.status.code(), Some(130), "{:?}", out.status);
    }

    #[test]
    fn retry_interrupt_in_last_attempt_gives_marker() {
        assert_passed_then_interrupted("retry_last_attempt");
    }

    #[cfg(feature = "http")]
    #[test]
    fn an_interrupt_stops_an_http_retry_wait() {
        assert_passed_then_interrupted("http_retry_interrupt");
    }

    #[test]
    fn retry_names_the_interrupt_once() {
        assert_passed_then_interrupted("retry_sh_blocked");
    }

    #[cfg(unix)]
    #[test]
    fn an_interrupt_stops_the_stdout_atomic_rename() {
        assert_passed_then_interrupted("atomic_interrupt");
    }

    #[cfg(unix)]
    mod unix {
        use super::super::*;
        use std::os::unix::process::{CommandExt, ExitStatusExt};

        fn kill(signal: &str, target: &str) {
            let status = Command::new("kill")
                .args([&format!("-{signal}"), "--", target])
                .status()
                .unwrap();
            assert!(status.success());
        }

        #[test]
        fn sigint_runs_cleanup_then_dies_of_sigint() {
            let dir = tempfile::tempdir().unwrap();
            let child = child_command("wait_interrupt", dir.path())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            wait_for(&dir.path().join("ready"));
            kill("INT", &child.id().to_string());
            let out = child.wait_with_output().unwrap();
            let err = stderr(&out);
            assert_eq!(out.status.signal(), Some(2), "{:?} {err}", out.status);
            assert!(dir.path().join("cleaned").exists(), "{err}");
            assert!(!dir.path().join("spawned").exists(), "{err}");
            assert!(err.contains("error: Command not started"), "{err}");
            assert!(err.contains("caused by: interrupted by SIGINT"), "{err}");
        }

        #[test]
        fn sigterm_dies_of_sigterm() {
            let dir = tempfile::tempdir().unwrap();
            let child = child_command("wait_interrupt", dir.path()).spawn().unwrap();
            wait_for(&dir.path().join("ready"));
            kill("TERM", &child.id().to_string());
            let out = child.wait_with_output().unwrap();
            assert_eq!(out.status.signal(), Some(15), "{:?}", out.status);
            assert!(dir.path().join("cleaned").exists());
        }

        #[test]
        fn sighup_dies_of_sighup() {
            let dir = tempfile::tempdir().unwrap();
            let child = child_command("wait_interrupt", dir.path()).spawn().unwrap();
            wait_for(&dir.path().join("ready"));
            kill("HUP", &child.id().to_string());
            let out = child.wait_with_output().unwrap();
            assert_eq!(out.status.signal(), Some(1), "{:?}", out.status);
            assert!(dir.path().join("cleaned").exists());
        }

        #[test]
        fn bash_loop_stops_at_first_interrupt() {
            let dir = tempfile::tempdir().unwrap();
            let iters = dir.path().join("iters");
            let exe = std::env::current_exe().unwrap();
            // Ctrl-C in a terminal signals the whole process group.
            let mut bash = Command::new("bash");
            bash.arg("-c")
                .arg(r#"for i in 1 2 3; do "$0" child --exact --nocapture --test-threads=1; echo "$i" >> "$1"; done"#)
                .arg(&exe)
                .arg(&iters)
                .env(SCENARIO, "wait_interrupt")
                .env(DIR, dir.path())
                .process_group(0);
            let mut bash = bash.spawn().unwrap();
            wait_for(&dir.path().join("ready"));
            kill("INT", &format!("-{}", bash.id()));
            let status = bash.wait().unwrap();
            assert_eq!(status.signal(), Some(2), "{status:?}");
            assert!(!iters.exists(), "the loop went on");
            assert!(dir.path().join("cleaned").exists());
        }

        #[test]
        fn interrupt_blocks_new_children_but_not_cleanup() {
            let dir = tempfile::tempdir().unwrap();
            let out = run("blocked_spawn", dir.path());
            let err = stderr(&out);
            assert!(dir.path().join("passed").exists(), "{err}");
            assert!(!dir.path().join("spawned").exists());
            assert!(dir.path().join("cleaned").exists());
            // `Ok` after an interrupt: no error text, the process dies of SIGINT.
            assert_eq!(out.status.signal(), Some(2), "{:?} {err}", out.status);
            assert!(!err.contains("error:"), "{err}");
        }

        /// Returns `true` when process `pid` stops within 5 s. A zombie
        /// does not run.
        fn stops(pid: &str) -> bool {
            let end = Instant::now() + Duration::from_secs(5);
            loop {
                let out = Command::new("ps")
                    .args(["-o", "stat=", "-p", pid])
                    .output()
                    .unwrap();
                let stat = String::from_utf8_lossy(&out.stdout);
                if stat.trim().is_empty() || stat.trim().starts_with('Z') {
                    return true;
                }
                if Instant::now() >= end {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        /// Runs `scenario`, then checks that it passed, that SIGINT ended
        /// it, and that each process in its `pids` file stopped.
        fn assert_group_stopped(scenario: &str) {
            let dir = tempfile::tempdir().unwrap();
            let start = Instant::now();
            let out = run(scenario, dir.path());
            // A process that the kill missed holds the stderr pipe of the
            // scenario, so `run` would take 60 s.
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "a process ran on"
            );
            let err = stderr(&out);
            assert!(dir.path().join("passed").exists(), "{err}");
            assert_eq!(out.status.signal(), Some(2), "{:?} {err}", out.status);
            let pids = std::fs::read_to_string(dir.path().join("pids")).unwrap();
            for pid in pids.split_whitespace() {
                assert!(stops(pid), "process {pid} runs");
            }
        }

        #[test]
        fn interrupt_in_a_guarded_wait_kills_the_group() {
            assert_group_stopped("guarded_wait_interrupt");
        }

        #[test]
        fn interrupt_ends_the_script_and_its_guarded_group() {
            assert_group_stopped("guarded_interrupt");
        }

        #[test]
        fn retry_stops_once_interrupted() {
            let dir = tempfile::tempdir().unwrap();
            let start = Instant::now();
            let out = run("retry_interrupt", dir.path());
            assert!(dir.path().join("passed").exists(), "{}", stderr(&out));
            assert_eq!(out.status.signal(), Some(2), "{:?}", out.status);
            assert!(start.elapsed() < Duration::from_secs(20));
        }

        #[test]
        fn default_action_is_back_after_guard_drops() {
            let dir = tempfile::tempdir().unwrap();
            let child = child_command("drop_guard", dir.path()).spawn().unwrap();
            let pid = child.id().to_string();
            wait_for(&dir.path().join("ready"));
            kill("TERM", &pid);
            wait_for(&dir.path().join("dropped"));
            kill("TERM", &pid);
            let out = child.wait_with_output().unwrap();
            assert_eq!(out.status.signal(), Some(15), "{:?}", out.status);
            assert!(!dir.path().join("survived").exists());
        }

        #[test]
        fn default_action_is_back_after_quiet_guard_drops() {
            let dir = tempfile::tempdir().unwrap();
            let child = child_command("drop_quiet", dir.path()).spawn().unwrap();
            wait_for(&dir.path().join("ready"));
            kill("TERM", &child.id().to_string());
            let out = child.wait_with_output().unwrap();
            assert_eq!(out.status.signal(), Some(15), "{:?}", out.status);
            assert!(!dir.path().join("survived").exists());
        }

        #[test]
        fn new_guard_after_interrupt_lets_next_signal_kill() {
            let dir = tempfile::tempdir().unwrap();
            let child = child_command("guard_after_interrupt", dir.path())
                .spawn()
                .unwrap();
            let pid = child.id().to_string();
            wait_for(&dir.path().join("ready"));
            kill("INT", &pid);
            wait_for(&dir.path().join("second_guard"));
            kill("INT", &pid);
            let out = child.wait_with_output().unwrap();
            assert_eq!(out.status.signal(), Some(2), "{:?}", out.status);
            assert!(!dir.path().join("survived").exists());
        }

        #[test]
        fn second_signal_while_guarded_kills() {
            let dir = tempfile::tempdir().unwrap();
            let child = child_command("second_signal", dir.path()).spawn().unwrap();
            let pid = child.id().to_string();
            wait_for(&dir.path().join("ready"));
            kill("INT", &pid);
            wait_for(&dir.path().join("first"));
            kill("INT", &pid);
            let out = child.wait_with_output().unwrap();
            assert_eq!(out.status.signal(), Some(2), "{:?}", out.status);
            assert!(!dir.path().join("survived").exists());
        }
    }

    #[cfg(windows)]
    mod windows {
        use super::super::*;

        #[test]
        fn ctrl_c_exits_130_after_cleanup() {
            let dir = tempfile::tempdir().unwrap();
            let out = run("self_interrupt", dir.path());
            let err = stderr(&out);
            assert_eq!(out.status.code(), Some(130), "{err}");
            assert!(dir.path().join("cleaned").exists(), "{err}");
            assert!(!dir.path().join("spawned").exists(), "{err}");
        }

        #[test]
        fn interrupt_blocks_new_children_but_not_cleanup() {
            let dir = tempfile::tempdir().unwrap();
            let out = run("blocked_spawn", dir.path());
            assert!(dir.path().join("passed").exists(), "{}", stderr(&out));
            assert_eq!(out.status.code(), Some(130), "{}", stderr(&out));
            assert!(!dir.path().join("spawned").exists());
            assert!(dir.path().join("cleaned").exists());
        }

        #[test]
        fn retry_stops_once_interrupted() {
            let dir = tempfile::tempdir().unwrap();
            let out = run("retry_interrupt", dir.path());
            assert!(dir.path().join("passed").exists(), "{}", stderr(&out));
            assert_eq!(out.status.code(), Some(130), "{}", stderr(&out));
        }
    }
}
