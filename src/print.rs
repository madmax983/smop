//! Terminal output utilities.
//!
//! Rich terminal output including colored messages, spinners,
//! progress bars, and interactive prompts.
//!
//! This module is only available with the `print` feature.

use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use console::style;
use dialoguer::{Confirm, Input};
use indicatif::{ProgressBar, ProgressStyle};

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
    println!("{} {}", style("✓").green().bold(), message);
}

/// Internal function for the warn! macro.
#[doc(hidden)]
pub fn print_warn(message: &str) {
    println!("{} {}", style("⚠").yellow().bold(), message);
}

/// Internal function for the error! macro.
#[doc(hidden)]
pub fn print_error(message: &str) {
    eprintln!("{} {}", style("✗").red().bold(), message);
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

/// Creates a new progress bar with the given total.
///
/// # Examples
///
/// ```no_run
/// use smop::print;
///
/// let bar = print::progress(100);
/// for i in 0..100 {
///     // Do work...
///     bar.inc(1);
/// }
/// bar.finish();
/// ```
#[must_use]
pub fn progress(total: u64) -> ProgressBar {
    let bar = ProgressBar::new(total);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("█▓▒░"),
    );
    bar
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
    // Flush stdout to ensure the prompt appears
    std::io::stdout().flush().ok();

    Input::new()
        .with_prompt(message)
        .interact_text()
        .map_err(|e| anyhow::anyhow!("Failed to read input: {e}"))
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
    std::io::stdout().flush().ok();

    Input::new()
        .with_prompt(message)
        .default(default.to_string())
        .interact_text()
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
    std::io::stdout().flush().ok();

    Confirm::new()
        .with_prompt(message)
        .interact()
        .map_err(|e| anyhow::anyhow!("Failed to read confirmation: {e}"))
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
    std::io::stdout().flush().ok();

    Confirm::new()
        .with_prompt(message)
        .default(default)
        .interact()
        .map_err(|e| anyhow::anyhow!("Failed to read confirmation: {e}"))
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
        let bar = progress(10);
        for _ in 0..10 {
            bar.inc(1);
        }
        assert_eq!(bar.position(), 10);
        bar.finish();
    }

    #[test]
    fn progress_bar_can_be_created() {
        let bar = progress(100);
        bar.inc(50);
        assert_eq!(bar.position(), 50);
        bar.finish();
    }

    // Note: prompt functions require interactive input and cannot be unit tested easily.
    // They should be tested manually or with integration tests.
}
