//! Tests for `smop::print` that need their own process: stdin null,
//! captured stderr, and a set `GITHUB_ACTIONS`.
//!
//! Each test starts this test binary again with `CHILD` set. The
//! `child_entry` test then runs that case. Annotation cases run only in a
//! child: in CI, an in-process `::error::` line makes a real annotation.
#![cfg(feature = "print")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{IsTerminal, Read};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use smop::print;

/// The case for `child_entry`.
const CHILD: &str = "SMOP_PRINT_CHILD";

/// The line that a case prints when it ends without a panic.
const CASE_DONE: &str = "smop-print-case-done";

/// A child that runs longer than this is a hang.
const HANG: Duration = Duration::from_secs(60);

/// The time that a prompt can use to fail or to return its default.
const PROMPT_LIMIT: Duration = Duration::from_secs(1);

/// The output of a case.
struct Output {
    stdout: String,
    stderr: String,
}

/// A command that runs `case` of `child_entry` in this test binary.
fn case_command(case: &str) -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(CHILD, case)
        .env_remove("CLICOLOR_FORCE")
        .env_remove("GITHUB_ACTIONS");
    cmd
}

/// Runs `cmd` with stdin null. Kills it after [`HANG`].
fn run(mut cmd: Command) -> Output {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let read = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            pipe.read_to_string(&mut text).unwrap();
            text
        })
    };
    let stdout = read(Box::new(child.stdout.take().unwrap()));
    let stderr = read(Box::new(child.stderr.take().unwrap()));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > HANG {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("the child did not stop in {HANG:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let out = Output {
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    };
    assert!(
        status.success() && out.stdout.contains(CASE_DONE),
        "the case failed ({status}):\nstdout: {:?}\nstderr: {:?}",
        out.stdout,
        out.stderr
    );
    out
}

/// Runs `case` with `GITHUB_ACTIONS` set to `github`, or not set.
fn run_case(case: &str, github: Option<&str>) -> Output {
    let mut cmd = case_command(case);
    if let Some(value) = github {
        cmd.env("GITHUB_ACTIONS", value);
    }
    run(cmd)
}

/// The stderr lines that the case wrote, without the test harness lines.
fn lines(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter(|line| !line.starts_with("thread ") && !line.starts_with("note: "))
        .collect()
}

/// Calls `ask` and makes sure that it returns in [`PROMPT_LIMIT`].
fn timed<T>(name: &str, ask: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let value = ask();
    let elapsed = start.elapsed();
    assert!(elapsed < PROMPT_LIMIT, "{name} used {elapsed:?}");
    value
}

/// The no-terminal checks. Stdin is null in each case that calls this.
fn check_prompts_without_a_terminal() {
    assert!(!std::io::stdin().is_terminal());
    assert!(timed("confirm_default", || print::confirm_default("Go?", true)).unwrap());
    assert!(!timed("confirm_default", || print::confirm_default("Go?", false)).unwrap());
    assert_eq!(
        timed("prompt_default", || print::prompt_default("Port", "8080")).unwrap(),
        "8080"
    );
    let errors = [
        (
            "confirm",
            timed("confirm", || print::confirm("Sure?").map(|_| ())),
        ),
        (
            "prompt",
            timed("prompt", || print::prompt("Name?").map(|_| ())),
        ),
        (
            "select",
            timed("select", || print::select("Pick", &["a", "b"]).map(|_| ())),
        ),
        (
            "multiselect",
            timed("multiselect", || {
                print::multiselect("Pick", &["a", "b"]).map(|_| ())
            }),
        ),
        (
            "password",
            timed("password", || print::password("Token").map(|_| ())),
        ),
    ];
    for (name, result) in errors {
        let err = result.expect_err(name).to_string();
        assert!(
            err.contains("no terminal") && err.contains("stdin is not a terminal"),
            "{name}: {err}"
        );
    }
}

/// The cases that other tests run in a new process. Without `CHILD`, this
/// test does nothing.
#[test]
fn child_entry() {
    let Ok(case) = std::env::var(CHILD) else {
        return;
    };
    match case.as_str() {
        "status" => {
            smop::info!("info {}", 1);
            smop::step!("step {}", 2);
        }
        "no_tty" => check_prompts_without_a_terminal(),
        "no_tty_pty" => {
            // A person could answer on stderr, but stdin is null.
            assert!(std::io::stderr().is_terminal(), "stderr is not a pty");
            check_prompts_without_a_terminal();
        }
        "annotations" => {
            print::ci_error("bad\nlock");
            print::ci_warning(format_args!("{} old", 2));
            let value = print::group("tests", || {
                eprintln!("inside");
                7
            });
            assert_eq!(value, 7);
        }
        "group_panic" => {
            let result = std::panic::catch_unwind(|| print::group("g", || panic!("boom")));
            assert!(result.is_err());
        }
        other => panic!("unknown case {other}"),
    }
    println!("{CASE_DONE}");
}

#[test]
fn info_and_step_print_to_stderr() {
    let out = run_case("status", None);
    assert_eq!(lines(&out.stderr), ["ℹ info 1", "→ step 2"]);
    assert!(!out.stdout.contains("info 1") && !out.stdout.contains("step 2"));
}

#[test]
fn prompts_do_not_wait_when_stdin_is_null() {
    run_case("no_tty", None);
}

#[test]
fn annotations_are_workflow_commands_on_github_actions() {
    let out = run_case("annotations", Some("true"));
    assert_eq!(
        lines(&out.stderr),
        [
            "::error::bad%0Alock",
            "::warning::2 old",
            "::group::tests",
            "inside",
            "::endgroup::",
        ]
    );
}

#[test]
fn annotations_are_plain_text_outside_github_actions() {
    for github in [None, Some("1"), Some("false")] {
        let out = run_case("annotations", github);
        assert_eq!(
            lines(&out.stderr),
            ["✗ bad", "lock", "⚠ 2 old", "→ tests", "inside"],
            "GITHUB_ACTIONS={github:?}"
        );
    }
}

#[test]
fn a_group_ends_when_its_body_panics() {
    let out = run_case("group_panic", Some("true"));
    let lines = lines(&out.stderr);
    assert_eq!(lines.first(), Some(&"::group::g"), "{lines:?}");
    assert_eq!(lines.last(), Some(&"::endgroup::"), "{lines:?}");
}

/// stderr is a terminal and stdin is null: console would read keys from
/// `/dev/tty`. util-linux `script` gives the child a pty.
#[cfg(target_os = "linux")]
#[test]
fn prompts_do_not_wait_when_stdin_is_null_and_stderr_is_a_terminal() {
    let exe = std::env::current_exe().unwrap();
    let line = format!(
        "'{}' --exact child_entry --nocapture --test-threads=1 < /dev/null",
        exe.display()
    );
    let mut cmd = Command::new("script");
    cmd.args(["-qec", &line, "/dev/null"])
        .env(CHILD, "no_tty_pty")
        .env_remove("GITHUB_ACTIONS");
    run(cmd);
}
