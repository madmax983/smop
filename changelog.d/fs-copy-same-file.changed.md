- `fs::copy(a, a)`, or a copy onto a hard link of `a`, is an error. Before,
  it returned `Ok` and emptied the file (#37).
