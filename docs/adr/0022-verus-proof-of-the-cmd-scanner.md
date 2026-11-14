# ADR 0022: A Verus Proof of the `cmd!` Scanner

## Status

Accepted (0.5 cycle). Issue [madmax983/smop#55](https://github.com/madmax983/smop/issues/55).
Extends [ADR 0007](0007-cmd-macro.md) (Testing) and
[ADR 0017](0017-xtask-tooling.md).

## Context

- The `cmd!` lexer is pure code over the template text. A proof of it
  is worth the cost (ADR 0007).
- The issue asks for a proof of two properties:
  - each `{name}` maps to exactly one argv element
  - literal text never splits a value
- Verus needs its own crates and a pinned toolchain. `smop-macros` must
  keep zero dependencies.
- The lexer used `&str` iterators, `split`, `find`, closures and
  `format!`. Verus has no specs for most of these.

## Decision

- Split the lexer into two modules:
  - `smop-macros/src/scan.rs`: the scanner. It reads `&[char]` and gives
    words of atoms with their positions, or a `Fault` with positions. It
    uses only Rust that Verus accepts: indexes, `while` loops, `?`, no
    closures and no `String`.
  - `smop-macros/src/lex.rs`: the glue. It makes the `String`s of each
    piece and the error messages. It has no word rules.
- `verus/lexer.rs` is a copy of `scan.rs` with ghost code: specs, loop
  invariants, `proof` blocks and lemmas. The `verus/` directory is not in
  a published crate (the root `Cargo.toml` excludes it).
- `cargo xtask verus-copy` erases the ghost code and compares the tokens
  that stay with `scan.rs`. Comments do not count. `&&` is not `& &`. A
  trailing comma does not count, except in a one-element group: `(x,)`
  is not `(x)`.
  - The copy is inner `#![verifier::..]` attributes, then
    `use vstd::prelude::*;`, then one `verus! {}` block. Verus does not
    check code outside `verus!`.
  - It erases only Verus syntax: `verus! {}`, `use vstd::prelude::*;`,
    `spec fn` and `proof fn` items, clauses (`requires`, `ensures`,
    `invariant`, `decreases`, ...) after a `fn`, `while`, `loop` or `for`
    header, `proof {}` and `let ghost` at the start of a statement,
    `#[verifier::..]`, `#[trigger]` and the name of a named return value.
    Other code stays and must be equal.
  - Neither file can use `cfg`, `include!`, `mod`, `crate`, `super`,
    `macro_rules` or `env!`: they can give the same tokens a different
    meaning in the two files.
  - The copy can use only `#[verifier::loop_isolation]`, `opaque` and
    `rlimit`.
  - `SPEC_SHA256` pins the statements: each `spec fn`, and the signature
    and clauses of each `pub fn`. A change to the spec or to the theorems
    fails the check until a person reviews it and sets the new value.
    Proof bodies, private lemmas and loop invariants can change freely.
  - It rejects `assume`, `admit`, `external`, `external_body`,
    `assume_specification`, `external_fn_specification`,
    `external_type_specification` and `exec_allows_no_decreases_clause` in
    the copy (`UNSOUND` in `xtask/src/verus.rs`). Verus also runs with
    `--no-cheating`.
  - CI's docs-drift job runs it. The xtask test
    `the_copy_in_the_repository_is_identical` also runs it in the Test job.
- `cargo xtask verus` runs the copy check, then Verus. It downloads
  release `0.2026.11.02.8bf68ee` for x86-64 Linux, checks its SHA-256,
  and installs its Rust toolchain (`1.99.1`). `--verus <path>` uses a
  local Verus.
- `.github/workflows/verus.yml` runs `cargo xtask verus`. It is optional:
  not a required check. It runs on a change to the scanner, the proof or
  the task, and by hand.

### The properties

`scan` ensures `lexed(t, words)`:

- Only separators are before, between and after the words. There is at
  least one separator between two words.
- The atoms of a word cover its text in sequence, with no gap and no
  overlap. A quoted section is a quote, atoms, and the same quote, at the
  end of the word.
- Each atom agrees with the text: one character (not a separator outside
  quotes), `{{` or `}}`, or `{`, ASCII name characters and `}` (with `...`
  before the `}` for a splat).
- In a word, a separator is only between the quotes.

Two theorems follow from `lexed`:

- `each_placeholder_is_one_argument` (P1): a `Var` atom of word `i` is
  `{`, name characters and `}` in the template. It is in word `i`, and in no other
  word. `expand` makes one argv element for each word that has no splat.
- `text_never_splits_a_word` (P2): a character that is not a separator,
  next to a character of a word, is in that word. Thus only separators
  end a word.

## Consequences

- The proof covers the scanner. The glue in `lex.rs` (strings, messages)
  has tests, not a proof. `scan_tests.rs` has the spec as executable Rust
  and proptests it. If the spec is too strong, these proptests fail.
- A change to `scan.rs` needs the same change in `verus/lexer.rs`. The
  copy check fails until both agree.
- A new Verus release needs new values for `RELEASE`, `SHA256` and
  `TOOLCHAIN` in `xtask/src/verus.rs`. The proof can need changes.
- The error messages and their order did not change: the `lex.rs` tests
  did not change, and they pass. Before the old lexer was removed, a
  differential proptest (not kept) compared the two on 400,000 templates.
