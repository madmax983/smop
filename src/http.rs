//! HTTP client utilities.
//!
//! Simple synchronous HTTP client wrapper around ureq.
//! This module is only available with the `http` feature.

use anyhow::{Context, Result, anyhow};
use serde::{Serialize, de::DeserializeOwned};

/// Performs a GET request and returns the response body as a string.
///
/// # Errors
///
/// Returns an error if the request fails or returns a non-2xx status.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
///
/// let body = http::get("https://httpbin.org/get")?;
/// println!("{}", body);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn get(url: &str) -> Result<String> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| handle_ureq_error(e, url))?;

    response
        .into_body()
        .read_to_string()
        .with_context(|| format!("Failed to read response body from: {url}"))
}

/// Performs a GET request and deserializes the JSON response.
///
/// # Errors
///
/// Returns an error if the request fails, returns a non-2xx status,
/// or if the response body is not valid JSON.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Response {
///     origin: String,
/// }
///
/// let response: Response = http::get_json("https://httpbin.org/get")?;
/// println!("Origin: {}", response.origin);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn get_json<T: DeserializeOwned>(url: &str) -> Result<T> {
    let body = get(url)?;
    serde_json::from_str(&body)
        .with_context(|| format!("Failed to parse JSON response from: {url}"))
}

/// Performs a POST request with a string body.
///
/// # Errors
///
/// Returns an error if the request fails or returns a non-2xx status.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
///
/// let response = http::post("https://httpbin.org/post", "Hello, world!")?;
/// println!("{}", response);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn post(url: &str, body: &str) -> Result<String> {
    let response = ureq::post(url)
        .header("Content-Type", "text/plain")
        .send(body.as_bytes())
        .map_err(|e| handle_ureq_error(e, url))?;

    response
        .into_body()
        .read_to_string()
        .with_context(|| format!("Failed to read response body from: {url}"))
}

/// Performs a POST request with a JSON body and deserializes the JSON response.
///
/// # Errors
///
/// Returns an error if the request fails, returns a non-2xx status,
/// or if the response body is not valid JSON.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct Request {
///     name: String,
/// }
///
/// #[derive(Deserialize)]
/// struct Response {
///     json: Request,
/// }
///
/// let request = Request { name: "test".into() };
/// let response: Response = http::post_json("https://httpbin.org/post", &request)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn post_json<T: Serialize, R: DeserializeOwned>(url: &str, body: &T) -> Result<R> {
    let json_body = serde_json::to_string(body)
        .with_context(|| format!("Failed to serialize request body for: {url}"))?;

    let response = ureq::post(url)
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
        .map_err(|e| handle_ureq_error(e, url))?;

    let response_body = response
        .into_body()
        .read_to_string()
        .with_context(|| format!("Failed to read response body from: {url}"))?;

    serde_json::from_str(&response_body)
        .with_context(|| format!("Failed to parse JSON response from: {url}"))
}

/// Handles ureq errors and converts them to anyhow errors.
fn handle_ureq_error(error: ureq::Error, url: &str) -> anyhow::Error {
    match error {
        ureq::Error::StatusCode(code) => {
            anyhow!("HTTP request to {url} failed with status {code}")
        }
        ureq::Error::Timeout(kind) => {
            anyhow!("HTTP request to {url} timed out: {kind:?}")
        }
        ureq::Error::HostNotFound => {
            anyhow!("Host not found for URL: {url}")
        }
        ureq::Error::Io(e) => {
            anyhow!("IO error during HTTP request to {url}: {e}")
        }
        _ => {
            anyhow!("HTTP request to {url} failed: {error}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require mockito for proper testing.
    // In a real test environment, you'd use:
    // let server = mockito::Server::new();
    // let url = server.url();
    // let _mock = server.mock("GET", "/").with_body("test").create();

    #[test]
    fn get_returns_error_for_invalid_url() {
        let result = get("http://invalid.local.host.that.does.not.exist.12345/");
        assert!(result.is_err());
    }

    #[test]
    fn get_json_returns_error_for_invalid_url() {
        let result: Result<serde_json::Value> =
            get_json("http://invalid.local.host.that.does.not.exist.12345/");
        assert!(result.is_err());
    }

    #[test]
    fn post_returns_error_for_invalid_url() {
        let result = post(
            "http://invalid.local.host.that.does.not.exist.12345/",
            "body",
        );
        assert!(result.is_err());
    }

    #[test]
    fn post_json_returns_error_for_invalid_url() {
        #[derive(serde::Serialize)]
        struct TestBody {
            key: String,
        }
        let body = TestBody {
            key: "value".to_string(),
        };
        let result: Result<serde_json::Value> = post_json(
            "http://invalid.local.host.that.does.not.exist.12345/",
            &body,
        );
        assert!(result.is_err());
    }

    // Integration tests with mockito would look like:
    // #[test]
    // fn get_fetches_url() {
    //     let mut server = mockito::Server::new();
    //     let mock = server.mock("GET", "/")
    //         .with_status(200)
    //         .with_body("Hello, World!")
    //         .create();
    //
    //     let result = get(&server.url()).unwrap();
    //     assert_eq!(result, "Hello, World!");
    //     mock.assert();
    // }
}
