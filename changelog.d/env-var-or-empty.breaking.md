- **`env::var_or` gives the default for an empty value** (#36). This is a
  change in behaviour only. It applies to a type that parses `""`, such as
  `String` or `PathBuf`: before, `var_or("NAME", "x".to_string())` with
  `NAME=` gave `""`. Migration: to keep the old behaviour, use
  `std::env::var("NAME").unwrap_or_else(|_| "x".to_string())`.
