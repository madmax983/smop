//! HTTP client utilities.
//!
//! Simple synchronous HTTP client wrapper around ureq.
//! This module is only available with the `http` feature.
//!
//! There are two layers:
//!
//! - **Typed helpers** ([`get`], [`get_json`], [`post_json`], …, and the same
//!   methods on [`Client`]) return the body or an error. Any non-2xx status is
//!   an error whose message includes the status code and the response body.
//! - **The raw layer** ([`request`] / [`Client::request`] → [`Request`] →
//!   [`Response`]) never turns a status code into an error, so code can branch
//!   on 404s, 409s or 422s and read the error body.
//!
//! ```no_run
//! use smop::http;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Issue { number: u64 }
//!
//! #[derive(Deserialize)]
//! struct ApiError { message: String }
//!
//! let client = http::Client::new().header("Authorization", "Bearer TOKEN");
//! let resp = client
//!     .request("POST", "https://api.example.com/issues")
//!     .json(&smop::serde_json::json!({ "title": "Bug" }))
//!     .send()?;
//!
//! match resp.status() {
//!     201 => println!("created #{}", resp.json::<Issue>()?.number),
//!     404 => println!("repository not found"),
//!     422 => println!("rejected: {}", resp.json::<ApiError>()?.message),
//!     _ => {
//!         resp.error_for_status()?;
//!     }
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Serialize, de::DeserializeOwned};

/// Longest slice of a response body quoted in an error message.
const ERROR_BODY_LIMIT: usize = 2000;

// ============================================================================
// Raw layer: Request / Response
// ============================================================================

/// Starts building a request with the given method (e.g. `"GET"`, `"POST"`).
///
/// [`Request::send`] returns the [`Response`] whatever its status code, so the
/// caller decides what a 404 or 422 means. Use [`Client::request`] to share
/// headers and a timeout across requests.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
///
/// let resp = http::request("GET", "https://api.example.com/users/42").send()?;
/// if resp.status() == 404 {
///     println!("no such user");
/// } else {
///     println!("{}", resp.error_for_status()?.text()?);
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn request(method: &str, url: &str) -> Request {
    Request::new(ureq::Agent::new_with_defaults(), &[], method, url)
}

/// A request being built. Create one with [`request`] or [`Client::request`].
#[must_use = "a Request does nothing until .send() is called"]
pub struct Request {
    agent: ureq::Agent,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    default_content_type: Option<&'static str>,
    error: Option<anyhow::Error>,
}

impl Request {
    fn new(agent: ureq::Agent, headers: &[(String, String)], method: &str, url: &str) -> Self {
        Self {
            agent,
            method: method.to_ascii_uppercase(),
            url: url.to_string(),
            headers: headers.to_vec(),
            body: Vec::new(),
            default_content_type: None,
            error: None,
        }
    }

    /// Adds a header to this request.
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    /// Sets a text body. `Content-Type` defaults to `text/plain` unless a
    /// `Content-Type` header was set explicitly.
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into().into_bytes();
        self.default_content_type = Some("text/plain");
        self
    }

    /// Sets a JSON body. `Content-Type` defaults to `application/json` unless
    /// a `Content-Type` header was set explicitly.
    ///
    /// A serialization failure is reported by [`send`](Self::send).
    pub fn json<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        match serde_json::to_vec(value) {
            Ok(body) => {
                self.body = body;
                self.default_content_type = Some("application/json");
            }
            Err(e) => {
                self.error = Some(anyhow::Error::new(e).context(format!(
                    "Failed to serialize request body for: {}",
                    self.url
                )));
            }
        }
        self
    }

    /// Sends the request and reads the whole response.
    ///
    /// Any status code, including 4xx and 5xx, is returned as `Ok`; use
    /// [`Response::status`] or [`Response::error_for_status`] to act on it.
    ///
    /// # Errors
    ///
    /// Returns an error only if the request could not be completed: an
    /// invalid URL or header, DNS or connection failure, timeout, a body that
    /// failed to serialize, or a failure reading the response.
    pub fn send(self) -> Result<Response> {
        let url = self.url.clone();
        let response = self.execute()?;
        Response::read(url, response)
    }

    /// Builds and runs the request with status codes *not* treated as errors.
    fn execute(self) -> Result<ureq::http::Response<ureq::Body>> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let url = self.url;

        let has_content_type = self
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));

        let mut builder = ureq::http::Request::builder()
            .method(self.method.as_str())
            .uri(url.as_str());
        if let Some(content_type) = self.default_content_type
            && !has_content_type
        {
            builder = builder.header("Content-Type", content_type);
        }
        for (key, value) in &self.headers {
            builder = builder.header(key.as_str(), value.as_str());
        }
        let request = builder
            .body(self.body)
            .with_context(|| format!("Invalid HTTP request to {url}"))?;

        let request = self
            .agent
            .configure_request(request)
            .http_status_as_error(false)
            .build();

        self.agent
            .run(request)
            .map_err(|e| handle_ureq_error(e, &url))
    }
}

/// A complete HTTP response: status, headers and body.
///
/// Returned by [`Request::send`] for every status code.
#[derive(Debug, Clone)]
pub struct Response {
    url: String,
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    fn read(url: String, response: ureq::http::Response<ureq::Body>) -> Result<Self> {
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect();
        let body = response
            .into_body()
            .read_to_vec()
            .with_context(|| format!("Failed to read response body from: {url}"))?;
        Ok(Self {
            url,
            status,
            headers,
            body,
        })
    }

    /// The HTTP status code, e.g. `200` or `404`.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// Whether the status code is in the 2xx range.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// The URL the request was sent to.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The first value of the named header (case-insensitive), if present.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// All response headers as `(name, value)` pairs, in the order received.
    /// Names are lowercase.
    pub fn headers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.headers.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The raw response body.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// The response body as UTF-8 text.
    ///
    /// # Errors
    ///
    /// Returns an error if the body is not valid UTF-8.
    pub fn text(&self) -> Result<&str> {
        std::str::from_utf8(&self.body)
            .with_context(|| format!("Response body from {} is not valid UTF-8", self.url))
    }

    /// Consumes the response and returns the body as a `String`.
    ///
    /// # Errors
    ///
    /// Returns an error if the body is not valid UTF-8.
    pub fn into_text(self) -> Result<String> {
        let url = self.url;
        String::from_utf8(self.body)
            .with_context(|| format!("Response body from {url} is not valid UTF-8"))
    }

    /// Deserializes the body as JSON. Works for error bodies too, e.g. a 422
    /// validation payload.
    ///
    /// # Errors
    ///
    /// Returns an error if the body is not valid JSON for `T`.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body).with_context(|| {
            format!(
                "Failed to parse JSON response (status {}) from: {}",
                self.status, self.url
            )
        })
    }

    /// Returns `self` if the status is 2xx, otherwise an error naming the
    /// URL and status and quoting the response body.
    ///
    /// # Errors
    ///
    /// Returns an error for any non-2xx status.
    pub fn error_for_status(self) -> Result<Self> {
        if self.is_success() {
            return Ok(self);
        }
        let body = String::from_utf8_lossy(&self.body);
        let body = body.trim();
        if body.is_empty() {
            bail!(
                "HTTP request to {} failed with status {}",
                self.url,
                self.status
            );
        }
        let snippet = truncate(body, ERROR_BODY_LIMIT);
        bail!(
            "HTTP request to {} failed with status {}: {snippet}",
            self.url,
            self.status
        )
    }
}

/// Truncates to at most `limit` bytes on a char boundary, marking the cut.
fn truncate(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        return s.to_string();
    }
    let mut end = limit;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes total)", &s[..end], s.len())
}

// ============================================================================
// Typed helpers (module-level)
// ============================================================================

/// Performs a GET request and returns the response body as a string.
///
/// # Errors
///
/// Returns an error if the request fails or returns a non-2xx status (the
/// message includes the status and response body).
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
    Client::new().get(url)
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
    Client::new().get_json(url)
}

/// Performs a POST request with a text body (`Content-Type: text/plain`).
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
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn post(url: &str, body: &str) -> Result<String> {
    Client::new().post(url, body)
}

/// Performs a POST request with a JSON body and deserializes the JSON response.
///
/// # Errors
///
/// Returns an error if serialization fails, the request fails, it returns a
/// non-2xx status, or the response is not valid JSON.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize)]
/// struct Request { name: String }
///
/// #[derive(Deserialize)]
/// struct Response { id: u32 }
///
/// let req = Request { name: "test".into() };
/// let resp: Response = http::post_json("https://api.example.com/items", &req)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn post_json<T: Serialize, R: DeserializeOwned>(url: &str, body: &T) -> Result<R> {
    Client::new().post_json(url, body)
}

/// Downloads a URL to a file, streaming the body to disk.
///
/// # Errors
///
/// Returns an error if the request fails, returns a non-2xx status, or the
/// file can't be written.
///
/// # Examples
///
/// ```no_run
/// use smop::http;
///
/// http::download("https://example.com/file.zip", "file.zip")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn download<P: AsRef<Path>>(url: &str, path: P) -> Result<()> {
    Client::new().download(url, path)
}

/// Performs a PUT request with a text body.
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
/// let response = http::put("https://httpbin.org/put", "updated data")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn put(url: &str, body: &str) -> Result<String> {
    Client::new().put(url, body)
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
    Client::new().delete(url)
}

/// Performs a PATCH request with a text body.
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
/// let response = http::patch("https://httpbin.org/patch", "partial update")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn patch(url: &str, body: &str) -> Result<String> {
    Client::new().patch(url, body)
}

/// Performs a PUT request with a JSON body and deserializes the JSON response.
///
/// # Errors
///
/// Returns an error if serialization fails, the request fails, it returns a
/// non-2xx status, or the response is not valid JSON.
pub fn put_json<T: Serialize, R: DeserializeOwned>(url: &str, body: &T) -> Result<R> {
    Client::new().put_json(url, body)
}

/// Performs a DELETE request and deserializes the JSON response.
///
/// # Errors
///
/// Returns an error if the request fails, it returns a non-2xx status, or the
/// response is not valid JSON.
pub fn delete_json<R: DeserializeOwned>(url: &str) -> Result<R> {
    Client::new().delete_json(url)
}

/// Performs a PATCH request with a JSON body and deserializes the JSON
/// response.
///
/// # Errors
///
/// Returns an error if serialization fails, the request fails, it returns a
/// non-2xx status, or the response is not valid JSON.
pub fn patch_json<T: Serialize, R: DeserializeOwned>(url: &str, body: &T) -> Result<R> {
    Client::new().patch_json(url, body)
}

// ============================================================================
// Client
// ============================================================================

/// Configurable HTTP client with builder pattern.
///
/// Headers and the timeout apply to every request made through the client,
/// including raw ones from [`Client::request`].
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

    /// Sets HTTP Basic authentication credentials
    /// (`Authorization: Basic base64(username:password)`).
    pub fn auth(mut self, username: &str, password: &str) -> Self {
        let credentials = base64_encode(format!("{username}:{password}").as_bytes());
        self.headers
            .push(("Authorization".to_string(), format!("Basic {credentials}")));
        self
    }

    /// Adds a custom header to all requests.
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    /// Starts a raw request that returns the [`Response`] whatever its status.
    ///
    /// The client's headers and timeout apply.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::http;
    ///
    /// let gh = http::Client::new().header("Authorization", "Bearer TOKEN");
    /// let resp = gh
    ///     .request("GET", "https://api.github.com/repos/owner/repo/pulls/1")
    ///     .send()?;
    /// let exists = resp.status() != 404;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn request(&self, method: &str, url: &str) -> Request {
        Request::new(self.agent.clone(), &self.headers, method, url)
    }

    fn text(&self, method: &str, url: &str, body: Option<&str>) -> Result<String> {
        let mut request = self.request(method, url);
        if let Some(body) = body {
            request = request.body(body);
        }
        request.send()?.error_for_status()?.into_text()
    }

    fn json<T: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        method: &str,
        url: &str,
        body: Option<&T>,
    ) -> Result<R> {
        let mut request = self.request(method, url);
        if let Some(body) = body {
            request = request.json(body);
        }
        request.send()?.error_for_status()?.json()
    }

    /// Performs a GET request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or returns a non-2xx status.
    pub fn get(&self, url: &str) -> Result<String> {
        self.text("GET", url, None)
    }

    /// Performs a POST request with a text body.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or returns a non-2xx status.
    pub fn post(&self, url: &str, body: &str) -> Result<String> {
        self.text("POST", url, Some(body))
    }

    /// Performs a PUT request with a text body.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or returns a non-2xx status.
    pub fn put(&self, url: &str, body: &str) -> Result<String> {
        self.text("PUT", url, Some(body))
    }

    /// Performs a DELETE request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or returns a non-2xx status.
    pub fn delete(&self, url: &str) -> Result<String> {
        self.text("DELETE", url, None)
    }

    /// Performs a PATCH request with a text body.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or returns a non-2xx status.
    pub fn patch(&self, url: &str, body: &str) -> Result<String> {
        self.text("PATCH", url, Some(body))
    }

    /// Performs a GET request and deserializes JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, returns a non-2xx status, or the
    /// response is not valid JSON.
    pub fn get_json<R: DeserializeOwned>(&self, url: &str) -> Result<R> {
        self.json::<(), R>("GET", url, None)
    }

    /// Performs a POST request with a JSON body and deserializes the JSON
    /// response.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails, the request fails, it returns
    /// a non-2xx status, or the response is not valid JSON.
    pub fn post_json<T: Serialize, R: DeserializeOwned>(&self, url: &str, body: &T) -> Result<R> {
        self.json("POST", url, Some(body))
    }

    /// Performs a PUT request with a JSON body and deserializes the JSON
    /// response.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails, the request fails, it returns
    /// a non-2xx status, or the response is not valid JSON.
    pub fn put_json<T: Serialize, R: DeserializeOwned>(&self, url: &str, body: &T) -> Result<R> {
        self.json("PUT", url, Some(body))
    }

    /// Performs a DELETE request and deserializes the JSON response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, returns a non-2xx status, or the
    /// response is not valid JSON.
    pub fn delete_json<R: DeserializeOwned>(&self, url: &str) -> Result<R> {
        self.json::<(), R>("DELETE", url, None)
    }

    /// Performs a PATCH request with a JSON body and deserializes the JSON
    /// response.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails, the request fails, it returns
    /// a non-2xx status, or the response is not valid JSON.
    pub fn patch_json<T: Serialize, R: DeserializeOwned>(&self, url: &str, body: &T) -> Result<R> {
        self.json("PATCH", url, Some(body))
    }

    /// Downloads a URL to a file, streaming the body to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, returns a non-2xx status (the
    /// message includes the response body), or the file can't be written.
    pub fn download<P: AsRef<Path>>(&self, url: &str, path: P) -> Result<()> {
        let path = path.as_ref();
        let response = self.request("GET", url).execute()?;

        if !response.status().is_success() {
            // Read the (error) body so the message can quote it.
            return Response::read(url.to_string(), response)?
                .error_for_status()
                .map(drop);
        }

        let mut file = std::fs::File::create(path)
            .with_context(|| format!("Failed to create file: {}", path.display()))?;

        std::io::copy(&mut response.into_body().as_reader(), &mut file)
            .with_context(|| format!("Failed to download {url} to {}", path.display()))?;

        Ok(())
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard (RFC 4648) base64 with padding, for Basic auth.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 0x3f) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Converts transport-level ureq errors to anyhow errors naming the URL.
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
    use mockito::Matcher;

    const BAD_HOST: &str = "http://invalid.local.host.that.does.not.exist.12345/";

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Item {
        id: u32,
    }

    #[derive(serde::Serialize)]
    struct NewItem {
        name: &'static str,
    }

    // ---- transport errors ----------------------------------------------

    #[test]
    fn get_returns_error_for_invalid_url() {
        assert!(get(BAD_HOST).is_err());
    }

    #[test]
    fn get_json_returns_error_for_invalid_url() {
        assert!(get_json::<serde_json::Value>(BAD_HOST).is_err());
    }

    #[test]
    fn post_returns_error_for_invalid_url() {
        assert!(post(BAD_HOST, "body").is_err());
    }

    #[test]
    fn post_json_returns_error_for_invalid_url() {
        let result: Result<serde_json::Value> = post_json(BAD_HOST, &NewItem { name: "x" });
        assert!(result.is_err());
    }

    /// A localhost URL with nothing listening, so connecting fails (and,
    /// unlike an unknown hostname, no HTTP proxy can answer for it).
    fn closed_port_url() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        format!("http://127.0.0.1:{port}/")
    }

    #[test]
    fn raw_send_errors_on_transport_failure() {
        let url = closed_port_url();
        let err = request("GET", &url).send().unwrap_err();
        assert!(
            err.to_string().contains(&url),
            "error should name the URL: {err}"
        );
    }

    #[test]
    fn raw_send_errors_on_malformed_url() {
        let err = request("GET", "not a url").send().unwrap_err();
        assert!(err.to_string().contains("not a url"), "{err}");
    }

    // ---- raw layer -------------------------------------------------------

    #[test]
    fn raw_response_exposes_404_status_and_body() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/repos/missing")
            .with_status(404)
            .with_header("x-request-id", "abc123")
            .with_body(r#"{"message":"Not Found"}"#)
            .create();

        let resp = request("GET", &format!("{}/repos/missing", server.url()))
            .send()
            .unwrap();

        assert_eq!(resp.status(), 404);
        assert!(!resp.is_success());
        assert_eq!(resp.header("X-Request-Id"), Some("abc123"));
        assert_eq!(resp.text().unwrap(), r#"{"message":"Not Found"}"#);
        let body: serde_json::Value = resp.json().unwrap();
        assert_eq!(body["message"], "Not Found");
        mock.assert();
    }

    #[test]
    fn raw_response_parses_422_error_body_as_json() {
        #[derive(serde::Deserialize)]
        struct ApiError {
            message: String,
            errors: Vec<String>,
        }

        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/items")
            .match_header("content-type", "application/json")
            .match_body(Matcher::JsonString(r#"{"name":""}"#.into()))
            .with_status(422)
            .with_body(r#"{"message":"Validation Failed","errors":["name is blank"]}"#)
            .create();

        let resp = request("post", &format!("{}/items", server.url()))
            .json(&NewItem { name: "" })
            .send()
            .unwrap();

        assert_eq!(resp.status(), 422);
        let err: ApiError = resp.json().unwrap();
        assert_eq!(err.message, "Validation Failed");
        assert_eq!(err.errors, ["name is blank"]);
        mock.assert();
    }

    #[test]
    fn raw_success_response_and_headers() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/item")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":7}"#)
            .create();

        let resp = request("GET", &format!("{}/item", server.url()))
            .send()
            .unwrap();
        assert!(resp.is_success());
        assert!(resp.url().ends_with("/item"));
        assert!(
            resp.headers()
                .any(|(k, v)| k == "content-type" && v == "application/json")
        );
        assert_eq!(resp.bytes(), br#"{"id":7}"#);
        assert_eq!(resp.json::<Item>().unwrap(), Item { id: 7 });
        mock.assert();
    }

    #[test]
    fn explicit_content_type_overrides_default() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("PUT", "/doc")
            .match_header("content-type", "application/merge-patch+json")
            .match_body("{}")
            .with_status(204)
            .create();

        let resp = request("PUT", &format!("{}/doc", server.url()))
            .header("Content-Type", "application/merge-patch+json")
            .json(&serde_json::json!({}))
            .send()
            .unwrap();
        assert_eq!(resp.status(), 204);
        assert!(resp.bytes().is_empty());
        mock.assert();
    }

    #[test]
    fn json_serialization_error_is_reported_by_send() {
        struct Unserializable;
        impl serde::Serialize for Unserializable {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("nope"))
            }
        }
        let err = request("POST", &closed_port_url())
            .json(&Unserializable)
            .send()
            .unwrap_err();
        assert!(
            format!("{err:#}").contains("Failed to serialize request body"),
            "{err:#}"
        );
    }

    #[test]
    fn error_for_status_passes_success_through() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/").with_body("ok").create();
        let resp = request("GET", &server.url()).send().unwrap();
        assert_eq!(resp.error_for_status().unwrap().into_text().unwrap(), "ok");
    }

    #[test]
    fn error_for_status_without_body_names_status() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/").with_status(500).create();
        let err = request("GET", &server.url())
            .send()
            .unwrap()
            .error_for_status()
            .unwrap_err();
        assert!(err.to_string().ends_with("failed with status 500"), "{err}");
    }

    #[test]
    fn truncate_cuts_long_bodies_on_char_boundary() {
        let long = "é".repeat(10); // 20 bytes
        let cut = truncate(&long, 5);
        assert!(cut.starts_with("éé…"), "{cut}");
        assert!(cut.contains("20 bytes total"), "{cut}");
        assert_eq!(truncate("short", 100), "short");
    }

    // ---- typed helpers now report status + body -----------------------------

    #[test]
    fn get_error_includes_status_and_body() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/gone")
            .with_status(410)
            .with_body("resource was deleted")
            .create();

        let err = get(&format!("{}/gone", server.url())).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("status 410"), "{msg}");
        assert!(msg.contains("resource was deleted"), "{msg}");
    }

    #[test]
    fn get_json_success() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/i").with_body(r#"{"id":1}"#).create();
        let item: Item = get_json(&format!("{}/i", server.url())).unwrap();
        assert_eq!(item, Item { id: 1 });
    }

    #[test]
    fn text_helpers_send_text_plain_bodies() {
        let mut server = mockito::Server::new();
        for method in ["POST", "PUT", "PATCH"] {
            server
                .mock(method, "/t")
                .match_header("content-type", "text/plain")
                .match_body("payload")
                .with_body(method)
                .create();
        }
        let url = format!("{}/t", server.url());
        assert_eq!(post(&url, "payload").unwrap(), "POST");
        assert_eq!(put(&url, "payload").unwrap(), "PUT");
        assert_eq!(patch(&url, "payload").unwrap(), "PATCH");
    }

    #[test]
    fn json_helpers_round_trip() {
        let mut server = mockito::Server::new();
        for method in ["POST", "PUT", "PATCH"] {
            server
                .mock(method, "/j")
                .match_header("content-type", "application/json")
                .match_body(Matcher::JsonString(r#"{"name":"n"}"#.into()))
                .with_body(r#"{"id":3}"#)
                .create();
        }
        server
            .mock("DELETE", "/j")
            .with_body(r#"{"id":4}"#)
            .create();
        let url = format!("{}/j", server.url());
        let body = NewItem { name: "n" };
        assert_eq!(post_json::<_, Item>(&url, &body).unwrap().id, 3);
        assert_eq!(put_json::<_, Item>(&url, &body).unwrap().id, 3);
        assert_eq!(patch_json::<_, Item>(&url, &body).unwrap().id, 3);
        assert_eq!(delete_json::<Item>(&url).unwrap().id, 4);
    }

    #[test]
    fn delete_returns_body() {
        let mut server = mockito::Server::new();
        server.mock("DELETE", "/d").with_body("deleted").create();
        assert_eq!(delete(&format!("{}/d", server.url())).unwrap(), "deleted");
    }

    #[test]
    fn download_writes_file_and_reports_error_body() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/f").with_body("file contents").create();
        server
            .mock("GET", "/missing")
            .with_status(404)
            .with_body("no such file")
            .create();

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("out.txt");
        download(&format!("{}/f", server.url()), &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "file contents");

        let missing = dir.path().join("missing.txt");
        let err = download(&format!("{}/missing", server.url()), &missing).unwrap_err();
        assert!(err.to_string().contains("no such file"), "{err}");
        assert!(!missing.exists(), "no file should be created on error");
    }

    // ---- Client ------------------------------------------------------------

    #[test]
    fn client_headers_apply_to_raw_and_typed_requests() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/h")
            .match_header("x-api-key", "secret")
            .with_body("ok")
            .expect(2)
            .create();

        let client = Client::new().header("X-Api-Key", "secret").timeout(10);
        let url = format!("{}/h", server.url());
        assert_eq!(client.get(&url).unwrap(), "ok");
        assert_eq!(client.request("GET", &url).send().unwrap().status(), 200);
        mock.assert();
    }

    #[test]
    fn client_auth_sends_base64_basic_credentials() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/a")
            .match_header("authorization", "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==")
            .with_body("ok")
            .create();

        let client = Client::new().auth("Aladdin", "open sesame");
        assert_eq!(client.get(&format!("{}/a", server.url())).unwrap(), "ok");
        mock.assert();
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        let cases = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (input, expected) in cases {
            assert_eq!(base64_encode(input.as_bytes()), expected, "input {input:?}");
        }
    }
}
