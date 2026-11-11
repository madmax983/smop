- `sh::CommandBuilder` is `#[must_use]`. A builder that no method starts,
  such as `cmd!("make");` or a builder that a `match` or `if` gives, is an
  `unused_must_use` warning. Before, only a direct call such as
  `sh::cmd("make");` gave the warning.
