# Verus proof of the `cmd!` scanner

`lexer.rs` is a copy of `smop-macros/src/scan.rs` with a Verus proof
([ADR 0022](../docs/adr/0022-verus-proof-of-the-cmd-scanner.md)). This
directory is not in a published crate.

It proves two properties of the scanner:

- P1, `each_placeholder_is_one_argument`: each `{name}` maps to exactly one
  argv element.
- P2, `text_never_splits_a_word`: literal text never splits a value.

## Run it

```bash
cargo xtask verus-copy             # the copy check only (CI runs it)
cargo xtask verus                  # the copy check, then Verus (x86-64 Linux)
cargo xtask verus --verus ~/verus/verus  # use a local Verus
```

`cargo xtask verus` downloads the pinned release to `target/verus/`, checks
its SHA-256 and installs its Rust toolchain with `rustup`. The pin is
`RELEASE`, `SHA256` and `TOOLCHAIN` in `xtask/src/verus.rs`. The optional
`Verus` workflow (`.github/workflows/verus.yml`) runs the same command.

## Change the scanner

1. Change `smop-macros/src/scan.rs`.
2. Make the same change in `lexer.rs`, and fix the proof.
3. Run `cargo xtask verus`. Without Verus, run `cargo xtask verus-copy`
   and let the Verus workflow run the proof.

The copy check erases only Verus syntax. It rejects `assume`, `admit`,
`external_body` and the other words in `UNSOUND` (`xtask/src/verus.rs`).
