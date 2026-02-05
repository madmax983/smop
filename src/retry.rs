//! Retry logic for robust scripts.
//!
//! Provides configurable retry mechanisms for operations that might fail.

use anyhow::Result;
use std::thread;
use std::time::Duration;

/// Configuration for retrying operations.
pub struct Retry {
    attempts: u32,
    delay: Duration,
    factor: f64,
    max_delay: Option<Duration>,
}

impl Retry {
    /// Creates a new retry configuration with defaults.
    ///
    /// Defaults:
    /// - 3 attempts
    /// - 1 second delay
    /// - No backoff (factor 1.0)
    #[must_use]
    pub const fn new() -> Self {
        Self {
            attempts: 3,
            delay: Duration::from_secs(1),
            factor: 1.0,
            max_delay: None,
        }
    }

    /// Sets the maximum number of attempts.
    #[must_use]
    pub const fn attempts(mut self, attempts: u32) -> Self {
        self.attempts = attempts;
        self
    }

    /// Sets the initial delay between attempts.
    #[must_use]
    pub const fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Sets the exponential backoff factor.
    ///
    /// - `1.0`: Constant delay (default)
    /// - `2.0`: Double the delay each time
    #[must_use]
    pub const fn factor(mut self, factor: f64) -> Self {
        self.factor = factor;
        self
    }

    /// Sets a maximum delay cap.
    #[must_use]
    pub const fn max_delay(mut self, max: Duration) -> Self {
        self.max_delay = Some(max);
        self
    }

    /// Executes the operation with retry logic.
    ///
    /// # Errors
    ///
    /// Returns the last error if all attempts fail.
    pub fn run<F, T>(self, mut op: F) -> Result<T>
    where
        F: FnMut() -> Result<T>,
    {
        let mut attempts = 0;
        let mut current_delay = self.delay;

        loop {
            attempts += 1;
            match op() {
                Ok(val) => return Ok(val),
                Err(e) => {
                    if attempts >= self.attempts {
                        return Err(e);
                    }

                    thread::sleep(current_delay);

                    current_delay = current_delay.mul_f64(self.factor);
                    if let Some(max) = self.max_delay {
                        current_delay = current_delay.min(max);
                    }
                }
            }
        }
    }
}

impl Default for Retry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to retry an operation with default settings.
///
/// Retries 3 times with 1 second delay.
///
/// # Errors
///
/// Returns the last error if all attempts fail.
pub fn retry<F, T>(op: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    Retry::new().run(op)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::sync::{Arc, Mutex};

    #[test]
    fn retry_succeeds_immediately() {
        let result = Retry::new().run(|| Ok::<_, anyhow::Error>("success"));
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn retry_retries_and_succeeds() {
        let attempts = Arc::new(Mutex::new(0));
        let attempts_clone = attempts.clone();

        let result = Retry::new()
            .attempts(3)
            .delay(Duration::from_millis(1))
            .run(move || {
                let mut count = attempts_clone.lock().unwrap();
                *count += 1;
                if *count < 2 {
                    Err(anyhow!("fail"))
                } else {
                    Ok("success")
                }
            });

        assert_eq!(result.unwrap(), "success");
        assert_eq!(*attempts.lock().unwrap(), 2);
    }

    #[test]
    fn retry_fails_after_max_attempts() {
        let attempts = Arc::new(Mutex::new(0));
        let attempts_clone = attempts.clone();

        let result = Retry::new()
            .attempts(3)
            .delay(Duration::from_millis(1))
            .run(move || {
                {
                    let mut count = attempts_clone.lock().unwrap();
                    *count += 1;
                }
                Err::<(), _>(anyhow!("fail"))
            });

        assert!(result.is_err());
        assert_eq!(*attempts.lock().unwrap(), 3);
    }
}
