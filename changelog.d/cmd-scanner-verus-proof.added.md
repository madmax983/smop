- The scanner of the `cmd!` lexer has a Verus proof (ADR 0022): each
  `{name}` maps to exactly one argument, and literal text never splits a
  value. The proof is in `verus/`, not in a published crate. `cmd!` does
  not change.
