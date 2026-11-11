- `env::var_opt::<T>(name) -> Result<Option<T>>` (#36): `None` when the
  variable is unset or empty, an error when it does not parse or is not
  valid UTF-8.
