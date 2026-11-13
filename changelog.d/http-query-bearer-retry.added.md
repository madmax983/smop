- `http`: `Request::query` and `Client::query` add a percent-encoded
  `key=value` to the URL. `Request::bearer` and `Client::bearer` set
  `Authorization: Bearer <token>` (ADR 0018).
- `http::Retry` and `Client::retry` / `Request::retry`: opt-in retries,
  also for `download`. A retry occurs after a timeout or status 408, 429,
  500, 502, 503 or 504, as with `curl --retry`. The wait starts at 1 s and
  doubles. `Retry-After` (seconds or HTTP-date) replaces it, and
  `max_delay` (60 s) caps it. POST and PATCH retry only with
  `any_method()`. Each retry writes one `warning:` line to stderr.
