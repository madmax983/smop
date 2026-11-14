//! Tests for `smop::print` that need their own process: stdin null,
//! captured stderr, a pty, and a set `GITHUB_ACTIONS`.
//!
//! Each test starts this test binary again with `CHILD` set. The
//! `child_entry` test then runs that case. Annotation cases run only in a
//! child: in CI, an in-process `::error::` line makes a real annotation.
#![cfg(feature = "print")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{IsTerminal, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use smop::print;

/// The environment variable that names the case for `child_entry`.
const CHILD: &str = "SMOP_PRINT_CHILD";

/// The line that a case prints when it ends without a panic.
const CASE_DONE: &str = "smop-print-case-done";

/// A child that runs longer than this is a hang.
const HANG: Duration = Duration::from_secs(15);

/// The maximum time for a prompt to fail or to return its default.
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

/// Reads all of `pipe` on a thread. Bytes that are not UTF-8 become U+FFFD.
fn read_all(mut pipe: impl Read + Send + 'static) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Waits for `child` until `deadline`, then kills it.
fn wait_until(child: &mut Child, deadline: Instant) -> ExitStatus {
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("the child did not stop in {HANG:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Runs `cmd` with stdin null. Kills it after [`HANG`].
fn run(mut cmd: Command) -> Output {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = read_all(child.stdout.take().unwrap());
    let stderr = read_all(child.stderr.take().unwrap());
    let status = wait_until(&mut child, Instant::now() + HANG);
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
        "interactive" => {
            assert!(std::io::stdin().is_terminal() && std::io::stderr().is_terminal());
            assert!(print::confirm("smop-confirm").unwrap());
            assert_eq!(print::prompt("smop-prompt").unwrap(), "abc");
            assert_eq!(print::select("smop-select", &["a", "b"]).unwrap(), 1);
            assert_eq!(
                print::multiselect("smop-multi", &["a", "b", "c"]).unwrap(),
                [0, 2]
            );
            assert_eq!(print::password("smop-password").unwrap(), "secret");
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
fn a_group_ends_once_when_its_body_panics() {
    let out = run_case("group_panic", Some("true"));
    let github = lines(&out.stderr);
    assert_eq!(github.first(), Some(&"::group::g"), "{github:?}");
    assert_eq!(github.last(), Some(&"::endgroup::"), "{github:?}");
    let ends = github.iter().filter(|l| **l == "::endgroup::").count();
    assert_eq!(ends, 1, "{github:?}");

    let out = run_case("group_panic", None);
    let plain = lines(&out.stderr);
    assert_eq!(plain.first(), Some(&"→ g"), "{plain:?}");
    assert!(!plain.iter().any(|l| l.starts_with("::")), "{plain:?}");
}

/// Quotes `text` for `/bin/sh`.
#[cfg(unix)]
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// A command that runs `case` under `script`, so the child has a pty.
/// `redirect` goes after the command line, for example `< /dev/null`.
/// `None` if there is no `script`, outside CI. In CI, `script` must exist.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn pty_command(case: &str, redirect: &str) -> Option<Command> {
    let has_script = std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join("script").is_file()));
    if !has_script {
        assert!(std::env::var_os("CI").is_none(), "CI has no `script`");
        eprintln!("skipped: no `script` program");
        return None;
    }
    let exe = std::env::current_exe().unwrap();
    let line = format!(
        "{} --exact child_entry --nocapture --test-threads=1 {redirect}",
        shell_quote(&exe.to_string_lossy())
    );
    let mut cmd = Command::new("script");
    if cfg!(target_os = "linux") {
        // util-linux: -e gives the exit code of the child.
        cmd.args(["-qec", &line, "/dev/null"]);
    } else {
        cmd.args(["-q", "/dev/null", "/bin/sh", "-c", &line]);
    }
    cmd.env(CHILD, case)
        .env_remove("CLICOLOR_FORCE")
        .env_remove("GITHUB_ACTIONS");
    Some(cmd)
}

/// stderr is a terminal and stdin is null: console would read keys from
/// `/dev/tty`.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn prompts_do_not_wait_when_stdin_is_null_and_stderr_is_a_terminal() {
    if let Some(cmd) = pty_command("no_tty_pty", "< /dev/null") {
        run(cmd);
    }
}

/// Collects the pty output of `script` and waits for text in it.
#[cfg(target_os = "linux")]
struct Screen {
    chunks: std::sync::mpsc::Receiver<String>,
    seen: String,
    deadline: Instant,
}

#[cfg(target_os = "linux")]
impl Screen {
    /// Waits until `text` is on the screen. `false` after `limit`.
    fn wait_for(&mut self, text: &str, limit: Duration) -> bool {
        let until = (Instant::now() + limit).min(self.deadline);
        while !self.seen.contains(text) {
            let left = until.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(left) {
                Ok(chunk) => self.seen.push_str(&chunk),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return false,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("the child stopped before {text:?}:\n{}", self.seen)
                }
            }
        }
        true
    }

    /// Waits until `text` is on the screen, or panics at the deadline.
    fn expect(&mut self, text: &str) {
        assert!(self.wait_for(text, HANG), "no {text:?} in:\n{}", self.seen);
    }
}

/// A person answers each prompt on a pty. This shows that the prompts ask
/// when stdin and stderr are terminals.
#[cfg(target_os = "linux")]
#[test]
fn prompts_ask_when_stdin_and_stderr_are_terminals() {
    use std::io::Write;

    let Some(mut cmd) = pty_command("interactive", "") else {
        return;
    };
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut keys = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let stderr = read_all(child.stderr.take().unwrap());
    let (tx, chunks) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0; 4096];
        while let Ok(n @ 1..) = stdout.read(&mut buf) {
            if tx
                .send(String::from_utf8_lossy(&buf[..n]).into_owned())
                .is_err()
            {
                break;
            }
        }
    });
    let deadline = Instant::now() + HANG;
    let mut screen = Screen {
        chunks,
        seen: String::new(),
        deadline,
    };
    // Send the keys only after the prompt shows, so the child reads them.
    for (prompt, typed) in [
        ("smop-confirm", "y"),
        ("smop-prompt", "abc\r"),
        ("smop-select", "\x1b[B\r"),
        ("smop-multi", " \x1b[B\x1b[B \r"),
    ] {
        screen.expect(prompt);
        keys.write_all(typed.as_bytes()).unwrap();
    }
    // The password read discards input that comes before it starts, so
    // send the line again until the case ends.
    screen.expect("smop-password");
    loop {
        keys.write_all(b"secret\n").unwrap();
        if screen.wait_for(CASE_DONE, Duration::from_millis(500)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no {CASE_DONE:?} in:\n{}",
            screen.seen
        );
    }
    drop(keys);
    let status = wait_until(&mut child, deadline);
    assert!(
        status.success(),
        "the case failed ({status}):\n{}\n{}",
        screen.seen,
        stderr.join().unwrap()
    );
}
