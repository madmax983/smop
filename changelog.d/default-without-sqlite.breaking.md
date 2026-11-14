- `default` is no longer `full`. `full` is every feature. `default` is every
  feature but `sqlite`, so the default build does not compile SQLite. The
  modules of the default build do not change.

  Migration: no change is needed for the default features. To get
  `sqlite`, use `features = ["sqlite"]` or `features = ["full"]`.
