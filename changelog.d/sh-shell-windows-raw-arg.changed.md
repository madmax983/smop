- **Windows: `sh::run` and `sh::output` give the line to `cmd /C` as it
  is** (`raw_arg`). Before, the C runtime quoting of `.arg` changed lines
  that have quotes, so `run(r#"echo "a  b""#)` printed `\"a  b\"`.
