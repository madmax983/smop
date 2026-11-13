- `http::Client::auth` and `header("Authorization", ..)` on `Client` and
  `Request` now replace an earlier `Authorization` header. Before, they
  added a second one.
- `http`: an I/O error of kind `TimedOut` now reads
  `HTTP request to <url> timed out: ...`, as other timeouts do.
