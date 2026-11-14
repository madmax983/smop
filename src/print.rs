//! Terminal output utilities.
//!
//! Rich terminal output including colored messages, spinners,
//! progress bars, and interactive prompts.
//!
//! This module is only available with the `print` feature.
//!
//! # Without a terminal
//!
//! A prompt asks only when stdin and stderr are both terminals. Else it
//! does not wait: [`confirm_default`] and [`prompt_default`] return the
//! default, and the other prompts return an error that names the prompt.
//!
//! # GitHub Actions
//!
//! [`ci_error`], [`ci_warning`] and [`group`] write workflow commands when
//! `GITHUB_ACTIONS` is `true`, and plain status lines elsewhere.

use std::ffi::OsStr;
use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::time::Duration;

use anyhow::{Result, bail};
use console::{Term, style};
use dialoguer::{Confirm, Input, MultiSelect, Password, Select};
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

/// Prints an information message with a blue `ℹ` prefix to stderr.
///
/// # Examples
///
/// ```no_run
/// use smop::info;
///
/// info!("Using cached index");
/// info!("{} files to check", 12);
/// ```
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        $crate::print::print_info(&format!($($arg)*));
    }};
}

/// Prints a step heading with a cyan `→` prefix to stderr.
///
/// # Examples
///
/// ```no_run
/// use smop::step;
///
/// step!("Build");
/// step!("Test {} crates", 3);
/// ```
#[macro_export]
macro_rules! step {
    ($($arg:tt)*) => {{
        $crate::print::print_step(&format!($($arg)*));
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

/// Internal function for the info! macro.
#[doc(hidden)]
pub fn print_info(message: &str) {
    eprintln!("{}", info_line(message));
}

/// Internal function for the step! macro.
#[doc(hidden)]
pub fn print_step(message: &str) {
    eprintln!("{}", step_line(message));
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

fn info_line(message: &str) -> String {
    format!("{} {message}", style("ℹ").blue().bold())
}

fn step_line(message: &str) -> String {
    format!("{} {}", style("→").cyan().bold(), style(message).bold())
}

/// A GitHub Actions workflow command that [`ci_error`], [`ci_warning`] and
/// [`group`] write.
#[derive(Clone, Copy)]
enum Annotation {
    Error,
    Warning,
    Group,
}

/// True if `GITHUB_ACTIONS` has the value that GitHub sets: `true`.
fn is_github_actions(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("true"))
}

fn on_github_actions() -> bool {
    is_github_actions(std::env::var_os("GITHUB_ACTIONS").as_deref())
}

/// Escapes workflow command data as the runner reads it. Text with a line
/// break then stays one command and cannot start a new one.
fn escape_data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// The line for `annotation`: a workflow command on GitHub Actions, else a
/// status line.
fn annotation_line(annotation: Annotation, message: &str, github: bool) -> String {
    if github {
        let command = match annotation {
            Annotation::Error => "error",
            Annotation::Warning => "warning",
            Annotation::Group => "group",
        };
        format!("::{command}::{}", escape_data(message))
    } else {
        match annotation {
            Annotation::Error => error_line(message),
            Annotation::Warning => warn_line(message),
            Annotation::Group => step_line(message),
        }
    }
}

/// The line that ends a group. Outside GitHub Actions there is none.
fn end_group_line(github: bool) -> Option<String> {
    github.then(|| "::endgroup::".to_string())
}

/// Prints an error annotation to stderr.
///
/// On GitHub Actions (`GITHUB_ACTIONS=true`) it is `::error::message`, and
/// GitHub shows it on the run and the pull request. Elsewhere it is the
/// same line as [`error!`](crate::error). Line breaks stay in the message.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// print::ci_error("Cargo.lock is out of date");
/// print::ci_error(format_args!("{} adds a crate", "sh"));
/// ```
pub fn ci_error(message: impl Display) {
    eprintln!(
        "{}",
        annotation_line(Annotation::Error, &message.to_string(), on_github_actions())
    );
}

/// Prints a warning annotation to stderr.
///
/// On GitHub Actions (`GITHUB_ACTIONS=true`) it is `::warning::message`.
/// Elsewhere it is the same line as [`warn!`](crate::warn), but on stderr.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// print::ci_warning("Skipped the slow tests");
/// ```
pub fn ci_warning(message: impl Display) {
    eprintln!(
        "{}",
        annotation_line(
            Annotation::Warning,
            &message.to_string(),
            on_github_actions()
        )
    );
}

/// Prints `::endgroup::` when it drops, also during a panic.
struct GroupEnd {
    github: bool,
}

impl Drop for GroupEnd {
    fn drop(&mut self) {
        if let Some(line) = end_group_line(self.github) {
            eprintln!("{line}");
        }
    }
}

/// Runs `body` in a log group and returns its value.
///
/// On GitHub Actions (`GITHUB_ACTIONS=true`) it prints `::group::name`
/// before `body` and `::endgroup::` after it, also after a panic. GitHub
/// shows the output between them as one folded section. GitHub does not
/// nest groups. Elsewhere it prints the same line as [`step!`](crate::step)
/// before `body`. Both go to stderr.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let count = print::group("Count files", || std::fs::read_dir(".").map(|d| d.count()))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn group<T>(name: impl Display, body: impl FnOnce() -> T) -> T {
    let github = on_github_actions();
    eprintln!(
        "{}",
        annotation_line(Annotation::Group, &name.to_string(), github)
    );
    let _end = GroupEnd { github };
    body()
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

/// Where a prompt reads keys and draws. A person can answer only when
/// stdin and the drawing terminal are both terminals: with stdin
/// redirected, console reads keys from `/dev/tty` and waits.
struct Tty {
    term: Term,
    stdin: bool,
}

impl Tty {
    /// The prompts draw on stderr, as dialoguer does.
    fn stderr() -> Self {
        Self {
            term: Term::stderr(),
            stdin: std::io::stdin().is_terminal(),
        }
    }

    fn attended(&self) -> bool {
        self.stdin && self.term.is_term()
    }

    /// The error for a prompt that cannot ask: it names the prompt and the
    /// stream that is not a terminal.
    fn missing(&self, message: &str) -> anyhow::Error {
        let stream = if self.stdin { "stderr" } else { "stdin" };
        anyhow::anyhow!(
            "no terminal for the prompt {message:?}: {stream} is not a terminal; \
             use a function with a default or give the value another way"
        )
    }

    /// `Ok` if a person can answer, else the error of [`Tty::missing`].
    fn require(&self, message: &str) -> Result<()> {
        if self.attended() {
            Ok(())
        } else {
            Err(self.missing(message))
        }
    }
}

/// Prompts the user for text input.
///
/// # Errors
///
/// Returns an error if stdin or stderr is not a terminal, without a wait,
/// or if input cannot be read.
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
    prompt_on(&Tty::stderr(), message, None)
}

/// Prompts the user for text input with a default value.
///
/// Without a terminal on stdin and stderr, it returns `default` and does
/// not wait.
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
    prompt_on(&Tty::stderr(), message, Some(default))
}

/// Shared body of [`prompt`] and [`prompt_default`]. Tests give a
/// [`Tty`] that is not a terminal.
fn prompt_on(tty: &Tty, message: &str, default: Option<&str>) -> Result<String> {
    if !tty.attended() {
        return default
            .map(str::to_string)
            .ok_or_else(|| tty.missing(message));
    }
    std::io::stdout().flush().ok();

    let mut input = Input::<String>::new().with_prompt(message);
    if let Some(default) = default {
        input = input.default(default.to_string());
    }
    input
        .interact_text_on(&tty.term)
        .map_err(|e| anyhow::anyhow!("Failed to read input: {e}"))
}

/// Prompts the user for a yes/no confirmation.
///
/// # Errors
///
/// Returns an error if stdin or stderr is not a terminal, without a wait,
/// or if input cannot be read.
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
    confirm_on(&Tty::stderr(), message, None)
}

/// Prompts the user for a yes/no confirmation with a default value.
///
/// Without a terminal on stdin and stderr, it returns `default` and does
/// not wait.
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
    confirm_on(&Tty::stderr(), message, Some(default))
}

/// Shared body of [`confirm`] and [`confirm_default`].
fn confirm_on(tty: &Tty, message: &str, default: Option<bool>) -> Result<bool> {
    if !tty.attended() {
        return default.ok_or_else(|| tty.missing(message));
    }
    std::io::stdout().flush().ok();

    let mut confirm = Confirm::new().with_prompt(message);
    if let Some(default) = default {
        confirm = confirm.default(default);
    }
    confirm
        .interact_on(&tty.term)
        .map_err(|e| anyhow::anyhow!("Failed to read confirmation: {e}"))
}

/// Asks the user to pick one item with the arrow keys. Returns its index.
///
/// # Errors
///
/// Returns an error if `items` is empty, if stdin or stderr is not a
/// terminal (without a wait), or if input cannot be read.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let profiles = ["debug", "release"];
/// let i = print::select("Profile", &profiles)?;
/// println!("Building {}", profiles[i]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn select<T: Display>(message: &str, items: &[T]) -> Result<usize> {
    select_on(&Tty::stderr(), message, &labels(items))
}

/// Shared body of [`select`].
fn select_on(tty: &Tty, message: &str, items: &[String]) -> Result<usize> {
    require_items(message, items)?;
    tty.require(message)?;
    std::io::stdout().flush().ok();
    Select::new()
        .with_prompt(message)
        .items(items)
        .default(0)
        .interact_on(&tty.term)
        .map_err(|e| anyhow::anyhow!("Failed to read selection: {e}"))
}

/// Asks the user to pick zero or more items. Space toggles an item, Enter
/// accepts. Returns the indexes of the picked items, in order.
///
/// # Errors
///
/// Returns an error if `items` is empty, if stdin or stderr is not a
/// terminal (without a wait), or if input cannot be read.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let targets = ["linux", "macos", "windows"];
/// for i in print::multiselect("Targets", &targets)? {
///     println!("Building for {}", targets[i]);
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn multiselect<T: Display>(message: &str, items: &[T]) -> Result<Vec<usize>> {
    multiselect_on(&Tty::stderr(), message, &labels(items))
}

/// Shared body of [`multiselect`].
fn multiselect_on(tty: &Tty, message: &str, items: &[String]) -> Result<Vec<usize>> {
    require_items(message, items)?;
    tty.require(message)?;
    std::io::stdout().flush().ok();
    MultiSelect::new()
        .with_prompt(message)
        .items(items)
        .interact_on(&tty.term)
        .map_err(|e| anyhow::anyhow!("Failed to read selection: {e}"))
}

/// Asks the user for a secret. The terminal does not show the input. An
/// empty input is not accepted; the prompt asks again.
///
/// # Errors
///
/// Returns an error if stdin or stderr is not a terminal (without a wait),
/// or if input cannot be read.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let token = print::password("API token")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn password(message: &str) -> Result<String> {
    password_on(&Tty::stderr(), message)
}

/// Shared body of [`password`].
fn password_on(tty: &Tty, message: &str) -> Result<String> {
    tty.require(message)?;
    std::io::stdout().flush().ok();
    Password::new()
        .with_prompt(message)
        .interact_on(&tty.term)
        .map_err(|e| anyhow::anyhow!("Failed to read password: {e}"))
}

fn labels<T: Display>(items: &[T]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

fn require_items(message: &str, items: &[String]) -> Result<()> {
    if items.is_empty() {
        bail!("no items for the prompt {message:?}");
    }
    Ok(())
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

    #[test]
    fn info_and_step_lines_carry_their_symbol_and_message() {
        let plain = |line: String| console::strip_ansi_codes(&line).into_owned();
        assert_eq!(plain(info_line("3 crates")), "ℹ 3 crates");
        assert_eq!(plain(step_line("Build")), "→ Build");
    }

    #[test]
    fn info_and_step_macros_format_their_arguments() {
        crate::info!("{} files", 2);
        crate::step!("step {}", 1);
        print_info("plain");
        print_step("plain");
    }

    #[test]
    fn workflow_data_escapes_percent_cr_and_lf() {
        assert_eq!(escape_data("50%\r\nnext"), "50%25%0D%0Anext");
        assert_eq!(escape_data("%0A"), "%250A");
        assert_eq!(escape_data("plain: a, b"), "plain: a, b");
    }

    #[test]
    fn only_the_exact_value_true_means_github_actions() {
        use std::ffi::OsStr;
        assert!(is_github_actions(Some(OsStr::new("true"))));
        for value in ["1", "TRUE", "yes", "", " true"] {
            assert!(!is_github_actions(Some(OsStr::new(value))), "{value:?}");
        }
        assert!(!is_github_actions(None));
    }

    #[test]
    fn annotations_use_workflow_commands_on_github_actions() {
        assert_eq!(
            annotation_line(Annotation::Error, "bad\nlock", true),
            "::error::bad%0Alock"
        );
        assert_eq!(
            annotation_line(Annotation::Warning, "50% done", true),
            "::warning::50%25 done"
        );
        assert_eq!(
            annotation_line(Annotation::Group, "cargo test", true),
            "::group::cargo test"
        );
        assert_eq!(end_group_line(true).as_deref(), Some("::endgroup::"));
    }

    #[test]
    fn annotations_are_plain_lines_outside_github_actions() {
        let plain = |line: String| console::strip_ansi_codes(&line).into_owned();
        assert_eq!(
            plain(annotation_line(Annotation::Error, "bad", false)),
            "✗ bad"
        );
        assert_eq!(
            plain(annotation_line(Annotation::Warning, "old", false)),
            "⚠ old"
        );
        assert_eq!(
            plain(annotation_line(Annotation::Group, "test", false)),
            "→ test"
        );
        assert_eq!(end_group_line(false), None);
        // A plain line keeps the text as it is.
        assert!(plain(annotation_line(Annotation::Error, "50%\nx", false)).ends_with("50%\nx"));
    }

    /// A terminal backed by a temp file: `is_term()` is false, as in CI or
    /// when stdin and stderr are redirected.
    #[cfg(unix)]
    fn non_tty() -> Result<Term> {
        let file = tempfile::tempfile()?;
        Ok(Term::read_write_pair(file.try_clone()?, file))
    }

    /// No terminal on stdin and none to draw on.
    #[cfg(unix)]
    fn unattended() -> Result<Tty> {
        Ok(Tty {
            term: non_tty()?,
            stdin: false,
        })
    }

    #[cfg(unix)]
    fn err_text<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Ok(value) => panic!("expected an error, got {value:?}"),
            Err(err) => err.to_string(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn prompts_with_a_default_return_it_without_a_terminal() -> Result<()> {
        let tty = unattended()?;
        assert!(confirm_on(&tty, "Continue?", Some(true))?);
        assert!(!confirm_on(&tty, "Continue?", Some(false))?);
        assert_eq!(prompt_on(&tty, "Port", Some("8080"))?, "8080");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn prompts_without_a_default_fail_and_name_the_missing_terminal() -> Result<()> {
        let tty = unattended()?;
        let items = ["a".to_string(), "b".to_string()];
        for err in [
            err_text(confirm_on(&tty, "Sure?", None)),
            err_text(prompt_on(&tty, "Name?", None)),
            err_text(select_on(&tty, "Pick", &items)),
            err_text(multiselect_on(&tty, "Pick", &items)),
            err_text(password_on(&tty, "Token")),
        ] {
            assert!(
                err.contains("no terminal") && err.contains("stdin is not a terminal"),
                "{err}"
            );
        }
        let err = err_text(confirm_on(&tty, "Sure?", None));
        assert!(err.contains(r#""Sure?""#), "{err}");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_terminal_on_stdin_alone_is_not_enough() -> Result<()> {
        let tty = Tty {
            term: non_tty()?,
            stdin: true,
        };
        let err = err_text(confirm_on(&tty, "Sure?", None));
        assert!(err.contains("stderr is not a terminal"), "{err}");
        assert!(confirm_on(&tty, "Sure?", Some(true))?);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn select_and_multiselect_need_items() -> Result<()> {
        let tty = unattended()?;
        for err in [
            err_text(select_on(&tty, "Pick", &[])),
            err_text(multiselect_on(&tty, "Pick", &[])),
        ] {
            assert!(
                err.contains("no items") && err.contains(r#""Pick""#),
                "{err}"
            );
        }
        Ok(())
    }

    #[test]
    fn public_prompts_do_not_wait_without_a_terminal() {
        // With a person at the terminal (`cargo test` from a shell) these
        // would wait, so only run them when there is none, as in CI.
        if Tty::stderr().attended() {
            return;
        }
        assert!(prompt("Name?").is_err());
        assert_eq!(prompt_default("Port", "8080").ok().as_deref(), Some("8080"));
        assert!(confirm("Sure?").is_err());
        assert_eq!(confirm_default("Continue?", false).ok(), Some(false));
        assert!(select("Pick", &["a", "b"]).is_err());
        assert!(multiselect("Pick", &["a", "b"]).is_err());
        assert!(password("Token").is_err());
    }
}
