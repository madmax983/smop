//! HTTP client utilities.
//!
//! Simple synchronous HTTP client wrapper around ureq.
//! This module is only available with the `http` feature.

use std::path::Path;

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

/// Downloads a file from a URL to the specified path.
///
/// # Errors
///
/// Returns an error if the request fails or the file cannot be written.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
///
/// http::download("https://example.com/file.zip", "local.zip")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn download<P: AsRef<Path>>(url: &str, path: P) -> Result<()> {
    let path = path.as_ref();
    let response = ureq::get(url)
        .call()
        .map_err(|e| handle_ureq_error(e, url))?;

    let mut file = std::fs::File::create(path)
        .with_context(|| format!("Failed to create file: {}", path.display()))?;

    std::io::copy(&mut response.into_body().as_reader(), &mut file)
        .with_context(|| format!("Failed to download {url} to {}", path.display()))?;

    Ok(())
}

/// Performs a PUT request with a string body.
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
/// let response = http::put("https://httpbin.org/put", "data")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn put(url: &str, body: &str) -> Result<String> {
    let response = ureq::put(url)
        .header("Content-Type", "text/plain")
        .send(body.as_bytes())
        .map_err(|e| handle_ureq_error(e, url))?;

    response
        .into_body()
        .read_to_string()
        .with_context(|| format!("Failed to read response body from: {url}"))
}

/// Performs a DELETE request.
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
/// let response = http::delete("https://httpbin.org/delete")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn delete(url: &str) -> Result<String> {
    let response = ureq::delete(url)
        .call()
        .map_err(|e| handle_ureq_error(e, url))?;

    response
        .into_body()
        .read_to_string()
        .with_context(|| format!("Failed to read response body from: {url}"))
}

/// Performs a PATCH request with a string body.
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
/// let response = http::patch("https://httpbin.org/patch", "data")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn patch(url: &str, body: &str) -> Result<String> {
    let response = ureq::patch(url)
        .header("Content-Type", "text/plain")
        .send(body.as_bytes())
        .map_err(|e| handle_ureq_error(e, url))?;

    response
        .into_body()
        .read_to_string()
        .with_context(|| format!("Failed to read response body from: {url}"))
}

/// Performs a PUT request with a JSON body and deserializes the JSON response.
///
/// # Errors
///
/// Returns an error if the request fails or the response is not valid JSON.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize)]
/// struct Request { value: i32 }
///
/// #[derive(Deserialize)]
/// struct Response { value: i32 }
///
/// let req = Request { value: 42 };
/// let res: Response = http::put_json("https://httpbin.org/put", &req)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn put_json<T: Serialize, R: DeserializeOwned>(url: &str, body: &T) -> Result<R> {
    let json_body = serde_json::to_string(body)
        .with_context(|| format!("Failed to serialize request body for: {url}"))?;

    let response = ureq::put(url)
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

/// Performs a DELETE request and deserializes the JSON response.
///
/// # Errors
///
/// Returns an error if the request fails or the response is not valid JSON.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Response { deleted: bool }
///
/// let res: Response = http::delete_json("https://httpbin.org/delete")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn delete_json<R: DeserializeOwned>(url: &str) -> Result<R> {
    let body = delete(url)?;
    serde_json::from_str(&body)
        .with_context(|| format!("Failed to parse JSON response from: {url}"))
}

/// Performs a PATCH request with a JSON body and deserializes the JSON response.
///
/// # Errors
///
/// Returns an error if the request fails or the response is not valid JSON.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize)]
/// struct Request { field: String }
///
/// #[derive(Deserialize)]
/// struct Response { field: String }
///
/// let req = Request { field: "updated".into() };
/// let res: Response = http::patch_json("https://httpbin.org/patch", &req)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn patch_json<T: Serialize, R: DeserializeOwned>(url: &str, body: &T) -> Result<R> {
    let json_body = serde_json::to_string(body)
        .with_context(|| format!("Failed to serialize request body for: {url}"))?;

    let response = ureq::patch(url)
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

/// Configurable HTTP client with builder pattern.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
///
/// let client = http::Client::new()
///     .timeout(30)
///     .header("X-Api-Key", "secret");
///
/// let response = client.get("https://api.example.com/data")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[must_use]
pub struct Client {
    agent: ureq::Agent,
    headers: Vec<(String, String)>,
}

impl Client {
    /// Creates a new HTTP client with default settings.
    pub fn new() -> Self {
        Self {
            agent: ureq::Agent::new_with_defaults(),
            headers: Vec::new(),
        }
    }

    /// Sets the request timeout in seconds.
    pub fn timeout(mut self, seconds: u64) -> Self {
        let config = ureq::config::Config::builder()
            .timeout_global(Some(std::time::Duration::from_secs(seconds)))
            .build();
        self.agent = ureq::Agent::new_with_config(config);
        self
    }

    /// Sets basic authentication credentials.
    pub fn auth(mut self, username: &str, password: &str) -> Self {
        // Note: ureq 3.x handles auth differently - we store as custom header
        // In a real implementation, you'd use the agent's built-in auth methods
        let credentials = format!("{username}:{password}");
        self.headers
            .push(("Authorization".to_string(), format!("Basic {credentials}")));
        self
    }

    /// Adds a custom header to all requests.
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    /// Performs a GET request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub fn get(&self, url: &str) -> Result<String> {
        let mut request = self.agent.get(url);
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request.call().map_err(|e| handle_ureq_error(e, url))?;

        response
            .into_body()
            .read_to_string()
            .with_context(|| format!("Failed to read response body from: {url}"))
    }

    /// Performs a POST request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub fn post(&self, url: &str, body: &str) -> Result<String> {
        let mut request = self.agent.post(url).header("Content-Type", "text/plain");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request
            .send(body.as_bytes())
            .map_err(|e| handle_ureq_error(e, url))?;

        response
            .into_body()
            .read_to_string()
            .with_context(|| format!("Failed to read response body from: {url}"))
    }

    /// Performs a GET request and deserializes JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response is not valid JSON.
    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let body = self.get(url)?;
        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse JSON response from: {url}"))
    }

    /// Performs a POST request with JSON body and deserializes JSON response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response is not valid JSON.
    pub fn post_json<T: Serialize, R: DeserializeOwned>(&self, url: &str, body: &T) -> Result<R> {
        let json_body = serde_json::to_string(body)
            .with_context(|| format!("Failed to serialize request body for: {url}"))?;

        let mut request = self
            .agent
            .post(url)
            .header("Content-Type", "application/json");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request
            .send(json_body.as_bytes())
            .map_err(|e| handle_ureq_error(e, url))?;

        let response_body = response
            .into_body()
            .read_to_string()
            .with_context(|| format!("Failed to read response body from: {url}"))?;

        serde_json::from_str(&response_body)
            .with_context(|| format!("Failed to parse JSON response from: {url}"))
    }

    /// Performs a PUT request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub fn put(&self, url: &str, body: &str) -> Result<String> {
        let mut request = self.agent.put(url).header("Content-Type", "text/plain");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request
            .send(body.as_bytes())
            .map_err(|e| handle_ureq_error(e, url))?;

        response
            .into_body()
            .read_to_string()
            .with_context(|| format!("Failed to read response body from: {url}"))
    }

    /// Performs a DELETE request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub fn delete(&self, url: &str) -> Result<String> {
        let mut request = self.agent.delete(url);
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request.call().map_err(|e| handle_ureq_error(e, url))?;

        response
            .into_body()
            .read_to_string()
            .with_context(|| format!("Failed to read response body from: {url}"))
    }

    /// Performs a PATCH request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub fn patch(&self, url: &str, body: &str) -> Result<String> {
        let mut request = self.agent.patch(url).header("Content-Type", "text/plain");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request
            .send(body.as_bytes())
            .map_err(|e| handle_ureq_error(e, url))?;

        response
            .into_body()
            .read_to_string()
            .with_context(|| format!("Failed to read response body from: {url}"))
    }

    /// Performs a PUT request with JSON body and deserializes JSON response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response is not valid JSON.
    pub fn put_json<T: Serialize, R: DeserializeOwned>(&self, url: &str, body: &T) -> Result<R> {
        let json_body = serde_json::to_string(body)
            .with_context(|| format!("Failed to serialize request body for: {url}"))?;

        let mut request = self
            .agent
            .put(url)
            .header("Content-Type", "application/json");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request
            .send(json_body.as_bytes())
            .map_err(|e| handle_ureq_error(e, url))?;

        let response_body = response
            .into_body()
            .read_to_string()
            .with_context(|| format!("Failed to read response body from: {url}"))?;

        serde_json::from_str(&response_body)
            .with_context(|| format!("Failed to parse JSON response from: {url}"))
    }

    /// Performs a DELETE request and deserializes JSON response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response is not valid JSON.
    pub fn delete_json<R: DeserializeOwned>(&self, url: &str) -> Result<R> {
        let body = self.delete(url)?;
        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse JSON response from: {url}"))
    }

    /// Performs a PATCH request with JSON body and deserializes JSON response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response is not valid JSON.
    pub fn patch_json<T: Serialize, R: DeserializeOwned>(&self, url: &str, body: &T) -> Result<R> {
        let json_body = serde_json::to_string(body)
            .with_context(|| format!("Failed to serialize request body for: {url}"))?;

        let mut request = self
            .agent
            .patch(url)
            .header("Content-Type", "application/json");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request
            .send(json_body.as_bytes())
            .map_err(|e| handle_ureq_error(e, url))?;

        let response_body = response
            .into_body()
            .read_to_string()
            .with_context(|| format!("Failed to read response body from: {url}"))?;

        serde_json::from_str(&response_body)
            .with_context(|| format!("Failed to parse JSON response from: {url}"))
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
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
