//! Terminal output utilities.
//!
//! Rich terminal output including colored messages, spinners,
//! progress bars, and interactive prompts.
//!
//! This module is only available with the `print` feature.

use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use console::{Term, style};
use dialoguer::{Confirm, Input};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;

/// Prints a success message with a green checkmark prefix.
///
/// # Examples
///
/// ```no_run
/// use smop::success;
///
/// success!("Operation completed");
/// success!("Processed {} files", 42);
/// ```
#[macro_export]
macro_rules! success {
    ($($arg:tt)*) => {{
        $crate::print::print_success(&format!($($arg)*));
    }};
}

/// Prints a warning message with a yellow warning prefix.
///
/// # Examples
///
/// ```no_run
/// use smop::warn;
///
/// warn!("File already exists");
/// warn!("Skipping {} items", 5);
/// ```
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        $crate::print::print_warn(&format!($($arg)*));
    }};
}

/// Prints an error message with a red X prefix.
///
/// # Examples
///
/// ```no_run
/// use smop::error;
///
/// error!("Failed to connect");
/// error!("Error code: {}", 500);
/// ```
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        $crate::print::print_error(&format!($($arg)*));
    }};
}

/// Internal function for the success! macro.
#[doc(hidden)]
pub fn print_success(message: &str) {
    println!("{}", success_line(message));
}

/// Internal function for the warn! macro.
#[doc(hidden)]
pub fn print_warn(message: &str) {
    println!("{}", warn_line(message));
}

/// Internal function for the error! macro.
#[doc(hidden)]
pub fn print_error(message: &str) {
    eprintln!("{}", error_line(message));
}

fn success_line(message: &str) -> String {
    format!("{} {message}", style("✓").green().bold())
}

fn warn_line(message: &str) -> String {
    format!("{} {message}", style("⚠").yellow().bold())
}

fn error_line(message: &str) -> String {
    format!("{} {message}", style("✗").red().bold())
}

/// A wrapper around an indicatif spinner.
pub struct Spinner {
    bar: ProgressBar,
}

impl Spinner {
    /// Creates a new spinner with the given message.
    fn new(message: &str) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        bar.set_message(message.to_string());
        bar.enable_steady_tick(Duration::from_millis(100));
        Self { bar }
    }

    /// Finishes the spinner with a success message.
    pub fn finish(self) {
        self.bar.finish_and_clear();
    }

    /// Finishes the spinner with a custom message.
    pub fn finish_with_message(self, message: &str) {
        self.bar.finish_with_message(message.to_string());
    }

    /// Updates the spinner message.
    pub fn set_message(&self, message: &str) {
        self.bar.set_message(message.to_string());
    }
}

/// Creates a new spinner with the given message.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let spinner = print::spinner("Loading...");
/// // Do some work...
/// spinner.finish();
/// ```
#[must_use]
pub fn spinner(message: &str) -> Spinner {
    Spinner::new(message)
}

/// A progress bar with a known total. Make one with [`progress`].
///
/// It draws to stderr, and only when stderr is a terminal. A clone shows
/// the same bar, so threads can share it.
#[derive(Clone)]
pub struct Progress {
    bar: ProgressBar,
}

impl Progress {
    fn new(total: u64) -> Self {
        let bar = ProgressBar::new(total);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("█▓▒░"),
        );
        Self { bar }
    }

    /// Moves the bar forward by `delta`. The position stops at `u64::MAX`.
    pub fn inc(&self, delta: u64) {
        // indicatif's `inc` wraps around on overflow.
        self.bar.inc(delta.min(u64::MAX - self.bar.position()));
    }

    /// Sets the position.
    pub fn set_position(&self, position: u64) {
        self.bar.set_position(position);
    }

    /// Returns the position.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.bar.position()
    }

    /// Sets the message after the bar.
    pub fn set_message(&self, message: impl Into<String>) {
        self.bar.set_message(message.into());
    }

    /// Finishes the bar and leaves it on the screen.
    pub fn finish(&self) {
        self.bar.finish();
    }

    /// Finishes the bar with a message and leaves it on the screen.
    pub fn finish_with_message(&self, message: impl Into<String>) {
        self.bar.finish_with_message(message.into());
    }
}

impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Progress")
            .field("position", &self.bar.position())
            .field("length", &self.bar.length().unwrap_or_default())
            .finish()
    }
}

/// Creates a progress bar for `total` steps.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let bar = print::progress(100);
/// for i in 0..100 {
///     bar.set_message(format!("item {i}"));
///     bar.inc(1);
/// }
/// bar.finish_with_message("done");
/// ```
#[must_use]
pub fn progress(total: u64) -> Progress {
    Progress::new(total)
}

/// Prompts the user for text input.
///
/// # Errors
///
/// Returns an error if input cannot be read.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let name = print::prompt("What is your name?")?;
/// println!("Hello, {}!", name);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn prompt(message: &str) -> Result<String> {
    prompt_on(&Term::stderr(), message, None)
}

/// Prompts the user for text input with a default value.
///
/// # Errors
///
/// Returns an error if input cannot be read.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let port = print::prompt_default("Port", "8080")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn prompt_default(message: &str, default: &str) -> Result<String> {
    prompt_on(&Term::stderr(), message, Some(default))
}

/// Shared body of [`prompt`] and [`prompt_default`] on a given terminal.
/// dialoguer's `interact_text()` is `interact_text_on(&Term::stderr())`, so
/// the public functions behave exactly as before; tests pass a non-TTY term.
fn prompt_on(term: &Term, message: &str, default: Option<&str>) -> Result<String> {
    // Flush stdout to ensure the prompt appears
    std::io::stdout().flush().ok();

    let mut input = Input::<String>::new().with_prompt(message);
    if let Some(default) = default {
        input = input.default(default.to_string());
    }
    input
        .interact_text_on(term)
        .map_err(|e| anyhow::anyhow!("Failed to read input: {e}"))
}

/// Prompts the user for a yes/no confirmation.
///
/// # Errors
///
/// Returns an error if input cannot be read.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// if print::confirm("Are you sure?")? {
///     println!("Proceeding...");
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn confirm(message: &str) -> Result<bool> {
    confirm_on(&Term::stderr(), message, None)
}

/// Prompts the user for a yes/no confirmation with a default value.
///
/// # Errors
///
/// Returns an error if input cannot be read.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// // Defaults to "yes" if user just presses Enter
/// if print::confirm_default("Continue?", true)? {
///     println!("Continuing...");
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn confirm_default(message: &str, default: bool) -> Result<bool> {
    confirm_on(&Term::stderr(), message, Some(default))
}

/// Shared body of [`confirm`] and [`confirm_default`] on a given terminal
/// (dialoguer's `interact()` is `interact_on(&Term::stderr())`).
fn confirm_on(term: &Term, message: &str, default: Option<bool>) -> Result<bool> {
    std::io::stdout().flush().ok();

    let mut confirm = Confirm::new().with_prompt(message);
    if let Some(default) = default {
        confirm = confirm.default(default);
    }
    confirm
        .interact_on(term)
        .map_err(|e| anyhow::anyhow!("Failed to read confirmation: {e}"))
}

/// Formats data as a table with headers and rows.
///
/// Returns a formatted string - the caller decides whether to print it.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let headers = &["Name", "Age"];
/// let rows = vec![
///     vec!["Alice".to_string(), "30".to_string()],
///     vec!["Bob".to_string(), "25".to_string()],
/// ];
///
/// println!("{}", print::table(headers, &rows));
/// ```
#[must_use]
#[cfg(feature = "print")]
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    use comfy_table::{Table, presets::UTF8_FULL};

    let mut table = Table::new();
    table.load_style(UTF8_FULL);
    table.set_header(headers);

    for row in rows {
        table.add_row(row);
    }

    table.to_string()
}

/// Pretty-prints a value as JSON to stdout.
///
/// # Errors
///
/// Returns an error if the value cannot be serialized.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data {
///     name: String,
///     count: i32,
/// }
///
/// let data = Data { name: "test".into(), count: 42 };
/// print::print_json(&data)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| anyhow::anyhow!("Failed to serialize value for printing: {e}"))?;
    println!("{json}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_can_be_created_and_finished() {
        let spinner = spinner("Testing...");
        spinner.set_message("Still testing...");
        spinner.finish();
    }

    #[test]
    fn spinner_finish_with_message() {
        let spinner = spinner("Working...");
        spinner.finish_with_message("Done!");
    }

    #[test]
    fn progress_bar_tracks_correctly() {
        let bar: Progress = progress(10);
        for _ in 0..10 {
            bar.inc(1);
        }
        assert_eq!(bar.position(), 10);
        bar.finish();
    }

    #[test]
    fn progress_set_position_moves_the_bar() {
        let bar = progress(100);
        bar.inc(50);
        assert_eq!(bar.position(), 50);
        bar.set_position(20);
        assert_eq!(bar.position(), 20);
        bar.set_position(100);
        assert_eq!(bar.position(), 100);
        bar.finish();
    }

    #[test]
    fn progress_position_stops_at_u64_max() {
        let bar = progress(10);
        bar.set_position(u64::MAX - 1);
        bar.inc(5);
        assert_eq!(bar.position(), u64::MAX);
    }

    #[test]
    fn progress_set_message_takes_str_and_string() {
        let bar = progress(3);
        bar.set_message("first");
        assert_eq!(bar.bar.message(), "first");
        bar.set_message(format!("file {}", 2));
        assert_eq!(bar.bar.message(), "file 2");
        bar.finish();
    }

    #[test]
    fn progress_finish_with_message_sets_the_message() {
        let bar = progress(1);
        bar.inc(1);
        bar.finish_with_message(format!("{} done", 1));
        assert!(bar.bar.is_finished());
        assert_eq!(bar.bar.message(), "1 done");
    }

    #[test]
    fn progress_finish_finishes_the_bar() {
        let bar = progress(2);
        bar.finish();
        assert!(bar.bar.is_finished());
    }

    #[test]
    fn progress_clones_share_one_bar_across_threads() {
        let bar = progress(4);
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let bar = bar.clone();
                std::thread::spawn(move || bar.inc(1))
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(bar.position(), 4);
    }

    #[test]
    fn progress_debug_shows_position_and_length() {
        let bar = progress(8);
        bar.inc(3);
        assert_eq!(format!("{bar:?}"), "Progress { position: 3, length: 8 }");
    }

    #[test]
    fn table_formats_correctly() {
        let headers = &["Name", "Age"];
        let rows = vec![
            vec!["Alice".to_string(), "30".to_string()],
            vec!["Bob".to_string(), "25".to_string()],
        ];

        let output = table(headers, &rows);

        assert!(output.contains("Name"));
        assert!(output.contains("Age"));
        assert!(output.contains("Alice"));
        assert!(output.contains("30"));
        assert!(output.contains("Bob"));
        assert!(output.contains("25"));
    }

    #[test]
    fn table_handles_empty_rows() {
        let headers = &["Col1", "Col2"];
        let rows: Vec<Vec<String>> = vec![];

        let output = table(headers, &rows);

        assert!(output.contains("Col1"));
        assert!(output.contains("Col2"));
    }

    #[test]
    fn print_json_serializes_value() {
        #[derive(serde::Serialize)]
        struct TestData {
            name: String,
            value: i32,
        }

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        // Should not panic
        let result = print_json(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn status_lines_carry_their_symbol_and_message() {
        // style() drops colour codes when output isn't a terminal; strip any
        // that remain so the assertion holds either way.
        let plain = |line: String| console::strip_ansi_codes(&line).into_owned();
        assert_eq!(plain(success_line("built 3 crates")), "✓ built 3 crates");
        assert_eq!(plain(warn_line("skipped 1")), "⚠ skipped 1");
        assert_eq!(plain(error_line("failed")), "✗ failed");
    }

    #[test]
    fn status_macros_format_their_arguments() {
        // The macros only forward to print_*; this checks they expand and
        // accept format arguments.
        crate::success!("done in {}s", 2);
        crate::warn!("{} warnings", 1);
        crate::error!("exit code {}", 3);
        print_success("plain");
        print_warn("plain");
        print_error("plain");
    }

    #[test]
    fn print_json_reports_unserializable_values() {
        // JSON object keys must be strings, so a tuple-keyed map can't be
        // serialized.
        let map = std::collections::BTreeMap::from([((1, 2), "a")]);
        let err = match print_json(&map) {
            Ok(()) => panic!("expected a serialization error"),
            Err(err) => err,
        };
        assert!(
            err.to_string()
                .contains("Failed to serialize value for printing"),
            "{err}"
        );
    }

    /// A terminal backed by a temp file: `is_term()` is false, as in CI or
    /// when stdin and stderr are redirected.
    #[cfg(unix)]
    fn non_tty() -> Result<Term> {
        let file = tempfile::tempfile()?;
        Ok(Term::read_write_pair(file.try_clone()?, file))
    }

    #[cfg(unix)]
    #[test]
    fn prompts_fail_instead_of_waiting_without_a_terminal() -> Result<()> {
        let term = non_tty()?;
        for result in [
            prompt_on(&term, "Name?", None),
            prompt_on(&term, "Port", Some("8080")),
        ] {
            let err = result.err().map(|e| e.to_string()).unwrap_or_default();
            assert!(
                err.contains("Failed to read input") && err.contains("not a terminal"),
                "{err}"
            );
        }
        for result in [
            confirm_on(&term, "Sure?", None),
            confirm_on(&term, "Continue?", Some(true)),
        ] {
            let err = result.err().map(|e| e.to_string()).unwrap_or_default();
            assert!(
                err.contains("Failed to read confirmation") && err.contains("not a terminal"),
                "{err}"
            );
        }
        Ok(())
    }

    #[test]
    fn public_prompts_fail_instead_of_waiting_without_a_terminal() {
        // With a real terminal on stderr (`cargo test` from a shell) these
        // would wait for a person, so only run them when there is none, as
        // in CI.
        if Term::stderr().is_term() {
            return;
        }
        assert!(prompt("Name?").is_err());
        assert!(prompt_default("Port", "8080").is_err());
        assert!(confirm("Sure?").is_err());
        assert!(confirm_default("Continue?", false).is_err());
    }
}
