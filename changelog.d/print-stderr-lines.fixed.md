- `error!` uses the colour choice of stderr, not of stdout. With
  `2>file`, the file gets no colour codes.
- `success!`, `warn!` and `error!` hide the live spinners and progress
  bars while they print. Before, a redraw could overwrite the line.
