- `veredictum test` and `test --rejudge` take their exit status from the
  attribution each red row's record already carries. A run exits `1` when a
  row counts against the server (it failed, a warning row excepted, or
  errored with the server named), and `3`, which is new, when no row does but
  at least one is inconclusive (the instrument could not judge it); `0` and
  `2` are unchanged. The line before the exit names both counts. This is a
  breaking change for a script that tests for exactly `1`: a run whose only
  red rows the instrument could not judge used to exit `1` and now exits `3`.
  The GitHub Action takes a `fail-on-inconclusive` input (default `true`);
  with `false`, exit status `3` passes the step and the job summary lists the
  inconclusive rows, while `1` and `2` always fail it.
