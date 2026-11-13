//! The lifecycle of a script: exit codes, interrupts, cleanup and retries.
//!
//! - [`main`] runs the script. It prints an error and exits with a shell
//!   exit code.
//! - `guard` (`script-signals` feature) catches Ctrl-C, SIGTERM and SIGHUP
//!   while it lives. After an interrupt, `sh` starts no new command.
//! - [`defer`] runs a cleanup command at the end of its scope, as
//!   `trap … EXIT` does at the end of a bash script.
//! - [`retry`] and [`timed`] wrap one step.
//!
//! ```no_run
//! use std::time::Duration;
//! use smop::prelude::*;
//!
//! fn main() {
//!     script::main(|| {
//!         # #[cfg(feature = "script-signals")]
//!         let _guard = script::guard()?;
//!         let _cleanup = script::defer(sh::cmd("docker").args(["rm", "-f", "db"]));
//!         sh::cmd("docker").args(["run", "-d", "--name", "db", "postgres"]).run()?;
//!         script::retry(5, Duration::from_secs(1), || {
//!             sh::cmd("pg_isready").args(["-h", "localhost"]).run()
//!         })?;
//!         script::timed("tests", || sh::cargo().arg("test").run())
//!     })
//! }
//! ```

use std::fmt;
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};

use crate::sh::CommandBuilder;

/// Runs `f`, then ends the process with an exit code.
///
/// The same as `Main::new().run(f)`. See [`Main::run`] for the exit codes.
///
/// # Examples
///
/// ```no_run
/// use smop::prelude::*;
///
/// fn main() {
///     script::main(|| {
///         sh::cargo().args(["test", "--workspace"]).run()?;
///         Ok(())
///     })
/// }
/// ```
pub fn main<F: FnOnce() -> Result<()>>(f: F) -> ! {
    Main::new().run(f)
}

/// Settings for [`main`].
///
/// # Examples
///
/// ```no_run
/// use smop::prelude::*;
///
/// fn main() {
///     // An internal error exits with 2. A failed gate exits with 1.
///     script::Main::new().error_code(2).run(|| {
///         if !sh::cargo().arg("test").success()? {
///             std::process::exit(1);
///         }
///         Ok(())
///     })
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Main {
    error_code: u8,
}

impl Main {
    /// Makes the default settings. An error without a code exits with 1.
    #[must_use]
    pub const fn new() -> Self {
        Self { error_code: 1 }
    }

    /// Sets the exit code for an error that has no code of its own.
    ///
    /// A value of 0 becomes 1. An error never exits with 0.
    #[must_use]
    pub const fn error_code(mut self, code: u8) -> Self {
        self.error_code = if code == 0 { 1 } else { code };
        self
    }

    /// Runs `f`, then ends the process with an exit code.
    ///
    /// An error prints to stderr as `error: <message>`, then one
    /// `caused by: <message>` line for each source. With the `print`
    /// feature, the labels have colour on a terminal.
    ///
    /// The process ends in this order of priority:
    ///
    /// 1. After an interrupt (see `guard`), with `Ok` or an error: on Unix,
    ///    the process stops by the same signal, so a calling shell loop
    ///    stops too. On Windows, the exit code is 130.
    /// 2. `Ok`: exit code 0.
    /// 3. An error that holds a [`sh::CommandError`](crate::sh::CommandError):
    ///    the code of the command (see [`sh::exit_code`](crate::sh::exit_code)).
    /// 4. A different error: the code from [`error_code`](Self::error_code),
    ///    1 by default.
    ///
    /// When `f` returns, its local values drop, so its [`defer`] commands
    /// run. Then `run` ends the process. A `defer` value outside `f` does
    /// not run.
    pub fn run<F: FnOnce() -> Result<()>>(self, f: F) -> ! {
        let result = f();
        if let Err(err) = &result {
            report(err);
        }
        let interrupt = result
            .as_ref()
            .err()
            .and_then(|err| {
                err.chain()
                    .find_map(|e| e.downcast_ref::<Interrupted>().copied())
            })
            .or_else(|| caught().map(|signal| Interrupted { signal }));
        if let Some(interrupt) = interrupt {
            end_by(interrupt);
        }
        match result {
            Ok(()) => exit(0),
            Err(err) => {
                exit(crate::sh::command_code(&err).unwrap_or_else(|| i32::from(self.error_code)))
            }
        }
    }
}

impl Default for Main {
    fn default() -> Self {
        Self::new()
    }
}

/// Prints the error chain to stderr.
fn report(err: &anyhow::Error) {
    let mut stderr = io::stderr().lock();
    for (i, cause) in err.chain().enumerate() {
        let label = if i == 0 { "error:" } else { "caused by:" };
        // No other channel can show an error of stderr. Ignore it.
        let _ = writeln!(stderr, "{} {cause}", paint(label));
    }
}

/// Gives `label` colour on a terminal.
#[cfg(feature = "print")]
fn paint(label: &str) -> impl fmt::Display {
    console::style(label).red().bold().for_stderr()
}

/// Gives `label` as it is (no `print` feature).
#[cfg(not(feature = "print"))]
const fn paint(label: &str) -> impl fmt::Display + '_ {
    label
}

/// Flushes stdout and ends the process with `code`.
fn exit(code: i32) -> ! {
    let _ = io::stdout().flush();
    std::process::exit(code)
}

/// Ends the process after an interrupt.
///
/// On Unix, the default action of the signal stops the process. On
/// Windows, or if that fails, the process exits with the code of `interrupt`.
fn end_by(interrupt: Interrupted) -> ! {
    #[cfg(all(unix, feature = "script-signals"))]
    {
        let _ = io::stdout().flush();
        let _ = signal_hook::low_level::emulate_default_handler(interrupt.signal);
    }
    exit(interrupt.code())
}

/// The error of a step that did not start, because a `guard` caught an
/// interrupt.
///
/// `sh` commands and [`retry`] give an error that holds it. Search for it with
/// `err.chain().find_map(|e| e.downcast_ref::<Interrupted>())`.
/// [`sh::exit_code`](crate::sh::exit_code) gives its code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interrupted {
    signal: i32,
}

impl Interrupted {
    /// Gets the signal number, for example 2 for SIGINT.
    #[must_use]
    pub const fn signal(&self) -> i32 {
        self.signal
    }

    /// Gets the shell exit code: 128 + signal on Unix, 130 on Windows.
    #[must_use]
    pub const fn code(&self) -> i32 {
        if cfg!(windows) {
            130
        } else {
            128 + self.signal
        }
    }
}

impl fmt::Display for Interrupted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match signal_name(self.signal) {
            Some(name) => write!(f, "interrupted by {name}"),
            None => write!(f, "interrupted by signal {}", self.signal),
        }
    }
}

impl std::error::Error for Interrupted {}

/// Gets the name of a signal that a guard catches.
const fn signal_name(signal: i32) -> Option<&'static str> {
    match signal {
        2 => Some("SIGINT"),
        15 => Some("SIGTERM"),
        #[cfg(unix)]
        1 => Some("SIGHUP"),
        #[cfg(windows)]
        21 => Some("SIGBREAK"),
        _ => None,
    }
}

/// The last signal that a guard caught. 0 is none.
///
/// It stays set after the guard drops, so that [`main`] can see it.
static SIGNAL: AtomicUsize = AtomicUsize::new(0);

/// The number of live guards.
static GUARDS: AtomicUsize = AtomicUsize::new(0);

/// Gets the signal that a guard caught.
fn caught() -> Option<i32> {
    match SIGNAL.load(Ordering::SeqCst) {
        0 => None,
        signal => i32::try_from(signal).ok(),
    }
}

/// Returns `true` after a `guard` caught a signal.
///
/// It stays `true` after the guard drops. A long loop that does not call
/// `sh` can use it to stop.
///
/// # Examples
///
/// ```no_run
/// # fn work() {}
/// use smop::script;
///
/// # #[cfg(feature = "script-signals")]
/// # fn f() -> smop::Result<()> {
/// let _guard = script::guard()?;
/// for _ in 0..1000 {
///     if script::interrupted() {
///         break;
///     }
///     work();
/// }
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn interrupted() -> bool {
    caught().is_some()
}

/// Gets the interrupt when a guard lives and caught a signal.
///
/// `sh` and [`retry`] stop when this is `Some`.
pub(crate) fn stop() -> Option<Interrupted> {
    if GUARDS.load(Ordering::SeqCst) == 0 {
        return None;
    }
    caught().map(|signal| Interrupted { signal })
}

#[cfg(feature = "script-signals")]
pub use signals::{Guard, guard};

/// The interrupt guard. ADR 0011 records why it uses `signal-hook`.
#[cfg(feature = "script-signals")]
mod signals {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock, PoisonError};

    use anyhow::Result;
    #[cfg(windows)]
    use signal_hook::consts::signal::SIGBREAK;
    use signal_hook::consts::signal::SIGINT;
    #[cfg(unix)]
    use signal_hook::consts::signal::{SIGHUP, SIGTERM};
    use signal_hook::flag;

    use super::{GUARDS, SIGNAL, caught};

    /// The signals that a guard catches.
    #[cfg(unix)]
    const SIGNALS: &[i32] = &[SIGINT, SIGTERM, SIGHUP];
    /// The signals that a guard catches: Ctrl-C and Ctrl-Break.
    #[cfg(windows)]
    const SIGNALS: &[i32] = &[SIGINT, SIGBREAK];

    /// `true`: a signal gets its default action. It is `false` only while
    /// a guard lives and no signal came.
    ///
    /// The handlers stay for the life of the process (`signal-hook` cannot
    /// remove them), so this flag turns them off.
    static DEFAULT: AtomicBool = AtomicBool::new(true);

    /// The result of the one-time handler setup.
    static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();

    /// Lets only one thread make or drop a guard at a time.
    static LOCK: Mutex<()> = Mutex::new(());

    /// Catches interrupts while it lives.
    ///
    /// See [`guard`].
    #[derive(Debug)]
    #[must_use = "the guard stops when it drops; bind it: `let _guard = script::guard()?;`"]
    pub struct Guard {
        _private: (),
    }

    /// Makes an interrupt guard.
    ///
    /// While a guard lives, a signal does not stop the process. The guard
    /// keeps the signal, and then:
    ///
    /// - `sh` starts no new command. It returns an error that holds
    ///   [`Interrupted`](super::Interrupted). A command with
    ///   [`cleanup`](crate::sh::CommandBuilder::cleanup) still runs.
    /// - [`retry`](super::retry) stops.
    /// - [`interrupted`](super::interrupted) returns `true`.
    /// - When the script function returns, [`main`](super::main) stops the
    ///   process by the signal (Unix) or exits with 130 (Windows).
    /// - A second signal gets its default action. Send a second signal to
    ///   stop a script that does not stop.
    ///
    /// When the last guard drops, signals get their default action again.
    ///
    /// The signals are SIGINT (Ctrl-C), SIGTERM and SIGHUP on Unix, and
    /// Ctrl-C and Ctrl-Break on Windows. A command that runs in the
    /// foreground gets Ctrl-C from the terminal too, and usually stops.
    ///
    /// The handlers stay installed for the life of the process. Thus, a
    /// signal that was ignored at start (for example SIGHUP under `nohup`)
    /// is caught while a guard lives, and gets its default action after
    /// that. On Windows, Ctrl-C without a guard then exits with 130.
    ///
    /// # Errors
    ///
    /// Returns an error when a signal handler cannot be installed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::prelude::*;
    ///
    /// fn main() {
    ///     script::main(|| {
    ///         let _guard = script::guard()?;
    ///         let _cleanup = script::defer(sh::cmd("docker").args(["rm", "-f", "db"]));
    ///         sh::cmd("docker").args(["run", "--name", "db", "postgres"]).run()
    ///     })
    /// }
    /// ```
    pub fn guard() -> Result<Guard> {
        let _lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        INSTALLED
            .get_or_init(install)
            .clone()
            .map_err(anyhow::Error::msg)?;
        // Only the first guard sets the flag. A nested guard must not clear
        // a flag that a signal set. After an interrupt, the flag stays set,
        // so the next signal gets its default action.
        if GUARDS.fetch_add(1, Ordering::SeqCst) == 0 {
            DEFAULT.store(caught().is_some(), Ordering::SeqCst);
        }
        Ok(Guard { _private: () })
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            let _lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
            if GUARDS.fetch_sub(1, Ordering::SeqCst) == 1 {
                DEFAULT.store(true, Ordering::SeqCst);
            }
        }
    }

    /// Installs the handlers for all [`SIGNALS`].
    fn install() -> Result<(), String> {
        SIGNALS.iter().try_for_each(|&signal| {
            install_one(signal)
                .map_err(|e| format!("Failed to install a handler for signal {signal}: {e}"))
        })
    }

    /// Installs the three actions for `signal`. They run in this order.
    fn install_one(signal: i32) -> std::io::Result<()> {
        // 1. Without a live guard, or at the second signal: the default action.
        #[cfg(unix)]
        flag::register_conditional_default(signal, &DEFAULT)?;
        #[cfg(windows)]
        flag::register_conditional_shutdown(signal, 130, &DEFAULT)?;
        // 2. Keep the signal number. 3. Arm the default action for the next one.
        let number = usize::try_from(signal).map_err(std::io::Error::other)?;
        flag::register_usize(signal, &SIGNAL, number)?;
        flag::register(signal, &DEFAULT)?;
        Ok(())
    }
}

/// A cleanup command that runs when this value drops.
///
/// Make it with [`defer`].
#[derive(Debug)]
#[must_use = "the command runs when this value drops; bind it: `let _cleanup = script::defer(..);`"]
pub struct Defer {
    command: Option<CommandBuilder>,
}

/// Runs `command` at the end of the scope, as `trap … EXIT` does at the end
/// of a bash script.
///
/// The command runs when the returned value drops. This is also after an
/// error, a panic (with `panic = "unwind"`) or an interrupt that a `guard`
/// caught. It runs as a
/// [`cleanup`](CommandBuilder::cleanup) command. If it fails, a warning
/// prints to stderr and the script continues.
///
/// Values drop in reverse order, so the last `defer` runs first.
///
/// Bind the value to a name: `let _cleanup = defer(..)`. `let _ = defer(..)`
/// drops it at once. It does not run when the process stops by
/// `std::process::exit` or by a signal without a guard.
///
/// # Examples
///
/// ```no_run
/// use smop::prelude::*;
///
/// # fn f() -> Result<()> {
/// sh::cmd("docker").args(["run", "-d", "--name", "db", "postgres"]).run()?;
/// let _cleanup = script::defer(sh::cmd("docker").args(["rm", "-f", "db"]));
/// sh::cargo().arg("test").run()?;
/// # Ok(())
/// # }
/// ```
pub const fn defer(command: CommandBuilder) -> Defer {
    Defer {
        command: Some(command),
    }
}

impl Drop for Defer {
    fn drop(&mut self) {
        if let Some(command) = self.command.take()
            && let Err(err) = command.cleanup().run()
        {
            let _ = writeln!(io::stderr(), "warning: cleanup failed: {err:#}");
        }
    }
}

/// The longest sleep before [`retry`] checks for an interrupt.
const SLICE: Duration = Duration::from_millis(50);

/// Calls `f` until it returns `Ok`, at most `attempts` times.
///
/// The first `Ok` is the result. After failed attempt `n`, `retry` waits
/// `backoff × 2^(n-1)`: `backoff`, then `2 × backoff`, then `4 × backoff`.
/// It does not wait after the last attempt. The factor stops at
/// `u32::MAX`.
///
/// # Errors
///
/// - `attempts` is 0. `f` is not called.
/// - All attempts fail. The error is `failed after <attempts> attempts`,
///   and its source is the last error.
/// - A `guard` caught an interrupt. `retry` does not start a new attempt.
///   The error holds [`Interrupted`], also when the interrupt came in the
///   last attempt. The error also holds or quotes the last error.
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
/// use smop::prelude::*;
///
/// // Waits 1 s, 2 s, 4 s and 8 s between the 5 attempts.
/// script::retry(5, Duration::from_secs(1), || sh::run("curl -fsS localhost:8080/health"))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn retry<T, F: FnMut() -> Result<T>>(attempts: u32, backoff: Duration, mut f: F) -> Result<T> {
    ensure!(attempts > 0, "retry needs at least 1 attempt, got 0");
    if let Some(interrupt) = stop() {
        return Err(interrupt.into());
    }
    let mut n = 1;
    loop {
        let err = match f() {
            Ok(value) => return Ok(value),
            Err(err) => err,
        };
        if let Some(interrupt) = stop() {
            return Err(stopped(err, interrupt, n, attempts));
        }
        if n == attempts {
            let s = if n == 1 { "" } else { "s" };
            return Err(err.context(format!("failed after {n} attempt{s}")));
        }
        if let Some(interrupt) = wait(delay(backoff, n)) {
            return Err(stopped(err, interrupt, n, attempts));
        }
        n += 1;
    }
}

/// Makes the error of a `retry` that an interrupt stopped after attempt `n`.
///
/// The chain is a list, so it cannot hold both `Interrupted` and an
/// unrelated last error. `Interrupted` sets the exit code, so keep it. Put
/// the last error in the message.
fn stopped(err: anyhow::Error, interrupt: Interrupted, n: u32, attempts: u32) -> anyhow::Error {
    let message = format!("retry stopped after attempt {n} of {attempts}");
    if err.chain().any(<dyn std::error::Error>::is::<Interrupted>) {
        return err.context(message);
    }
    anyhow::Error::new(interrupt).context(format!("{message} (last error: {err:#})"))
}

/// Gets the wait after failed attempt `n` (from 1): `backoff × 2^(n-1)`.
fn delay(backoff: Duration, n: u32) -> Duration {
    let factor = 2u32.checked_pow(n.saturating_sub(1)).unwrap_or(u32::MAX);
    backoff.saturating_mul(factor)
}

/// Sleeps for `time`. Stops early and gives the interrupt when a guard
/// catches one. `http` retries use it too.
pub(crate) fn wait(time: Duration) -> Option<Interrupted> {
    let start = Instant::now();
    loop {
        if let Some(interrupt) = stop() {
            return Some(interrupt);
        }
        let left = time.saturating_sub(start.elapsed());
        if left.is_zero() {
            return None;
        }
        std::thread::sleep(left.min(SLICE));
    }
}

/// Calls `f`, prints `<label>: <seconds>s` to stderr, and returns the value
/// of `f`.
///
/// # Examples
///
/// ```no_run
/// use smop::prelude::*;
///
/// // Prints "build: 12.34s".
/// script::timed("build", || sh::cargo().arg("build").run())?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn timed<T, F: FnOnce() -> T>(label: &str, f: F) -> T {
    let start = Instant::now();
    let value = f();
    let secs = start.elapsed().as_secs_f64();
    let _ = writeln!(io::stderr(), "{label}: {secs:.2}s");
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sh;
    use std::cell::Cell;

    #[test]
    fn retry_first_ok_wins() {
        let calls = Cell::new(0);
        let value = retry(3, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            if calls.get() < 2 {
                anyhow::bail!("not yet")
            }
            Ok(calls.get())
        })
        .unwrap();
        assert_eq!((value, calls.get()), (2, 2));
    }

    #[test]
    fn retry_calls_at_most_attempts_times() {
        let calls = Cell::new(0);
        let err = retry(3, Duration::ZERO, || -> Result<()> {
            calls.set(calls.get() + 1);
            anyhow::bail!("fail {}", calls.get())
        })
        .unwrap_err();
        assert_eq!(calls.get(), 3);
        assert_eq!(err.to_string(), "failed after 3 attempts");
        let source = err.chain().nth(1).unwrap();
        assert_eq!(source.to_string(), "fail 3");
    }

    #[test]
    fn retry_one_attempt_does_not_wait() {
        let start = std::time::Instant::now();
        let err = retry(1, Duration::from_secs(60), || -> Result<()> {
            anyhow::bail!("x")
        })
        .unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(err.to_string(), "failed after 1 attempt");
    }

    #[test]
    fn retry_zero_attempts_is_an_error_without_a_call() {
        let calls = Cell::new(0);
        let err = retry(0, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap_err();
        assert_eq!(calls.get(), 0);
        assert!(err.to_string().contains("at least 1 attempt"), "{err}");
    }

    #[test]
    fn retry_keeps_command_error_code() {
        let err = retry(2, Duration::ZERO, || sh::run("exit 4")).unwrap_err();
        assert_eq!(sh::exit_code(&err), Some(4));
    }

    #[test]
    fn retry_waits_between_attempts() {
        let start = std::time::Instant::now();
        let _ = retry(3, Duration::from_millis(20), || -> Result<()> {
            anyhow::bail!("x")
        });
        // 20 ms, then 40 ms.
        assert!(start.elapsed() >= Duration::from_millis(60));
    }

    #[test]
    fn backoff_doubles_and_saturates() {
        let b = Duration::from_millis(100);
        assert_eq!(delay(b, 1), b);
        assert_eq!(delay(b, 2), b * 2);
        assert_eq!(delay(b, 3), b * 4);
        assert_eq!(delay(Duration::ZERO, 40), Duration::ZERO);
        let max = Duration::from_secs(u64::from(u32::MAX));
        assert_eq!(delay(Duration::from_secs(1), 33), max);
        assert_eq!(delay(Duration::from_secs(1), u32::MAX), max);
        assert_eq!(delay(Duration::MAX, 2), Duration::MAX);
    }

    #[test]
    fn timed_returns_the_value() {
        assert_eq!(timed("step", || 42), 42);
        let r: Result<u8> = timed("step", || anyhow::bail!("no"));
        assert!(r.is_err());
    }

    #[test]
    fn defer_runs_its_command_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("cleaned");
        {
            let _cleanup = defer(touch(&marker));
            assert!(!marker.exists());
        }
        assert!(marker.exists());
    }

    #[test]
    fn defer_ignores_failure() {
        drop(defer(sh::cmd("smop-no-such-program-12345")));
        let failing = if cfg!(windows) {
            sh::cmd("cmd").args(["/C", "exit 3"])
        } else {
            sh::cmd("false")
        };
        drop(defer(failing));
    }

    #[test]
    fn no_interrupt_without_a_guard() {
        assert!(!interrupted());
        assert!(stop().is_none());
    }

    #[test]
    fn interrupted_names_the_signal_and_code() {
        let int = Interrupted { signal: 2 };
        assert_eq!(int.signal(), 2);
        assert_eq!(int.to_string(), "interrupted by SIGINT");
        assert_eq!(int.code(), 130);
        assert_eq!(
            Interrupted { signal: 15 }.to_string(),
            "interrupted by SIGTERM"
        );
        assert_eq!(
            Interrupted { signal: 9 }.to_string(),
            "interrupted by signal 9"
        );
        if cfg!(unix) {
            assert_eq!(
                Interrupted { signal: 1 }.to_string(),
                "interrupted by SIGHUP"
            );
            assert_eq!(Interrupted { signal: 15 }.code(), 143);
        } else {
            assert_eq!(
                Interrupted { signal: 21 }.to_string(),
                "interrupted by SIGBREAK"
            );
            assert_eq!(Interrupted { signal: 21 }.code(), 130);
        }
    }

    #[test]
    fn exit_code_finds_an_interrupt() {
        let err = anyhow::Error::new(Interrupted { signal: 2 }).context("step");
        assert_eq!(sh::exit_code(&err), Some(130));
    }

    #[test]
    fn main_settings_keep_the_error_code() {
        assert_eq!(Main::new().error_code, 1);
        assert_eq!(Main::default().error_code(2).error_code, 2);
        // 0 would hide a failure.
        assert_eq!(Main::new().error_code(0).error_code, 1);
    }

    /// Makes a command that creates `path`.
    fn touch(path: &std::path::Path) -> CommandBuilder {
        if cfg!(windows) {
            sh::cmd("cmd")
                .arg("/C")
                .arg(format!("echo x> {}", path.display()))
        } else {
            sh::cmd("touch").arg(path)
        }
    }
}
