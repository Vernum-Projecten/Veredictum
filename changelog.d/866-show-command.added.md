- `veredictum show <results.json>` prints a result in the terminal: one line
  per chapter, band and option with a bar and the count of each mark, in two
  parts, BASE and OPTIONS. Each option names its selected arm and its kind,
  and a closing note counts the cases tagged `@SpecDiverge`. `--failed` expands every failed and errored row with
  its four-part explanation and lists the advisory rows apart. Colour follows
  the terminal and `NO_COLOR`, and `--color` overrides it; every mark is a
  glyph as well, so the view reads without colour. `veredictum test` prints
  the same view when a run finishes.
