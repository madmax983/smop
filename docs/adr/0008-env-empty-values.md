# ADR 0008: Strict `env` Parsing and Empty Values

## Status

Accepted (0.4.0). Issue [madmax983/smop#36](https://github.com/madmax983/smop/issues/36).

## Context

Up to 0.3, `env::var_or` gave the default when the value did not parse.
`PORT=808O` gave 8080 and no error. 0.4 removes silent failures.

A CI system or a compose file often sets a variable to an empty value
(`FOO=`). Harvest's xtask defines a `${NAME:-default}` helper five times.

## Decision

- A value that is set but does not parse is an error. The error names the
  variable and the value. This applies to `var`, `var_opt` and `var_or`.
- `var_or` returns `Result<T>`. We do not add a strict twin. The compiler
  shows each call site that must change.
- `var_opt` and `var_or` read an empty value as unset, as
  `${NAME:-default}` does. Whitespace is a value, not an empty value.
- `var` gives an empty value to the parser. It is the raw reader: `String`
  gives `""`, a number gives a parse error.
- `require_vars` checks only that a variable is set. An empty value passes.
- A value that is not valid UTF-8 is an error in all three functions. It is
  not "unset".
- `is_ci` uses the same rule: an empty `CI` is unset. Then it reads
  variables of CI systems that do not set `CI`. Jenkins needs `JENKINS_URL`
  and `BUILD_ID`, because `jenkins-cli` users set `JENKINS_URL` in their
  shell.

## Consequences

### Positive

- A typo in a value stops the script. It does not change the behaviour.
- One rule for optional settings: unset and empty are the same.

### Negative

- `var_or` breaks at each call site (add `?`).
- `var_or::<String>` no longer gives `""` for an empty value. To get it,
  use `std::env::var`.
- `var` and `var_opt` differ for an empty value. The module docs say so.
- The error text shows the value. If the logs are public, read a secret
  with `var::<String>`. It does not fail to parse.
