# ADR 0018: HTTP Query, Bearer Token and Retries

## Status

Accepted (0.5 cycle). Issue [madmax983/smop#50](https://github.com/madmax983/smop/issues/50).

## Context

Harvest's xtask downloads three pinned files with
`curl -fsSL --retry 3 -o FILE URL`, then checks each SHA-256. smop has
`http::download`, but it has no retries. Measured against curl 8.5.0,
`--retry 3` retries a timeout and status 408, 429, 500, 502, 503 and 504.
It does not retry 404, 501 or 505. It does not retry a refused connection
(that needs `--retry-connrefused`). The first wait is 1 s and it doubles.
A `Retry-After` header replaces the wait.

API scripts also put values into query strings by `format!` and set
`Authorization: Bearer` by hand.

## Decision

- `Request::query(key, value)` and `Client::query(key, value)`. smop
  percent-encodes the key and the value. Only the RFC 3986 unreserved
  characters (`A-Z a-z 0-9 - . _ ~`) stay. A space is `%20`, not `+`. The
  pair goes into the URL at once, before a `#fragment`, so
  `Response::url()` and errors show the URL that was sent. Client pairs
  come first.
- `Request::bearer(token)` and `Client::bearer(token)` set
  `Authorization: Bearer <token>`. They replace an earlier
  `Authorization` header. `Client::auth` and `header("Authorization", ..)`
  now do the same. Two `Authorization` headers are never correct.
- One value type, `http::Retry`, holds the policy. `Client::retry` sets
  it for all requests, `download` included. `Request::retry` replaces it
  for one request. Retries are off by default.
  - `Retry::new(n)`: `n` retries after the first attempt, as with
    `curl --retry n`.
  - A retry occurs only after a timeout, or after status 408, 429, 500,
    502, 503 or 504. A timeout is a `ureq` timeout or an I/O error of
    kind `TimedOut`. A private error type marks it, so no `ureq` type is
    in the error chain. Other transport errors, such as a refused
    connection, do not retry.
  - The wait before retry `k` is `delay × 2^(k-1)` (`delay` is 1 s). A
    valid `Retry-After` replaces it: delay-seconds, or an HTTP-date in
    each of the three forms of RFC 9110, 5.6.7. A two-digit year is not
    more than 50 years in the future. A date in the past is no wait. A
    value that is not valid gives the backoff.
  - `max_delay` (60 s) caps each wait, `Retry-After` included. A script
    must not wait an hour because a server says so. It is the only cap.
  - Only GET, HEAD, PUT, DELETE, OPTIONS and TRACE retry (the idempotent
    methods of RFC 9110, 9.2.2). `any_method()` adds POST, PATCH and all
    other methods.
  - After the last attempt, the result is that of the last attempt: the
    raw layer gives the last `Response`, the typed layer its error.
  - The client timeout applies to each attempt.
  - Each retry writes one line to stderr, as curl does:
    `warning: GET https://host/path?…: status 503; retry 1 of 3 in 1s`.
    The line has no user info and no query values: they can hold a token.
  - With the `script` feature, an interrupt that a `script::guard`
    catches stops the wait. No new attempt starts. The error holds
    `script::Interrupted`, as `script::retry` and `sh` do (ADR 0011).
  - `Retry` is `Clone`, not `Copy`. A later option can then hold a list
    or a closure with no breaking change.
- The parser and the policy are plain functions in `src/http/retry.rs`,
  with `now` as an argument. Unit tests cover them with no clock and no
  server.

## Options considered

- `Client::retries(n)` with three more setters on `Client` and `Request`:
  eight methods instead of one type.
- A crate for retries or for HTTP dates (`httpdate`): adds a dependency
  for about 80 lines.
- `percent-encoding`: it is in the tree through `ureq`, but a direct
  dependency is one more crate to keep in step. smop has its own base64
  for the same reason.
- Retry a refused connection, as `curl --retry-connrefused` does: a
  wrong port or a typo then waits for all retries. Plain `curl --retry`
  does not do it either. A test on a closed localhost port shows that
  smop does not retry it.
- Wait the full `Retry-After`, or stop when it is above the cap: the
  first can stop CI for hours; the second gives up when a short wait
  would work.
- Retry silently: a script that waits 60 s with no output looks hung.

## Consequences

- Harvest's downloads become
  `http::Client::new().retry(http::Retry::new(3)).download(url, file)`.
- A retry can come before the `Retry-After` time when the cap is lower.
  Set `max_delay` higher to wait longer.
- A request can take `(n + 1) × timeout` plus the waits.
- A `Retry::any_method()` POST can reach the server two times. The docs
  say so.
- An error message still quotes the full URL, as before this change.
