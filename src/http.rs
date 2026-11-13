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
//! [`Retry`] sets retries, as `curl --retry` does. They are off by default.
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
//! let client = http::Client::new().bearer("TOKEN").retry(http::Retry::new(3));
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

mod retry;

use std::path::Path;
use std::time::{Duration, SystemTime};

pub use retry::Retry;
use retry::{retry_after, retryable_status};

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
    Request::new(
        ureq::Agent::new_with_defaults(),
        &[],
        &[],
        Retry::new(0),
        method,
        url,
    )
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
    retry: Retry,
    error: Option<anyhow::Error>,
}

impl Request {
    fn new(
        agent: ureq::Agent,
        headers: &[(String, String)],
        query: &[(String, String)],
        retry: Retry,
        method: &str,
        url: &str,
    ) -> Self {
        let url = query
            .iter()
            .fold(url.to_string(), |url, (k, v)| with_query(&url, k, v));
        Self {
            agent,
            method: method.to_ascii_uppercase(),
            url,
            headers: headers.to_vec(),
            body: Vec::new(),
            default_content_type: None,
            retry,
            error: None,
        }
    }

    /// Adds a header to this request. An `Authorization` header replaces
    /// an earlier one, also one from the [`Client`].
    pub fn header(mut self, key: &str, value: &str) -> Self {
        add_header(&mut self.headers, key, value);
        self
    }

    /// Adds `key=value` to the query string of the URL. The key and the
    /// value are percent-encoded: only `A-Z a-z 0-9 - . _ ~` stay as they
    /// are. A space becomes `%20`.
    pub fn query(mut self, key: &str, value: &str) -> Self {
        self.url = with_query(&self.url, key, value);
        self
    }

    /// Sets `Authorization: Bearer <token>`. It replaces an earlier
    /// `Authorization` header. This includes a header from the [`Client`].
    pub fn bearer(mut self, token: &str) -> Self {
        set_authorization(&mut self.headers, &format!("Bearer {token}"));
        self
    }

    /// Sets the retry policy. It replaces the policy of the [`Client`].
    pub const fn retry(mut self, retry: Retry) -> Self {
        self.retry = retry;
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
    /// With a [`Retry`], the result is that of the last attempt.
    ///
    /// # Errors
    ///
    /// Returns an error only if the request could not be completed: an
    /// invalid URL or header, DNS or connection failure, timeout, a body that
    /// failed to serialize, or a failure reading the response.
    pub fn send(mut self) -> Result<Response> {
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        self.retrying(
            |req| Response::read(req.url.clone(), req.execute()?),
            |response| Some(response),
        )
    }

    /// Calls `attempt` until its result is final, as the [`Retry`] tells.
    /// `response` gives the response in a result, if there is one.
    fn retrying<T>(
        &self,
        mut attempt: impl FnMut(&Self) -> Result<T>,
        response: fn(&T) -> Option<&Response>,
    ) -> Result<T> {
        let retries = self.retry.retries_for(&self.method);
        let mut n = 0;
        loop {
            let result = attempt(self);
            if n == retries {
                return result;
            }
            let (reason, after) = match &result {
                Ok(value) => match response(value) {
                    Some(resp) if retryable_status(resp.status) => (
                        format!("status {}", resp.status),
                        resp.header("retry-after")
                            .and_then(|v| retry_after(v, SystemTime::now())),
                    ),
                    _ => return result,
                },
                Err(err) if is_timeout(err) => ("timed out".to_string(), None),
                Err(_) => return result,
            };
            n += 1;
            let wait = self.retry.wait(n, after);
            eprintln!(
                "{}",
                retry_note(&self.method, &self.url, &reason, n, retries, wait)
            );
            if let Some(interrupt) = pause(wait) {
                return Err(interrupt.context(format!(
                    "{} {}: retry stopped after attempt {n}: {reason}",
                    self.method,
                    redact(&self.url)
                )));
            }
        }
    }

    /// Builds and runs the request with status codes *not* treated as errors.
    fn execute(&self) -> Result<ureq::http::Response<ureq::Body>> {
        let url = &self.url;

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
            .body(self.body.clone())
            .with_context(|| format!("Invalid HTTP request to {url}"))?;

        let request = self
            .agent
            .configure_request(request)
            .http_status_as_error(false)
            .build();

        self.agent
            .run(request)
            .map_err(|e| handle_ureq_error(e, url))
    }

    /// One download attempt. Gives the response of a non-2xx status, or
    /// `None` when the file is written.
    fn download_once(&self, path: &Path) -> Result<Option<Response>> {
        let url = &self.url;
        let response = self.execute()?;

        if !response.status().is_success() {
            // Read the (error) body so the message can quote it.
            return Response::read(url.clone(), response).map(Some);
        }

        let mut file = std::fs::File::create(path)
            .with_context(|| format!("Failed to create file: {}", path.display()))?;

        let Err(error) = std::io::copy(&mut response.into_body().as_reader(), &mut file) else {
            return Ok(None);
        };
        // The file holds only part of the body, so remove it.
        drop(file);
        let _ = std::fs::remove_file(path);
        // ureq puts its own error, for example a timeout, in the io::Error.
        if let Some(&ureq::Error::Timeout(kind)) = error
            .get_ref()
            .and_then(|e| e.downcast_ref::<ureq::Error>())
        {
            return Err(handle_ureq_error(ureq::Error::Timeout(kind), url));
        }
        if error.kind() == std::io::ErrorKind::TimedOut {
            return Err(handle_ureq_error(ureq::Error::Io(error), url));
        }
        Err(anyhow::Error::new(error)
            .context(format!("Failed to download {url} to {}", path.display())))
    }
}

/// Percent-encodes all bytes except `A-Z a-z 0-9 - . _ ~` (RFC 3986).
fn encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xf)]));
        }
    }
    out
}

/// Adds an encoded `key=value` to the query of `url`, before a `#fragment`.
fn with_query(url: &str, key: &str, value: &str) -> String {
    let (base, fragment) = url
        .split_once('#')
        .map_or((url, None), |(b, f)| (b, Some(f)));
    let separator = if !base.contains('?') {
        "?"
    } else if base.ends_with(['?', '&']) {
        ""
    } else {
        "&"
    };
    let mut out = format!("{base}{separator}{}={}", encode(key), encode(value));
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// Adds a header. An `Authorization` header replaces the earlier ones:
/// two are never correct.
fn add_header(headers: &mut Vec<(String, String)>, key: &str, value: &str) {
    if key.eq_ignore_ascii_case("authorization") {
        headers.retain(|(k, _)| !k.eq_ignore_ascii_case("authorization"));
    }
    headers.push((key.to_string(), value.to_string()));
}

/// Sets the one `Authorization` header.
fn set_authorization(headers: &mut Vec<(String, String)>, value: &str) {
    add_header(headers, "Authorization", value);
}

/// Sleeps for `wait`. Gives an error that holds the interrupt when a
/// `script::guard` catches one.
#[cfg(feature = "script")]
fn pause(wait: Duration) -> Option<anyhow::Error> {
    crate::script::wait(wait).map(anyhow::Error::new)
}

/// Sleeps for `wait`. Without the `script` feature, no guard can stop it.
#[cfg(not(feature = "script"))]
fn pause(wait: Duration) -> Option<anyhow::Error> {
    std::thread::sleep(wait);
    None
}

/// The URL without user info and query values, for the retry line.
/// A query can hold a token.
fn redact(url: &str) -> String {
    let url = url.split_once('#').map_or(url, |(base, _)| base);
    let (url, query) = url.split_once('?').map_or((url, false), |(b, _)| (b, true));
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let mut out = String::with_capacity(url.len() + 2);
    if !scheme.is_empty() {
        out.push_str(scheme);
        out.push_str("://");
    }
    out.push_str(host);
    if rest.contains('/') {
        out.push('/');
        out.push_str(path);
    }
    if query {
        out.push_str("?…");
    }
    out
}

/// The stderr line for one retry.
fn retry_note(
    method: &str,
    url: &str,
    reason: &str,
    n: u32,
    retries: u32,
    wait: Duration,
) -> String {
    format!(
        "warning: {method} {}: {reason}; retry {n} of {retries} in {}s",
        redact(url),
        wait.as_secs_f64()
    )
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
        // A timeout gives the same error before and after the headers.
        let body = response.into_body().read_to_vec().map_err(|e| match e {
            ureq::Error::Timeout(_) => handle_ureq_error(e, &url),
            ureq::Error::Io(ref io) if io.kind() == std::io::ErrorKind::TimedOut => {
                handle_ureq_error(e, &url)
            }
            e => anyhow::Error::new(e).context(format!("Failed to read response body from: {url}")),
        })?;
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
/// It does not retry. For retries, use [`Client::retry`] and
/// [`Client::download`].
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

/// [`Client::timeout`] sets no limit for a value above this.
const NO_LIMIT_ABOVE: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);

/// Configurable HTTP client with builder pattern.
///
/// Headers, query pairs, the timeout and the [`Retry`] apply to every request
/// made through the client, including raw ones from [`Client::request`].
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
/// use smop::http;
///
/// let client = http::Client::new()
///     .timeout(Duration::from_secs(30))
///     .header("X-Api-Key", "secret");
///
/// let response = client.get("https://api.example.com/data")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[must_use]
pub struct Client {
    agent: ureq::Agent,
    headers: Vec<(String, String)>,
    query: Vec<(String, String)>,
    retry: Retry,
}

impl Client {
    /// Creates a new HTTP client with default settings.
    pub fn new() -> Self {
        Self {
            agent: ureq::Agent::new_with_defaults(),
            headers: Vec::new(),
            query: Vec::new(),
            retry: Retry::new(0),
        }
    }

    /// Sets a time limit for each whole request, from connect to the end of
    /// the body. A request that goes past it fails with a timeout error.
    ///
    /// `Duration::ZERO` makes every request fail. A limit above 100 years,
    /// such as `Duration::MAX`, sets no limit.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use smop::http;
    ///
    /// let client = http::Client::new().timeout(Duration::from_secs(30));
    /// ```
    pub fn timeout(mut self, timeout: Duration) -> Self {
        // ureq adds the limit to an `Instant` without a check, which panics
        // on overflow. 100 years is safe on all platforms.
        let timeout = (timeout <= NO_LIMIT_ABOVE).then_some(timeout);
        let config = ureq::config::Config::builder()
            .timeout_global(timeout)
            .build();
        self.agent = ureq::Agent::new_with_config(config);
        self
    }

    /// Sets HTTP Basic authentication credentials
    /// (`Authorization: Basic base64(username:password)`).
    ///
    /// It replaces an `Authorization` header set before.
    pub fn auth(mut self, username: &str, password: &str) -> Self {
        let credentials = base64_encode(format!("{username}:{password}").as_bytes());
        set_authorization(&mut self.headers, &format!("Basic {credentials}"));
        self
    }

    /// Sets `Authorization: Bearer <token>` for all requests. It replaces an
    /// `Authorization` header set before.
    pub fn bearer(mut self, token: &str) -> Self {
        set_authorization(&mut self.headers, &format!("Bearer {token}"));
        self
    }

    /// Adds a custom header to all requests. An `Authorization` header
    /// replaces an earlier one.
    pub fn header(mut self, key: &str, value: &str) -> Self {
        add_header(&mut self.headers, key, value);
        self
    }

    /// Adds `key=value` to the query string of all requests, before the
    /// pairs of [`Request::query`]. The encoding is that of
    /// [`Request::query`].
    pub fn query(mut self, key: &str, value: &str) -> Self {
        self.query.push((key.to_string(), value.to_string()));
        self
    }

    /// Sets the retry policy for all requests, [`download`](Self::download)
    /// included. [`Request::retry`] replaces it for one request.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::http;
    ///
    /// let client = http::Client::new().retry(http::Retry::new(3));
    /// let body = client.get("https://example.com/flaky")?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub const fn retry(mut self, retry: Retry) -> Self {
        self.retry = retry;
        self
    }

    /// Starts a raw request that returns the [`Response`] whatever its status.
    ///
    /// The client's headers, query pairs, timeout and [`Retry`] apply.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::http;
    ///
    /// let gh = http::Client::new().bearer("TOKEN");
    /// let resp = gh
    ///     .request("GET", "https://api.github.com/repos/owner/repo/pulls/1")
    ///     .send()?;
    /// let exists = resp.status() != 404;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn request(&self, method: &str, url: &str) -> Request {
        Request::new(
            self.agent.clone(),
            &self.headers,
            &self.query,
            self.retry.clone(),
            method,
            url,
        )
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
    /// The client [`Retry`] applies. If the body stops before its end,
    /// smop removes the partial file.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, returns a non-2xx status (the
    /// message includes the response body), or the file can't be written.
    pub fn download<P: AsRef<Path>>(&self, url: &str, path: P) -> Result<()> {
        let path = path.as_ref();
        let request = self.request("GET", url);
        request
            .retrying(|req| req.download_once(path), Option::as_ref)?
            .map_or(Ok(()), |failed| failed.error_for_status().map(drop))
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

/// A timeout. Only a timeout causes a retry, so it has its own type.
#[derive(Debug)]
struct TimedOut(String);

impl std::fmt::Display for TimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TimedOut {}

fn is_timeout(err: &anyhow::Error) -> bool {
    err.chain().any(<dyn std::error::Error>::is::<TimedOut>)
}

/// Converts transport-level ureq errors to anyhow errors naming the URL.
fn handle_ureq_error(error: ureq::Error, url: &str) -> anyhow::Error {
    match error {
        ureq::Error::StatusCode(code) => {
            anyhow!("HTTP request to {url} failed with status {code}")
        }
        ureq::Error::Timeout(kind) => anyhow::Error::new(TimedOut(format!(
            "HTTP request to {url} timed out: {kind:?}"
        ))),
        ureq::Error::Io(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            anyhow::Error::new(TimedOut(format!("HTTP request to {url} timed out: {e}")))
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

// A mock server stays until the end of its test, so the server lock does too.
#[cfg(test)]
#[allow(clippy::significant_drop_tightening)]
mod tests {
    use super::*;
    use mockito::Matcher;
    use std::time::Instant;

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
        assert_eq!(resp.bytes(), [] as [u8; 0]);
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

        let client = Client::new()
            .header("X-Api-Key", "secret")
            .timeout(Duration::from_secs(10));
        let url = format!("{}/h", server.url());
        assert_eq!(client.get(&url).unwrap(), "ok");
        assert_eq!(client.request("GET", &url).send().unwrap().status(), 200);
        mock.assert();
    }

    /// A mock whose body comes `delay` after the request.
    fn slow_mock(server: &mut mockito::ServerGuard, path: &str, delay: Duration) -> mockito::Mock {
        server
            .mock("GET", path)
            .with_chunked_body(move |w| {
                std::thread::sleep(delay);
                w.write_all(b"late")
            })
            .create()
    }

    #[test]
    fn client_timeout_fails_a_slow_response() {
        let mut server = mockito::Server::new();
        let delay = Duration::from_secs(2);
        slow_mock(&mut server, "/slow", delay);
        let url = format!("{}/slow", server.url());
        let client = Client::new().timeout(Duration::from_millis(200));

        let typed = || client.get(&url).map(drop);
        let raw = || client.request("GET", &url).send().map(drop);
        for call in [&typed as &dyn Fn() -> Result<()>, &raw] {
            let start = Instant::now();
            let msg = format!("{:#}", call().unwrap_err());
            assert!(msg.contains("timed out"), "{msg}");
            assert!(start.elapsed() < delay, "{msg}");
        }
    }

    #[test]
    fn client_timeout_fails_a_slow_download_and_removes_the_file() {
        let mut server = mockito::Server::new();
        slow_mock(&mut server, "/big", Duration::from_secs(2));
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("big.bin");
        let client = Client::new().timeout(Duration::from_millis(200));

        let err = client
            .download(&format!("{}/big", server.url()), &path)
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("timed out"), "{msg}");
        assert!(!path.exists(), "a failed download must not leave a file");
    }

    #[test]
    fn client_timeout_fails_when_no_response_comes() {
        // The OS accepts the connection, but nothing reads or answers.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let client = Client::new().timeout(Duration::from_millis(200));

        let start = Instant::now();
        let msg = format!("{:#}", client.get(&url).unwrap_err());
        assert!(msg.contains("timed out"), "{msg}");
        assert!(start.elapsed() < Duration::from_secs(5), "{msg}");
    }

    #[test]
    fn client_timeout_zero_fails_every_request() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/z").with_body("ok").create();
        let client = Client::new().timeout(Duration::ZERO);
        let msg = format!(
            "{:#}",
            client.get(&format!("{}/z", server.url())).unwrap_err()
        );
        assert!(msg.contains("timed out"), "{msg}");
    }

    #[test]
    fn client_timeout_allows_a_response_inside_the_limit() {
        let mut server = mockito::Server::new();
        slow_mock(&mut server, "/ok", Duration::from_millis(50));
        let client = Client::new().timeout(Duration::from_secs(10));
        assert_eq!(client.get(&format!("{}/ok", server.url())).unwrap(), "late");
    }

    #[test]
    fn client_timeout_max_sets_no_limit() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/m").with_body("ok").create();
        let url = format!("{}/m", server.url());
        let just_above = NO_LIMIT_ABOVE + Duration::from_nanos(1);
        for timeout in [Duration::MAX, just_above, NO_LIMIT_ABOVE] {
            let client = Client::new().timeout(timeout);
            assert_eq!(client.get(&url).unwrap(), "ok");
        }
    }

    #[test]
    fn client_auth_sends_base64_basic_credentials() {
        // Credentials are generated at runtime (no hard-coded secret); the
        // encoding itself is pinned by `base64_matches_rfc4648_vectors`.
        let user = "smop-test";
        let password = std::process::id().to_string();
        let expected = format!(
            "Basic {}",
            base64_encode(format!("{user}:{password}").as_bytes())
        );

        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/a")
            .match_header("authorization", expected.as_str())
            .with_body("ok")
            .create();

        let client = Client::new().auth(user, &password);
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
            // RFC 7617 section 2 Basic auth example (user-id:password).
            ("Aladdin:open sesame", "QWxhZGRpbjpvcGVuIHNlc2FtZQ=="),
        ];
        for (input, expected) in cases {
            assert_eq!(base64_encode(input.as_bytes()), expected, "input {input:?}");
        }
    }

    // ---- query and bearer ------------------------------------------------

    #[test]
    fn encode_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(encode("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(
            encode("a b&c=d/é+?#%"),
            "a%20b%26c%3Dd%2F%C3%A9%2B%3F%23%25"
        );
        assert_eq!(encode(""), "");
    }

    #[test]
    fn with_query_appends_in_the_right_place() {
        let cases = [
            ("http://h/p", "http://h/p?k=v"),
            ("http://h/p?a=1", "http://h/p?a=1&k=v"),
            ("http://h/p?", "http://h/p?k=v"),
            ("http://h/p?a=1&", "http://h/p?a=1&k=v"),
            ("http://h/p#top", "http://h/p?k=v#top"),
            ("http://h/p?a=1#top", "http://h/p?a=1&k=v#top"),
        ];
        for (url, expected) in cases {
            assert_eq!(with_query(url, "k", "v"), expected, "{url}");
        }
    }

    #[test]
    fn query_percent_encodes_keys_and_values() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/search")
            .match_query(Matcher::AllOf(vec![
                Matcher::Exact("q=a%20b%26c%3Dd&k%C3%A9y=%2B1&page=2".into()),
                Matcher::UrlEncoded("q".into(), "a b&c=d".into()),
                Matcher::UrlEncoded("kéy".into(), "+1".into()),
            ]))
            .with_body("ok")
            .expect(2)
            .create();

        let url = format!("{}/search", server.url());
        let resp = request("GET", &url)
            .query("q", "a b&c=d")
            .query("kéy", "+1")
            .query("page", "2")
            .send()
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert!(
            resp.url()
                .ends_with("/search?q=a%20b%26c%3Dd&k%C3%A9y=%2B1&page=2")
        );

        // Client pairs come first, then the request's.
        let client = Client::new().query("q", "a b&c=d");
        let resp = client
            .request("GET", &url)
            .query("kéy", "+1")
            .query("page", "2")
            .send()
            .unwrap();
        assert_eq!(resp.status(), 200);
        mock.assert();
    }

    #[test]
    fn bearer_sends_authorization_header() {
        let token = format!("t-{}", std::process::id());
        let expected = format!("Bearer {token}");
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/b")
            .match_header("authorization", expected.as_str())
            .with_body("ok")
            .expect(2)
            .create();
        let url = format!("{}/b", server.url());

        let resp = request("GET", &url).bearer(&token).send().unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(Client::new().bearer(&token).get(&url).unwrap(), "ok");
        mock.assert();
    }

    #[test]
    fn bearer_and_auth_replace_an_earlier_authorization() {
        // Values come at runtime: no hard-coded secret.
        let user = "smop-test";
        let password = std::process::id().to_string();
        let (old, new) = (format!("old-{password}"), format!("new-{password}"));
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/b")
            .match_header("authorization", format!("Bearer {new}").as_str())
            .with_body("ok")
            .expect(3)
            .create();
        let url = format!("{}/b", server.url());

        let client = Client::new().auth(user, &password).bearer(&new);
        assert_eq!(client.get(&url).unwrap(), "ok");
        let client = Client::new().header("authorization", &format!("Bearer {old}"));
        let resp = client.request("GET", &url).bearer(&new).send().unwrap();
        assert_eq!(resp.status(), 200);
        let resp = request("GET", &url)
            .bearer(&old)
            .bearer(&new)
            .send()
            .unwrap();
        assert_eq!(resp.status(), 200);
        mock.assert();

        let basic = format!(
            "Basic {}",
            base64_encode(format!("{user}:{password}").as_bytes())
        );
        let mock = server
            .mock("GET", "/a")
            .match_header("authorization", basic.as_str())
            .with_body("ok")
            .create();
        let client = Client::new().bearer(&old).auth(user, &password);
        assert_eq!(client.get(&format!("{}/a", server.url())).unwrap(), "ok");
        mock.assert();
    }

    // ---- retries ------------------------------------------------------------

    /// No wait between attempts.
    fn fast(retries: u32) -> Retry {
        Retry::new(retries).delay(Duration::ZERO)
    }

    #[test]
    fn retry_repeats_each_listed_status_then_succeeds() {
        let mut server = mockito::Server::new();
        let client = Client::new().retry(fast(1));
        for code in [408, 429, 500, 502, 503, 504] {
            let path = format!("/s{code}");
            let fail = server
                .mock("GET", path.as_str())
                .with_status(code)
                .expect(1)
                .create();
            let ok = server
                .mock("GET", path.as_str())
                .with_body("ok")
                .expect(1)
                .create();
            let url = format!("{}{path}", server.url());
            assert_eq!(client.get(&url).unwrap(), "ok", "status {code}");
            fail.assert();
            ok.assert();
        }
    }

    #[test]
    fn retry_skips_other_statuses() {
        let mut server = mockito::Server::new();
        let client = Client::new().retry(fast(3));
        for code in [400, 401, 403, 404, 409, 422, 501, 505] {
            let path = format!("/s{code}");
            let mock = server
                .mock("GET", path.as_str())
                .with_status(code)
                .with_header("retry-after", "0")
                .expect(1)
                .create();
            let url = format!("{}{path}", server.url());
            let resp = client.request("GET", &url).send().unwrap();
            assert_eq!(usize::from(resp.status()), code);
            mock.assert();
        }
    }

    #[test]
    fn retry_stops_after_the_count_and_returns_the_last_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/down")
            .with_status(503)
            .with_body("try later")
            .expect(3)
            .create();
        let url = format!("{}/down", server.url());
        let resp = request("GET", &url).retry(fast(2)).send().unwrap();
        assert_eq!(resp.status(), 503);
        assert_eq!(resp.text().unwrap(), "try later");
        mock.assert();

        let mock = server
            .mock("GET", "/down")
            .with_status(503)
            .with_body("try later")
            .expect(3)
            .create();
        let msg = Client::new()
            .retry(fast(2))
            .get(&url)
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("status 503") && msg.contains("try later"),
            "{msg}"
        );
        mock.assert();
    }

    #[test]
    fn no_retry_by_default() {
        let mut server = mockito::Server::new();
        // Three calls, three hits: no call is retried.
        let mock = server.mock("GET", "/d").with_status(503).expect(3).create();
        let url = format!("{}/d", server.url());
        assert_eq!(request("GET", &url).send().unwrap().status(), 503);
        assert!(get(&url).is_err());
        let dir = tempfile::TempDir::new().unwrap();
        assert!(download(&url, dir.path().join("f")).is_err());
        mock.assert();
    }

    #[test]
    fn request_retry_replaces_the_client_retry() {
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", "/r").with_status(503).expect(1).create();
        let url = format!("{}/r", server.url());
        let client = Client::new().retry(fast(3));
        let resp = client
            .request("GET", &url)
            .retry(Retry::new(0))
            .send()
            .unwrap();
        assert_eq!(resp.status(), 503);
        mock.assert();
    }

    #[test]
    fn retry_after_seconds_is_honoured() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/ra")
            .with_status(429)
            .with_header("retry-after", "1")
            .expect(1)
            .create();
        server.mock("GET", "/ra").with_body("ok").create();
        let client = Client::new().retry(fast(1));

        let start = Instant::now();
        assert_eq!(client.get(&format!("{}/ra", server.url())).unwrap(), "ok");
        assert!(
            start.elapsed() >= Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn retry_after_date_is_honoured() {
        // A date in the past means no wait, not the 10 s backoff.
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/rd")
            .with_status(503)
            .with_header("retry-after", "Sun, 06 Nov 1994 08:49:37 GMT")
            .expect(1)
            .create();
        server.mock("GET", "/rd").with_body("ok").create();
        let client = Client::new().retry(Retry::new(1).delay(Duration::from_secs(10)));

        let start = Instant::now();
        assert_eq!(client.get(&format!("{}/rd", server.url())).unwrap(), "ok");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn retry_after_is_capped_in_both_forms() {
        let cap = Duration::from_millis(300);
        for value in ["3600", "Fri, 01 Jan 2100 00:00:00 GMT"] {
            let mut server = mockito::Server::new();
            let fail = server
                .mock("GET", "/cap")
                .with_status(503)
                .with_header("retry-after", value)
                .expect(1)
                .create();
            let ok = server
                .mock("GET", "/cap")
                .with_body("ok")
                .expect(1)
                .create();
            let client = Client::new().retry(fast(1).max_delay(cap));

            let start = Instant::now();
            assert_eq!(client.get(&format!("{}/cap", server.url())).unwrap(), "ok");
            let took = start.elapsed();
            assert!(took >= cap, "{value}: {took:?}");
            assert!(took < Duration::from_secs(5), "{value}: {took:?}");
            fail.assert();
            ok.assert();
        }
    }

    #[test]
    fn post_and_patch_retry_only_when_asked() {
        let mut server = mockito::Server::new();
        for method in ["POST", "PATCH"] {
            let mock = server
                .mock(method, "/w")
                .with_status(503)
                .expect(1)
                .create();
            let url = format!("{}/w", server.url());
            let resp = request(method, &url)
                .body("x")
                .retry(fast(2))
                .send()
                .unwrap();
            assert_eq!(resp.status(), 503);
            mock.assert();

            let mock = server
                .mock(method, "/w")
                .match_body("x")
                .with_status(503)
                .expect(3)
                .create();
            let resp = request(method, &url)
                .body("x")
                .retry(fast(2).any_method())
                .send()
                .unwrap();
            assert_eq!(resp.status(), 503);
            mock.assert();
        }
    }

    #[test]
    fn idempotent_methods_retry_and_resend_the_body() {
        let mut server = mockito::Server::new();
        for method in ["PUT", "DELETE"] {
            let fail = server
                .mock(method, "/i")
                .match_body("same")
                .with_status(502)
                .expect(1)
                .create();
            let ok = server
                .mock(method, "/i")
                .match_body("same")
                .with_body("ok")
                .expect(1)
                .create();
            let url = format!("{}/i", server.url());
            let resp = request(method, &url)
                .body("same")
                .retry(fast(1))
                .send()
                .unwrap();
            assert_eq!(resp.status(), 200, "{method}");
            fail.assert();
            ok.assert();
        }
    }

    /// A localhost port that accepts connections but never answers.
    /// Returns its URL and the number of connections so far.
    fn silent_port() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let count = std::sync::Arc::new(AtomicUsize::new(0));
        let seen = std::sync::Arc::clone(&count);
        std::thread::spawn(move || {
            // Keep each connection open: the client gets no reply.
            let open: Vec<_> = listener
                .incoming()
                .inspect(|_| {
                    seen.fetch_add(1, Ordering::SeqCst);
                })
                .collect();
            drop(open);
        });
        (url, count)
    }

    /// The connection count, after the accept thread had time to see the
    /// last connection.
    fn settled(count: &std::sync::atomic::AtomicUsize, expected: usize) -> usize {
        use std::sync::atomic::Ordering;
        let start = Instant::now();
        while count.load(Ordering::SeqCst) < expected && start.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
        }
        // A late extra connection also has time to show.
        std::thread::sleep(Duration::from_millis(50));
        count.load(Ordering::SeqCst)
    }

    #[test]
    fn retry_on_timeout() {
        let (url, count) = silent_port();
        let client = Client::new()
            .timeout(Duration::from_millis(200))
            .retry(fast(2));
        let msg = format!("{:#}", client.get(&url).unwrap_err());
        assert!(msg.contains("timed out"), "{msg}");
        assert_eq!(settled(&count, 3), 3, "{msg}");
    }

    #[test]
    fn timeout_does_not_retry_post_by_default() {
        let (url, count) = silent_port();
        let client = Client::new()
            .timeout(Duration::from_millis(200))
            .retry(fast(2));
        assert!(client.post(&url, "x").is_err());
        assert_eq!(settled(&count, 1), 1);
    }

    #[test]
    fn retry_on_a_body_timeout_in_send() {
        let url = scripted(&[
            "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nok",
            "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nfull",
        ]);
        let client = Client::new()
            .timeout(Duration::from_millis(500))
            .retry(fast(1));
        assert_eq!(client.get(&url).unwrap(), "full");
    }

    #[test]
    fn invalid_retry_after_uses_the_backoff() {
        let mut server = mockito::Server::new();
        let fail = server
            .mock("GET", "/bad")
            .with_status(503)
            .with_header("retry-after", "soon")
            .expect(1)
            .create();
        let ok = server
            .mock("GET", "/bad")
            .with_body("ok")
            .expect(1)
            .create();
        let backoff = Duration::from_millis(300);
        let client = Client::new().retry(Retry::new(1).delay(backoff));

        let start = Instant::now();
        assert_eq!(client.get(&format!("{}/bad", server.url())).unwrap(), "ok");
        let took = start.elapsed();
        assert!(took >= backoff && took < Duration::from_secs(5), "{took:?}");
        fail.assert();
        ok.assert();
    }

    #[test]
    fn connection_refused_is_not_retried() {
        // curl --retry does not retry it either: only timeouts.
        let url = closed_port_url();
        let client = Client::new().retry(Retry::new(3).delay(Duration::from_secs(10)));
        let start = Instant::now();
        let err = client.get(&url).unwrap_err();
        assert!(err.to_string().contains(&url), "{err}");
        assert!(start.elapsed() < Duration::from_secs(5), "{err}");
    }

    /// Reads a request head from `stream`: the lines up to the empty line.
    fn read_head(stream: &std::net::TcpStream) -> std::io::Result<String> {
        use std::io::{BufRead, BufReader};
        let mut reader = BufReader::new(stream);
        let mut head = String::new();
        loop {
            let before = head.len();
            if reader.read_line(&mut head)? == 0 || head[before..].trim().is_empty() {
                return Ok(head);
            }
        }
    }

    /// A localhost server for one connection per reply: connection `i`
    /// gets `replies[i]`. Each connection has its own thread and stays open
    /// for 3 s, so a reply with a short body stalls the client. A failure in
    /// a server thread shows as a client error.
    fn scripted(replies: &'static [&'static str]) -> String {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for (stream, reply) in listener.incoming().zip(replies) {
                std::thread::spawn(move || {
                    let Ok(mut stream) = stream else { return };
                    if read_head(&stream).is_ok() {
                        let _ = stream.write_all(reply.as_bytes());
                        std::thread::sleep(Duration::from_secs(3));
                    }
                });
            }
        });
        url
    }

    /// A localhost server that answers one request with `200 ok` and
    /// gives its request head.
    fn capture() -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let Ok(head) = read_head(&stream) else { return };
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            let _ = tx.send(head);
        });
        (url, rx)
    }

    /// The `Authorization` values in a request head.
    fn authorization(head: &str) -> Vec<String> {
        head.lines()
            .filter_map(|l| l.split_once(':'))
            .filter(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            .map(|(_, v)| v.trim().to_string())
            .collect()
    }

    #[test]
    fn header_authorization_replaces_bearer_and_is_sent_once() {
        let (url, head) = capture();
        let client = Client::new().bearer("a");
        let resp = client
            .request("GET", &url)
            .header("Authorization", "Bearer b")
            .send()
            .unwrap();
        assert_eq!(resp.status(), 200);
        let head = head.recv().unwrap();
        assert_eq!(authorization(&head), ["Bearer b"], "{head}");

        let (url, head) = capture();
        let client = Client::new().bearer("a").header("authorization", "Basic x");
        assert_eq!(client.get(&url).unwrap(), "ok");
        assert_eq!(authorization(&head.recv().unwrap()), ["Basic x"]);

        // Other headers still add.
        let (url, head) = capture();
        let resp = request("GET", &url)
            .header("X-Tag", "1")
            .header("X-Tag", "2")
            .send()
            .unwrap();
        assert_eq!(resp.status(), 200);
        let head = head.recv().unwrap();
        assert_eq!(head.matches("x-tag").count(), 2, "{head}");
    }

    #[test]
    fn download_retries_a_status_and_a_slow_body() {
        let url = scripted(&[
            "HTTP/1.1 503 Busy\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nfile",
            "HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nfile contents",
        ]);
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("out.txt");
        Client::new()
            .timeout(Duration::from_millis(500))
            .retry(fast(2))
            .download(&url, &path)
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "file contents");
    }

    #[test]
    fn download_gives_up_with_the_last_status() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/g")
            .with_status(500)
            .with_body("broken")
            .expect(2)
            .create();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("out.txt");
        let err = Client::new()
            .retry(fast(1))
            .download(&format!("{}/g", server.url()), &path)
            .unwrap_err();
        assert!(err.to_string().contains("broken"), "{err}");
        assert!(!path.exists());
        mock.assert();
    }

    #[test]
    fn retry_note_names_the_request_and_the_wait() {
        assert_eq!(
            retry_note(
                "GET",
                "http://h/",
                "status 503",
                1,
                3,
                Duration::from_millis(1500)
            ),
            "warning: GET http://h/: status 503; retry 1 of 3 in 1.5s"
        );
        let note = retry_note(
            "GET",
            "https://u:secret@h/p?api_key=secret",
            "timed out",
            1,
            1,
            Duration::ZERO,
        );
        assert!(!note.contains("secret"), "{note}");
    }

    #[test]
    fn redact_removes_user_info_query_and_fragment() {
        let cases = [
            ("http://h/", "http://h/"),
            ("http://h", "http://h"),
            ("http://h:8080/a/b", "http://h:8080/a/b"),
            ("http://h/p?token=t", "http://h/p?…"),
            ("http://h?token=t", "http://h?…"),
            ("https://user:pw@h/p", "https://h/p"),
            ("https://user:p@ss@h/p", "https://h/p"),
            ("http://h/p#frag", "http://h/p"),
            ("http://h/p?t=1#frag", "http://h/p?…"),
            ("not a url", "not a url"),
        ];
        for (url, expected) in cases {
            assert_eq!(redact(url), expected, "{url}");
        }
    }
}
