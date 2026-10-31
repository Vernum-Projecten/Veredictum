- `veredictum test` takes `--specs`, the vendored openEHR spec tree, as
  `validate` does, so a catalogue that does not sit beside `specs/` runs and
  re-judges against an explicit specification root. Without the flag the tree
  is `specs/openehr` beside `--root`, as before.
