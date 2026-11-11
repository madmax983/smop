# ADR 0014: `git` Module

## Status

Accepted (0.4.0).

## Context

Release scripts and xtasks ask git the same questions many times. Harvest's
xtask runs git 16 times in 5 files. Each call parses the output by hand:
`ls-files -z`, `diff --quiet -- PATH`, `log --format=%H -- PATH`,
`show REV:PATH` and `check-ignore -q`.

## Decision

- The `git` feature (`git = ["sh"]`) adds `smop::git`. It is part of
  `full`. It adds no crate. It runs the `git` program through `sh`.
- The functions: `head`, `branch`, `is_clean`, `is_clean_paths`,
  `changed_since`, `ls_files`, `show` and `is_ignored`. They query the
  repository of the current directory.
- `git::Repo` has the same functions as methods. `Repo::new(dir)` is the
  variant that takes a repository directory. One type is smaller than eight
  `*_in(dir, ..)` functions.
- `Repo::new(dir)` runs `git -C dir`. Thus the error shows the directory,
  and a missing directory gives the git error (code 128).
- `Repo::new(dir)` removes the variables that `git rev-parse
  --local-env-vars` lists (`GIT_DIR`, `GIT_INDEX_FILE`, ...). A git hook
  sets them. Without this, a script that a hook starts queries the wrong
  repository. `Repo::default()` and the free functions keep them, as git
  does.
- Errors: a failed git command gives a `sh::CommandError` with the stderr
  of git. smop adds no context, so the first line of the error has the git
  command line and the stderr follows it.
- Paths in a result are relative to the repository root, sorted by path
  components, with no duplicates. On Unix, smop keeps bytes that are not
  UTF-8 (`-z` output).
- smop reads the paths that you give literally (`--literal-pathspecs`).
  `check-ignore` has no literal mode, so smop puts `./` before a leading
  `:`.
- Results do not change with user config. smop sets
  `--untracked-files=normal`, `diff.relative=false`, `--no-renames`,
  `--no-ext-diff` and `--no-color`.
- `is_clean` runs `git status --porcelain` with `GIT_OPTIONAL_LOCKS=0`. It
  does not write the index, so it does not stop on the `index.lock` of an
  editor.
- `changed_since(rev)` compares `rev` with the index and with the work tree,
  and adds the untracked files that are not ignored. Thus
  `changed_since("HEAD")` is empty if, and only if, `is_clean()` is `true`.
  A proptest checks this. smop resolves `rev` with `rev-parse --verify
  rev^{commit}` first. Outside a repository, `git diff` is `git diff
  --no-index`, which gives a usage error.
- `show` runs `git cat-file blob rev:path`. It gives the committed bytes
  with no textconv. A directory is an error (`git show` shows a tree
  listing). The text is not trimmed. Content that is not UTF-8 is an error.
- A `rev` that starts with `-` is an error. Git would read it as an option,
  for example `--output=FILE`.
- smop does not use `--end-of-options` (git 2.24) or `--no-relative` (git
  2.28). Older git gives a usage error for them.

## Consequences

- Scripts get typed results and errors with exit codes. They do not parse
  git output.
- The functions need the `git` program on `PATH`. Without it, the error has
  code 127.
- Each call starts one or more processes (`changed_since` starts four). A
  script that queries in a loop can be slow.
- `show` gives text only. For binary content, use
  `sh::cmd("git").args(["cat-file", "blob", "rev:path"]).output_raw()`.
