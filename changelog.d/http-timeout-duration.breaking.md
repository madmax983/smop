- **`http::Client::timeout` takes a `Duration`**, not seconds as a `u64`.
  Migration: `.timeout(30)` becomes `.timeout(Duration::from_secs(30))`.
  `Duration::ZERO` makes every request fail, as `.timeout(0)` did. A limit
  above 100 years, such as `Duration::MAX`, sets no limit. Before,
  `.timeout(u64::MAX)` panicked.
