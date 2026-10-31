- `veredictum validate --statement` reads the repository root from `--specs`
  when it is given, as `veredictum test --specs` does, so the review of a
  statement against a catalogue that does not sit beside `specs/` resolves
  its skip citations against the named specification tree.
