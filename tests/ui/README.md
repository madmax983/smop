# `cmd!` compile-fail cases

Each `.rs` file here must not compile. Its `.stderr` file is the compiler
output that `trybuild` expects. `tests/cmd_macro_errors.rs` runs the cases.

## Run the cases

The text of a rustc error is not stable:

- It changes between releases.
- It names a type by a longer path (`std::string::String`) when the crate
  graph has a second type of that name.

So the cases run on one pinned toolchain, on Linux, with the `macros`
feature alone:

```bash
cargo +1.97.0 test -p smop --no-default-features --features macros --test cmd_macro_errors -- --ignored
```

Without `--ignored`, the test binary only checks the snapshots as text. That
check runs on each platform, each toolchain and each feature set.

## Add or change a case

1. Add or edit `tests/ui/<case>.rs`.
2. Add the case and the message of its rule to `CASES` in
   `tests/cmd_macro_errors.rs`.
3. Write the snapshot again:

   ```bash
   TRYBUILD=overwrite cargo +1.97.0 test -p smop --no-default-features --features macros --test cmd_macro_errors -- --ignored
   ```

4. Read the new `.stderr` file before you commit it.

## Move the pinned toolchain

The version is in two places: CI's `compile-fail` job and this file. Change
both. Then write all snapshots again and read the diff.
