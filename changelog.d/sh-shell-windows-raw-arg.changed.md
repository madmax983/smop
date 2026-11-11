- **Windows: `sh::run` and `sh::output` give the line to `cmd /C` with no
  C runtime quoting** (`raw_arg`). Before, the quoting changed lines that
  have quotes, so `run(r#"echo "a  b""#)` printed `\"a  b\"`. If a line
  has `\"` to get past the old quoting, remove the backslashes.
- **Unix: `sh::run` and `sh::output` use `/bin/sh`**, not `sh` on `PATH`.
