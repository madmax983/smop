# `cmd!` compile-fail cases

Each `.rs` file here must not compile. Its `.stderr` file is the compiler
output that `trybuild` expects. `tests/cmd_macro_errors.rs` runs the cases.

## Run the cases

The text of rustc errors changes between releases. So the cases run on one
pinned toolchain, on Linux. CI's `compile-fail` job has the version.

```bash
cargo +1.97.0 test -p smop --test cmd_macro_errors -- --ignored
```

Without `--ignored`, the test binary only checks the snapshots as text. That
check runs on each platform and each toolchain.

## Add or change a case

1. Add or edit `tests/ui/<case>.rs`.
2. Add the case and the message of its rule to `CASES` in
   `tests/cmd_macro_errors.rs`.
3. Write the snapshot again:

   ```bash
   TRYBUILD=overwrite cargo +1.97.0 test -p smop --test cmd_macro_errors -- --ignored
   ```

4. Read the new `.stderr` file before you commit it.

## Move the pinned toolchain

Change the version in CI's `compile-fail` job and in this file. Then write
all snapshots again and read the diff.
