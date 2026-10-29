- `veredictum perf` creates the measured record when `--results` names no
  file, so a measured run no longer needs a `results.json` that no remaining
  command writes. Before it seeds anything, it reads the SUT identity through
  System OPTIONS, and the new `--sut-name` and `--sut-version` flags supply
  what the server does not state. The new record holds the measurements and no
  conformance outcome: it is `statement_blind` with the defaulted technology
  profile, and its verification pack is `not_run`. An existing record keeps
  its header and gains or replaces the measured case's record, as before.
  `verdicts` and `perf-assets` read either kind.
