- `re` feature (in `full`): `smop::re` re-exports all of `regex` and adds
  helpers that take the pattern as `&str`: `compile`, `is_match`, `find`,
  `find_all`, `captures`, `captures_all`, `replace`, `replace_all` and
  `split`. An invalid pattern gives `Invalid regex pattern '<pattern>'`;
  a pattern that is too big gives `Regex pattern '<pattern>' is too big`.
  Each helper compiles its pattern on each call. In a loop, use `compile`.
