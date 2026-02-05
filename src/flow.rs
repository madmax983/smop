//! Control flow utilities for robust scripting.
//!
//! This module provides tools for retrying operations, running tasks in parallel,
//! and other control flow patterns.
//!
//! Available with the `nova` feature.

use anyhow::{Result, anyhow};
use std::thread;
use std::time::Duration;

/// Runs multiple operations in parallel and collects their results.
///
/// Uses `std::thread::scope` to allow operations to borrow from the environment.
///
/// # Errors
///
/// Returns an error if any of the operations fail or if a thread panics.
/// If multiple operations fail, the first error is returned.
pub fn parallel<T, F>(ops: Vec<F>) -> Result<Vec<T>>
where
    T: Send,
    F: FnOnce() -> Result<T> + Send,
{
    thread::scope(|s| {
        let handles: Vec<_> = ops.into_iter().map(|op| s.spawn(move || op())).collect();

        let mut results = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(result) => results.push(result?),
                Err(_) => return Err(anyhow!("Thread panicked")),
            }
        }
        Ok(results)
    })
}

/// Retry policy for operations.
#[derive(Debug, Clone, Copy)]
pub enum Policy {
    /// Retry immediately without delay.
    Immediate,
    /// Retry with a fixed delay.
    Fixed(Duration),
    /// Retry with exponential backoff.
    Exponential {
        /// Initial delay.
        initial: Duration,
        /// Maximum delay cap.
        max: Duration,
        /// Multiplier (usually 2.0).
        multiplier: f64,
    },
}

/// Builder for retrying operations.
pub struct Retry<F> {
    op: F,
    policy: Policy,
    max_attempts: u32,
}

impl<F, T> Retry<F>
where
    F: FnMut() -> Result<T>,
{
    /// Creates a new retry builder for the given operation.
    ///
    /// Defaults:
    /// - Policy: Fixed(1 second)
    /// - Max attempts: 3
    pub fn new(op: F) -> Self {
        Self {
            op,
            policy: Policy::Fixed(Duration::from_secs(1)),
            max_attempts: 3,
        }
    }

    /// Sets the retry policy.
    pub fn policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// Sets the maximum number of attempts (including the first one).
    pub fn max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts;
        self
    }

    /// Executes the operation with retry logic.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails after all attempts are exhausted.
    pub fn run(mut self) -> Result<T> {
        let mut attempts = 0;
        loop {
            attempts += 1;
            match (self.op)() {
                Ok(val) => return Ok(val),
                Err(e) => {
                    if attempts >= self.max_attempts {
                        return Err(
                            e.context(format!("Operation failed after {} attempts", attempts))
                        );
                    }

                    let delay = match self.policy {
                        Policy::Immediate => Duration::ZERO,
                        Policy::Fixed(d) => d,
                        Policy::Exponential {
                            initial,
                            max,
                            multiplier,
                        } => {
                            let factor = multiplier.powf((attempts - 1) as f64);
                            let d = initial.mul_f64(factor);
                            d.min(max)
                        }
                    };

                    if !delay.is_zero() {
                        thread::sleep(delay);
                    }
                }
            }
        }
    }
}

/// Helper function to create a new Retry builder.
pub fn retry<F, T>(op: F) -> Retry<F>
where
    F: FnMut() -> Result<T>,
{
    Retry::new(op)
}

/// Executes an operation while showing a spinner.
///
/// This function is only available with the `print` feature.
///
/// # Errors
///
/// Returns the error from the operation if it fails.
///
/// # Examples
///
/// ```no_run
/// use smop::flow;
///
/// # #[cfg(feature = "print")]
/// flow::spin("Processing...", || {
///     // Do work...
///     Ok(())
/// })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[cfg(feature = "print")]
pub fn spin<F, T>(message: &str, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let spinner = crate::print::spinner(message);
    let result = f();
    match &result {
        Ok(_) => spinner.finish(),
        Err(e) => spinner.finish_with_message(&format!("Failed: {e}")),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn retry_succeeds_eventually() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry(move || {
            let val = counter_clone.fetch_add(1, Ordering::SeqCst);
            if val < 2 {
                Err(anyhow!("fail"))
            } else {
                Ok(val)
            }
        })
        .max_attempts(5)
        .policy(Policy::Immediate)
        .run();

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn retry_fails_after_max_attempts() {
        let result = retry(|| Err::<(), _>(anyhow!("fail")))
            .max_attempts(3)
            .policy(Policy::Immediate)
            .run();

        assert!(result.is_err());
    }

    #[test]
    fn parallel_collects_results() {
        let ops = vec![|| Ok(1), || Ok(2), || Ok(3)];

        let mut results = parallel(ops).unwrap();
        results.sort();
        assert_eq!(results, vec![1, 2, 3]);
    }

    #[test]
    fn parallel_handles_errors() {
        let ops = vec![|| Ok(1), || Err::<i32, _>(anyhow!("fail"))];

        let result = parallel(ops);
        assert!(result.is_err());
    }

    #[cfg(feature = "print")]
    #[test]
    fn spin_executes_closure() {
        let result = spin("Test", || Ok(42));
        assert_eq!(result.unwrap(), 42);
    }
}
